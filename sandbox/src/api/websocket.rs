//! WebSocket API for real-time desktop events
//!
//! Uses Axum WebSocket support.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::actor_manager::{AppState, DesktopActorMsg};
use crate::api::ApiState;

/// Shared state for WebSocket sessions
pub type WsSessions = Arc<Mutex<HashMap<String, HashMap<Uuid, mpsc::UnboundedSender<Message>>>>>;

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
        width: i32,
        height: i32,
    },

    #[serde(rename = "window_focused")]
    WindowFocused { window_id: String, z_index: u32 },

    #[serde(rename = "window_minimized")]
    WindowMinimized { window_id: String },

    #[serde(rename = "window_maximized")]
    WindowMaximized {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },

    #[serde(rename = "window_restored")]
    WindowRestored {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        from: String,
    },

    #[serde(rename = "app_registered")]
    AppRegistered { app: shared_types::AppDefinition },

    #[serde(rename = "error")]
    Error { message: String },
}

/// WebSocket handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<ApiState>) -> impl IntoResponse {
    let app_state = state.app_state.clone();
    let sessions = state.ws_sessions.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, app_state, sessions))
}

async fn handle_socket(socket: WebSocket, app_state: Arc<AppState>, sessions: WsSessions) {
    tracing::info!("WebSocket connection established");

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let _ = send_json(&tx, &WsMessage::Pong);

    let mut current_desktop_id: Option<String> = None;
    let session_id = Uuid::new_v4();

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                tracing::debug!("WebSocket received: {}", text);

                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(WsMessage::Ping) => {
                        let _ = send_json(&tx, &WsMessage::Pong);
                    }
                    Ok(WsMessage::Subscribe { desktop_id }) => {
                        tracing::info!("WebSocket subscribed to desktop: {}", desktop_id);

                        if let Some(prev_desktop_id) = current_desktop_id.take() {
                            unsubscribe_session(&sessions, &prev_desktop_id, session_id).await;
                        }

                        current_desktop_id = Some(desktop_id.clone());

                        let desktop_actor = app_state
                            .actor_manager
                            .get_or_create_desktop(desktop_id.clone(), "anonymous".to_string())
                            .await;

                        match ractor::call!(desktop_actor, |reply| {
                            DesktopActorMsg::GetDesktopState { reply }
                        }) {
                            Ok(desktop) => {
                                let _ = send_json(&tx, &WsMessage::DesktopState { desktop });
                            }
                            Err(e) => {
                                tracing::error!("Failed to get desktop state: {}", e);
                                let _ = send_json(
                                    &tx,
                                    &WsMessage::Error {
                                        message: format!("Failed to get desktop state: {e}"),
                                    },
                                );
                            }
                        }

                        subscribe_session(&sessions, &desktop_id, session_id, tx.clone()).await;
                    }
                    _ => {
                        tracing::warn!("Unknown or invalid WebSocket message: {}", text);
                    }
                }
            }
            Message::Ping(data) => {
                let _ = tx.send(Message::Pong(data));
            }
            Message::Close(_) => {
                tracing::info!("WebSocket close message received");
                break;
            }
            _ => {}
        }
    }

    if let Some(desktop_id) = current_desktop_id {
        tracing::info!("WebSocket disconnected from desktop: {}", desktop_id);
        unsubscribe_session(&sessions, &desktop_id, session_id).await;
    }

    writer.abort();
}

/// Broadcast an event to all subscribers of a desktop
#[allow(dead_code)]
pub async fn broadcast_event(sessions: &WsSessions, desktop_id: &str, event: WsMessage) {
    let json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize WS message: {}", e);
            return;
        }
    };

    let mut sessions = sessions.lock().await;
    if let Some(subscribers) = sessions.get_mut(desktop_id) {
        subscribers.retain(|_, sender| sender.send(Message::Text(json.clone().into())).is_ok());
    }
}

/// Subscribe a session to a desktop
pub async fn subscribe_session(
    sessions: &WsSessions,
    desktop_id: &str,
    session_id: Uuid,
    sender: mpsc::UnboundedSender<Message>,
) {
    let mut sessions = sessions.lock().await;
    sessions
        .entry(desktop_id.to_string())
        .or_default()
        .insert(session_id, sender);
}

/// Remove a session from a desktop
pub async fn unsubscribe_session(sessions: &WsSessions, desktop_id: &str, session_id: Uuid) {
    let mut sessions = sessions.lock().await;
    if let Some(subscribers) = sessions.get_mut(desktop_id) {
        subscribers.remove(&session_id);
        if subscribers.is_empty() {
            sessions.remove(desktop_id);
        }
    }
}

fn send_json(tx: &mpsc::UnboundedSender<Message>, msg: &WsMessage) -> bool {
    match serde_json::to_string(msg) {
        Ok(text) => tx.send(Message::Text(text.into())).is_ok(),
        Err(e) => {
            tracing::error!("Failed to serialize WS message: {}", e);
            false
        }
    }
}
