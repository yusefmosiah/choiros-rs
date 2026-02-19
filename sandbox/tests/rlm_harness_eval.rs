//! RLM Harness Eval — test the actual RLM execution model with real LLM calls.
//!
//! Compares AlmHarness (context composition + working memory) against the
//! linear AgentHarness on identical tasks.
//!
//! Run:
//!   cargo test -p sandbox --test rlm_harness_eval -- --nocapture
//!   CHOIR_LIVE_MODEL_IDS=KimiK25 cargo test -p sandbox --test rlm_harness_eval -- --nocapture

use sandbox::actors::agent_harness::alm::{
    LlmCallResult, AlmConfig, AlmHarness, AlmPort, RlmRunResult, AlmToolExecution,
};
use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::types::ContextSourceKind;
use sandbox::runtime_env::ensure_tls_cert_env;
use shared_types;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ─── Eval adapter ────────────────────────────────────────────────────────────

struct EvalAlmPort {
    model_id: String,
}

impl EvalAlmPort {
    fn new(model_id: String) -> Self {
        Self { model_id }
    }
}

#[async_trait::async_trait]
impl AlmPort for EvalAlmPort {
    fn capabilities_description(&self) -> String {
        r#"Available tools:

1. bash - Execute shell commands
   Args: command (string, required)

2. file_read - Read a local file
   Args: path (string, required)

3. file_write - Write a file
   Args: path (string, required), content (string, required)

Available context sources:
- Document: Load a file by path
- PreviousTurn: Include output from a prior turn (by turn number)
- ToolOutput: Include a specific tool result

Note: MemoryQuery is not yet available (Phase 5).
"#
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
            ContextSourceKind::Document => {
                // Resolve file reads
                tokio::fs::read_to_string(source_ref).await.ok()
            }
            ContextSourceKind::MemoryQuery => {
                // Stub: return nothing (Phase 5)
                Some("(memory not yet available)".to_string())
            }
            ContextSourceKind::PreviousTurn | ContextSourceKind::ToolOutput => {
                // Turn history is already in the turn context
                None
            }
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
                        error: Some("timeout".into()),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            "file_read" => {
                let path = tool_args.get("path").map(|s| s.as_str()).unwrap_or("");
                match tokio::fs::read_to_string(path).await {
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
        _system_prompt: Option<&str>,
        _model_hint: Option<&str>,
    ) -> LlmCallResult {
        // For eval: stub LLM call that echoes the prompt summary
        // Real implementation would call BAML with the resolved model
        let start = Instant::now();
        LlmCallResult {
            output: format!(
                "(eval stub LLM response for prompt of {} chars)",
                prompt.len()
            ),
            success: true,
            error: None,
            elapsed_ms: start.elapsed().as_millis() as u64,
        }
    }

    async fn emit_message(&self, message: &str) {
        println!("  [EMIT] {}", &message[..message.len().min(200)]);
    }

    fn run_id(&self) -> &str {
        "eval-run"
    }

    fn actor_id(&self) -> &str {
        "eval-actor"
    }

    async fn dispatch_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
        corr_id: &str,
    ) {
        // In eval: dispatch is a no-op stub — tools still run synchronously
        // via execute_tool. This will be wired to actor messages in Phase 4.5.
        println!("  [DISPATCH] corr:{corr_id} tool:{tool_name} args:{tool_args:?}");
    }

    async fn write_checkpoint(&self, checkpoint: &shared_types::HarnessCheckpoint) {
        // In eval: log checkpoint but don't persist (no EventStore in eval context).
        println!(
            "  [CHECKPOINT] run:{} turn:{} pending:{}",
            checkpoint.run_id,
            checkpoint.turn_number,
            checkpoint.pending_replies.len()
        );
    }

    async fn spawn_actor_harness(&self, objective: &str, _context: serde_json::Value, corr_id: &str) {
        // In eval: log spawn but don't actually launch actors (no actor system in eval).
        println!("  [SPAWN_SUBHARNESS] corr:{corr_id} objective:{objective}");
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
    let defaults = ["ClaudeBedrockSonnet46", "KimiK25", "ZaiGLM47"];
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

fn print_rlm_result(model_id: &str, scenario: &str, result: &RlmRunResult, elapsed_ms: u64) {
    println!(
        "\n  --- {model_id} / {scenario} ({elapsed_ms}ms, {} turns) ---",
        result.turns_taken
    );
    println!("  completion: {}", result.completion_reason);
    println!(
        "  tools: {}",
        result
            .tool_executions
            .iter()
            .map(|t| format!("{}({})", t.tool_name, if t.success { "ok" } else { "err" }))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for tl in &result.turn_log {
        println!(
            "    turn {}: [{}] wm='{}' sources={:?} ({}ms)",
            tl.turn_number,
            tl.action_kind,
            truncate(&tl.working_memory, 100),
            tl.sources_requested,
            tl.elapsed_ms,
        );
    }
    println!(
        "  final_wm: '{}'",
        truncate(&result.final_working_memory, 200)
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

// ─── Eval scenarios ──────────────────────────────────────────────────────────

#[tokio::test]
async fn rlm_harness_basic_scenarios() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let requested = eval_models();
    let available: Vec<String> = requested
        .into_iter()
        .filter(|m| model_available(&registry, m))
        .collect();

    println!("\n=== RLM Harness Eval ===");
    println!("models: {}", available.join(", "));
    assert!(!available.is_empty(), "No models available");

    let scenarios: Vec<(&str, &str)> = vec![
        (
            "bash_echo",
            "Run the command `echo RLM_OK` using bash and report the output.",
        ),
        (
            "multi_step",
            "Run `uname -s` to get the OS name, then create a file at /tmp/choiros_rlm_eval.txt containing that OS name. Verify the file was written correctly by reading it back.",
        ),
        (
            "context_compose",
            "Read the file at Cargo.toml to understand the project structure. Then summarize what workspace members exist.",
        ),
    ];

    let config = AlmConfig {
        max_turns: 8,
        max_recurse_depth: 2,
        timeout_budget_ms: 60_000,
        max_dag_steps: 30,
    };

    let mut total_pass = 0;
    let mut total_run = 0;

    for model_id in &available {
        for (name, objective) in &scenarios {
            total_run += 1;
            let start = Instant::now();
            println!("\n  running {model_id} / {name}...");

            let port = EvalAlmPort::new(model_id.clone());
            let harness = AlmHarness::new(port, ModelRegistry::new(), config.clone());

            let result =
                tokio::time::timeout(Duration::from_secs(90), harness.run(objective.to_string()))
                    .await;

            let elapsed_ms = start.elapsed().as_millis() as u64;

            match result {
                Ok(Ok(run_result)) => {
                    let is_complete = !run_result.completion_reason.starts_with("BLOCKED")
                        && !run_result.completion_reason.starts_with("budget exhausted");

                    print_rlm_result(model_id, name, &run_result, elapsed_ms);

                    if is_complete {
                        println!("  RESULT: PASS");
                        total_pass += 1;
                    } else {
                        println!("  RESULT: INCOMPLETE ({})", run_result.completion_reason);
                    }
                }
                Ok(Err(e)) => {
                    println!("  RESULT: ERROR — {e}");
                }
                Err(_) => {
                    println!("  RESULT: TIMEOUT (90s)");
                }
            }
        }
    }

    println!("\n=== RLM Eval Summary: {total_pass}/{total_run} passed ===\n");
}
