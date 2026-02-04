//! ChatAgent - BAML-powered agent with tool execution
//!
//! This actor combines BAML LLM planning with tool execution to provide
//! an intelligent chat interface with file system access.
//!
//! Converted from Actix to ractor actor model.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::HashMap;
use std::sync::Arc;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::baml_client::types::{Message as BamlMessage, ToolResult};
use crate::baml_client::ClientRegistry;
use crate::tools::{ToolError, ToolOutput, ToolRegistry};

/// ChatAgent - AI assistant with planning and tool execution capabilities
pub struct ChatAgent {
    actor_id: String,
    user_id: String,
    messages: Vec<BamlMessage>,
    tool_registry: Arc<ToolRegistry>,
    current_model: String,
    conversation_context: HashMap<String, String>,
}

/// Arguments for spawning ChatAgent
#[derive(Debug, Clone)]
pub struct ChatAgentArguments {
    pub actor_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
}

/// State for ChatAgent
pub struct ChatAgentState {
    args: ChatAgentArguments,
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
    GetAvailableTools {
        reply: RpcReplyPort<Vec<String>>,
    },
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
}

impl From<serde_json::Error> for ChatAgentError {
    fn from(e: serde_json::Error) -> Self {
        ChatAgentError::Serialization(e.to_string())
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

impl ChatAgent {
    /// Create a new ChatAgent actor instance
    pub fn new() -> Self {
        Self {
            actor_id: String::new(),
            user_id: String::new(),
            messages: Vec::new(),
            tool_registry: Arc::new(ToolRegistry::new()),
            current_model: "ClaudeBedrock".to_string(),
            conversation_context: HashMap::new(),
        }
    }

    /// Get available tools description for BAML prompt
    fn get_tools_description(&self) -> String {
        self.tool_registry.descriptions()
    }

    /// Get system context for agent planning
    fn get_system_context(&self, state: &ChatAgentState) -> String {
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
            self.actor_id,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
        )
    }

    /// Log an event to the EventStore using RPC
    async fn log_event(
        &self,
        state: &ChatAgentState,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), ChatAgentError> {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.actor_id.clone(),
            user_id: self.user_id.clone(),
        };

        let result = ractor::call!(
            state.args.event_store,
            |reply| EventStoreMsg::Append { event, reply }
        );

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ChatAgentError::EventStore(e.to_string())),
            Err(e) => Err(ChatAgentError::EventStore(e.to_string())),
        }
    }

    /// Handle ProcessMessage
    async fn handle_process_message(
        &mut self,
        state: &ChatAgentState,
        text: String,
    ) -> Result<AgentResponse, ChatAgentError> {
        let actor_id = self.actor_id.clone();
        let tools_description = self.get_tools_description();
        let system_context = self.get_system_context(state);
        let current_model = self.current_model.clone();

        // Add user message to conversation history
        self.messages.push(BamlMessage {
            role: "user".to_string(),
            content: text.clone(),
        });

        let messages = self.messages.clone();
        let user_text_async = text.clone();

        // Create client registry
        let client_registry = {
            let mut cr = ClientRegistry::new();
            let mut bedrock_options = std::collections::HashMap::new();
            bedrock_options.insert(
                "model".to_string(),
                serde_json::json!("us.anthropic.claude-opus-4-5-20251101-v1:0"),
            );
            bedrock_options.insert("region".to_string(), serde_json::json!("us-east-1"));
            cr.add_llm_client("ClaudeBedrock", "aws-bedrock", bedrock_options);
            cr
        };

        // Step 1: Plan the action using BAML
        let plan_result = crate::baml_client::B
            .PlanAction
            .with_client_registry(&client_registry)
            .call(&messages, &system_context, &tools_description)
            .await;

        let plan = match plan_result {
            Ok(p) => p,
            Err(e) => return Err(ChatAgentError::Baml(e.to_string())),
        };

        tracing::info!(
            actor_id = %actor_id,
            thinking = %plan.thinking,
            confidence = %plan.confidence,
            tool_count = plan.tool_calls.len(),
            "Agent planned action"
        );

        // Step 2: Execute tool calls if any
        let mut executed_tools: Vec<ExecutedToolCall> = Vec::new();
        let mut tool_results: Vec<ToolResult> = Vec::new();

        for tool_call in &plan.tool_calls {
            tracing::info!(
                tool = %tool_call.tool_name,
                args = %tool_call.tool_args,
                "Executing tool"
            );

            // Execute the tool
            let result = execute_tool_impl(&tool_call.tool_name, &tool_call.tool_args).await;

            match result {
                Ok(output) => {
                    tracing::info!(
                        tool = %tool_call.tool_name,
                        success = %output.success,
                        "Tool executed"
                    );

                    executed_tools.push(ExecutedToolCall {
                        tool_name: tool_call.tool_name.clone(),
                        tool_args: tool_call.tool_args.clone(),
                        reasoning: tool_call.reasoning.clone(),
                        result: output.clone(),
                    });

                    tool_results.push(ToolResult {
                        tool_name: tool_call.tool_name.clone(),
                        success: output.success,
                        output: output.content.clone(),
                        error: None,
                    });
                }
                Err(e) => {
                    tracing::error!(
                        tool = %tool_call.tool_name,
                        error = %e,
                        "Tool execution failed"
                    );

                    tool_results.push(ToolResult {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(e.message.clone()),
                    });
                }
            }
        }

        // Step 3: Synthesize response
        let conversation_context = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let response_text = if let Some(final_response) = plan.final_response {
            // Agent already provided a final response
            final_response
        } else {
            // Need to synthesize from tool results
            match crate::baml_client::B
                .SynthesizeResponse
                .with_client_registry(&client_registry)
                .call(&user_text_async, &tool_results, &conversation_context)
                .await
            {
                Ok(text) => text,
                Err(e) => return Err(ChatAgentError::Baml(e.to_string())),
            }
        };

        let response = AgentResponse {
            text: response_text.clone(),
            tool_calls: executed_tools.clone(),
            thinking: plan.thinking.clone(),
            confidence: plan.confidence,
            model_used: current_model.clone(),
        };

        // Add assistant response to conversation history
        self.messages.push(BamlMessage {
            role: "assistant".to_string(),
            content: response_text.clone(),
        });

        // Log events for the interaction (fire and forget)
        let event_store = state.args.event_store.clone();
        let actor_id_log = self.actor_id.clone();
        let user_id_log = self.user_id.clone();
        let user_text_log = text.clone();
        let response_clone = response.clone();

        tokio::spawn(async move {
            // Log user message
            let _ = ractor::call!(
                event_store.clone(),
                |reply| EventStoreMsg::Append {
                    event: AppendEvent {
                        event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                        payload: serde_json::json!(user_text_log),
                        actor_id: actor_id_log.clone(),
                        user_id: user_id_log.clone(),
                    },
                    reply,
                }
            );

            // Log assistant response
            let _ = ractor::call!(
                event_store.clone(),
                |reply| EventStoreMsg::Append {
                    event: AppendEvent {
                        event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                        payload: serde_json::json!({
                            "text": response_clone.text,
                            "thinking": response_clone.thinking,
                            "confidence": response_clone.confidence,
                            "model": response_clone.model_used,
                            "tools_used": response_clone.tool_calls.len(),
                        }),
                        actor_id: actor_id_log.clone(),
                        user_id: "system".to_string(),
                    },
                    reply,
                }
            );

            // Log tool calls
            for tool in &response_clone.tool_calls {
                let _ = ractor::call!(
                    event_store.clone(),
                    |reply| EventStoreMsg::Append {
                        event: AppendEvent {
                            event_type: shared_types::EVENT_CHAT_TOOL_CALL.to_string(),
                            payload: serde_json::json!({
                                "tool_name": tool.tool_name,
                                "tool_args": tool.tool_args,
                                "reasoning": tool.reasoning,
                                "success": tool.result.success,
                                "output_preview": &tool.result.content.chars().take(200).collect::<String>(),
                            }),
                            actor_id: actor_id_log.clone(),
                            user_id: user_id_log.clone(),
                        },
                        reply,
                    }
                );
            }
        });

        Ok(response)
    }

    /// Handle SwitchModel
    fn handle_switch_model(&mut self, model: String) -> Result<(), ChatAgentError> {
        match model.as_str() {
            "ClaudeBedrock" | "GLM47" => {
                tracing::info!(
                    actor_id = %self.actor_id,
                    old_model = %self.current_model,
                    new_model = %model,
                    "Switching model"
                );
                self.current_model = model;
                Ok(())
            }
            _ => Err(ChatAgentError::InvalidModel(format!(
                "Unknown model: {}. Available: ClaudeBedrock, GLM47",
                model
            ))),
        }
    }

    /// Handle GetConversationHistory
    fn handle_get_conversation_history(&self) -> Vec<BamlMessage> {
        self.messages.clone()
    }

    /// Handle GetAvailableTools
    fn handle_get_available_tools(&self) -> Vec<String> {
        self.tool_registry.available_tools()
    }

    /// Handle ExecuteTool
    async fn handle_execute_tool(
        &self,
        tool_name: String,
        tool_args: String,
    ) -> Result<ToolOutput, ToolError> {
        execute_tool_impl(&tool_name, &tool_args).await
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

        Ok(ChatAgentState { args })
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
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // We need to handle messages that require mutable self
        // Since handle takes &self, we use interior mutability pattern
        // For now, we'll process the message and use unsafe to get mutable access
        // This is a temporary solution - in production, use proper interior mutability
        
        // Actually, we need to restructure this. Let's use a different approach
        // where we spawn a task for async operations and use channels for state updates
        
        match message {
            ChatAgentMsg::ProcessMessage { text, reply } => {
                // For async operations, we need to clone data and spawn
                let actor_id = self.actor_id.clone();
                let user_id = self.user_id.clone();
                let messages = self.messages.clone();
                let tool_registry = self.tool_registry.clone();
                let current_model = self.current_model.clone();
                let conversation_context = self.conversation_context.clone();
                let state_clone = ChatAgentState {
                    args: state.args.clone(),
                };
                
                let handle = tokio::spawn(async move {
                    let mut agent = ChatAgent {
                        actor_id,
                        user_id,
                        messages,
                        tool_registry,
                        current_model,
                        conversation_context,
                    };
                    agent.handle_process_message(&state_clone, text).await
                });
                
                match handle.await {
                    Ok(result) => {
                        let _ = reply.send(result);
                    }
                    Err(e) => {
                        let _ = reply.send(Err(ChatAgentError::Baml(format!("Task join error: {}", e))));
                    }
                }
            }
            ChatAgentMsg::SwitchModel { model, reply } => {
                // For sync operations, we can use a similar pattern
                let actor_id = self.actor_id.clone();
                let user_id = self.user_id.clone();
                let messages = self.messages.clone();
                let tool_registry = self.tool_registry.clone();
                let current_model = self.current_model.clone();
                let conversation_context = self.conversation_context.clone();
                
                let mut agent = ChatAgent {
                    actor_id,
                    user_id,
                    messages,
                    tool_registry,
                    current_model,
                    conversation_context,
                };
                
                let result = agent.handle_switch_model(model);
                let _ = reply.send(result);
            }
            ChatAgentMsg::GetConversationHistory { reply } => {
                let history = self.handle_get_conversation_history();
                let _ = reply.send(history);
            }
            ChatAgentMsg::GetAvailableTools { reply } => {
                let tools = self.handle_get_available_tools();
                let _ = reply.send(tools);
            }
            ChatAgentMsg::ExecuteTool {
                tool_name,
                tool_args,
                reply,
            } => {
                let actor_id = self.actor_id.clone();
                let user_id = self.user_id.clone();
                let messages = self.messages.clone();
                let tool_registry = self.tool_registry.clone();
                let current_model = self.current_model.clone();
                let conversation_context = self.conversation_context.clone();
                
                let handle = tokio::spawn(async move {
                    let agent = ChatAgent {
                        actor_id,
                        user_id,
                        messages,
                        tool_registry,
                        current_model,
                        conversation_context,
                    };
                    agent.handle_execute_tool(tool_name, tool_args).await
                });
                
                match handle.await {
                    Ok(result) => {
                        let _ = reply.send(result);
                    }
                    Err(e) => {
                        let _ = reply.send(Err(ToolError::new(format!("Task join error: {}", e))));
                    }
                }
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
async fn execute_tool_impl(tool_name: &str, tool_args: &str) -> Result<ToolOutput, ToolError> {
    let registry = ToolRegistry::new();

    let args: serde_json::Value = serde_json::from_str(tool_args)
        .map_err(|e| ToolError::new(format!("Invalid tool arguments: {e}")))?;

    registry.execute(tool_name, args)
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
    ractor::call!(agent, |reply| ChatAgentMsg::GetConversationHistory { reply })
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
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: "agent-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
            },
        )
        .await
        .unwrap();

        // Test getting available tools
        let tools = get_available_tools(&agent_ref).await.unwrap();
        assert!(!tools.is_empty());
        assert!(tools.contains(&"bash".to_string()));
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));

        // Cleanup
        agent_ref.stop(None);
        event_store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_model_switching() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (agent_ref, _agent_handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: "agent-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref.clone(),
            },
        )
        .await
        .unwrap();

        // Test valid model switch
        let result = switch_model(&agent_ref, "GLM47").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());

        // Test invalid model
        let result = switch_model(&agent_ref, "InvalidModel").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());

        // Cleanup
        agent_ref.stop(None);
        event_store_ref.stop(None);
    }
}
