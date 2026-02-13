//! Conductor API endpoints
//!
//! All orchestration flows through ConductorActor.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::actors::conductor::{file_tools, ConductorError as ActorConductorError, ConductorMsg};
use crate::api::websocket::{broadcast_event, WsMessage};
use crate::api::ApiState;
use shared_types::{
    ConductorError, ConductorExecuteRequest, ConductorExecuteResponse, ConductorTaskState,
    ConductorTaskStatus, EventImportance, WriterWindowProps,
};

/// Conductor error codes for machine-readable error responses
#[derive(Debug, Clone)]
pub enum ConductorErrorCode {
    InvalidRequest,
    ActorNotAvailable,
    TaskNotFound,
    InternalError,
}

impl ConductorErrorCode {
    fn as_str(&self) -> &'static str {
        match self {
            ConductorErrorCode::InvalidRequest => "INVALID_REQUEST",
            ConductorErrorCode::ActorNotAvailable => "ACTOR_NOT_AVAILABLE",
            ConductorErrorCode::TaskNotFound => "TASK_NOT_FOUND",
            ConductorErrorCode::InternalError => "INTERNAL_ERROR",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            ConductorErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            ConductorErrorCode::ActorNotAvailable => StatusCode::SERVICE_UNAVAILABLE,
            ConductorErrorCode::TaskNotFound => StatusCode::NOT_FOUND,
            ConductorErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Serialize)]
struct TaskStatusErrorResponse {
    task_id: String,
    error: ConductorError,
}

fn conductor_error(
    code: ConductorErrorCode,
    message: impl Into<String>,
    failure_kind: Option<shared_types::FailureKind>,
) -> ConductorError {
    ConductorError {
        code: code.as_str().to_string(),
        message: message.into(),
        failure_kind,
    }
}

fn status_code_for_task(status: ConductorTaskStatus) -> StatusCode {
    match status {
        ConductorTaskStatus::Queued
        | ConductorTaskStatus::Running
        | ConductorTaskStatus::WaitingWorker => StatusCode::ACCEPTED,
        ConductorTaskStatus::Completed => StatusCode::OK,
        ConductorTaskStatus::Failed => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn writer_window_props_for_report(report_path: &str, run_id: Option<&str>) -> WriterWindowProps {
    WriterWindowProps {
        x: 100,
        y: 100,
        width: 900,
        height: 680,
        path: report_path.to_string(),
        preview_mode: true,
        run_id: run_id.map(ToString::to_string),
    }
}

fn writer_window_props_for_run_document(document_path: &str, run_id: &str) -> WriterWindowProps {
    WriterWindowProps {
        x: 100,
        y: 100,
        width: 900,
        height: 680,
        path: document_path.to_string(),
        preview_mode: false,
        run_id: Some(run_id.to_string()),
    }
}

fn task_state_to_execute_response(task: ConductorTaskState) -> ConductorExecuteResponse {
    let run_id = task.task_id.clone();
    let (document_path, writer_window_props) = match task.status {
        ConductorTaskStatus::Queued
        | ConductorTaskStatus::Running
        | ConductorTaskStatus::WaitingWorker => {
            let path = file_tools::get_run_document_path(&run_id);
            let props = writer_window_props_for_run_document(&path, &run_id);
            (Some(path), Some(props))
        }
        ConductorTaskStatus::Completed => {
            let report_path = task.report_path.clone();
            let props =
                if task.output_mode == shared_types::ConductorOutputMode::MarkdownReportToWriter {
                    report_path
                        .as_ref()
                        .map(|path| writer_window_props_for_report(path, Some(&run_id)))
                } else {
                    None
                };
            (report_path, props)
        }
        ConductorTaskStatus::Failed => (task.report_path.clone(), None),
    };

    ConductorExecuteResponse {
        task_id: task.task_id.clone(),
        run_id: Some(run_id),
        status: task.status,
        document_path,
        writer_window_props,
        toast: task.toast,
        correlation_id: task.correlation_id,
        error: task.error,
    }
}

fn map_actor_error(err: ActorConductorError) -> (StatusCode, ConductorError) {
    match err {
        ActorConductorError::ActorUnavailable(msg) => (
            ConductorErrorCode::ActorNotAvailable.status_code(),
            conductor_error(
                ConductorErrorCode::ActorNotAvailable,
                msg,
                Some(shared_types::FailureKind::Unknown),
            ),
        ),
        ActorConductorError::InvalidRequest(msg) => (
            ConductorErrorCode::InvalidRequest.status_code(),
            conductor_error(
                ConductorErrorCode::InvalidRequest,
                msg,
                Some(shared_types::FailureKind::Validation),
            ),
        ),
        ActorConductorError::NotFound(msg) => (
            ConductorErrorCode::TaskNotFound.status_code(),
            conductor_error(
                ConductorErrorCode::TaskNotFound,
                msg,
                Some(shared_types::FailureKind::Unknown),
            ),
        ),
        ActorConductorError::WorkerFailed(msg)
        | ActorConductorError::WorkerBlocked(msg)
        | ActorConductorError::ReportWriteFailed(msg)
        | ActorConductorError::DuplicateTask(msg)
        | ActorConductorError::PolicyError(msg)
        | ActorConductorError::FileError(msg) => (
            ConductorErrorCode::InternalError.status_code(),
            conductor_error(
                ConductorErrorCode::InternalError,
                msg,
                Some(shared_types::FailureKind::Unknown),
            ),
        ),
    }
}

/// Broadcast a telemetry event to all WebSocket clients for a desktop
async fn broadcast_telemetry_event(
    ws_sessions: &crate::api::websocket::WsSessions,
    desktop_id: &str,
    event_type: &str,
    capability: &str,
    phase: &str,
    importance: EventImportance,
    data: serde_json::Value,
) {
    let importance_str = match importance {
        EventImportance::High => "high",
        EventImportance::Normal => "normal",
        EventImportance::Low => "low",
    };

    broadcast_event(
        ws_sessions,
        desktop_id,
        WsMessage::Telemetry {
            event_type: event_type.to_string(),
            capability: capability.to_string(),
            phase: phase.to_string(),
            importance: importance_str.to_string(),
            data,
        },
    )
    .await;
}

/// Broadcast a document update event to all WebSocket clients for a desktop
pub async fn broadcast_document_update(
    ws_sessions: &crate::api::websocket::WsSessions,
    desktop_id: &str,
    run_id: &str,
    document_path: &str,
    content_excerpt: &str,
    timestamp: &str,
) {
    broadcast_event(
        ws_sessions,
        desktop_id,
        WsMessage::DocumentUpdate {
            run_id: run_id.to_string(),
            document_path: document_path.to_string(),
            content_excerpt: content_excerpt.to_string(),
            timestamp: timestamp.to_string(),
        },
    )
    .await;
}

/// POST /conductor/execute - Submit a new Conductor task
pub async fn execute_task(
    State(state): State<ApiState>,
    Json(request): Json<ConductorExecuteRequest>,
) -> impl IntoResponse {
    if request.objective.trim().is_empty() {
        let error = conductor_error(
            ConductorErrorCode::InvalidRequest,
            "Objective cannot be empty",
            Some(shared_types::FailureKind::Validation),
        );
        let body = Json(ConductorExecuteResponse {
            task_id: String::new(),
            run_id: None,
            status: ConductorTaskStatus::Failed,
            document_path: None,
            writer_window_props: None,
            toast: None,
            correlation_id: request.correlation_id.unwrap_or_default(),
            error: Some(error),
        });
        return (StatusCode::BAD_REQUEST, body).into_response();
    }

    if request.desktop_id.trim().is_empty() {
        let error = conductor_error(
            ConductorErrorCode::InvalidRequest,
            "Desktop ID cannot be empty",
            Some(shared_types::FailureKind::Validation),
        );
        let body = Json(ConductorExecuteResponse {
            task_id: String::new(),
            run_id: None,
            status: ConductorTaskStatus::Failed,
            document_path: None,
            writer_window_props: None,
            toast: None,
            correlation_id: request.correlation_id.unwrap_or_default(),
            error: Some(error),
        });
        return (StatusCode::BAD_REQUEST, body).into_response();
    }

    let conductor = match state.app_state.ensure_conductor().await {
        Ok(actor) => actor,
        Err(e) => {
            let error = conductor_error(
                ConductorErrorCode::ActorNotAvailable,
                format!("Failed to ensure conductor actor: {e}"),
                Some(shared_types::FailureKind::Unknown),
            );
            let body = Json(ConductorExecuteResponse {
                task_id: String::new(),
                run_id: None,
                status: ConductorTaskStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
                correlation_id: request.correlation_id.unwrap_or_default(),
                error: Some(error),
            });
            return (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
        }
    };

    // Broadcast task started telemetry event
    broadcast_telemetry_event(
        &state.ws_sessions,
        &request.desktop_id,
        "conductor.task.started",
        "conductor",
        "initialization",
        EventImportance::Normal,
        serde_json::json!({
            "task_id": &request.objective,
            "objective": &request.objective,
        }),
    )
    .await;

    let desktop_id = request.desktop_id.clone();
    let ws_sessions = state.ws_sessions.clone();
    let result: Result<Result<ConductorTaskState, ActorConductorError>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::ExecuteTask {
            request,
            reply,
        });

    match result {
        Ok(Ok(task_state)) => {
            // Broadcast completion telemetry event
            let (event_type, phase, importance) = match task_state.status {
                ConductorTaskStatus::Completed => (
                    "conductor.task.completed",
                    "completion",
                    EventImportance::Normal,
                ),
                ConductorTaskStatus::Failed => {
                    ("conductor.task.failed", "failure", EventImportance::High)
                }
                _ => ("conductor.task.progress", "running", EventImportance::Low),
            };
            broadcast_telemetry_event(
                &ws_sessions,
                &desktop_id,
                event_type,
                "conductor",
                phase,
                importance,
                serde_json::json!({
                    "task_id": &task_state.task_id,
                    "status": format!("{:?}", task_state.status),
                }),
            )
            .await;
            let status = status_code_for_task(task_state.status);
            let response = task_state_to_execute_response(task_state);
            (status, Json(response)).into_response()
        }
        Ok(Err(actor_err)) => {
            let (status, error) = map_actor_error(actor_err);
            let body = Json(ConductorExecuteResponse {
                task_id: String::new(),
                run_id: None,
                status: ConductorTaskStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
                correlation_id: String::new(),
                error: Some(error),
            });
            (status, body).into_response()
        }
        Err(e) => {
            let error = conductor_error(
                ConductorErrorCode::ActorNotAvailable,
                format!("Conductor RPC failed: {e}"),
                Some(shared_types::FailureKind::Unknown),
            );
            let body = Json(ConductorExecuteResponse {
                task_id: String::new(),
                run_id: None,
                status: ConductorTaskStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
                correlation_id: String::new(),
                error: Some(error),
            });
            (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
        }
    }
}

/// GET /conductor/tasks/:task_id - Get current task state
pub async fn get_task_status(
    State(state): State<ApiState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    if task_id.trim().is_empty() {
        let body = Json(TaskStatusErrorResponse {
            task_id,
            error: conductor_error(
                ConductorErrorCode::InvalidRequest,
                "Task ID cannot be empty",
                Some(shared_types::FailureKind::Validation),
            ),
        });
        return (StatusCode::BAD_REQUEST, body).into_response();
    }

    let conductor = match state.app_state.ensure_conductor().await {
        Ok(actor) => actor,
        Err(e) => {
            let body = Json(TaskStatusErrorResponse {
                task_id,
                error: conductor_error(
                    ConductorErrorCode::ActorNotAvailable,
                    format!("Failed to ensure conductor actor: {e}"),
                    Some(shared_types::FailureKind::Unknown),
                ),
            });
            return (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
        }
    };

    let result: Result<Option<ConductorTaskState>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::GetTaskState {
            task_id: task_id.clone(),
            reply,
        });

    match result {
        Ok(Some(task_state)) => (StatusCode::OK, Json(task_state)).into_response(),
        Ok(None) => {
            let body = Json(TaskStatusErrorResponse {
                task_id,
                error: conductor_error(
                    ConductorErrorCode::TaskNotFound,
                    "Task not found",
                    Some(shared_types::FailureKind::Unknown),
                ),
            });
            (StatusCode::NOT_FOUND, body).into_response()
        }
        Err(e) => {
            let body = Json(TaskStatusErrorResponse {
                task_id,
                error: conductor_error(
                    ConductorErrorCode::ActorNotAvailable,
                    format!("Conductor RPC failed: {e}"),
                    Some(shared_types::FailureKind::Unknown),
                ),
            });
            (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use shared_types::ConductorOutputMode;

    #[test]
    fn test_status_code_for_task() {
        assert_eq!(
            status_code_for_task(ConductorTaskStatus::Queued),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_task(ConductorTaskStatus::Running),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_task(ConductorTaskStatus::WaitingWorker),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_task(ConductorTaskStatus::Completed),
            StatusCode::OK
        );
        assert_eq!(
            status_code_for_task(ConductorTaskStatus::Failed),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_writer_props_contains_preview_mode() {
        let props = writer_window_props_for_report("reports/test.md", None);
        assert_eq!(props.path, "reports/test.md");
        assert!(props.preview_mode);
    }

    #[test]
    fn test_task_state_to_execute_response_targets_live_document_for_accepted_status() {
        let now = Utc::now();
        let task = ConductorTaskState {
            task_id: "run_123".to_string(),
            status: ConductorTaskStatus::WaitingWorker,
            objective: "test objective".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::Auto,
            correlation_id: "corr-1".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };

        let response = task_state_to_execute_response(task);

        assert_eq!(
            response.document_path.as_deref(),
            Some("conductor/runs/run_123/draft.md")
        );
        let props = response
            .writer_window_props
            .expect("accepted response must include writer props");
        assert_eq!(props.path, "conductor/runs/run_123/draft.md");
        assert!(!props.preview_mode);
        assert_eq!(props.run_id.as_deref(), Some("run_123"));
    }

    #[test]
    fn test_map_actor_unavailable_maps_to_service_unavailable() {
        let (status, error) = map_actor_error(ActorConductorError::ActorUnavailable(
            "workers unavailable".to_string(),
        ));
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code, "ACTOR_NOT_AVAILABLE");
        assert!(error.message.contains("workers unavailable"));
    }
}
