//! Chat API endpoints with ActorManager
//!
//! PREDICTION: HTTP endpoints can use ActorManager to get persistent actor
//! instances, enabling multiturn chat with history preservation.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::{ChatActorMsg, ChatAgentMsg};
use crate::actors::event_store::get_events_for_actor;
use crate::api::ApiState;

/// Request to send a chat message
#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub actor_id: String,
    pub user_id: String,
    pub text: String,
}

/// Response after sending a message
#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub success: bool,
    pub temp_id: String,
    pub message: String,
}

/// Send a message to a chat actor
pub async fn send_message(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();
    let actor_id = req.actor_id.clone();
    let user_id = req.user_id.clone();
    let text = req.text.clone();

    // Get or create persistent ChatActor via Manager
    let chat_actor = app_state
        .actor_manager
        .get_or_create_chat(actor_id.clone(), user_id.clone()).await;

    // Send the message (optimistic) using ractor call pattern
    match ractor::call!(
        chat_actor,
        |reply| ChatActorMsg::SendUserMessage {
            text: text.clone(),
            reply,
        }
    ) {
        Ok(Ok(temp_id)) => {
            // Persist the user message to EventStore immediately
            let event_store = app_state.actor_manager.event_store();
            let actor_id_for_event = actor_id.clone();
            let user_id_for_event = user_id.clone();
            let text_for_event = text.clone();
            
            // Spawn async task to persist the event (fire-and-forget)
            tokio::spawn(async move {
                use crate::actors::event_store::{AppendEvent, EventStoreMsg};
                
                let append_event = AppendEvent {
                    event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                    payload: serde_json::json!(text_for_event),
                    actor_id: actor_id_for_event.clone(),
                    user_id: user_id_for_event.clone(),
                };
                
                match ractor::call!(
                    event_store,
                    |reply| EventStoreMsg::Append {
                        event: append_event,
                        reply,
                    }
                ) {
                    Ok(Ok(event)) => {
                        tracing::info!(
                            actor_id = %actor_id_for_event,
                            seq = event.seq,
                            "User message persisted to EventStore"
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::error!(
                            actor_id = %actor_id_for_event,
                            error = %e,
                            "Failed to persist user message to EventStore"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            actor_id = %actor_id_for_event,
                            error = %e,
                            "EventStore actor error when persisting message"
                        );
                    }
                }
            });
            
            // Trigger ChatAgent to process the message and generate response (fire and forget)
            // Note: ChatAgent will log the assistant response to EventStore
            let chat_agent = app_state
                .actor_manager
                .get_or_create_chat_agent(actor_id.clone(), user_id.clone()).await;
            let text_for_agent = text.clone();
            
            // Spawn async task for fire-and-forget processing
            tokio::spawn(async move {
                match ractor::call!(
                    chat_agent,
                    |reply| ChatAgentMsg::ProcessMessage {
                        text: text_for_agent,
                        reply,
                    }
                ) {
                    Ok(Ok(response)) => {
                        tracing::info!(
                            actor_id = %actor_id,
                            response_preview = %response.text.chars().take(50).collect::<String>(),
                            "ChatAgent processed message successfully"
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::error!(
                            actor_id = %actor_id,
                            error = %e,
                            "ChatAgent failed to process message"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            actor_id = %actor_id,
                            error = %e,
                            "ChatAgent actor error"
                        );
                    }
                }
            });

            (
                StatusCode::OK,
                Json(SendMessageResponse {
                    success: true,
                    temp_id,
                    message: "Message sent".to_string(),
                }),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Get messages for a chat actor
pub async fn get_messages(
    Path(actor_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();

    // Query EventStore directly for chat events using ractor
    let event_store = app_state.actor_manager.event_store();

    match get_events_for_actor(&event_store, actor_id.clone(), 0).await {
        Ok(Ok(events)) => {
            // Convert events to ChatMessages
            let messages: Vec<shared_types::ChatMessage> = events
                .into_iter()
                .filter_map(|event| match event.event_type.as_str() {
                    shared_types::EVENT_CHAT_USER_MSG => {
                        if let Ok(text) = serde_json::from_value::<String>(event.payload.clone()) {
                            Some(shared_types::ChatMessage {
                                id: event.event_id.clone(),
                                text,
                                sender: shared_types::Sender::User,
                                timestamp: event.timestamp,
                                pending: false,
                            })
                        } else {
                            None
                        }
                    }
                    shared_types::EVENT_CHAT_ASSISTANT_MSG => {
                        if let Ok(payload) =
                            serde_json::from_value::<serde_json::Value>(event.payload.clone())
                        {
                            let text = payload
                                .get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(shared_types::ChatMessage {
                                id: event.event_id.clone(),
                                text,
                                sender: shared_types::Sender::Assistant,
                                timestamp: event.timestamp,
                                pending: false,
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "messages": messages
                })),
            )
                .into_response()
        }
        Ok(Err(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "EventStore error"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to get messages: {}", e)
            })),
        )
            .into_response(),
    }
}
