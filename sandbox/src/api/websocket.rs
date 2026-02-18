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
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::actors::desktop::DesktopActorMsg;
use crate::actors::event_store::EventStoreMsg;
use crate::api::ApiState;
use crate::app_state::AppState;

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
        maximized: bool,
    },

    #[serde(rename = "app_registered")]
    AppRegistered { app: shared_types::AppDefinition },

    #[serde(rename = "telemetry")]
    Telemetry {
        event_type: String,
        capability: String,
        phase: String,
        importance: String,
        data: serde_json::Value,
    },

    /// Document update event for live streaming of conductor run documents
    #[serde(rename = "conductor.run.document_update")]
    DocumentUpdate {
        run_id: String,
        document_path: String,
        content_excerpt: String,
        timestamp: String,
    },

    #[serde(rename = "writer.run.started")]
    WriterRunStarted {
        desktop_id: String,
        session_id: String,
        thread_id: String,
        run_id: String,
        document_path: String,
        revision: u64,
        timestamp: String,
        objective: String,
    },

    #[serde(rename = "writer.run.progress")]
    WriterRunProgress {
        desktop_id: String,
        session_id: String,
        thread_id: String,
        run_id: String,
        document_path: String,
        revision: u64,
        timestamp: String,
        phase: String,
        message: String,
        progress_pct: Option<u8>,
    },

    #[serde(rename = "writer.run.patch")]
    WriterRunPatch {
        desktop_id: String,
        session_id: String,
        thread_id: String,
        run_id: String,
        document_path: String,
        revision: u64,
        timestamp: String,
        patch_id: String,
        source: shared_types::PatchSource,
        section_id: Option<String>,
        ops: Vec<shared_types::PatchOp>,
        proposal: Option<String>,
        base_version_id: Option<u64>,
        target_version_id: Option<u64>,
        overlay_id: Option<String>,
    },

    #[serde(rename = "writer.run.status")]
    WriterRunStatus {
        desktop_id: String,
        session_id: String,
        thread_id: String,
        run_id: String,
        document_path: String,
        revision: u64,
        timestamp: String,
        status: shared_types::WriterRunStatusKind,
        message: Option<String>,
    },

    #[serde(rename = "writer.run.failed")]
    WriterRunFailed {
        desktop_id: String,
        session_id: String,
        thread_id: String,
        run_id: String,
        document_path: String,
        revision: u64,
        timestamp: String,
        error_code: String,
        error_message: String,
        failure_kind: Option<shared_types::FailureKind>,
    },

    #[serde(rename = "writer.run.changeset")]
    WriterRunChangeset {
        desktop_id: String,
        run_id: String,
        patch_id: String,
        target_version_id: u64,
        source: String,
        summary: String,
        impact: String,
        op_taxonomy: Vec<String>,
    },

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

                        let desktop_actor = match app_state
                            .get_or_create_desktop(desktop_id.clone(), "anonymous".to_string())
                            .await
                        {
                            Ok(actor) => actor,
                            Err(e) => {
                                let _ = send_json(
                                    &tx,
                                    &WsMessage::Error {
                                        message: format!("Failed to get desktop: {e}"),
                                    },
                                );
                                continue;
                            }
                        };

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

#[derive(Debug, Deserialize)]
struct WriterRunBasePayload {
    desktop_id: String,
    session_id: String,
    thread_id: String,
    run_id: String,
    document_path: String,
    revision: u64,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct WriterRunStartedPayload {
    #[serde(flatten)]
    base: WriterRunBasePayload,
    objective: String,
}

#[derive(Debug, Deserialize)]
struct WriterRunProgressPayload {
    #[serde(flatten)]
    base: WriterRunBasePayload,
    phase: String,
    message: String,
    progress_pct: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct WriterRunPatchPayload {
    #[serde(flatten)]
    base: WriterRunBasePayload,
    patch_id: String,
    source: shared_types::PatchSource,
    section_id: Option<String>,
    ops: Vec<shared_types::PatchOp>,
    proposal: Option<String>,
    base_version_id: Option<u64>,
    target_version_id: Option<u64>,
    overlay_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriterRunStatusPayload {
    #[serde(flatten)]
    base: WriterRunBasePayload,
    status: shared_types::WriterRunStatusKind,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriterRunFailedPayload {
    #[serde(flatten)]
    base: WriterRunBasePayload,
    error_code: String,
    error_message: String,
    failure_kind: Option<shared_types::FailureKind>,
}

#[derive(Debug, Deserialize)]
struct WriterRunChangesetPayload {
    desktop_id: String,
    run_id: String,
    patch_id: String,
    target_version_id: u64,
    source: String,
    summary: String,
    impact: String,
    op_taxonomy: Vec<String>,
}

fn writer_ws_message_from_event(
    event_type: &str,
    payload: &serde_json::Value,
) -> Option<(String, WsMessage)> {
    match event_type {
        "writer.run.started" => {
            let parsed: WriterRunStartedPayload = serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.base.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunStarted {
                    desktop_id: parsed.base.desktop_id,
                    session_id: parsed.base.session_id,
                    thread_id: parsed.base.thread_id,
                    run_id: parsed.base.run_id,
                    document_path: parsed.base.document_path,
                    revision: parsed.base.revision,
                    timestamp: parsed.base.timestamp,
                    objective: parsed.objective,
                },
            ))
        }
        "writer.run.progress" => {
            let parsed: WriterRunProgressPayload = serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.base.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunProgress {
                    desktop_id: parsed.base.desktop_id,
                    session_id: parsed.base.session_id,
                    thread_id: parsed.base.thread_id,
                    run_id: parsed.base.run_id,
                    document_path: parsed.base.document_path,
                    revision: parsed.base.revision,
                    timestamp: parsed.base.timestamp,
                    phase: parsed.phase,
                    message: parsed.message,
                    progress_pct: parsed.progress_pct,
                },
            ))
        }
        "writer.run.patch" => {
            let parsed: WriterRunPatchPayload = serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.base.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunPatch {
                    desktop_id: parsed.base.desktop_id,
                    session_id: parsed.base.session_id,
                    thread_id: parsed.base.thread_id,
                    run_id: parsed.base.run_id,
                    document_path: parsed.base.document_path,
                    revision: parsed.base.revision,
                    timestamp: parsed.base.timestamp,
                    patch_id: parsed.patch_id,
                    source: parsed.source,
                    section_id: parsed.section_id,
                    ops: parsed.ops,
                    proposal: parsed.proposal,
                    base_version_id: parsed.base_version_id,
                    target_version_id: parsed.target_version_id,
                    overlay_id: parsed.overlay_id,
                },
            ))
        }
        "writer.run.status" => {
            let parsed: WriterRunStatusPayload = serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.base.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunStatus {
                    desktop_id: parsed.base.desktop_id,
                    session_id: parsed.base.session_id,
                    thread_id: parsed.base.thread_id,
                    run_id: parsed.base.run_id,
                    document_path: parsed.base.document_path,
                    revision: parsed.base.revision,
                    timestamp: parsed.base.timestamp,
                    status: parsed.status,
                    message: parsed.message,
                },
            ))
        }
        "writer.run.failed" => {
            let parsed: WriterRunFailedPayload = serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.base.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunFailed {
                    desktop_id: parsed.base.desktop_id,
                    session_id: parsed.base.session_id,
                    thread_id: parsed.base.thread_id,
                    run_id: parsed.base.run_id,
                    document_path: parsed.base.document_path,
                    revision: parsed.base.revision,
                    timestamp: parsed.base.timestamp,
                    error_code: parsed.error_code,
                    error_message: parsed.error_message,
                    failure_kind: parsed.failure_kind,
                },
            ))
        }
        "writer.run.changeset" => {
            let parsed: WriterRunChangesetPayload =
                serde_json::from_value(payload.clone()).ok()?;
            let desktop_id = parsed.desktop_id.clone();
            Some((
                desktop_id,
                WsMessage::WriterRunChangeset {
                    desktop_id: parsed.desktop_id,
                    run_id: parsed.run_id,
                    patch_id: parsed.patch_id,
                    target_version_id: parsed.target_version_id,
                    source: parsed.source,
                    summary: parsed.summary,
                    impact: parsed.impact,
                    op_taxonomy: parsed.op_taxonomy,
                },
            ))
        }
        _ => None,
    }
}

pub fn spawn_writer_run_event_forwarder(
    event_store: ractor::ActorRef<EventStoreMsg>,
    sessions: WsSessions,
) {
    tokio::spawn(async move {
        let mut since_seq =
            match ractor::call!(event_store, |reply| EventStoreMsg::GetLatestSeq { reply }) {
                Ok(Ok(Some(seq))) => seq,
                Ok(Ok(None)) => 0,
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, "writer run forwarder failed to read latest seq");
                    0
                }
                Err(err) => {
                    tracing::warn!(error = %err, "writer run forwarder failed to query latest seq");
                    0
                }
            };

        let mut ticker = tokio::time::interval(Duration::from_millis(120));
        loop {
            ticker.tick().await;

            let events = match ractor::call!(event_store, |reply| EventStoreMsg::GetRecentEvents {
                since_seq,
                limit: 250,
                event_type_prefix: Some("writer.run.".to_string()),
                actor_id: None,
                user_id: None,
                reply
            }) {
                Ok(Ok(events)) => events,
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, "writer run forwarder query failed");
                    continue;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "writer run forwarder rpc failed");
                    continue;
                }
            };

            for event in events {
                since_seq = since_seq.max(event.seq);
                if let Some((desktop_id, message)) =
                    writer_ws_message_from_event(&event.event_type, &event.payload)
                {
                    broadcast_event(&sessions, &desktop_id, message).await;
                }
            }
        }
    });
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
