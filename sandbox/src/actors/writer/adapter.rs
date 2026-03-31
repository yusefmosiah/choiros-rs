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
use crate::actors::writer::{
    dispatch_delegate_capability, VersionSource, WriterDelegateCapability, WriterMsg,
};
use crate::baml_client::types::{
    MessageWriterToolCall,
    Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall,
};
use crate::supervisor::researcher::ResearcherSupervisorMsg;
use crate::supervisor::terminal::TerminalSupervisorMsg;

fn tool_call_name(tool_call: &AgentToolCall) -> &str {
    match tool_call {
        AgentToolCall::BashToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::WebSearchToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FetchUrlToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileReadToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileWriteToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FileEditToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::MessageWriterToolCall(call) => call.tool_name.as_str(),
        AgentToolCall::FinishedToolCall(call) => call.tool_name.as_str(),
    }
}

pub(crate) struct WriterDelegationAdapter {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
    writer_actor: ActorRef<WriterMsg>,
    researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
    run_id: Option<String>,
    parent_version_id: Option<u64>,
}

impl WriterDelegationAdapter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        writer_id: String,
        user_id: String,
        event_store: ActorRef<EventStoreMsg>,
        writer_actor: ActorRef<WriterMsg>,
        researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
        terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
        run_id: Option<String>,
        parent_version_id: Option<u64>,
    ) -> Self {
        Self {
            writer_id,
            user_id,
            event_store,
            writer_actor,
            researcher_supervisor,
            terminal_supervisor,
            run_id,
            parent_version_id,
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

        // Handle write_revision mode — compose document content directly.
        if mode == "write_revision" || mode == "revision" {
            let content = objective; // reuse the parsed content field
            if content.is_empty() {
                return Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(
                        "write_revision requires non-empty content (the revised document)"
                            .to_string(),
                    ),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                });
            }

            let (Some(run_id), Some(parent_version_id)) =
                (self.run_id.as_ref(), self.parent_version_id)
            else {
                return Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(
                        "write_revision unavailable: no run context (run_id/parent_version_id)"
                            .to_string(),
                    ),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                });
            };

            self.emit_event(
                "writer.delegation.write_revision",
                serde_json::json!({
                    "run_id": run_id,
                    "call_id": ctx.call_id,
                    "parent_version_id": parent_version_id,
                    "content_len": content.len(),
                }),
            );

            // Fire-and-forget: the WriterActor is blocked awaiting this harness, so
            // ractor::call! would deadlock. Queue the message and return success now;
            // the actor processes CreateWriterDocumentVersion after the harness finishes.
            let (tx, rx) = tokio::sync::oneshot::channel::<
                Result<crate::actors::writer::DocumentVersion, crate::actors::writer::WriterError>,
            >();
            let send_result =
                self.writer_actor
                    .send_message(WriterMsg::CreateWriterDocumentVersion {
                        run_id: run_id.clone(),
                        parent_version_id: Some(parent_version_id),
                        content,
                        source: VersionSource::Writer,
                        reply: ractor::RpcReplyPort::from(tx),
                    });
            drop(rx); // we don't await the reply

            return match send_result {
                Ok(()) => Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: true,
                    output: serde_json::json!({
                        "mode": "write_revision",
                        "status": "revision_queued",
                        "next_step": "Revision queued. Call finished now.",
                    })
                    .to_string(),
                    error: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                }),
                Err(e) => Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(format!("Writer actor send failed: {e}")),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                }),
            };
        }

        let capability = match mode.as_str() {
            "delegate_researcher" | "researcher" => WriterDelegateCapability::Researcher,
            "delegate_terminal" | "terminal" => WriterDelegateCapability::Terminal,
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
        let availability = match capability {
            WriterDelegateCapability::Researcher => self.researcher_supervisor.is_some(),
            WriterDelegateCapability::Terminal => self.terminal_supervisor.is_some(),
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

        let delegate_result = dispatch_delegate_capability(
            &self.writer_id,
            &self.user_id,
            &self.writer_actor,
            self.researcher_supervisor.clone(),
            self.terminal_supervisor.clone(),
            capability,
            objective.clone(),
            Some(180_000),
            requested_steps,
            ctx.run_id.clone(),
            ctx.call_id.clone(),
        )
        .map_err(|e| HarnessError::Adapter(format!("Writer delegation dispatch failed: {e}")))?;

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
Description: Dispatch a delegated worker task or write document content through WriterActor.
Required args:
- mode: "write_revision" | "delegate_researcher" | "delegate_terminal"
- content: revised document (for write_revision) or delegated objective (for delegation)
Optional args:
- mode_arg: max steps for delegated worker (1-100)
Important:
- For editorial tasks, compose the content using write_revision, then call finished.
- Use delegation calls when the objective needs external research or local execution.
- Combine delegate_terminal + delegate_researcher when both local and external evidence are needed."#
            .to_string()
    }

    fn allowed_tool_names(&self) -> Option<&'static [&'static str]> {
        Some(&["message_writer", "finished"])
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
             "You are WriterActor — the single authority over document content.\n\
             Decide whether this user prompt needs worker delegation or direct writing.\n\
             - Use delegate_researcher for fact-finding, links, verification, or web research.\n\
             - Use delegate_terminal for repository inspection, architecture analysis, docs/codebase research, shell commands, or local execution.\n\
             - When objective spans both local codebase understanding and external evidence, call both delegate_terminal and delegate_researcher.\n\
             - If the prompt is editorial only (no research or execution needed), compose the content using write_revision with mode=\"write_revision\", then call finished.\n\
             - Keep delegated objectives concise and actionable.\n\
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

// ---------------------------------------------------------------------------
// WriterUserPromptAdapter — handles user prompt diffs through the LLM.
//
// Unlike WriterDelegationAdapter (delegation-only), this adapter also supports
// a `write_revision` mode that lets the LLM write its revised content directly
// back to the document as a new Writer version.
// ---------------------------------------------------------------------------

pub(crate) struct WriterUserPromptAdapter {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
    writer_actor: ActorRef<WriterMsg>,
    researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
    run_id: String,
    parent_version_id: u64,
}

impl WriterUserPromptAdapter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        writer_id: String,
        user_id: String,
        event_store: ActorRef<EventStoreMsg>,
        writer_actor: ActorRef<WriterMsg>,
        researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
        terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
        run_id: String,
        parent_version_id: u64,
    ) -> Self {
        Self {
            writer_id,
            user_id,
            event_store,
            writer_actor,
            researcher_supervisor,
            terminal_supervisor,
            run_id,
            parent_version_id,
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

    async fn execute_message_writer(
        &self,
        ctx: &ExecutionContext,
        call: &MessageWriterToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start = tokio::time::Instant::now();
        let mode = call.tool_args.mode.trim().to_ascii_lowercase();
        let content = call.tool_args.content.trim().to_string();

        match mode.as_str() {
            // Write the revised document content as a new Writer version.
            "write_revision" | "revision" => {
                if content.is_empty() {
                    return Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(
                            "write_revision requires non-empty content (the revised document)"
                                .to_string(),
                        ),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    });
                }

                self.emit_event(
                    "writer.user_prompt.write_revision",
                    serde_json::json!({
                        "run_id": &self.run_id,
                        "call_id": ctx.call_id,
                        "parent_version_id": self.parent_version_id,
                        "content_len": content.len(),
                    }),
                );

                let result: Result<_, ractor::RactorErr<WriterMsg>> =
                    ractor::call!(self.writer_actor, |reply| {
                        WriterMsg::CreateWriterDocumentVersion {
                            run_id: self.run_id.clone(),
                            parent_version_id: Some(self.parent_version_id),
                            content,
                            source: VersionSource::Writer,
                            reply,
                        }
                    });

                match result {
                    Ok(Ok(version)) => Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: true,
                        output: serde_json::json!({
                            "mode": "write_revision",
                            "version_id": version.version_id,
                            "status": "revision_applied",
                            "next_step": "If this revision satisfies the objective and you have no pending delegations, call finished now. Do not emit another write_revision unless new worker results require a materially different document.",
                        })
                        .to_string(),
                        error: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                    Ok(Err(e)) => Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to create revision: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                    Err(e) => Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Writer actor call failed: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }

            // Delegate to researcher or terminal.
            "delegate_researcher" | "researcher" | "delegate_terminal" | "terminal" => {
                let capability = match mode.as_str() {
                    "delegate_researcher" | "researcher" => WriterDelegateCapability::Researcher,
                    _ => WriterDelegateCapability::Terminal,
                };
                let availability = match capability {
                    WriterDelegateCapability::Researcher => self.researcher_supervisor.is_some(),
                    WriterDelegateCapability::Terminal => self.terminal_supervisor.is_some(),
                };

                if content.is_empty() {
                    return Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(
                            "delegation requires non-empty content (objective)".to_string(),
                        ),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    });
                }
                if !availability {
                    return Ok(ToolExecution {
                        tool_name: "message_writer".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("{mode} actor unavailable for delegation")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    });
                }

                let requested_steps = call
                    .tool_args
                    .mode_arg
                    .as_deref()
                    .map(str::trim)
                    .filter(|raw| !raw.is_empty())
                    .and_then(|raw| raw.parse::<u8>().ok())
                    .map(|v| v.clamp(1, 100));

                let delegate_result = dispatch_delegate_capability(
                    &self.writer_id,
                    &self.user_id,
                    &self.writer_actor,
                    self.researcher_supervisor.clone(),
                    self.terminal_supervisor.clone(),
                    capability,
                    content.clone(),
                    Some(180_000),
                    requested_steps,
                    Some(self.run_id.clone()),
                    ctx.call_id.clone(),
                )
                .map_err(|e| {
                    HarnessError::Adapter(format!("Writer delegation dispatch failed: {e}"))
                })?;

                let capability_name = match delegate_result.capability {
                    WriterDelegateCapability::Researcher => "researcher",
                    WriterDelegateCapability::Terminal => "terminal",
                };

                Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: delegate_result.success,
                    output: serde_json::json!({
                        "capability": capability_name,
                        "success": delegate_result.success,
                        "summary": delegate_result.summary,
                    })
                    .to_string(),
                    error: if delegate_result.success {
                        None
                    } else {
                        Some(delegate_result.summary)
                    },
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
            }

            _ => Ok(ToolExecution {
                tool_name: "message_writer".to_string(),
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Unsupported message_writer mode '{}'. Use: write_revision, \
                     delegate_researcher, delegate_terminal",
                    call.tool_args.mode
                )),
                execution_time_ms: start.elapsed().as_millis() as u64,
            }),
        }
    }
}

#[async_trait]
impl WorkerPort for WriterUserPromptAdapter {
    fn get_model_role(&self) -> &str {
        "writer"
    }

    fn get_tool_description(&self) -> String {
        r#"Tool: message_writer
Description: Writer document revision and worker delegation.
Required args:
- mode: "write_revision" | "delegate_researcher" | "delegate_terminal"
- content: for write_revision this is the full revised document content; for delegation this is the objective
Optional args:
- mode_arg: max steps for delegated worker (1-100)
Instructions:
- Use "write_revision" to produce the final revised document. The content arg must be the COMPLETE revised document.
- Use "delegate_researcher" when you need external research, web search, or fact verification before revising.
- Use "delegate_terminal" when you need to inspect the local codebase, run commands, or check files before revising.
- You may delegate first, then call write_revision with the final content.
- If the user's changes are purely editorial (typo fixes, reformatting, direct content changes), apply them directly via write_revision.
- Always call write_revision before calling `finished` — the revision is the primary output.
- After write_revision succeeds, call `finished` immediately unless you still need unresolved worker delegation.
- Do not use markdown footnote syntax like [^s1] or [1] unless the document already contains a resolved citation system. Prefer inline source mentions or plain prose; source URLs are rendered separately in the UI."#
            .to_string()
    }

    fn allowed_tool_names(&self) -> Option<&'static [&'static str]> {
        Some(&["message_writer", "finished"])
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            "You are the ChoirOS Writer. You receive a document and the user's edits (as a diff), \
             and you produce a revised document.\n\
             \n\
             Your job:\n\
             1. Read the user's diff carefully to understand their intent.\n\
             2. If the changes are editorial (typos, rephrasing, adding/removing content), \
                produce the revision directly via write_revision.\n\
             3. If the changes include instructions that require research or code inspection, \
                delegate to workers first, then revise.\n\
             4. The write_revision content must be the COMPLETE revised document, not a partial diff.\n\
             5. Always call write_revision with your final output before calling finished.\n\
             6. After write_revision succeeds, call finished immediately unless unresolved \
                worker delegation still needs to change the document.\n\
             7. Do not invent markdown footnote markers like [^s1] or [1] unless the \
                document already contains a working citation system. Prefer inline source \
                mentions or plain prose; the UI renders source URLs separately.\n\
             \n\
             Run ID: {:?}\n\
             Call ID: {:?}\n\
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
                error: Some("Unsupported tool for writer user prompt adapter".to_string()),
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
            "writer.user_prompt.worker_report",
            serde_json::json!({
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
            "writer.user_prompt.progress",
            serde_json::json!({
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
