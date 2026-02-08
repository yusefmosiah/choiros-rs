//! Shared types between frontend and backend
//!
//! These types are used by both:
//! - Actix actors (native Rust)
//! - Dioxus components (WASM)
//!
//! Serializable with serde for JSON over WebSocket/HTTP

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ActorId(pub String);

impl ActorId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Event System
// ============================================================================

/// Event - append-only log entry
/// All state changes are logged as events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct Event {
    /// Global sequence number (strictly increasing)
    pub seq: i64,

    /// Unique event ID (ULID)
    pub event_id: String,

    /// When the event occurred
    pub timestamp: DateTime<Utc>,

    /// Which actor produced this event
    pub actor_id: ActorId,

    /// Event type (e.g., "chat.user_msg", "file.write")
    pub event_type: String,

    /// Event-specific payload
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,

    /// Which user triggered this (for audit)
    pub user_id: String,
}

/// Request to append an event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct AppendEvent {
    pub event_type: String,
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    pub actor_id: ActorId,
    pub user_id: String,
}

/// Query events for an actor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QueryEvents {
    pub actor_id: ActorId,
    pub since_seq: i64,
}

// ============================================================================
// Actor Messages
// ============================================================================

/// Messages that can be sent to ChatActor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ChatMsg {
    /// User typed a message
    UserTyped { text: String, window_id: String },

    /// Assistant responded
    AssistantReply { text: String, model: String },

    /// Tool was called
    ToolCall {
        tool: String,
        #[ts(type = "unknown")]
        args: serde_json::Value,
        call_id: String,
    },

    /// Tool returned result
    ToolResult {
        call_id: String,
        status: ToolStatus,
        #[ts(type = "unknown")]
        output: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ToolStatus {
    Success,
    Error(String),
}

/// Messages that can be sent to WriterActor  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriterMsg {
    CreateDoc { title: String },
    EditFile { path: String, content: String },
    ReadFile { path: String },
}

// ============================================================================
// UI State
// ============================================================================

/// Desktop state - all windows and their positions
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct DesktopState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
    pub apps: Vec<AppDefinition>,
}

/// Individual window state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WindowState {
    pub id: String,
    pub app_id: String, // "chat", "writer", "mail", etc.
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z_index: u32,
    pub minimized: bool,
    pub maximized: bool,
    #[ts(type = "unknown")]
    pub props: serde_json::Value, // App-specific data
}

/// App definition for dynamic app registration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct AppDefinition {
    pub id: String,
    pub name: String,
    pub icon: String,           // emoji or SVG
    pub component_code: String, // Source code or WASM path
    pub default_width: i32,
    pub default_height: i32,
}

/// Chat message for UI display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ChatMessage {
    pub id: String,
    pub text: String,
    pub sender: Sender,
    pub timestamp: DateTime<Utc>,
    pub pending: bool, // True if optimistic (not confirmed by actor yet)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum Sender {
    User,
    Assistant,
    System,
}

// ============================================================================
// Viewer Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ViewerKind {
    Text,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerResource {
    pub uri: String,
    pub mime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerCapabilities {
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerDescriptor {
    pub kind: ViewerKind,
    pub resource: ViewerResource,
    pub capabilities: ViewerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerRevision {
    pub rev: i64,
    pub updated_at: String,
}

// ============================================================================
// API Types
// ============================================================================

/// Generic API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// WebSocket message protocol
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WsMsg {
    /// Client → Server: Subscribe to actor events
    Subscribe { actor_id: ActorId },

    /// Client → Server: Send message to actor
    Send {
        actor_id: ActorId,
        #[ts(type = "unknown")]
        payload: serde_json::Value,
    },

    /// Server → Client: Event occurred
    Event { actor_id: ActorId, event: Event },

    /// Server → Client: Current state snapshot
    State {
        actor_id: ActorId,
        #[ts(type = "unknown")]
        state: serde_json::Value,
    },

    /// Server → Client: Error occurred
    Error { message: String },
}

// ============================================================================
// Tool Definitions
// ============================================================================

/// Tool definition for LLM
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[ts(type = "unknown")]
    pub parameters: serde_json::Value, // JSON Schema
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ToolCall {
    pub id: String,
    pub tool: String,
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
}

// ============================================================================
// Control Plane Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum DelegatedTaskKind {
    TerminalCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct DelegatedTask {
    pub task_id: String,
    pub correlation_id: String,
    pub actor_id: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub kind: DelegatedTaskKind,
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum DelegatedTaskStatus {
    Accepted,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct DelegatedTaskResult {
    pub task_id: String,
    pub correlation_id: String,
    pub status: DelegatedTaskStatus,
    pub output: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

// ============================================================================
// Constants
// ============================================================================

/// Event types
pub const EVENT_CHAT_USER_MSG: &str = "chat.user_msg";
pub const EVENT_CHAT_ASSISTANT_MSG: &str = "chat.assistant_msg";
pub const EVENT_CHAT_TOOL_CALL: &str = "chat.tool_call";
pub const EVENT_CHAT_TOOL_RESULT: &str = "chat.tool_result";
pub const EVENT_MODEL_SELECTION: &str = "model.selection";
pub const EVENT_MODEL_CHANGED: &str = "model.changed";

/// Build a chat user-message payload with optional session/thread scope metadata.
pub fn chat_user_payload(
    text: impl Into<String>,
    session_id: Option<String>,
    thread_id: Option<String>,
) -> serde_json::Value {
    let text = text.into();
    let mut scope = serde_json::Map::new();
    if let Some(session_id) = session_id {
        scope.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id),
        );
    }
    if let Some(thread_id) = thread_id {
        scope.insert(
            "thread_id".to_string(),
            serde_json::Value::String(thread_id),
        );
    }

    if scope.is_empty() {
        return serde_json::Value::String(text);
    }

    serde_json::json!({
        "text": text,
        "scope": scope,
    })
}

/// Parse chat user-message text from either legacy string payloads or scoped object payloads.
pub fn parse_chat_user_text(payload: &serde_json::Value) -> Option<String> {
    payload.as_str().map(ToString::to_string).or_else(|| {
        payload
            .get("text")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    })
}

/// Attach optional scope metadata to any payload.
///
/// If no scope is provided, returns payload unchanged.
/// If payload is an object, inserts `scope`.
/// Otherwise wraps as `{ "value": <payload>, "scope": {...} }`.
pub fn with_scope(
    payload: serde_json::Value,
    session_id: Option<String>,
    thread_id: Option<String>,
) -> serde_json::Value {
    let mut scope = serde_json::Map::new();
    if let Some(session_id) = session_id {
        scope.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id),
        );
    }
    if let Some(thread_id) = thread_id {
        scope.insert(
            "thread_id".to_string(),
            serde_json::Value::String(thread_id),
        );
    }
    if scope.is_empty() {
        return payload;
    }

    match payload {
        serde_json::Value::Object(mut obj) => {
            obj.insert("scope".to_string(), serde_json::Value::Object(scope));
            serde_json::Value::Object(obj)
        }
        other => serde_json::json!({
            "value": other,
            "scope": scope,
        }),
    }
}
pub const EVENT_USER_THEME_PREFERENCE: &str = "user.theme_preference";
pub const EVENT_FILE_WRITE: &str = "file.write";
pub const EVENT_FILE_EDIT: &str = "file.edit";
pub const EVENT_ACTOR_SPAWNED: &str = "actor.spawned";
pub const EVENT_VIEWER_CONTENT_SAVED: &str = "viewer.content_saved";
pub const EVENT_VIEWER_CONTENT_CONFLICT: &str = "viewer.content_conflict";
pub const EVENT_TOPIC_WORKER_TASK_STARTED: &str = "worker.task.started";
pub const EVENT_TOPIC_WORKER_TASK_PROGRESS: &str = "worker.task.progress";
pub const EVENT_TOPIC_WORKER_TASK_COMPLETED: &str = "worker.task.completed";
pub const EVENT_TOPIC_WORKER_TASK_FAILED: &str = "worker.task.failed";

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ts_rs::Config;

    #[test]
    fn test_actor_id_generation() {
        let id1 = ActorId::new();
        let id2 = ActorId::new();
        assert_ne!(id1, id2);
        assert_eq!(id1.0.len(), 36); // UUID length
    }

    #[test]
    fn test_event_serialization() {
        let event = Event {
            seq: 1,
            event_id: "evt_123".to_string(),
            timestamp: Utc::now(),
            actor_id: ActorId::new(),
            event_type: EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!({"text": "Hello"}),
            user_id: "user_1".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(event.seq, deserialized.seq);
        assert_eq!(event.event_type, deserialized.event_type);
    }

    #[test]
    fn test_ws_msg_protocol() {
        let msg = WsMsg::Subscribe {
            actor_id: ActorId::new(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Subscribe"));
    }

    #[test]
    fn test_viewer_kind_serialization() {
        let kind = ViewerKind::Text;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"text\"");
    }

    #[test]
    fn export_types() {
        // Export all types to TypeScript
        // The export_to attribute in each type's #[ts] macro specifies the output file
        let config = Config::default();
        ActorId::export(&config).unwrap();
        Event::export(&config).unwrap();
        AppendEvent::export(&config).unwrap();
        QueryEvents::export(&config).unwrap();
        ChatMsg::export(&config).unwrap();
        ToolStatus::export(&config).unwrap();
        DesktopState::export(&config).unwrap();
        WindowState::export(&config).unwrap();
        AppDefinition::export(&config).unwrap();
        ChatMessage::export(&config).unwrap();
        Sender::export(&config).unwrap();
        ViewerKind::export(&config).unwrap();
        ViewerResource::export(&config).unwrap();
        ViewerCapabilities::export(&config).unwrap();
        ViewerDescriptor::export(&config).unwrap();
        ViewerRevision::export(&config).unwrap();
        WsMsg::export(&config).unwrap();
        ToolDef::export(&config).unwrap();
        ToolCall::export(&config).unwrap();
    }
}
