//! Unified Agent Harness - Shared loop framework for agentic workers
//!
//! This module provides a generic harness for building agentic workers with:
//! - Model resolution via ModelRegistry
//! - BAML-based planning and synthesis
//! - Structured event emission (started, progress, finding, learning, completed/failed)
//! - WorkerTurnReport generation at completion
//!
//! ## Architecture
//!
//! The harness uses a state machine loop:
//! RECEIVE_OBJECTIVE -> PLAN_STEP -> EXECUTE_TOOLS -> OBSERVE_RESULTS -> SYNTHESIZE or PLAN_STEP
//!
//! ## Usage
//!
//! Implement the `AgentAdapter` trait for your specific worker type, then use
//! `AgentHarness::run()` to execute the agentic loop.
//!
//! ```rust,ignore
//! pub struct MyAdapter;
//!
//! impl AgentAdapter for MyAdapter {
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
use crate::baml_client::types::{AgentPlan, AgentToolCall, Message as BamlMessage, ToolResult as BamlToolResult};
use crate::baml_client::{ClientRegistry, B};

// Re-export shared types for convenience
pub use shared_types::{
    WorkerArtifact, WorkerEscalation, WorkerEscalationKind, WorkerEscalationUrgency, WorkerFinding,
    WorkerLearning, WorkerTurnReport, WorkerTurnStatus,
};

// ============================================================================
// Core Types
// ============================================================================

/// State machine states for the agentic loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLoopState {
    /// Initial state - objective received, preparing to plan
    ReceiveObjective,
    /// Planning next action using LLM
    PlanStep,
    /// Executing tool calls
    ExecuteTools,
    /// Observing tool results
    ObserveResults,
    /// Synthesizing final response
    Synthesize,
    /// Loop completed
    Completed,
    /// Loop failed/blocked
    Failed,
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
    /// Whether to emit findings and learnings
    pub emit_structured_signals: bool,
    /// Whether to generate WorkerTurnReport at completion
    pub emit_worker_report: bool,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            timeout_budget_ms: 30_000,
            max_steps: 6,
            emit_progress: true,
            emit_structured_signals: true,
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
    #[error("Planning failed: {0}")]
    Planning(String),
    #[error("Tool execution failed: {0}")]
    ToolExecution(String),
    #[error("Synthesis failed: {0}")]
    Synthesis(String),
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
// AgentAdapter Trait
// ============================================================================

/// Trait for adapting the harness to specific worker types
///
/// Implement this trait to customize the harness behavior for your worker.
/// The adapter provides worker-specific logic while the harness handles
/// the common loop control flow.
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Returns the model role identifier for this worker type
    /// (e.g., "terminal", "researcher", "watcher")
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

    /// Emit a finding (structured insight)
    ///
    /// Called when the worker discovers something noteworthy.
    async fn emit_finding(
        &self,
        ctx: &ExecutionContext,
        finding: WorkerFinding,
    ) -> Result<(), HarnessError>;

    /// Emit a learning (adjustment to understanding)
    ///
    /// Called when the worker learns something that changes its approach.
    async fn emit_learning(
        &self,
        ctx: &ExecutionContext,
        learning: WorkerLearning,
    ) -> Result<(), HarnessError>;

    /// Build the final WorkerTurnReport
    ///
    /// The harness provides default findings/learnings, but the adapter
    /// can customize the report generation.
    fn build_worker_report(
        &self,
        ctx: &ExecutionContext,
        findings: Vec<WorkerFinding>,
        learnings: Vec<WorkerLearning>,
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
            findings,
            learnings,
            escalations: Vec::new(),
            artifacts: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

// ============================================================================
// AgentHarness
// ============================================================================

/// The unified agent harness
///
/// This struct provides the core agentic loop implementation.
/// It is generic over the `AgentAdapter` trait to allow customization
/// for different worker types.
pub struct AgentHarness<A: AgentAdapter> {
    adapter: A,
    model_registry: ModelRegistry,
    config: HarnessConfig,
}

impl<A: AgentAdapter> AgentHarness<A> {
    /// Create a new agent harness with the given adapter and model registry
    pub fn new(adapter: A, model_registry: ModelRegistry) -> Self {
        Self {
            adapter,
            model_registry,
            config: HarnessConfig::default(),
        }
    }

    /// Create a new agent harness with custom configuration
    pub fn with_config(adapter: A, model_registry: ModelRegistry, config: HarnessConfig) -> Self {
        Self {
            adapter,
            model_registry,
            config,
        }
    }

    /// Run the agentic loop with the given objective
    ///
    /// This is the main entry point for executing an agentic task.
    /// The loop will:
    /// 1. Resolve the model to use
    /// 2. Plan actions using the LLM
    /// 3. Execute tools via the adapter
    /// 4. Observe results and iterate
    /// 5. Synthesize a final response
    /// 6. Emit a WorkerTurnReport
    pub async fn run(
        &self,
        worker_id: String,
        user_id: String,
        objective: String,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<AgentProgress>>,
    ) -> Result<AgentResult, HarnessError> {
        let loop_id = ulid::Ulid::new().to_string();

        // Resolve model
        let resolved_model = self
            .model_registry
            .resolve_for_role(
                self.adapter.get_model_role(),
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
            role = %self.adapter.get_model_role(),
            model = %model_used,
            "Starting agentic loop"
        );

        // Create execution context
        let ctx = ExecutionContext {
            loop_id: loop_id.clone(),
            worker_id: worker_id.clone(),
            user_id: user_id.clone(),
            step_number: 0,
            max_steps: self.config.max_steps,
            model_used: model_used.clone(),
            objective: objective.clone(),
        };

        // Emit started event
        self.emit_started(&ctx).await?;

        // Initialize loop state
        let mut messages = vec![BamlMessage {
            role: "user".to_string(),
            content: format!("[{}]\n{}", chrono::Utc::now().to_rfc3339(), objective),
        }];
        let mut tool_executions: Vec<ToolExecution> = Vec::new();
        let mut findings: Vec<WorkerFinding> = Vec::new();
        let mut learnings: Vec<WorkerLearning> = Vec::new();
        let mut loop_state = AgentLoopState::ReceiveObjective;
        let mut step_count = 0;
        let mut final_summary = String::new();
        let mut completion_reason = String::new();
        let mut objective_status = ObjectiveStatus::Incomplete;

        // Get client registry for BAML calls
        let client_registry = self
            .model_registry
            .create_runtime_client_registry_for_model(&model_used)
            .map_err(HarnessError::from)?;

        // Main loop
        while step_count < self.config.max_steps {
            step_count += 1;

            match loop_state {
                AgentLoopState::ReceiveObjective | AgentLoopState::PlanStep => {
                    self.emit_progress_internal(
                        &ctx,
                        &progress_tx,
                        "planning",
                        &format!("Planning step {}/{}", step_count, self.config.max_steps),
                        Some(step_count),
                        Some(self.config.max_steps),
                    )
                    .await?;

                    // Call BAML PlanAction
                    let plan = match self.plan_step(&client_registry, &messages, &ctx).await {
                        Ok(plan) => plan,
                        Err(e) => {
                            error!(error = %e, "Planning failed");
                            objective_status = ObjectiveStatus::Blocked;
                            completion_reason = format!("Planning failed: {e}");
                            loop_state = AgentLoopState::Failed;
                            break;
                        }
                    };

                    // Check for final response (no tool calls)
                    if plan.tool_calls.is_empty() {
                        if let Some(response) = plan.final_response {
                            final_summary = response;
                            objective_status = ObjectiveStatus::Complete;
                            completion_reason =
                                "Agent produced final response without tool calls".to_string();
                            loop_state = AgentLoopState::Synthesize;
                            break;
                        }
                    }

                    // Store reasoning as a learning
                    if !plan.thinking.is_empty() {
                        let learning = WorkerLearning {
                            learning_id: ulid::Ulid::new().to_string(),
                            insight: plan.thinking.clone(),
                            confidence: 0.8,
                            supports: Vec::new(),
                            changes_plan: Some(true),
                        };
                        self.adapter.emit_learning(&ctx, learning.clone()).await?;
                        learnings.push(learning);
                    }

                    // Execute tools
                    for tool_call in &plan.tool_calls {
                        if self.adapter.should_defer(&tool_call.tool_name) {
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

                        let tool_result = self
                            .adapter
                            .execute_tool_call(&ctx, tool_call)
                            .await;

                        match tool_result {
                            Ok(execution) => {
                                // Create finding from successful execution
                                if execution.success {
                                    let finding = WorkerFinding {
                                        finding_id: ulid::Ulid::new().to_string(),
                                        claim: format!(
                                            "Tool {} executed successfully",
                                            tool_call.tool_name
                                        ),
                                        confidence: 0.9,
                                        evidence_refs: vec![execution.output.clone()],
                                        novel: Some(true),
                                    };
                                    self.adapter.emit_finding(&ctx, finding.clone()).await?;
                                    findings.push(finding);
                                }

                                // Add to messages for next planning round
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
                                messages.push(BamlMessage {
                                    role: "assistant".to_string(),
                                    content: format!("Tool {} failed: {}", tool_call.tool_name, e),
                                });
                            }
                        }
                    }

                    loop_state = AgentLoopState::ObserveResults;
                }

                AgentLoopState::ExecuteTools => {
                    // Tool execution happens within PlanStep arm
                    // This state exists for tracking but tools are executed immediately after planning
                    loop_state = AgentLoopState::ObserveResults;
                }

                AgentLoopState::ObserveResults => {
                    self.emit_progress_internal(
                        &ctx,
                        &progress_tx,
                        "observing",
                        "Observing results and planning next step",
                        Some(step_count),
                        Some(self.config.max_steps),
                    )
                    .await?;

                    // Check if we should continue or synthesize
                    if step_count >= self.config.max_steps {
                        loop_state = AgentLoopState::Synthesize;
                    } else {
                        loop_state = AgentLoopState::PlanStep;
                    }
                }

                AgentLoopState::Synthesize => {
                    self.emit_progress_internal(
                        &ctx,
                        &progress_tx,
                        "synthesizing",
                        "Synthesizing final response",
                        Some(step_count),
                        Some(self.config.max_steps),
                    )
                    .await?;

                    // Call BAML SynthesizeResponse
                    final_summary = self
                        .synthesize(&client_registry, &objective, &tool_executions, &ctx)
                        .await?;

                    objective_status = if tool_executions.iter().all(|t| t.success) {
                        ObjectiveStatus::Complete
                    } else if tool_executions.iter().any(|t| t.success) {
                        ObjectiveStatus::Incomplete
                    } else {
                        ObjectiveStatus::Blocked
                    };

                    completion_reason = format!("Completed after {} steps", step_count);
                    loop_state = AgentLoopState::Completed;
                    break;
                }

                AgentLoopState::Completed | AgentLoopState::Failed => {
                    break;
                }
            }
        }

        // If we hit max steps without completing, synthesize anyway
        if loop_state != AgentLoopState::Completed && loop_state != AgentLoopState::Failed {
            final_summary = self
                .synthesize(&client_registry, &objective, &tool_executions, &ctx)
                .await?;
            objective_status = ObjectiveStatus::Incomplete;
            completion_reason = format!("Reached max steps ({})", self.config.max_steps);
        }

        // Build and emit WorkerTurnReport
        let worker_report = if self.config.emit_worker_report {
            let report = self.adapter.build_worker_report(
                &ctx,
                findings,
                learnings,
                &final_summary,
                objective_status != ObjectiveStatus::Blocked,
            );

            self.adapter
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

    async fn plan_step(
        &self,
        client_registry: &ClientRegistry,
        messages: &[BamlMessage],
        ctx: &ExecutionContext,
    ) -> Result<AgentPlan, HarnessError> {
        let system_context = self.adapter.get_system_context(ctx);
        let tools_description = self.adapter.get_tool_description();

        B
            .PlanAction
            .with_client_registry(client_registry)
            .call(messages, &system_context, &tools_description)
            .await
            .map_err(|e| HarnessError::Planning(e.to_string()))
    }

    async fn synthesize(
        &self,
        client_registry: &ClientRegistry,
        objective: &str,
        tool_results: &[ToolExecution],
        ctx: &ExecutionContext,
    ) -> Result<String, HarnessError> {
        let baml_results: Vec<BamlToolResult> = tool_results
            .iter()
            .map(|t| BamlToolResult {
                tool_name: t.tool_name.clone(),
                success: t.success,
                output: t.output.clone(),
                error: t.error.clone(),
            })
            .collect();

        let conversation_context = format!(
            "Generated at UTC {}. Executed {} tools in {} steps.",
            chrono::Utc::now().to_rfc3339(),
            tool_results.len(),
            ctx.step_number
        );

        B.SynthesizeResponse
            .with_client_registry(client_registry)
            .call(objective, &baml_results, &conversation_context)
            .await
            .map_err(|e| HarnessError::Synthesis(e.to_string()))
    }

    async fn emit_started(&self, ctx: &ExecutionContext) -> Result<(), HarnessError> {
        self.adapter
            .emit_progress(
                ctx,
                AgentProgress {
                    phase: "started".to_string(),
                    message: format!(
                        "{} agent started objective execution",
                        self.adapter.get_model_role()
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
        self.adapter
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
        self.adapter
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
        self.adapter.emit_progress(ctx, progress).await
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
impl AgentAdapter for DefaultAdapter {
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

    async fn emit_finding(
        &self,
        ctx: &ExecutionContext,
        finding: WorkerFinding,
    ) -> Result<(), HarnessError> {
        if let Some(emitter) = &self.event_emitter {
            emitter.emit_worker_finding(
                &ctx.loop_id,
                &finding.finding_id,
                &finding.claim,
                finding.confidence,
                &finding.evidence_refs,
            );
        }
        Ok(())
    }

    async fn emit_learning(
        &self,
        ctx: &ExecutionContext,
        learning: WorkerLearning,
    ) -> Result<(), HarnessError> {
        if let Some(emitter) = &self.event_emitter {
            emitter.emit_worker_learning(
                &ctx.loop_id,
                &learning.learning_id,
                &learning.insight,
                learning.confidence,
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
        assert!(config.emit_structured_signals);
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
        // Test that the state machine has the expected states
        let states = vec![
            AgentLoopState::ReceiveObjective,
            AgentLoopState::PlanStep,
            AgentLoopState::ExecuteTools,
            AgentLoopState::ObserveResults,
            AgentLoopState::Synthesize,
            AgentLoopState::Completed,
            AgentLoopState::Failed,
        ];

        // Verify all states are distinct
        let mut unique = std::collections::HashSet::new();
        for state in states {
            assert!(unique.insert(std::mem::discriminant(&state)));
        }
    }
}
