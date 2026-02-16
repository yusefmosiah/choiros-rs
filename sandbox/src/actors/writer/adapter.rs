//! WriterDelegationAdapter - AgentHarness bridge for writer-side delegation.
//!
//! This adapter enables WriterActor to use the unified harness loop for
//! delegation planning and execution through the existing `message_writer`
//! tool contract.

use async_trait::async_trait;
use ractor::ActorRef;

use crate::actors::agent_harness::{
    AgentProgress, ExecutionContext, HarnessError, ToolExecution, WorkerPort, WorkerTurnReport,
};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::writer::{WriterDelegateCapability, WriterMsg};
use crate::baml_client::types::{
    MessageWriterToolCall,
    Union7BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall,
};

fn tool_call_name(tool_call: &AgentToolCall) -> &str {
    match tool_call {
        AgentToolCall::BashToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::WebSearchToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FetchUrlToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileReadToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileWriteToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileEditToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::MessageWriterToolCall(call) => call.tool_name.as_str(),
    }
}

pub(crate) struct WriterDelegationAdapter {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
    writer_actor: ActorRef<WriterMsg>,
    researcher_available: bool,
    terminal_available: bool,
}

impl WriterDelegationAdapter {
    pub(crate) fn new(
        writer_id: String,
        user_id: String,
        event_store: ActorRef<EventStoreMsg>,
        writer_actor: ActorRef<WriterMsg>,
        researcher_available: bool,
        terminal_available: bool,
    ) -> Self {
        Self {
            writer_id,
            user_id,
            event_store,
            writer_actor,
            researcher_available,
            terminal_available,
        }
    }

    fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.writer_id.clone(),
            user_id: self.user_id.clone(),
        };
        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    fn parse_max_steps(mode_arg: Option<&str>) -> Option<u8> {
        mode_arg
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| raw.parse::<u8>().ok())
            .map(|value| value.clamp(1, 100))
    }

    async fn execute_message_writer(
        &self,
        ctx: &ExecutionContext,
        call: &MessageWriterToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start = tokio::time::Instant::now();
        let mode = call.tool_args.mode.trim().to_ascii_lowercase();
        let objective = call.tool_args.content.trim().to_string();
        let requested_steps = Self::parse_max_steps(call.tool_args.mode_arg.as_deref());

        let (capability, availability) = match mode.as_str() {
            "delegate_researcher" | "researcher" => (
                WriterDelegateCapability::Researcher,
                self.researcher_available,
            ),
            "delegate_terminal" | "terminal" => {
                (WriterDelegateCapability::Terminal, self.terminal_available)
            }
            _ => {
                return Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Unsupported message_writer mode '{}' for writer delegation",
                        call.tool_args.mode
                    )),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        if objective.is_empty() {
            return Ok(ToolExecution {
                tool_name: "message_writer".to_string(),
                success: false,
                output: String::new(),
                error: Some("message_writer delegation requires non-empty content".to_string()),
                execution_time_ms: start.elapsed().as_millis() as u64,
            });
        }
        if !availability {
            let capability_name = match capability {
                WriterDelegateCapability::Researcher => "researcher",
                WriterDelegateCapability::Terminal => "terminal",
            };
            return Ok(ToolExecution {
                tool_name: "message_writer".to_string(),
                success: false,
                output: String::new(),
                error: Some(format!(
                    "{capability_name} actor unavailable for delegation"
                )),
                execution_time_ms: start.elapsed().as_millis() as u64,
            });
        }

        self.emit_event(
            "writer.delegation.tool_call",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "mode": call.tool_args.mode,
                "requested_steps": requested_steps,
                "objective": objective,
            }),
        );

        let delegate_result = ractor::call!(self.writer_actor, |reply| WriterMsg::DelegateTask {
            capability,
            objective: objective.clone(),
            timeout_ms: Some(180_000),
            max_steps: requested_steps,
            run_id: ctx.run_id.clone(),
            call_id: ctx.call_id.clone(),
            reply,
        })
        .map_err(|e| HarnessError::Adapter(format!("Writer DelegateTask RPC failed: {e}")))?
        .map_err(|e| HarnessError::Adapter(format!("Writer DelegateTask failed: {e}")))?;

        let capability_name = match delegate_result.capability {
            WriterDelegateCapability::Researcher => "researcher",
            WriterDelegateCapability::Terminal => "terminal",
        };
        let output = serde_json::json!({
            "capability": capability_name,
            "success": delegate_result.success,
            "summary": delegate_result.summary,
            "requested_steps": requested_steps,
        });

        self.emit_event(
            "writer.delegation.tool_result",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "capability": capability_name,
                "success": delegate_result.success,
                "summary": delegate_result.summary,
            }),
        );

        Ok(ToolExecution {
            tool_name: "message_writer".to_string(),
            success: delegate_result.success,
            output: output.to_string(),
            error: if delegate_result.success {
                None
            } else {
                Some(delegate_result.summary)
            },
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[async_trait]
impl WorkerPort for WriterDelegationAdapter {
    fn get_model_role(&self) -> &str {
        "writer"
    }

    fn get_tool_description(&self) -> String {
        r#"Tool: message_writer
Description: Dispatch a delegated worker task through WriterActor.
Required args:
- mode: "delegate_researcher" | "delegate_terminal"
- content: delegated objective
Optional args:
- mode_arg: max steps for delegated worker (1-100)
Important:
- Use one or multiple delegation calls when useful.
- Combine delegate_terminal + delegate_researcher when the objective needs both local codebase evidence and external/web evidence.
- If delegation is unnecessary, return no tool calls and complete in message."#
            .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
             "You are WriterActor delegation planner/executor.\n\
             Decide whether this user prompt needs worker delegation before writer revision.\n\
             - Use delegate_researcher for fact-finding, links, verification, or web research.\n\
             - Use delegate_terminal for repository inspection, architecture analysis, docs/codebase research, shell commands, or local execution.\n\
             - When objective spans both local codebase understanding and external evidence, call both delegate_terminal and delegate_researcher.\n\
             - If the prompt is editorial only, return no tool calls.\n\
             - Keep delegated objectives concise and actionable.\n\
             - Do not rewrite the document here; only decide delegation.\n\
             \n\
             Run ID: {:?}\n\
             Delegation Call ID: {:?}\n\
             Writer ID: {}",
            ctx.run_id, ctx.call_id, self.writer_id
        )
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        match tool_call {
            AgentToolCall::MessageWriterToolCall(call) => {
                self.execute_message_writer(ctx, call).await
            }
            _ => Ok(ToolExecution {
                tool_name: tool_call_name(tool_call).to_string(),
                success: false,
                output: String::new(),
                error: Some("Unsupported tool for writer delegation adapter".to_string()),
                execution_time_ms: 0,
            }),
        }
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        self.emit_event(
            "writer.delegation.worker_report",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "report": report,
            }),
        );
        Ok(())
    }

    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        self.emit_event(
            "writer.delegation.progress",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "phase": progress.phase,
                "message": progress.message,
                "step_index": progress.step_index,
                "step_total": progress.step_total,
                "model_used": progress.model_used,
            }),
        );
        Ok(())
    }
}

pub(crate) struct WriterSynthesisAdapter {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
}

impl WriterSynthesisAdapter {
    pub(crate) fn new(
        writer_id: String,
        user_id: String,
        event_store: ActorRef<EventStoreMsg>,
    ) -> Self {
        Self {
            writer_id,
            user_id,
            event_store,
        }
    }

    fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.writer_id.clone(),
            user_id: self.user_id.clone(),
        };
        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }
}

#[async_trait]
impl WorkerPort for WriterSynthesisAdapter {
    fn get_model_role(&self) -> &str {
        "writer"
    }

    fn get_tool_description(&self) -> String {
        "No tools are available in this synthesis step. Return final markdown in message with an empty tool_calls array."
            .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            "You are WriterActor synthesis mode.\n\
             Produce the revised markdown directly in message.\n\
             Do not call tools.\n\
             \n\
             Run ID: {:?}\n\
             Call ID: {:?}\n\
             Writer ID: {}",
            ctx.run_id, ctx.call_id, self.writer_id
        )
    }

    async fn execute_tool_call(
        &self,
        _ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        Ok(ToolExecution {
            tool_name: tool_call_name(tool_call).to_string(),
            success: false,
            output: String::new(),
            error: Some("No tools available for writer synthesis".to_string()),
            execution_time_ms: 0,
        })
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        self.emit_event(
            "writer.synthesis.worker_report",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "report": report,
            }),
        );
        Ok(())
    }

    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        self.emit_event(
            "writer.synthesis.progress",
            serde_json::json!({
                "loop_id": ctx.loop_id,
                "run_id": ctx.run_id,
                "call_id": ctx.call_id,
                "phase": progress.phase,
                "message": progress.message,
                "step_index": progress.step_index,
                "step_total": progress.step_total,
                "model_used": progress.model_used,
            }),
        );
        Ok(())
    }
}
