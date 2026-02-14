//! Unified Agent Harness - Shared loop framework for agentic workers
//!
//! This module provides a generic harness for building agentic workers with:
//! - Model resolution via ModelRegistry
//! - BAML-based decision loop (simplified: Decide -> Execute -> loop/return)
//! - Structured event emission (started, progress, completed/failed)
//! - WorkerTurnReport generation at completion
//!
//! ## Architecture
//!
//! The harness uses a simplified loop:
//! DECIDE -> EXECUTE -> (loop or return)
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

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::{ModelConfigError, ModelRegistry, ModelResolutionContext};
use crate::baml_client::types::{Action, AgentDecision, AgentToolCall, Message as BamlMessage};
use crate::baml_client::{ClientRegistry, B};
use crate::observability::llm_trace::{LlmCallScope, LlmTraceEmitter};

// Re-export shared types for convenience
pub use shared_types::{
    WorkerArtifact, WorkerEscalation, WorkerEscalationKind, WorkerEscalationUrgency,
    WorkerTurnReport, WorkerTurnStatus,
};

// ============================================================================
// Core Types
// ============================================================================

// Action and AgentDecision are now imported from crate::baml_client::types

/// State machine states for the agentic loop (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLoopState {
    /// Still deciding/executing
    Running,
    /// Got Complete action with summary
    Complete,
    /// Got Block action
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
            max_steps: 6,
            emit_progress: true,
            emit_worker_report: true,
        }
    }
}

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

    /// Validate whether a terminal decision (Complete/Block) is allowed.
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
    /// 2. Call BAML Decide to get action
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
            .resolve_for_role(
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
        let mut messages = vec![BamlMessage {
            role: "user".to_string(),
            content: format!("[{}]\n{}", chrono::Utc::now().to_rfc3339(), objective),
        }];
        let mut tool_executions: Vec<ToolExecution> = Vec::new();
        let mut step_count = 0;
        let mut final_summary = String::new();
        let mut completion_reason = String::new();
        let mut objective_status = ObjectiveStatus::Incomplete;
        let mut loop_state = AgentLoopState::Running;

        // Get client registry for BAML calls
        let client_registry = self
            .model_registry
            .create_runtime_client_registry_for_model(&model_used)
            .map_err(HarnessError::from)?;

        // Main loop: Decide -> Execute -> (loop or return)
        while step_count < self.config.max_steps && loop_state == AgentLoopState::Running {
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
            let decision = match self.decide(&client_registry, &messages, &ctx).await {
                Ok(decision) => decision,
                Err(e) => {
                    error!(error = %e, "Decision failed");
                    objective_status = ObjectiveStatus::Blocked;
                    completion_reason = format!("Decision failed: {e}");
                    loop_state = AgentLoopState::Blocked;
                    break;
                }
            };

            match decision.action {
                Action::ToolCall => {
                    // Execute tools from decision.tool_calls
                    for tool_call in &decision.tool_calls {
                        if self.worker_port.should_defer(&tool_call.tool_name) {
                            debug!(tool = %tool_call.tool_name, "Tool deferred");
                            continue;
                        }

                        self.emit_progress_internal(
                            &ctx,
                            &progress_tx,
                            "executing_tool",
                            &format!("Executing tool: {}", tool_call.tool_name),
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
                        let tool_args_json = serde_json::json!({
                            "debug": format!("{:?}", tool_call.tool_args),
                        });
                        let tool_ctx = self.trace_emitter.start_tool_call(
                            self.worker_port.get_model_role(),
                            &ctx.worker_id,
                            &tool_call.tool_name,
                            &tool_args_json,
                            tool_call.reasoning.as_deref(),
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
                                // Add to messages for next decision round
                                messages.push(BamlMessage {
                                    role: "assistant".to_string(),
                                    content: format!(
                                        "Executed {}:\nOutput: {}\nSuccess: {}",
                                        tool_call.tool_name, execution.output, execution.success
                                    ),
                                });
                                tool_executions.push(execution);
                            }
                            Err(e) => {
                                error!(tool = %tool_call.tool_name, error = %e, "Tool execution failed");
                                self.trace_emitter.complete_tool_call(
                                    &tool_ctx,
                                    false,
                                    "",
                                    Some(&e.to_string()),
                                );
                                messages.push(BamlMessage {
                                    role: "assistant".to_string(),
                                    content: format!("Tool {} failed: {}", tool_call.tool_name, e),
                                });
                            }
                        }
                    }
                    // Continue loop for next decision
                }
                Action::Complete => {
                    if let Err(reason) = self.worker_port.validate_terminal_decision(
                        &ctx,
                        &decision,
                        &tool_executions,
                    ) {
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
                    final_summary = decision.summary.unwrap_or_default();
                    objective_status = ObjectiveStatus::Complete;
                    completion_reason = decision.reason.unwrap_or_default();
                    loop_state = AgentLoopState::Complete;
                    break;
                }
                Action::Block => {
                    if let Err(reason) = self.worker_port.validate_terminal_decision(
                        &ctx,
                        &decision,
                        &tool_executions,
                    ) {
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
                                "Block rejected by worker guard: {reason}\n\
                                 Continue and use tools to satisfy this requirement."
                            ),
                        });
                        continue;
                    }
                    let reason = decision
                        .reason
                        .unwrap_or_else(|| "Blocked without reason".to_string());
                    final_summary = reason.clone();
                    objective_status = ObjectiveStatus::Blocked;
                    completion_reason = reason;
                    loop_state = AgentLoopState::Blocked;
                    break;
                }
            }
        }

        // If we hit max steps without completing, mark as incomplete
        if loop_state == AgentLoopState::Running {
            objective_status = ObjectiveStatus::Incomplete;
            completion_reason = format!("Reached max steps ({})", self.config.max_steps);
            // Use last reasoning as summary if available
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

    /// Call BAML Decide function to get the next action
    async fn decide(
        &self,
        client_registry: &ClientRegistry,
        messages: &[BamlMessage],
        ctx: &ExecutionContext,
    ) -> Result<AgentDecision, HarnessError> {
        let system_context = self.worker_port.get_system_context(ctx);
        let tools_description = self.worker_port.get_tool_description();

        let input = serde_json::json!({
            "message_count": messages.len(),
            "tools_description_length": tools_description.len(),
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

        let result = B
            .Decide
            .with_client_registry(client_registry)
            .call(messages, &system_context, &tools_description)
            .await;

        match &result {
            Ok(decision) => {
                let output = serde_json::json!({
                    "action": format!("{:?}", decision.action),
                    "tool_calls_count": decision.tool_calls.len(),
                });
                self.trace_emitter.complete_call(
                    &trace_ctx,
                    model_used,
                    provider,
                    &output,
                    &format!("{:?}", decision.action),
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call(
                    &trace_ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                );
            }
        }

        result.map_err(|e| HarnessError::Decision(e.to_string()))
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
        let progress = AgentProgress {
            phase: phase.to_string(),
            message: message.to_string(),
            step_index,
            step_total,
            model_used: Some(ctx.model_used.clone()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            context: None,
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

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            "You are a {} agent. Current step {}/{}\nTimestamp: {}",
            self.model_role,
            ctx.step_number,
            ctx.max_steps,
            chrono::Utc::now().to_rfc3339()
        )
    }

    async fn execute_tool_call(
        &self,
        _ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        // Default implementation - subclasses should override
        Ok(ToolExecution {
            tool_name: tool_call.tool_name.clone(),
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
        assert_eq!(config.max_steps, 6);
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
    fn test_action_variants() {
        // Test Action enum variants (BAML-generated simple enums)
        let tool_call = Action::ToolCall;
        assert!(matches!(tool_call, Action::ToolCall));

        let complete = Action::Complete;
        assert!(matches!(complete, Action::Complete));

        let block = Action::Block;
        assert!(matches!(block, Action::Block));
    }
}
