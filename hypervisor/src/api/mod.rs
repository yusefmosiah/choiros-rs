use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::error;

use tower_sessions::Session;

use crate::{
    auth::session as sess,
    runtime_registry::{self, PointerTarget},
    sandbox::SandboxRole,
    AppState,
};

/// POST /heartbeat — touch sandbox activity timestamp without proxying.
/// Keeps the idle watchdog from hibernating an active user's sandbox.
pub async fn heartbeat(State(state): State<Arc<AppState>>, session: Session) -> impl IntoResponse {
    let Some(user_id) = sess::get_user_id(&session).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    state
        .sandbox_registry
        .touch_activity(&user_id, SandboxRole::Live)
        .await;
    StatusCode::OK.into_response()
}

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

/// POST /admin/sandboxes/:user_id/:role/hibernate
pub async fn hibernate_sandbox(
    State(state): State<Arc<AppState>>,
    Path(p): Path<SandboxActionPath>,
) -> impl IntoResponse {
    let Some(role) = parse_role(&p.role) else {
        return (StatusCode::BAD_REQUEST, "role must be 'live' or 'dev'").into_response();
    };
    match state.sandbox_registry.hibernate(&p.user_id, role).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("hibernate sandbox: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /admin/machine-classes — list available machine classes
pub async fn list_machine_classes(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.sandbox_registry.list_machine_classes())
}

/// PUT /admin/sandboxes/:user_id/machine-class — set per-user machine class override
pub async fn set_machine_class(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(body): Json<SetMachineClassRequest>,
) -> impl IntoResponse {
    match state
        .sandbox_registry
        .set_user_machine_class(&user_id, &body.class_name)
    {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "user_id": user_id,
            "machine_class": body.class_name,
        }))
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// DELETE /admin/sandboxes/:user_id/machine-class — clear per-user override
pub async fn clear_machine_class(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    state.sandbox_registry.clear_user_machine_class(&user_id);
    StatusCode::OK
}

#[derive(serde::Deserialize)]
pub struct SetMachineClassRequest {
    pub class_name: String,
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

#[derive(serde::Deserialize)]
pub struct BranchActionPath {
    pub user_id: String,
    pub branch: String,
}

/// POST /admin/sandboxes/:user_id/branches/:branch/start
pub async fn start_branch_sandbox(
    State(state): State<Arc<AppState>>,
    Path(p): Path<BranchActionPath>,
) -> impl IntoResponse {
    match state
        .sandbox_registry
        .ensure_branch_running(&p.user_id, &p.branch)
        .await
    {
        Ok(port) => Json(serde_json::json!({ "status": "running", "port": port })).into_response(),
        Err(e) => {
            error!("start branch sandbox: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// POST /admin/sandboxes/:user_id/branches/:branch/stop
pub async fn stop_branch_sandbox(
    State(state): State<Arc<AppState>>,
    Path(p): Path<BranchActionPath>,
) -> impl IntoResponse {
    match state
        .sandbox_registry
        .stop_branch(&p.user_id, &p.branch)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("stop branch sandbox: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct SetPointerRequest {
    pub pointer_name: String,
    pub target_kind: String,
    pub target_value: String,
}

/// GET /admin/sandboxes/:user_id/pointers
pub async fn list_route_pointers(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match runtime_registry::list_route_pointers(&state.db, &user_id).await {
        Ok(pointers) => Json(pointers).into_response(),
        Err(e) => {
            error!("list route pointers: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// POST /admin/sandboxes/:user_id/pointers/set
pub async fn set_route_pointer(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(body): Json<SetPointerRequest>,
) -> impl IntoResponse {
    if !runtime_registry::is_valid_pointer_name(&body.pointer_name) {
        return (
            StatusCode::BAD_REQUEST,
            "invalid pointer name (allowed: [A-Za-z0-9._-], max 64)",
        )
            .into_response();
    }

    let target = match body.target_kind.as_str() {
        "role" => {
            let Some(role) = parse_role(&body.target_value) else {
                return (
                    StatusCode::BAD_REQUEST,
                    "role target_value must be 'live' or 'dev'",
                )
                    .into_response();
            };
            PointerTarget::Role(role)
        }
        "branch" => {
            if !runtime_registry::is_valid_branch_name(&body.target_value) {
                return (
                    StatusCode::BAD_REQUEST,
                    "invalid branch target_value (allowed: [A-Za-z0-9._-], max 64)",
                )
                    .into_response();
            }
            PointerTarget::Branch(body.target_value.clone())
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "target_kind must be 'role' or 'branch'",
            )
                .into_response()
        }
    };

    match runtime_registry::upsert_route_pointer(&state.db, &user_id, &body.pointer_name, &target)
        .await
    {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "user_id": user_id,
            "pointer_name": body.pointer_name,
        }))
        .into_response(),
        Err(e) => {
            error!("set route pointer: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
