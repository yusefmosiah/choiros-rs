//! Conductor API endpoints
//!
//! All orchestration flows through ConductorActor.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::actors::conductor::{ConductorError as ActorConductorError, ConductorMsg};
use crate::api::websocket::{broadcast_event, WsMessage};
use crate::api::ApiState;
use shared_types::{
    ConductorError, ConductorExecuteRequest, ConductorExecuteResponse, ConductorRunState,
    ConductorRunStatus, ConductorRunStatusResponse, EventImportance, WriterWindowProps,
};

/// Conductor error codes for machine-readable error responses
#[derive(Debug, Clone)]
pub enum ConductorErrorCode {
    InvalidRequest,
    ActorNotAvailable,
    RunNotFound,
    InternalError,
}

impl ConductorErrorCode {
    fn as_str(&self) -> &'static str {
        match self {
            ConductorErrorCode::InvalidRequest => "INVALID_REQUEST",
            ConductorErrorCode::ActorNotAvailable => "ACTOR_NOT_AVAILABLE",
            ConductorErrorCode::RunNotFound => "RUN_NOT_FOUND",
            ConductorErrorCode::InternalError => "INTERNAL_ERROR",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            ConductorErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            ConductorErrorCode::ActorNotAvailable => StatusCode::SERVICE_UNAVAILABLE,
            ConductorErrorCode::RunNotFound => StatusCode::NOT_FOUND,
            ConductorErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Serialize)]
struct RunStatusErrorResponse {
    run_id: String,
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

fn status_code_for_run(status: ConductorRunStatus) -> StatusCode {
    match status {
        ConductorRunStatus::Initializing
        | ConductorRunStatus::Running
        | ConductorRunStatus::WaitingForCalls
        | ConductorRunStatus::Completing => StatusCode::ACCEPTED,
        ConductorRunStatus::Completed => StatusCode::OK,
        ConductorRunStatus::Failed | ConductorRunStatus::Blocked => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
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

fn run_error_for_status(status: ConductorRunStatus) -> Option<ConductorError> {
    if status == ConductorRunStatus::Blocked {
        Some(ConductorError {
            code: "RUN_BLOCKED".to_string(),
            message: "Run blocked by conductor model gateway".to_string(),
            failure_kind: Some(shared_types::FailureKind::Unknown),
        })
    } else if status == ConductorRunStatus::Failed {
        Some(ConductorError {
            code: "RUN_FAILED".to_string(),
            message: "Run failed".to_string(),
            failure_kind: Some(shared_types::FailureKind::Unknown),
        })
    } else {
        None
    }
}

fn run_state_to_execute_response(run: ConductorRunState) -> ConductorExecuteResponse {
    let run_id = run.run_id.clone();
    let report_path = if run.status == ConductorRunStatus::Completed {
        Some(format!("reports/{run_id}.md"))
    } else {
        None
    };
    let error = run_error_for_status(run.status);
    let (document_path, writer_window_props) = match run.status {
        ConductorRunStatus::Initializing
        | ConductorRunStatus::Running
        | ConductorRunStatus::WaitingForCalls
        | ConductorRunStatus::Completing => {
            let path = run.document_path.clone();
            let props = writer_window_props_for_run_document(&path, &run_id);
            (Some(path), Some(props))
        }
        ConductorRunStatus::Completed => {
            let props =
                if run.output_mode == shared_types::ConductorOutputMode::MarkdownReportToWriter {
                    report_path
                        .as_ref()
                        .map(|path| writer_window_props_for_report(path, Some(&run_id)))
                } else {
                    None
                };
            (report_path, props)
        }
        ConductorRunStatus::Failed | ConductorRunStatus::Blocked => (report_path, None),
    };

    ConductorExecuteResponse {
        run_id,
        status: run.status,
        document_path,
        writer_window_props,
        toast: None,
        error,
    }
}

fn run_state_to_status_response(run: ConductorRunState) -> ConductorRunStatusResponse {
    let report_path = if run.status == ConductorRunStatus::Completed {
        Some(format!("reports/{}.md", run.run_id))
    } else {
        None
    };

    ConductorRunStatusResponse {
        run_id: run.run_id,
        status: run.status,
        objective: run.objective,
        desktop_id: run.desktop_id,
        output_mode: run.output_mode,
        created_at: run.created_at,
        updated_at: run.updated_at,
        completed_at: run.completed_at,
        document_path: run.document_path,
        report_path,
        toast: None,
        error: run_error_for_status(run.status),
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
            ConductorErrorCode::RunNotFound.status_code(),
            conductor_error(
                ConductorErrorCode::RunNotFound,
                msg,
                Some(shared_types::FailureKind::Unknown),
            ),
        ),
        ActorConductorError::WorkerFailed(msg)
        | ActorConductorError::WorkerBlocked(msg)
        | ActorConductorError::ReportWriteFailed(msg)
        | ActorConductorError::DuplicateRun(msg)
        | ActorConductorError::ModelGatewayError(msg)
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
            run_id: String::new(),
            status: ConductorRunStatus::Failed,
            document_path: None,
            writer_window_props: None,
            toast: None,
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
            run_id: String::new(),
            status: ConductorRunStatus::Failed,
            document_path: None,
            writer_window_props: None,
            toast: None,
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
                run_id: String::new(),
                status: ConductorRunStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
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
            "objective": &request.objective,
        }),
    )
    .await;

    let desktop_id = request.desktop_id.clone();
    let ws_sessions = state.ws_sessions.clone();
    let result: Result<Result<ConductorRunState, ActorConductorError>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::ExecuteTask {
            request,
            reply,
        });

    match result {
        Ok(Ok(run_state)) => {
            let run_status = run_state.status;
            // Broadcast completion telemetry event
            let (event_type, phase, importance) = match run_status {
                ConductorRunStatus::Completed => (
                    "conductor.task.completed",
                    "completion",
                    EventImportance::Normal,
                ),
                ConductorRunStatus::Failed | ConductorRunStatus::Blocked => {
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
                    "run_id": &run_state.run_id,
                    "status": format!("{:?}", run_state.status),
                }),
            )
            .await;
            let status = status_code_for_run(run_status);
            let response = run_state_to_execute_response(run_state);
            (status, Json(response)).into_response()
        }
        Ok(Err(actor_err)) => {
            let (status, error) = map_actor_error(actor_err);
            let body = Json(ConductorExecuteResponse {
                run_id: String::new(),
                status: ConductorRunStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
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
                run_id: String::new(),
                status: ConductorRunStatus::Failed,
                document_path: None,
                writer_window_props: None,
                toast: None,
                error: Some(error),
            });
            (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
        }
    }
}

/// GET /conductor/runs/:run_id - Get current run state
pub async fn get_run_status(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    if run_id.trim().is_empty() {
        let body = Json(RunStatusErrorResponse {
            run_id,
            error: conductor_error(
                ConductorErrorCode::InvalidRequest,
                "Run ID cannot be empty",
                Some(shared_types::FailureKind::Validation),
            ),
        });
        return (StatusCode::BAD_REQUEST, body).into_response();
    }

    let conductor = match state.app_state.ensure_conductor().await {
        Ok(actor) => actor,
        Err(e) => {
            let body = Json(RunStatusErrorResponse {
                run_id,
                error: conductor_error(
                    ConductorErrorCode::ActorNotAvailable,
                    format!("Failed to ensure conductor actor: {e}"),
                    Some(shared_types::FailureKind::Unknown),
                ),
            });
            return (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
        }
    };

    let result: Result<Option<ConductorRunState>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::GetRunState {
            run_id: run_id.clone(),
            reply,
        });

    match result {
        Ok(Some(run_state)) => {
            let run_state = run_state_to_status_response(run_state);
            (StatusCode::OK, Json(run_state)).into_response()
        }
        Ok(None) => {
            let body = Json(RunStatusErrorResponse {
                run_id,
                error: conductor_error(
                    ConductorErrorCode::RunNotFound,
                    "Run not found",
                    Some(shared_types::FailureKind::Unknown),
                ),
            });
            (StatusCode::NOT_FOUND, body).into_response()
        }
        Err(e) => {
            let body = Json(RunStatusErrorResponse {
                run_id,
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
    fn test_status_code_for_run() {
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Initializing),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Running),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::WaitingForCalls),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Completing),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Completed),
            StatusCode::OK
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Failed),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_code_for_run(ConductorRunStatus::Blocked),
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
    fn test_run_state_to_execute_response_targets_live_document_for_accepted_status() {
        let now = Utc::now();
        let run = ConductorRunState {
            run_id: "run_123".to_string(),
            status: ConductorRunStatus::WaitingForCalls,
            objective: "test objective".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::Auto,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "conductor/runs/run_123/draft.md".to_string(),
        };

        let response = run_state_to_execute_response(run);
        assert_eq!(response.run_id, "run_123");
        assert_eq!(response.status, ConductorRunStatus::WaitingForCalls);

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
