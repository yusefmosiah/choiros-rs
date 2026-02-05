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

use crate::actor_manager::{AppState, ChatAgentMsg};
use crate::api::ApiState;

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
    Message { text: String },

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
    Connected { actor_id: String, user_id: String },
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

    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        "New chat WebSocket connection"
    );

    let app_state = state.app_state.clone();
    ws.on_upgrade(move |socket| handle_chat_socket(socket, app_state, actor_id, user_id))
}

/// WebSocket connection handler for /ws/chat/{actor_id}/{user_id}
pub async fn chat_websocket_with_user(
    ws: WebSocketUpgrade,
    Path((actor_id, user_id)): Path<(String, String)>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        "New chat WebSocket connection"
    );

    let app_state = state.app_state.clone();
    ws.on_upgrade(move |socket| handle_chat_socket(socket, app_state, actor_id, user_id))
}

async fn handle_chat_socket(
    socket: WebSocket,
    app_state: Arc<AppState>,
    actor_id: String,
    user_id: String,
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
        })
        .to_string()
        .into(),
    ));

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Message { text: user_text }) => {
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

                    tokio::spawn(async move {
                        let event_store = app_state.actor_manager.event_store();
                        let append_user_event = crate::actors::event_store::AppendEvent {
                            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                            payload: serde_json::json!(user_text.clone()),
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

                        let agent = app_state
                            .actor_manager
                            .get_or_create_chat_agent(actor_id.clone(), user_id.clone())
                            .await;

                        match ractor::call!(agent, |reply| ChatAgentMsg::ProcessMessage {
                            text: user_text,
                            reply,
                        }) {
                            Ok(Ok(resp)) => {
                                let _ = send_chunk(
                                    &tx_clone,
                                    StreamChunk {
                                        chunk_type: "thinking".to_string(),
                                        content: resp.thinking,
                                    },
                                );

                                for tool in &resp.tool_calls {
                                    let _ = send_chunk(
                                        &tx_clone,
                                        StreamChunk {
                                            chunk_type: "tool_call".to_string(),
                                            content: json!({
                                                "tool_name": tool.tool_name,
                                                "tool_args": tool.tool_args,
                                                "reasoning": tool.reasoning,
                                            })
                                            .to_string(),
                                        },
                                    );

                                    let output =
                                        &tool.result.content[..tool.result.content.len().min(500)];
                                    let _ = send_chunk(
                                        &tx_clone,
                                        StreamChunk {
                                            chunk_type: "tool_result".to_string(),
                                            content: json!({
                                                "tool_name": tool.tool_name,
                                                "success": tool.result.success,
                                                "output": output,
                                            })
                                            .to_string(),
                                        },
                                    );
                                }

                                let _ = send_chunk(
                                    &tx_clone,
                                    StreamChunk {
                                        chunk_type: "response".to_string(),
                                        content: json!({
                                            "text": resp.text,
                                            "confidence": resp.confidence,
                                            "model_used": resp.model_used,
                                        })
                                        .to_string(),
                                    },
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::error!(
                                    actor_id = %actor_id,
                                    error = %e,
                                    "Message processing failed"
                                );
                                let _ = send_error(&tx_clone, "Failed to process message");
                            }
                            Err(e) => {
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

                    tokio::spawn(async move {
                        let agent = app_state
                            .actor_manager
                            .get_or_create_chat_agent(actor_id.clone(), user_id.clone())
                            .await;

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
