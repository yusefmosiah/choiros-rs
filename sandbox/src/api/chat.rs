//! Chat API endpoints with ActorManager
//!
//! PREDICTION: HTTP endpoints can use ActorManager to get persistent actor
//! instances, enabling multiturn chat with history preservation.

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actor_manager::AppState;
use crate::actors::chat::{SendUserMessage, GetMessages};
use crate::actors::event_store::{EventStoreActor, AppendEvent};

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
    // Get or create persistent ChatActor via Manager
    let chat_actor = state.actor_manager.get_or_create_chat(
        req.actor_id.clone(),
        req.user_id.clone()
    );
    
    // Send the message (optimistic)
    match chat_actor.send(SendUserMessage {
        text: req.text.clone(),
    }).await {
        Ok(Ok(temp_id)) => {
            HttpResponse::Ok().json(SendMessageResponse {
                success: true,
                temp_id,
                message: "Message sent".to_string(),
            })
        }
        Ok(Err(e)) => {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "error": e.to_string()
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Actor mailbox error"
            }))
        }
    }
}

/// Get messages for a chat actor
#[get("/chat/{actor_id}/messages")]
pub async fn get_messages(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let actor_id = path.into_inner();
    
    // Get existing ChatActor via Manager (returns same instance!)
    let chat_actor = state.actor_manager.get_or_create_chat(
        actor_id,
        "system".to_string() // user_id not important for reads
    );
    
    match chat_actor.send(GetMessages).await {
        Ok(messages) => {
            HttpResponse::Ok().json(json!({
                "success": true,
                "messages": messages
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "error": "Failed to get messages"
            }))
        }
    }
}
