//! WebSocket API for real-time desktop events
//!
//! Uses Axum WebSocket support.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::actors::desktop::DesktopActorMsg;
use crate::actors::event_store::EventStoreMsg;
use crate::api::ApiState;
use crate::app_state::AppState;
pub use shared_types::DesktopWsMessage as WsMessage;
use shared_types::WriterRunEvent;

/// Shared state for WebSocket sessions
pub type WsSessions = Arc<Mutex<HashMap<String, HashMap<Uuid, mpsc::UnboundedSender<Message>>>>>;

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

fn writer_ws_message_from_event(
    event_type: &str,
    payload: &serde_json::Value,
) -> Option<(String, WsMessage)> {
    let mut writer_event_payload = payload.clone();
    writer_event_payload.as_object_mut()?.insert(
        "event_type".to_string(),
        serde_json::Value::String(event_type.to_string()),
    );
    let writer_event: WriterRunEvent = serde_json::from_value(writer_event_payload).ok()?;

    match writer_event {
        WriterRunEvent::Started { base, objective } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunStarted { base, objective },
        )),
        WriterRunEvent::Progress {
            base,
            phase,
            message,
            progress_pct,
            source_refs,
        } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunProgress {
                base,
                phase,
                message,
                progress_pct,
                source_refs,
            },
        )),
        WriterRunEvent::Patch { base, payload } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunPatch { base, payload },
        )),
        WriterRunEvent::Changeset { base, payload } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunChangeset { base, payload },
        )),
        WriterRunEvent::Status {
            base,
            status,
            message,
        } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunStatus {
                base,
                status,
                message,
            },
        )),
        WriterRunEvent::Failed {
            base,
            error_code,
            error_message,
            failure_kind,
        } => Some((
            base.desktop_id.clone(),
            WsMessage::WriterRunFailed {
                base,
                error_code,
                error_message,
                failure_kind,
            },
        )),
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

#[cfg(test)]
mod tests {
    use super::{writer_ws_message_from_event, WsMessage};
    use serde_json::json;

    #[test]
    fn changeset_ws_message_preserves_writer_run_base_fields() {
        let payload = json!({
            "desktop_id": "desktop-1",
            "session_id": "session-1",
            "thread_id": "thread-1",
            "run_id": "run-1",
            "document_path": "conductor/runs/run-1/draft.md",
            "revision": 7,
            "head_version_id": 3,
            "timestamp": "2026-03-13T22:00:00Z",
            "patch_id": "patch-1",
            "loop_id": "loop-1",
            "target_version_id": 3,
            "source": "writer",
            "summary": "Added a tighter summary paragraph.",
            "impact": "medium",
            "op_taxonomy": ["insert", "clarification"]
        });

        let (_, message) = writer_ws_message_from_event("writer.run.changeset", &payload)
            .expect("changeset should map to websocket message");

        match message {
            WsMessage::WriterRunChangeset { base, payload } => {
                assert_eq!(base.desktop_id, "desktop-1");
                assert_eq!(base.session_id, "session-1");
                assert_eq!(base.thread_id, "thread-1");
                assert_eq!(base.run_id, "run-1");
                assert_eq!(base.document_path, "conductor/runs/run-1/draft.md");
                assert_eq!(base.revision, 7);
                assert_eq!(base.head_version_id, Some(3));
                assert_eq!(base.timestamp.to_rfc3339(), "2026-03-13T22:00:00+00:00");
                assert_eq!(payload.patch_id, "patch-1");
                assert_eq!(payload.loop_id.as_deref(), Some("loop-1"));
                assert_eq!(payload.target_version_id, Some(3));
                assert_eq!(payload.source.as_deref(), Some("writer"));
                assert_eq!(payload.summary, "Added a tighter summary paragraph.");
                assert_eq!(payload.impact, shared_types::ChangesetImpact::Medium);
                assert_eq!(payload.op_taxonomy, vec!["insert", "clarification"]);
            }
            other => panic!("unexpected websocket message: {other:?}"),
        }
    }
}
