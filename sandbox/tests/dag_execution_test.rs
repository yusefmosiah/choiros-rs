//! DAG Execution Tests — test the computationally universal execution substrate.
//!
//! These tests exercise the DAG executor directly with a mock RlmPort, verifying:
//! - Multi-step execution with variable references
//! - Conditional gates (steps skipped when gate is false)
//! - Transform operations (regex, truncate, json_extract)
//! - LLM calls within DAGs (mocked)
//! - Emit messages to parent actor
//! - Error handling (missing deps, cycles, tool failures)
//! - Diamond dependency patterns
//!
//! No real LLM calls — the mock port provides deterministic responses.
//!
//! Run:
//!   cargo test -p sandbox --test dag_execution_test -- --nocapture

use sandbox::actors::agent_harness::rlm::{
    execute_dag, DagStepTrace, DagTrace, LlmCallResult, RlmConfig, RlmHarness, RlmPort,
    RlmRunResult, RlmToolExecution,
};
use sandbox::baml_client::types::{ContextSourceKind, DagStep, StepOp};
use shared_types;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ─── Mock Port ───────────────────────────────────────────────────────────────

struct MockRlmPort {
    /// Captured emit messages for assertions
    emitted: Arc<Mutex<Vec<String>>>,
    /// Captured LLM prompts for assertions
    llm_prompts: Arc<Mutex<Vec<String>>>,
    /// Fixed LLM response to return
    llm_response: String,
}

impl MockRlmPort {
    fn new(llm_response: &str) -> Self {
        Self {
            emitted: Arc::new(Mutex::new(Vec::new())),
            llm_prompts: Arc::new(Mutex::new(Vec::new())),
            llm_response: llm_response.to_string(),
        }
    }

    fn emitted(&self) -> Vec<String> {
        self.emitted.lock().unwrap().clone()
    }

    fn llm_prompts(&self) -> Vec<String> {
        self.llm_prompts.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl RlmPort for MockRlmPort {
    fn capabilities_description(&self) -> String {
        "mock".to_string()
    }

    fn model_id(&self) -> &str {
        "mock"
    }

    async fn resolve_source(
        &self,
        _kind: &ContextSourceKind,
        _source_ref: &str,
        _max_tokens: Option<i64>,
    ) -> Option<String> {
        None
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
    ) -> RlmToolExecution {
        let start = Instant::now();
        match tool_name {
            "bash" => {
                let cmd = tool_args.get("command").cloned().unwrap_or_default();
                // Simple mock: echo commands return the argument
                if cmd.starts_with("echo ") {
                    RlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: cmd.strip_prefix("echo ").unwrap_or("").to_string(),
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    }
                } else if cmd == "fail" {
                    RlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("command failed".into()),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    }
                } else {
                    RlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: format!("executed: {cmd}"),
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    }
                }
            }
            "file_read" => {
                let path = tool_args.get("path").cloned().unwrap_or_default();
                RlmToolExecution {
                    turn: 0,
                    tool_name: "file_read".into(),
                    tool_args: tool_args.clone(),
                    success: true,
                    output: format!("contents of {path}"),
                    error: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                }
            }
            _ => RlmToolExecution {
                turn: 0,
                tool_name: tool_name.into(),
                tool_args: tool_args.clone(),
                success: false,
                output: String::new(),
                error: Some(format!("unknown tool: {tool_name}")),
                elapsed_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    async fn call_llm(
        &self,
        prompt: &str,
        _system_prompt: Option<&str>,
        _model_hint: Option<&str>,
    ) -> LlmCallResult {
        self.llm_prompts.lock().unwrap().push(prompt.to_string());
        LlmCallResult {
            output: self.llm_response.clone(),
            success: true,
            error: None,
            elapsed_ms: 1,
        }
    }

    async fn emit_message(&self, message: &str) {
        self.emitted.lock().unwrap().push(message.to_string());
    }

    fn run_id(&self) -> &str { "test-run" }
    fn actor_id(&self) -> &str { "test-actor" }

    async fn dispatch_tool(&self, tool_name: &str, _args: &HashMap<String, String>, corr_id: &str) {
        println!("[DISPATCH] corr:{corr_id} tool:{tool_name}");
    }

    async fn write_checkpoint(&self, checkpoint: &shared_types::HarnessCheckpoint) {
        println!(
            "[CHECKPOINT] turn:{} pending:{}",
            checkpoint.turn_number, checkpoint.pending_replies.len()
        );
    }

    async fn spawn_subharness(
        &self,
        objective: &str,
        _context: serde_json::Value,
        corr_id: &str,
    ) {
        println!("[SPAWN_SUBHARNESS] corr:{corr_id} objective:{objective}");
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_step(id: &str, op: StepOp, depends_on: Vec<&str>) -> DagStep {
    DagStep {
        id: id.to_string(),
        op,
        depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
        condition: None,
        tool_name: None,
        tool_args: None,
        prompt: None,
        model_hint: None,
        system_prompt: None,
        transform_op: None,
        transform_input: None,
        transform_pattern: None,
        gate_predicate: None,
        emit_message: None,
        eval_code: None,
        eval_inputs: None,
        description: None,
    }
}

fn print_trace(trace: &DagTrace) {
    println!("  DAG trace (turn {}, {}ms total):", trace.turn, trace.total_elapsed_ms);
    for st in &trace.steps {
        let status = if st.skipped {
            "SKIP"
        } else if st.success {
            "OK"
        } else {
            "FAIL"
        };
        let out_preview = if st.output.len() > 80 {
            format!("{}...", &st.output[..80])
        } else {
            st.output.clone()
        };
        println!(
            "    [{status}] {}: {} — {}",
            st.step_id, st.op, out_preview
        );
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_dag_linear_tool_chain() {
    // a -> b -> c: each step reads the output of the previous via ${ref}
    let port = MockRlmPort::new("");
    let steps = vec![
        {
            let mut s = make_step("get_os", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([("command".into(), "echo Linux".into())]));
            s.description = Some("Get OS name".into());
            s
        },
        {
            let mut s = make_step("write_file", StepOp::ToolCall, vec!["get_os"]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo wrote ${get_os}".into(),
            )]));
            s.description = Some("Write OS to file".into());
            s
        },
        {
            let mut s = make_step("verify", StepOp::ToolCall, vec!["write_file"]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo verified: ${write_file}".into(),
            )]));
            s.description = Some("Verify written".into());
            s
        },
    ];

    let (trace, outputs, tool_execs) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    assert_eq!(trace.steps.len(), 3);
    assert!(trace.steps.iter().all(|s| s.success));
    assert_eq!(outputs["get_os"], "Linux");
    assert_eq!(outputs["write_file"], "wrote Linux");
    assert_eq!(outputs["verify"], "verified: wrote Linux");
    assert_eq!(tool_execs.len(), 3);
    println!("  PASS: linear tool chain with variable substitution");
}

#[tokio::test]
async fn test_dag_conditional_gate_true() {
    // read -> analyze (LLM) -> gate (contains CRITICAL) -> deep_review (only if gate true)
    let port = MockRlmPort::new("Found CRITICAL vulnerability in auth module");
    let steps = vec![
        {
            let mut s = make_step("read", StepOp::ToolCall, vec![]);
            s.tool_name = Some("file_read".to_string());
            s.tool_args = Some(HashMap::from([("path".into(), "src/auth.rs".into())]));
            s
        },
        {
            let mut s = make_step("analyze", StepOp::LlmCall, vec!["read"]);
            s.prompt = Some("Analyze for security issues:\n${read}".to_string());
            s
        },
        {
            let mut s = make_step("is_critical", StepOp::Gate, vec!["analyze"]);
            s.gate_predicate = Some("contains:CRITICAL".to_string());
            s
        },
        {
            let mut s = make_step("deep_review", StepOp::LlmCall, vec!["read", "analyze"]);
            s.condition = Some("is_critical".to_string());
            s.prompt = Some("Deep review:\n${read}\nAnalysis:\n${analyze}".to_string());
            s.model_hint = Some("strong".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    // Gate should be true, deep_review should execute
    assert_eq!(outputs["is_critical"], "true");
    assert!(!trace.steps[3].skipped, "deep_review should NOT be skipped");
    assert!(trace.steps[3].success);

    // Verify LLM was called with substituted prompt
    let prompts = port.llm_prompts();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].contains("contents of src/auth.rs"));
    assert!(prompts[1].contains("contents of src/auth.rs"));
    assert!(prompts[1].contains("CRITICAL"));
    println!("  PASS: conditional gate (true path)");
}

#[tokio::test]
async fn test_dag_conditional_gate_false() {
    // Same structure but LLM response doesn't contain CRITICAL
    let port = MockRlmPort::new("Code looks clean, no issues found");
    let steps = vec![
        {
            let mut s = make_step("read", StepOp::ToolCall, vec![]);
            s.tool_name = Some("file_read".to_string());
            s.tool_args = Some(HashMap::from([("path".into(), "src/auth.rs".into())]));
            s
        },
        {
            let mut s = make_step("analyze", StepOp::LlmCall, vec!["read"]);
            s.prompt = Some("Analyze:\n${read}".to_string());
            s
        },
        {
            let mut s = make_step("is_critical", StepOp::Gate, vec!["analyze"]);
            s.gate_predicate = Some("contains:CRITICAL".to_string());
            s
        },
        {
            let mut s = make_step("deep_review", StepOp::LlmCall, vec!["analyze"]);
            s.condition = Some("is_critical".to_string());
            s.prompt = Some("Deep review: ${analyze}".to_string());
            s
        },
        {
            let mut s = make_step("report", StepOp::Emit, vec!["analyze", "deep_review"]);
            s.emit_message = Some("Result: ${analyze} / Deep: ${deep_review}".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    // Gate should be false, deep_review should be skipped
    assert_eq!(outputs["is_critical"], "false");
    assert!(trace.steps[3].skipped, "deep_review SHOULD be skipped");
    assert_eq!(outputs["deep_review"], "(skipped)");

    // Emit should still run with (skipped) substituted for deep_review
    let emitted = port.emitted();
    assert_eq!(emitted.len(), 1);
    assert!(emitted[0].contains("(skipped)"));

    // Only 1 LLM call (analyze), not 2 (deep_review skipped)
    assert_eq!(port.llm_prompts().len(), 1);
    println!("  PASS: conditional gate (false path, skip works)");
}

#[tokio::test]
async fn test_dag_transform_regex() {
    let port = MockRlmPort::new("");
    let steps = vec![
        {
            let mut s = make_step("data", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo HTTP/1.1 404 Not Found".into(),
            )]));
            s
        },
        {
            let mut s = make_step("extract_code", StepOp::Transform, vec!["data"]);
            s.transform_op = Some("regex".to_string());
            s.transform_input = Some("${data}".to_string());
            s.transform_pattern = Some(r"(\d{3})".to_string());
            s
        },
        {
            let mut s = make_step("is_error", StepOp::Gate, vec!["extract_code"]);
            s.gate_predicate = Some("equals:404".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    assert_eq!(outputs["extract_code"], "404");
    assert_eq!(outputs["is_error"], "true");
    println!("  PASS: transform regex + gate equals");
}

#[tokio::test]
async fn test_dag_transform_json_extract() {
    let port = MockRlmPort::new("");
    let steps = vec![
        {
            let mut s = make_step("api_call", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                r#"echo {"status":"ok","count":42}"#.into(),
            )]));
            s
        },
        {
            let mut s = make_step("get_status", StepOp::Transform, vec!["api_call"]);
            s.transform_op = Some("json_extract".to_string());
            s.transform_input = Some("${api_call}".to_string());
            s.transform_pattern = Some("status".to_string());
            s
        },
        {
            let mut s = make_step("is_ok", StepOp::Gate, vec!["get_status"]);
            s.gate_predicate = Some("equals:ok".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    assert_eq!(outputs["get_status"], "ok");
    assert_eq!(outputs["is_ok"], "true");
    println!("  PASS: JSON extract + gate");
}

#[tokio::test]
async fn test_dag_diamond_dependency() {
    // Diamond: a -> b, a -> c, b+c -> d
    let port = MockRlmPort::new("synthesized");
    let steps = vec![
        {
            let mut s = make_step("a", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([("command".into(), "echo source_data".into())]));
            s
        },
        {
            let mut s = make_step("b", StepOp::LlmCall, vec!["a"]);
            s.prompt = Some("Path B: ${a}".to_string());
            s
        },
        {
            let mut s = make_step("c", StepOp::LlmCall, vec!["a"]);
            s.prompt = Some("Path C: ${a}".to_string());
            s
        },
        {
            let mut s = make_step("d", StepOp::Emit, vec!["b", "c"]);
            s.emit_message = Some("B=${b} C=${c}".to_string());
            s
        },
    ];

    let (trace, _outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    // All 4 steps should succeed
    assert_eq!(trace.steps.len(), 4);
    assert!(trace.steps.iter().all(|s| s.success && !s.skipped));

    // Emit should contain both B and C results
    let emitted = port.emitted();
    assert_eq!(emitted.len(), 1);
    assert!(emitted[0].contains("B=synthesized"));
    assert!(emitted[0].contains("C=synthesized"));
    println!("  PASS: diamond dependency pattern");
}

#[tokio::test]
async fn test_dag_tool_failure_propagates() {
    // Step a fails, step b gets the error text via ${a}
    let port = MockRlmPort::new("");
    let steps = vec![
        {
            let mut s = make_step("a", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([("command".into(), "fail".into())]));
            s
        },
        {
            let mut s = make_step("b", StepOp::ToolCall, vec!["a"]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo got: ${a}".into(),
            )]));
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    // a fails but doesn't crash the DAG — b sees the error text
    assert!(outputs["a"].contains("ERROR"));
    assert!(outputs["b"].contains("ERROR"));
    println!("  PASS: tool failure propagates as text (non-fatal)");
}

#[tokio::test]
async fn test_dag_empty() {
    let port = MockRlmPort::new("");
    let (trace, outputs, tool_execs) = execute_dag(&port, &[], 1, 30).await.unwrap();
    assert_eq!(trace.steps.len(), 0);
    assert!(outputs.is_empty());
    assert!(tool_execs.is_empty());
    println!("  PASS: empty DAG");
}

#[tokio::test]
async fn test_dag_exceeds_max_steps() {
    let port = MockRlmPort::new("");
    let steps: Vec<DagStep> = (0..5)
        .map(|i| {
            let mut s = make_step(&format!("s{i}"), StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([("command".into(), "echo hi".into())]));
            s
        })
        .collect();

    let result = execute_dag(&port, &steps, 1, 3).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("exceeding max"));
    println!("  PASS: DAG step limit enforced");
}

#[tokio::test]
async fn test_dag_llm_call_with_substitution() {
    // Verify that LLM prompts receive properly substituted content
    let port = MockRlmPort::new("LLM analysis complete");
    let steps = vec![
        {
            let mut s = make_step("read", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo function auth() { return true; }".into(),
            )]));
            s
        },
        {
            let mut s = make_step("analyze", StepOp::LlmCall, vec!["read"]);
            s.prompt = Some("Review this code:\n${read}".to_string());
            s.system_prompt = Some("You are a security auditor.".to_string());
            s.model_hint = Some("strong".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    assert_eq!(outputs["analyze"], "LLM analysis complete");

    let prompts = port.llm_prompts();
    assert_eq!(prompts.len(), 1);
    assert!(
        prompts[0].contains("function auth()"),
        "LLM prompt should contain substituted code"
    );
    println!("  PASS: LLM call receives substituted prompt");
}

#[tokio::test]
async fn test_dag_multi_gate_cascade() {
    // Two gates in sequence: gate1 -> conditional step -> gate2 -> conditional step
    let port = MockRlmPort::new("result with WARNING flag");
    let steps = vec![
        {
            let mut s = make_step("data", StepOp::ToolCall, vec![]);
            s.tool_name = Some("bash".to_string());
            s.tool_args = Some(HashMap::from([(
                "command".into(),
                "echo status: WARNING".into(),
            )]));
            s
        },
        {
            let mut s = make_step("has_warning", StepOp::Gate, vec!["data"]);
            s.gate_predicate = Some("contains:WARNING".to_string());
            s
        },
        {
            let mut s = make_step("investigate", StepOp::LlmCall, vec!["data"]);
            s.condition = Some("has_warning".to_string());
            s.prompt = Some("Investigate: ${data}".to_string());
            s
        },
        {
            let mut s = make_step("has_critical", StepOp::Gate, vec!["investigate"]);
            s.condition = Some("has_warning".to_string());
            s.gate_predicate = Some("contains:CRITICAL".to_string());
            s
        },
        {
            let mut s = make_step("escalate", StepOp::Emit, vec!["investigate"]);
            s.condition = Some("has_critical".to_string());
            s.emit_message = Some("ESCALATION: ${investigate}".to_string());
            s
        },
    ];

    let (trace, outputs, _) = execute_dag(&port, &steps, 1, 30).await.unwrap();
    print_trace(&trace);

    // has_warning = true, so investigate runs
    assert_eq!(outputs["has_warning"], "true");
    assert!(!trace.steps[2].skipped);

    // has_critical = false (LLM response has WARNING not CRITICAL)
    assert_eq!(outputs["has_critical"], "false");

    // escalate should be skipped
    assert!(trace.steps[4].skipped);
    assert!(port.emitted().is_empty());
    println!("  PASS: multi-gate cascade with partial skip");
}
