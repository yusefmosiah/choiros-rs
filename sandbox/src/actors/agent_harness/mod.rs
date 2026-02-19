//! Unified Agent Harness - Shared loop framework for agentic workers
//!
//! This module provides a generic harness for building agentic workers with:
//! - Model resolution via ModelRegistry
//! - BAML-based decision loop (simplified: Decide -> Execute tools -> loop/return)
//! - Structured event emission (started, progress, completed/failed)
//! - WorkerTurnReport generation at completion
//!
//! The `rlm` submodule provides the ALM (Agentic Language Model) harness —
//! the general execution mode where the model composes its own context each turn.
//! The linear `AgentHarness` loop is a degenerate case of the RLM pattern.
//!
//! ## Architecture
//!
//! The harness uses a simplified loop:
//! DECIDE -> EXECUTE TOOLS -> (loop or return final message)
//!
//! ## Usage
//!
//! Implement the `WorkerPort` trait for your specific worker type, then use
//! `AgentHarness::run()` to execute the agentic loop.
//!
//! ```rust,ignore
//! pub struct MyAdapter;
//!
//! impl WorkerPort for MyAdapter {
//!     fn get_model_role(&self) -> &str { "my_worker" }
//!     // ... other methods
//! }
//!
//! let harness = AgentHarness::new(adapter, ModelRegistry::new());
//! let result = harness.run(objective, timeout, max_steps).await?;
//! ```

pub mod alm;
pub mod alm_port;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::{ModelConfigError, ModelRegistry, ModelResolutionContext};
use crate::baml_client::types::{
    AgentDecision, Message as BamlMessage,
    Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall,
};
use crate::baml_client::{new_collector, ClientRegistry, B};
use crate::observability::llm_trace::{
    token_usage_from_collector, LlmCallScope, LlmTokenUsage, LlmTraceEmitter,
};

// Re-export shared types for convenience
pub use shared_types::{
    WorkerArtifact, WorkerEscalation, WorkerEscalationKind, WorkerEscalationUrgency,
    WorkerTurnReport, WorkerTurnStatus,
};

// ============================================================================
// Core Types
// ============================================================================

const TOOL_OUTPUT_ARTIFACT_DIR: &str = "agent_harness/tool_outputs";
const MIN_TOOL_OUTPUT_ECHO_CHARS: usize = 1_000;
const MAX_TOOL_OUTPUT_ECHO_CHARS: usize = 6_000;
const INPUT_TOKEN_SPIKE_DELTA_THRESHOLD: i64 = 1_000;
const INPUT_TOKEN_SPIKE_RATIO_THRESHOLD: f64 = 1.35;

fn tool_call_name(tool_call: &AgentToolCall) -> &str {
    match tool_call {
        AgentToolCall::BashToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::WebSearchToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FetchUrlToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileReadToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileWriteToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileEditToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::MessageWriterToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FinishedToolCall(call) => call.tool_name.as_str(),
    }
}

fn decision_tool_names(decision: &AgentDecision) -> Vec<&str> {
    decision.tool_calls.iter().map(tool_call_name).collect()
}

fn tool_call_reasoning(tool_call: &AgentToolCall) -> Option<&str> {
    match tool_call {
        AgentToolCall::BashToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::WebSearchToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::FetchUrlToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::FileReadToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::FileWriteToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::FileEditToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::MessageWriterToolCall(call) => call.reasoning.as_deref(),
        AgentToolCall::FinishedToolCall(call) => call.reasoning.as_deref(),
    }
}

fn tool_call_args_json(tool_call: &AgentToolCall) -> serde_json::Value {
    match tool_call {
        AgentToolCall::BashToolCall(call) => serde_json::json!({
            "command": call.tool_args.command,
        }),
        AgentToolCall::WebSearchToolCall(call) => serde_json::json!({
            "query": call.tool_args.query,
        }),
        AgentToolCall::FetchUrlToolCall(call) => serde_json::json!({
            "path": call.tool_args.path,
        }),
        AgentToolCall::FileReadToolCall(call) => serde_json::json!({
            "path": call.tool_args.path,
        }),
        AgentToolCall::FileWriteToolCall(call) => serde_json::json!({
            "path": call.tool_args.path,
            "content": call.tool_args.content,
        }),
        AgentToolCall::FileEditToolCall(call) => serde_json::json!({
            "path": call.tool_args.path,
            "old_text": call.tool_args.old_text,
            "new_text": call.tool_args.new_text,
        }),
        AgentToolCall::MessageWriterToolCall(call) => serde_json::json!({
            "path": call.tool_args.path,
            "content": call.tool_args.content,
            "mode": call.tool_args.mode,
            "mode_arg": call.tool_args.mode_arg,
        }),
        AgentToolCall::FinishedToolCall(call) => serde_json::json!({
            "summary": call.tool_args.summary,
        }),
    }
}

fn truncate_for_next_turn(value: &str, max_chars: usize) -> (String, bool) {
    let mut iter = value.chars();
    let truncated: String = iter.by_ref().take(max_chars).collect();
    let was_truncated = iter.next().is_some();
    (truncated, was_truncated)
}

fn tool_name_slug(tool_name: &str) -> String {
    let slug: String = tool_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.to_string()
    }
}

fn adaptive_tool_output_echo_chars(step_number: usize, max_steps: usize) -> usize {
    let remaining_steps = max_steps.saturating_sub(step_number);
    if remaining_steps <= 1 {
        MAX_TOOL_OUTPUT_ECHO_CHARS
    } else if remaining_steps <= 3 {
        3_000
    } else {
        MIN_TOOL_OUTPUT_ECHO_CHARS
    }
}

fn tool_output_relative_path(
    loop_id: &str,
    step_number: usize,
    tool_index: usize,
    tool_name: &str,
) -> String {
    format!(
        "{}/{}/step-{:02}-tool-{:02}-{}.json",
        TOOL_OUTPUT_ARTIFACT_DIR,
        loop_id,
        step_number,
        tool_index + 1,
        tool_name_slug(tool_name)
    )
}

fn option_delta(current: i64, previous: Option<i64>) -> Option<i64> {
    previous.map(|prev| current.saturating_sub(prev))
}

fn is_input_token_spike(current: i64, previous: Option<i64>, delta: Option<i64>) -> bool {
    let delta_spike = delta
        .map(|value| value >= INPUT_TOKEN_SPIKE_DELTA_THRESHOLD)
        .unwrap_or(false);
    let ratio_spike = previous
        .filter(|prev| *prev > 0)
        .map(|prev| (current as f64 / prev as f64) >= INPUT_TOKEN_SPIKE_RATIO_THRESHOLD)
        .unwrap_or(false);
    delta_spike || ratio_spike
}

fn is_retryable_decision_error_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("parsing error")
        || lower.contains("failed to parse llm response")
        || lower.contains("missing required field")
        || lower.contains("missing required fields")
}

/// Resolve the base directory for tool-output artifacts.
///
/// Preference order:
/// 1. `CHOIROS_DATA_DIR` env var (set by the runtime/container)
/// 2. `CARGO_MANIFEST_DIR` (fallback for local dev builds)
///
/// Using `CARGO_MANIFEST_DIR` via `env!()` is a compile-time macro that bakes
/// in the source-tree path. That is fine for local dev but breaks in CI /
/// containers / installed binaries where the source tree is absent.
fn artifact_base_dir() -> PathBuf {
    if let Ok(data_dir) = std::env::var("CHOIROS_DATA_DIR") {
        if !data_dir.is_empty() {
            return PathBuf::from(data_dir);
        }
    }
    // Compile-time fallback — acceptable for local dev, not for production.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

async fn persist_tool_execution_artifact(
    loop_id: &str,
    step_number: usize,
    tool_index: usize,
    tool_name: &str,
    execution: &ToolExecution,
) -> Result<String, std::io::Error> {
    let relative_path = tool_output_relative_path(loop_id, step_number, tool_index, tool_name);
    let absolute_path = artifact_base_dir().join(&relative_path);
    if let Some(parent) = absolute_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let payload = serde_json::json!({
        "tool_name": execution.tool_name,
        "success": execution.success,
        "execution_time_ms": execution.execution_time_ms,
        "error": execution.error,
        "output": execution.output,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let serialized = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    tokio::fs::write(&absolute_path, serialized).await?;
    Ok(relative_path)
}

fn tool_execution_message(
    ctx: &ExecutionContext,
    tool_name: &str,
    execution: &ToolExecution,
    output_artifact_path: Option<&str>,
) -> String {
    let max_echo_chars = adaptive_tool_output_echo_chars(ctx.step_number, ctx.max_steps);
    let (output_excerpt, output_truncated) =
        truncate_for_next_turn(&execution.output, max_echo_chars);
    let output_label = if output_truncated {
        "Output (truncated)"
    } else {
        "Output"
    };
    let artifact_line = output_artifact_path
        .map(|path| format!("\nFullOutputPath: {path}"))
        .unwrap_or_default();
    let error_line = execution
        .error
        .as_ref()
        .map(|err| format!("\nError: {err}"))
        .unwrap_or_default();
    let retrieval_hint = if output_artifact_path.is_some() {
        "\nIf needed, inspect full output via file_read or bash cat using FullOutputPath."
    } else {
        ""
    };

    format!(
        "Executed {tool_name}\nSuccess: {}{artifact_line}\n{output_label}: {output_excerpt}{error_line}{retrieval_hint}",
        execution.success
    )
}

/// State machine states for the agentic loop (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLoopState {
    /// Still deciding/executing
    Running,
    /// Got final model response
    Complete,
    /// Blocked due to model/tool failure
    Blocked,
}

/// Configuration for the agent harness
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Default timeout for tool execution (milliseconds)
    pub timeout_budget_ms: u64,
    /// Maximum number of planning/execution steps
    pub max_steps: usize,
    /// Whether to emit progress events
    pub emit_progress: bool,
    /// Whether to generate WorkerTurnReport at completion
    pub emit_worker_report: bool,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            timeout_budget_ms: 30_000,
            max_steps: 100,
            emit_progress: true,
            emit_worker_report: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 2.5 — HarnessProfile
// ---------------------------------------------------------------------------

/// Execution profile for an `AgentHarness` run.
///
/// The profile controls step budget and context management policy.
/// Selection happens at spawn time via `HarnessConfig::from_profile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessProfile {
    /// Brief conductor turns: low step budget, memory-managed context,
    /// fast decisions. No direct tool execution.
    Conductor,
    /// Full worker turns: high step budget, full context, returns
    /// `result + findings + citations`.
    Worker,
    /// Scoped subharness turns: medium step budget, objective-scoped context,
    /// returns typed completion to conductor.
    Harness,
}

impl HarnessProfile {
    /// Default `HarnessConfig` for this profile.
    pub fn default_config(self) -> HarnessConfig {
        match self {
            HarnessProfile::Conductor => HarnessConfig {
                timeout_budget_ms: 10_000,
                max_steps: 10,
                emit_progress: false,
                emit_worker_report: false,
            },
            HarnessProfile::Worker => HarnessConfig {
                timeout_budget_ms: 120_000,
                max_steps: 200,
                emit_progress: true,
                emit_worker_report: true,
            },
            HarnessProfile::Harness => HarnessConfig {
                timeout_budget_ms: 60_000,
                max_steps: 50,
                emit_progress: true,
                emit_worker_report: false,
            },
        }
    }
}

// ---------------------------------------------------------------------------

/// Context passed to the adapter during execution
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Unique identifier for this execution loop
    pub loop_id: String,
    /// Worker/actor identifier
    pub worker_id: String,
    /// User identifier
    pub user_id: String,
    /// Current step number (1-indexed)
    pub step_number: usize,
    /// Maximum steps allowed
    pub max_steps: usize,
    /// Model being used
    pub model_used: String,
    /// Original objective
    pub objective: String,
    /// Optional conductor run scope
    pub run_id: Option<String>,
    /// Optional conductor capability call id
    pub call_id: Option<String>,
}

/// Result of a single tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Progress update emitted during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProgress {
    pub phase: String,
    pub message: String,
    pub step_index: Option<usize>,
    pub step_total: Option<usize>,
    pub model_used: Option<String>,
    pub timestamp: String,
    /// Additional context specific to the worker type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Final result from agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub summary: String,
    pub success: bool,
    pub objective_status: ObjectiveStatus,
    pub completion_reason: String,
    pub model_used: Option<String>,
    pub steps_taken: usize,
    pub tool_executions: Vec<ToolExecution>,
    pub worker_report: Option<WorkerTurnReport>,
    /// Optional fields for extensibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Status of the objective at completion
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveStatus {
    /// Objective fully achieved
    Complete,
    /// Partial progress but not complete
    Incomplete,
    /// Cannot proceed (needs escalation)
    Blocked,
}

/// Error types for the harness
#[derive(Debug, thiserror::Error, Clone)]
pub enum HarnessError {
    #[error("Model resolution error: {0}")]
    ModelResolution(String),
    #[error("Decision failed: {0}")]
    Decision(String),
    #[error("Tool execution failed: {0}")]
    ToolExecution(String),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("Blocked: {0}")]
    Blocked(String),
    #[error("Adapter error: {0}")]
    Adapter(String),
}

impl From<ModelConfigError> for HarnessError {
    fn from(e: ModelConfigError) -> Self {
        match e {
            ModelConfigError::UnknownModel(id) => {
                HarnessError::ModelResolution(format!("Unknown model: {id}"))
            }
            ModelConfigError::MissingApiKey(env) => {
                HarnessError::ModelResolution(format!("Missing API key: {env}"))
            }
            ModelConfigError::NoFallbackAvailable => {
                HarnessError::ModelResolution("No fallback model available".to_string())
            }
        }
    }
}

// ============================================================================
// WorkerPort Trait
// ============================================================================

/// Trait for adapting the harness to specific worker types
///
/// Implement this trait to customize the harness behavior for your worker.
/// The worker port provides worker-specific logic while the harness handles
/// the common loop control flow.
#[async_trait]
pub trait WorkerPort: Send + Sync {
    /// Returns the model role identifier for this worker type
    /// (e.g., "terminal", "researcher")
    fn get_model_role(&self) -> &str;

    /// Returns the tool description for BAML planning
    ///
    /// This should be a formatted string describing available tools
    /// in a format the LLM can understand.
    fn get_tool_description(&self) -> String;

    /// Optional strict allow-list for tool names in model decisions.
    ///
    /// When set, the harness rejects decisions containing tools outside this
    /// allow-list and retries once with a correction reminder.
    fn allowed_tool_names(&self) -> Option<&'static [&'static str]> {
        None
    }

    /// Returns the system context for planning
    ///
    /// This is added to the system prompt for the planning LLM.
    fn get_system_context(&self, ctx: &ExecutionContext) -> String;

    /// Execute a tool call
    ///
    /// The adapter is responsible for executing the named tool with
    /// the provided arguments and returning the result.
    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError>;

    /// Check if a tool call should be deferred to another actor/system
    ///
    /// Returns true if the tool should not be executed locally but
    /// instead deferred to external handling.
    fn should_defer(&self, tool_name: &str) -> bool;

    /// Emit a structured WorkerTurnReport
    ///
    /// Called at loop completion to emit the final report.
    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), HarnessError>;

    /// Emit a progress update
    ///
    /// Called during loop execution to report progress.
    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError>;

    /// Build the final WorkerTurnReport
    ///
    /// The harness provides a default implementation, but the adapter
    /// can customize the report generation.
    fn build_worker_report(
        &self,
        ctx: &ExecutionContext,
        summary: &str,
        success: bool,
    ) -> WorkerTurnReport {
        WorkerTurnReport {
            turn_id: ctx.loop_id.clone(),
            worker_id: ctx.worker_id.clone(),
            task_id: ctx.loop_id.clone(),
            worker_role: Some(self.get_model_role().to_string()),
            status: if success {
                WorkerTurnStatus::Completed
            } else {
                WorkerTurnStatus::Failed
            },
            summary: Some(summary.to_string()),
            findings: Vec::new(),
            learnings: Vec::new(),
            escalations: Vec::new(),
            artifacts: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Validate whether a terminal model completion decision is allowed.
    ///
    /// Adapters can enforce capability-specific completion invariants.
    fn validate_terminal_decision(
        &self,
        _ctx: &ExecutionContext,
        _decision: &AgentDecision,
        _tool_executions: &[ToolExecution],
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Backward-compatible alias for older call sites.
///
/// New code should implement/use `WorkerPort`.
pub trait AgentAdapter: WorkerPort {}
impl<T: WorkerPort + ?Sized> AgentAdapter for T {}

// ============================================================================
// AgentHarness
// ============================================================================

/// The unified agent harness
///
/// This struct provides the core agentic loop implementation.
/// It is generic over the `WorkerPort` trait to allow customization
/// for different worker types.
pub struct AgentHarness<W: WorkerPort> {
    worker_port: W,
    model_registry: ModelRegistry,
    config: HarnessConfig,
    trace_emitter: LlmTraceEmitter,
}

impl<W: WorkerPort> AgentHarness<W> {
    fn disallowed_tool_names(&self, decision: &AgentDecision) -> Vec<String> {
        let Some(allowed) = self.worker_port.allowed_tool_names() else {
            return Vec::new();
        };
        let mut disallowed = Vec::new();
        for tool_name in decision_tool_names(decision) {
            if !allowed
                .iter()
                .any(|allowed_name| tool_name == *allowed_name)
            {
                disallowed.push(tool_name.to_string());
            }
        }
        disallowed.sort();
        disallowed.dedup();
        disallowed
    }

    fn is_retryable_decide_error(error: &HarnessError) -> bool {
        match error {
            HarnessError::Decision(message) => is_retryable_decision_error_message(message),
            _ => false,
        }
    }

    /// Create a new agent harness with the given worker port and model registry
    pub fn new(
        worker_port: W,
        model_registry: ModelRegistry,
        trace_emitter: LlmTraceEmitter,
    ) -> Self {
        Self {
            worker_port,
            model_registry,
            config: HarnessConfig::default(),
            trace_emitter,
        }
    }

    pub fn with_config(
        worker_port: W,
        model_registry: ModelRegistry,
        config: HarnessConfig,
        trace_emitter: LlmTraceEmitter,
    ) -> Self {
        Self {
            worker_port,
            model_registry,
            config,
            trace_emitter,
        }
    }

    /// Run the agentic loop with the given objective
    ///
    /// This is the main entry point for executing an agentic task.
    /// The simplified loop:
    /// 1. Resolve the model to use
    /// 2. Call BAML Decide to get tool calls or a final response
    /// 3. Execute tools or return result
    /// 4. Emit a WorkerTurnReport
    pub async fn run(
        &self,
        worker_id: String,
        user_id: String,
        objective: String,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<AgentProgress>>,
        run_id: Option<String>,
        call_id: Option<String>,
    ) -> Result<AgentResult, HarnessError> {
        let loop_id = ulid::Ulid::new().to_string();

        // Resolve model
        let resolved_model = self
            .model_registry
            .resolve_for_callsite(
                self.worker_port.get_model_role(),
                &ModelResolutionContext {
                    request_model: model_override,
                    app_preference: None,
                    user_preference: None,
                },
            )
            .map_err(HarnessError::from)?;
        let model_used = resolved_model.config.id;

        info!(
            loop_id = %loop_id,
            worker_id = %worker_id,
            role = %self.worker_port.get_model_role(),
            model = %model_used,
            "Starting agentic loop"
        );

        // Create execution context
        let mut ctx = ExecutionContext {
            loop_id: loop_id.clone(),
            worker_id: worker_id.clone(),
            user_id: user_id.clone(),
            step_number: 0,
            max_steps: self.config.max_steps,
            model_used: model_used.clone(),
            objective: objective.clone(),
            run_id,
            call_id,
        };

        // Emit started event
        self.emit_started(&ctx).await?;

        // Initialize loop state
        let current_utc = chrono::Utc::now().to_rfc3339();
        let mut messages = vec![BamlMessage {
            role: "user".to_string(),
            content: format!(
                "Current UTC datetime: {current_utc}\n\
                 Use this timestamp to resolve relative-time references (e.g., today, yesterday, this week), especially for search and verification tasks.\n\n\
                 Objective:\n{objective}"
            ),
        }];
        let mut tool_executions: Vec<ToolExecution> = Vec::new();
        let mut step_count = 0;
        let mut final_summary = String::new();
        let mut completion_reason = String::new();
        let mut objective_status = ObjectiveStatus::Incomplete;
        let mut loop_state = AgentLoopState::Running;
        let mut previous_decide_input_tokens: Option<i64> = None;
        let mut previous_decide_output_tokens: Option<i64> = None;
        let mut previous_decide_cached_input_tokens: Option<i64> = None;
        let mut cumulative_decide_input_tokens: i64 = 0;
        let mut cumulative_decide_output_tokens: i64 = 0;
        let mut cumulative_decide_cached_input_tokens: i64 = 0;

        // Get client registry for BAML calls
        let client_registry = self
            .model_registry
            .create_runtime_client_registry_for_model(&model_used)
            .map_err(HarnessError::from)?;
        // Build a stable system context once per loop. This prevents per-step
        // prompt churn that defeats upstream prompt caching.
        let system_context = self.worker_port.get_system_context(&ctx);

        // Deadline from the configured budget. Checked at the top of each step.
        let loop_deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(self.config.timeout_budget_ms);

        // Main loop: Decide -> Execute -> (loop or return)
        while step_count < self.config.max_steps && loop_state == AgentLoopState::Running {
            // Enforce wall-clock budget before starting each new step.
            if tokio::time::Instant::now() >= loop_deadline {
                tracing::warn!(
                    loop_id = %loop_id,
                    step = step_count,
                    budget_ms = self.config.timeout_budget_ms,
                    "Harness timeout budget exceeded; stopping loop"
                );
                objective_status = ObjectiveStatus::Incomplete;
                completion_reason = format!(
                    "Timeout: exceeded {}ms budget",
                    self.config.timeout_budget_ms
                );
                if final_summary.is_empty() {
                    final_summary = format!(
                        "Timeout after {}ms. Completed {} steps.",
                        self.config.timeout_budget_ms, step_count
                    );
                }
                loop_state = AgentLoopState::Blocked;
                break;
            }

            step_count += 1;
            ctx.step_number = step_count;

            self.emit_progress_internal(
                &ctx,
                &progress_tx,
                "deciding",
                &format!("Deciding step {}/{}", step_count, self.config.max_steps),
                Some(step_count),
                Some(self.config.max_steps),
            )
            .await?;

            // Call BAML Decide
            let mut decision_result: Option<(AgentDecision, Option<LlmTokenUsage>)> = None;
            let mut decision_error: Option<HarnessError> = None;

            for attempt in 0..=1 {
                match self
                    .decide(&client_registry, &messages, &ctx, &system_context)
                    .await
                {
                    Ok(result) => {
                        let disallowed_tools = self.disallowed_tool_names(&result.0);
                        if !disallowed_tools.is_empty() {
                            let allowed_tools = self
                                .worker_port
                                .allowed_tool_names()
                                .map(|names| names.join(", "))
                                .unwrap_or_else(|| "<none>".to_string());
                            let disallowed_tools_csv = disallowed_tools.join(", ");
                            if attempt == 0 {
                                self.emit_progress_internal(
                                    &ctx,
                                    &progress_tx,
                                    "decide_retry",
                                    &format!(
                                        "Decision used disallowed tools ({disallowed_tools_csv}); retrying with strict tool contract"
                                    ),
                                    Some(step_count),
                                    Some(self.config.max_steps),
                                )
                                .await?;
                                messages.push(BamlMessage {
                                    role: "assistant".to_string(),
                                    content: format!(
                                        "Previous output violated tool contract. Disallowed tools: {disallowed_tools_csv}. Allowed tools: {allowed_tools}. Retry with only allowed tool names."
                                    ),
                                });
                                continue;
                            }
                            decision_error = Some(HarnessError::Decision(format!(
                                "Model emitted disallowed tools after retry: {disallowed_tools_csv}. Allowed: {allowed_tools}"
                            )));
                            break;
                        }
                        decision_result = Some(result);
                        break;
                    }
                    Err(err) => {
                        let retryable = Self::is_retryable_decide_error(&err);
                        if attempt == 0 && retryable {
                            self.emit_progress_internal(
                                &ctx,
                                &progress_tx,
                                "decide_retry",
                                "Decision parse failed; retrying with stricter schema reminder",
                                Some(step_count),
                                Some(self.config.max_steps),
                            )
                            .await?;
                            messages.push(BamlMessage {
                                role: "assistant".to_string(),
                                content: "Previous output was invalid AgentDecision JSON. Retry with strict JSON object including both required fields: tool_calls (array) and message (string).".to_string(),
                            });
                            continue;
                        }
                        decision_error = Some(err);
                        break;
                    }
                }
            }

            let (decision, decide_usage) = match decision_result {
                Some(result) => result,
                None => {
                    let error = decision_error.unwrap_or_else(|| {
                        HarnessError::Decision("Unknown decision failure".into())
                    });
                    error!(error = %error, "Decision failed");
                    objective_status = ObjectiveStatus::Blocked;
                    completion_reason = format!("Decision failed: {error}");
                    loop_state = AgentLoopState::Blocked;
                    break;
                }
            };

            if let Some(usage) = decide_usage {
                let input_tokens = usage.input_tokens.unwrap_or(0).max(0);
                let output_tokens = usage.output_tokens.unwrap_or(0).max(0);
                let cached_input_tokens = usage.cached_input_tokens.unwrap_or(0).max(0);

                let input_delta = option_delta(input_tokens, previous_decide_input_tokens);
                let output_delta = option_delta(output_tokens, previous_decide_output_tokens);
                let cached_input_delta =
                    option_delta(cached_input_tokens, previous_decide_cached_input_tokens);
                let input_spike =
                    is_input_token_spike(input_tokens, previous_decide_input_tokens, input_delta);

                cumulative_decide_input_tokens =
                    cumulative_decide_input_tokens.saturating_add(input_tokens);
                cumulative_decide_output_tokens =
                    cumulative_decide_output_tokens.saturating_add(output_tokens);
                cumulative_decide_cached_input_tokens =
                    cumulative_decide_cached_input_tokens.saturating_add(cached_input_tokens);

                let message_count = messages.len();
                let message_chars: usize = messages.iter().map(|m| m.content.chars().count()).sum();
                let delta_text = input_delta
                    .map(|value| format!("{value:+}"))
                    .unwrap_or_else(|| "n/a".to_string());

                self.emit_progress_internal_with_context(
                    &ctx,
                    &progress_tx,
                    "token_growth",
                    &format!(
                        "Decide token usage step {}/{}: in={} (Δ{}), out={}, cached_in={}",
                        step_count,
                        self.config.max_steps,
                        input_tokens,
                        delta_text,
                        output_tokens,
                        cached_input_tokens
                    ),
                    Some(step_count),
                    Some(self.config.max_steps),
                    Some(serde_json::json!({
                        "step": step_count,
                        "max_steps": self.config.max_steps,
                        "decide_tokens": {
                            "input": input_tokens,
                            "output": output_tokens,
                            "cached_input": cached_input_tokens,
                            "total": input_tokens.saturating_add(output_tokens),
                        },
                        "decide_token_delta": {
                            "input": input_delta,
                            "output": output_delta,
                            "cached_input": cached_input_delta,
                        },
                        "decide_token_cumulative": {
                            "input": cumulative_decide_input_tokens,
                            "output": cumulative_decide_output_tokens,
                            "cached_input": cumulative_decide_cached_input_tokens,
                        },
                        "context_size": {
                            "message_count": message_count,
                            "message_chars": message_chars,
                        },
                        "input_spike": input_spike,
                        "spike_thresholds": {
                            "input_delta_tokens": INPUT_TOKEN_SPIKE_DELTA_THRESHOLD,
                            "input_ratio": INPUT_TOKEN_SPIKE_RATIO_THRESHOLD,
                        }
                    })),
                )
                .await?;

                previous_decide_input_tokens = Some(input_tokens);
                previous_decide_output_tokens = Some(output_tokens);
                previous_decide_cached_input_tokens = Some(cached_input_tokens);
            }

            if decision.tool_calls.is_empty() {
                let reason = "Completion requires a `finished` tool call. Return tool_calls including `finished` when the objective is complete.";
                self.emit_progress_internal(
                    &ctx,
                    &progress_tx,
                    "completion_guard",
                    reason,
                    Some(step_count),
                    Some(self.config.max_steps),
                )
                .await?;
                messages.push(BamlMessage {
                    role: "assistant".to_string(),
                    content: format!(
                        "Completion rejected by worker guard: {reason}\n\
                         Continue and use tools to satisfy this requirement."
                    ),
                });
                continue;
            }

            // Execute tools from decision.tool_calls
            let mut finished_requested = false;
            let mut finished_summary_override: Option<String> = None;
            for (tool_index, tool_call) in decision.tool_calls.iter().enumerate() {
                let tool_name = tool_call_name(tool_call).to_string();
                let tool_reasoning = tool_call_reasoning(tool_call);
                let tool_args_json = tool_call_args_json(tool_call);

                if let AgentToolCall::FinishedToolCall(call) = tool_call {
                    finished_requested = true;
                    if let Some(summary) = &call.tool_args.summary {
                        let trimmed = summary.trim();
                        if !trimmed.is_empty() {
                            finished_summary_override = Some(trimmed.to_string());
                        }
                    }
                    let tool_scope = LlmCallScope {
                        run_id: ctx.run_id.clone(),
                        task_id: Some(ctx.loop_id.clone()),
                        call_id: ctx.call_id.clone(),
                        session_id: None,
                        thread_id: None,
                    };
                    let tool_ctx = self.trace_emitter.start_tool_call(
                        self.worker_port.get_model_role(),
                        &ctx.worker_id,
                        &tool_name,
                        &tool_args_json,
                        tool_reasoning,
                        Some(tool_scope),
                    );
                    let execution = ToolExecution {
                        tool_name: "finished".to_string(),
                        success: true,
                        output: serde_json::json!({
                            "status": "finished",
                            "summary": finished_summary_override,
                        })
                        .to_string(),
                        error: None,
                        execution_time_ms: 0,
                    };
                    self.trace_emitter.complete_tool_call(
                        &tool_ctx,
                        true,
                        &execution.output,
                        execution.error.as_deref(),
                    );
                    messages.push(BamlMessage {
                        role: "assistant".to_string(),
                        content: tool_execution_message(&ctx, &tool_name, &execution, None),
                    });
                    tool_executions.push(execution);
                    continue;
                }

                if self.worker_port.should_defer(&tool_name) {
                    debug!(tool = %tool_name, "Tool deferred");
                    continue;
                }

                self.emit_progress_internal(
                    &ctx,
                    &progress_tx,
                    "executing_tool",
                    &format!("Executing tool: {}", tool_name),
                    Some(step_count),
                    Some(self.config.max_steps),
                )
                .await?;

                let tool_scope = LlmCallScope {
                    run_id: ctx.run_id.clone(),
                    task_id: Some(ctx.loop_id.clone()),
                    call_id: ctx.call_id.clone(),
                    session_id: None,
                    thread_id: None,
                };
                let tool_ctx = self.trace_emitter.start_tool_call(
                    self.worker_port.get_model_role(),
                    &ctx.worker_id,
                    &tool_name,
                    &tool_args_json,
                    tool_reasoning,
                    Some(tool_scope),
                );

                let tool_result = self.worker_port.execute_tool_call(&ctx, tool_call).await;

                match tool_result {
                    Ok(execution) => {
                        self.trace_emitter.complete_tool_call(
                            &tool_ctx,
                            execution.success,
                            &execution.output,
                            execution.error.as_deref(),
                        );
                        let artifact_path = match persist_tool_execution_artifact(
                            &ctx.loop_id,
                            step_count,
                            tool_index,
                            &tool_name,
                            &execution,
                        )
                        .await
                        {
                            Ok(path) => Some(path),
                            Err(write_err) => {
                                error!(
                                    tool = %tool_name,
                                    error = %write_err,
                                    "Failed to persist tool output artifact"
                                );
                                None
                            }
                        };
                        messages.push(BamlMessage {
                            role: "assistant".to_string(),
                            content: tool_execution_message(
                                &ctx,
                                &tool_name,
                                &execution,
                                artifact_path.as_deref(),
                            ),
                        });
                        tool_executions.push(execution);
                    }
                    Err(e) => {
                        error!(tool = %tool_name, error = %e, "Tool execution failed");
                        self.trace_emitter.complete_tool_call(
                            &tool_ctx,
                            false,
                            "",
                            Some(&e.to_string()),
                        );
                        messages.push(BamlMessage {
                            role: "assistant".to_string(),
                            content: format!("Tool {} failed: {}", tool_name, e),
                        });
                    }
                }
            }

            if finished_requested {
                if let Err(reason) =
                    self.worker_port
                        .validate_terminal_decision(&ctx, &decision, &tool_executions)
                {
                    self.emit_progress_internal(
                        &ctx,
                        &progress_tx,
                        "completion_guard",
                        &reason,
                        Some(step_count),
                        Some(self.config.max_steps),
                    )
                    .await?;
                    messages.push(BamlMessage {
                        role: "assistant".to_string(),
                        content: format!(
                            "Completion rejected by worker guard: {reason}\n\
                             Continue and use tools to satisfy this requirement."
                        ),
                    });
                    continue;
                }

                final_summary = if decision.message.trim().is_empty() {
                    finished_summary_override.unwrap_or_else(|| "Objective finished.".to_string())
                } else {
                    decision.message.clone()
                };
                completion_reason = "Model called finished tool".to_string();
                objective_status = ObjectiveStatus::Complete;
                loop_state = AgentLoopState::Complete;
                break;
            }
        }

        // If we hit max steps without completing, mark as incomplete
        if loop_state == AgentLoopState::Running {
            objective_status = ObjectiveStatus::Incomplete;
            completion_reason = format!("Reached max steps ({})", self.config.max_steps);
            // Set a fallback summary when no final response was produced
            if final_summary.is_empty() {
                final_summary = format!(
                    "Reached maximum steps without completion. Executed {} tool calls.",
                    tool_executions.len()
                );
            }
        }

        // Build and emit WorkerTurnReport
        let worker_report = if self.config.emit_worker_report {
            let report = self.worker_port.build_worker_report(
                &ctx,
                &final_summary,
                objective_status != ObjectiveStatus::Blocked,
            );

            self.worker_port
                .emit_worker_report(&ctx, report.clone())
                .await?;
            Some(report)
        } else {
            None
        };

        // Emit completed/failed event
        if objective_status == ObjectiveStatus::Blocked {
            self.emit_failed(&ctx, &completion_reason).await?;
        } else {
            self.emit_completed(&ctx, &final_summary).await?;
        }

        info!(
            loop_id = %loop_id,
            steps = step_count,
            status = ?objective_status,
            "Agentic loop completed"
        );

        Ok(AgentResult {
            summary: final_summary,
            success: objective_status != ObjectiveStatus::Blocked,
            objective_status,
            completion_reason,
            model_used: Some(model_used),
            steps_taken: step_count,
            tool_executions,
            worker_report,
            metadata: None,
        })
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Call BAML Decide function to get tool calls or a final message
    async fn decide(
        &self,
        client_registry: &ClientRegistry,
        messages: &[BamlMessage],
        ctx: &ExecutionContext,
        system_context: &str,
    ) -> Result<(AgentDecision, Option<LlmTokenUsage>), HarnessError> {
        let tools_description = self.worker_port.get_tool_description();

        let message_payload: Vec<serde_json::Value> = messages
            .iter()
            .map(|message| {
                serde_json::json!({
                    "role": message.role,
                    "content": message.content,
                })
            })
            .collect();
        let input = serde_json::json!({
            "objective": ctx.objective,
            "step_number": ctx.step_number,
            "max_steps": ctx.max_steps,
            "messages": message_payload,
            "tools_description": tools_description,
        });
        let input_summary = format!(
            "{} messages, step {}/{}",
            messages.len(),
            ctx.step_number,
            ctx.max_steps
        );

        let model_used = &ctx.model_used;
        let provider: Option<&str> = None;

        let trace_ctx = self.trace_emitter.start_call(
            self.worker_port.get_model_role(),
            "Decide",
            &ctx.worker_id,
            model_used,
            provider,
            &system_context,
            &input,
            &input_summary,
            Some(LlmCallScope {
                run_id: ctx.run_id.clone(),
                task_id: Some(ctx.loop_id.clone()),
                call_id: ctx.call_id.clone(),
                session_id: None,
                thread_id: None,
            }),
        );

        let collector = new_collector("agent_harness.decide");
        let result = B
            .Decide
            .with_client_registry(client_registry)
            .with_collector(&collector)
            .call(messages, system_context, &tools_description)
            .await;
        let usage = token_usage_from_collector(&collector);

        match &result {
            Ok(decision) => {
                let tool_calls: Vec<serde_json::Value> = decision
                    .tool_calls
                    .iter()
                    .map(|tool_call| {
                        serde_json::json!({
                            "tool_name": tool_call_name(tool_call),
                            "reasoning": tool_call_reasoning(tool_call),
                            "tool_args": tool_call_args_json(tool_call),
                        })
                    })
                    .collect();
                let output = serde_json::json!({
                    "tool_calls_count": decision.tool_calls.len(),
                    "tool_calls": tool_calls,
                    "message": decision.message,
                });
                let output_summary = if decision.tool_calls.is_empty() {
                    "final_message"
                } else {
                    "tool_calls"
                };
                self.trace_emitter.complete_call_with_usage(
                    &trace_ctx,
                    model_used,
                    provider,
                    &output,
                    output_summary,
                    usage.clone(),
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call_with_usage(
                    &trace_ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                    usage.clone(),
                );
            }
        }

        result
            .map(|decision| (decision, usage))
            .map_err(|e| HarnessError::Decision(e.to_string()))
    }

    async fn emit_started(&self, ctx: &ExecutionContext) -> Result<(), HarnessError> {
        self.worker_port
            .emit_progress(
                ctx,
                AgentProgress {
                    phase: "started".to_string(),
                    message: format!(
                        "{} agent started objective execution",
                        self.worker_port.get_model_role()
                    ),
                    step_index: Some(0),
                    step_total: Some(ctx.max_steps),
                    model_used: Some(ctx.model_used.clone()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    context: Some(serde_json::json!({
                        "objective": ctx.objective,
                    })),
                },
            )
            .await
    }

    async fn emit_completed(
        &self,
        ctx: &ExecutionContext,
        summary: &str,
    ) -> Result<(), HarnessError> {
        self.worker_port
            .emit_progress(
                ctx,
                AgentProgress {
                    phase: "completed".to_string(),
                    message: summary.to_string(),
                    step_index: Some(ctx.step_number),
                    step_total: Some(ctx.max_steps),
                    model_used: Some(ctx.model_used.clone()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    context: None,
                },
            )
            .await
    }

    async fn emit_failed(&self, ctx: &ExecutionContext, error: &str) -> Result<(), HarnessError> {
        self.worker_port
            .emit_progress(
                ctx,
                AgentProgress {
                    phase: "failed".to_string(),
                    message: error.to_string(),
                    step_index: Some(ctx.step_number),
                    step_total: Some(ctx.max_steps),
                    model_used: Some(ctx.model_used.clone()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    context: None,
                },
            )
            .await
    }

    async fn emit_progress_internal(
        &self,
        ctx: &ExecutionContext,
        progress_tx: &Option<mpsc::UnboundedSender<AgentProgress>>,
        phase: &str,
        message: &str,
        step_index: Option<usize>,
        step_total: Option<usize>,
    ) -> Result<(), HarnessError> {
        self.emit_progress_internal_with_context(
            ctx,
            progress_tx,
            phase,
            message,
            step_index,
            step_total,
            None,
        )
        .await
    }

    async fn emit_progress_internal_with_context(
        &self,
        ctx: &ExecutionContext,
        progress_tx: &Option<mpsc::UnboundedSender<AgentProgress>>,
        phase: &str,
        message: &str,
        step_index: Option<usize>,
        step_total: Option<usize>,
        context: Option<serde_json::Value>,
    ) -> Result<(), HarnessError> {
        let progress = AgentProgress {
            phase: phase.to_string(),
            message: message.to_string(),
            step_index,
            step_total,
            model_used: Some(ctx.model_used.clone()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            context,
        };

        // Send to optional external channel
        if let Some(tx) = progress_tx {
            let _ = tx.send(progress.clone());
        }

        // Emit via adapter
        self.worker_port.emit_progress(ctx, progress).await
    }
}

// ============================================================================
// EventEmitter Adapter Helper
// ============================================================================

/// A helper adapter that emits events to the EventStore
pub struct EventStoreEmitter {
    event_store: ractor::ActorRef<EventStoreMsg>,
    worker_id: String,
    user_id: String,
}

impl EventStoreEmitter {
    pub fn new(
        event_store: ractor::ActorRef<EventStoreMsg>,
        worker_id: String,
        user_id: String,
    ) -> Self {
        Self {
            event_store,
            worker_id,
            user_id,
        }
    }

    pub fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.worker_id.clone(),
            user_id: self.user_id.clone(),
        };
        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    pub fn emit_worker_progress(
        &self,
        task_id: &str,
        phase: &str,
        message: &str,
        model_used: Option<&str>,
    ) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "phase": phase,
            "message": message,
            "model_used": model_used,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.progress", payload);
    }

    pub fn emit_worker_started(&self, task_id: &str, objective: &str, model: &str) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "status": "started",
            "phase": "agent_loop",
            "objective": objective,
            "model_used": model,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.started", payload);
    }

    pub fn emit_worker_completed(&self, task_id: &str, summary: &str) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "status": "completed",
            "phase": "agent_loop",
            "summary": summary,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.completed", payload);
    }

    pub fn emit_worker_failed(&self, task_id: &str, error: &str) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "status": "failed",
            "phase": "agent_loop",
            "error": error,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.failed", payload);
    }

    pub fn emit_worker_finding(
        &self,
        task_id: &str,
        finding_id: &str,
        claim: &str,
        confidence: f64,
        evidence_refs: &[String],
    ) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "phase": "finding",
            "finding_id": finding_id,
            "claim": claim,
            "confidence": confidence,
            "evidence_refs": evidence_refs,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.finding", payload);
    }

    pub fn emit_worker_learning(
        &self,
        task_id: &str,
        learning_id: &str,
        insight: &str,
        confidence: f64,
    ) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.worker_id,
            "phase": "learning",
            "learning_id": learning_id,
            "insight": insight,
            "confidence": confidence,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.learning", payload);
    }
}

// ============================================================================
// Default Adapter Implementation
// ============================================================================

/// A default adapter implementation that can be used as a base
pub struct DefaultAdapter {
    model_role: String,
    tool_description: String,
    event_emitter: Option<EventStoreEmitter>,
}

impl DefaultAdapter {
    pub fn new(model_role: impl Into<String>, tool_description: impl Into<String>) -> Self {
        Self {
            model_role: model_role.into(),
            tool_description: tool_description.into(),
            event_emitter: None,
        }
    }

    pub fn with_event_store(
        mut self,
        event_store: ractor::ActorRef<EventStoreMsg>,
        worker_id: String,
        user_id: String,
    ) -> Self {
        self.event_emitter = Some(EventStoreEmitter::new(event_store, worker_id, user_id));
        self
    }
}

#[async_trait]
impl WorkerPort for DefaultAdapter {
    fn get_model_role(&self) -> &str {
        &self.model_role
    }

    fn get_tool_description(&self) -> String {
        self.tool_description.clone()
    }

    fn get_system_context(&self, _ctx: &ExecutionContext) -> String {
        format!("You are a {} agent.", self.model_role)
    }

    async fn execute_tool_call(
        &self,
        _ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        // Default implementation - subclasses should override
        Ok(ToolExecution {
            tool_name: tool_call_name(tool_call).to_string(),
            success: false,
            output: String::new(),
            error: Some("Tool not implemented in default adapter".to_string()),
            execution_time_ms: 0,
        })
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        if let Some(emitter) = &self.event_emitter {
            let payload = serde_json::json!({
                "task_id": ctx.loop_id,
                "worker_id": ctx.worker_id,
                "report": report,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            emitter.emit_event("worker.report.received", payload);
        }
        Ok(())
    }

    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        if let Some(emitter) = &self.event_emitter {
            emitter.emit_worker_progress(
                &ctx.loop_id,
                &progress.phase,
                &progress.message,
                progress.model_used.as_deref(),
            );
        }
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_config_default() {
        let config = HarnessConfig::default();
        assert_eq!(config.timeout_budget_ms, 30_000);
        assert_eq!(config.max_steps, 100);
        assert!(config.emit_progress);
        assert!(config.emit_worker_report);
    }

    #[test]
    fn test_objective_status_serialization() {
        let complete = ObjectiveStatus::Complete;
        let json = serde_json::to_string(&complete).unwrap();
        assert_eq!(json, "\"complete\"");

        let blocked = ObjectiveStatus::Blocked;
        let json = serde_json::to_string(&blocked).unwrap();
        assert_eq!(json, "\"blocked\"");
    }

    #[test]
    fn test_agent_loop_state_transitions() {
        // Test that the simplified state machine has the expected states
        let states = vec![
            AgentLoopState::Running,
            AgentLoopState::Complete,
            AgentLoopState::Blocked,
        ];

        // Verify all states are distinct
        let mut unique = std::collections::HashSet::new();
        for state in states {
            assert!(unique.insert(std::mem::discriminant(&state)));
        }
    }

    #[test]
    fn test_tool_output_relative_path_stable() {
        let path = tool_output_relative_path("loop_123", 2, 0, "web/search");
        assert_eq!(
            path,
            "agent_harness/tool_outputs/loop_123/step-02-tool-01-web_search.json"
        );
    }

    #[test]
    fn test_adaptive_tool_output_echo_chars_bounds() {
        assert_eq!(
            adaptive_tool_output_echo_chars(1, 6),
            MIN_TOOL_OUTPUT_ECHO_CHARS
        );
        assert_eq!(adaptive_tool_output_echo_chars(4, 6), 3_000);
        assert_eq!(
            adaptive_tool_output_echo_chars(6, 6),
            MAX_TOOL_OUTPUT_ECHO_CHARS
        );
    }
}
