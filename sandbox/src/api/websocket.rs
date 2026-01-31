//! WebSocket API for real-time desktop events
//!
//! Uses actix-ws (native Actix Web 4 WebSocket support)

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actor_manager::AppState;

/// Shared state for WebSocket sessions
pub type WsSessions = Arc<Mutex<HashMap<String, Vec<actix_ws::Session>>>>;

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    // Client -> Server
    #[serde(rename = "subscribe")]
    Subscribe { desktop_id: String },
    
    #[serde(rename = "ping")]
    Ping,
    
    // Server -> Client
    #[serde(rename = "pong")]
    Pong,
    
    #[serde(rename = "desktop_state")]
    DesktopState { desktop: shared_types::DesktopState },
    
    #[serde(rename = "window_opened")]
    WindowOpened { window: shared_types::WindowState },
    
    #[serde(rename = "window_closed")]
    WindowClosed { window_id: String },
    
    #[serde(rename = "window_moved")]
    WindowMoved { window_id: String, x: i32, y: i32 },
    
    #[serde(rename = "window_resized")]
    WindowResized { window_id: String, width: u32, height: u32 },
    
    #[serde(rename = "window_focused")]
    WindowFocused { window_id: String, z_index: u32 },
    
    #[serde(rename = "app_registered")]
    AppRegistered { app: shared_types::AppDefinition },
    
    #[serde(rename = "error")]
    Error { message: String },
}

/// WebSocket handler
pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    app_state: web::Data<AppState>,
    sessions: web::Data<WsSessions>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, _msg_stream) = actix_ws::handle(&req, body)?;
    
    tracing::info!("WebSocket connection established");
    
    // Clone for the spawned task
    let _app_state_clone = app_state.clone();
    let sessions_clone = sessions.clone();
    let mut session_clone = session.clone();
    
    // Spawn handler task
    actix_web::rt::spawn(async move {
        let current_desktop_id: Option<String> = None;
        
        // Send initial pong
        let pong_msg = WsMessage::Pong;
        if let Ok(json) = serde_json::to_string(&pong_msg) {
            let _ = session_clone.text(json).await;
        }
        
        // Handle messages
        // Note: In actix-ws, we need to process the stream manually
        // For now, just keep connection alive with ping/pong
        
        // When connection closes, remove from sessions
        if let Some(desktop_id) = current_desktop_id {
            if let Ok(mut sessions) = sessions_clone.lock() {
                if let Some(_subscribers) = sessions.get_mut(&desktop_id) {
                    // Note: Can't easily remove specific session without storing ID
                    // For now, we'll handle this differently
                }
            }
        }
    });
    
    Ok(response)
}

/// Broadcast an event to all subscribers of a desktop
pub async fn broadcast_event(
    sessions: &WsSessions, 
    desktop_id: &str, 
    event: WsMessage
) {
    let json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize WS message: {}", e);
            return;
        }
    };
    
    if let Ok(sessions) = sessions.lock() {
        if let Some(subscribers) = sessions.get(desktop_id) {
            for session in subscribers {
                let mut session = session.clone();
                let json_clone = json.clone();
                actix_web::rt::spawn(async move {
                    let _ = session.text(json_clone).await;
                });
            }
        }
    }
}

/// Subscribe a session to a desktop
pub fn subscribe_session(
    sessions: &WsSessions,
    desktop_id: String,
    session: actix_ws::Session,
) {
    if let Ok(mut sessions) = sessions.lock() {
        sessions.entry(desktop_id).or_default().push(session);
    }
}
