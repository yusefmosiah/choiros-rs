//! WebSocket handler for streaming chat responses
//!
//! Provides real-time streaming of agent thinking, tool calls, and responses
//! using WebSocket connections.

use actix::{Actor, ActorContext, ActorFutureExt, AsyncContext, StreamHandler, WrapFuture};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::AppState;
use crate::actors::chat_agent::{ChatAgent, ProcessMessage};

/// WebSocket actor for chat sessions with streaming responses
pub struct ChatWebSocket {
    actor_id: String,
    user_id: String,
    chat_agent: Option<actix::Addr<ChatAgent>>,
    app_state: web::Data<AppState>,
}

impl ChatWebSocket {
    pub fn new(actor_id: String, user_id: String, app_state: web::Data<AppState>) -> Self {
        Self {
            actor_id,
            user_id,
            chat_agent: None,
            app_state,
        }
    }

    /// Initialize the chat agent
    fn init_agent(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        let actor_id = self.actor_id.clone();
        let user_id = self.user_id.clone();

        // Get or create ChatAgent via the ActorManager
        let agent_addr = self
            .app_state
            .actor_manager
            .get_or_create_chat_agent(actor_id.clone(), user_id.clone());

        self.chat_agent = Some(agent_addr);

        // Send connection confirmation
        ctx.text(
            json!({
                "type": "connected",
                "actor_id": self.actor_id,
                "user_id": self.user_id,
            })
            .to_string(),
        );
    }

    /// Send a stream chunk to the client
    fn send_chunk(&self, chunk: StreamChunk, ctx: &mut ws::WebsocketContext<Self>) {
        let msg = json!({
            "type": chunk.chunk_type,
            "content": chunk.content,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        ctx.text(msg.to_string());
    }
}

impl Actor for ChatWebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!(
            actor_id = %self.actor_id,
            user_id = %self.user_id,
            "ChatWebSocket connection started"
        );
        self.init_agent(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(
            actor_id = %self.actor_id,
            "ChatWebSocket connection closed"
        );
    }
}

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

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Message { text: user_text }) => {
                        if let Some(agent) = self.chat_agent.clone() {
                            let actor_id = self.actor_id.clone();

                            // Send thinking start
                            self.send_chunk(
                                StreamChunk {
                                    chunk_type: "thinking".to_string(),
                                    content: "Processing your message...".to_string(),
                                },
                                ctx,
                            );

                            // Process message asynchronously
                            let fut = agent.send(ProcessMessage { text: user_text });

                            ctx.spawn(
                                async move {
                                    match fut.await {
                                        Ok(Ok(response)) => Some(response),
                                        Ok(Err(e)) => {
                                            tracing::error!(
                                                actor_id = %actor_id,
                                                error = %e,
                                                "Message processing failed"
                                            );
                                            None
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                actor_id = %actor_id,
                                                error = %e,
                                                "Actor mailbox error"
                                            );
                                            None
                                        }
                                    }
                                }
                                .into_actor(self)
                                .map(|response, actor, ctx| {
                                    if let Some(resp) = response {
                                        // Send thinking
                                        actor.send_chunk(
                                            StreamChunk {
                                                chunk_type: "thinking".to_string(),
                                                content: resp.thinking,
                                            },
                                            ctx,
                                        );

                                        // Send tool calls
                                        for tool in &resp.tool_calls {
                                            actor.send_chunk(
                                                StreamChunk {
                                                    chunk_type: "tool_call".to_string(),
                                                    content: json!({
                                                        "tool_name": tool.tool_name,
                                                        "tool_args": tool.tool_args,
                                                        "reasoning": tool.reasoning,
                                                    }).to_string(),
                                                },
                                                ctx,
                                            );

                                            actor.send_chunk(
                                                StreamChunk {
                                                    chunk_type: "tool_result".to_string(),
                                                    content: json!({
                                                        "tool_name": tool.tool_name,
                                                        "success": tool.result.success,
                                                        "output": &tool.result.content[..tool.result.content.len().min(500)],
                                                    }).to_string(),
                                                },
                                                ctx,
                                            );
                                        }

                                        // Send final response
                                        actor.send_chunk(
                                            StreamChunk {
                                                chunk_type: "response".to_string(),
                                                content: json!({
                                                    "text": resp.text,
                                                    "confidence": resp.confidence,
                                                    "model_used": resp.model_used,
                                                }).to_string(),
                                            },
                                            ctx,
                                        );
                                    } else {
                                        actor.send_chunk(
                                            StreamChunk {
                                                chunk_type: "error".to_string(),
                                                content: "Failed to process message".to_string(),
                                            },
                                            ctx,
                                        );
                                    }
                                }),
                            );
                        }
                    }
                    Ok(ClientMessage::Ping) => {
                        ctx.text(json!({"type": "pong"}).to_string());
                    }
                    Ok(ClientMessage::SwitchModel { model }) => {
                        // Handle model switching
                        ctx.text(
                            json!({
                                "type": "model_switched",
                                "model": model,
                                "status": "success"
                            })
                            .to_string(),
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Invalid WebSocket message: {}", e);
                        ctx.text(
                            json!({
                                "type": "error",
                                "message": "Invalid message format"
                            })
                            .to_string(),
                        );
                    }
                }
            }
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Close(reason)) => {
                tracing::info!(
                    actor_id = %self.actor_id,
                    reason = ?reason,
                    "WebSocket closing"
                );
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// WebSocket connection handler for /ws/chat/{actor_id}
pub async fn chat_websocket(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let actor_id = path.into_inner();
    let user_id = query
        .get("user_id")
        .cloned()
        .unwrap_or_else(|| "anonymous".to_string());

    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        "New chat WebSocket connection"
    );

    ws::start(ChatWebSocket::new(actor_id, user_id, data), &req, stream)
}

/// WebSocket connection handler for /ws/chat/{actor_id}/{user_id}
pub async fn chat_websocket_with_user(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (actor_id, user_id) = path.into_inner();

    tracing::info!(
        actor_id = %actor_id,
        user_id = %user_id,
        "New chat WebSocket connection"
    );

    ws::start(ChatWebSocket::new(actor_id, user_id, data), &req, stream)
}
