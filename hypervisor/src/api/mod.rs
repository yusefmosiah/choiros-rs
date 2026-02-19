use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::error;

use crate::{sandbox::SandboxRole, AppState};

/// GET /admin/sandboxes — list all sandbox statuses
pub async fn list_sandboxes(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let snapshots = state.sandbox_registry.snapshot().await;
    Json(snapshots)
}

#[derive(serde::Deserialize)]
pub struct SandboxActionPath {
    pub user_id: String,
    pub role: String,
}

fn parse_role(s: &str) -> Option<SandboxRole> {
    match s {
        "live" => Some(SandboxRole::Live),
        "dev" => Some(SandboxRole::Dev),
        _ => None,
    }
}

/// POST /admin/sandboxes/:user_id/:role/start
pub async fn start_sandbox(
    State(state): State<Arc<AppState>>,
    Path(p): Path<SandboxActionPath>,
) -> impl IntoResponse {
    let Some(role) = parse_role(&p.role) else {
        return (StatusCode::BAD_REQUEST, "role must be 'live' or 'dev'").into_response();
    };
    match state
        .sandbox_registry
        .ensure_running(&p.user_id, role)
        .await
    {
        Ok(port) => Json(serde_json::json!({ "status": "running", "port": port })).into_response(),
        Err(e) => {
            error!("start sandbox: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// POST /admin/sandboxes/:user_id/:role/stop
pub async fn stop_sandbox(
    State(state): State<Arc<AppState>>,
    Path(p): Path<SandboxActionPath>,
) -> impl IntoResponse {
    let Some(role) = parse_role(&p.role) else {
        return (StatusCode::BAD_REQUEST, "role must be 'live' or 'dev'").into_response();
    };
    match state.sandbox_registry.stop(&p.user_id, role).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("stop sandbox: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// POST /admin/sandboxes/:user_id/swap — promote dev to live
pub async fn swap_sandbox_roles(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.sandbox_registry.swap_roles(&user_id).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("swap sandbox roles: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
