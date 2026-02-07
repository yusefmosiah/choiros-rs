//! Desktop API endpoints
//!
//! PREDICTION: RESTful endpoints can manage window state and app registry,
//! providing the UI with desktop functionality via HTTP.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actors::desktop::DesktopActorMsg;
use crate::api::websocket::{broadcast_event, WsMessage};
use crate::api::ApiState;

const MIN_WINDOW_WIDTH: i32 = 200;
const MIN_WINDOW_HEIGHT: i32 = 160;

async fn get_desktop_actor(
    app_state: &std::sync::Arc<crate::app_state::AppState>,
    desktop_id: &str,
) -> Result<ractor::ActorRef<DesktopActorMsg>, axum::response::Response> {
    app_state
        .get_or_create_desktop(desktop_id.to_string(), "system".to_string())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": format!("Failed to get desktop: {e}")
                })),
            )
                .into_response()
        })
}

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
pub async fn open_window(
    Path(desktop_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<OpenWindowRequest>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::OpenWindow {
        app_id: req.app_id.clone(),
        title: req.title.clone(),
        props: req.props.clone(),
        reply,
    }) {
        Ok(Ok(window)) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowOpened {
                    window: window.clone(),
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(OpenWindowResponse {
                    success: true,
                    window: Some(window),
                    error: None,
                }),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(OpenWindowResponse {
                success: false,
                window: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OpenWindowResponse {
                success: false,
                window: None,
                error: Some(format!("Actor error: {e}")),
            }),
        )
            .into_response(),
    }
}

/// Get all windows for a desktop
pub async fn get_windows(
    Path(desktop_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetWindows { reply }) {
        Ok(windows) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "windows": windows
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to get windows: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Close a window
pub async fn close_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::CloseWindow {
        window_id: window_id.clone(),
        reply,
    }) {
        Ok(Ok(())) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowClosed {
                    window_id: window_id.clone(),
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Window closed"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Move a window
pub async fn move_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<MoveWindowRequest>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::MoveWindow {
        window_id: window_id.clone(),
        x: req.x,
        y: req.y,
        reply,
    }) {
        Ok(Ok(())) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowMoved {
                    window_id: window_id.clone(),
                    x: req.x,
                    y: req.y,
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Window moved"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Resize a window
pub async fn resize_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<ResizeWindowRequest>,
) -> impl IntoResponse {
    if req.width < MIN_WINDOW_WIDTH || req.height < MIN_WINDOW_HEIGHT {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!(
                    "Invalid size {}x{} (minimum is {}x{})",
                    req.width,
                    req.height,
                    MIN_WINDOW_WIDTH,
                    MIN_WINDOW_HEIGHT
                )
            })),
        )
            .into_response();
    }

    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::ResizeWindow {
        window_id: window_id.clone(),
        width: req.width,
        height: req.height,
        reply,
    }) {
        Ok(Ok(())) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowResized {
                    window_id: window_id.clone(),
                    width: req.width,
                    height: req.height,
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Window resized"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Focus a window (bring to front)
pub async fn focus_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::FocusWindow {
        window_id: window_id.clone(),
        reply,
    }) {
        Ok(Ok(())) => {
            let z_index =
                match ractor::call!(desktop, |reply| DesktopActorMsg::GetWindows { reply }) {
                    Ok(windows) => windows
                        .into_iter()
                        .find(|w| w.id == window_id)
                        .map(|w| w.z_index)
                        .unwrap_or(0),
                    Err(_) => 0,
                };

            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowFocused {
                    window_id: window_id.clone(),
                    z_index,
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Window focused"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Minimize a window
pub async fn minimize_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match ractor::call!(desktop, |reply| DesktopActorMsg::MinimizeWindow {
        window_id: window_id.clone(),
        reply,
    }) {
        Ok(Ok(())) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowMinimized {
                    window_id: window_id.clone(),
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Window minimized"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Maximize a window
pub async fn maximize_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match ractor::call!(desktop, |reply| DesktopActorMsg::MaximizeWindow {
        window_id: window_id.clone(),
        reply,
    }) {
        Ok(Ok(window)) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowMaximized {
                    window_id: window.id.clone(),
                    x: window.x,
                    y: window.y,
                    width: window.width,
                    height: window.height,
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "window": window,
                    "message": "Window maximized"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Restore a window from minimized/maximized
pub async fn restore_window(
    Path((desktop_id, window_id)): Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match ractor::call!(desktop, |reply| DesktopActorMsg::RestoreWindow {
        window_id: window_id.clone(),
        reply,
    }) {
        Ok(Ok(restored)) => {
            broadcast_event(
                &state.ws_sessions,
                &desktop_id,
                WsMessage::WindowRestored {
                    window_id: restored.window.id.clone(),
                    x: restored.window.x,
                    y: restored.window.y,
                    width: restored.window.width,
                    height: restored.window.height,
                    from: restored.from.clone(),
                },
            )
            .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "window": restored.window,
                    "from": restored.from,
                    "message": "Window restored"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Get full desktop state
pub async fn get_desktop_state(
    Path(desktop_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetDesktopState { reply }) {
        Ok(desktop_state) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "desktop": desktop_state
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to get desktop state: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Register a new app
pub async fn register_app(
    Path(desktop_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<shared_types::AppDefinition>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::RegisterApp {
        app: req,
        reply,
    }) {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "App registered"
            })),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Get all registered apps
pub async fn get_apps(
    Path(desktop_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    let desktop = match get_desktop_actor(&app_state, &desktop_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Use ractor call pattern
    match ractor::call!(desktop, |reply| DesktopActorMsg::GetApps { reply }) {
        Ok(apps) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "apps": apps
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to get apps: {}", e)
            })),
        )
            .into_response(),
    }
}
