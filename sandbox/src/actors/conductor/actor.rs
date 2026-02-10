//! ConductorActor - orchestrates task execution across worker actors
//!
//! The ConductorActor is responsible for:
//! - Receiving task execution requests
//! - Routing tasks using typed worker plans
//! - Managing task lifecycle (Queued -> Running -> WaitingWorker -> Completed/Failed)
//! - Writing reports to sandbox-safe paths
//! - Emitting events for observability

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use shared_types::{
    ConductorExecuteRequest, ConductorOutputMode, ConductorTaskState, ConductorTaskStatus,
    ConductorToastPayload, ConductorToastTone, ConductorWorkerStep, ConductorWorkerType,
};
use std::path::Path;

use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg, WorkerOutput},
    router::WorkerRouter,
    state::ConductorState as TaskState,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::{ResearcherMsg, ResearcherResult};
use crate::actors::terminal::{
    TerminalAgentResult, TerminalBashToolRequest, TerminalError, TerminalMsg,
};

/// ConductorActor - main orchestration actor
#[derive(Debug, Default)]
pub struct ConductorActor;

/// Arguments for spawning ConductorActor
#[derive(Debug, Clone)]
pub struct ConductorArguments {
    /// Event store actor reference for persistence
    pub event_store: ActorRef<EventStoreMsg>,
    /// Optional researcher actor for delegation
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    /// Optional terminal actor for delegation
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
}

/// Internal state for ConductorActor
pub struct ConductorState {
    /// Task state management
    tasks: TaskState,
    /// Event store reference
    event_store: ActorRef<EventStoreMsg>,
    /// Researcher actor reference
    researcher_actor: Option<ActorRef<ResearcherMsg>>,
    /// Terminal actor reference
    terminal_actor: Option<ActorRef<TerminalMsg>>,
    /// Worker router
    router: WorkerRouter,
}

#[async_trait]
impl Actor for ConductorActor {
    type Msg = ConductorMsg;
    type State = ConductorState;
    type Arguments = ConductorArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor starting");

        Ok(ConductorState {
            tasks: TaskState::new(),
            event_store: args.event_store,
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
            router: WorkerRouter::new(),
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor started successfully");
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ConductorMsg::ExecuteTask { request, reply } => {
                let result = self.handle_execute_task(myself, state, request).await;
                let _ = reply.send(result);
            }
            ConductorMsg::GetTaskState { task_id, reply } => {
                let task_state = state.tasks.get_task(&task_id).cloned();
                let _ = reply.send(task_state);
            }
            ConductorMsg::WorkerResult { task_id, result } => {
                self.handle_worker_result(state, task_id, result).await?;
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor stopped");
        Ok(())
    }
}

impl ConductorActor {
    /// Handle ExecuteTask message
    async fn handle_execute_task(
        &self,
        myself: ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        request: ConductorExecuteRequest,
    ) -> Result<ConductorTaskState, ConductorError> {
        let task_id = ulid::Ulid::new().to_string();
        let correlation_id = request
            .correlation_id
            .clone()
            .unwrap_or_else(|| task_id.clone());

        tracing::info!(
            task_id = %task_id,
            correlation_id = %correlation_id,
            objective = %request.objective,
            "Executing new conductor task"
        );

        let now = chrono::Utc::now();
        let task_state = ConductorTaskState {
            task_id: task_id.clone(),
            status: ConductorTaskStatus::Queued,
            objective: request.objective.clone(),
            desktop_id: request.desktop_id.clone(),
            output_mode: request.output_mode,
            correlation_id: correlation_id.clone(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };
        state.tasks.insert_task(task_state)?;

        events::emit_task_started(
            &state.event_store,
            &task_id,
            &correlation_id,
            &request.objective,
            &request.desktop_id,
        )
        .await;

        let routing = state
            .router
            .route(&request)
            .map_err(ConductorError::InvalidRequest)?;
        let (plan, routing_reason) = if routing.plan.is_empty() {
            let plan = build_default_plan(state, &request.objective)?;
            (
                plan.clone(),
                format!(
                    "No explicit worker_plan provided; conductor policy selected {}",
                    describe_worker_plan(&plan)
                ),
            )
        } else {
            (routing.plan, routing.reason)
        };
        validate_worker_availability(&plan, state)?;

        state.tasks.transition_to_running(&task_id)?;
        events::emit_task_progress(
            &state.event_store,
            &task_id,
            &correlation_id,
            "running",
            "routing",
            Some(serde_json::json!({
                "worker_plan": plan,
                "reason": routing_reason,
            })),
        )
        .await;

        state.tasks.transition_to_waiting_worker(&task_id)?;
        events::emit_task_progress(
            &state.event_store,
            &task_id,
            &correlation_id,
            "waiting_worker",
            "worker_execution",
            None,
        )
        .await;

        let event_store = state.event_store.clone();
        let researcher = state.researcher_actor.clone();
        let terminal = state.terminal_actor.clone();
        let objective = request.objective.clone();
        let myself_clone = myself.clone();
        let task_id_clone = task_id.clone();
        let correlation_id_clone = correlation_id.clone();

        tokio::spawn(async move {
            let result = execute_worker_plan(
                &event_store,
                &task_id_clone,
                &correlation_id_clone,
                plan,
                objective,
                researcher,
                terminal,
            )
            .await;

            let _ = myself_clone.send_message(ConductorMsg::WorkerResult {
                task_id: task_id_clone,
                result,
            });
        });

        Ok(state
            .tasks
            .get_task(&task_id)
            .cloned()
            .expect("task must exist after insertion"))
    }

    /// Handle WorkerResult message
    async fn handle_worker_result(
        &self,
        state: &mut ConductorState,
        task_id: String,
        result: Result<WorkerOutput, ConductorError>,
    ) -> Result<(), ActorProcessingErr> {
        let correlation_id = state
            .tasks
            .get_task(&task_id)
            .map(|t| t.correlation_id.clone())
            .unwrap_or_else(|| task_id.clone());

        match result {
            Ok(output) => {
                tracing::info!(task_id = %task_id, "Worker plan completed, writing report");

                let report_path = match self.write_report(&task_id, &output.report_content).await {
                    Ok(path) => path,
                    Err(e) => {
                        let shared_error: shared_types::ConductorError = e.clone().into();
                        state
                            .tasks
                            .transition_to_failed(&task_id, shared_error.clone())?;
                        events::emit_task_failed(
                            &state.event_store,
                            &task_id,
                            &correlation_id,
                            &shared_error.code,
                            &shared_error.message,
                            shared_error.failure_kind,
                        )
                        .await;
                        return Ok(());
                    }
                };

                let requested_mode = state
                    .tasks
                    .get_task(&task_id)
                    .map(|task| task.output_mode)
                    .unwrap_or(ConductorOutputMode::MarkdownReportToWriter);
                let selected_mode = resolve_output_mode(requested_mode, &output);
                let toast = build_completion_toast(selected_mode, &output, &report_path);

                state.tasks.transition_to_completed(
                    &task_id,
                    selected_mode,
                    report_path.clone(),
                    toast.clone(),
                )?;

                let writer_props = if selected_mode == ConductorOutputMode::MarkdownReportToWriter {
                    Some(build_writer_window_props(&report_path))
                } else {
                    None
                };
                events::emit_task_completed(
                    &state.event_store,
                    &task_id,
                    &correlation_id,
                    selected_mode,
                    &report_path,
                    writer_props.as_ref(),
                    toast.as_ref(),
                )
                .await;
            }
            Err(e) => {
                tracing::error!(task_id = %task_id, error = %e, "Worker plan failed");
                let shared_error: shared_types::ConductorError = e.clone().into();

                state
                    .tasks
                    .transition_to_failed(&task_id, shared_error.clone())?;
                events::emit_task_failed(
                    &state.event_store,
                    &task_id,
                    &correlation_id,
                    &shared_error.code,
                    &shared_error.message,
                    shared_error.failure_kind,
                )
                .await;
            }
        }

        Ok(())
    }

    /// Write report to sandbox-safe path
    async fn write_report(&self, task_id: &str, content: &str) -> Result<String, ConductorError> {
        let sandbox = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reports_dir = sandbox.join("reports");

        if let Err(e) = tokio::fs::create_dir_all(&reports_dir).await {
            return Err(ConductorError::ReportWriteFailed(format!(
                "Failed to create reports directory: {e}"
            )));
        }

        if task_id.contains('/') || task_id.contains('\\') || task_id.contains("..") {
            return Err(ConductorError::InvalidRequest(
                "Invalid task_id: contains path separators".to_string(),
            ));
        }

        let report_path = reports_dir.join(format!("{task_id}.md"));
        if let Err(e) = tokio::fs::write(&report_path, content).await {
            return Err(ConductorError::ReportWriteFailed(format!(
                "Failed to write report: {e}"
            )));
        }

        Ok(format!("reports/{task_id}.md"))
    }
}

fn validate_worker_availability(
    plan: &[ConductorWorkerStep],
    state: &ConductorState,
) -> Result<(), ConductorError> {
    for step in plan {
        match step.worker_type {
            ConductorWorkerType::Researcher if state.researcher_actor.is_none() => {
                return Err(ConductorError::InvalidRequest(
                    "ResearcherActor not available".to_string(),
                ));
            }
            ConductorWorkerType::Terminal if state.terminal_actor.is_none() => {
                return Err(ConductorError::InvalidRequest(
                    "TerminalActor not available".to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn worker_type_name(worker_type: ConductorWorkerType) -> &'static str {
    match worker_type {
        ConductorWorkerType::Researcher => "researcher",
        ConductorWorkerType::Terminal => "terminal",
    }
}

fn describe_worker_plan(plan: &[ConductorWorkerStep]) -> String {
    let workers: Vec<&str> = plan
        .iter()
        .map(|step| worker_type_name(step.worker_type))
        .collect();
    format!("plan={}", workers.join(" -> "))
}

fn build_default_plan(
    state: &ConductorState,
    objective: &str,
) -> Result<Vec<ConductorWorkerStep>, ConductorError> {
    let has_terminal = state.terminal_actor.is_some();
    let has_researcher = state.researcher_actor.is_some();

    match (has_terminal, has_researcher) {
        (true, true) => Ok(vec![
            ConductorWorkerStep {
                worker_type: ConductorWorkerType::Terminal,
                objective: Some(objective.to_string()),
                terminal_command: None,
                timeout_ms: Some(60_000),
                max_results: None,
                max_steps: Some(4),
            },
            ConductorWorkerStep {
                worker_type: ConductorWorkerType::Researcher,
                objective: Some(objective.to_string()),
                terminal_command: None,
                timeout_ms: Some(60_000),
                max_results: Some(8),
                max_steps: None,
            },
        ]),
        (true, false) => Ok(vec![ConductorWorkerStep {
            worker_type: ConductorWorkerType::Terminal,
            objective: Some(objective.to_string()),
            terminal_command: None,
            timeout_ms: Some(60_000),
            max_results: None,
            max_steps: Some(4),
        }]),
        (false, true) => Ok(vec![ConductorWorkerStep {
            worker_type: ConductorWorkerType::Researcher,
            objective: Some(objective.to_string()),
            terminal_command: None,
            timeout_ms: Some(60_000),
            max_results: Some(8),
            max_steps: None,
        }]),
        (false, false) => Err(ConductorError::InvalidRequest(
            "No worker actors available for Conductor default policy".to_string(),
        )),
    }
}

fn build_writer_window_props(report_path: &str) -> serde_json::Value {
    serde_json::json!({
        "x": 100,
        "y": 100,
        "width": 900,
        "height": 680,
        "path": report_path,
        "preview_mode": true,
    })
}

fn resolve_output_mode(
    requested: ConductorOutputMode,
    output: &WorkerOutput,
) -> ConductorOutputMode {
    match requested {
        ConductorOutputMode::MarkdownReportToWriter => ConductorOutputMode::MarkdownReportToWriter,
        ConductorOutputMode::ToastWithReportLink => ConductorOutputMode::ToastWithReportLink,
        ConductorOutputMode::Auto => {
            if output.report_content.chars().count() <= 900 && output.citations.len() <= 2 {
                ConductorOutputMode::ToastWithReportLink
            } else {
                ConductorOutputMode::MarkdownReportToWriter
            }
        }
    }
}

fn build_completion_toast(
    output_mode: ConductorOutputMode,
    output: &WorkerOutput,
    report_path: &str,
) -> Option<ConductorToastPayload> {
    if output_mode != ConductorOutputMode::ToastWithReportLink {
        return None;
    }

    let summary_line = output
        .report_content
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("```")
        })
        .unwrap_or("Conductor completed.");
    let message = summary_line.chars().take(240).collect::<String>();

    Some(ConductorToastPayload {
        title: "Conductor Answer".to_string(),
        message,
        tone: ConductorToastTone::Success,
        report_path: Some(report_path.to_string()),
    })
}

async fn execute_worker_plan(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    plan: Vec<ConductorWorkerStep>,
    default_objective: String,
    researcher: Option<ActorRef<ResearcherMsg>>,
    terminal: Option<ActorRef<TerminalMsg>>,
) -> Result<WorkerOutput, ConductorError> {
    let mut report_sections: Vec<String> = Vec::new();
    let mut all_citations = Vec::new();

    for (index, step) in plan.iter().enumerate() {
        let objective = step
            .objective
            .clone()
            .unwrap_or_else(|| default_objective.clone());
        let worker_name = worker_type_name(step.worker_type);

        events::emit_task_progress(
            event_store,
            task_id,
            correlation_id,
            "running",
            "worker_step",
            Some(serde_json::json!({
                "step_index": index + 1,
                "step_total": plan.len(),
                "worker_type": worker_name,
            })),
        )
        .await;
        events::emit_worker_call(
            event_store,
            task_id,
            correlation_id,
            worker_name,
            &objective,
        )
        .await;

        match step.worker_type {
            ConductorWorkerType::Researcher => {
                let researcher_ref = researcher.as_ref().ok_or_else(|| {
                    ConductorError::InvalidRequest("ResearcherActor not available".to_string())
                })?;
                let research_result = call_researcher(
                    researcher_ref,
                    objective.clone(),
                    step.timeout_ms,
                    step.max_results,
                    step.max_steps,
                )
                .await;

                match research_result {
                    Ok(result) => {
                        all_citations.extend(result.citations.clone());
                        report_sections.push(format!(
                            "## Step {}: Research\n\n{}\n",
                            index + 1,
                            result.summary
                        ));
                        events::emit_worker_result(
                            event_store,
                            task_id,
                            correlation_id,
                            worker_name,
                            true,
                            &format!(
                                "Research complete with {} citations",
                                result.citations.len()
                            ),
                        )
                        .await;
                    }
                    Err(err) => {
                        events::emit_worker_result(
                            event_store,
                            task_id,
                            correlation_id,
                            worker_name,
                            false,
                            &err.to_string(),
                        )
                        .await;
                        return Err(err);
                    }
                }
            }
            ConductorWorkerType::Terminal => {
                let terminal_ref = terminal.as_ref().ok_or_else(|| {
                    ConductorError::InvalidRequest("TerminalActor not available".to_string())
                })?;
                let terminal_result = call_terminal(
                    terminal_ref,
                    objective.clone(),
                    step.terminal_command.clone(),
                    step.timeout_ms,
                    step.max_steps,
                )
                .await;

                match terminal_result {
                    Ok(result) => {
                        if !result.success {
                            let err = ConductorError::WorkerFailed(result.summary.clone());
                            events::emit_worker_result(
                                event_store,
                                task_id,
                                correlation_id,
                                worker_name,
                                false,
                                &result.summary,
                            )
                            .await;
                            return Err(err);
                        }

                        report_sections.push(format!(
                            "## Step {}: Terminal\n\n{}\n\n```text\n{}\n```\n",
                            index + 1,
                            objective,
                            result.summary
                        ));
                        events::emit_worker_result(
                            event_store,
                            task_id,
                            correlation_id,
                            worker_name,
                            true,
                            &format!(
                                "Terminal step completed ({} commands)",
                                result.executed_commands.len()
                            ),
                        )
                        .await;
                    }
                    Err(err) => {
                        events::emit_worker_result(
                            event_store,
                            task_id,
                            correlation_id,
                            worker_name,
                            false,
                            &err.to_string(),
                        )
                        .await;
                        return Err(err);
                    }
                }
            }
        }
    }

    let mut report_content = format!(
        "# Conductor Report\n\n## Objective\n\n{}\n\n",
        default_objective
    );
    for section in report_sections {
        report_content.push_str(&section);
        report_content.push('\n');
    }
    if !all_citations.is_empty() {
        report_content.push_str("## Citations\n\n");
        for citation in &all_citations {
            report_content.push_str(&format!(
                "- [{}]({}) - {}\n",
                citation.title, citation.url, citation.provider
            ));
        }
    }

    Ok(WorkerOutput {
        report_content,
        citations: all_citations,
    })
}

/// Call the ResearcherActor
async fn call_researcher(
    researcher: &ActorRef<ResearcherMsg>,
    objective: String,
    timeout_ms: Option<u64>,
    max_results: Option<u32>,
    max_rounds: Option<u8>,
) -> Result<ResearcherResult, ConductorError> {
    use ractor::call;

    call!(researcher, |reply| ResearcherMsg::RunAgenticTask {
        objective,
        timeout_ms,
        max_results,
        max_rounds,
        model_override: None,
        progress_tx: None,
        reply,
    })
    .map_err(|e| ConductorError::WorkerFailed(format!("Failed to call researcher actor: {e}")))?
    .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
}

/// Call the TerminalActor
async fn call_terminal(
    terminal: &ActorRef<TerminalMsg>,
    objective: String,
    terminal_command: Option<String>,
    timeout_ms: Option<u64>,
    max_steps: Option<u8>,
) -> Result<TerminalAgentResult, ConductorError> {
    use ractor::call;

    match call!(terminal, |reply| TerminalMsg::Start { reply }) {
        Ok(Ok(())) | Ok(Err(TerminalError::AlreadyRunning)) => {}
        Ok(Err(e)) => {
            return Err(ConductorError::WorkerFailed(format!(
                "Failed to start terminal: {e}"
            )))
        }
        Err(e) => {
            return Err(ConductorError::WorkerFailed(format!(
                "Failed to call terminal start: {e}"
            )))
        }
    }

    if let Some(cmd) = terminal_command {
        call!(terminal, |reply| TerminalMsg::RunBashTool {
            request: TerminalBashToolRequest {
                cmd,
                timeout_ms,
                model_override: None,
                reasoning: Some("conductor typed worker plan terminal step".to_string()),
            },
            progress_tx: None,
            reply,
        })
        .map_err(|e| {
            ConductorError::WorkerFailed(format!("Failed to call terminal bash tool: {e}"))
        })?
        .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
    } else {
        call!(terminal, |reply| TerminalMsg::RunAgenticTask {
            objective,
            timeout_ms,
            max_steps,
            model_override: None,
            progress_tx: None,
            reply,
        })
        .map_err(|e| {
            ConductorError::WorkerFailed(format!("Failed to call terminal agent task: {e}"))
        })?
        .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::{call, Actor};
    use shared_types::{ConductorOutputMode, FailureKind};

    async fn setup_test_conductor() -> (ActorRef<ConductorMsg>, ActorRef<EventStoreMsg>) {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let args = ConductorArguments {
            event_store: store_ref.clone(),
            researcher_actor: None,
            terminal_actor: None,
        };

        let (conductor_ref, _conductor_handle) =
            Actor::spawn(None, ConductorActor, args).await.unwrap();

        (conductor_ref, store_ref)
    }

    #[tokio::test]
    async fn test_conductor_actor_spawn() {
        let (conductor_ref, store_ref) = setup_test_conductor().await;
        let actor_id = conductor_ref.get_id();
        assert!(!actor_id.to_string().is_empty());
        conductor_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_execute_task_message_missing_researcher() {
        let (conductor_ref, store_ref) = setup_test_conductor().await;

        let request = ConductorExecuteRequest {
            objective: "Research Rust async patterns".to_string(),
            desktop_id: "test-desktop-001".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            worker_plan: None,
            hints: None,
            correlation_id: Some("test-correlation-001".to_string()),
        };

        let result: Result<Result<ConductorTaskState, ConductorError>, _> =
            call!(conductor_ref, |reply| ConductorMsg::ExecuteTask {
                request,
                reply,
            });

        assert!(result.is_ok());
        let task_result = result.unwrap();
        assert!(task_result.is_err());
        match task_result.unwrap_err() {
            ConductorError::InvalidRequest(msg) => {
                assert!(msg.contains("No worker actors available"));
            }
            other => panic!("Expected InvalidRequest, got {:?}", other),
        }

        conductor_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_get_task_state_nonexistent() {
        let (conductor_ref, store_ref) = setup_test_conductor().await;

        let state_result: Result<Option<ConductorTaskState>, _> =
            call!(conductor_ref, |reply| ConductorMsg::GetTaskState {
                task_id: "non-existent-task-id".to_string(),
                reply,
            });

        assert!(state_result.is_ok());
        assert!(state_result.unwrap().is_none());

        conductor_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_task_lifecycle_transitions() {
        use crate::actors::conductor::state::ConductorState;

        let mut state = ConductorState::new();
        let task_id = "test-task-001".to_string();
        let now = chrono::Utc::now();
        let task_state = ConductorTaskState {
            task_id: task_id.clone(),
            status: ConductorTaskStatus::Queued,
            objective: "Test objective".to_string(),
            desktop_id: "test-desktop".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            correlation_id: "test-correlation".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };

        assert!(state.insert_task(task_state).is_ok());
        assert_eq!(
            state.get_task(&task_id).expect("task exists").status,
            ConductorTaskStatus::Queued
        );

        assert!(state.transition_to_running(&task_id).is_ok());
        assert_eq!(
            state.get_task(&task_id).expect("task exists").status,
            ConductorTaskStatus::Running
        );

        assert!(state.transition_to_waiting_worker(&task_id).is_ok());
        assert_eq!(
            state.get_task(&task_id).expect("task exists").status,
            ConductorTaskStatus::WaitingWorker
        );

        assert!(state
            .transition_to_completed(
                &task_id,
                ConductorOutputMode::MarkdownReportToWriter,
                "reports/test-task-001.md".to_string(),
                None,
            )
            .is_ok());
        let completed = state.get_task(&task_id).expect("task exists");
        assert_eq!(completed.status, ConductorTaskStatus::Completed);
        assert!(completed.completed_at.is_some());

        let task_id_2 = "test-task-002".to_string();
        let task_state_2 = ConductorTaskState {
            task_id: task_id_2.clone(),
            status: ConductorTaskStatus::Queued,
            objective: "Test objective 2".to_string(),
            desktop_id: "test-desktop".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            correlation_id: "test-correlation-2".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };
        state.insert_task(task_state_2).unwrap();
        state.transition_to_running(&task_id_2).unwrap();

        let error = shared_types::ConductorError {
            code: "WORKER_FAILED".to_string(),
            message: "Worker timed out".to_string(),
            failure_kind: Some(FailureKind::Timeout),
        };
        assert!(state.transition_to_failed(&task_id_2, error).is_ok());
        let failed = state.get_task(&task_id_2).expect("task exists");
        assert_eq!(failed.status, ConductorTaskStatus::Failed);
        assert!(failed.error.is_some());
    }

    #[tokio::test]
    async fn test_duplicate_task_rejection() {
        use crate::actors::conductor::state::ConductorState;

        let mut state = ConductorState::new();
        let now = chrono::Utc::now();
        let task_state = ConductorTaskState {
            task_id: "duplicate-task".to_string(),
            status: ConductorTaskStatus::Queued,
            objective: "Test objective".to_string(),
            desktop_id: "test-desktop".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            correlation_id: "test-correlation".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };

        assert!(state.insert_task(task_state.clone()).is_ok());
        let duplicate = state.insert_task(task_state);
        assert!(duplicate.is_err());
        match duplicate.unwrap_err() {
            ConductorError::DuplicateTask(id) => assert_eq!(id, "duplicate-task"),
            other => panic!("Expected DuplicateTask, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_worker_result_message_send() {
        let (conductor_ref, store_ref) = setup_test_conductor().await;
        let worker_output = WorkerOutput {
            report_content: "# Test Report\n\nThis is a test.".to_string(),
            citations: vec![],
        };

        let result = conductor_ref.send_message(ConductorMsg::WorkerResult {
            task_id: "non-existent-task".to_string(),
            result: Ok(worker_output),
        });
        assert!(result.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        conductor_ref.stop(None);
        store_ref.stop(None);
    }

    #[test]
    fn test_resolve_output_mode_auto_prefers_toast_for_brief_output() {
        let output = WorkerOutput {
            report_content: "Short answer line.\n".to_string(),
            citations: vec![],
        };
        assert_eq!(
            resolve_output_mode(ConductorOutputMode::Auto, &output),
            ConductorOutputMode::ToastWithReportLink
        );
    }

    #[test]
    fn test_resolve_output_mode_auto_prefers_report_for_long_output() {
        let output = WorkerOutput {
            report_content: "x".repeat(1600),
            citations: vec![],
        };
        assert_eq!(
            resolve_output_mode(ConductorOutputMode::Auto, &output),
            ConductorOutputMode::MarkdownReportToWriter
        );
    }
}
