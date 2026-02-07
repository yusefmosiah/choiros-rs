//! ChatAgent - BAML-powered agent with tool execution
//!
//! This actor combines BAML LLM planning with tool execution to provide
//! an intelligent chat interface with file system access.
//!
//! Converted from Actix to ractor actor model.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::sync::Arc;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::baml_client::types::{Message as BamlMessage, ToolResult};
use crate::baml_client::ClientRegistry;
use crate::supervisor::ApplicationSupervisorMsg;
use crate::tools::{ToolError, ToolOutput, ToolRegistry};

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
    tool_registry: Arc<ToolRegistry>,
    current_model: String,
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
    /// Execute a specific tool (for testing/debugging)
    ExecuteTool {
        tool_name: String,
        tool_args: String,
        reply: RpcReplyPort<Result<ToolOutput, ToolError>>,
    },
}

/// Agent response structure
#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub text: String,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub thinking: String,
    pub confidence: f64,
    pub model_used: String,
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

    fn get_tools_description(state: &ChatAgentState) -> String {
        state.tool_registry.descriptions()
    }

    fn get_system_context(state: &ChatAgentState) -> String {
        format!(
            r#"You are ChoirOS, an AI assistant in a web desktop environment.

User ID: {}
Actor ID: {}
Working Directory: {}

You have access to tools for:
- Executing bash commands
- Reading files
- Writing files
- Listing directories
- Searching files

Be helpful, accurate, and concise. Use tools when needed to complete user requests."#,
            state.args.user_id,
            state.args.actor_id,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
        )
    }

    fn create_client_registry(current_model: &str) -> ClientRegistry {
        let mut cr = ClientRegistry::new();

        match current_model {
            "GLM47" => {
                let mut glm_options = std::collections::HashMap::new();
                glm_options.insert("api_key".to_string(), serde_json::json!("env.ZAI_API_KEY"));
                glm_options.insert(
                    "base_url".to_string(),
                    serde_json::json!("https://api.z.ai/api/anthropic"),
                );
                glm_options.insert("model".to_string(), serde_json::json!("glm-4.7"));
                // Keep alias as ClaudeBedrock since BAML functions target this client name.
                cr.add_llm_client("ClaudeBedrock", "anthropic", glm_options);
            }
            _ => {
                let mut bedrock_options = std::collections::HashMap::new();
                bedrock_options.insert(
                    "model".to_string(),
                    serde_json::json!("us.anthropic.claude-opus-4-5-20251101-v1:0"),
                );
                bedrock_options.insert("region".to_string(), serde_json::json!("us-east-1"));
                cr.add_llm_client("ClaudeBedrock", "aws-bedrock", bedrock_options);
            }
        }

        cr
    }

    fn history_from_events(events: Vec<shared_types::Event>) -> Vec<BamlMessage> {
        let mut history = Vec::new();

        for event in events {
            match event.event_type.as_str() {
                shared_types::EVENT_CHAT_USER_MSG => {
                    if let Some(text) = shared_types::parse_chat_user_text(&event.payload) {
                        history.push(BamlMessage {
                            role: "user".to_string(),
                            content: text,
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
                            content: text,
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

    /// Handle ProcessMessage
    async fn handle_process_message(
        &self,
        state: &mut ChatAgentState,
        text: String,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<AgentResponse, ChatAgentError> {
        let user_text = text.trim().to_string();
        if user_text.is_empty() {
            return Err(ChatAgentError::Validation(
                "Message cannot be empty".to_string(),
            ));
        }

        state.messages.push(BamlMessage {
            role: "user".to_string(),
            content: user_text.clone(),
        });

        let tools_description = Self::get_tools_description(state);
        let system_context = Self::get_system_context(state);
        let current_model = state.current_model.clone();
        let client_registry = Self::create_client_registry(&current_model);

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
            self.log_event(
                state,
                shared_types::EVENT_CHAT_TOOL_CALL,
                serde_json::json!({
                    "tool_name": tool_call.tool_name,
                    "tool_args": tool_call.tool_args,
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
                    tool_call.tool_args.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                )
                .await
                .map_err(|e| ToolError::new(e.to_string()))
            } else {
                execute_tool_impl(
                    state.tool_registry.clone(),
                    tool_call.tool_name.clone(),
                    tool_call.tool_args.clone(),
                )
                .await
            };

            match result {
                Ok(output) => {
                    executed_tools.push(ExecutedToolCall {
                        tool_name: tool_call.tool_name.clone(),
                        tool_args: tool_call.tool_args.clone(),
                        reasoning: tool_call.reasoning.clone(),
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
                            "error": e.message,
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
                        error: Some(e.message),
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
            crate::baml_client::B
                .SynthesizeResponse
                .with_client_registry(&client_registry)
                .call(&user_text, &tool_results, &conversation_context)
                .await
                .map_err(|e| ChatAgentError::Baml(e.to_string()))?
        };

        state.messages.push(BamlMessage {
            role: "assistant".to_string(),
            content: response_text.clone(),
        });

        self.log_event(
            state,
            shared_types::EVENT_CHAT_ASSISTANT_MSG,
            serde_json::json!({
                "text": response_text,
                "thinking": plan.thinking,
                "confidence": plan.confidence,
                "model": current_model,
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
            model_used: state.current_model.clone(),
        })
    }

    /// Handle SwitchModel
    fn handle_switch_model(
        &self,
        state: &mut ChatAgentState,
        model: String,
    ) -> Result<(), ChatAgentError> {
        match model.as_str() {
            "ClaudeBedrock" | "GLM47" => {
                tracing::info!(
                    actor_id = %state.args.actor_id,
                    old_model = %state.current_model,
                    new_model = %model,
                    "Switching model"
                );
                state.current_model = model;
                Ok(())
            }
            _ => Err(ChatAgentError::InvalidModel(format!(
                "Unknown model: {model}. Available: ClaudeBedrock, GLM47"
            ))),
        }
    }

    /// Handle GetConversationHistory
    fn handle_get_conversation_history(&self, state: &ChatAgentState) -> Vec<BamlMessage> {
        state.messages.clone()
    }

    /// Handle GetAvailableTools
    fn handle_get_available_tools(&self, state: &ChatAgentState) -> Vec<String> {
        state.tool_registry.available_tools()
    }

    /// Handle ExecuteTool
    async fn handle_execute_tool(
        &self,
        state: &ChatAgentState,
        tool_name: String,
        tool_args: String,
    ) -> Result<ToolOutput, ToolError> {
        if tool_name == "bash" {
            return self
                .delegate_terminal_tool(state, tool_args, None, None)
                .await
                .map_err(|e| ToolError::new(e.to_string()));
        }

        execute_tool_impl(state.tool_registry.clone(), tool_name, tool_args).await
    }

    async fn delegate_terminal_tool(
        &self,
        state: &ChatAgentState,
        tool_args: String,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<ToolOutput, ChatAgentError> {
        let Some(supervisor) = &state.args.application_supervisor else {
            return Err(ChatAgentError::Tool(
                "ApplicationSupervisor unavailable for delegation".to_string(),
            ));
        };

        let parsed_args: serde_json::Value = serde_json::from_str(&tool_args)
            .map_err(|e| ChatAgentError::Serialization(e.to_string()))?;
        let command = parsed_args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChatAgentError::Validation("Missing 'command' argument".to_string()))?
            .to_string();
        let timeout_ms = parsed_args.get("timeout_ms").and_then(|v| v.as_u64());

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
                working_dir: ".".to_string(),
                command: command.clone(),
                timeout_ms,
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                reply,
            }
        })
        .map_err(|e| ChatAgentError::Tool(e.to_string()))?
        .map_err(ChatAgentError::Tool)?;

        let wait_timeout_ms = timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000) + 2_000;
        let result = self
            .wait_for_delegated_task_result(
                &state.args.event_store,
                &state.args.actor_id,
                &task.task_id,
                session_id,
                thread_id,
                wait_timeout_ms,
            )
            .await?;

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

    async fn wait_for_delegated_task_result(
        &self,
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
                    "Delegated terminal task timed out after {timeout_ms}ms while awaiting result"
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
                        .map(ToString::to_string),
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
            tool_registry: Arc::new(ToolRegistry::new()),
            current_model: "ClaudeBedrock".to_string(),
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
                reply,
            } => {
                let result = self
                    .handle_process_message(state, text, session_id, thread_id)
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
            ChatAgentMsg::ExecuteTool {
                tool_name,
                tool_args,
                reply,
            } => {
                let result = self.handle_execute_tool(state, tool_name, tool_args).await;
                let _ = reply.send(result);
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

// Helper function to execute a tool outside of the actor context
async fn execute_tool_impl(
    registry: Arc<ToolRegistry>,
    tool_name: String,
    tool_args: String,
) -> Result<ToolOutput, ToolError> {
    let args: serde_json::Value = serde_json::from_str(&tool_args)
        .map_err(|e| ToolError::new(format!("Invalid tool arguments: {e}")))?;

    tokio::task::spawn_blocking(move || registry.execute(&tool_name, args))
        .await
        .map_err(|e| ToolError::new(format!("Tool execution task failed: {e}")))?
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

/// Convenience function to execute a tool
pub async fn execute_tool(
    agent: &ActorRef<ChatAgentMsg>,
    tool_name: impl Into<String>,
    tool_args: impl Into<String>,
) -> Result<Result<ToolOutput, ToolError>, ractor::RactorErr<ChatAgentMsg>> {
    ractor::call!(agent, |reply| ChatAgentMsg::ExecuteTool {
        tool_name: tool_name.into(),
        tool_args: tool_args.into(),
        reply,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

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
        assert!(!tools.is_empty());
        assert!(tools.contains(&"bash".to_string()));
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));

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
