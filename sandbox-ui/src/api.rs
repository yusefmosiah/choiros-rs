use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use shared_types::{AppDefinition, ChatMessage, DesktopState, Sender, ViewerRevision, WindowState};
use std::sync::OnceLock;

/// Get the API base URL based on current environment
/// - In development (localhost): use http://localhost:8080
/// - In production: use same origin (API serves static files)
fn get_api_base() -> String {
    // Get the current hostname from the browser
    let hostname = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_default();

    // If running on localhost, point to the API server on port 8080
    if hostname == "localhost" || hostname == "127.0.0.1" {
        "http://localhost:8080".to_string()
    } else {
        // In production, use same origin
        "".to_string()
    }
}

/// Lazy-static equivalent for WASM - computed at first use
static API_BASE_CACHE: OnceLock<String> = OnceLock::new();

/// Get the cached API base URL
pub fn api_base() -> &'static str {
    API_BASE_CACHE.get_or_init(get_api_base).as_str()
}

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
    let url = format!("{}/chat/{}/messages", api_base(), actor_id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: GetMessagesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err("API returned success=false".to_string());
    }

    let messages = data
        .messages
        .into_iter()
        .map(|m| ChatMessage {
            id: m.id,
            text: m.text,
            sender: match m.sender.as_str() {
                "User" => Sender::User,
                "System" => Sender::System,
                _ => Sender::Assistant,
            },
            timestamp: m.timestamp,
            pending: m.pending,
        })
        .collect();

    Ok(messages)
}

pub async fn send_chat_message(actor_id: &str, user_id: &str, text: &str) -> Result<(), String> {
    let url = format!("{}/chat/send", api_base());

    let request = SendMessageRequest {
        actor_id: actor_id.to_string(),
        user_id: user_id.to_string(),
        text: text.to_string(),
    };

    let response = Request::post(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: SendMessageResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

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

#[derive(Debug, Deserialize)]
pub struct UserPreferencesResponse {
    pub success: bool,
    pub theme: String,
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
    let url = format!("{}/desktop/{}", api_base(), desktop_id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: GetDesktopStateResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err("API returned success=false".to_string());
    }

    Ok(data.desktop)
}

pub async fn fetch_windows(desktop_id: &str) -> Result<Vec<WindowState>, String> {
    let url = format!("{}/desktop/{}/windows", api_base(), desktop_id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: GetWindowsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

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
    let url = format!("{}/desktop/{}/windows", api_base(), desktop_id);

    let request = OpenWindowRequest {
        app_id: app_id.to_string(),
        title: title.to_string(),
        props,
    };

    let response = Request::post(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: OpenWindowResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    data.window.ok_or_else(|| "Window not returned".to_string())
}

pub async fn fetch_user_theme_preference(user_id: &str) -> Result<String, String> {
    let url = format!("{}/user/{}/preferences", api_base(), user_id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: UserPreferencesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err("API returned success=false".to_string());
    }

    Ok(data.theme)
}

pub async fn update_user_theme_preference(user_id: &str, theme: &str) -> Result<String, String> {
    let url = format!("{}/user/{}/preferences", api_base(), user_id);
    let request = serde_json::json!({ "theme": theme });

    let response = Request::patch(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: UserPreferencesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err("API returned success=false".to_string());
    }

    Ok(data.theme)
}

pub async fn close_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}",
        api_base(),
        desktop_id,
        window_id
    );

    let response = Request::delete(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

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
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    Ok(())
}

pub async fn focus_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/focus",
        api_base(),
        desktop_id,
        window_id
    );

    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

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
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct MoveWindowRequest {
    pub x: i32,
    pub y: i32,
}

pub async fn move_window(desktop_id: &str, window_id: &str, x: i32, y: i32) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/position",
        api_base(),
        desktop_id,
        window_id
    );

    let request = MoveWindowRequest { x, y };

    let response = Request::patch(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

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
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ResizeWindowRequest {
    pub width: i32,
    pub height: i32,
}

pub async fn resize_window(
    desktop_id: &str,
    window_id: &str,
    width: i32,
    height: i32,
) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/size",
        api_base(),
        desktop_id,
        window_id
    );

    let request = ResizeWindowRequest { width, height };

    let response = Request::patch(&url)
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

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
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    Ok(())
}

pub async fn fetch_apps(desktop_id: &str) -> Result<Vec<AppDefinition>, String> {
    let url = format!("{}/desktop/{}/apps", api_base(), desktop_id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: GetAppsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err("API returned success=false".to_string());
    }

    Ok(data.apps)
}

pub async fn register_app(desktop_id: &str, app: &AppDefinition) -> Result<(), String> {
    let url = format!("{}/desktop/{}/apps", api_base(), desktop_id);

    let response = Request::post(&url)
        .json(app)
        .map_err(|e| format!("Failed to serialize request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

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
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if !data.success {
        return Err(data.error.unwrap_or_else(|| "Unknown error".to_string()));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ViewerContentResponse {
    pub success: bool,
    pub uri: String,
    pub mime: String,
    pub content: String,
    pub revision: ViewerRevision,
    pub readonly: bool,
}

#[derive(Debug, Serialize)]
pub struct PatchViewerContentRequest {
    pub uri: String,
    pub base_rev: i64,
    pub content: String,
    pub window_id: String,
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchViewerContentResponse {
    pub success: bool,
    pub revision: Option<ViewerRevision>,
    pub error: Option<String>,
    pub latest: Option<PatchViewerContentLatest>,
}

#[derive(Debug, Deserialize)]
pub struct PatchViewerContentLatest {
    pub content: String,
    pub revision: ViewerRevision,
}

#[derive(Debug)]
pub enum PatchViewerContentError {
    Conflict {
        latest_content: String,
        latest_revision: ViewerRevision,
    },
    Message(String),
}

pub async fn fetch_viewer_content(uri: &str) -> Result<ViewerContentResponse, String> {
    let url = format!("{}/viewer/content?uri={}", api_base(), uri);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    let data: ViewerContentResponse = response
        .json()
        .await
        .map_err(|e| format!("failed to parse JSON: {e}"))?;
    if !data.success {
        return Err("viewer API returned success=false".to_string());
    }
    Ok(data)
}

pub async fn patch_viewer_content(
    uri: &str,
    base_rev: i64,
    content: &str,
    window_id: &str,
) -> Result<ViewerRevision, PatchViewerContentError> {
    let url = format!("{}/viewer/content", api_base());
    let req = PatchViewerContentRequest {
        uri: uri.to_string(),
        base_rev,
        content: content.to_string(),
        window_id: window_id.to_string(),
        user_id: "user-1".to_string(),
    };

    let response = Request::patch(&url)
        .json(&req)
        .map_err(|e| PatchViewerContentError::Message(format!("request encode failed: {e}")))?
        .send()
        .await
        .map_err(|e| PatchViewerContentError::Message(format!("request failed: {e}")))?;

    let status = response.status();
    let data: PatchViewerContentResponse = response
        .json()
        .await
        .map_err(|e| PatchViewerContentError::Message(format!("failed to parse JSON: {e}")))?;

    if status == 409 || data.error.as_deref() == Some("revision_conflict") {
        if let Some(latest) = data.latest {
            return Err(PatchViewerContentError::Conflict {
                latest_content: latest.content,
                latest_revision: latest.revision,
            });
        }
        return Err(PatchViewerContentError::Message(
            "revision_conflict without latest payload".to_string(),
        ));
    }

    if !data.success {
        return Err(PatchViewerContentError::Message(
            data.error
                .unwrap_or_else(|| "unknown viewer save error".to_string()),
        ));
    }

    data.revision.ok_or_else(|| {
        PatchViewerContentError::Message("missing revision in save response".to_string())
    })
}
