//! WebSocket handler for streaming chat responses
//!
//! Provides real-time streaming of agent thinking, tool calls, and responses
//! using WebSocket connections.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};

use crate::actors::chat_agent::ChatAgentMsg;
use crate::actors::event_store::{get_events_for_actor_with_scope, EventStoreMsg};
use crate::api::ApiState;
use crate::app_state::AppState;

/// Stream chunk types for WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub chunk_type: String,
    pub content: String,
}

/// Incoming WebSocket messages
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "message")]
    Message {
        text: String,
        #[serde(default)]
        client_message_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },

    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "switch_model")]
    SwitchModel { model: String },
}

/// Outgoing WebSocket messages
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum ServerMessage {
    #[serde(rename = "thinking")]
    Thinking { content: String },

    #[serde(rename = "tool_call")]
    ToolCall {
        tool_name: String,
        tool_args: String,
        reasoning: String,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_name: String,
        success: bool,
        output: String,
    },

    #[serde(rename = "response")]
    Response {
        text: String,
        confidence: f64,
        model_used: String,
    },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "connected")]
    Connected {
        actor_id: String,
        user_id: String,
        session_id: String,
        thread_id: String,
    },
}

/// WebSocket connection handler for /ws/chat/{actor_id}
pub async fn chat_websocket(
    ws: WebSocketUpgrade,
    Path(actor_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let user_id = query
        .get("user_id")
        .cloned()
        .unwrap_or_else(|| "anonymous".to_string());
    let session_id = query
        .get("session_id")
        .cloned()
        .unwrap_or_else(|| format!("session:{actor_id}"));
    let thread_id = query
        .get("thread_id")
        .cloned()
        .unwrap_or_else(|| format!("thread:{actor_id}"));

    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        session_id = %session_id,
        thread_id = %thread_id,
        "New chat WebSocket connection"
    );

    let app_state = state.app_state.clone();
    ws.on_upgrade(move |socket| {
        handle_chat_socket(socket, app_state, actor_id, user_id, session_id, thread_id)
    })
}

/// WebSocket connection handler for /ws/chat/{actor_id}/{user_id}
pub async fn chat_websocket_with_user(
    ws: WebSocketUpgrade,
    Path((actor_id, user_id)): Path<(String, String)>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let session_id = format!("session:{actor_id}");
    let thread_id = format!("thread:{actor_id}");
    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        session_id = %session_id,
        thread_id = %thread_id,
        "New chat WebSocket connection"
    );

    let app_state = state.app_state.clone();
    ws.on_upgrade(move |socket| {
        handle_chat_socket(socket, app_state, actor_id, user_id, session_id, thread_id)
    })
}

async fn handle_chat_socket(
    socket: WebSocket,
    app_state: Arc<AppState>,
    actor_id: String,
    user_id: String,
    session_id: String,
    thread_id: String,
) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let _ = tx.send(Message::Text(
        json!({
            "type": "connected",
            "actor_id": actor_id,
            "user_id": user_id,
            "session_id": session_id,
            "thread_id": thread_id,
        })
        .to_string()
        .into(),
    ));

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Message {
                    text: user_text,
                    client_message_id,
                    model,
                }) => {
                    let _ = send_chunk(
                        &tx,
                        StreamChunk {
                            chunk_type: "thinking".to_string(),
                            content: "Processing your message...".to_string(),
                        },
                    );

                    let tx_clone = tx.clone();
                    let app_state = app_state.clone();
                    let actor_id = actor_id.clone();
                    let user_id = user_id.clone();
                    let session_id = session_id.clone();
                    let thread_id = thread_id.clone();
                    let client_message_id = client_message_id.clone();
                    let model_override = model.clone();

                    tokio::spawn(async move {
                        let event_store = app_state.event_store();
                        let append_user_event = crate::actors::event_store::AppendEvent {
                            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                            payload: shared_types::chat_user_payload(
                                user_text.clone(),
                                Some(session_id.clone()),
                                Some(thread_id.clone()),
                            ),
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                        };

                        match ractor::call!(event_store, |reply| {
                            crate::actors::event_store::EventStoreMsg::Append {
                                event: append_user_event,
                                reply,
                            }
                        }) {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "Failed to persist WebSocket user message"
                                );
                                let _ = send_error(&tx_clone, "Failed to persist user message");
                                return;
                            }
                            Err(e) => {
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "EventStore actor error while persisting WebSocket user message"
                                );
                                let _ = send_error(&tx_clone, "Failed to persist user message");
                                return;
                            }
                        }

                        let last_seq = match get_events_for_actor_with_scope(
                            &event_store,
                            actor_id.clone(),
                            session_id.clone(),
                            thread_id.clone(),
                            0,
                        )
                        .await
                        {
                            Ok(Ok(events)) => events.last().map(|e| e.seq).unwrap_or(0),
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    actor_id = %actor_id,
                                    session_id = %session_id,
                                    thread_id = %thread_id,
                                    error = %e,
                                    "Failed to get scoped event cursor for tool streaming"
                                );
                                0
                            }
                            Err(e) => {
                                tracing::warn!(
                                    actor_id = %actor_id,
                                    session_id = %session_id,
                                    thread_id = %thread_id,
                                    error = %e,
                                    "EventStore actor error while preparing scoped tool streaming"
                                );
                                0
                            }
                        };

                        let (stream_done_tx, stream_done_rx) = oneshot::channel::<()>();
                        let stream_task = tokio::spawn(stream_tool_events(
                            tx_clone.clone(),
                            event_store.clone(),
                            actor_id.clone(),
                            session_id.clone(),
                            thread_id.clone(),
                            last_seq,
                            stream_done_rx,
                        ));

                        let agent = match app_state
                            .get_or_create_chat_agent(
                                scoped_agent_id(
                                    &actor_id,
                                    &Some(session_id.clone()),
                                    &Some(thread_id.clone()),
                                ),
                                actor_id.clone(),
                                user_id.clone(),
                                Some(session_id.clone()),
                                Some(thread_id.clone()),
                            )
                            .await
                        {
                            Ok(agent) => agent,
                            Err(e) => {
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "Failed to get chat agent"
                                );
                                let _ = send_error(&tx_clone, "Failed to initialize chat agent");
                                let _ = stream_done_tx.send(());
                                let _ = stream_task.await;
                                return;
                            }
                        };

                        match ractor::call!(agent, |reply| ChatAgentMsg::ProcessMessage {
                            text: user_text,
                            session_id: Some(session_id),
                            thread_id: Some(thread_id),
                            model_override,
                            reply,
                        }) {
                            Ok(Ok(resp)) => {
                                let _ = stream_done_tx.send(());
                                let _ = stream_task.await;

                                let _ = send_chunk(
                                    &tx_clone,
                                    StreamChunk {
                                        chunk_type: "thinking".to_string(),
                                        content: resp.thinking,
                                    },
                                );

                                let _ = send_chunk(
                                    &tx_clone,
                                    StreamChunk {
                                        chunk_type: "response".to_string(),
                                        content: json!({
                                            "text": resp.text,
                                            "confidence": resp.confidence,
                                            "model_used": resp.model_used,
                                            "client_message_id": client_message_id,
                                        })
                                        .to_string(),
                                    },
                                );
                            }
                            Ok(Err(e)) => {
                                let _ = stream_done_tx.send(());
                                let _ = stream_task.await;
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "Message processing failed"
                                );
                                let _ = send_error(&tx_clone, "Failed to process message");
                            }
                            Err(e) => {
                                let _ = stream_done_tx.send(());
                                let _ = stream_task.await;
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "Actor error"
                                );
                                let _ = send_error(&tx_clone, "Failed to process message");
                            }
                        }
                    });
                }
                Ok(ClientMessage::Ping) => {
                    let _ = tx.send(Message::Text(json!({"type": "pong"}).to_string().into()));
                }
                Ok(ClientMessage::SwitchModel { model }) => {
                    let tx_clone = tx.clone();
                    let app_state = app_state.clone();
                    let actor_id = actor_id.clone();
                    let user_id = user_id.clone();
                    let session_id = session_id.clone();
                    let thread_id = thread_id.clone();

                    tokio::spawn(async move {
                        let agent = match app_state
                            .get_or_create_chat_agent(
                                scoped_agent_id(
                                    &actor_id,
                                    &Some(session_id.clone()),
                                    &Some(thread_id.clone()),
                                ),
                                actor_id.clone(),
                                user_id.clone(),
                                Some(session_id.clone()),
                                Some(thread_id.clone()),
                            )
                            .await
                        {
                            Ok(agent) => agent,
                            Err(e) => {
                                let _ = tx_clone.send(Message::Text(
                                    json!({
                                        "type": "error",
                                        "message": format!("Model switch failed: {e}")
                                    })
                                    .to_string()
                                    .into(),
                                ));
                                return;
                            }
                        };

                        match ractor::call!(agent, |reply| ChatAgentMsg::SwitchModel {
                            model: model.clone(),
                            reply,
                        }) {
                            Ok(Ok(())) => {
                                let _ = tx_clone.send(Message::Text(
                                    json!({
                                        "type": "model_switched",
                                        "model": model,
                                        "status": "success"
                                    })
                                    .to_string()
                                    .into(),
                                ));
                            }
                            Ok(Err(e)) => {
                                let _ = tx_clone.send(Message::Text(
                                    json!({
                                        "type": "error",
                                        "message": e.to_string()
                                    })
                                    .to_string()
                                    .into(),
                                ));
                            }
                            Err(e) => {
                                let _ = tx_clone.send(Message::Text(
                                    json!({
                                        "type": "error",
                                        "message": format!("Model switch failed: {e}")
                                    })
                                    .to_string()
                                    .into(),
                                ));
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("Invalid WebSocket message: {}", e);
                    let _ = tx.send(Message::Text(
                        json!({
                            "type": "error",
                            "message": "Invalid message format"
                        })
                        .to_string()
                        .into(),
                    ));
                }
            },
            Message::Ping(data) => {
                let _ = tx.send(Message::Pong(data));
            }
            Message::Close(reason) => {
                tracing::info!(
                    actor_id = %actor_id,
                    reason = ?reason,
                    "WebSocket closing"
                );
                break;
            }
            _ => {}
        }
    }

    writer.abort();
}

fn scoped_agent_id(
    actor_id: &str,
    session_id: &Option<String>,
    thread_id: &Option<String>,
) -> String {
    match (session_id, thread_id) {
        (Some(session_id), Some(thread_id)) => {
            format!("{actor_id}::session={session_id}::thread={thread_id}")
        }
        _ => actor_id.to_string(),
    }
}

fn send_chunk(tx: &mpsc::UnboundedSender<Message>, chunk: StreamChunk) -> bool {
    let msg = json!({
        "type": chunk.chunk_type,
        "content": chunk.content,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    tx.send(Message::Text(msg.to_string().into())).is_ok()
}

fn send_error(tx: &mpsc::UnboundedSender<Message>, message: &str) -> bool {
    tx.send(Message::Text(
        json!({
            "type": "error",
            "message": message
        })
        .to_string()
        .into(),
    ))
    .is_ok()
}

async fn stream_tool_events(
    tx: mpsc::UnboundedSender<Message>,
    event_store: ractor::ActorRef<EventStoreMsg>,
    actor_id: String,
    session_id: String,
    thread_id: String,
    mut since_seq: i64,
    mut done: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = sleep(Duration::from_millis(120)) => {
                if !emit_tool_events_since(
                    &tx,
                    &event_store,
                    &actor_id,
                    &session_id,
                    &thread_id,
                    &mut since_seq,
                ).await {
                    return;
                }
            }
            _ = &mut done => {
                let _ = emit_tool_events_since(
                    &tx,
                    &event_store,
                    &actor_id,
                    &session_id,
                    &thread_id,
                    &mut since_seq,
                ).await;
                return;
            }
        }
    }
}

async fn emit_tool_events_since(
    tx: &mpsc::UnboundedSender<Message>,
    event_store: &ractor::ActorRef<EventStoreMsg>,
    actor_id: &str,
    session_id: &str,
    thread_id: &str,
    since_seq: &mut i64,
) -> bool {
    let events = match get_events_for_actor_with_scope(
        event_store,
        actor_id.to_string(),
        session_id.to_string(),
        thread_id.to_string(),
        *since_seq,
    )
    .await
    {
        Ok(Ok(events)) => events,
        Ok(Err(e)) => {
            tracing::warn!(
                actor_id = %actor_id,
                session_id = %session_id,
                thread_id = %thread_id,
                error = %e,
                "Failed to fetch scoped incremental events for tool streaming"
            );
            return true;
        }
        Err(e) => {
            tracing::warn!(
                actor_id = %actor_id,
                session_id = %session_id,
                thread_id = %thread_id,
                error = %e,
                "EventStore actor error while fetching scoped incremental tool events"
            );
            return true;
        }
    };

    for event in events {
        *since_seq = (*since_seq).max(event.seq);

        match event.event_type.as_str() {
            shared_types::EVENT_CHAT_TOOL_CALL => {
                let _ = send_chunk(
                    tx,
                    StreamChunk {
                        chunk_type: "tool_call".to_string(),
                        content: event.payload.to_string(),
                    },
                );
            }
            shared_types::EVENT_CHAT_TOOL_RESULT => {
                let _ = send_chunk(
                    tx,
                    StreamChunk {
                        chunk_type: "tool_result".to_string(),
                        content: event.payload.to_string(),
                    },
                );
            }
            "worker_spawned" | "worker_progress" | "worker_complete" | "worker_failed" => {
                let payload_with_event_type = match event.payload {
                    serde_json::Value::Object(mut obj) => {
                        obj.insert(
                            "event_type".to_string(),
                            serde_json::Value::String(event.event_type.clone()),
                        );
                        serde_json::Value::Object(obj)
                    }
                    other => serde_json::json!({
                        "value": other,
                        "event_type": event.event_type,
                    }),
                };
                let _ = send_chunk(
                    tx,
                    StreamChunk {
                        chunk_type: "actor_call".to_string(),
                        content: payload_with_event_type.to_string(),
                    },
                );
            }
            _ => {}
        }
    }

    true
}
