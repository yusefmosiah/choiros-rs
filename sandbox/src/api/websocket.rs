//! WebSocket API for real-time desktop events
//!
//! Uses actix-ws (native Actix Web 4 WebSocket support)

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actor_manager::AppState;
use crate::actors::desktop::GetDesktopState;
use futures_util::stream::StreamExt;

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
    WindowResized {
        window_id: String,
        width: u32,
        height: u32,
    },

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
    let (response, mut session, msg_stream) = actix_ws::handle(&req, body)?;

    tracing::info!("WebSocket connection established");

    // Clone for the spawned task
    let app_state_clone = app_state.clone();
    let sessions_clone = sessions.clone();

    // Spawn handler task to process messages and keep connection alive
    actix_web::rt::spawn(async move {
        let mut current_desktop_id: Option<String> = None;
        let mut stream = msg_stream;

        // Send initial pong to confirm connection
        let pong_msg = WsMessage::Pong;
        if let Ok(json) = serde_json::to_string(&pong_msg) {
            let _ = session.text(json).await;
        }

        // Process messages in a loop to keep connection alive
        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    tracing::debug!("WebSocket received: {}", text);

                    // Parse the message
                    match serde_json::from_str::<WsMessage>(&text) {
                        Ok(WsMessage::Ping) => {
                            // Respond with pong
                            let pong = WsMessage::Pong;
                            if let Ok(json) = serde_json::to_string(&pong) {
                                let _ = session.text(json).await;
                            }
                        }
                        Ok(WsMessage::Subscribe { desktop_id }) => {
                            tracing::info!("WebSocket subscribed to desktop: {}", desktop_id);
                            current_desktop_id = Some(desktop_id.clone());

                            // Get or create the desktop actor
                            let desktop_actor = app_state_clone
                                .actor_manager
                                .get_or_create_desktop(desktop_id.clone(), "anonymous".to_string());

                            // Get current desktop state and send it
                            match desktop_actor.send(GetDesktopState).await {
                                Ok(desktop) => {
                                    let state_msg = WsMessage::DesktopState { desktop };
                                    if let Ok(json) = serde_json::to_string(&state_msg) {
                                        let _ = session.text(json).await;
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to get desktop state: {}", e);
                                    let error_msg = WsMessage::Error {
                                        message: format!("Failed to get desktop state: {e}"),
                                    };
                                    if let Ok(json) = serde_json::to_string(&error_msg) {
                                        let _ = session.text(json).await;
                                    }
                                }
                            }

                            // Store session for broadcasting
                            subscribe_session(&sessions_clone, desktop_id, session.clone());
                        }
                        _ => {
                            tracing::warn!("Unknown or invalid WebSocket message: {}", text);
                        }
                    }
                }
                Some(Ok(Message::Ping(_data))) => {
                    // Automatic pong response by actix-ws
                    tracing::debug!("WebSocket ping received");
                }
                Some(Ok(Message::Close(_))) => {
                    tracing::info!("WebSocket close message received");
                    break;
                }
                Some(Err(e)) => {
                    tracing::error!("WebSocket error: {}", e);
                    break;
                }
                None => {
                    tracing::info!("WebSocket stream ended");
                    break;
                }
                _ => {}
            }
        }

        // Cleanup: remove from sessions when connection closes
        if let Some(desktop_id) = current_desktop_id {
            tracing::info!("WebSocket disconnected from desktop: {}", desktop_id);
        }
    });

    Ok(response)
}

/// Broadcast an event to all subscribers of a desktop
pub async fn broadcast_event(sessions: &WsSessions, desktop_id: &str, event: WsMessage) {
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
pub fn subscribe_session(sessions: &WsSessions, desktop_id: String, session: actix_ws::Session) {
    if let Ok(mut sessions) = sessions.lock() {
        sessions.entry(desktop_id).or_default().push(session);
    }
}
