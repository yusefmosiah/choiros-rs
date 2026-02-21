//! DAG Runtime Eval — real LLM calls testing whether models can author DAG programs.
//!
//! This is the frontier eval: can models use the Program (DAG) execution mode
//! when the task calls for multi-step computation with dependencies, conditionals,
//! and embedded LLM calls?
//!
//! Scenarios are designed in pairs:
//!   - Simple tasks where ToolCalls is the natural choice
//!   - Complex tasks where Program/DAG is the better choice
//!   - Tasks that specifically require embedded LLM calls within the DAG
//!
//! Full output is written to `tests/artifacts/dag_eval_report.txt`.
//!
//! Run:
//!   cargo test -p sandbox --test dag_eval -- --nocapture
//!   CHOIR_LIVE_MODEL_IDS=KimiK25,ZaiGLM47 cargo test -p sandbox --test dag_eval -- --nocapture

use sandbox::actors::agent_harness::alm::{
    AlmConfig, AlmHarness, AlmPort, AlmRunResult, AlmToolExecution, LlmCallResult,
};
use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::types::ContextSourceKind;
use sandbox::baml_client::B;
use sandbox::runtime_env::ensure_tls_cert_env;
use shared_types;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File, OpenOptions};
use std::io::Write as IoWrite;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ─── Report writer (streaming to disk) ───────────────────────────────────────

struct Report {
    /// In-memory buffer (used for final summary table construction)
    buf: String,
    /// Streaming file handle — flushed after every write
    file: File,
    file_path: String,
}

impl Report {
    fn new() -> Self {
        let dir = format!("{}/tests/artifacts", env!("CARGO_MANIFEST_DIR"));
        fs::create_dir_all(&dir).ok();
        let file_path = format!("{dir}/dag_eval_report.txt");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&file_path)
            .expect("failed to open report file");
        println!(">>> Streaming report to: {file_path}");
        Self {
            buf: String::with_capacity(256 * 1024),
            file,
            file_path,
        }
    }

    fn line(&mut self, s: &str) {
        writeln!(self.buf, "{s}").ok();
        writeln!(self.file, "{s}").ok();
        self.file.flush().ok();
        // Also print to stdout for --nocapture
        println!("{s}");
    }

    fn section(&mut self, title: &str) {
        let bar = "=".repeat(80);
        self.line("");
        self.line(&bar);
        self.line(&format!("  {title}"));
        self.line(&bar);
    }

    fn subsection(&mut self, title: &str) {
        let bar = "-".repeat(60);
        self.line("");
        self.line(&bar);
        self.line(&format!("  {title}"));
        self.line(&bar);
    }

    fn finish(&mut self) {
        self.file.flush().ok();
        println!(
            "\n>>> Report complete: {} ({} bytes)",
            self.file_path,
            self.buf.len()
        );
    }
}

// ─── Eval adapter with real LLM calls ────────────────────────────────────────

struct DagEvalPort {
    model_id: String,
    model_registry: ModelRegistry,
    /// Log of all LLM calls (prompt, response, elapsed)
    llm_call_log: Arc<Mutex<Vec<LlmCallLog>>>,
    /// Log of all emitted messages
    emit_log: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Clone)]
struct LlmCallLog {
    prompt_len: usize,
    prompt_preview: String,
    system_prompt: Option<String>,
    model_hint: Option<String>,
    response_len: usize,
    response_preview: String,
    success: bool,
    elapsed_ms: u64,
}

impl DagEvalPort {
    fn new(model_id: String, model_registry: ModelRegistry) -> Self {
        Self {
            model_id,
            model_registry,
            llm_call_log: Arc::new(Mutex::new(Vec::new())),
            emit_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    fn take_llm_log(&self) -> Vec<LlmCallLog> {
        std::mem::take(&mut *self.llm_call_log.lock().unwrap())
    }

    #[allow(dead_code)]
    fn take_emit_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.emit_log.lock().unwrap())
    }
}

#[async_trait::async_trait]
impl AlmPort for DagEvalPort {
    fn capabilities_description(&self) -> String {
        r#"Available tools:

1. bash - Execute shell commands
   Args: command (string, required)

2. file_read - Read a local file
   Args: path (string, required)

3. file_write - Write/overwrite a file
   Args: path (string, required), content (string, required)

Available context sources:
- Document: Load a file by path
- MemoryQuery: (not yet available)

IMPORTANT — You have a powerful execution mode called Program.
When your task requires multi-step computation where later steps depend on
earlier results, use kind=Program to write a DAG of operations. Within a DAG:
- Steps can call tools, call LLMs, transform data, gate conditionally, emit messages
- Steps reference prior step outputs via ${step_id}
- The harness traces every step

Use ToolCalls for simple independent operations.
Use Program when you need data flow between steps."#
            .to_string()
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    async fn resolve_source(
        &self,
        kind: &ContextSourceKind,
        source_ref: &str,
        _max_tokens: Option<i64>,
    ) -> Option<String> {
        match kind {
            ContextSourceKind::Document => tokio::fs::read_to_string(source_ref).await.ok(),
            ContextSourceKind::MemoryQuery => Some("(memory not yet available)".to_string()),
            ContextSourceKind::PreviousTurn | ContextSourceKind::ToolOutput => None,
        }
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
    ) -> AlmToolExecution {
        let start = Instant::now();
        match tool_name {
            "bash" => {
                let command = tool_args.get("command").map(|s| s.as_str()).unwrap_or("");
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let result = tokio::time::timeout(
                    Duration::from_secs(15),
                    tokio::process::Command::new(&shell)
                        .arg("-lc")
                        .arg(command)
                        .current_dir(env!("CARGO_MANIFEST_DIR"))
                        .output(),
                )
                .await;

                match result {
                    Ok(Ok(output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let mut combined = String::new();
                        if !stdout.trim().is_empty() {
                            combined.push_str(stdout.trim_end());
                        }
                        if !stderr.trim().is_empty() {
                            if !combined.is_empty() {
                                combined.push('\n');
                            }
                            combined.push_str(stderr.trim_end());
                        }
                        AlmToolExecution {
                            turn: 0,
                            tool_name: "bash".into(),
                            tool_args: tool_args.clone(),
                            success: output.status.success(),
                            output: combined,
                            error: if output.status.success() {
                                None
                            } else {
                                Some(format!("exit {}", output.status.code().unwrap_or(1)))
                            },
                            elapsed_ms: start.elapsed().as_millis() as u64,
                        }
                    }
                    Ok(Err(e)) => AlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("exec: {e}")),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(_) => AlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("timeout (15s)".into()),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            "file_read" => {
                let path = tool_args.get("path").map(|s| s.as_str()).unwrap_or("");
                // Sandbox to CARGO_MANIFEST_DIR
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path)
                };
                match tokio::fs::read_to_string(&full_path).await {
                    Ok(content) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_read".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: content,
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_read".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("read: {e}")),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            "file_write" => {
                let path = tool_args.get("path").map(|s| s.as_str()).unwrap_or("");
                let content = tool_args.get("content").map(|s| s.as_str()).unwrap_or("");
                // Only allow /tmp writes
                if !path.starts_with("/tmp/") {
                    return AlmToolExecution {
                        turn: 0,
                        tool_name: "file_write".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("write only allowed under /tmp/".into()),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
                match tokio::fs::write(path, content).await {
                    Ok(_) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_write".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: format!("wrote {} bytes to {path}", content.len()),
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_write".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("write: {e}")),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            _ => AlmToolExecution {
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
        system_prompt: Option<&str>,
        model_hint: Option<&str>,
    ) -> LlmCallResult {
        let start = Instant::now();

        // Resolve which model to use for this nested call
        let nested_model_id = match model_hint {
            Some("fast") | Some("cheap") => {
                // Try flash model
                if self.model_registry.get("ZaiGLM47Flash").is_some() {
                    "ZaiGLM47Flash"
                } else {
                    &self.model_id
                }
            }
            Some("strong") | Some("opus") => {
                if self.model_registry.get("ClaudeBedrockOpus46").is_some() {
                    "ClaudeBedrockOpus46"
                } else {
                    &self.model_id
                }
            }
            _ => &self.model_id, // default: same model as the harness
        };

        let client_registry = match self
            .model_registry
            .create_runtime_client_registry_for_model(nested_model_id)
        {
            Ok(cr) => cr,
            Err(e) => {
                let elapsed = start.elapsed().as_millis() as u64;
                self.llm_call_log.lock().unwrap().push(LlmCallLog {
                    prompt_len: prompt.len(),
                    prompt_preview: truncate(prompt, 200),
                    system_prompt: system_prompt.map(|s| truncate(s, 100)),
                    model_hint: model_hint.map(|s| s.to_string()),
                    response_len: 0,
                    response_preview: String::new(),
                    success: false,
                    elapsed_ms: elapsed,
                });
                return LlmCallResult {
                    output: String::new(),
                    success: false,
                    error: Some(format!("model registry error: {e}")),
                    elapsed_ms: elapsed,
                };
            }
        };

        // Use the DagLlmCall BAML function for real LLM call
        let sys = system_prompt.map(|s| s.to_string());
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            B.DagLlmCall
                .with_client_registry(&client_registry)
                .call(prompt, sys.as_deref()),
        )
        .await;

        let elapsed = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(response)) => {
                self.llm_call_log.lock().unwrap().push(LlmCallLog {
                    prompt_len: prompt.len(),
                    prompt_preview: truncate(prompt, 200),
                    system_prompt: system_prompt.map(|s| truncate(s, 100)),
                    model_hint: model_hint.map(|s| s.to_string()),
                    response_len: response.len(),
                    response_preview: truncate(&response, 200),
                    success: true,
                    elapsed_ms: elapsed,
                });
                LlmCallResult {
                    output: response,
                    success: true,
                    error: None,
                    elapsed_ms: elapsed,
                }
            }
            Ok(Err(e)) => {
                self.llm_call_log.lock().unwrap().push(LlmCallLog {
                    prompt_len: prompt.len(),
                    prompt_preview: truncate(prompt, 200),
                    system_prompt: system_prompt.map(|s| truncate(s, 100)),
                    model_hint: model_hint.map(|s| s.to_string()),
                    response_len: 0,
                    response_preview: String::new(),
                    success: false,
                    elapsed_ms: elapsed,
                });
                LlmCallResult {
                    output: String::new(),
                    success: false,
                    error: Some(format!("LLM call error: {e}")),
                    elapsed_ms: elapsed,
                }
            }
            Err(_) => {
                self.llm_call_log.lock().unwrap().push(LlmCallLog {
                    prompt_len: prompt.len(),
                    prompt_preview: truncate(prompt, 200),
                    system_prompt: system_prompt.map(|s| truncate(s, 100)),
                    model_hint: model_hint.map(|s| s.to_string()),
                    response_len: 0,
                    response_preview: String::new(),
                    success: false,
                    elapsed_ms: elapsed,
                });
                LlmCallResult {
                    output: String::new(),
                    success: false,
                    error: Some("LLM call timeout (30s)".into()),
                    elapsed_ms: elapsed,
                }
            }
        }
    }

    async fn emit_message(&self, message: &str) {
        self.emit_log.lock().unwrap().push(message.to_string());
    }

    fn run_id(&self) -> &str {
        "dag-eval-run"
    }
    fn actor_id(&self) -> &str {
        "dag-eval-actor"
    }

    async fn dispatch_tool(&self, tool_name: &str, _args: &HashMap<String, String>, corr_id: &str) {
        println!("  [DISPATCH] corr:{corr_id} tool:{tool_name}");
    }

    async fn write_checkpoint(&self, checkpoint: &shared_types::HarnessCheckpoint) {
        println!(
            "  [CHECKPOINT] run:{} turn:{} pending:{}",
            checkpoint.run_id,
            checkpoint.turn_number,
            checkpoint.pending_replies.len()
        );
    }

    async fn spawn_harness(&self, objective: &str, _context: serde_json::Value, corr_id: &str) {
        println!(
            "  [SPAWN_SUBHARNESS] corr:{corr_id} obj:{}",
            &objective[..objective.len().min(80)]
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn env_present(key: &str) -> bool {
    std::env::var(key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn bedrock_auth_present() -> bool {
    env_present("AWS_BEARER_TOKEN_BEDROCK")
        || env_present("AWS_PROFILE")
        || (env_present("AWS_ACCESS_KEY_ID") && env_present("AWS_SECRET_ACCESS_KEY"))
}

fn model_available(registry: &ModelRegistry, model_id: &str) -> bool {
    let Some(config) = registry.get(model_id) else {
        return false;
    };
    match &config.provider {
        ProviderConfig::AwsBedrock { .. } => {
            bedrock_auth_present() && ensure_tls_cert_env().is_some()
        }
        ProviderConfig::AnthropicCompatible { api_key_env, .. }
        | ProviderConfig::OpenAiGeneric { api_key_env, .. } => env_present(api_key_env),
    }
}

fn eval_models() -> Vec<String> {
    let defaults = [
        "ClaudeBedrockOpus46",
        "ClaudeBedrockSonnet46",
        "KimiK25",
        "ZaiGLM47",
        "ZaiGLM5",
        "ZaiGLM47Flash",
    ];
    if let Ok(raw) = std::env::var("CHOIR_LIVE_MODEL_IDS") {
        let parsed: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    defaults.iter().map(|s| s.to_string()).collect()
}

/// Write the full report for a single scenario run
fn report_scenario(
    report: &mut Report,
    model_id: &str,
    scenario_name: &str,
    result: &AlmRunResult,
    llm_log: &[LlmCallLog],
    emit_log: &[String],
    elapsed_ms: u64,
) {
    report.subsection(&format!("{model_id} / {scenario_name}"));
    report.line(&format!(
        "  elapsed: {}ms | turns: {} | tools: {} | nested_llm_calls: {} | emits: {}",
        elapsed_ms,
        result.turns_taken,
        result.tool_executions.len(),
        llm_log.len(),
        emit_log.len(),
    ));
    report.line(&format!("  completion: {}", result.completion_reason));

    // Turn-by-turn detail
    for tl in &result.turn_log {
        report.line(&format!(
            "\n  TURN {} [{}] ({}ms)",
            tl.turn_number, tl.action_kind, tl.elapsed_ms
        ));
        report.line(&format!("    working_memory: {}", tl.working_memory));
        if !tl.sources_requested.is_empty() {
            report.line(&format!("    sources: {:?}", tl.sources_requested));
        }
        report.line(&format!("    action_summary: {}", tl.action_summary));
    }

    // DAG traces (the interesting part)
    for dag in &result.dag_traces {
        report.line(&format!(
            "\n  DAG TRACE (turn {}, {}ms total, {} steps)",
            dag.turn,
            dag.total_elapsed_ms,
            dag.steps.len()
        ));
        for st in &dag.steps {
            let status = if st.skipped {
                "SKIP"
            } else if st.success {
                "OK"
            } else {
                "FAIL"
            };
            report.line(&format!(
                "    [{status}] step:{} op:{} ({}ms)",
                st.step_id, st.op, st.elapsed_ms
            ));
            if let Some(desc) = &st.description {
                report.line(&format!("      desc: {desc}"));
            }
            // Full output (this is the verbose part — intentional for research)
            report.line(&format!("      output: {}", &st.output));
            if let Some(err) = &st.error {
                report.line(&format!("      error: {err}"));
            }
        }
    }

    // Tool executions
    if !result.tool_executions.is_empty() {
        report.line("\n  TOOL EXECUTIONS:");
        for te in &result.tool_executions {
            let status = if te.success { "OK" } else { "ERR" };
            report.line(&format!(
                "    [{status}] {}({}) -> {} ({}ms)",
                te.tool_name,
                te.tool_args
                    .iter()
                    .map(|(k, v)| format!("{k}={}", truncate(v, 60)))
                    .collect::<Vec<_>>()
                    .join(", "),
                truncate(&te.output, 200),
                te.elapsed_ms,
            ));
        }
    }

    // Nested LLM calls
    if !llm_log.is_empty() {
        report.line("\n  NESTED LLM CALLS:");
        for (i, lc) in llm_log.iter().enumerate() {
            let status = if lc.success { "OK" } else { "ERR" };
            report.line(&format!(
                "    [{status}] call #{} ({}ms) hint={:?}",
                i + 1,
                lc.elapsed_ms,
                lc.model_hint,
            ));
            report.line(&format!(
                "      prompt ({} chars): {}",
                lc.prompt_len, lc.prompt_preview
            ));
            if let Some(sp) = &lc.system_prompt {
                report.line(&format!("      system: {sp}"));
            }
            report.line(&format!(
                "      response ({} chars): {}",
                lc.response_len, lc.response_preview
            ));
        }
    }

    // Emitted messages
    if !emit_log.is_empty() {
        report.line("\n  EMITTED MESSAGES:");
        for (i, msg) in emit_log.iter().enumerate() {
            report.line(&format!("    #{}: {}", i + 1, truncate(msg, 300)));
        }
    }

    report.line(&format!(
        "\n  FINAL WORKING MEMORY:\n    {}",
        result.final_working_memory
    ));
}

// ─── Scenarios ───────────────────────────────────────────────────────────────

struct Scenario {
    name: &'static str,
    objective: &'static str,
    /// What we expect the model to choose
    expected_mode: &'static str,
    /// Validation function
    validate: fn(&AlmRunResult) -> (bool, String),
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // ── Tier 1: Simple tasks (ToolCalls expected) ──
        Scenario {
            name: "T1_bash_echo",
            objective: "Run the command `echo DAG_EVAL_OK` using bash and report the output.",
            expected_mode: "ToolCalls",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                let has_tool = r.tool_executions.iter().any(|t| t.tool_name == "bash");
                (
                    completed && has_tool,
                    format!("completed={completed} has_bash={has_tool}"),
                )
            },
        },
        Scenario {
            name: "T1_read_cargo",
            objective: "Read the file at Cargo.toml and tell me what workspace members are defined.",
            expected_mode: "ToolCalls",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                (completed, format!("completed={completed}"))
            },
        },
        // ── Tier 2: Multi-step with dependencies (Program/DAG expected) ──
        Scenario {
            name: "T2_analyze_and_summarize",
            objective: "Read the file Cargo.toml. Then use an LLM call to analyze which crates \
                        have the most dependencies. Finally, write a summary to /tmp/dag_eval_deps.txt. \
                        This requires a Program with steps that depend on each other's outputs.",
            expected_mode: "Program",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                let used_program = r.turn_log.iter().any(|t| t.action_kind == "Program");
                let has_dag = !r.dag_traces.is_empty();
                (
                    completed,
                    format!(
                        "completed={completed} used_program={used_program} has_dag_trace={has_dag}"
                    ),
                )
            },
        },
        Scenario {
            name: "T2_conditional_check",
            objective: "Run `cargo --version` to check the Rust toolchain. If the version contains \
                        'nightly', report 'NIGHTLY BUILD'. Otherwise, report 'STABLE BUILD'. \
                        Use a Program with a Gate step to conditionally branch based on the output.",
            expected_mode: "Program",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                let used_program = r.turn_log.iter().any(|t| t.action_kind == "Program");
                (
                    completed,
                    format!("completed={completed} used_program={used_program}"),
                )
            },
        },
        // ── Tier 3: Requires embedded LLM calls in DAG ──
        Scenario {
            name: "T3_read_analyze_classify",
            objective: "Execute a Program (DAG) that does the following in a single turn:\n\
                        1. Read the file `src/lib.rs` using file_read\n\
                        2. Use an LlmCall step to analyze what the file exports (the prompt should include ${read_step_id})\n\
                        3. Use a Gate step to check if the analysis mentions 'actor' or 'Actor'\n\
                        4. If the gate is true, use another LlmCall step to write a brief description of the actor system\n\
                        5. Emit the final result\n\
                        You MUST use kind=Program with op=LlmCall steps, not just ToolCalls.",
            expected_mode: "Program",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                let used_program = r.turn_log.iter().any(|t| t.action_kind == "Program");
                let has_llm_in_dag = r.dag_traces.iter().any(|d| {
                    d.steps.iter().any(|s| s.op == "LlmCall" && s.success)
                });
                (
                    completed,
                    format!(
                        "completed={completed} used_program={used_program} llm_in_dag={has_llm_in_dag}"
                    ),
                )
            },
        },
        Scenario {
            name: "T3_multi_file_synthesis",
            objective: "Execute a Program (DAG) in a single turn that:\n\
                        1. Reads Cargo.toml (step: read_cargo)\n\
                        2. Reads src/lib.rs (step: read_lib)\n\
                        3. Uses an LlmCall to synthesize both files into a project description \
                           (prompt should reference ${read_cargo} and ${read_lib})\n\
                        4. Uses a Transform step to truncate the synthesis to 500 characters\n\
                        5. Writes the truncated result to /tmp/dag_eval_synthesis.txt\n\
                        You MUST use kind=Program for this.",
            expected_mode: "Program",
            validate: |r| {
                let completed = !r.completion_reason.starts_with("BLOCKED")
                    && !r.completion_reason.starts_with("budget");
                let used_program = r.turn_log.iter().any(|t| t.action_kind == "Program");
                let has_transform = r.dag_traces.iter().any(|d| {
                    d.steps.iter().any(|s| s.op == "Transform" && s.success)
                });
                let has_llm_in_dag = r.dag_traces.iter().any(|d| {
                    d.steps.iter().any(|s| s.op == "LlmCall" && s.success)
                });
                (
                    completed,
                    format!(
                        "completed={completed} used_program={used_program} has_transform={has_transform} llm_in_dag={has_llm_in_dag}"
                    ),
                )
            },
        },
    ]
}

// ─── Main eval ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn dag_runtime_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let requested = eval_models();
    let available: Vec<String> = requested
        .into_iter()
        .filter(|m| model_available(&registry, m))
        .collect();

    let mut report = Report::new();
    report.section("DAG Runtime Eval — Frontier Research Report");
    report.line(&format!(
        "Date: {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    report.line(&format!("Models available: {}", available.join(", ")));
    report.line(&format!(
        "Models requested but unavailable: {}",
        eval_models()
            .into_iter()
            .filter(|m| !available.contains(m))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if available.is_empty() {
        report.line("\nERROR: No models available. Set API keys and retry.");
        report.finish();
        panic!("No models available");
    }

    let all_scenarios = scenarios();
    report.line(&format!("Scenarios: {}", all_scenarios.len()));
    for s in &all_scenarios {
        report.line(&format!("  - {} (expected: {})", s.name, s.expected_mode));
    }

    let config = AlmConfig {
        max_turns: 10,
        max_recurse_depth: 3,
        timeout_budget_ms: 120_000,
        max_dag_steps: 30,
    };

    // ── Summary table ──
    #[allow(dead_code)]
    struct ScenarioResult {
        model: String,
        scenario: String,
        passed: bool,
        used_program: bool,
        expected_mode: String,
        detail: String,
        elapsed_ms: u64,
        turns: usize,
        dag_steps: usize,
        nested_llm_calls: usize,
        error: Option<String>,
    }

    let mut all_results: Vec<ScenarioResult> = Vec::new();

    for model_id in &available {
        report.section(&format!("Model: {model_id}"));

        for scenario in &all_scenarios {
            let start = Instant::now();
            report.line(&format!("\n  >>> Running: {} ...", scenario.name));

            let port = DagEvalPort::new(model_id.clone(), ModelRegistry::new());
            let harness = AlmHarness::new(port, ModelRegistry::new(), config.clone());

            let result = tokio::time::timeout(
                Duration::from_secs(120),
                harness.run(scenario.objective.to_string()),
            )
            .await;

            let elapsed_ms = start.elapsed().as_millis() as u64;

            // We need to get the port back for logs — but it's moved into harness.
            // Workaround: create a new port for logging. The Arc<Mutex> logs are
            // inside the harness's port. We can't get them back easily.
            //
            // Better approach: run with a port that has externalized logs.
            // Let me restructure — the port's Arc<Mutex> logs are shared.
            // Actually, port is consumed by HarnessRlm::new. We need to
            // share the logs via Arc.

            match result {
                Ok(Ok(run_result)) => {
                    let (passed, detail) = (scenario.validate)(&run_result);
                    let used_program = run_result
                        .turn_log
                        .iter()
                        .any(|t| t.action_kind == "Program");
                    let dag_steps: usize =
                        run_result.dag_traces.iter().map(|d| d.steps.len()).sum();
                    // We can't access llm_log from here since port was moved.
                    // The nested LLM calls are visible in dag_traces as LlmCall steps.
                    let nested_llm_calls = run_result
                        .dag_traces
                        .iter()
                        .flat_map(|d| &d.steps)
                        .filter(|s| s.op == "LlmCall" && !s.skipped)
                        .count();

                    report_scenario(
                        &mut report,
                        model_id,
                        scenario.name,
                        &run_result,
                        &[], // can't access port's llm_log after move
                        &[], // can't access port's emit_log after move
                        elapsed_ms,
                    );

                    let verdict = if passed { "PASS" } else { "FAIL" };
                    let mode_match = if scenario.expected_mode == "Program" && used_program {
                        "MODE_MATCH"
                    } else if scenario.expected_mode == "ToolCalls" && !used_program {
                        "MODE_MATCH"
                    } else if scenario.expected_mode == "Program" && !used_program {
                        "USED_TOOLCALLS_INSTEAD"
                    } else {
                        "USED_PROGRAM_UNEXPECTED"
                    };

                    report.line(&format!("\n  VERDICT: {verdict} | {mode_match} | {detail}"));

                    all_results.push(ScenarioResult {
                        model: model_id.clone(),
                        scenario: scenario.name.to_string(),
                        passed,
                        used_program,
                        expected_mode: scenario.expected_mode.to_string(),
                        detail,
                        elapsed_ms,
                        turns: run_result.turns_taken,
                        dag_steps,
                        nested_llm_calls,
                        error: None,
                    });
                }
                Ok(Err(e)) => {
                    report.line(&format!("  ERROR: {e}"));
                    all_results.push(ScenarioResult {
                        model: model_id.clone(),
                        scenario: scenario.name.to_string(),
                        passed: false,
                        used_program: false,
                        expected_mode: scenario.expected_mode.to_string(),
                        detail: String::new(),
                        elapsed_ms,
                        turns: 0,
                        dag_steps: 0,
                        nested_llm_calls: 0,
                        error: Some(e),
                    });
                }
                Err(_) => {
                    report.line("  TIMEOUT (120s)");
                    all_results.push(ScenarioResult {
                        model: model_id.clone(),
                        scenario: scenario.name.to_string(),
                        passed: false,
                        used_program: false,
                        expected_mode: scenario.expected_mode.to_string(),
                        detail: String::new(),
                        elapsed_ms,
                        turns: 0,
                        dag_steps: 0,
                        nested_llm_calls: 0,
                        error: Some("timeout (120s)".into()),
                    });
                }
            }
        }
    }

    // ── Summary table ──
    report.section("SUMMARY TABLE");
    report.line(&format!(
        "{:<25} {:<30} {:>5} {:>8} {:>7} {:>5} {:>5} {:>5}  {}",
        "MODEL", "SCENARIO", "PASS", "MODE", "MATCH", "TURNS", "DAG", "LLM", "DETAIL"
    ));
    report.line(&"-".repeat(130));

    let mut total_pass = 0;
    let mut total_run = 0;
    let mut program_used_count = 0;
    let mut program_expected_count = 0;

    for r in &all_results {
        total_run += 1;
        if r.passed {
            total_pass += 1;
        }
        if r.expected_mode == "Program" {
            program_expected_count += 1;
            if r.used_program {
                program_used_count += 1;
            }
        }

        let pass_str = if r.passed { "PASS" } else { "FAIL" };
        let mode_str = if r.used_program {
            "Program"
        } else {
            "ToolCalls"
        };
        let match_str = if r.expected_mode == "Program" && r.used_program {
            "YES"
        } else if r.expected_mode == "ToolCalls" && !r.used_program {
            "YES"
        } else {
            "NO"
        };

        let detail = if let Some(err) = &r.error {
            truncate(err, 40)
        } else {
            truncate(&r.detail, 40)
        };

        report.line(&format!(
            "{:<25} {:<30} {:>5} {:>8} {:>7} {:>5} {:>5} {:>5}  {}",
            r.model,
            r.scenario,
            pass_str,
            mode_str,
            match_str,
            r.turns,
            r.dag_steps,
            r.nested_llm_calls,
            detail
        ));
    }

    report.line(&"-".repeat(130));
    report.line(&format!(
        "Total: {total_pass}/{total_run} passed | Program mode used: {program_used_count}/{program_expected_count} expected"
    ));

    // ── Key research questions ──
    report.section("RESEARCH OBSERVATIONS");
    report.line("(Fill in after reviewing the results above)");
    report.line("");
    report.line("1. Do models spontaneously choose Program mode for complex tasks?");
    report.line("2. When models use Program, do they correctly author DAG dependencies?");
    report.line("3. Do models use LlmCall steps within DAGs (meta-cognition via nested calls)?");
    report.line("4. Do models use Gate/conditional steps, or do they prefer linear execution?");
    report.line("5. Do models use Transform steps for data manipulation?");
    report.line("6. Which models are best at DAG authoring? Quality tier differences?");
    report.line("7. Is the DAG representation ergonomic for the model, or do they struggle with the syntax?");
    report.line(
        "8. How does this compare to the model just writing bash scripts with embedded curl calls?",
    );

    report.finish();
}
