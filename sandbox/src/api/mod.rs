//! HTTP API routes for ChoirOS Sandbox
//!
//! PREDICTION: RESTful endpoints can bridge the actor system to the UI,
//! providing stateless HTTP access to the event-sourced backend.

use actix_web::{web, HttpResponse};
use serde_json::json;

pub mod chat;

/// Configure all API routes
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(chat::send_message)
        .service(chat::get_messages);
}

/// Health check endpoint
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "service": "choiros-sandbox",
        "version": "0.1.0"
    }))
}
