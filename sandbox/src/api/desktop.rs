//! Desktop API endpoints
//!
//! PREDICTION: RESTful endpoints can manage window state and app registry,
//! providing the UI with desktop functionality via HTTP.

use actix_web::{get, post, web, HttpResponse, delete, patch};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::AppState;
use crate::actors::desktop::{
    OpenWindow, CloseWindow, MoveWindow, ResizeWindow, FocusWindow,
    GetWindows, GetDesktopState, RegisterApp, GetApps
};

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
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(OpenWindow {
        app_id: req.app_id.clone(),
        title: req.title.clone(),
        props: req.props.clone(),
    }).await {
        Ok(Ok(window)) => {
            HttpResponse::Ok().json(OpenWindowResponse {
                success: true,
                window: Some(window),
                error: None,
            })
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(OpenWindowResponse {
                success: false,
                window: None,
                error: Some(e.to_string()),
            })
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(OpenWindowResponse {
                success: false,
                window: None,
                error: Some("Actor mailbox error".to_string()),
            })
        }
    }
}

/// Get all windows for a desktop
#[get("/desktop/{desktop_id}/windows")]
pub async fn get_windows(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(GetWindows).await {
        Ok(windows) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "windows": windows
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Failed to get windows"
            }))
        }
    }
}

/// Close a window
#[delete("/desktop/{desktop_id}/windows/{window_id}")]
pub async fn close_window(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(CloseWindow { window_id }).await {
        Ok(Ok(())) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "message": "Window closed"
            }))
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
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
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(MoveWindow {
        window_id,
        x: req.x,
        y: req.y,
    }).await {
        Ok(Ok(())) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "message": "Window moved"
            }))
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
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
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(ResizeWindow {
        window_id,
        width: req.width,
        height: req.height,
    }).await {
        Ok(Ok(())) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "message": "Window resized"
            }))
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
    }
}

/// Focus a window (bring to front)
#[post("/desktop/{desktop_id}/windows/{window_id}/focus")]
pub async fn focus_window(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let (desktop_id, window_id) = path.into_inner();
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(FocusWindow { window_id }).await {
        Ok(Ok(())) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "message": "Window focused"
            }))
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
    }
}

/// Get full desktop state
#[get("/desktop/{desktop_id}")]
pub async fn get_desktop_state(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(GetDesktopState).await {
        Ok(desktop_state) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "desktop": desktop_state
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Failed to get desktop state"
            }))
        }
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
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(RegisterApp { app: req.into_inner() }).await {
        Ok(Ok(())) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "message": "App registered"
            }))
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
    }
}

/// Get all registered apps
#[get("/desktop/{desktop_id}/apps")]
pub async fn get_apps(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let desktop_id = path.into_inner();
    
    let desktop = state.actor_manager.get_or_create_desktop(
        desktop_id,
        "system".to_string()
    );
    
    match desktop.send(GetApps).await {
        Ok(apps) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "apps": apps
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Failed to get apps"
            }))
        }
    }
}
