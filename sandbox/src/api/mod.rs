//! HTTP API routes for ChoirOS Sandbox
//!
//! PREDICTION: RESTful endpoints can bridge the actor system to the UI,
//! providing stateless HTTP access to the event-sourced backend.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;

pub mod chat;
pub mod desktop;
pub mod terminal;
pub mod user;
pub mod viewer;
pub mod websocket;
pub mod websocket_chat;

use crate::api::websocket::WsSessions;
use crate::app_state::AppState;

#[derive(Clone)]
pub struct ApiState {
    pub app_state: Arc<AppState>,
    pub ws_sessions: WsSessions,
}

/// Configure all API routes
pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(websocket::ws_handler))
        // Chat routes
        .route("/chat/send", post(chat::send_message))
        .route("/chat/{actor_id}/messages", get(chat::get_messages))
        // User preference routes
        .route(
            "/user/{user_id}/preferences",
            get(user::get_user_preferences).patch(user::update_user_preferences),
        )
        // Desktop routes
        .route("/desktop/{desktop_id}", get(desktop::get_desktop_state))
        .route(
            "/desktop/{desktop_id}/windows",
            get(desktop::get_windows).post(desktop::open_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}",
            delete(desktop::close_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/position",
            patch(desktop::move_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/size",
            patch(desktop::resize_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/focus",
            post(desktop::focus_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/minimize",
            post(desktop::minimize_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/maximize",
            post(desktop::maximize_window),
        )
        .route(
            "/desktop/{desktop_id}/windows/{window_id}/restore",
            post(desktop::restore_window),
        )
        .route(
            "/desktop/{desktop_id}/apps",
            get(desktop::get_apps).post(desktop::register_app),
        )
        // Viewer routes
        .route(
            "/viewer/content",
            get(viewer::get_viewer_content).patch(viewer::patch_viewer_content),
        )
        // Terminal routes
        .route(
            "/api/terminals/{terminal_id}",
            get(terminal::create_terminal),
        )
        .route(
            "/api/terminals/{terminal_id}/info",
            get(terminal::get_terminal_info),
        )
        .route(
            "/api/terminals/{terminal_id}/stop",
            get(terminal::stop_terminal),
        )
        // Terminal WebSocket route
        .route(
            "/ws/terminal/{terminal_id}",
            get(terminal::terminal_websocket),
        )
        // Chat agent WebSocket routes
        .route("/ws/chat/{actor_id}", get(websocket_chat::chat_websocket))
        .route(
            "/ws/chat/{actor_id}/{user_id}",
            get(websocket_chat::chat_websocket_with_user),
        )
}

/// Health check endpoint
pub async fn health_check(State(_state): State<ApiState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
        "status": "healthy",
        "service": "choiros-sandbox",
        "version": "0.1.0"
        })),
    )
}
