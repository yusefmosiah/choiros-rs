use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sqlx::Row;
use tracing::error;

use tower_sessions::Session;

use crate::{
    auth::session as sess,
    jobs,
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

/// GET /admin/stats — host-level resource stats for stress testing
pub async fn host_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let snapshots = state.sandbox_registry.snapshot().await;
    let running = snapshots
        .iter()
        .filter(|s| s.status == crate::sandbox::SandboxStatus::Running)
        .count();
    let total = snapshots.len();

    Json(serde_json::json!({
        "memory_total_mb": crate::sandbox::read_total_memory_mb(),
        "memory_available_mb": crate::sandbox::read_available_memory_mb(),
        "vms_running": running,
        "vms_total": total,
    }))
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

// ── ADR-0014 Phase 7: Job queue endpoints ────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CreateJobRequest {
    pub job_type: String,
    pub command: Option<String>,
    pub payload_json: Option<String>,
    pub machine_class: Option<String>,
    pub priority: Option<i32>,
    pub max_duration_s: Option<i32>,
}

/// POST /admin/jobs — create a new job in the queue
pub async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateJobRequest>,
) -> impl IntoResponse {
    match jobs::create_job(&jobs::CreateJobParams {
        pool: &state.db,
        user_id: "system",
        job_type: &body.job_type,
        command: body.command.as_deref(),
        payload_json: body.payload_json.as_deref(),
        machine_class: body.machine_class.as_deref(),
        priority: body.priority.unwrap_or(0),
        max_duration_s: body.max_duration_s.unwrap_or(1800),
    })
    .await
    {
        Ok(job_id) => {
            Json(serde_json::json!({ "job_id": job_id, "status": "queued" })).into_response()
        }
        Err(e) => {
            error!("create job: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /admin/jobs/:job_id — get job status
pub async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    match jobs::get_job(&state.db, &job_id).await {
        Ok(Some(job)) => Json(job).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("get job: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /admin/jobs — list all queued/running jobs
pub async fn list_jobs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query(
        r#"
        SELECT id, user_id, job_type, status, priority, machine_class,
               command, payload_json, result_json, error_message,
               worker_vm_id, max_duration_s, created_at, started_at, completed_at
        FROM jobs ORDER BY created_at DESC LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => {
            let jobs: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get::<String, _>("id"),
                        "user_id": r.get::<String, _>("user_id"),
                        "job_type": r.get::<String, _>("job_type"),
                        "status": r.get::<String, _>("status"),
                        "priority": r.get::<i32, _>("priority"),
                        "created_at": r.get::<i64, _>("created_at"),
                    })
                })
                .collect();
            Json(serde_json::json!({ "jobs": jobs })).into_response()
        }
        Err(e) => {
            error!("list jobs: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// DELETE /admin/jobs/:job_id — cancel a job
pub async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    match jobs::cancel_job(&state.db, &job_id).await {
        Ok(()) => {
            Json(serde_json::json!({ "job_id": job_id, "status": "cancelled" })).into_response()
        }
        Err(e) => {
            error!("cancel job: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ── ADR-0014 Phase 8: Promotion endpoints ────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CreatePromotionRequest {
    pub job_id: Option<String>,
    pub binary_path: Option<String>,
    pub verification: Option<serde_json::Value>,
}

/// POST /admin/sandboxes/:user_id/promote — start a promotion
pub async fn promote_sandbox(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(body): Json<CreatePromotionRequest>,
) -> impl IntoResponse {
    let verification_json = body
        .verification
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    match jobs::create_promotion(
        &state.db,
        &user_id,
        body.job_id.as_deref(),
        body.binary_path.as_deref(),
        verification_json.as_deref(),
    )
    .await
    {
        Ok(promotion_id) => {
            // Execute promotion in background
            let db = state.db.clone();
            let registry = Arc::clone(&state.sandbox_registry);
            let pid = promotion_id.clone();
            let uid = user_id.clone();
            tokio::spawn(async move {
                if let Err(e) = jobs::execute_promotion(&db, &pid, &uid, &registry).await {
                    error!(promotion_id = %pid, "promotion execution failed: {e}");
                }
            });

            Json(serde_json::json!({
                "promotion_id": promotion_id,
                "status": "pending"
            }))
            .into_response()
        }
        Err(e) => {
            error!("create promotion: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /admin/sandboxes/:user_id/promotions — list promotions for a user
pub async fn list_promotions(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match jobs::list_promotions_for_user(&state.db, &user_id).await {
        Ok(promotions) => Json(serde_json::json!({ "promotions": promotions })).into_response(),
        Err(e) => {
            error!("list promotions: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /admin/promotions/:promotion_id — get promotion status
pub async fn get_promotion(
    State(state): State<Arc<AppState>>,
    Path(promotion_id): Path<String>,
) -> impl IntoResponse {
    match jobs::get_promotion(&state.db, &promotion_id).await {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("get promotion: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
