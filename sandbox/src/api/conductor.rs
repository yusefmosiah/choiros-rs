//! Conductor API endpoints
//!
//! All orchestration flows through ConductorActor.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::actors::conductor::{ConductorError as ActorConductorError, ConductorMsg};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::api::websocket::{broadcast_event, WsMessage};
use crate::api::ApiState;
use shared_types::{
    ConductorError, ConductorExecuteRequest, ConductorExecuteResponse, ConductorRunState,
    ConductorRunStatus, ConductorRunStatusResponse, ConductorToastPayload, ConductorToastTone,
    EventImportance, WriterWindowProps,
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

fn pre_run_telemetry_descriptor() -> (&'static str, &'static str, EventImportance) {
    (
        "conductor.task.requested",
        "initialization",
        EventImportance::Normal,
    )
}

fn run_lifecycle_telemetry_descriptor(
    status: ConductorRunStatus,
) -> (&'static str, &'static str, EventImportance) {
    match status {
        ConductorRunStatus::Initializing
        | ConductorRunStatus::Running
        | ConductorRunStatus::WaitingForCalls
        | ConductorRunStatus::Completing => (
            "conductor.task.started",
            "run_start",
            EventImportance::Normal,
        ),
        ConductorRunStatus::Completed => (
            "conductor.task.completed",
            "completion",
            EventImportance::Normal,
        ),
        ConductorRunStatus::Failed | ConductorRunStatus::Blocked => {
            ("conductor.task.failed", "failure", EventImportance::High)
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

fn should_retry_conductor_rpc(err: &str) -> bool {
    err.contains("Messaging failed to enqueue")
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

fn run_error_from_artifacts(run: &ConductorRunState) -> Option<ConductorError> {
    for artifact in run.artifacts.iter().rev() {
        let Some(metadata) = artifact.metadata.as_ref() else {
            continue;
        };
        let Some(event_type) = metadata.get("event_type").and_then(|value| value.as_str()) else {
            continue;
        };
        if event_type != "conductor.task.failed" {
            continue;
        }

        let Some(payload) = metadata.get("event_payload") else {
            continue;
        };
        let code = payload
            .get("error_code")
            .and_then(|value| value.as_str())
            .unwrap_or("RUN_FAILED")
            .to_string();
        let message = payload
            .get("error_message")
            .and_then(|value| value.as_str())
            .unwrap_or("Run failed")
            .to_string();
        let failure_kind = payload
            .get("failure_kind")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());

        return Some(ConductorError {
            code,
            message,
            failure_kind,
        });
    }

    None
}

fn run_error_for_status(run: &ConductorRunState) -> Option<ConductorError> {
    if let Some(error) = run_error_from_artifacts(run) {
        return Some(error);
    }

    if run.status == ConductorRunStatus::Blocked {
        Some(ConductorError {
            code: "RUN_BLOCKED".to_string(),
            message: "Run blocked by conductor model gateway".to_string(),
            failure_kind: Some(shared_types::FailureKind::Unknown),
        })
    } else if run.status == ConductorRunStatus::Failed {
        Some(ConductorError {
            code: "RUN_FAILED".to_string(),
            message: "Run failed".to_string(),
            failure_kind: Some(shared_types::FailureKind::Unknown),
        })
    } else {
        None
    }
}

fn immediate_response_from_run(run: &ConductorRunState) -> Option<String> {
    run.artifacts.iter().rev().find_map(|artifact| {
        let metadata = artifact.metadata.as_ref()?;
        let capability = metadata.get("capability")?.as_str()?;
        if capability != "immediate_response" {
            return None;
        }
        metadata
            .get("message")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn report_summary_message(report_path: &str) -> Option<String> {
    let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(report_path);
    let content = std::fs::read_to_string(full_path).ok()?;
    let line = content.lines().find(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("```")
    })?;
    let message = line.trim().chars().take(240).collect::<String>();
    (!message.is_empty()).then_some(message)
}

fn toast_from_run(
    run: &ConductorRunState,
    report_path: Option<&str>,
) -> Option<ConductorToastPayload> {
    if run.status != ConductorRunStatus::Completed {
        return None;
    }
    if let Some(message) = immediate_response_from_run(run) {
        return Some(ConductorToastPayload {
            title: "Conductor".to_string(),
            message,
            tone: ConductorToastTone::Info,
            report_path: None,
        });
    }
    if run.output_mode != shared_types::ConductorOutputMode::ToastWithReportLink {
        return None;
    }
    let report_path = report_path?;
    Some(ConductorToastPayload {
        title: "Conductor Answer".to_string(),
        message: report_summary_message(report_path)
            .unwrap_or_else(|| "Conductor completed.".to_string()),
        tone: ConductorToastTone::Success,
        report_path: Some(report_path.to_string()),
    })
}

fn run_state_to_execute_response(run: ConductorRunState) -> ConductorExecuteResponse {
    let run_id = run.run_id.clone();
    let report_path = if run.status == ConductorRunStatus::Completed
        && immediate_response_from_run(&run).is_none()
    {
        Some(format!("reports/{run_id}.md"))
    } else {
        None
    };
    let toast = toast_from_run(&run, report_path.as_deref());
    let error = run_error_for_status(&run);
    let (document_path, writer_window_props) = match run.status {
        ConductorRunStatus::Running => {
            let path = run.document_path.clone();
            // Freshly accepted runs have no seeded agenda yet; avoid opening Writer
            // until planning has selected non-trivial capabilities.
            let props = if run.agenda.is_empty() && run.active_calls.is_empty() {
                None
            } else {
                Some(writer_window_props_for_run_document(&path, &run_id))
            };
            (Some(path), props)
        }
        ConductorRunStatus::Initializing
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
        toast,
        error,
    }
}

fn run_state_to_status_response(run: ConductorRunState) -> ConductorRunStatusResponse {
    let report_path = if run.status == ConductorRunStatus::Completed
        && immediate_response_from_run(&run).is_none()
    {
        Some(format!("reports/{}.md", run.run_id))
    } else {
        None
    };
    let toast = toast_from_run(&run, report_path.as_deref());
    let error = run_error_for_status(&run);

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
        toast,
        error,
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

async fn maybe_wait_for_terminal_run_state(
    conductor: ractor::ActorRef<ConductorMsg>,
    run_id: &str,
    max_wait_ms: u64,
) -> Option<ConductorRunState> {
    let started = std::time::Instant::now();
    loop {
        if started.elapsed().as_millis() as u64 >= max_wait_ms {
            return None;
        }
        let run = ractor::call!(conductor, |reply| ConductorMsg::GetRunState {
            run_id: run_id.to_string(),
            reply,
        })
        .ok()
        .flatten()?;
        match run.status {
            ConductorRunStatus::Completed
            | ConductorRunStatus::Failed
            | ConductorRunStatus::Blocked => return Some(run),
            _ => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
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

    let input_id = ulid::Ulid::new().to_string();
    let user_input_record = shared_types::UserInputRecord {
        input_id: input_id.clone(),
        content: request.objective.clone(),
        surface: "conductor".to_string(),
        desktop_id: request.desktop_id.clone(),
        session_id: request.desktop_id.clone(),
        thread_id: String::new(),
        run_id: None,
        document_path: None,
        base_version_id: None,
        created_at: chrono::Utc::now(),
    };
    let _ = state
        .app_state
        .event_store()
        .cast(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: shared_types::EVENT_TOPIC_USER_INPUT.to_string(),
                payload: serde_json::json!({
                    "surface": "conductor.execute",
                    "objective": &request.objective,
                    "desktop_id": &request.desktop_id,
                    "record": user_input_record,
                }),
                actor_id: "api.conductor".to_string(),
                user_id: "system".to_string(),
            },
        });

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
    let (event_type, phase, importance) = pre_run_telemetry_descriptor();
    broadcast_telemetry_event(
        &state.ws_sessions,
        &request.desktop_id,
        event_type,
        "conductor",
        phase,
        importance,
        serde_json::json!({
            "objective": &request.objective,
        }),
    )
    .await;

    let desktop_id = request.desktop_id.clone();
    let ws_sessions = state.ws_sessions.clone();
    let mut result: Result<Result<ConductorRunState, ActorConductorError>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::ExecuteTask {
            request: request.clone(),
            reply,
        });

    if let Err(err) = &result {
        let err_text = err.to_string();
        if should_retry_conductor_rpc(&err_text) {
            if let Ok(refreshed) = state.app_state.ensure_conductor().await {
                result = ractor::call!(refreshed, |reply| ConductorMsg::ExecuteTask {
                    request,
                    reply,
                });
            }
        }
    }

    match result {
        Ok(Ok(run_state)) => {
            let run_state = match run_state.status {
                ConductorRunStatus::Initializing
                | ConductorRunStatus::Running
                | ConductorRunStatus::WaitingForCalls
                | ConductorRunStatus::Completing => {
                    if let Some(terminal_run) = maybe_wait_for_terminal_run_state(
                        conductor.clone(),
                        &run_state.run_id,
                        1500,
                    )
                    .await
                    {
                        terminal_run
                    } else {
                        run_state
                    }
                }
                _ => run_state,
            };
            let run_status = run_state.status;
            let (event_type, phase, importance) = run_lifecycle_telemetry_descriptor(run_status);
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

/// GET /conductor/runs - List all runs sorted by most recently created
pub async fn list_runs(State(state): State<ApiState>) -> impl IntoResponse {
    let conductor = match state.app_state.ensure_conductor().await {
        Ok(actor) => actor,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": format!("Conductor unavailable: {e}") })),
            )
                .into_response();
        }
    };

    match ractor::call!(conductor, |reply| ConductorMsg::ListRuns { reply }) {
        Ok(runs) => {
            let responses: Vec<ConductorRunStatusResponse> =
                runs.into_iter().map(run_state_to_status_response).collect();
            (StatusCode::OK, Json(responses)).into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": format!("Conductor RPC failed: {e}") })),
        )
            .into_response(),
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

    let mut result: Result<Option<ConductorRunState>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::GetRunState {
            run_id: run_id.clone(),
            reply,
        });

    if let Err(err) = &result {
        let err_text = err.to_string();
        if should_retry_conductor_rpc(&err_text) {
            if let Ok(refreshed) = state.app_state.ensure_conductor().await {
                result = ractor::call!(refreshed, |reply| ConductorMsg::GetRunState {
                    run_id: run_id.clone(),
                    reply,
                });
            }
        }
    }

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

/// GET /conductor/runs/:run_id/state - Get full run state (agenda/calls/artifacts included)
pub async fn get_run_state(
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

    let mut result: Result<Option<ConductorRunState>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::GetRunState {
            run_id: run_id.clone(),
            reply,
        });

    if let Err(err) = &result {
        let err_text = err.to_string();
        if should_retry_conductor_rpc(&err_text) {
            if let Ok(refreshed) = state.app_state.ensure_conductor().await {
                result = ractor::call!(refreshed, |reply| ConductorMsg::GetRunState {
                    run_id: run_id.clone(),
                    reply,
                });
            }
        }
    }

    match result {
        Ok(Some(run_state)) => (StatusCode::OK, Json(run_state)).into_response(),
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
    fn test_run_state_to_execute_response_running_without_agenda_defers_writer_open() {
        let now = Utc::now();
        let run = ConductorRunState {
            run_id: "run_456".to_string(),
            status: ConductorRunStatus::Running,
            objective: "hi".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::Auto,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "conductor/runs/run_456/draft.md".to_string(),
        };

        let response = run_state_to_execute_response(run);
        assert_eq!(response.status, ConductorRunStatus::Running);
        assert_eq!(
            response.document_path.as_deref(),
            Some("conductor/runs/run_456/draft.md")
        );
        assert!(response.writer_window_props.is_none());
    }

    #[test]
    fn test_run_state_to_status_response_uses_failed_event_error_details() {
        let now = Utc::now();
        let run = ConductorRunState {
            run_id: "run_failed".to_string(),
            status: ConductorRunStatus::Failed,
            objective: "test".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::Auto,
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![shared_types::ConductorArtifact {
                artifact_id: "artifact-1".to_string(),
                kind: shared_types::ArtifactKind::JsonData,
                reference: "event://conductor.task.failed".to_string(),
                mime_type: Some("application/json".to_string()),
                created_at: now,
                source_call_id: "event".to_string(),
                metadata: Some(serde_json::json!({
                    "event_type": "conductor.task.failed",
                    "event_payload": {
                        "error_code": "MODEL_GATEWAY_ERROR",
                        "error_message": "Missing API key: OPENAI_API_KEY",
                        "failure_kind": "provider"
                    }
                })),
            }],
            decision_log: vec![],
            document_path: "conductor/runs/run_failed/draft.md".to_string(),
        };

        let response = run_state_to_status_response(run);
        let error = response.error.expect("error details");
        assert_eq!(error.code, "MODEL_GATEWAY_ERROR");
        assert_eq!(error.message, "Missing API key: OPENAI_API_KEY");
        assert_eq!(
            error.failure_kind,
            Some(shared_types::FailureKind::Provider)
        );
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

    #[test]
    fn test_pre_run_telemetry_uses_requested_event() {
        let (event_type, phase, importance) = pre_run_telemetry_descriptor();
        assert_eq!(event_type, "conductor.task.requested");
        assert_eq!(phase, "initialization");
        assert!(matches!(importance, EventImportance::Normal));
    }

    #[test]
    fn test_run_lifecycle_telemetry_uses_started_for_accepted_statuses() {
        let statuses = [
            ConductorRunStatus::Initializing,
            ConductorRunStatus::Running,
            ConductorRunStatus::WaitingForCalls,
            ConductorRunStatus::Completing,
        ];
        for status in statuses {
            let (event_type, phase, importance) = run_lifecycle_telemetry_descriptor(status);
            assert_eq!(event_type, "conductor.task.started");
            assert_eq!(phase, "run_start");
            assert!(matches!(importance, EventImportance::Normal));
        }
    }

    #[test]
    fn test_run_lifecycle_telemetry_maps_terminal_statuses() {
        let (completed_event, completed_phase, completed_importance) =
            run_lifecycle_telemetry_descriptor(ConductorRunStatus::Completed);
        assert_eq!(completed_event, "conductor.task.completed");
        assert_eq!(completed_phase, "completion");
        assert!(matches!(completed_importance, EventImportance::Normal));

        let (failed_event, failed_phase, failed_importance) =
            run_lifecycle_telemetry_descriptor(ConductorRunStatus::Failed);
        assert_eq!(failed_event, "conductor.task.failed");
        assert_eq!(failed_phase, "failure");
        assert!(matches!(failed_importance, EventImportance::High));
    }
}
