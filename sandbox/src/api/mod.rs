//! HTTP API routes for ChoirOS Sandbox
//!
//! PREDICTION: RESTful endpoints can bridge the actor system to the UI,
//! providing stateless HTTP access to the event-sourced backend.

use actix_web::{web, HttpResponse};
use serde_json::json;

pub mod chat;
pub mod desktop;
pub mod websocket;
pub mod websocket_chat;

/// Configure all API routes
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(chat::send_message)
        .service(chat::get_messages)
        // Desktop routes
        .service(desktop::get_desktop_state)
        .service(desktop::get_windows)
        .service(desktop::open_window)
        .service(desktop::close_window)
        .service(desktop::move_window)
        .service(desktop::resize_window)
        .service(desktop::focus_window)
        .service(desktop::get_apps)
        .service(desktop::register_app)
        // Chat agent WebSocket routes
        .route(
            "/ws/chat/{actor_id}",
            web::get().to(websocket_chat::chat_websocket),
        )
        .route(
            "/ws/chat/{actor_id}/{user_id}",
            web::get().to(websocket_chat::chat_websocket_with_user),
        );
}

/// Health check endpoint
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "service": "choiros-sandbox",
        "version": "0.1.0"
    }))
}
