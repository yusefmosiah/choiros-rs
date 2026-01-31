use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use shared_types::{ChatMessage, Sender, WindowState, AppDefinition, DesktopState};

const API_BASE: &str = "";

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

// ============================================================================
// Desktop API Functions
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GetDesktopStateResponse {
    pub success: bool,
    pub desktop: DesktopState,
}

#[derive(Debug, Deserialize)]
pub struct GetWindowsResponse {
    pub success: bool,
    pub windows: Vec<WindowState>,
}

#[derive(Debug, Deserialize)]
pub struct GetAppsResponse {
    pub success: bool,
    pub apps: Vec<AppDefinition>,
}

#[derive(Debug, Serialize)]
pub struct OpenWindowRequest {
    pub app_id: String,
    pub title: String,
    pub props: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenWindowResponse {
    pub success: bool,
    pub window: Option<WindowState>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterAppRequest {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub component_code: String,
    pub default_width: i32,
    pub default_height: i32,
}

pub async fn fetch_desktop_state(desktop_id: &str) -> Result<DesktopState, String> {
    let url = format!("{}/desktop/{}", API_BASE, desktop_id);
    
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let data: GetDesktopStateResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err("API returned success=false".to_string());
    }
    
    Ok(data.desktop)
}

pub async fn fetch_windows(desktop_id: &str) -> Result<Vec<WindowState>, String> {
    let url = format!("{}/desktop/{}/windows", API_BASE, desktop_id);
    
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let data: GetWindowsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err("API returned success=false".to_string());
    }
    
    Ok(data.windows)
}

pub async fn open_window(
    desktop_id: &str,
    app_id: &str,
    title: &str,
    props: Option<serde_json::Value>,
) -> Result<WindowState, String> {
    let url = format!("{}/desktop/{}/windows", API_BASE, desktop_id);
    
    let request = OpenWindowRequest {
        app_id: app_id.to_string(),
        title: title.to_string(),
        props,
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
    
    let data: OpenWindowResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }
    
    data.window.ok_or_else(|| "Window not returned".to_string())
}

pub async fn close_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!("{}/desktop/{}/windows/{}", API_BASE, desktop_id, window_id);
    
    let response = Request::delete(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    #[derive(Debug, Deserialize)]
    struct Response {
        success: bool,
        error: Option<String>,
    }
    
    let data: Response = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }
    
    Ok(())
}

pub async fn focus_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!("{}/desktop/{}/windows/{}/focus", API_BASE, desktop_id, window_id);
    
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    #[derive(Debug, Deserialize)]
    struct Response {
        success: bool,
        error: Option<String>,
    }
    
    let data: Response = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }
    
    Ok(())
}

pub async fn fetch_apps(desktop_id: &str) -> Result<Vec<AppDefinition>, String> {
    let url = format!("{}/desktop/{}/apps", API_BASE, desktop_id);
    
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let data: GetAppsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err("API returned success=false".to_string());
    }
    
    Ok(data.apps)
}

pub async fn register_app(
    desktop_id: &str,
    app: &AppDefinition,
) -> Result<(), String> {
    let url = format!("{}/desktop/{}/apps", API_BASE, desktop_id);
    
    let response = Request::post(&url)
        .json(app)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    #[derive(Debug, Deserialize)]
    struct Response {
        success: bool,
        error: Option<String>,
    }
    
    let data: Response = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    
    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }
    
    Ok(())
}
