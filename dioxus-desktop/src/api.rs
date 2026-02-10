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

async fn describe_http_error(response: gloo_net::http::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        return format!("HTTP error: {status}");
    }

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
            return format!("HTTP error: {status} ({error})");
        }
        if let Some(message) = json.get("message").and_then(|v| v.as_str()) {
            return format!("HTTP error: {status} ({message})");
        }
    }

    format!("HTTP error: {status} ({body})")
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

#[derive(Debug, Deserialize, Clone)]
pub struct LogsEvent {
    pub seq: i64,
    pub event_id: String,
    pub timestamp: String,
    pub event_type: String,
    pub actor_id: String,
    pub user_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct GetLogsEventsResponse {
    pub events: Vec<LogsEvent>,
}

pub async fn fetch_logs_events(
    since_seq: i64,
    limit: i64,
    event_type_prefix: Option<&str>,
) -> Result<Vec<LogsEvent>, String> {
    let mut url = format!(
        "{}/logs/events?since_seq={}&limit={}",
        api_base(),
        since_seq.max(0),
        limit.clamp(1, 1000)
    );
    if let Some(prefix) = event_type_prefix {
        let encoded = js_sys::encode_uri_component(prefix)
            .as_string()
            .unwrap_or_else(|| prefix.to_string());
        url.push_str("&event_type_prefix=");
        url.push_str(&encoded);
    }

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.ok() {
        return Err(describe_http_error(response).await);
    }
    let data: GetLogsEventsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;
    Ok(data.events)
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
        return Err(describe_http_error(response).await);
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
        return Err(describe_http_error(response).await);
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

pub async fn minimize_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/minimize",
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

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MaximizeWindowRequest {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub async fn maximize_window(
    desktop_id: &str,
    window_id: &str,
    work_area: Option<MaximizeWindowRequest>,
) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/maximize",
        api_base(),
        desktop_id,
        window_id
    );

    let response = match work_area {
        Some(work_area) => Request::post(&url)
            .json(&work_area)
            .map_err(|e| format!("Failed to serialize request: {e}"))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?,
        None => Request::post(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?,
    };

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

pub async fn restore_window(desktop_id: &str, window_id: &str) -> Result<(), String> {
    let url = format!(
        "{}/desktop/{}/windows/{}/restore",
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
    pub rendered_html: Option<String>,
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

// ============================================================================
// Files API Functions
// ============================================================================

pub mod files_api {
    use super::*;

    /// Directory entry in a listing
    #[derive(Debug, Clone, Deserialize)]
    pub struct DirectoryEntry {
        pub name: String,
        pub path: String,
        pub is_file: bool,
        pub is_dir: bool,
        pub size: u64,
        pub modified_at: String,
    }

    /// List directory response
    #[derive(Debug, Clone, Deserialize)]
    pub struct ListDirectoryResponse {
        pub path: String,
        pub entries: Vec<DirectoryEntry>,
        pub total_count: usize,
    }

    /// File content response
    #[derive(Debug, Clone, Deserialize)]
    pub struct FileContentResponse {
        pub path: String,
        pub content: String,
        pub size: usize,
        pub is_truncated: bool,
        pub encoding: String,
    }

    /// Create file request
    #[derive(Debug, Serialize)]
    struct CreateFileRequest {
        path: String,
        content: Option<String>,
        overwrite: Option<bool>,
    }

    /// Create file response
    #[derive(Debug, Deserialize)]
    pub struct CreateFileResponse {
        pub path: String,
        pub created: bool,
        pub size: u64,
    }

    /// Create directory request
    #[derive(Debug, Serialize)]
    struct CreateDirectoryRequest {
        path: String,
        recursive: Option<bool>,
    }

    /// Create directory response
    #[derive(Debug, Deserialize)]
    pub struct CreateDirectoryResponse {
        pub path: String,
        pub created: bool,
    }

    /// Rename request
    #[derive(Debug, Serialize)]
    struct RenameRequest {
        source: String,
        target: String,
        overwrite: Option<bool>,
    }

    /// Rename response
    #[derive(Debug, Deserialize)]
    pub struct RenameResponse {
        pub source: String,
        pub target: String,
        pub renamed: bool,
    }

    /// Delete request
    #[derive(Debug, Serialize)]
    struct DeleteRequest {
        path: String,
        recursive: Option<bool>,
    }

    /// Delete response
    #[derive(Debug, Deserialize)]
    pub struct DeleteResponse {
        pub path: String,
        pub deleted: bool,
        #[serde(rename = "type")]
        pub entry_type: String,
    }

    /// Error response
    #[derive(Debug, Deserialize)]
    struct ErrorResponse {
        pub error: ErrorDetail,
    }

    #[derive(Debug, Deserialize)]
    struct ErrorDetail {
        pub code: String,
        pub message: String,
    }

    /// List directory contents
    pub async fn list_directory(path: &str) -> Result<ListDirectoryResponse, String> {
        let encoded_path = js_sys::encode_uri_component(path)
            .as_string()
            .unwrap_or_else(|| path.to_string());
        let url = format!("{}/files/list?path={}", api_base(), encoded_path);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<ListDirectoryResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    /// Read file content
    pub async fn read_file_content(path: &str) -> Result<FileContentResponse, String> {
        let encoded_path = js_sys::encode_uri_component(path)
            .as_string()
            .unwrap_or_else(|| path.to_string());
        let url = format!("{}/files/content?path={}", api_base(), encoded_path);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<FileContentResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    /// Create a new file
    pub async fn create_file(path: &str, content: Option<String>) -> Result<CreateFileResponse, String> {
        let url = format!("{}/files/create", api_base());
        let request = CreateFileRequest {
            path: path.to_string(),
            content,
            overwrite: Some(false),
        };

        let response = Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<CreateFileResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    /// Create a new directory
    pub async fn create_directory(path: &str) -> Result<CreateDirectoryResponse, String> {
        let url = format!("{}/files/mkdir", api_base());
        let request = CreateDirectoryRequest {
            path: path.to_string(),
            recursive: Some(true),
        };

        let response = Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<CreateDirectoryResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    /// Rename/move a file or directory
    pub async fn rename_file(source: &str, target: &str) -> Result<RenameResponse, String> {
        let url = format!("{}/files/rename", api_base());
        let request = RenameRequest {
            source: source.to_string(),
            target: target.to_string(),
            overwrite: Some(false),
        };

        let response = Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<RenameResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    /// Delete a file or directory
    pub async fn delete_file(path: &str, is_dir: bool) -> Result<DeleteResponse, String> {
        let url = format!("{}/files/delete", api_base());
        let request = DeleteRequest {
            path: path.to_string(),
            recursive: Some(is_dir),
        };

        let response = Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(format!("{}: {}", err.error.code, err.error.message));
            }
            return Err(format!("HTTP error: {status}"));
        }

        response
            .json::<DeleteResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {e}"))
    }
}
