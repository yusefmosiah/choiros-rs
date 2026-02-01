//! Chat API endpoints with ActorManager
//!
//! PREDICTION: HTTP endpoints can use ActorManager to get persistent actor
//! instances, enabling multiturn chat with history preservation.

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::AppState;
use crate::actors::chat::SendUserMessage;
use crate::actors::chat_agent::ProcessMessage;
use crate::actors::event_store::GetEventsForActor;

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
#[post("/chat/send")]
pub async fn send_message(
    req: web::Json<SendMessageRequest>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let actor_id = req.actor_id.clone();
    let user_id = req.user_id.clone();
    let text = req.text.clone();

    // Get or create persistent ChatActor via Manager
    let chat_actor = state
        .actor_manager
        .get_or_create_chat(actor_id.clone(), user_id.clone());

    // Send the message (optimistic)
    match chat_actor
        .send(SendUserMessage { text: text.clone() })
        .await
    {
        Ok(Ok(temp_id)) => {
            // Trigger ChatAgent to process the message and generate response (fire and forget)
            // Note: ChatAgent will log both the user message and assistant response to EventStore
            let chat_agent = state
                .actor_manager
                .get_or_create_chat_agent(actor_id.clone(), user_id.clone());
            let text_for_agent = text.clone();
            actix::spawn(async move {
                match chat_agent
                    .send(ProcessMessage {
                        text: text_for_agent,
                    })
                    .await
                {
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
                            "ChatAgent mailbox error"
                        );
                    }
                }
            });

            HttpResponse::Ok().json(SendMessageResponse {
                success: true,
                temp_id,
                message: "Message sent".to_string(),
            })
        }
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": e.to_string()
        })),
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": "Actor mailbox error"
        })),
    }
}

/// Get messages for a chat actor
#[get("/chat/{actor_id}/messages")]
pub async fn get_messages(path: web::Path<String>, state: web::Data<AppState>) -> HttpResponse {
    let actor_id = path.into_inner();

    // Query EventStore directly for chat events
    let event_store = state.actor_manager.event_store();

    match event_store
        .send(GetEventsForActor {
            actor_id: actor_id.clone(),
            since_seq: 0,
        })
        .await
    {
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

            HttpResponse::Ok().json(json!({
                "success": true,
                "messages": messages
            }))
        }
        Ok(Err(_)) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": "EventStore error"
        })),
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "error": "Failed to get messages"
        })),
    }
}
