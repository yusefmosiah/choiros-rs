//! ChatAgent - BAML-powered agent with tool execution
//!
//! This actor combines BAML LLM planning with tool execution to provide
//! an intelligent chat interface with file system access.
//!
//! Converted from Actix to ractor actor model.

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::{
    load_model_policy, ModelConfigError, ModelRegistry, ModelResolutionContext,
};
use crate::baml_client::types::{Message as BamlMessage, ToolResult};
use crate::supervisor::ApplicationSupervisorMsg;
use crate::tools::ToolOutput;

/// ChatAgent - AI assistant with planning and tool execution capabilities
pub struct ChatAgent;

/// Arguments for spawning ChatAgent
#[derive(Debug, Clone)]
pub struct ChatAgentArguments {
    pub actor_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub preload_session_id: Option<String>,
    pub preload_thread_id: Option<String>,
    pub application_supervisor: Option<ActorRef<ApplicationSupervisorMsg>>,
}

/// State for ChatAgent
pub struct ChatAgentState {
    args: ChatAgentArguments,
    messages: Vec<BamlMessage>,
    current_model: String,
    model_registry: ModelRegistry,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by ChatAgent
#[derive(Debug)]
pub enum ChatAgentMsg {
    /// Process a user message and return agent response
    ProcessMessage {
        text: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        model_override: Option<String>,
        reply: RpcReplyPort<Result<AgentResponse, ChatAgentError>>,
    },
    /// Switch between available LLM models
    SwitchModel {
        model: String,
        reply: RpcReplyPort<Result<(), ChatAgentError>>,
    },
    /// Get conversation history
    GetConversationHistory {
        reply: RpcReplyPort<Vec<BamlMessage>>,
    },
    /// Get available tools list
    GetAvailableTools { reply: RpcReplyPort<Vec<String>> },
}

/// Agent response structure
#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub text: String,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub thinking: String,
    pub confidence: f64,
    pub model_used: String,
    pub model_source: String,
}

/// Record of an executed tool call
#[derive(Debug, Clone)]
pub struct ExecutedToolCall {
    pub tool_name: String,
    pub tool_args: String,
    pub reasoning: String,
    pub result: ToolOutput,
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug, thiserror::Error, Clone)]
pub enum ChatAgentError {
    #[error("BAML error: {0}")]
    Baml(String),

    #[error("Tool execution error: {0}")]
    Tool(String),

    #[error("Event store error: {0}")]
    EventStore(String),

    #[error("Model switch error: {0}")]
    ModelSwitch(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid model: {0}")]
    InvalidModel(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<serde_json::Error> for ChatAgentError {
    fn from(e: serde_json::Error) -> Self {
        ChatAgentError::Serialization(e.to_string())
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

impl Default for ChatAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatAgent {
    /// Create a new ChatAgent actor instance
    pub fn new() -> Self {
        Self
    }

    fn format_timestamp(ts: DateTime<Utc>) -> String {
        ts.to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    fn timestamped_prompt_content(content: &str, ts: DateTime<Utc>) -> String {
        format!("[{}]\n{}", Self::format_timestamp(ts), content)
    }

    fn delegated_soft_wait_ms(hard_timeout_ms: u64) -> u64 {
        let configured = std::env::var("CHOIR_DELEGATED_TOOL_SOFT_WAIT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1_500);
        configured.clamp(300, hard_timeout_ms.max(300))
    }

    fn delegated_result_output(payload: &serde_json::Value) -> Option<String> {
        payload
            .get("output")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("summary").and_then(|v| v.as_str()))
            .map(ToString::to_string)
    }

    fn get_tools_description(_state: &ChatAgentState) -> String {
        let bash_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "reasoning": {
                    "type": "string",
                    "description": "Optional rationale for why this command is being run."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 30000)",
                    "default": 30000
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for command execution."
                },
                "command": {
                    "type": "string",
                    "description": "The shell command to execute (legacy key)."
                },
                "cmd": {
                    "type": "string",
                    "description": "The shell command to execute (preferred key)."
                },
                "model": {
                    "type": "string",
                    "description": "Optional terminal runtime model override."
                }
            },
            "anyOf": [
                { "required": ["command"] },
                { "required": ["cmd"] }
            ]
        });
        let web_search_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language web research query."
                },
                "provider": {
                    "type": "string",
                    "description": "Optional provider override: auto, tavily, brave, exa, all, or comma-separated list (e.g. tavily,brave)."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum results to return (1-20).",
                    "default": 6
                },
                "time_range": {
                    "type": "string",
                    "description": "Optional freshness scope: day, week, month, year."
                },
                "include_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional allowlist of domains."
                },
                "exclude_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional blocklist of domains."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 45000)",
                    "default": 45000
                },
                "model": {
                    "type": "string",
                    "description": "Optional researcher runtime model override."
                },
                "reasoning": {
                    "type": "string",
                    "description": "Optional rationale for this search."
                }
            },
            "required": ["query"]
        });
        format!(
            "Tool: bash\nDescription: Execute shell commands via TerminalActor delegation.\nParameters Schema: {}\n\nTool: web_search\nDescription: Execute web research via ResearcherActor (provider-isolated search adapters).\nParameters Schema: {}\n",
            bash_schema, web_search_schema
        )
    }

    fn get_system_context(state: &ChatAgentState) -> String {
        let now = Self::format_timestamp(Utc::now());
        format!(
            r#"You are ChoirOS, an AI assistant in a web desktop environment.

System Prompt Timestamp (UTC): {}
Current UTC Timestamp: {}

User ID: {}
Actor ID: {}
Working Directory: {}

You have access to tools for:
- Executing bash commands (delegated via TerminalActor)
- Running web research queries (delegated via ResearcherActor)

Behavior requirements:
- If the user asks for real-time/external information (for example weather, web/API data, latest status), attempt a tool call first.
- Prefer `web_search` for web research and source-citation tasks.
- If the user explicitly asks to "use api", "use bash", or "run a command", use the bash tool unless unsafe.
- Do not claim internet/API limitations before attempting a relevant tool call.
- If a tool call fails, explain the concrete failure and then provide alternatives.

Be helpful, accurate, and concise. Use tools when needed to complete user requests."#,
            now,
            now,
            state.args.user_id,
            state.args.actor_id,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
        )
    }

    fn map_model_error(error: ModelConfigError) -> ChatAgentError {
        match error {
            ModelConfigError::UnknownModel(model_id) => {
                ChatAgentError::InvalidModel(format!("Unknown model: {model_id}"))
            }
            ModelConfigError::MissingApiKey(env_var) => ChatAgentError::ModelSwitch(format!(
                "Missing API key environment variable for selected model: {env_var}"
            )),
            ModelConfigError::NoFallbackAvailable => {
                ChatAgentError::ModelSwitch("No fallback model available".to_string())
            }
        }
    }

    fn bash_tool_args_to_value(
        args: &crate::baml_client::types::BashToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.command {
            map.insert("command".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cmd {
            map.insert("cmd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cwd {
            map.insert("cwd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.reasoning {
            map.insert(
                "reasoning".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = args.timeout_ms {
            map.insert(
                "timeout_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.model {
            map.insert("model".to_string(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map)
    }

    fn read_file_tool_args_to_value(
        args: &crate::baml_client::types::ReadFileToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.limit {
            map.insert(
                "limit".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = args.offset {
            map.insert(
                "offset".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        serde_json::Value::Object(map)
    }

    fn write_file_tool_args_to_value(
        args: &crate::baml_client::types::WriteFileToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.content {
            map.insert("content".to_string(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map)
    }

    fn list_files_tool_args_to_value(
        args: &crate::baml_client::types::ListFilesToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.recursive {
            map.insert("recursive".to_string(), serde_json::Value::Bool(v));
        }
        serde_json::Value::Object(map)
    }

    fn search_files_tool_args_to_value(
        args: &crate::baml_client::types::SearchFilesToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.pattern {
            map.insert("pattern".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.file_pattern {
            map.insert(
                "file_pattern".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        serde_json::Value::Object(map)
    }

    fn web_search_tool_args_to_value(
        args: &crate::baml_client::types::WebSearchToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.query {
            map.insert("query".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.provider {
            map.insert("provider".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.max_results {
            map.insert(
                "max_results".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.time_range {
            map.insert(
                "time_range".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = &args.include_domains {
            map.insert(
                "include_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        if let Some(v) = &args.exclude_domains {
            map.insert(
                "exclude_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        if let Some(v) = args.timeout_ms {
            map.insert(
                "timeout_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.model {
            map.insert("model".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.reasoning {
            map.insert(
                "reasoning".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        serde_json::Value::Object(map)
    }

    fn agent_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.bash {
            map.insert("bash".to_string(), Self::bash_tool_args_to_value(v));
        }
        if let Some(v) = &args.read_file {
            map.insert(
                "read_file".to_string(),
                Self::read_file_tool_args_to_value(v),
            );
        }
        if let Some(v) = &args.write_file {
            map.insert(
                "write_file".to_string(),
                Self::write_file_tool_args_to_value(v),
            );
        }
        if let Some(v) = &args.list_files {
            map.insert(
                "list_files".to_string(),
                Self::list_files_tool_args_to_value(v),
            );
        }
        if let Some(v) = &args.search_files {
            map.insert(
                "search_files".to_string(),
                Self::search_files_tool_args_to_value(v),
            );
        }
        if let Some(v) = &args.web_search {
            map.insert(
                "web_search".to_string(),
                Self::web_search_tool_args_to_value(v),
            );
        }
        if let Some(v) = &args.command {
            map.insert("command".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cmd {
            map.insert("cmd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cwd {
            map.insert("cwd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.reasoning {
            map.insert(
                "reasoning".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = args.timeout_ms {
            map.insert(
                "timeout_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.model {
            map.insert("model".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.content {
            map.insert("content".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.pattern {
            map.insert("pattern".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.file_pattern {
            map.insert(
                "file_pattern".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = args.recursive {
            map.insert("recursive".to_string(), serde_json::Value::Bool(v));
        }
        if let Some(v) = args.limit {
            map.insert(
                "limit".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = args.offset {
            map.insert(
                "offset".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.query {
            map.insert("query".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.provider {
            map.insert("provider".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.max_results {
            map.insert(
                "max_results".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.time_range {
            map.insert(
                "time_range".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = &args.include_domains {
            map.insert(
                "include_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        if let Some(v) = &args.exclude_domains {
            map.insert(
                "exclude_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        serde_json::Value::Object(map)
    }

    fn legacy_bash_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.command {
            map.insert("command".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cmd {
            map.insert("cmd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.cwd {
            map.insert("cwd".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.reasoning {
            map.insert(
                "reasoning".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = args.timeout_ms {
            map.insert(
                "timeout_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.model {
            map.insert("model".to_string(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map)
    }

    fn legacy_read_file_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.limit {
            map.insert(
                "limit".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = args.offset {
            map.insert(
                "offset".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        serde_json::Value::Object(map)
    }

    fn legacy_write_file_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.content {
            map.insert("content".to_string(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map)
    }

    fn legacy_list_files_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.recursive {
            map.insert("recursive".to_string(), serde_json::Value::Bool(v));
        }
        serde_json::Value::Object(map)
    }

    fn legacy_search_files_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.pattern {
            map.insert("pattern".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.path {
            map.insert("path".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.file_pattern {
            map.insert(
                "file_pattern".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        serde_json::Value::Object(map)
    }

    fn legacy_web_search_tool_args_to_value(
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(v) = &args.query {
            map.insert("query".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.provider {
            map.insert("provider".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = args.max_results {
            map.insert(
                "max_results".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.time_range {
            map.insert(
                "time_range".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = &args.include_domains {
            map.insert(
                "include_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        if let Some(v) = &args.exclude_domains {
            map.insert(
                "exclude_domains".to_string(),
                serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
            );
        }
        if let Some(v) = args.timeout_ms {
            map.insert(
                "timeout_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(v)),
            );
        }
        if let Some(v) = &args.model {
            map.insert("model".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &args.reasoning {
            map.insert(
                "reasoning".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        serde_json::Value::Object(map)
    }

    fn tool_execution_args_to_value(
        tool_name: &str,
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        match tool_name {
            "bash" => args
                .bash
                .as_ref()
                .map(Self::bash_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_bash_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            "read_file" => args
                .read_file
                .as_ref()
                .map(Self::read_file_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_read_file_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            "write_file" => args
                .write_file
                .as_ref()
                .map(Self::write_file_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_write_file_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            "list_files" => args
                .list_files
                .as_ref()
                .map(Self::list_files_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_list_files_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            "search_files" => args
                .search_files
                .as_ref()
                .map(Self::search_files_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_search_files_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            "web_search" => args
                .web_search
                .as_ref()
                .map(Self::web_search_tool_args_to_value)
                .or_else(|| {
                    let legacy = Self::legacy_web_search_tool_args_to_value(args);
                    legacy
                        .as_object()
                        .is_some_and(|o| !o.is_empty())
                        .then_some(legacy)
                })
                .unwrap_or_else(|| serde_json::json!({})),
            _ => serde_json::json!({}),
        }
    }

    fn tool_args_for_log(
        tool_name: &str,
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> serde_json::Value {
        let execution_args = Self::tool_execution_args_to_value(tool_name, args);
        if execution_args.as_object().is_some_and(|o| !o.is_empty()) {
            execution_args
        } else {
            Self::agent_tool_args_to_value(args)
        }
    }

    fn tool_args_for_execution(
        tool_name: &str,
        args: &crate::baml_client::types::AgentToolArgs,
    ) -> String {
        Self::tool_execution_args_to_value(tool_name, args).to_string()
    }

    fn history_from_events(events: Vec<shared_types::Event>) -> Vec<BamlMessage> {
        let mut history = Vec::new();

        for event in events {
            match event.event_type.as_str() {
                shared_types::EVENT_CHAT_USER_MSG => {
                    if let Some(text) = shared_types::parse_chat_user_text(&event.payload) {
                        history.push(BamlMessage {
                            role: "user".to_string(),
                            content: Self::timestamped_prompt_content(&text, event.timestamp),
                        });
                    }
                }
                shared_types::EVENT_CHAT_ASSISTANT_MSG => {
                    let text = event
                        .payload
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    if !text.is_empty() {
                        history.push(BamlMessage {
                            role: "assistant".to_string(),
                            content: Self::timestamped_prompt_content(&text, event.timestamp),
                        });
                    }
                }
                _ => {}
            }
        }

        history
    }

    /// Log an event to the EventStore using RPC
    async fn log_event(
        &self,
        state: &ChatAgentState,
        event_type: &str,
        payload: serde_json::Value,
        session_id: Option<String>,
        thread_id: Option<String>,
        user_id: String,
    ) -> Result<(), ChatAgentError> {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload: shared_types::with_scope(payload, session_id, thread_id),
            actor_id: state.args.actor_id.clone(),
            user_id,
        };

        let result = ractor::call!(state.args.event_store.clone(), |reply| {
            EventStoreMsg::Append { event, reply }
        });

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ChatAgentError::EventStore(e.to_string())),
            Err(e) => Err(ChatAgentError::EventStore(e.to_string())),
        }
    }

    fn spawn_background_followup(
        &self,
        state: &ChatAgentState,
        task_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        hard_wait_timeout_ms: u64,
        capability: &'static str,
    ) {
        let event_store = state.args.event_store.clone();
        let actor_id = state.args.actor_id.clone();
        tokio::spawn(async move {
            let result = Self::wait_for_delegated_task_result_internal(
                &event_store,
                &actor_id,
                &task_id,
                session_id.clone(),
                thread_id.clone(),
                hard_wait_timeout_ms,
            )
            .await;

            let followup_text = match result {
                Ok(result) => match result.status {
                    shared_types::DelegatedTaskStatus::Completed => {
                        let output = result
                            .output
                            .unwrap_or_else(|| format!("{capability} task completed."));
                        format!("Async {capability} update\n\n{output}")
                    }
                    shared_types::DelegatedTaskStatus::Failed => {
                        let error = result
                            .error
                            .unwrap_or_else(|| format!("{capability} task failed."));
                        format!("Async {capability} update\n\nTask failed: {error}")
                    }
                    _ => return,
                },
                Err(err) => format!("Async {capability} update\n\nTask did not finish: {err}"),
            };

            let payload = serde_json::json!({
                "text": followup_text,
                "thinking": format!("Asynchronous {} follow-up emitted from delegated task completion.", capability),
                "confidence": 1.0,
                "model": "system.delegated_task",
                "model_source": "system",
                "tools_used": 1,
                "async_followup": true,
                "task_id": task_id,
                "capability": capability,
            });
            let event = AppendEvent {
                event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                payload: shared_types::with_scope(payload, session_id, thread_id),
                actor_id,
                user_id: "system".to_string(),
            };

            let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append { event, reply });
        });
    }

    /// Handle ProcessMessage
    async fn handle_process_message(
        &self,
        state: &mut ChatAgentState,
        text: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        model_override: Option<String>,
    ) -> Result<AgentResponse, ChatAgentError> {
        let user_text = text.trim().to_string();
        if user_text.is_empty() {
            return Err(ChatAgentError::Validation(
                "Message cannot be empty".to_string(),
            ));
        }

        self.log_event(
            state,
            shared_types::EVENT_CHAT_USER_MSG,
            shared_types::chat_user_payload(
                user_text.clone(),
                session_id.clone(),
                thread_id.clone(),
            ),
            session_id.clone(),
            thread_id.clone(),
            state.args.user_id.clone(),
        )
        .await?;

        state.messages.push(BamlMessage {
            role: "user".to_string(),
            content: Self::timestamped_prompt_content(&user_text, Utc::now()),
        });

        let tools_description = Self::get_tools_description(state);
        let system_context = Self::get_system_context(state);
        let requested_model = model_override.clone();
        let resolved_model = state
            .model_registry
            .resolve_for_role(
                "chat",
                &ModelResolutionContext {
                    request_model: model_override,
                    app_preference: Some(state.current_model.clone()),
                    user_preference: None,
                },
            )
            .map_err(Self::map_model_error)?;
        let model_used = resolved_model.config.id;
        let model_source = resolved_model.source.as_str().to_string();
        let client_registry = state
            .model_registry
            .create_runtime_client_registry_for_model(&model_used)
            .map_err(Self::map_model_error)?;

        self.log_event(
            state,
            shared_types::EVENT_MODEL_SELECTION,
            serde_json::json!({
                "role": "chat",
                "model_used": model_used.clone(),
                "model_source": model_source.clone(),
                "requested_model": requested_model,
                "chat_model_preference": state.current_model.clone(),
            }),
            session_id.clone(),
            thread_id.clone(),
            state.args.user_id.clone(),
        )
        .await?;

        let plan = crate::baml_client::B
            .PlanAction
            .with_client_registry(&client_registry)
            .call(&state.messages, &system_context, &tools_description)
            .await
            .map_err(|e| ChatAgentError::Baml(e.to_string()))?;

        tracing::info!(
            actor_id = %state.args.actor_id,
            thinking = %plan.thinking,
            confidence = %plan.confidence,
            tool_count = plan.tool_calls.len(),
            "Agent planned action"
        );

        let mut executed_tools: Vec<ExecutedToolCall> = Vec::new();
        let mut tool_results: Vec<ToolResult> = Vec::new();

        for tool_call in &plan.tool_calls {
            let tool_args_value =
                Self::tool_args_for_log(&tool_call.tool_name, &tool_call.tool_args);
            let tool_args =
                Self::tool_args_for_execution(&tool_call.tool_name, &tool_call.tool_args);
            self.log_event(
                state,
                shared_types::EVENT_CHAT_TOOL_CALL,
                serde_json::json!({
                    "tool_name": tool_call.tool_name,
                    "tool_args": tool_args_value,
                    "reasoning": tool_call.reasoning,
                }),
                session_id.clone(),
                thread_id.clone(),
                state.args.user_id.clone(),
            )
            .await?;

            let result = if tool_call.tool_name == "bash" {
                self.delegate_terminal_tool(
                    state,
                    tool_args.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                    Some(model_used.clone()),
                )
                .await
            } else if tool_call.tool_name == "web_search" {
                self.delegate_research_tool(
                    state,
                    tool_args.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                    Some(model_used.clone()),
                )
                .await
            } else {
                Err(ChatAgentError::Tool(format!(
                    "Unsupported tool '{}' for ChatAgent. Use delegated capability actors only.",
                    tool_call.tool_name
                )))
            };

            match result {
                Ok(output) => {
                    executed_tools.push(ExecutedToolCall {
                        tool_name: tool_call.tool_name.clone(),
                        tool_args: tool_args.clone(),
                        reasoning: tool_call.reasoning.clone().unwrap_or_default(),
                        result: output.clone(),
                    });

                    self.log_event(
                        state,
                        shared_types::EVENT_CHAT_TOOL_RESULT,
                        serde_json::json!({
                            "tool_name": tool_call.tool_name,
                            "success": output.success,
                            "output": output.content,
                        }),
                        session_id.clone(),
                        thread_id.clone(),
                        state.args.user_id.clone(),
                    )
                    .await?;

                    tool_results.push(ToolResult {
                        tool_name: tool_call.tool_name.clone(),
                        success: output.success,
                        output: output.content.clone(),
                        error: None,
                    });
                }
                Err(e) => {
                    self.log_event(
                        state,
                        shared_types::EVENT_CHAT_TOOL_RESULT,
                        serde_json::json!({
                            "tool_name": tool_call.tool_name,
                            "success": false,
                            "error": e.to_string(),
                        }),
                        session_id.clone(),
                        thread_id.clone(),
                        state.args.user_id.clone(),
                    )
                    .await?;

                    tool_results.push(ToolResult {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let conversation_context = state
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let response_text = if let Some(final_response) = plan.final_response {
            final_response
        } else {
            let synthesis_user_prompt = Self::timestamped_prompt_content(&user_text, Utc::now());
            crate::baml_client::B
                .SynthesizeResponse
                .with_client_registry(&client_registry)
                .call(&synthesis_user_prompt, &tool_results, &conversation_context)
                .await
                .map_err(|e| ChatAgentError::Baml(e.to_string()))?
        };

        state.messages.push(BamlMessage {
            role: "assistant".to_string(),
            content: Self::timestamped_prompt_content(&response_text, Utc::now()),
        });

        self.log_event(
            state,
            shared_types::EVENT_CHAT_ASSISTANT_MSG,
            serde_json::json!({
                "text": response_text,
                "thinking": plan.thinking,
                "confidence": plan.confidence,
                "model": model_used.clone(),
                "model_source": model_source.clone(),
                "tools_used": executed_tools.len(),
            }),
            session_id,
            thread_id,
            "system".to_string(),
        )
        .await?;

        Ok(AgentResponse {
            text: response_text,
            tool_calls: executed_tools,
            thinking: plan.thinking,
            confidence: plan.confidence,
            model_used,
            model_source,
        })
    }

    /// Handle SwitchModel
    fn handle_switch_model(
        &self,
        state: &mut ChatAgentState,
        model: String,
    ) -> Result<(), ChatAgentError> {
        let resolved_model = state.model_registry.get(&model).cloned().ok_or_else(|| {
            let available = state.model_registry.available_model_ids().join(", ");
            ChatAgentError::InvalidModel(format!(
                "Unknown model: {model}. Available models: {available}"
            ))
        })?;
        tracing::info!(
            actor_id = %state.args.actor_id,
            old_model = %state.current_model,
            new_model = %resolved_model.id,
            "Switching model"
        );
        let old_model = state.current_model.clone();
        state.current_model = resolved_model.id;
        let event_store = state.args.event_store.clone();
        let actor_id = state.args.actor_id.clone();
        let user_id = state.args.user_id.clone();
        let new_model = state.current_model.clone();
        tokio::spawn(async move {
            let event = AppendEvent {
                event_type: shared_types::EVENT_MODEL_CHANGED.to_string(),
                payload: serde_json::json!({
                    "old_model": old_model,
                    "new_model": new_model,
                    "source": "switch_model",
                }),
                actor_id,
                user_id,
            };
            let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append { event, reply });
        });
        Ok(())
    }

    /// Handle GetConversationHistory
    fn handle_get_conversation_history(&self, state: &ChatAgentState) -> Vec<BamlMessage> {
        state.messages.clone()
    }

    /// Handle GetAvailableTools
    fn handle_get_available_tools(&self, _state: &ChatAgentState) -> Vec<String> {
        vec!["bash".to_string(), "web_search".to_string()]
    }

    async fn delegate_terminal_tool(
        &self,
        state: &ChatAgentState,
        tool_args: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        default_model_override: Option<String>,
    ) -> Result<ToolOutput, ChatAgentError> {
        let Some(supervisor) = &state.args.application_supervisor else {
            return Err(ChatAgentError::Tool(
                "ApplicationSupervisor unavailable for delegation".to_string(),
            ));
        };

        let parsed_args: serde_json::Value = serde_json::from_str(&tool_args)
            .map_err(|e| ChatAgentError::Serialization(e.to_string()))?;
        let command = parsed_args
            .get("cmd")
            .and_then(|v| v.as_str())
            .or_else(|| parsed_args.get("command").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ChatAgentError::Validation(
                    "Missing 'cmd' (or legacy 'command') argument".to_string(),
                )
            })?
            .to_string();
        let _reasoning = parsed_args
            .get("reasoning")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let working_dir = parsed_args
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();
        let timeout_ms = parsed_args.get("timeout_ms").and_then(|v| v.as_u64());
        let model_override = parsed_args
            .get("model")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or(default_model_override);

        let terminal_id = match (&session_id, &thread_id) {
            (Some(session_id), Some(thread_id)) => {
                format!("term:{}:{}:{}", state.args.actor_id, session_id, thread_id)
            }
            _ => format!("term:{}", state.args.actor_id),
        };

        let task = ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::DelegateTerminalTask {
                terminal_id,
                actor_id: state.args.actor_id.clone(),
                user_id: state.args.user_id.clone(),
                shell: "/bin/zsh".to_string(),
                working_dir: working_dir.clone(),
                command: command.clone(),
                timeout_ms,
                model_override,
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                reply,
            }
        })
        .map_err(|e| ChatAgentError::Tool(e.to_string()))?
        .map_err(ChatAgentError::Tool)?;

        let wait_timeout_ms = timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000) + 2_000;
        let soft_wait_ms = Self::delegated_soft_wait_ms(wait_timeout_ms);
        let result = self
            .wait_for_delegated_task_result(
                &state.args.event_store,
                &state.args.actor_id,
                &task.task_id,
                session_id.clone(),
                thread_id.clone(),
                soft_wait_ms,
            )
            .await;

        let result = match result {
            Ok(result) => result,
            Err(ChatAgentError::Tool(err)) if err.contains("timed out") => {
                self.spawn_background_followup(
                    state,
                    task.task_id.clone(),
                    session_id,
                    thread_id,
                    wait_timeout_ms,
                    "terminal",
                );
                return Ok(ToolOutput {
                    success: true,
                    content: format!(
                        "Terminal task {} is running in the background. I will append results when it finishes.",
                        task.task_id
                    ),
                });
            }
            Err(err) => return Err(err),
        };

        match result.status {
            shared_types::DelegatedTaskStatus::Completed => Ok(ToolOutput {
                success: true,
                content: result
                    .output
                    .unwrap_or_else(|| "Terminal task completed (no output)".to_string()),
            }),
            shared_types::DelegatedTaskStatus::Failed => {
                Err(ChatAgentError::Tool(result.error.unwrap_or_else(|| {
                    "Delegated terminal task failed".to_string()
                })))
            }
            _ => Err(ChatAgentError::Tool(
                "Delegated task ended in unexpected state".to_string(),
            )),
        }
    }

    async fn delegate_research_tool(
        &self,
        state: &ChatAgentState,
        tool_args: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        default_model_override: Option<String>,
    ) -> Result<ToolOutput, ChatAgentError> {
        let Some(supervisor) = &state.args.application_supervisor else {
            return Err(ChatAgentError::Tool(
                "ApplicationSupervisor unavailable for research delegation".to_string(),
            ));
        };

        let parsed_args: serde_json::Value = serde_json::from_str(&tool_args)
            .map_err(|e| ChatAgentError::Serialization(e.to_string()))?;

        let query = parsed_args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChatAgentError::Validation("Missing 'query' argument".to_string()))?
            .to_string();
        let provider = parsed_args
            .get("provider")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let max_results = parsed_args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|v| v.clamp(1, 20) as u32);
        let time_range = parsed_args
            .get("time_range")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let include_domains = parsed_args
            .get("include_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            });
        let exclude_domains = parsed_args
            .get("exclude_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            });
        let timeout_ms = parsed_args.get("timeout_ms").and_then(|v| v.as_u64());
        let model_override = parsed_args
            .get("model")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or(default_model_override);
        let reasoning = parsed_args
            .get("reasoning")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);

        let researcher_id = match (&session_id, &thread_id) {
            (Some(session_id), Some(thread_id)) => {
                format!(
                    "research:{}:{}:{}",
                    state.args.actor_id, session_id, thread_id
                )
            }
            _ => format!("research:{}", state.args.actor_id),
        };

        let task = ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::DelegateResearchTask {
                researcher_id,
                actor_id: state.args.actor_id.clone(),
                user_id: state.args.user_id.clone(),
                query,
                provider,
                max_results,
                time_range,
                include_domains,
                exclude_domains,
                timeout_ms,
                model_override,
                reasoning,
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                reply,
            }
        })
        .map_err(|e| ChatAgentError::Tool(e.to_string()))?
        .map_err(ChatAgentError::Tool)?;

        let wait_timeout_ms = timeout_ms.unwrap_or(45_000).clamp(3_000, 120_000) + 2_000;
        let soft_wait_ms = Self::delegated_soft_wait_ms(wait_timeout_ms);
        let result = self
            .wait_for_delegated_task_result(
                &state.args.event_store,
                &state.args.actor_id,
                &task.task_id,
                session_id.clone(),
                thread_id.clone(),
                soft_wait_ms,
            )
            .await;

        let result = match result {
            Ok(result) => result,
            Err(ChatAgentError::Tool(err)) if err.contains("timed out") => {
                self.spawn_background_followup(
                    state,
                    task.task_id.clone(),
                    session_id,
                    thread_id,
                    wait_timeout_ms,
                    "research",
                );
                return Ok(ToolOutput {
                    success: true,
                    content: format!(
                        "Research task {} is running in the background. I will append findings when it finishes.",
                        task.task_id
                    ),
                });
            }
            Err(err) => return Err(err),
        };

        match result.status {
            shared_types::DelegatedTaskStatus::Completed => Ok(ToolOutput {
                success: true,
                content: result
                    .output
                    .unwrap_or_else(|| "Research task completed (no output)".to_string()),
            }),
            shared_types::DelegatedTaskStatus::Failed => {
                Err(ChatAgentError::Tool(result.error.unwrap_or_else(|| {
                    "Delegated research task failed".to_string()
                })))
            }
            _ => Err(ChatAgentError::Tool(
                "Delegated task ended in unexpected state".to_string(),
            )),
        }
    }

    async fn wait_for_delegated_task_result(
        &self,
        event_store: &ActorRef<EventStoreMsg>,
        actor_id: &str,
        task_id: &str,
        session_id: Option<String>,
        thread_id: Option<String>,
        timeout_ms: u64,
    ) -> Result<shared_types::DelegatedTaskResult, ChatAgentError> {
        Self::wait_for_delegated_task_result_internal(
            event_store,
            actor_id,
            task_id,
            session_id,
            thread_id,
            timeout_ms,
        )
        .await
    }

    async fn wait_for_delegated_task_result_internal(
        event_store: &ActorRef<EventStoreMsg>,
        actor_id: &str,
        task_id: &str,
        session_id: Option<String>,
        thread_id: Option<String>,
        timeout_ms: u64,
    ) -> Result<shared_types::DelegatedTaskResult, ChatAgentError> {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms.max(1_000));
        let mut since_seq = 0;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(ChatAgentError::Tool(format!(
                    "Delegated task timed out after {timeout_ms}ms while awaiting result"
                )));
            }

            let events = match (&session_id, &thread_id) {
                (Some(session_id), Some(thread_id)) => ractor::call!(event_store, |reply| {
                    EventStoreMsg::GetEventsForActorWithScope {
                        actor_id: actor_id.to_string(),
                        session_id: session_id.clone(),
                        thread_id: thread_id.clone(),
                        since_seq,
                        reply,
                    }
                })
                .map_err(|e| ChatAgentError::EventStore(e.to_string()))?
                .map_err(|e| ChatAgentError::EventStore(e.to_string()))?,
                _ => ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
                    actor_id: actor_id.to_string(),
                    since_seq,
                    reply,
                })
                .map_err(|e| ChatAgentError::EventStore(e.to_string()))?
                .map_err(|e| ChatAgentError::EventStore(e.to_string()))?,
            };

            for event in events {
                since_seq = since_seq.max(event.seq);
                let matches_task = event
                    .payload
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v == task_id)
                    .unwrap_or(false);
                if !matches_task {
                    continue;
                }

                let event_status = event
                    .payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let status = match event_status {
                    "completed" => shared_types::DelegatedTaskStatus::Completed,
                    "failed" => shared_types::DelegatedTaskStatus::Failed,
                    "running" => shared_types::DelegatedTaskStatus::Running,
                    "accepted" => shared_types::DelegatedTaskStatus::Accepted,
                    _ => continue,
                };
                if !matches!(
                    status,
                    shared_types::DelegatedTaskStatus::Completed
                        | shared_types::DelegatedTaskStatus::Failed
                ) {
                    continue;
                }

                return Ok(shared_types::DelegatedTaskResult {
                    task_id: task_id.to_string(),
                    correlation_id: event
                        .payload
                        .get("correlation_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    status,
                    output: event
                        .payload
                        .get("output")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                        .or_else(|| Self::delegated_result_output(&event.payload)),
                    error: event
                        .payload
                        .get("error")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string),
                    started_at: event.timestamp.to_rfc3339(),
                    finished_at: event
                        .payload
                        .get("finished_at")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string),
                });
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

#[async_trait]
impl Actor for ChatAgent {
    type Msg = ChatAgentMsg;
    type State = ChatAgentState;
    type Arguments = ChatAgentArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            chat_actor_id = %args.actor_id,
            user_id = %args.user_id,
            "ChatAgent starting"
        );

        let messages = match ractor::call!(args.event_store.clone(), |reply| {
            match (&args.preload_session_id, &args.preload_thread_id) {
                (Some(session_id), Some(thread_id)) => EventStoreMsg::GetEventsForActorWithScope {
                    actor_id: args.actor_id.clone(),
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    since_seq: 0,
                    reply,
                },
                _ => EventStoreMsg::GetEventsForActor {
                    actor_id: args.actor_id.clone(),
                    since_seq: 0,
                    reply,
                },
            }
        }) {
            Ok(Ok(events)) => Self::history_from_events(events),
            Ok(Err(e)) => {
                tracing::warn!(
                    actor_id = %args.actor_id,
                    error = %e,
                    "Failed to load conversation history"
                );
                Vec::new()
            }
            Err(e) => {
                tracing::warn!(
                    actor_id = %args.actor_id,
                    error = %e,
                    "Failed to contact EventStore during history load"
                );
                Vec::new()
            }
        };

        Ok(ChatAgentState {
            args,
            messages,
            current_model: std::env::var("CHOIR_CHAT_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| load_model_policy().chat_default_model)
                .unwrap_or_else(|| "ClaudeBedrockSonnet45".to_string()),
            model_registry: ModelRegistry::new(),
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "ChatAgent started successfully"
        );
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ChatAgentMsg::ProcessMessage {
                text,
                session_id,
                thread_id,
                model_override,
                reply,
            } => {
                let result = self
                    .handle_process_message(state, text, session_id, thread_id, model_override)
                    .await;
                let _ = reply.send(result);
            }
            ChatAgentMsg::SwitchModel { model, reply } => {
                let result = self.handle_switch_model(state, model);
                let _ = reply.send(result);
            }
            ChatAgentMsg::GetConversationHistory { reply } => {
                let history = self.handle_get_conversation_history(state);
                let _ = reply.send(history);
            }
            ChatAgentMsg::GetAvailableTools { reply } => {
                let tools = self.handle_get_available_tools(state);
                let _ = reply.send(tools);
            }
        }

        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "ChatAgent stopped"
        );
        Ok(())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Convenience function to process a message
pub async fn process_message(
    agent: &ActorRef<ChatAgentMsg>,
    text: impl Into<String>,
) -> Result<Result<AgentResponse, ChatAgentError>, ractor::RactorErr<ChatAgentMsg>> {
    ractor::call!(agent, |reply| ChatAgentMsg::ProcessMessage {
        text: text.into(),
        session_id: None,
        thread_id: None,
        model_override: None,
        reply,
    })
}

/// Convenience function to switch model
pub async fn switch_model(
    agent: &ActorRef<ChatAgentMsg>,
    model: impl Into<String>,
) -> Result<Result<(), ChatAgentError>, ractor::RactorErr<ChatAgentMsg>> {
    ractor::call!(agent, |reply| ChatAgentMsg::SwitchModel {
        model: model.into(),
        reply,
    })
}

/// Convenience function to get conversation history
pub async fn get_conversation_history(
    agent: &ActorRef<ChatAgentMsg>,
) -> Result<Vec<BamlMessage>, ractor::RactorErr<ChatAgentMsg>> {
    ractor::call!(agent, |reply| ChatAgentMsg::GetConversationHistory {
        reply
    })
}

/// Convenience function to get available tools
pub async fn get_available_tools(
    agent: &ActorRef<ChatAgentMsg>,
) -> Result<Vec<String>, ractor::RactorErr<ChatAgentMsg>> {
    ractor::call!(agent, |reply| ChatAgentMsg::GetAvailableTools { reply })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use chrono::TimeZone;
    use ractor::Actor;

    #[test]
    fn test_timestamped_prompt_content_prefixes_iso_timestamp() {
        let ts = Utc.with_ymd_and_hms(2026, 2, 8, 20, 49, 43).unwrap();
        let stamped = ChatAgent::timestamped_prompt_content("hello world", ts);
        assert!(stamped.starts_with("[2026-02-08T20:49:43Z]"));
        assert!(stamped.ends_with("hello world"));
    }

    #[test]
    fn test_delegated_result_output_falls_back_to_summary() {
        let payload = serde_json::json!({
            "summary": "Research summary output"
        });
        let output = ChatAgent::delegated_result_output(&payload);
        assert_eq!(output.as_deref(), Some("Research summary output"));
    }

    #[tokio::test]
    async fn test_chat_agent_creation() {
        let (event_store_ref, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: "agent-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
                preload_session_id: None,
                preload_thread_id: None,
                application_supervisor: None,
            },
        )
        .await
        .unwrap();

        let tools = get_available_tools(&agent_ref).await.unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"bash".to_string()));
        assert!(tools.contains(&"web_search".to_string()));

        agent_ref.stop(None);
        event_store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_model_switching() {
        let (event_store_ref, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: "agent-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
                preload_session_id: None,
                preload_thread_id: None,
                application_supervisor: None,
            },
        )
        .await
        .unwrap();

        let result = switch_model(&agent_ref, "GLM47").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());

        let result = switch_model(&agent_ref, "InvalidModel").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());

        agent_ref.stop(None);
        event_store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_per_request_model_override_validation() {
        let (event_store_ref, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: "agent-override-test".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
                preload_session_id: None,
                preload_thread_id: None,
                application_supervisor: None,
            },
        )
        .await
        .unwrap();

        let result = ractor::call!(agent_ref, |reply| ChatAgentMsg::ProcessMessage {
            text: "hello".to_string(),
            session_id: None,
            thread_id: None,
            model_override: Some("NotARealModel".to_string()),
            reply,
        })
        .expect("chat agent call should succeed");

        assert!(matches!(result, Err(ChatAgentError::InvalidModel(_))));

        agent_ref.stop(None);
        event_store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_history_loaded_from_event_store() {
        let (event_store_ref, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let actor_id = "history-actor".to_string();

        let _ = ractor::call!(event_store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                payload: serde_json::json!("Hello"),
                actor_id: actor_id.clone(),
                user_id: "user-1".to_string(),
            },
            reply,
        })
        .unwrap()
        .unwrap();

        let _ = ractor::call!(event_store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                payload: serde_json::json!({"text": "Hi there"}),
                actor_id: actor_id.clone(),
                user_id: "system".to_string(),
            },
            reply,
        })
        .unwrap()
        .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id,
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
                preload_session_id: None,
                preload_thread_id: None,
                application_supervisor: None,
            },
        )
        .await
        .unwrap();

        let history = get_conversation_history(&agent_ref).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "Hi there");

        agent_ref.stop(None);
        event_store_ref.stop(None);
    }
}
