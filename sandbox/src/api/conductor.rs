//! Conductor API endpoints
//!
//! All orchestration flows through ConductorActor.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::actors::conductor::{ConductorError as ActorConductorError, ConductorMsg};
use crate::api::ApiState;
use shared_types::{
    ConductorError, ConductorExecuteRequest, ConductorExecuteResponse, ConductorTaskState,
    ConductorTaskStatus,
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

fn writer_window_props_for_report(report_path: &str) -> serde_json::Value {
    serde_json::json!({
        "x": 100,
        "y": 100,
        "width": 900,
        "height": 680,
        "path": report_path,
        "preview_mode": true,
    })
}

fn task_state_to_execute_response(task: ConductorTaskState) -> ConductorExecuteResponse {
    let writer_window_props =
        if task.output_mode == shared_types::ConductorOutputMode::MarkdownReportToWriter {
            task.report_path
                .as_ref()
                .map(|path| writer_window_props_for_report(path))
        } else {
            None
        };

    ConductorExecuteResponse {
        task_id: task.task_id,
        status: task.status,
        report_path: task.report_path,
        writer_window_props,
        toast: task.toast,
        correlation_id: task.correlation_id,
        error: task.error,
    }
}

fn map_actor_error(err: ActorConductorError) -> (StatusCode, ConductorError) {
    match err {
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
        | ActorConductorError::ReportWriteFailed(msg)
        | ActorConductorError::DuplicateTask(msg) => (
            ConductorErrorCode::InternalError.status_code(),
            conductor_error(
                ConductorErrorCode::InternalError,
                msg,
                Some(shared_types::FailureKind::Unknown),
            ),
        ),
    }
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
            status: ConductorTaskStatus::Failed,
            report_path: None,
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
            status: ConductorTaskStatus::Failed,
            report_path: None,
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
                status: ConductorTaskStatus::Failed,
                report_path: None,
                writer_window_props: None,
                toast: None,
                correlation_id: request.correlation_id.unwrap_or_default(),
                error: Some(error),
            });
            return (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
        }
    };

    let result: Result<Result<ConductorTaskState, ActorConductorError>, _> =
        ractor::call!(conductor, |reply| ConductorMsg::ExecuteTask {
            request,
            reply,
        });

    match result {
        Ok(Ok(task_state)) => {
            let status = status_code_for_task(task_state.status);
            let response = task_state_to_execute_response(task_state);
            (status, Json(response)).into_response()
        }
        Ok(Err(actor_err)) => {
            let (status, error) = map_actor_error(actor_err);
            let body = Json(ConductorExecuteResponse {
                task_id: String::new(),
                status: ConductorTaskStatus::Failed,
                report_path: None,
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
                status: ConductorTaskStatus::Failed,
                report_path: None,
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
        let props = writer_window_props_for_report("reports/test.md");
        assert_eq!(props["path"], "reports/test.md");
        assert_eq!(props["preview_mode"], true);
    }
}
