//! Desktop API endpoints
//!
//! PREDICTION: RESTful endpoints can manage window state and app registry,
//! providing the UI with desktop functionality via HTTP.

use actix_web::{delete, get, patch, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::{AppState, DesktopActorMsg};

/// Request to open a window
#[derive(Debug, Deserialize)]
pub struct OpenWindowRequest {
    pub app_id: String,
    pub title: String,
    pub props: Option<serde_json::Value>,
}

/// Response after opening a window
#[derive(Debug, Serialize)]
pub struct OpenWindowResponse {
    pub success: bool,
    pub window: Option<shared_types::WindowState>,
    pub error: Option<String>,
}

/// Request to move a window
#[derive(Debug, Deserialize)]
pub struct MoveWindowRequest {
    pub x: i32,
    pub y: i32,
}

/// Request to resize a window
#[derive(Debug, Deserialize)]
pub struct ResizeWindowRequest {
    pub width: i32,
    pub height: i32,
}

/// Open a new window for an app
#[post("/desktop/{desktop_id}/windows")]
pub async fn open_window(
    path: web::Path<String>,
    req: web::Json<OpenWindowRequest>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();

    // Get or create DesktopActor
    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::OpenWindow {
            app_id: req.app_id.clone(),
            title: req.title.clone(),
            props: req.props.clone(),
            reply,
        }
    ) {
        Ok(Ok(window)) => HttpResponse::Ok().json(OpenWindowResponse {
            success: true,
            window: Some(window),
            error: None,
        }),
        Ok(Err(e)) => HttpResponse::BadRequest().json(OpenWindowResponse {
            success: false,
            window: None,
            error: Some(e.to_string()),
        }),
        Err(e) => HttpResponse::InternalServerError().json(OpenWindowResponse {
            success: false,
            window: None,
            error: Some(format!("Actor error: {}", e)),
        }),
    }
}

/// Get all windows for a desktop
#[get("/desktop/{desktop_id}/windows")]
pub async fn get_windows(path: web::Path<String>, state: web::Data<AppState>) -> HttpResponse {
    let desktop_id = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetWindows { reply }) {
        Ok(windows) => HttpResponse::Ok().json(json!({
            "success": true,
            "windows": windows
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Failed to get windows: {}", e)
        })),
    }
}

/// Close a window
#[delete("/desktop/{desktop_id}/windows/{window_id}")]
pub async fn close_window(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::CloseWindow {
            window_id: window_id.clone(),
            reply,
        }
    ) {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "success": true,
            "message": "Window closed"
        })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Actor error: {}", e)
        })),
    }
}

/// Move a window
#[patch("/desktop/{desktop_id}/windows/{window_id}/position")]
pub async fn move_window(
    path: web::Path<(String, String)>,
    req: web::Json<MoveWindowRequest>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::MoveWindow {
            window_id: window_id.clone(),
            x: req.x,
            y: req.y,
            reply,
        }
    ) {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "success": true,
            "message": "Window moved"
        })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Actor error: {}", e)
        })),
    }
}

/// Resize a window
#[patch("/desktop/{desktop_id}/windows/{window_id}/size")]
pub async fn resize_window(
    path: web::Path<(String, String)>,
    req: web::Json<ResizeWindowRequest>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::ResizeWindow {
            window_id: window_id.clone(),
            width: req.width,
            height: req.height,
            reply,
        }
    ) {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "success": true,
            "message": "Window resized"
        })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Actor error: {}", e)
        })),
    }
}

/// Focus a window (bring to front)
#[post("/desktop/{desktop_id}/windows/{window_id}/focus")]
pub async fn focus_window(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::FocusWindow {
            window_id: window_id.clone(),
            reply,
        }
    ) {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "success": true,
            "message": "Window focused"
        })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Actor error: {}", e)
        })),
    }
}

/// Get full desktop state
#[get("/desktop/{desktop_id}")]
pub async fn get_desktop_state(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetDesktopState { reply }) {
        Ok(desktop_state) => HttpResponse::Ok().json(json!({
            "success": true,
            "desktop": desktop_state
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Failed to get desktop state: {}", e)
        })),
    }
}

/// Register a new app
#[post("/desktop/{desktop_id}/apps")]
pub async fn register_app(
    path: web::Path<String>,
    req: web::Json<shared_types::AppDefinition>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(
        desktop,
        |reply| DesktopActorMsg::RegisterApp {
            app: req.into_inner(),
            reply,
        }
    ) {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "success": true,
            "message": "App registered"
        })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Actor error: {}", e)
        })),
    }
}

/// Get all registered apps
#[get("/desktop/{desktop_id}/apps")]
pub async fn get_apps(path: web::Path<String>, state: web::Data<AppState>) -> HttpResponse {
    let desktop_id = path.into_inner();

    let desktop = state
        .actor_manager
        .get_or_create_desktop(desktop_id, "system".to_string()).await;

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetApps { reply }) {
        Ok(apps) => HttpResponse::Ok().json(json!({
            "success": true,
            "apps": apps
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": format!("Failed to get apps: {}", e)
        })),
    }
}
