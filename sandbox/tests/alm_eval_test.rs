//! RLM Evaluation Suite — real model quality assessment across tiers.
//!
//! Tier 1: Solo BAML function evals (ConductorBootstrapAgenda, Decide, SummarizeChangeset)
//! Tier 2: Full AgentHarness loop with MinimalEvalAdapter
//! Tier 3: End-to-end /conductor/execute against a running server
//!
//! All tests make REAL LLM calls. They require API keys in the environment.
//! Use `CHOIR_LIVE_MODEL_IDS` to control which models are tested.
//!
//! Run:
//!   cargo test -p sandbox --test alm_eval_test -- --nocapture
//!   CHOIR_LIVE_MODEL_IDS=KimiK25,ClaudeBedrockSonnet46 cargo test -p sandbox --test alm_eval_test -- --nocapture

use ractor::Actor;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::types::{
    ChangesetInput, ConductorBootstrapInput, Message as BamlMessage,
};
use sandbox::baml_client::B;
use sandbox::runtime_env::ensure_tls_cert_env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;

// ─── Configuration ──────────────────────────────────────────────────────────

const DEFAULT_EVAL_MODEL_TARGETS: &[&str] = &[
    "ClaudeBedrockSonnet46",
    "KimiK25",
    "ZaiGLM47",
    "ZaiGLM47Flash",
];

// ─── Shared helpers ──────────────────────────────────────────────────────────

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

fn missing_env_for_provider(provider: &ProviderConfig) -> Vec<String> {
    match provider {
        ProviderConfig::AwsBedrock { .. } => {
            let tls_ready = ensure_tls_cert_env().is_some();
            if bedrock_auth_present() && tls_ready {
                Vec::new()
            } else {
                let mut missing = vec!["AWS auth or SSL_CERT_FILE".to_string()];
                if !tls_ready {
                    missing.push("SSL_CERT_FILE".to_string());
                }
                missing
            }
        }
        ProviderConfig::AnthropicCompatible { api_key_env, .. }
        | ProviderConfig::OpenAiGeneric { api_key_env, .. } => {
            if env_present(api_key_env) {
                Vec::new()
            } else {
                vec![api_key_env.clone()]
            }
        }
    }
}

fn is_rate_limited_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("ratelimited")
        || lower.contains("rate limited")
        || lower.contains("too many requests")
        || lower.contains("status code: 429")
}

async fn run_with_retry<T, F, Fut>(label: &str, attempts: usize, mut op: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let base_delay_ms: u64 = 1_200;
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_error = err.clone();
                if attempt < attempts && is_rate_limited_error(&err) {
                    let delay = base_delay_ms.saturating_mul(attempt as u64);
                    println!("  RETRY {label} attempt {attempt}/{attempts} after {delay}ms: {err}");
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                break;
            }
        }
    }
    Err(last_error)
}

fn available_eval_models(registry: &ModelRegistry) -> (Vec<String>, Vec<String>) {
    let mut eligible = Vec::new();
    let mut skipped = Vec::new();
    for model_id in registry.available_model_ids() {
        let Some(config) = registry.get(&model_id) else {
            skipped.push(format!("{model_id} (missing)"));
            continue;
        };
        let missing = missing_env_for_provider(&config.provider);
        if !missing.is_empty() {
            skipped.push(format!("{model_id} (missing env: {})", missing.join(",")));
            continue;
        }
        eligible.push(model_id);
    }
    (eligible, skipped)
}

fn requested_eval_model_targets() -> Vec<String> {
    if let Ok(raw) = std::env::var("CHOIR_LIVE_MODEL_IDS") {
        let mut parsed: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
            .collect();
        parsed.dedup();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    DEFAULT_EVAL_MODEL_TARGETS
        .iter()
        .map(|m| (*m).to_string())
        .collect()
}

fn sampled_eval_models(eligible: &[String]) -> Vec<String> {
    let requested = requested_eval_model_targets();
    requested
        .into_iter()
        .filter(|m| eligible.contains(m))
        .collect()
}

// ─── Grading ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Grade {
    Pass,
    Marginal(String),
    Fail(String),
}

impl Grade {
    fn symbol(&self) -> &str {
        match self {
            Grade::Pass => "PASS",
            Grade::Marginal(_) => "MARGINAL",
            Grade::Fail(_) => "FAIL",
        }
    }
}

#[derive(Debug, Clone)]
struct EvalResult {
    model_id: String,
    scenario: String,
    grade: Grade,
    latency_ms: u64,
    detail: String,
}

impl std::fmt::Display for EvalResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} / {} ({}ms) -- {}",
            self.grade.symbol(),
            self.model_id,
            self.scenario,
            self.latency_ms,
            self.detail,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 1.1: ConductorBootstrapAgenda
// ═══════════════════════════════════════════════════════════════════════════════

fn grade_bootstrap(
    capabilities: &[String],
    expected_any: &[String],
    forbidden: &[String],
    expect_block: bool,
    dispatched: &[String],
    block_reason: &Option<String>,
    confidence: f64,
    rationale: &str,
) -> Grade {
    if expect_block {
        if dispatched.is_empty() && block_reason.is_some() {
            return Grade::Pass;
        }
        return Grade::Fail(format!("expected block but got dispatched={dispatched:?}"));
    }

    if dispatched.is_empty() {
        return Grade::Fail(format!(
            "expected dispatch from {expected_any:?} but got nothing, block_reason={block_reason:?}"
        ));
    }

    for f in forbidden {
        if dispatched.iter().any(|d| d == f) {
            return Grade::Fail(format!("dispatched forbidden: {f}"));
        }
    }

    if !expected_any.is_empty() {
        let has = expected_any
            .iter()
            .any(|exp| dispatched.iter().any(|d| d == exp));
        if !has {
            return Grade::Marginal(format!(
                "dispatched {dispatched:?} but expected one of {expected_any:?}"
            ));
        }
    }

    for d in dispatched {
        if !capabilities.contains(d) {
            return Grade::Fail(format!("dispatched '{d}' not in available capabilities"));
        }
    }

    if confidence < 0.1 {
        return Grade::Marginal(format!("very low confidence: {confidence}"));
    }
    if rationale.trim().len() < 10 {
        return Grade::Marginal(format!("weak rationale: '{rationale}'"));
    }

    Grade::Pass
}

#[tokio::test]
async fn tier1_conductor_bootstrap_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_eval_models(&registry);
    let sampled = sampled_eval_models(&eligible);

    println!("\n=== TIER 1.1: ConductorBootstrapAgenda ===");
    println!("sampled models: {}", sampled.join(", "));
    if !skipped.is_empty() {
        println!("skipped: {}", skipped.join(", "));
    }
    assert!(!sampled.is_empty(), "No models available for eval");

    // scenarios: (name, objective, capabilities, expected_any, forbidden, expect_block)
    let scenarios: Vec<(&str, &str, Vec<String>, Vec<String>, Vec<String>, bool)> = vec![
        (
            "simple_greeting",
            "Hello! How are you?",
            vec!["immediate_response".into(), "writer".into()],
            vec!["immediate_response".into()],
            vec!["writer".into()],
            false,
        ),
        (
            "research_and_write",
            "Research the latest developments in Rust async runtimes and write a summary report",
            vec!["immediate_response".into(), "writer".into()],
            vec!["writer".into()],
            vec![],
            false,
        ),
        (
            "code_task",
            "Fix the failing test in src/actors/terminal.rs by analyzing the error output",
            vec!["immediate_response".into(), "writer".into()],
            vec!["writer".into()],
            vec![],
            false,
        ),
        (
            "no_capabilities",
            "Compile and deploy the application to production",
            vec![],
            vec![],
            vec![],
            true,
        ),
        (
            "ping",
            "ping",
            vec!["immediate_response".into(), "writer".into()],
            vec!["immediate_response".into()],
            vec![],
            false,
        ),
    ];

    let semaphore = Arc::new(Semaphore::new(2));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        for (name, objective, caps, expected, forbidden, expect_block) in scenarios.clone() {
            let registry = registry.clone();
            let model_id = model_id.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            join_set.spawn(async move {
                let _permit = permit;
                let start = Instant::now();

                let result = run_with_retry(&format!("bootstrap:{model_id}:{name}"), 3, || {
                    let registry = registry.clone();
                    let model_id = model_id.clone();
                    let caps = caps.clone();
                    let objective = objective.to_string();
                    async move {
                        let client_registry = registry
                            .create_runtime_client_registry_for_model(&model_id)
                            .map_err(|e| format!("registry: {e}"))?;
                        let input = ConductorBootstrapInput {
                            raw_objective: objective,
                            available_capabilities: caps,
                        };
                        tokio::time::timeout(
                            Duration::from_secs(30),
                            B.ConductorBootstrapAgenda
                                .with_client_registry(&client_registry)
                                .call(&input),
                        )
                        .await
                        .map_err(|_| "timed out".to_string())?
                        .map_err(|e| format!("call: {e}"))
                    }
                })
                .await;

                let latency_ms = start.elapsed().as_millis() as u64;
                match result {
                    Ok(output) => {
                        let grade = grade_bootstrap(
                            &caps,
                            &expected,
                            &forbidden,
                            expect_block,
                            &output.dispatch_capabilities,
                            &output.block_reason,
                            output.confidence,
                            &output.rationale,
                        );
                        EvalResult {
                            model_id,
                            scenario: name.to_string(),
                            grade,
                            latency_ms,
                            detail: format!(
                                "dispatched={:?} conf={:.2} rationale='{}'",
                                output.dispatch_capabilities,
                                output.confidence,
                                truncate(&output.rationale, 100),
                            ),
                        }
                    }
                    Err(reason) => EvalResult {
                        model_id,
                        scenario: name.to_string(),
                        grade: Grade::Fail(reason.clone()),
                        latency_ms,
                        detail: reason,
                    },
                }
            });
        }
    }

    let mut results = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        if let Ok(eval) = joined {
            println!("  {eval}");
            results.push(eval);
        }
    }

    print_eval_summary("Tier 1.1 -- ConductorBootstrapAgenda", &results);
    assert_failure_rate(&results, 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 1.2: Decide (tool use)
// ═══════════════════════════════════════════════════════════════════════════════

fn grade_decide(
    expect_tool_calls: bool,
    acceptable_tools: &[String],
    tool_names: &[String],
    message: &str,
) -> Grade {
    if expect_tool_calls {
        if tool_names.is_empty() {
            return Grade::Fail("expected tool calls but got none".to_string());
        }
        let non_finished: Vec<_> = tool_names
            .iter()
            .filter(|t| t.as_str() != "finished")
            .collect();
        if non_finished.is_empty() {
            return Grade::Marginal("only finished call, no action tools".to_string());
        }
        let has = non_finished
            .iter()
            .any(|t| acceptable_tools.iter().any(|a| a == *t));
        if !has {
            return Grade::Fail(format!(
                "tools {tool_names:?} don't match acceptable {acceptable_tools:?}"
            ));
        }
    }
    if message.trim().is_empty() {
        return Grade::Marginal("empty message".to_string());
    }
    Grade::Pass
}

#[tokio::test]
async fn tier1_decide_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_eval_models(&registry);
    let sampled = sampled_eval_models(&eligible);

    println!("\n=== TIER 1.2: Decide (tool use) ===");
    println!("sampled models: {}", sampled.join(", "));
    if !skipped.is_empty() {
        println!("skipped: {}", skipped.join(", "));
    }
    assert!(!sampled.is_empty(), "No models available for eval");

    // (name, messages, system_context, tools_json, expect_tool_calls, acceptable_tools)
    let scenarios: Vec<(
        &str,
        Vec<BamlMessage>,
        &str,
        &str,
        bool,
        Vec<String>,
    )> = vec![
        (
            "bash_simple",
            vec![BamlMessage {
                role: "user".into(),
                content: "Use bash to run `echo hello_world` and report the output.".into(),
            }],
            "You are a task executor. Execute commands when asked.",
            r#"[{"name":"bash","description":"Execute shell commands","parameters":{"command":"string"}},{"name":"finished","description":"Signal completion","parameters":{"summary":"string?"}}]"#,
            true,
            vec!["bash".into()],
        ),
        (
            "web_search",
            vec![BamlMessage {
                role: "user".into(),
                content: "Search the web for the current Rust stable release version.".into(),
            }],
            "You are a research assistant.",
            r#"[{"name":"web_search","description":"Search the web","parameters":{"query":"string"}},{"name":"finished","description":"Signal completion","parameters":{"summary":"string?"}}]"#,
            true,
            vec!["web_search".into()],
        ),
        (
            "file_read",
            vec![BamlMessage {
                role: "user".into(),
                content: "Read the file at src/main.rs and tell me what the main function does.".into(),
            }],
            "You are a code analysis assistant.",
            r#"[{"name":"file_read","description":"Read a file","parameters":{"path":"string"}},{"name":"finished","description":"Signal completion","parameters":{"summary":"string?"}}]"#,
            true,
            vec!["file_read".into()],
        ),
        (
            "message_parent",
            vec![BamlMessage {
                role: "user".into(),
                content: "Run `cargo test --lib` and report your progress using message_writer before finishing.".into(),
            }],
            "You are a sub-agent. Use message_writer to report progress to your parent conductor.",
            r#"[{"name":"bash","description":"Execute shell commands","parameters":{"command":"string"}},{"name":"message_writer","description":"Report progress to parent","parameters":{"content":"string","mode":"string","path":"string?","mode_arg":"string?"}},{"name":"finished","description":"Signal completion","parameters":{"summary":"string?"}}]"#,
            true,
            vec!["bash".into(), "message_writer".into()],
        ),
    ];

    let semaphore = Arc::new(Semaphore::new(2));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        for (name, messages, sys_ctx, tools, expect_tc, acceptable) in scenarios.clone() {
            let registry = registry.clone();
            let model_id = model_id.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            join_set.spawn(async move {
                let _permit = permit;
                let start = Instant::now();

                let result = run_with_retry(&format!("decide:{model_id}:{name}"), 3, || {
                    let registry = registry.clone();
                    let model_id = model_id.clone();
                    let messages = messages.clone();
                    let sys_ctx = sys_ctx.to_string();
                    let tools = tools.to_string();
                    async move {
                        let client_registry = registry
                            .create_runtime_client_registry_for_model(&model_id)
                            .map_err(|e| format!("registry: {e}"))?;
                        tokio::time::timeout(
                            Duration::from_secs(30),
                            B.Decide
                                .with_client_registry(&client_registry)
                                .call(&messages, &sys_ctx, &tools),
                        )
                        .await
                        .map_err(|_| "timed out".to_string())?
                        .map_err(|e| format!("call: {e}"))
                    }
                })
                .await;

                let latency_ms = start.elapsed().as_millis() as u64;
                match result {
                    Ok(decision) => {
                        let tool_names: Vec<String> =
                            decision.tool_calls.iter().map(extract_tool_name).collect();
                        let grade =
                            grade_decide(expect_tc, &acceptable, &tool_names, &decision.message);
                        EvalResult {
                            model_id,
                            scenario: name.to_string(),
                            grade,
                            latency_ms,
                            detail: format!(
                                "tools={tool_names:?} msg='{}'",
                                truncate(&decision.message, 80),
                            ),
                        }
                    }
                    Err(reason) => EvalResult {
                        model_id,
                        scenario: name.to_string(),
                        grade: Grade::Fail(reason.clone()),
                        latency_ms,
                        detail: reason,
                    },
                }
            });
        }
    }

    let mut results = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        if let Ok(eval) = joined {
            println!("  {eval}");
            results.push(eval);
        }
    }

    print_eval_summary("Tier 1.2 -- Decide", &results);
    assert_failure_rate(&results, 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 1.3: SummarizeChangeset
// ═══════════════════════════════════════════════════════════════════════════════

fn grade_changeset(
    acceptable_impacts: &[String],
    expected_keywords: &[String],
    summary: &str,
    impact_str: &str,
    op_taxonomy: &[String],
) -> Grade {
    if !acceptable_impacts
        .iter()
        .any(|a| impact_str.contains(a.as_str()))
    {
        return Grade::Marginal(format!(
            "impact '{impact_str}' not in {acceptable_impacts:?}"
        ));
    }
    let summary_lower = summary.to_ascii_lowercase();
    let has_kw = expected_keywords
        .iter()
        .any(|kw| summary_lower.contains(kw.as_str()));
    if !has_kw {
        return Grade::Marginal(format!("summary missing keywords {expected_keywords:?}"));
    }
    if op_taxonomy.is_empty() {
        return Grade::Marginal("empty op_taxonomy".to_string());
    }
    if summary.len() < 10 {
        return Grade::Marginal("summary too short".to_string());
    }
    Grade::Pass
}

#[tokio::test]
async fn tier1_summarize_changeset_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_eval_models(&registry);
    let sampled = sampled_eval_models(&eligible);

    println!("\n=== TIER 1.3: SummarizeChangeset ===");
    println!("sampled models: {}", sampled.join(", "));
    if !skipped.is_empty() {
        println!("skipped: {}", skipped.join(", "));
    }
    assert!(!sampled.is_empty(), "No models available for eval");

    // (name, before, after, ops_json, source, acceptable_impacts, expected_keywords)
    let scenarios: Vec<(
        &str, &str, &str, &str, &str,
        Vec<String>, Vec<String>,
    )> = vec![
        (
            "minor_typo_fix",
            "The quick brown fox jumps over teh lazy dog.",
            "The quick brown fox jumps over the lazy dog.",
            r#"[{"op":"replace","pos":36,"old_len":3,"text":"the"}]"#,
            "writer",
            vec!["Low".into()],
            vec!["typo".into(), "fix".into(), "correct".into()],
        ),
        (
            "new_section",
            "# Introduction\n\nThis is the intro.\n",
            "# Introduction\n\nThis is the intro.\n\n# Methods\n\nWe used a mixed-methods approach.\n",
            r#"[{"op":"insert","pos":35,"text":"..."}]"#,
            "writer",
            vec!["Medium".into(), "High".into()],
            vec!["section".into(), "method".into(), "add".into()],
        ),
        (
            "first_version",
            "",
            "# Project Report\n\n## Summary\n\nThe project delivered OAuth2 support.\n",
            r#"[{"op":"insert","pos":0,"text":"..."}]"#,
            "conductor",
            vec!["High".into()],
            vec!["initial".into(), "creat".into(), "report".into(), "new".into()],
        ),
    ];

    let semaphore = Arc::new(Semaphore::new(2));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        for (name, before, after, ops, source, impacts, keywords) in scenarios.clone() {
            let registry = registry.clone();
            let model_id = model_id.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            join_set.spawn(async move {
                let _permit = permit;
                let start = Instant::now();

                let result = run_with_retry(&format!("changeset:{model_id}:{name}"), 3, || {
                    let registry = registry.clone();
                    let model_id = model_id.clone();
                    async move {
                        let client_registry = registry
                            .create_runtime_client_registry_for_model(&model_id)
                            .map_err(|e| format!("registry: {e}"))?;
                        let input = ChangesetInput {
                            patch_id: format!("eval-{name}"),
                            loop_id: None,
                            before_content: before.to_string(),
                            after_content: after.to_string(),
                            ops_json: ops.to_string(),
                            source: source.to_string(),
                        };
                        tokio::time::timeout(
                            Duration::from_secs(30),
                            B.SummarizeChangeset
                                .with_client_registry(&client_registry)
                                .call(&input),
                        )
                        .await
                        .map_err(|_| "timed out".to_string())?
                        .map_err(|e| format!("call: {e}"))
                    }
                })
                .await;

                let latency_ms = start.elapsed().as_millis() as u64;
                match result {
                    Ok(output) => {
                        let impact_str = format!("{:?}", output.impact);
                        let grade = grade_changeset(
                            &impacts,
                            &keywords,
                            &output.summary,
                            &impact_str,
                            &output.op_taxonomy,
                        );
                        EvalResult {
                            model_id,
                            scenario: name.to_string(),
                            grade,
                            latency_ms,
                            detail: format!(
                                "impact={impact_str} taxonomy={:?} summary='{}'",
                                output.op_taxonomy,
                                truncate(&output.summary, 80),
                            ),
                        }
                    }
                    Err(reason) => EvalResult {
                        model_id,
                        scenario: name.to_string(),
                        grade: Grade::Fail(reason.clone()),
                        latency_ms,
                        detail: reason,
                    },
                }
            });
        }
    }

    let mut results = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        if let Ok(eval) = joined {
            println!("  {eval}");
            results.push(eval);
        }
    }

    print_eval_summary("Tier 1.3 -- SummarizeChangeset", &results);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 2: Full AgentHarness loop
// ═══════════════════════════════════════════════════════════════════════════════

use sandbox::actors::agent_harness::{
    AgentHarness, AgentProgress, ExecutionContext, HarnessConfig, HarnessError, ObjectiveStatus,
    ToolExecution, WorkerPort,
};
use sandbox::baml_client::types::Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall;
use sandbox::observability::llm_trace::LlmTraceEmitter;

struct MinimalEvalAdapter {
    model_id: String,
}

impl MinimalEvalAdapter {
    fn new(model_id: String) -> Self {
        Self { model_id }
    }
}

#[async_trait::async_trait]
impl WorkerPort for MinimalEvalAdapter {
    fn get_model_role(&self) -> &str {
        "harness"
    }

    fn get_tool_description(&self) -> String {
        r#"Available tools:

1. bash - Execute shell commands
   Args:
   - command: string (required)

2. file_read - Read a local file
   Args:
   - path: string (required)

3. file_write - Write a file
   Args:
   - path: string (required)
   - content: string (required)
"#
        .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            "You are an eval agent. Execute the objective efficiently.\nObjective: {}\nModel: {}",
            ctx.objective, self.model_id,
        )
    }

    async fn execute_tool_call(
        &self,
        _ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start = Instant::now();
        match tool_call {
            AgentToolCall::BashToolCall(call) => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let output = tokio::time::timeout(
                    Duration::from_secs(15),
                    tokio::process::Command::new(&shell)
                        .arg("-lc")
                        .arg(&call.tool_args.command)
                        .output(),
                )
                .await
                .map_err(|_| HarnessError::ToolExecution("bash timeout".into()))?
                .map_err(|e| HarnessError::ToolExecution(format!("bash: {e}")))?;

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

                Ok(ToolExecution {
                    tool_name: "bash".into(),
                    success: output.status.success(),
                    output: combined,
                    error: if output.status.success() {
                        None
                    } else {
                        Some(format!("exit {}", output.status.code().unwrap_or(1)))
                    },
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
            }
            AgentToolCall::FileReadToolCall(call) => {
                match tokio::fs::read_to_string(&call.tool_args.path).await {
                    Ok(content) => Ok(ToolExecution {
                        tool_name: "file_read".into(),
                        success: true,
                        output: content,
                        error: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                    Err(e) => Ok(ToolExecution {
                        tool_name: "file_read".into(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("read: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::FileWriteToolCall(call) => {
                match tokio::fs::write(&call.tool_args.path, &call.tool_args.content).await {
                    Ok(_) => Ok(ToolExecution {
                        tool_name: "file_write".into(),
                        success: true,
                        output: format!("wrote {} bytes", call.tool_args.content.len()),
                        error: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                    Err(e) => Ok(ToolExecution {
                        tool_name: "file_write".into(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("write: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            _ => Ok(ToolExecution {
                tool_name: "unknown".into(),
                success: false,
                output: String::new(),
                error: Some("tool not available in eval adapter".into()),
                execution_time_ms: start.elapsed().as_millis() as u64,
            }),
        }
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        _ctx: &ExecutionContext,
        _report: shared_types::WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn emit_progress(
        &self,
        _ctx: &ExecutionContext,
        _progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        Ok(())
    }
}

#[tokio::test]
async fn tier2_harness_loop_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_eval_models(&registry);
    let sampled = sampled_eval_models(&eligible);

    println!("\n=== TIER 2: Full AgentHarness loop ===");
    println!("sampled models: {}", sampled.join(", "));
    if !skipped.is_empty() {
        println!("skipped: {}", skipped.join(", "));
    }
    assert!(!sampled.is_empty(), "No models available for eval");

    let objectives: Vec<(&str, &str)> = vec![
        ("bash_echo", "Use bash to run `echo HARNESS_OK` and report the output."),
        ("file_create_read", "Create a file at /tmp/choiros_eval_test.txt containing 'eval_pass', then read it back to verify."),
        ("multi_step", "Run `uname -s` to get the OS name, then create a file /tmp/choiros_eval_os.txt with that name in it."),
    ];

    // Spawn in-memory event store for the trace emitter
    let (event_store, _es_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn in-memory event store");

    let mut results = Vec::new();

    for model_id in &sampled {
        for (name, objective) in &objectives {
            let start = Instant::now();
            println!("  running {model_id} / {name}...");

            let model_reg = ModelRegistry::new();
            let trace_emitter = LlmTraceEmitter::new(event_store.clone());
            let config = HarnessConfig {
                max_steps: 10,
                timeout_budget_ms: 45_000,
                emit_progress: false,
                emit_worker_report: false,
            };

            let adapter = MinimalEvalAdapter::new(model_id.clone());
            let harness = AgentHarness::with_config(adapter, model_reg, config, trace_emitter);

            let result = tokio::time::timeout(
                Duration::from_secs(60),
                harness.run(
                    format!("eval:{model_id}:{name}"),
                    "system".to_string(),
                    objective.to_string(),
                    None,
                    None,
                    None,
                    None,
                ),
            )
            .await;

            let latency_ms = start.elapsed().as_millis() as u64;

            let eval = match result {
                Ok(Ok(agent_result)) => {
                    let satisfied =
                        matches!(agent_result.objective_status, ObjectiveStatus::Complete);
                    let grade = if satisfied {
                        Grade::Pass
                    } else {
                        Grade::Marginal(format!("status={:?}", agent_result.objective_status,))
                    };
                    EvalResult {
                        model_id: model_id.clone(),
                        scenario: name.to_string(),
                        grade,
                        latency_ms,
                        detail: format!(
                            "steps={} reason='{}' summary='{}'",
                            agent_result.steps_taken,
                            agent_result.completion_reason,
                            truncate(&agent_result.summary, 80),
                        ),
                    }
                }
                Ok(Err(e)) => EvalResult {
                    model_id: model_id.clone(),
                    scenario: name.to_string(),
                    grade: Grade::Fail(format!("harness: {e}")),
                    latency_ms,
                    detail: e.to_string(),
                },
                Err(_) => EvalResult {
                    model_id: model_id.clone(),
                    scenario: name.to_string(),
                    grade: Grade::Fail("timeout 60s".into()),
                    latency_ms,
                    detail: "timeout".into(),
                },
            };

            println!("  {eval}");
            results.push(eval);
        }
    }

    print_eval_summary("Tier 2 -- AgentHarness loop", &results);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 3: End-to-end /conductor/execute
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tier3_conductor_e2e_eval() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();

    println!("\n=== TIER 3: End-to-end /conductor/execute ===");

    let base_url =
        std::env::var("CHOIR_EVAL_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let health = client.get(format!("{base_url}/health")).send().await;
    if health.is_err() {
        println!("  Server not running at {base_url} -- skipping Tier 3");
        println!("  Start with `just dev-sandbox` and re-run");
        return;
    }

    let objectives: Vec<(&str, &str)> = vec![
        ("greeting", "Hello, just checking in."),
        (
            "research_task",
            "Research what version of Rust is currently stable and write a brief note.",
        ),
        (
            "code_analysis",
            "Read sandbox/src/main.rs and summarize what it does.",
        ),
    ];

    let mut results = Vec::new();

    for (name, objective) in &objectives {
        let start = Instant::now();
        println!("  running e2e / {name}...");

        let body = serde_json::json!({
            "objective": objective,
            "desktop_id": format!("eval-{name}"),
        });

        let resp = client
            .post(format!("{base_url}/conductor/execute"))
            .json(&body)
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        let eval = match resp {
            Ok(r) => {
                let status = r.status();
                let body_text = r.text().await.unwrap_or_default();

                if status.is_success() || status.as_u16() == 202 {
                    match serde_json::from_str::<serde_json::Value>(&body_text) {
                        Ok(json) => {
                            let run_id = json.get("run_id").and_then(|v| v.as_str()).unwrap_or("?");
                            let run_status =
                                json.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                            let has_error =
                                json.get("error").map(|v| !v.is_null()).unwrap_or(false);

                            let grade = if has_error {
                                Grade::Fail(format!("error: {}", json["error"]))
                            } else if run_status == "Failed" || run_status == "Blocked" {
                                Grade::Marginal(format!("status: {run_status}"))
                            } else {
                                Grade::Pass
                            };

                            EvalResult {
                                model_id: "server-default".into(),
                                scenario: name.to_string(),
                                grade,
                                latency_ms,
                                detail: format!("run_id={run_id} status={run_status}"),
                            }
                        }
                        Err(e) => EvalResult {
                            model_id: "server-default".into(),
                            scenario: name.to_string(),
                            grade: Grade::Fail(format!("parse: {e}")),
                            latency_ms,
                            detail: truncate(&body_text, 100),
                        },
                    }
                } else {
                    EvalResult {
                        model_id: "server-default".into(),
                        scenario: name.to_string(),
                        grade: Grade::Fail(format!("HTTP {status}")),
                        latency_ms,
                        detail: truncate(&body_text, 100),
                    }
                }
            }
            Err(e) => EvalResult {
                model_id: "server-default".into(),
                scenario: name.to_string(),
                grade: Grade::Fail(format!("request: {e}")),
                latency_ms,
                detail: e.to_string(),
            },
        };

        println!("  {eval}");
        results.push(eval);
    }

    print_eval_summary("Tier 3 -- /conductor/execute e2e", &results);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_tool_name(tc: &AgentToolCall) -> String {
    match tc {
        AgentToolCall::BashToolCall(c) => c.tool_name.clone(),
        AgentToolCall::WebSearchToolCall(c) => c.tool_name.clone(),
        AgentToolCall::FetchUrlToolCall(c) => c.tool_name.clone(),
        AgentToolCall::FileReadToolCall(c) => c.tool_name.clone(),
        AgentToolCall::FileWriteToolCall(c) => c.tool_name.clone(),
        AgentToolCall::FileEditToolCall(c) => c.tool_name.clone(),
        AgentToolCall::MessageWriterToolCall(c) => c.tool_name.clone(),
        AgentToolCall::FinishedToolCall(c) => c.tool_name.clone(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn print_eval_summary(tier_name: &str, results: &[EvalResult]) {
    let total = results.len();
    let pass = results
        .iter()
        .filter(|r| matches!(r.grade, Grade::Pass))
        .count();
    let marginal = results
        .iter()
        .filter(|r| matches!(r.grade, Grade::Marginal(_)))
        .count();
    let fail = results
        .iter()
        .filter(|r| matches!(r.grade, Grade::Fail(_)))
        .count();
    let avg_latency = if total > 0 {
        results.iter().map(|r| r.latency_ms).sum::<u64>() / total as u64
    } else {
        0
    };

    println!("\n--- {tier_name} Summary ---");
    println!(
        "  total={total} pass={pass} marginal={marginal} fail={fail} avg_latency={avg_latency}ms"
    );

    let mut model_ids: Vec<String> = results.iter().map(|r| r.model_id.clone()).collect();
    model_ids.sort();
    model_ids.dedup();

    for mid in &model_ids {
        let mr: Vec<_> = results.iter().filter(|r| r.model_id == *mid).collect();
        let mp = mr.iter().filter(|r| matches!(r.grade, Grade::Pass)).count();
        let mt = mr.len();
        let ml = if mt > 0 {
            mr.iter().map(|r| r.latency_ms).sum::<u64>() / mt as u64
        } else {
            0
        };
        println!("  {mid}: {mp}/{mt} pass, avg {ml}ms");
    }
    println!();
}

fn assert_failure_rate(results: &[EvalResult], max_ratio_denominator: usize) {
    let failures: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.grade, Grade::Fail(_)))
        .collect();
    let threshold = results.len() / max_ratio_denominator;
    assert!(
        failures.len() <= threshold,
        "Too many failures ({}/{}, max {}): {}",
        failures.len(),
        results.len(),
        threshold,
        failures
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
