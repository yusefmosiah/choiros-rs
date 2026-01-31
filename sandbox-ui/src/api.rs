use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use shared_types::{ChatMessage, Sender};

const API_BASE: &str = "http://localhost:8080";

#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    pub actor_id: String,
    pub user_id: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageResponse {
    pub success: bool,
    pub temp_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct GetMessagesResponse {
    pub success: bool,
    pub messages: Vec<ApiMessage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiMessage {
    pub id: String,
    pub text: String,
    pub sender: String,
    pub timestamp: DateTime<Utc>,
    pub pending: bool,
}

pub async fn fetch_messages(actor_id: &str) -> Result<Vec<ChatMessage>, String> {
    let url = format!("{}/chat/{}/messages", API_BASE, actor_id);
    
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let data: GetMessagesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err("API returned success=false".to_string());
    }
    
    let messages = data.messages.into_iter().map(|m| ChatMessage {
        id: m.id,
        text: m.text,
        sender: if m.sender == "User" { Sender::User } else { Sender::Assistant },
        timestamp: m.timestamp,
        pending: m.pending,
    }).collect();
    
    Ok(messages)
}

pub async fn send_chat_message(actor_id: &str, user_id: &str, text: &str) -> Result<(), String> {
    let url = format!("{}/chat/send", API_BASE);
    
    let request = SendMessageRequest {
        actor_id: actor_id.to_string(),
        user_id: user_id.to_string(),
        text: text.to_string(),
    };
    
    let response = Request::post(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let data: SendMessageResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err(format!("API error: {}", data.message));
    }
    
    Ok(())
}
