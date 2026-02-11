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
use chrono::{SecondsFormat, Utc};
use portable_pty::{ChildKiller, CommandBuilder, PtySize};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::io::{Read, Write};
use tokio::sync::{broadcast, mpsc};

use crate::actors::event_store::EventStoreMsg;
use crate::actors::model_config::{ModelConfigError, ModelRegistry, ModelResolutionContext};
use crate::baml_client::types::{Message as BamlMessage, ToolResult as BamlToolResult};
use crate::baml_client::{ClientRegistry, B};

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
        reply: RpcReplyPort<Result<TerminalAgentResult, TerminalError>>,
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
}

#[derive(Clone)]
struct TerminalExecutionContext {
    terminal_id: String,
    working_dir: String,
    shell: String,
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
                            working_dir: state.working_dir.clone(),
                            shell: state.shell.clone(),
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
                let _ = reply.send(result);
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
                            working_dir: state.working_dir.clone(),
                            shell: state.shell.clone(),
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
    fn map_model_error(error: ModelConfigError) -> TerminalError {
        match error {
            ModelConfigError::UnknownModel(model_id) => {
                TerminalError::InvalidInput(format!("Unknown model: {model_id}"))
            }
            ModelConfigError::MissingApiKey(env_var) => TerminalError::InvalidInput(format!(
                "Missing API key environment variable for selected model: {env_var}"
            )),
            ModelConfigError::NoFallbackAvailable => {
                TerminalError::InvalidInput("No fallback model available".to_string())
            }
        }
    }

    fn resolve_model_registry(
        model_override: Option<String>,
    ) -> Result<(ClientRegistry, String), TerminalError> {
        let registry = ModelRegistry::new();
        let resolved_model = registry
            .resolve_for_role(
                "terminal",
                &ModelResolutionContext {
                    request_model: model_override,
                    app_preference: std::env::var("CHOIR_TERMINAL_MODEL")
                        .ok()
                        .filter(|value| !value.trim().is_empty()),
                    user_preference: None,
                },
            )
            .map_err(Self::map_model_error)?;
        let model_id = resolved_model.config.id;
        let client_registry = registry
            .create_runtime_client_registry_for_model(&model_id)
            .map_err(Self::map_model_error)?;
        Ok((client_registry, model_id))
    }

    fn format_timestamp() -> String {
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    fn timestamped_prompt_content(content: &str) -> String {
        format!("[{}]\n{}", Self::format_timestamp(), content)
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
        let per_step_timeout = timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000);
        let max_steps = max_steps.unwrap_or(3).clamp(1, 6) as usize;
        Self::emit_progress(
            &progress_tx,
            "terminal_agent_starting",
            "terminal agent started objective execution",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(max_steps),
        );

        let mut executed_commands = Vec::new();
        let mut steps: Vec<TerminalExecutionStep> = Vec::new();
        let mut tool_results: Vec<BamlToolResult> = Vec::new();
        let mut messages = vec![BamlMessage {
            role: "user".to_string(),
            content: Self::timestamped_prompt_content(&objective),
        }];
        let mut latest_reasoning: Option<String> = None;
        let (client_registry, model_used) = Self::resolve_model_registry(model_override)?;

        let tools_description = r#"Tool: bash
Description: Execute shell commands in the current terminal.
Parameters Schema: {"type":"object","properties":{"command":{"type":"string","description":"The shell command to execute"},"timeout_ms":{"type":"integer","description":"Timeout in milliseconds"}},"required":["command"]}"#;

        let system_context = format!(
            "You are ChoirOS Terminal Agent. Use bash tool calls to complete terminal objectives.\nSystem Prompt Timestamp (UTC): {}\nCurrent UTC Timestamp: {}\nTerminal ID: {}\nWorking Directory: {}\nPrefer minimal safe command sequences.",
            Self::format_timestamp(),
            Self::format_timestamp(),
            ctx.terminal_id,
            ctx.working_dir
        );
        Self::emit_progress(
            &progress_tx,
            "terminal_agent_model_selected",
            "terminal agent selected runtime model",
            Some(format!("Using model {model_used}")),
            None,
            Some(model_used.clone()),
            None,
            None,
            None,
            Some(max_steps),
        );

        for _ in 0..max_steps {
            Self::emit_progress(
                &progress_tx,
                "terminal_agent_planning",
                "terminal agent is planning next action",
                None,
                None,
                Some(model_used.clone()),
                None,
                None,
                None,
                Some(max_steps),
            );
            let plan = match B
                .PlanAction
                .with_client_registry(&client_registry)
                .call(&messages, &system_context, tools_description)
                .await
            {
                Ok(plan) => plan,
                Err(e) => {
                    let reason = format!("Planning failed: {e}");
                    Self::emit_progress(
                        &progress_tx,
                        "terminal_agent_blocked",
                        "terminal agent blocked during planning",
                        Some(reason.clone()),
                        Some(objective.clone()),
                        Some(model_used.clone()),
                        None,
                        None,
                        None,
                        Some(max_steps),
                    );
                    return Err(TerminalError::Blocked(reason));
                }
            };
            latest_reasoning = Some(plan.thinking.clone());
            Self::emit_progress(
                &progress_tx,
                "terminal_agent_reasoning",
                "terminal agent produced reasoning",
                Some(plan.thinking.clone()),
                None,
                Some(model_used.clone()),
                None,
                None,
                None,
                Some(max_steps),
            );

            if plan.tool_calls.is_empty() {
                if let Some(final_response) = plan.final_response {
                    Self::emit_progress(
                        &progress_tx,
                        "terminal_agent_synthesizing",
                        "terminal agent produced final response without tool calls",
                        latest_reasoning.clone(),
                        None,
                        Some(model_used.clone()),
                        Some(Self::truncate_excerpt(&final_response)),
                        None,
                        Some(steps.len()),
                        Some(max_steps),
                    );
                    return Ok(TerminalAgentResult {
                        summary: final_response,
                        reasoning: latest_reasoning.clone(),
                        success: tool_results.iter().all(|r| r.success),
                        model_used: Some(model_used.clone()),
                        exit_code: steps.last().map(|s| s.exit_code),
                        executed_commands,
                        steps,
                    });
                }
                break;
            }

            for tool_call in &plan.tool_calls {
                if tool_call.tool_name != "bash" {
                    continue;
                }

                let bash_args = tool_call.tool_args.bash.as_ref();
                let command = match bash_args
                    .and_then(|args| args.command.as_deref().or(args.cmd.as_deref()))
                    .or(tool_call.tool_args.command.as_deref())
                    .or(tool_call.tool_args.cmd.as_deref())
                {
                    Some(command) if !command.trim().is_empty() => command.to_string(),
                    _ => {
                        tool_results.push(BamlToolResult {
                            tool_name: "bash".to_string(),
                            success: false,
                            output: String::new(),
                            error: Some("Missing command/cmd".to_string()),
                        });
                        continue;
                    }
                };

                let command_timeout = bash_args
                    .and_then(|args| args.timeout_ms)
                    .or(tool_call.tool_args.timeout_ms)
                    .and_then(|value| u64::try_from(value).ok())
                    .unwrap_or(per_step_timeout)
                    .clamp(1_000, 120_000);

                executed_commands.push(command.clone());
                let step_index = executed_commands.len();
                Self::emit_progress(
                    &progress_tx,
                    "terminal_tool_call",
                    "terminal agent requested bash tool execution",
                    tool_call.reasoning.clone(),
                    Some(command.clone()),
                    Some(model_used.clone()),
                    None,
                    None,
                    Some(step_index),
                    Some(max_steps),
                );
                let (output, exit_code) = self
                    .execute_terminal_command(&ctx, &command, command_timeout)
                    .await?;
                let output_excerpt = Self::truncate_excerpt(&output);
                steps.push(TerminalExecutionStep {
                    command: command.clone(),
                    exit_code,
                    output_excerpt: output_excerpt.clone(),
                });
                tool_results.push(BamlToolResult {
                    tool_name: "bash".to_string(),
                    success: exit_code == 0,
                    output: output.clone(),
                    error: if exit_code == 0 {
                        None
                    } else {
                        Some(format!("Exit status {exit_code}"))
                    },
                });
                Self::emit_progress(
                    &progress_tx,
                    "terminal_tool_result",
                    "terminal agent received bash tool result",
                    tool_call.reasoning.clone(),
                    Some(command.clone()),
                    Some(model_used.clone()),
                    Some(output_excerpt),
                    Some(exit_code),
                    Some(step_index),
                    Some(max_steps),
                );
                messages.push(BamlMessage {
                    role: "assistant".to_string(),
                    content: Self::timestamped_prompt_content(&format!(
                        "Executed bash command:\n{}\nOutput:\n{}",
                        command, output
                    )),
                });
            }
        }

        let conversation_context = format!(
            "Generated at UTC {}. Executed {} terminal commands in {}.",
            Self::format_timestamp(),
            executed_commands.len(),
            ctx.working_dir
        );
        let synthesis_objective = Self::timestamped_prompt_content(&objective);
        let summary = B
            .SynthesizeResponse
            .with_client_registry(&client_registry)
            .call(&synthesis_objective, &tool_results, &conversation_context)
            .await
            .unwrap_or_else(|_| {
                tool_results
                    .last()
                    .map(|r| r.output.clone())
                    .unwrap_or_else(|| "No terminal actions were executed.".to_string())
            });
        Self::emit_progress(
            &progress_tx,
            "terminal_agent_synthesizing",
            "terminal agent synthesized final response",
            latest_reasoning.clone(),
            None,
            Some(model_used.clone()),
            Some(Self::truncate_excerpt(&summary)),
            None,
            Some(steps.len()),
            Some(max_steps),
        );

        Ok(TerminalAgentResult {
            summary,
            reasoning: latest_reasoning,
            success: tool_results.iter().all(|r| r.success),
            model_used: Some(model_used),
            exit_code: steps.last().map(|s| s.exit_code),
            executed_commands,
            steps,
        })
    }

    async fn execute_terminal_command(
        &self,
        ctx: &TerminalExecutionContext,
        command: &str,
        timeout_ms: u64,
    ) -> Result<(String, i32), TerminalError> {
        Self::validate_command_policy(command)?;
        let command = Self::normalize_command_for_runtime(command, timeout_ms);
        let output = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::process::Command::new(&ctx.shell)
                .arg("-lc")
                .arg(&command)
                .current_dir(&ctx.working_dir)
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

    async fn run_bash_tool_request(
        &self,
        ctx: TerminalExecutionContext,
        request: TerminalBashToolRequest,
        progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
    ) -> Result<TerminalAgentResult, TerminalError> {
        let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000);
        Self::emit_progress(
            &progress_tx,
            "terminal_tool_dispatch",
            "terminal actor received typed bash tool request",
            request.reasoning.clone(),
            Some(request.cmd.clone()),
            request.model_override.clone(),
            None,
            None,
            Some(1),
            Some(1),
        );

        let result = self
            .run_agentic_task(
                ctx,
                request.cmd.clone(),
                Some(timeout_ms),
                Some(4),
                request.model_override.clone(),
                progress_tx,
            )
            .await?;
        Ok(result)
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

    fn emit_progress(
        progress_tx: &Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
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
        let Some(tx) = progress_tx else {
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

    fn truncate_excerpt(text: &str) -> String {
        let max_len = 1200;
        let mut excerpt: String = text.chars().take(max_len).collect();
        if text.chars().count() > max_len {
            excerpt.push_str("...");
        }
        excerpt
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

    #[test]
    fn test_timestamped_prompt_content_prefixes_iso_timestamp() {
        let stamped = TerminalActor::timestamped_prompt_content("run ls");
        assert!(stamped.starts_with('['));
        assert!(stamped.contains("T"));
        assert!(stamped.contains("Z]\nrun ls"));
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

        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request_buf = [0_u8; 1024];
                let _ = std::io::Read::read(&mut stream, &mut request_buf);
                let response =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nlocal-ok\n";
                let _ = std::io::Write::write_all(&mut stream, response);
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
            timeout_ms: Some(5_000),
            max_steps: Some(1),
            model_override: None,
            progress_tx: None,
            reply,
        })
        .expect("run agentic task call failed")
        .expect("run agentic task returned error");

        assert!(
            run_result.success,
            "expected success from local curl task, got: {run_result:?}"
        );
        assert!(
            run_result.summary.contains("local-ok"),
            "expected local payload in summary, got: {}",
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
            max_steps: Some(1),
            model_override: None,
            progress_tx: None,
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
