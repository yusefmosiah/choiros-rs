//! Production `AlmPort` implementation for use within the actor system.
//!
//! `ActorAlmPort` holds references to the live actor tree and implements
//! `AlmPort` with two hard rules:
//!
//! ## Shell isolation rule
//!
//! Shell execution is **only** permitted through `TerminalActor`. No other
//! actor may call `tokio::process::Command` or equivalent.
//!
//! ## Async rule
//!
//! No `ractor::call!` (blocking RPC). Every actor interaction is a fire-and-forget
//! `send_message`. If the harness needs a result from a terminal command or
//! subharness, it:
//! 1. Fires the message with a `corr_id`
//! 2. Writes a checkpoint recording the pending corr_id
//! 3. Ends the turn — returning the corr_id as the step output
//! 4. On the next turn the model requests `ContextSourceKind::ToolOutput(corr_id)`
//!    which reads the result from EventStore (written by the terminal/subharness
//!    on completion).
//!
//! `execute_tool("bash")` therefore does NOT return the command output — it
//! returns `"dispatched:corr_id:<id>"`. The model learns to treat any inline
//! bash in a DAG step as an async dispatch whose result arrives next turn.
//!
//! ## File I/O
//!
//! `file_read` / `file_write` use `tokio::fs` directly — no code execution,
//! no shell, acceptable risk surface. These are the only inline synchronous
//! operations allowed.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use ractor::{Actor, ActorRef};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::actors::agent_harness::alm::{LlmCallResult, AlmPort, AlmToolExecution};
use crate::actors::conductor::protocol::{ConductorMsg, ActorHarnessMsg};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::actor_harness_actor::{ActorHarnessActor, ActorHarnessArguments};
use crate::actors::terminal::TerminalMsg;
use crate::baml_client::types::ContextSourceKind;
use crate::baml_client::{new_collector, B};
use shared_types::HarnessCheckpoint;

/// Production `AlmPort` backed by live actor references.
///
/// All actor interactions are fire-and-forget. Shell access is gated through
/// `terminal`. This port never calls `tokio::process::Command` directly.
pub struct ActorAlmPort {
    pub run_id: String,
    pub actor_id: String,
    pub model_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    /// Conductor that subharness actors reply to on completion.
    pub conductor: ActorRef<ConductorMsg>,
    /// Terminal actor for shell command delegation.
    /// `None` means shell tools are unavailable for this harness instance.
    pub terminal: Option<ActorRef<TerminalMsg>>,
}

impl ActorAlmPort {
    pub fn new(
        run_id: impl Into<String>,
        actor_id: impl Into<String>,
        model_id: impl Into<String>,
        event_store: ActorRef<EventStoreMsg>,
        conductor: ActorRef<ConductorMsg>,
        terminal: Option<ActorRef<TerminalMsg>>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            actor_id: actor_id.into(),
            model_id: model_id.into(),
            event_store,
            conductor,
            terminal,
        }
    }

    /// Dispatch a bash command to TerminalActor asynchronously.
    ///
    /// Returns the corr_id immediately. The terminal writes the result to
    /// EventStore as a `tool.result` event on completion.
    fn dispatch_bash_async(&self, command: &str, corr_id: &str) {
        let Some(terminal) = &self.terminal else {
            warn!("dispatch_bash_async: no terminal actor, corr_id:{corr_id}");
            return;
        };
        let _ = terminal.send_message(TerminalMsg::RunAgenticTaskDetached {
            objective: command.to_string(),
            timeout_ms: Some(60_000),
            max_steps: Some(10),
            model_override: None,
            progress_tx: None,
            writer_actor: None,
            run_id: Some(self.run_id.clone()),
            call_id: Some(corr_id.to_string()),
        });
    }
}

#[async_trait]
impl AlmPort for ActorAlmPort {
    fn capabilities_description(&self) -> String {
        let shell_line = if self.terminal.is_some() {
            "1. bash - Execute shell commands via TerminalActor (async — result arrives next turn)\n   Args: command (string, required)\n"
        } else {
            "   (bash not available — no terminal actor)\n"
        };
        format!(
            r#"Available tools:

{shell_line}
2. file_read - Read a local file (synchronous)
   Args: path (string, required)

3. file_write - Write or overwrite a file (synchronous)
   Args: path (string, required), content (string, required)

Available context sources:
- Document <path>: Load file content
- ToolOutput <corr_id>: Read result of a prior async bash/subharness dispatch
- PreviousTurn <N>: Prior turn output (already in turn context)

IMPORTANT: bash dispatches are async. The result is NOT available this turn.
After a bash dispatch the step output is "dispatched:corr_id:<id>". Use
ContextSourceKind::ToolOutput with that corr_id on the next turn to read the result.
"#
        )
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// Resolve a context source.
    ///
    /// `ToolOutput` polls EventStore for a result written by a prior async
    /// dispatch (terminal command or subharness). Returns `None` immediately if
    /// the result hasn't landed yet — the model re-requests next turn. Never
    /// blocks waiting for slow work.
    ///
    /// ## Why `ractor::call!` is acceptable here
    ///
    /// EventStore is a pure SQLite store actor — it does one bounded DB read
    /// and replies in microseconds. It does not orchestrate, spawn workers, or
    /// do user-initiated I/O. The harness runs in a spawned task (not inside an
    /// actor handle loop), so awaiting a fast DB reply is safe.
    ///
    /// This is categorically different from waiting on TerminalActor, which can
    /// take seconds to minutes and must never be awaited directly.
    async fn resolve_source(
        &self,
        kind: &ContextSourceKind,
        source_ref: &str,
        _max_tokens: Option<i64>,
    ) -> Option<String> {
        match kind {
            ContextSourceKind::Document => tokio::fs::read_to_string(source_ref).await.ok(),
            ContextSourceKind::ToolOutput => {
                // Poll EventStore for result events keyed by corr_id.
                // 2s timeout on the DB call: if EventStore is stuck we treat
                // the result as not-ready and the model retries next turn.
                for event_prefix in &["actor_harness.result", "tool.result"] {
                    let result = ractor::call_t!(
                        self.event_store,
                        |reply| EventStoreMsg::GetEventsByCorrId {
                            corr_id: source_ref.to_string(),
                            event_type_prefix: Some(event_prefix.to_string()),
                            reply,
                        },
                        2000
                    );
                    match result {
                        Ok(Ok(events)) if !events.is_empty() => {
                            let event = events.last().unwrap();
                            let text = event
                                .payload
                                .get("output_excerpt")
                                .or_else(|| event.payload.get("output"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| event.payload.to_string());
                            return Some(text);
                        }
                        Ok(Ok(_)) => continue, // not found under this prefix yet
                        Ok(Err(e)) => {
                            warn!("EventStore error reading corr_id {source_ref}: {e}");
                            return None;
                        }
                        Err(e) => {
                            // Timeout or actor dead — treat as not-ready
                            warn!("EventStore call_t error corr_id {source_ref}: {e}");
                            return None;
                        }
                    }
                }
                None // result not yet available — model retries next turn
            }
            ContextSourceKind::MemoryQuery | ContextSourceKind::PreviousTurn => None,
        }
    }

    /// Execute a tool call within the current turn.
    ///
    /// `bash` is dispatched asynchronously to TerminalActor — the return value
    /// is the corr_id string, not the command output. The model reads the
    /// actual result on the next turn via `resolve_source(ToolOutput, corr_id)`.
    ///
    /// `file_read` / `file_write` execute synchronously via `tokio::fs`.
    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
    ) -> AlmToolExecution {
        let start = Instant::now();
        match tool_name {
            "bash" => {
                let command = tool_args.get("command").cloned().unwrap_or_default();
                if self.terminal.is_none() {
                    return AlmToolExecution {
                        turn: 0,
                        tool_name: "bash".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("no terminal actor — bash unavailable".into()),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
                // Async dispatch: fire and return corr_id immediately.
                let corr_id = format!("bash-{}", Uuid::new_v4().as_simple());
                self.dispatch_bash_async(&command, &corr_id);
                info!("execute_tool(bash) dispatched async corr:{corr_id}");
                AlmToolExecution {
                    turn: 0,
                    tool_name: "bash".into(),
                    tool_args: tool_args.clone(),
                    success: true,
                    output: format!("dispatched:corr_id:{corr_id}"),
                    error: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                }
            }

            "file_read" => {
                let path = tool_args.get("path").map(|s| s.as_str()).unwrap_or("");
                match tokio::fs::read_to_string(path).await {
                    Ok(content) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_read".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: content,
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_read".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("read: {e}")),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }

            "file_write" => {
                let path = tool_args.get("path").map(|s| s.as_str()).unwrap_or("");
                let content = tool_args.get("content").map(|s| s.as_str()).unwrap_or("");
                match tokio::fs::write(path, content).await {
                    Ok(_) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_write".into(),
                        tool_args: tool_args.clone(),
                        success: true,
                        output: format!("wrote {} bytes", content.len()),
                        error: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => AlmToolExecution {
                        turn: 0,
                        tool_name: "file_write".into(),
                        tool_args: tool_args.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("write: {e}")),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }

            other => AlmToolExecution {
                turn: 0,
                tool_name: other.into(),
                tool_args: tool_args.clone(),
                success: false,
                output: String::new(),
                error: Some(format!(
                    "tool '{other}' not available inline; \
                     route through the appropriate actor"
                )),
                elapsed_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    /// Fire a tool call asynchronously (non-blocking, named dispatch).
    ///
    /// The caller provides the corr_id so it can be tracked in a checkpoint.
    /// For bash: delegates to TerminalActor via `RunAgenticTaskDetached`.
    async fn dispatch_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
        corr_id: &str,
    ) {
        match tool_name {
            "bash" => {
                let command = tool_args.get("command").cloned().unwrap_or_default();
                self.dispatch_bash_async(&command, corr_id);
                info!("dispatch_tool(bash) corr:{corr_id}");
            }
            other => {
                warn!("dispatch_tool: unhandled tool '{other}' corr:{corr_id} — no-op");
            }
        }
    }

    async fn call_llm(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        model_hint: Option<&str>,
    ) -> LlmCallResult {
        let start = Instant::now();
        // Resolve model: use model_hint if provided, else fall back to self.model_id.
        let model_id = model_hint.unwrap_or(&self.model_id);
        let registry = ModelRegistry::new();
        let client_registry = match registry.create_runtime_client_registry_for_model(model_id) {
            Ok(cr) => cr,
            Err(e) => {
                return LlmCallResult {
                    output: String::new(),
                    success: false,
                    error: Some(format!("model registry error for '{model_id}': {e}")),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                };
            }
        };
        let collector = new_collector("DagLlmCall");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            B.DagLlmCall
                .with_client_registry(&client_registry)
                .with_collector(&collector)
                .call(prompt, system_prompt),
        )
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        match result {
            Ok(Ok(output)) => LlmCallResult {
                output,
                success: true,
                error: None,
                elapsed_ms,
            },
            Ok(Err(e)) => LlmCallResult {
                output: String::new(),
                success: false,
                error: Some(format!("DagLlmCall error: {e}")),
                elapsed_ms,
            },
            Err(_) => LlmCallResult {
                output: String::new(),
                success: false,
                error: Some("DagLlmCall timed out".to_string()),
                elapsed_ms,
            },
        }
    }

    async fn emit_message(&self, message: &str) {
        let _ = self.event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "harness.emit".to_string(),
                payload: serde_json::json!({
                    "run_id": self.run_id,
                    "actor_id": self.actor_id,
                    "message": message,
                }),
                actor_id: self.actor_id.clone(),
                user_id: "system".to_string(),
            },
        });
    }

    async fn write_checkpoint(&self, checkpoint: &HarnessCheckpoint) {
        let payload = match serde_json::to_value(checkpoint) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to serialize HarnessCheckpoint: {e}");
                return;
            }
        };
        let _ = self.event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "harness.checkpoint".to_string(),
                payload,
                actor_id: self.actor_id.clone(),
                user_id: "system".to_string(),
            },
        });
        info!(
            "write_checkpoint: run:{} turn:{} pending:{}",
            checkpoint.run_id,
            checkpoint.turn_number,
            checkpoint.pending_replies.len()
        );
    }

    async fn spawn_actor_harness(&self, objective: &str, context: serde_json::Value, corr_id: &str) {
        info!("ActorAlmPort: spawning subharness corr:{corr_id}");
        let args = ActorHarnessArguments {
            event_store: self.event_store.clone(),
        };
        let spawn_result =
            Actor::spawn(Some(format!("subharness-{corr_id}")), ActorHarnessActor, args).await;

        match spawn_result {
            Ok((actor_ref, _)) => {
                let msg = ActorHarnessMsg::Execute {
                    objective: objective.to_string(),
                    context,
                    correlation_id: corr_id.to_string(),
                    reply_to: self.conductor.clone(),
                };
                if let Err(e) = actor_ref.send_message(msg) {
                    error!("Failed to send ActorHarnessMsg::Execute corr:{corr_id}: {e}");
                }
            }
            Err(e) => {
                error!("Failed to spawn ActorHarnessActor corr:{corr_id}: {e}");
            }
        }
    }
}
