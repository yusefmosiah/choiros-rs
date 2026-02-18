//! TerminalActor - manages terminal sessions for opencode integration
//!
//! PREDICTION: Terminal sessions can be managed as actors with PTY support,
//! enabling opencode (and other tools) to spawn and interact with terminals
//! through a unified API.
//!
//! EXPERIMENT:
//! 1. TerminalActor spawns PTY processes (bash, zsh, etc.)
//! 2. Input/output streamed via WebSocket or actor messages
//! 3. Sessions persist and can be reattached
//! 4. Multiple terminals per user, managed by DesktopActor windows
//!
//! OBSERVE:
//! - PTY processes survive actor restarts (via persistence)
//! - Output streaming works for long-running commands
//! - Session reattachment works across reconnections
//! - Integration with opencode CLI

use async_trait::async_trait;
use portable_pty::{ChildKiller, CommandBuilder, PtySize};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::io::{Read, Write};
use tokio::sync::{broadcast, mpsc};

use crate::actors::agent_harness::{
    AgentHarness, AgentProgress, ExecutionContext, HarnessConfig, HarnessError, ToolExecution,
    WorkerPort,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::model_config::ModelRegistry;
use crate::actors::writer::{
    SectionState, WriterDelegateCapability, WriterDelegateResult, WriterError,
    WriterInboundEnvelope, WriterMsg, WriterSource,
};
use crate::baml_client::types::{
    MessageWriterToolCall,
    Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall,
};
use crate::observability::llm_trace::LlmTraceEmitter;

use shared_types::{
    WorkerEscalation, WorkerEscalationKind, WorkerEscalationUrgency, WorkerTurnReport,
    WorkerTurnStatus,
};

// ============================================================================
// TerminalAdapter - AgentHarness implementation for Terminal
// ============================================================================

/// Adapter for running terminal commands through the unified agent harness
pub struct TerminalAdapter {
    terminal_id: String,
    working_dir: String,
    shell: String,
    event_store: Option<ActorRef<EventStoreMsg>>,
    progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
    writer_actor: Option<ActorRef<WriterMsg>>,
    run_id: Option<String>,
}

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

impl TerminalAdapter {
    pub fn new(
        terminal_id: String,
        working_dir: String,
        shell: String,
        event_store: Option<ActorRef<EventStoreMsg>>,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
        writer_actor: Option<ActorRef<WriterMsg>>,
        run_id: Option<String>,
    ) -> Self {
        Self {
            terminal_id,
            working_dir,
            shell,
            event_store,
            progress_tx,
            writer_actor,
            run_id,
        }
    }

    fn has_writer_document_context(&self) -> bool {
        self.writer_actor.is_some() && self.run_id.is_some()
    }

    fn writer_context(&self) -> Option<(ActorRef<WriterMsg>, String)> {
        Some((self.writer_actor.clone()?, self.run_id.clone()?))
    }

    fn resolve_writer_section(section_hint: Option<&str>) -> String {
        match section_hint
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .as_deref()
        {
            Some("conductor") => "conductor".to_string(),
            Some("researcher") => "researcher".to_string(),
            Some("user") => "user".to_string(),
            _ => "terminal".to_string(),
        }
    }

    fn parse_section_state(raw: Option<&str>) -> Option<SectionState> {
        match raw.map(|s| s.trim().to_ascii_lowercase())?.as_str() {
            "pending" => Some(SectionState::Pending),
            "running" => Some(SectionState::Running),
            "complete" | "completed" => Some(SectionState::Complete),
            "failed" => Some(SectionState::Failed),
            _ => None,
        }
    }

    fn has_successful_message_writer_call(tool_executions: &[ToolExecution]) -> bool {
        tool_executions
            .iter()
            .any(|exec| exec.tool_name == "message_writer" && exec.success)
    }

    async fn execute_message_writer(
        &self,
        tool_call: &MessageWriterToolCall,
    ) -> Result<ToolExecution, crate::actors::agent_harness::HarnessError> {
        let start_time = tokio::time::Instant::now();
        let (writer_actor, run_id) = match self.writer_context() {
            Some(ctx) => ctx,
            None => {
                return Ok(ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some("WriterActor/run context not configured for this run".to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                });
            }
        };

        let args = &tool_call.tool_args;
        let section_id = Self::resolve_writer_section(args.path.as_deref());
        let content = args.content.clone();
        let mode = args.mode.trim().to_ascii_lowercase();
        let mode_arg = args.mode_arg.clone();

        let result = match mode.as_str() {
            "progress" => {
                if content.trim().is_empty() {
                    Err("message_writer progress mode requires content".to_string())
                } else {
                    let phase = mode_arg
                        .clone()
                        .unwrap_or_else(|| "update".to_string())
                        .trim()
                        .to_string();
                    ractor::call!(writer_actor, |reply| WriterMsg::ReportProgress {
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Terminal,
                        phase,
                        message: content.clone(),
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|revision| {
                        serde_json::json!({
                            "mode": "progress",
                            "section_id": section_id,
                            "revision": revision,
                        })
                    })
                }
            }
            "state" => {
                let state = Self::parse_section_state(mode_arg.as_deref()).ok_or_else(|| {
                    "message_writer state mode requires mode_arg in {pending|running|complete|failed}"
                        .to_string()
                });
                match state {
                    Ok(state) => ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        state,
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|_| {
                        serde_json::json!({
                            "mode": "state",
                            "section_id": section_id,
                        })
                    }),
                    Err(error) => Err(error),
                }
            }
            "canon_append" => {
                if content.trim().is_empty() {
                    Err("message_writer canon_append mode requires content".to_string())
                } else {
                    ractor::call!(writer_actor, |reply| WriterMsg::ApplyText {
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Terminal,
                        content: content.clone(),
                        proposal: false,
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|revision| {
                        serde_json::json!({
                            "mode": "canon_append",
                            "section_id": section_id,
                            "revision": revision,
                        })
                    })
                }
            }
            "proposal_append" => {
                if content.trim().is_empty() {
                    Err("message_writer proposal_append mode requires content".to_string())
                } else {
                    let message_id = format!("{run_id}:terminal:tool:{}", ulid::Ulid::new());
                    let envelope = WriterInboundEnvelope {
                        message_id,
                        correlation_id: format!("{run_id}:{}", ulid::Ulid::new()),
                        kind: "terminal_tool_update".to_string(),
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Terminal,
                        content: content.clone(),
                        base_version_id: None,
                        prompt_diff: None,
                        overlay_id: None,
                        session_id: None,
                        thread_id: None,
                        call_id: None,
                        origin_actor: Some(self.terminal_id.clone()),
                    };
                    ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
                        envelope,
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|ack| {
                        serde_json::json!({
                            "mode": "proposal_append",
                            "section_id": section_id,
                            "message_id": ack.message_id,
                            "revision": ack.revision,
                            "queue_len": ack.queue_len,
                            "duplicate": ack.duplicate,
                        })
                    })
                }
            }
            "completion" => {
                if content.trim().is_empty() {
                    Err("message_writer completion mode requires content".to_string())
                } else {
                    let message_id =
                        format!("{run_id}:terminal:tool:completion:{}", ulid::Ulid::new());
                    let envelope = WriterInboundEnvelope {
                        message_id,
                        correlation_id: format!("{run_id}:{}", ulid::Ulid::new()),
                        kind: "terminal_tool_completion".to_string(),
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Terminal,
                        content: content.clone(),
                        base_version_id: None,
                        prompt_diff: None,
                        overlay_id: None,
                        session_id: None,
                        thread_id: None,
                        call_id: None,
                        origin_actor: Some(self.terminal_id.clone()),
                    };
                    ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
                        envelope,
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|ack| {
                        serde_json::json!({
                            "mode": "completion",
                            "section_id": section_id,
                            "message_id": ack.message_id,
                            "revision": ack.revision,
                            "queue_len": ack.queue_len,
                            "duplicate": ack.duplicate,
                        })
                    })
                }
            }
            _ => Err(format!(
                "Unknown message_writer mode '{}'. Supported: proposal_append, canon_append, progress, state, completion",
                mode
            )),
        };

        let elapsed = start_time.elapsed().as_millis() as u64;
        match result {
            Ok(output) => Ok(ToolExecution {
                tool_name: "message_writer".to_string(),
                success: true,
                output: output.to_string(),
                error: None,
                execution_time_ms: elapsed,
            }),
            Err(error) => Ok(ToolExecution {
                tool_name: "message_writer".to_string(),
                success: false,
                output: String::new(),
                error: Some(error),
                execution_time_ms: elapsed,
            }),
        }
    }

    /// Execute a bash command and return the result
    async fn execute_bash(
        &self,
        command: &str,
        timeout_ms: u64,
    ) -> Result<(String, i32), TerminalError> {
        Self::validate_command_policy(command)?;
        let command = Self::normalize_command_for_runtime(command, timeout_ms);

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::process::Command::new(&self.shell)
                .arg("-lc")
                .arg(&command)
                .current_dir(&self.working_dir)
                .output(),
        )
        .await
        .map_err(|_| TerminalError::Timeout(timeout_ms))?
        .map_err(|e| TerminalError::Io(format!("Failed to execute terminal command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut combined = String::new();
        if !stdout.trim().is_empty() {
            combined.push_str(stdout.trim_end());
        }
        if !stderr.trim().is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(stderr.trim_end());
        }

        Ok((combined, output.status.code().unwrap_or(1)))
    }

    fn validate_command_policy(command: &str) -> Result<(), TerminalError> {
        let allowed_prefixes = std::env::var("CHOIR_TERMINAL_ALLOWED_COMMAND_PREFIXES")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if allowed_prefixes.is_empty() {
            return Ok(());
        }

        let normalized = command.trim_start();
        if allowed_prefixes
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
        {
            return Ok(());
        }

        Err(TerminalError::InvalidInput(format!(
            "Command denied by terminal policy. Set CHOIR_TERMINAL_ALLOWED_COMMAND_PREFIXES to include one of: {}",
            allowed_prefixes.join(", ")
        )))
    }

    fn normalize_command_for_runtime(command: &str, timeout_ms: u64) -> String {
        let trimmed = command.trim();
        let Some(rest) = trimmed.strip_prefix("curl") else {
            return command.to_string();
        };
        let rest = rest.trim_start();
        if rest.is_empty() {
            return command.to_string();
        }

        let has_connect_timeout = trimmed.contains("--connect-timeout");
        let has_max_time = trimmed.contains("--max-time");
        let has_follow_redirects = trimmed.contains(" -L") || trimmed.starts_with("curl -L");

        let max_time_secs = (timeout_ms / 1000).saturating_sub(2).max(5);
        let mut injected_opts = Vec::new();
        if !has_follow_redirects {
            injected_opts.push("-L".to_string());
        }
        if !has_connect_timeout {
            injected_opts.push("--connect-timeout 8".to_string());
        }
        if !has_max_time {
            injected_opts.push(format!("--max-time {max_time_secs}"));
        }
        if injected_opts.is_empty() {
            return command.to_string();
        }

        format!("curl {} {}", injected_opts.join(" "), rest)
    }

    fn emit_terminal_progress(
        &self,
        phase: &str,
        message: &str,
        reasoning: Option<String>,
        command: Option<String>,
        model_used: Option<String>,
        output_excerpt: Option<String>,
        exit_code: Option<i32>,
        step_index: Option<usize>,
        step_total: Option<usize>,
    ) {
        let Some(tx) = &self.progress_tx else {
            return;
        };
        let _ = tx.send(TerminalAgentProgress {
            phase: phase.to_string(),
            message: message.to_string(),
            reasoning,
            command,
            model_used,
            output_excerpt,
            exit_code,
            step_index,
            step_total,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }
}

#[async_trait]
impl WorkerPort for TerminalAdapter {
    fn get_model_role(&self) -> &str {
        "terminal"
    }

    fn get_tool_description(&self) -> String {
        r#"Tools:

Tool: bash
Description: Execute shell commands in the current terminal.
Parameters Schema: {"type":"object","properties":{"command":{"type":"string","description":"The shell command to execute"},"timeout_ms":{"type":"integer","description":"Timeout in milliseconds"}},"required":["command"]}

Tool: message_writer
Description: Send typed actor messages to writer run document.
Args:
- mode: proposal_append|canon_append|progress|state|completion
- content: required for proposal_append|canon_append|progress|completion
- path: optional section_id override (default terminal)
- mode_arg: required for state (pending|running|complete|failed), optional for progress phase

Writer mode contract:
- If objective resolves in one step: send one message_writer mode=completion with final content.
- If objective is multi-step: send proposal_append deltas, then one completion message.
- After completion message is sent, call `finished` and return final response in message."#
            .to_string()
    }

    fn get_system_context(&self, _ctx: &ExecutionContext) -> String {
        let writer_mode_hint = if self.has_writer_document_context() {
            "- Run writer mode is active: use message_writer for incremental updates and final completion.\n\
             - Before final empty-tool response, send message_writer mode=completion."
        } else {
            ""
        };
        format!(
            "You are ChoirOS Terminal Agent. Use bash tool calls to complete terminal objectives.\n\
             Capability boundary:\n\
             - Use terminal for local shell/file/system execution.\n\
             - Do NOT perform general web research, news scraping, or search-engine browsing.\n\
             - If objective requires external research/sources, stop tool calling and return a final message indicating researcher capability is required.\n\
             - Treat codebase research as first-class terminal work: inspect repository code, docs, architecture, tests, and produce evidence-backed findings.\n\
             - For research-oriented objectives, prefer read/inspect commands and writing findings to docs markdown.\n\
             - Only edit source code when the objective explicitly asks for implementation/refactor/bug-fix changes.\n\
             - If objective is local diagnostics/build/test/file operations, proceed with minimal safe commands.\n\
             {}\n\
             Terminal ID: {}\n\
             Working Directory: {}\n\
             Shell: {}\n\
             Prefer minimal safe command sequences.",
            writer_mode_hint,
            self.terminal_id,
            self.working_dir,
            self.shell
        )
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, crate::actors::agent_harness::HarnessError> {
        match tool_call {
            AgentToolCall::BashToolCall(bash_call) => {
                let command = bash_call.tool_args.command.as_str();
                let timeout_ms = 30_000;
                let start_time = std::time::Instant::now();

                self.emit_terminal_progress(
                    "terminal_tool_call",
                    "terminal agent requested bash tool execution",
                    bash_call.reasoning.clone(),
                    Some(command.to_string()),
                    Some(ctx.model_used.clone()),
                    None,
                    None,
                    Some(ctx.step_number),
                    Some(ctx.max_steps),
                );

                match self.execute_bash(command, timeout_ms).await {
                    Ok((output, exit_code)) => {
                        let _execution_time_ms = start_time.elapsed().as_millis() as u64;
                        let success = exit_code == 0;

                        self.emit_terminal_progress(
                            "terminal_tool_result",
                            "terminal agent received bash tool result",
                            bash_call.reasoning.clone(),
                            Some(command.to_string()),
                            Some(ctx.model_used.clone()),
                            Some(Self::truncate_excerpt(&output)),
                            Some(exit_code),
                            Some(ctx.step_number),
                            Some(ctx.max_steps),
                        );

                        Ok(ToolExecution {
                            tool_name: "bash".to_string(),
                            success,
                            output,
                            error: if success {
                                None
                            } else {
                                Some(format!("Exit status {exit_code}"))
                            },
                            execution_time_ms: 0,
                        })
                    }
                    Err(e) => {
                        let _execution_time_ms = start_time.elapsed().as_millis() as u64;
                        Err(crate::actors::agent_harness::HarnessError::ToolExecution(
                            e.to_string(),
                        ))
                    }
                }
            }
            AgentToolCall::MessageWriterToolCall(call) => self.execute_message_writer(call).await,
            _ => {
                let tool_name = tool_call_name(tool_call).to_string();
                Ok(ToolExecution {
                    tool_name: tool_name.clone(),
                    success: false,
                    output: String::new(),
                    error: Some(format!("Unknown tool: {tool_name}")),
                    execution_time_ms: 0,
                })
            }
        }
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    fn validate_terminal_decision(
        &self,
        _ctx: &ExecutionContext,
        _decision: &crate::baml_client::types::AgentDecision,
        tool_executions: &[ToolExecution],
    ) -> Result<(), String> {
        if !self.has_writer_document_context() {
            return Ok(());
        }
        if !Self::has_successful_message_writer_call(tool_executions) {
            return Err(
                "Run writer mode requires at least one successful message_writer call before final completion.".to_string(),
            );
        }
        Ok(())
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), crate::actors::agent_harness::HarnessError> {
        if let Some(event_store) = &self.event_store {
            let payload = serde_json::json!({
                "task_id": ctx.loop_id,
                "worker_id": ctx.worker_id,
                "report": report,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let event = crate::actors::event_store::AppendEvent {
                event_type: "worker.report.received".to_string(),
                payload,
                actor_id: ctx.worker_id.clone(),
                user_id: ctx.user_id.clone(),
            };
            let _ = event_store
                .send_message(crate::actors::event_store::EventStoreMsg::AppendAsync { event });
        }
        Ok(())
    }

    async fn emit_progress(
        &self,
        _ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), crate::actors::agent_harness::HarnessError> {
        self.emit_terminal_progress(
            &progress.phase,
            &progress.message,
            None,
            None,
            progress.model_used,
            None,
            None,
            progress.step_index,
            progress.step_total,
        );
        Ok(())
    }

    fn build_worker_report(
        &self,
        ctx: &ExecutionContext,
        summary: &str,
        success: bool,
    ) -> WorkerTurnReport {
        // Build escalations for blocked commands
        let mut escalations = Vec::new();
        if !success {
            escalations.push(WorkerEscalation {
                escalation_id: ulid::Ulid::new().to_string(),
                kind: WorkerEscalationKind::Blocker,
                reason: format!("Terminal task failed or blocked: {}", summary),
                urgency: WorkerEscalationUrgency::Medium,
                options: vec![
                    "retry".to_string(),
                    "escalate".to_string(),
                    "abort".to_string(),
                ],
                recommended_option: Some("retry".to_string()),
                requires_human: Some(false),
            });
        }

        WorkerTurnReport {
            turn_id: ctx.loop_id.clone(),
            worker_id: ctx.worker_id.clone(),
            task_id: ctx.loop_id.clone(),
            worker_role: Some(self.get_model_role().to_string()),
            status: if success {
                WorkerTurnStatus::Completed
            } else {
                WorkerTurnStatus::Failed
            },
            summary: Some(summary.to_string()),
            findings: Vec::new(),
            learnings: Vec::new(),
            escalations,
            artifacts: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

impl TerminalAdapter {
    fn truncate_excerpt(text: &str) -> String {
        let max_len = 1200;
        let mut excerpt: String = text.chars().take(max_len).collect();
        if text.chars().count() > max_len {
            excerpt.push_str("...");
        }
        excerpt
    }
}

/// Actor that manages terminal sessions
#[derive(Debug, Default)]
pub struct TerminalActor;

/// Arguments for spawning TerminalActor
#[derive(Debug, Clone)]
pub struct TerminalArguments {
    pub terminal_id: String,
    pub user_id: String,
    pub shell: String, // e.g., "/bin/bash" or "/bin/zsh"
    pub working_dir: String,
    pub event_store: ActorRef<EventStoreMsg>,
}

/// State for TerminalActor
pub struct TerminalState {
    terminal_id: String,
    user_id: String,
    shell: String,
    working_dir: String,
    event_store: ActorRef<EventStoreMsg>,
    /// PTY master handle (for I/O and resize)
    #[allow(dead_code)]
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    /// Handle used to terminate spawned child process on stop
    child_killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    /// Channel for sending input to PTY
    input_tx: Option<mpsc::Sender<String>>,
    /// Broadcast channel for output from PTY
    output_tx: Option<broadcast::Sender<String>>,
    /// Buffer of recent output (for new connections)
    output_buffer: Vec<String>,
    /// Whether terminal is running
    is_running: bool,
    /// Exit code when process ends
    exit_code: Option<i32>,
    /// PID of the spawned shell process (if available)
    process_id: Option<u32>,
    /// Terminal dimensions
    rows: u16,
    cols: u16,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by TerminalActor
#[derive(Debug)]
pub enum TerminalMsg {
    /// Start the terminal session (spawn PTY)
    Start {
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Send input to terminal (keyboard input)
    SendInput {
        input: String,
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Get recent output (for new connections)
    GetOutput { reply: RpcReplyPort<Vec<String>> },
    /// Subscribe to output stream (returns channel receiver)
    SubscribeOutput {
        reply: RpcReplyPort<broadcast::Receiver<String>>,
    },
    /// Resize terminal (rows, cols)
    Resize {
        rows: u16,
        cols: u16,
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Get terminal info
    GetInfo { reply: RpcReplyPort<TerminalInfo> },
    /// Stop/kill terminal
    Stop {
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Execute a high-level natural-language objective over this terminal.
    /// Intended for uactor->actor orchestration contracts.
    RunAgenticTask {
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
        writer_actor: Option<ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
        reply: RpcReplyPort<Result<TerminalAgentResult, TerminalError>>,
    },
    RunAgenticTaskDetached {
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
        writer_actor: Option<ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
    },
    /// Execute one typed bash command for appactor->toolactor delegation.
    RunBashTool {
        request: TerminalBashToolRequest,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
        reply: RpcReplyPort<Result<TerminalAgentResult, TerminalError>>,
    },
    /// Internal: output received from PTY
    OutputReceived { data: String },
    /// Internal: process exited
    ProcessExited { exit_code: Option<i32> },
}

/// Terminal information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalInfo {
    pub terminal_id: String,
    pub user_id: String,
    pub shell: String,
    pub working_dir: String,
    pub is_running: bool,
    pub exit_code: Option<i32>,
    pub process_id: Option<u32>,
    pub rows: u16,
    pub cols: u16,
}

/// Result from agentic terminal execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalAgentResult {
    pub summary: String,
    pub reasoning: Option<String>,
    pub success: bool,
    pub model_used: Option<String>,
    pub exit_code: Option<i32>,
    pub executed_commands: Vec<String>,
    pub steps: Vec<TerminalExecutionStep>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalExecutionStep {
    pub command: String,
    pub exit_code: i32,
    pub output_excerpt: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalAgentProgress {
    pub phase: String,
    pub message: String,
    pub reasoning: Option<String>,
    pub command: Option<String>,
    pub model_used: Option<String>,
    pub output_excerpt: Option<String>,
    pub exit_code: Option<i32>,
    pub step_index: Option<usize>,
    pub step_total: Option<usize>,
    pub timestamp: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalBashToolRequest {
    pub cmd: String,
    pub timeout_ms: Option<u64>,
    pub model_override: Option<String>,
    pub reasoning: Option<String>,
    pub run_id: Option<String>,
    pub call_id: Option<String>,
}

#[derive(Clone)]
struct TerminalExecutionContext {
    terminal_id: String,
    user_id: String,
    working_dir: String,
    shell: String,
    event_store: ActorRef<EventStoreMsg>,
    writer_actor: Option<ActorRef<WriterMsg>>,
    run_id: Option<String>,
    call_id: Option<String>,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error, Clone)]
pub enum TerminalError {
    #[error("Terminal not running")]
    NotRunning,

    #[error("Terminal already running")]
    AlreadyRunning,

    #[error("Failed to spawn PTY: {0}")]
    SpawnFailed(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Terminal command timed out after {0}ms")]
    Timeout(u64),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Blocked: {0}")]
    Blocked(String),

    #[error("PTY not supported on this platform")]
    PtyNotSupported,
}

impl From<std::io::Error> for TerminalError {
    fn from(e: std::io::Error) -> Self {
        TerminalError::Io(e.to_string())
    }
}

/// Ensure a terminal actor has an active PTY process.
///
/// Spawning the actor is supervisor-owned; this helper only normalizes
/// "start if needed" semantics at callsites.
pub async fn ensure_terminal_started(terminal: &ActorRef<TerminalMsg>) -> Result<(), String> {
    match ractor::call!(terminal, |reply| TerminalMsg::Start { reply }) {
        Ok(Ok(())) | Ok(Err(TerminalError::AlreadyRunning)) => Ok(()),
        Ok(Err(e)) => Err(format!("Failed to start terminal: {e}")),
        Err(e) => Err(format!("Failed to call terminal start: {e}")),
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

#[async_trait]
impl Actor for TerminalActor {
    type Msg = TerminalMsg;
    type State = TerminalState;
    type Arguments = TerminalArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(TerminalState {
            terminal_id: args.terminal_id,
            user_id: args.user_id,
            shell: args.shell,
            working_dir: args.working_dir,
            event_store: args.event_store,
            pty_master: None,
            child_killer: None,
            input_tx: None,
            output_tx: None,
            output_buffer: Vec::with_capacity(1000), // Keep last 1000 lines
            is_running: false,
            exit_code: None,
            process_id: None,
            rows: 24,
            cols: 80,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TerminalMsg::Start { reply } => {
                if state.is_running {
                    let _ = reply.send(Err(TerminalError::AlreadyRunning));
                    return Ok(());
                }

                match spawn_pty(
                    &state.shell,
                    &state.working_dir,
                    state.rows,
                    state.cols,
                    myself.clone(),
                )
                .await
                {
                    Ok((pty_master, child_killer, input_tx, output_tx, process_id)) => {
                        state.pty_master = Some(pty_master);
                        state.child_killer = Some(child_killer);
                        state.input_tx = Some(input_tx);
                        state.output_tx = Some(output_tx);
                        state.is_running = true;
                        state.exit_code = None;
                        state.process_id = process_id;
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }

            TerminalMsg::SendInput { input, reply } => {
                if !state.is_running {
                    let _ = reply.send(Err(TerminalError::NotRunning));
                    return Ok(());
                }

                if let Some(ref tx) = state.input_tx {
                    match tx.send(input).await {
                        Ok(_) => {
                            let _ = reply.send(Ok(()));
                        }
                        Err(_) => {
                            let _ = reply
                                .send(Err(TerminalError::Io("Failed to send input".to_string())));
                        }
                    }
                } else {
                    let _ = reply.send(Err(TerminalError::NotRunning));
                }
            }

            TerminalMsg::GetOutput { reply } => {
                let _ = reply.send(state.output_buffer.clone());
            }

            TerminalMsg::SubscribeOutput { reply } => {
                if let Some(ref tx) = state.output_tx {
                    let rx = tx.subscribe();
                    let _ = reply.send(rx);
                } else {
                    // Not running yet; return a closed channel so callers can handle end-of-stream.
                    let (tx, rx) = broadcast::channel::<String>(1);
                    drop(tx);
                    let _ = reply.send(rx);
                }
            }

            TerminalMsg::Resize { rows, cols, reply } => {
                // Ignore pathological 0x0 resizes from transient client layout states.
                // Shared terminal sessions can be viewed by multiple browsers, so one
                // bad resize should not poison the PTY size for everyone.
                let rows = rows.max(2);
                let cols = cols.max(2);
                state.rows = rows;
                state.cols = cols;

                if let Some(ref mut pty_master) = state.pty_master {
                    match pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        Ok(_) => {
                            let _ = reply.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = reply.send(Err(TerminalError::Io(e.to_string())));
                        }
                    }
                } else {
                    let _ = reply.send(Ok(())); // Not running yet, just store dimensions
                }
            }

            TerminalMsg::GetInfo { reply } => {
                let info = TerminalInfo {
                    terminal_id: state.terminal_id.clone(),
                    user_id: state.user_id.clone(),
                    shell: state.shell.clone(),
                    working_dir: state.working_dir.clone(),
                    is_running: state.is_running,
                    exit_code: state.exit_code,
                    process_id: state.process_id,
                    rows: state.rows,
                    cols: state.cols,
                };
                let _ = reply.send(info);
            }

            TerminalMsg::Stop { reply } => {
                if let Some(mut child_killer) = state.child_killer.take() {
                    if let Err(e) = child_killer.kill() {
                        tracing::warn!(
                            terminal_id = %state.terminal_id,
                            error = %e,
                            "Failed to kill terminal child process"
                        );
                    }
                }
                state.pty_master = None;
                state.is_running = false;
                state.input_tx = None;
                state.output_tx = None;
                state.process_id = None;
                let _ = reply.send(Ok(()));
            }

            TerminalMsg::RunAgenticTask {
                objective,
                timeout_ms,
                max_steps,
                model_override,
                progress_tx,
                writer_actor,
                run_id,
                call_id,
                reply,
            } => {
                let result = match (
                    state.is_running,
                    state.input_tx.clone(),
                    state.output_tx.clone(),
                ) {
                    (true, Some(input_tx), Some(output_tx)) => {
                        let run_id_for_exec = run_id.clone();
                        let call_id_for_exec = call_id.clone();
                        let exec = TerminalExecutionContext {
                            terminal_id: state.terminal_id.clone(),
                            user_id: state.user_id.clone(),
                            working_dir: state.working_dir.clone(),
                            shell: state.shell.clone(),
                            event_store: state.event_store.clone(),
                            writer_actor: writer_actor.clone(),
                            run_id: run_id_for_exec,
                            call_id: call_id_for_exec,
                        };
                        drop(input_tx);
                        drop(output_tx);
                        self.run_agentic_task(
                            exec,
                            objective,
                            timeout_ms,
                            max_steps,
                            model_override,
                            progress_tx,
                        )
                        .await
                    }
                    _ => Err(TerminalError::NotRunning),
                };
                Self::emit_writer_completion(
                    writer_actor,
                    run_id.clone(),
                    call_id.clone(),
                    result.clone(),
                );
                let _ = reply.send(result);
            }
            TerminalMsg::RunAgenticTaskDetached {
                objective,
                timeout_ms,
                max_steps,
                model_override,
                progress_tx,
                writer_actor,
                run_id,
                call_id,
            } => {
                let result = match (
                    state.is_running,
                    state.input_tx.clone(),
                    state.output_tx.clone(),
                ) {
                    (true, Some(input_tx), Some(output_tx)) => {
                        let run_id_for_exec = run_id.clone();
                        let call_id_for_exec = call_id.clone();
                        let exec = TerminalExecutionContext {
                            terminal_id: state.terminal_id.clone(),
                            user_id: state.user_id.clone(),
                            working_dir: state.working_dir.clone(),
                            shell: state.shell.clone(),
                            event_store: state.event_store.clone(),
                            writer_actor: writer_actor.clone(),
                            run_id: run_id_for_exec,
                            call_id: call_id_for_exec,
                        };
                        drop(input_tx);
                        drop(output_tx);
                        self.run_agentic_task(
                            exec,
                            objective,
                            timeout_ms,
                            max_steps,
                            model_override,
                            progress_tx,
                        )
                        .await
                    }
                    _ => Err(TerminalError::NotRunning),
                };
                // Emit tool.result to EventStore so ActorRlmPort::resolve_source(ToolOutput, corr_id)
                // can find the result on the next harness turn. The call_id field carries the corr_id
                // assigned by dispatch_bash_async in ActorRlmPort.
                if let Some(corr_id) = &call_id {
                    let (success, output, error) = match &result {
                        Ok(r) => (r.success, r.summary.clone(), None::<String>),
                        Err(e) => (false, String::new(), Some(e.to_string())),
                    };
                    let payload = serde_json::json!({
                        "corr_id": corr_id,
                        "run_id": run_id,
                        "actor_id": state.terminal_id,
                        "success": success,
                        "output": output,
                        "error": error,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    let _ = state.event_store.send_message(
                        crate::actors::event_store::EventStoreMsg::AppendAsync {
                            event: crate::actors::event_store::AppendEvent {
                                event_type: "tool.result".to_string(),
                                payload,
                                actor_id: state.terminal_id.clone(),
                                user_id: state.user_id.clone(),
                            },
                        },
                    );
                }
                Self::emit_writer_completion(writer_actor, run_id, call_id, result);
            }
            TerminalMsg::RunBashTool {
                request,
                progress_tx,
                reply,
            } => {
                let result = match (
                    state.is_running,
                    state.input_tx.clone(),
                    state.output_tx.clone(),
                ) {
                    (true, Some(input_tx), Some(output_tx)) => {
                        let exec = TerminalExecutionContext {
                            terminal_id: state.terminal_id.clone(),
                            user_id: state.user_id.clone(),
                            working_dir: state.working_dir.clone(),
                            shell: state.shell.clone(),
                            event_store: state.event_store.clone(),
                            writer_actor: None,
                            run_id: request.run_id.clone(),
                            call_id: request.call_id.clone(),
                        };
                        drop(input_tx);
                        drop(output_tx);
                        self.run_bash_tool_request(exec, request, progress_tx).await
                    }
                    _ => Err(TerminalError::NotRunning),
                };
                let _ = reply.send(result);
            }

            TerminalMsg::OutputReceived { data } => {
                // Add to buffer, keeping only last 1000 lines
                state.output_buffer.push(data.clone());
                if state.output_buffer.len() > 1000 {
                    state.output_buffer.remove(0);
                }
            }

            TerminalMsg::ProcessExited { exit_code } => {
                state.is_running = false;
                state.exit_code = exit_code;
                state.pty_master = None;
                state.child_killer = None;
                state.input_tx = None;
                state.output_tx = None;
                state.process_id = None;
                // TODO: Emit event to EventStore
            }
        }

        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Clean up PTY if still running
        if let Some(mut child_killer) = state.child_killer.take() {
            let _ = child_killer.kill();
        }
        state.pty_master = None;
        state.input_tx = None;
        state.output_tx = None;
        state.process_id = None;
        Ok(())
    }
}

impl TerminalActor {
    fn emit_writer_completion(
        writer_actor: Option<ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
        result: Result<TerminalAgentResult, TerminalError>,
    ) {
        if let (Some(writer_actor), Some(run_id)) = (writer_actor, run_id) {
            let dispatch_id = call_id
                .clone()
                .and_then(|id| id.rsplit(':').next().map(ToString::to_string))
                .unwrap_or_else(|| ulid::Ulid::new().to_string());
            let completion = result
                .map(|terminal| WriterDelegateResult {
                    capability: WriterDelegateCapability::Terminal,
                    success: terminal.success,
                    summary: terminal.summary,
                    proposed_citation_ids: vec![],
                    proposed_citation_stubs: vec![],
                })
                .map_err(|error| WriterError::WorkerFailed(error.to_string()));
            let _ = writer_actor.send_message(WriterMsg::DelegationWorkerCompleted {
                capability: WriterDelegateCapability::Terminal,
                run_id: Some(run_id),
                call_id,
                dispatch_id,
                result: completion,
            });
        }
    }

    async fn run_agentic_task(
        &self,
        ctx: TerminalExecutionContext,
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
    ) -> Result<TerminalAgentResult, TerminalError> {
        // Create the terminal adapter for the harness
        let adapter = TerminalAdapter::new(
            ctx.terminal_id.clone(),
            ctx.working_dir.clone(),
            ctx.shell.clone(),
            Some(ctx.event_store.clone()),
            progress_tx.clone(),
            ctx.writer_actor.clone(),
            ctx.run_id.clone(),
        );

        // Configure the harness with provided parameters
        let config = HarnessConfig {
            timeout_budget_ms: timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000),
            max_steps: max_steps.unwrap_or(100).clamp(1, 100) as usize,
            emit_progress: true,
            emit_worker_report: true,
        };

        let timeout_budget_ms = config.timeout_budget_ms;
        let harness = AgentHarness::with_config(
            adapter,
            ModelRegistry::new(),
            config,
            LlmTraceEmitter::new(ctx.event_store.clone()),
        );

        // Run the agentic loop through the harness, enforcing the wall-clock budget
        let timeout_budget = std::time::Duration::from_millis(timeout_budget_ms);
        let run_future = harness.run(
            ctx.terminal_id.clone(),
            ctx.user_id.clone(),
            objective.clone(),
            model_override,
            None, // progress_tx for harness internal use - we use terminal-specific progress
            ctx.run_id.clone(),
            ctx.call_id.clone(),
        );
        let result = tokio::time::timeout(timeout_budget, run_future)
            .await
            .unwrap_or(Err(HarnessError::Timeout(timeout_budget_ms)));

        match result {
            Ok(agent_result) => {
                // Convert harness result to TerminalAgentResult
                let steps: Vec<TerminalExecutionStep> = agent_result
                    .tool_executions
                    .iter()
                    .map(|exec| TerminalExecutionStep {
                        command: exec.tool_name.clone(),
                        exit_code: if exec.success { 0 } else { 1 },
                        output_excerpt: TerminalAdapter::truncate_excerpt(&exec.output),
                    })
                    .collect();

                let executed_commands: Vec<String> = agent_result
                    .tool_executions
                    .iter()
                    .filter(|exec| exec.tool_name == "bash")
                    .map(|exec| exec.output.clone())
                    .collect();

                Ok(TerminalAgentResult {
                    summary: agent_result.summary,
                    reasoning: None, // Could extract from learnings if needed
                    success: agent_result.success,
                    model_used: agent_result.model_used,
                    exit_code: agent_result.tool_executions.last().map(|exec| {
                        if exec.success {
                            0
                        } else {
                            1
                        }
                    }),
                    executed_commands,
                    steps,
                })
            }
            Err(e) => {
                // Preserve Timeout errors; convert others to Blocked
                match e {
                    HarnessError::Timeout(ms) => Err(TerminalError::Timeout(ms)),
                    other => Err(TerminalError::Blocked(other.to_string())),
                }
            }
        }
    }

    async fn run_bash_tool_request(
        &self,
        ctx: TerminalExecutionContext,
        request: TerminalBashToolRequest,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
    ) -> Result<TerminalAgentResult, TerminalError> {
        // For RunBashTool, we use the harness with a single-step objective
        let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000);

        // Emit initial progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(TerminalAgentProgress {
                phase: "terminal_tool_dispatch".to_string(),
                message: "terminal actor received typed bash tool request".to_string(),
                reasoning: request.reasoning.clone(),
                command: Some(request.cmd.clone()),
                model_used: request.model_override.clone(),
                output_excerpt: None,
                exit_code: None,
                step_index: Some(1),
                step_total: Some(1),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }

        // Use the adapter directly for single command execution
        let adapter = TerminalAdapter::new(
            ctx.terminal_id.clone(),
            ctx.working_dir.clone(),
            ctx.shell.clone(),
            Some(ctx.event_store.clone()),
            progress_tx.clone(),
            None,
            ctx.run_id.clone(),
        );
        let trace_emitter = LlmTraceEmitter::new(ctx.event_store.clone());
        let tool_ctx = trace_emitter.start_tool_call(
            "terminal",
            &ctx.terminal_id,
            "bash",
            &serde_json::json!({
                "cmd": request.cmd.clone(),
                "timeout_ms": timeout_ms,
            }),
            request.reasoning.as_deref(),
            Some(crate::observability::llm_trace::LlmCallScope {
                run_id: ctx.run_id.clone(),
                task_id: None,
                call_id: ctx.call_id.clone(),
                session_id: None,
                thread_id: None,
            }),
        );

        // Execute the command directly using the adapter
        match adapter.execute_bash(&request.cmd, timeout_ms).await {
            Ok((output, exit_code)) => {
                let success = exit_code == 0;
                let exit_error = if success {
                    None
                } else {
                    Some(format!("Exit status {exit_code}"))
                };
                trace_emitter.complete_tool_call(
                    &tool_ctx,
                    success,
                    &output,
                    exit_error.as_deref(),
                );

                // Emit completion progress
                if let Some(ref tx) = progress_tx {
                    let _ = tx.send(TerminalAgentProgress {
                        phase: "terminal_tool_result".to_string(),
                        message: "terminal agent received bash tool result".to_string(),
                        reasoning: request.reasoning.clone(),
                        command: Some(request.cmd.clone()),
                        model_used: request.model_override.clone(),
                        output_excerpt: Some(TerminalAdapter::truncate_excerpt(&output)),
                        exit_code: Some(exit_code),
                        step_index: Some(1),
                        step_total: Some(1),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                Ok(TerminalAgentResult {
                    summary: output.clone(),
                    reasoning: request.reasoning.clone(),
                    success,
                    model_used: request.model_override.clone(),
                    exit_code: Some(exit_code),
                    executed_commands: vec![request.cmd.clone()],
                    steps: vec![TerminalExecutionStep {
                        command: request.cmd.clone(),
                        exit_code,
                        output_excerpt: TerminalAdapter::truncate_excerpt(&output),
                    }],
                })
            }
            Err(e) => {
                trace_emitter.complete_tool_call(&tool_ctx, false, "", Some(&e.to_string()));
                Err(e)
            }
        }
    }
}

// ============================================================================
// PTY Implementation
// ============================================================================

/// Spawn a PTY process and return handles
async fn spawn_pty(
    shell: &str,
    working_dir: &str,
    rows: u16,
    cols: u16,
    actor_ref: ActorRef<TerminalMsg>,
) -> Result<
    (
        Box<dyn portable_pty::MasterPty + Send>,
        Box<dyn ChildKiller + Send + Sync>,
        mpsc::Sender<String>,
        broadcast::Sender<String>,
        Option<u32>,
    ),
    TerminalError,
> {
    // Select the appropriate PTY system for the platform
    let pty_system = portable_pty::native_pty_system();

    // Open a PTY
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;

    // Build the command
    let mut cmd_builder = CommandBuilder::new(shell);
    cmd_builder.cwd(std::path::Path::new(working_dir));

    // Spawn the shell in the PTY
    let mut child = pair
        .slave
        .spawn_command(cmd_builder)
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;
    let child_killer = child.clone_killer();
    let process_id = child.process_id();

    // Create channels for communication
    let (input_tx, mut input_rx) = mpsc::channel::<String>(100);
    let (output_tx, _output_rx) = broadcast::channel::<String>(1000);

    // Get handles for I/O
    let mut master_writer = pair
        .master
        .take_writer()
        .map_err(|e| TerminalError::SpawnFailed(format!("Failed to get PTY writer: {e}")))?;

    let mut master_reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| TerminalError::SpawnFailed(format!("Failed to clone PTY reader: {e}")))?;

    // Spawn input task: read from channel, write to PTY (blocking I/O in spawn_blocking)
    tokio::task::spawn_blocking(move || {
        while let Some(input) = input_rx.blocking_recv() {
            if master_writer.write_all(input.as_bytes()).is_err() {
                break;
            }
            if master_writer.flush().is_err() {
                break;
            }
        }
    });

    // Spawn output task: read from PTY, forward into actor mailbox.
    // Actor state then handles buffer + subscriber broadcast in-order.
    let actor = actor_ref.clone();
    let output_tx_for_reader = output_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match master_reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF - process exited
                    let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
                    break;
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                    let _ = output_tx_for_reader.send(data.clone());
                    let _ = actor.send_message(TerminalMsg::OutputReceived { data });
                }
                Err(_) => {
                    // Read error
                    let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
                    break;
                }
            }
        }
    });

    // Spawn exit monitor task
    let actor = actor_ref.clone();
    tokio::task::spawn_blocking(move || {
        // Wait for the child process to exit
        match child.wait() {
            Ok(exit_status) => {
                let exit_code = Some(exit_status.exit_code() as i32);
                let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code });
            }
            Err(_) => {
                let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
            }
        }
    });

    Ok((pair.master, child_killer, input_tx, output_tx, process_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;
    use std::net::TcpListener;
    use tokio::time::{sleep, timeout, Duration, Instant};

    fn test_shell() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }

    fn test_working_dir() -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/".to_string())
    }

    fn has_curl() -> bool {
        std::process::Command::new("sh")
            .arg("-c")
            .arg("command -v curl >/dev/null 2>&1")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn has_live_terminal_planner() -> bool {
        let bedrock_auth = std::env::var("AWS_BEARER_TOKEN_BEDROCK")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            || std::env::var("AWS_PROFILE")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            || (std::env::var("AWS_ACCESS_KEY_ID")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
                && std::env::var("AWS_SECRET_ACCESS_KEY")
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false));
        bedrock_auth && crate::runtime_env::ensure_tls_cert_env().is_some()
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output();

        match output {
            Ok(output) => {
                output.status.success()
                    && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
            }
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    async fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !process_exists(pid) {
                return true;
            }
            sleep(Duration::from_millis(50)).await;
        }
        !process_exists(pid)
    }

    async fn wait_for_output(
        rx: &mut broadcast::Receiver<String>,
        needle: &str,
        timeout_duration: Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, rx.recv()).await {
                Ok(Ok(chunk)) => {
                    if chunk.contains(needle) {
                        return true;
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => return false,
                Err(_) => return false,
            }
        }
        false
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_stop_terminates_terminal_process() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-stop".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let info_before_stop = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info call failed");
        assert!(info_before_stop.is_running);
        let pid = info_before_stop
            .process_id
            .expect("terminal start should provide a process id");
        assert!(
            process_exists(pid),
            "expected process {pid} to exist after start"
        );

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        let exited = wait_for_process_exit(pid, Duration::from_secs(3)).await;
        assert!(exited, "terminal process {pid} still alive after stop");

        let info_after_stop = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info call failed");
        assert!(!info_after_stop.is_running);
        assert!(info_after_stop.process_id.is_none());

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_repeated_start_stop_cleans_up_each_process() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-restart".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        for _ in 0..5 {
            let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
                .expect("start call failed");
            assert!(
                start_result.is_ok(),
                "terminal failed to start: {start_result:?}"
            );

            let info = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
                .expect("get info call failed");
            let pid = info
                .process_id
                .expect("terminal start should provide a process id");
            assert!(
                process_exists(pid),
                "expected process {pid} to exist after start"
            );

            let stop_result = ractor::call!(terminal, |reply| TerminalMsg::Stop { reply })
                .expect("stop call failed");
            assert!(
                stop_result.is_ok(),
                "terminal failed to stop: {stop_result:?}"
            );

            let exited = wait_for_process_exit(pid, Duration::from_secs(3)).await;
            assert!(exited, "terminal process {pid} still alive after stop");
        }

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_terminal_output() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-multisub".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let mut rx_1 = ractor::call!(terminal, |reply| TerminalMsg::SubscribeOutput { reply })
            .expect("subscribe output #1 failed");
        let mut rx_2 = ractor::call!(terminal, |reply| TerminalMsg::SubscribeOutput { reply })
            .expect("subscribe output #2 failed");

        let marker = format!(
            "CHOIR_TERM_MULTI_SUB_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time error")
                .as_millis()
        );
        let command = format!("echo {marker}\n");

        let send_result = ractor::call!(terminal, |reply| TerminalMsg::SendInput {
            input: command,
            reply
        })
        .expect("send input call failed");
        assert!(send_result.is_ok(), "send input failed: {send_result:?}");

        let got_1 = wait_for_output(&mut rx_1, &marker, Duration::from_secs(3)).await;
        let got_2 = wait_for_output(&mut rx_2, &marker, Duration::from_secs(3)).await;
        assert!(got_1, "first subscriber did not receive marker output");
        assert!(got_2, "second subscriber did not receive marker output");

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_resize_clamps_zero_dimensions() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-resize-clamp".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let resize_result = ractor::call!(terminal, |reply| TerminalMsg::Resize {
            rows: 0,
            cols: 0,
            reply
        })
        .expect("resize call failed");
        assert!(resize_result.is_ok(), "resize failed: {resize_result:?}");

        let info = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info failed");
        assert!(
            info.rows >= 2 && info.cols >= 2,
            "expected clamped terminal size >= 2x2, got {}x{}",
            info.rows,
            info.cols
        );

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_run_agentic_task_executes_curl_against_local_server() {
        if !has_curl() {
            return;
        }
        if !has_live_terminal_planner() {
            return;
        }

        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind local test server");
        let port = listener
            .local_addr()
            .expect("failed to read local addr")
            .port();

        let server_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let server_done_clone = server_done.clone();
        let server = std::thread::spawn(move || {
            // Accept connections until the done flag is set (non-blocking poll)
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while std::time::Instant::now() < deadline
                && !server_done_clone.load(std::sync::atomic::Ordering::Relaxed)
            {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request_buf = [0_u8; 1024];
                        let _ = std::io::Read::read(&mut stream, &mut request_buf);
                        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nlocal-ok\n";
                        let _ = std::io::Write::write_all(&mut stream, response);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });

        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-agentic-curl".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let objective = format!("curl -s 'http://127.0.0.1:{port}/'");
        let run_result = ractor::call!(terminal, |reply| TerminalMsg::RunAgenticTask {
            objective,
            timeout_ms: Some(30_000),
            max_steps: Some(5),
            model_override: None,
            progress_tx: None,
            writer_actor: None,
            run_id: None,
            call_id: None,
            reply,
        })
        .expect("run agentic task call failed")
        .expect("run agentic task returned error");

        assert!(
            run_result.success,
            "expected success from local curl task, got: {run_result:?}"
        );
        // The model may surface "local-ok" in either the final summary or in
        // the raw bash output captured in executed_commands.
        let payload_found = run_result.summary.contains("local-ok")
            || run_result
                .executed_commands
                .iter()
                .any(|c| c.contains("local-ok"));
        assert!(
            payload_found,
            "expected local payload in summary or executed_commands, got summary: {}",
            run_result.summary
        );
        assert!(
            !run_result.steps.is_empty(),
            "expected at least one execution step"
        );

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        server_done.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = server.join();
        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_run_agentic_task_times_out_long_command() {
        if !has_live_terminal_planner() {
            return;
        }

        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-agentic-timeout".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let run_result = ractor::call!(terminal, |reply| TerminalMsg::RunAgenticTask {
            objective: "sleep 2 && echo done".to_string(),
            timeout_ms: Some(1_000),
            max_steps: None,
            model_override: None,
            progress_tx: None,
            writer_actor: None,
            run_id: None,
            call_id: None,
            reply,
        })
        .expect("run agentic task call failed");

        match run_result {
            Ok(result) => panic!("expected timeout error, got success: {result:?}"),
            Err(TerminalError::Timeout(ms)) => assert!(ms >= 1_000, "unexpected timeout ms: {ms}"),
            Err(e) => panic!("expected timeout error variant, got: {e:?}"),
        }

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        terminal.stop(None);
        event_store.stop(None);
    }
}
