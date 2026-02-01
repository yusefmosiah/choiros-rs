//! ChatAgent - BAML-powered agent with tool execution
//!
//! This actor combines BAML LLM planning with tool execution to provide
//! an intelligent chat interface with file system access.

use actix::{
    Actor, ActorFutureExt, Context, Handler, Message as ActixMessage, ResponseActFuture, WrapFuture,
};
use std::collections::HashMap;

use crate::actors::event_store::{AppendEvent, EventStoreActor};
use crate::baml_client::types::{Message as BamlMessage, ToolResult};
use crate::baml_client::ClientRegistry;
use crate::tools::{ToolError, ToolOutput, ToolRegistry};

/// ChatAgent - AI assistant with planning and tool execution capabilities
pub struct ChatAgent {
    actor_id: String,
    user_id: String,
    messages: Vec<BamlMessage>,
    tool_registry: ToolRegistry,
    current_model: String,
    event_store: Option<actix::Addr<EventStoreActor>>,
    conversation_context: HashMap<String, String>,
}

impl ChatAgent {
    /// Create a new ChatAgent
    pub fn new(
        actor_id: String,
        user_id: String,
        event_store: actix::Addr<EventStoreActor>,
    ) -> Self {
        Self {
            actor_id,
            user_id,
            messages: Vec::new(),
            tool_registry: ToolRegistry::new(),
            current_model: "ClaudeBedrock".to_string(),
            event_store: Some(event_store),
            conversation_context: HashMap::new(),
        }
    }

    /// Create a client registry with configured LLM clients
    fn create_client_registry(&self) -> ClientRegistry {
        let mut cr = ClientRegistry::new();

        // ClaudeBedrock client (uses AWS env credentials)
        let mut bedrock_options = std::collections::HashMap::new();
        bedrock_options.insert(
            "model".to_string(),
            serde_json::json!("us.anthropic.claude-opus-4-5-20251101-v1:0"),
        );
        bedrock_options.insert("region".to_string(), serde_json::json!("us-east-1"));
        cr.add_llm_client("ClaudeBedrock", "aws-bedrock", bedrock_options);

        // GLM47 client (uses ZAI_API_KEY)
        if let Ok(api_key) = std::env::var("ZAI_API_KEY") {
            let mut glm47_options = std::collections::HashMap::new();
            glm47_options.insert("model".to_string(), serde_json::json!("glm-4.7"));
            glm47_options.insert(
                "base_url".to_string(),
                serde_json::json!("https://api.z.ai/api/anthropic"),
            );
            glm47_options.insert("api_key".to_string(), serde_json::json!(api_key));
            cr.add_llm_client("GLM47", "openai-generic", glm47_options);
        }

        cr
    }

    /// Get available tools description for BAML prompt
    fn get_tools_description(&self) -> String {
        self.tool_registry.descriptions()
    }

    /// Get system context for agent planning
    fn get_system_context(&self) -> String {
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
            self.user_id,
            self.actor_id,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
        )
    }

    /// Log an event to the EventStore
    async fn log_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), ChatAgentError> {
        if let Some(event_store) = &self.event_store {
            let _ = event_store
                .send(AppendEvent {
                    event_type: event_type.to_string(),
                    payload,
                    actor_id: self.actor_id.clone(),
                    user_id: self.user_id.clone(),
                })
                .await
                .map_err(|e| ChatAgentError::EventStore(e.to_string()));
        }
        Ok(())
    }
}

impl Actor for ChatAgent {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(
            actor_id = %self.actor_id,
            user_id = %self.user_id,
            "ChatAgent started"
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(
            actor_id = %self.actor_id,
            "ChatAgent stopped"
        );
    }
}

// Implement Supervised for fault tolerance
impl actix::Supervised for ChatAgent {
    fn restarting(&mut self, _ctx: &mut Context<Self>) {
        // Clear in-memory state but keep identity
        tracing::info!(
            actor_id = %self.actor_id,
            "ChatAgent restarting - clearing state"
        );
        self.messages.clear();
        self.conversation_context.clear();
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Process a user message and return agent response
#[derive(ActixMessage)]
#[rtype(result = "Result<AgentResponse, ChatAgentError>")]
pub struct ProcessMessage {
    pub text: String,
}

/// Switch between available LLM models
#[derive(ActixMessage)]
#[rtype(result = "Result<(), ChatAgentError>")]
pub struct SwitchModel {
    pub model: String, // "ClaudeBedrock" or "GLM47"
}

/// Get conversation history
#[derive(ActixMessage)]
#[rtype(result = "Vec<BamlMessage>")]
pub struct GetConversationHistory;

/// Get available tools list
#[derive(ActixMessage)]
#[rtype(result = "Vec<String>")]
pub struct GetAvailableTools;

/// Execute a specific tool (for testing/debugging)
#[derive(ActixMessage)]
#[rtype(result = "Result<ToolOutput, ToolError>")]
pub struct ExecuteTool {
    pub tool_name: String,
    pub tool_args: String,
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

#[derive(Debug, thiserror::Error)]
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
    Serialization(#[from] serde_json::Error),

    #[error("Invalid model: {0}")]
    InvalidModel(String),
}

// ============================================================================
// Handlers
// ============================================================================

impl Handler<ProcessMessage> for ChatAgent {
    type Result = ResponseActFuture<Self, Result<AgentResponse, ChatAgentError>>;

    fn handle(&mut self, msg: ProcessMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let actor_id = self.actor_id.clone();
        let tools_description = self.get_tools_description();
        let system_context = self.get_system_context();
        let current_model = self.current_model.clone();

        // Add user message to conversation history
        self.messages.push(BamlMessage {
            role: "user".to_string(),
            content: msg.text.clone(),
        });

        // Clone necessary data for the async block
        let messages = self.messages.clone();
        let user_text_async = msg.text.clone();
        let user_text_result = msg.text.clone();

        Box::pin(
            async move {
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
                    let result =
                        execute_tool_impl(&tool_call.tool_name, &tool_call.tool_args).await;

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

                Ok(AgentResponse {
                    text: response_text,
                    tool_calls: executed_tools,
                    thinking: plan.thinking,
                    confidence: plan.confidence,
                    model_used: current_model,
                })
            }
            .into_actor(self)
            .map(move |result, actor, _ctx| {
                match &result {
                    Ok(response) => {
                        // Add assistant response to conversation history
                        actor.messages.push(BamlMessage {
                            role: "assistant".to_string(),
                            content: response.text.clone(),
                        });

                        // Log events for the interaction (fire and forget)
                        let actor_id = actor.actor_id.clone();
                        let user_id = actor.user_id.clone();
                        let event_store = actor.event_store.clone();
                        let user_text_log = user_text_result.clone();
                        let response_clone = response.clone();

                        actix::spawn(async move {
                            if let Some(es) = event_store {
                                // Log user message
                                let _ = es
                                    .send(AppendEvent {
                                        event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                                        payload: serde_json::json!(user_text_log),
                                        actor_id: actor_id.clone(),
                                        user_id: user_id.clone(),
                                    })
                                    .await;

                                // Log assistant response
                                let _ = es
                                    .send(AppendEvent {
                                        event_type:
                                            shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                                        payload: serde_json::json!({
                                            "text": response_clone.text,
                                            "thinking": response_clone.thinking,
                                            "confidence": response_clone.confidence,
                                            "model": response_clone.model_used,
                                            "tools_used": response_clone.tool_calls.len(),
                                        }),
                                        actor_id: actor_id.clone(),
                                        user_id: "system".to_string(),
                                    })
                                    .await;

                                // Log tool calls
                                for tool in &response_clone.tool_calls {
                                    let _ = es
                                        .send(AppendEvent {
                                            event_type: shared_types::EVENT_CHAT_TOOL_CALL
                                                .to_string(),
                                            payload: serde_json::json!({
                                                "tool_name": tool.tool_name,
                                                "tool_args": tool.tool_args,
                                                "reasoning": tool.reasoning,
                                                "success": tool.result.success,
                                                "output_preview": &tool.result.content.chars().take(200).collect::<String>(),
                                            }),
                                            actor_id: actor_id.clone(),
                                            user_id: user_id.clone(),
                                        })
                                        .await;
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(
                            actor_id = %actor.actor_id,
                            error = %e,
                            "Message processing failed"
                        );
                    }
                }
                result
            }),
        )
    }
}

impl Handler<SwitchModel> for ChatAgent {
    type Result = Result<(), ChatAgentError>;

    fn handle(&mut self, msg: SwitchModel, _ctx: &mut Context<Self>) -> Self::Result {
        match msg.model.as_str() {
            "ClaudeBedrock" | "GLM47" => {
                tracing::info!(
                    actor_id = %self.actor_id,
                    old_model = %self.current_model,
                    new_model = %msg.model,
                    "Switching model"
                );
                self.current_model = msg.model;
                Ok(())
            }
            _ => Err(ChatAgentError::InvalidModel(format!(
                "Unknown model: {}. Available: ClaudeBedrock, GLM47",
                msg.model
            ))),
        }
    }
}

impl Handler<GetConversationHistory> for ChatAgent {
    type Result = Vec<BamlMessage>;

    fn handle(&mut self, _msg: GetConversationHistory, _ctx: &mut Context<Self>) -> Self::Result {
        self.messages.clone()
    }
}

impl Handler<GetAvailableTools> for ChatAgent {
    type Result = Vec<String>;

    fn handle(&mut self, _msg: GetAvailableTools, _ctx: &mut Context<Self>) -> Self::Result {
        self.tool_registry.available_tools()
    }
}

impl Handler<ExecuteTool> for ChatAgent {
    type Result = ResponseActFuture<Self, Result<ToolOutput, ToolError>>;

    fn handle(&mut self, msg: ExecuteTool, _ctx: &mut Context<Self>) -> Self::Result {
        let tool_name = msg.tool_name;
        let tool_args = msg.tool_args;

        Box::pin(async move { execute_tool_impl(&tool_name, &tool_args).await }.into_actor(self))
    }
}

// Helper function to execute a tool outside of the actor context
async fn execute_tool_impl(tool_name: &str, tool_args: &str) -> Result<ToolOutput, ToolError> {
    let registry = ToolRegistry::new();

    let args: serde_json::Value = serde_json::from_str(tool_args)
        .map_err(|e| ToolError::new(format!("Invalid tool arguments: {e}")))?;

    registry.execute(tool_name, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Actor;

    #[actix::test]
    async fn test_chat_agent_creation() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let agent = ChatAgent::new("agent-1".to_string(), "user-1".to_string(), event_store);
        let addr = agent.start();

        // Test getting available tools
        let tools = addr.send(GetAvailableTools).await.unwrap();
        assert!(!tools.is_empty());
        assert!(tools.contains(&"bash".to_string()));
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));
    }

    #[actix::test]
    async fn test_model_switching() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let agent = ChatAgent::new("agent-1".to_string(), "user-1".to_string(), event_store);
        let addr = agent.start();

        // Test valid model switch
        let result = addr
            .send(SwitchModel {
                model: "GLM47".to_string(),
            })
            .await;
        assert!(result.is_ok());

        // Test invalid model
        let result = addr
            .send(SwitchModel {
                model: "InvalidModel".to_string(),
            })
            .await;
        // Mailbox delivery succeeds (outer Ok), but handler returns error (inner Err)
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
}
