//! SubharnessAdapter — WorkerPort implementation for SubharnessActor.
//!
//! Provides a scoped execution context with full tool access:
//! - Tools: bash, web_search, fetch_url, file_read, file_write, file_edit, message_parent
//! - `bash` runs sandboxed commands via `tokio::process::Command`
//! - `message_parent` sends structured progress reports to the Conductor
//! - `message_writer` is rewritten as `message_parent` — subharness reports to
//!   its orchestrator, not to a run document
//! - Context bundle injected into system prompt
//! - Progress emitted as events to EventStore

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ractor::ActorRef;

use crate::actors::agent_harness::{
    AgentProgress, ExecutionContext, HarnessError, ToolExecution, WorkerPort,
};
use crate::actors::conductor::protocol::ConductorMsg;
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::researcher::providers;
use crate::baml_client::types::{
    Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall
        as AgentToolCall,
};
use crate::actors::researcher::{
    ResearcherFetchUrlRequest, ResearcherWebSearchRequest,
};

/// Sandbox root for file operations.
///
/// Uses `CHOIROS_DATA_DIR` when set (container/CI/prod), falls back to the
/// compile-time `CARGO_MANIFEST_DIR` for local dev.
fn sandbox_root() -> PathBuf {
    if let Ok(data_dir) = std::env::var("CHOIROS_DATA_DIR") {
        if !data_dir.is_empty() {
            return PathBuf::from(data_dir);
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn validate_sandbox_path(user_path: &str) -> Result<PathBuf, String> {
    if user_path.starts_with('/') || user_path.starts_with('\\') || user_path.contains(':') {
        return Err("Absolute paths not allowed".to_string());
    }
    if user_path.contains("..") {
        return Err("Path traversal not allowed".to_string());
    }
    let sandbox = sandbox_root();
    let full_path = sandbox.join(user_path);
    let canonical = full_path.canonicalize().unwrap_or(full_path.clone());
    let sandbox_canonical = sandbox.canonicalize().unwrap_or(sandbox.clone());
    if !canonical.starts_with(&sandbox_canonical) {
        return Err("Path escapes sandbox".to_string());
    }
    Ok(full_path)
}

pub struct SubharnessAdapter {
    event_store: ActorRef<EventStoreMsg>,
    conductor: ActorRef<ConductorMsg>,
    correlation_id: String,
    context: serde_json::Value,
    http_client: reqwest::Client,
    working_dir: PathBuf,
    shell: String,
}

impl SubharnessAdapter {
    pub fn new(
        event_store: ActorRef<EventStoreMsg>,
        conductor: ActorRef<ConductorMsg>,
        correlation_id: String,
        context: serde_json::Value,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        let working_dir = sandbox_root();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        Self {
            event_store,
            conductor,
            correlation_id,
            context,
            http_client,
            working_dir,
            shell,
        }
    }

    fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let _ = self.event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: event_type.to_string(),
                payload,
                actor_id: format!("subharness:{}", self.correlation_id),
                user_id: "system".to_string(),
            },
        });
    }

    fn tool_name(tool_call: &AgentToolCall) -> &str {
        match tool_call {
            AgentToolCall::BashToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::WebSearchToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FetchUrlToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileReadToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileWriteToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileEditToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::MessageWriterToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FinishedToolCall(c) => c.tool_name.as_str(),
        }
    }

    /// Execute a sandboxed bash command via `tokio::process::Command`.
    async fn execute_bash(
        &self,
        command: &str,
        timeout_ms: u64,
    ) -> Result<(String, i32), String> {
        let output = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::process::Command::new(&self.shell)
                .arg("-lc")
                .arg(command)
                .current_dir(&self.working_dir)
                .output(),
        )
        .await
        .map_err(|_| format!("bash timed out after {}ms", timeout_ms))?
        .map_err(|e| format!("bash exec failed: {e}"))?;

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

    /// Send a progress report to the parent conductor via `message_writer` tool.
    ///
    /// The BAML schema uses `MessageWriterToolCall` — we reinterpret its fields
    /// for parent messaging:
    /// - `mode`: the report kind ("progress", "status", "finding")
    /// - `content`: the progress message body
    /// - `path`: optional structured data key
    /// - `mode_arg`: optional structured data value
    fn send_progress_to_conductor(
        &self,
        mode: &str,
        content: &str,
        path: Option<&str>,
        mode_arg: Option<&str>,
    ) {
        let payload = serde_json::json!({
            "kind": mode,
            "content": content,
            "key": path,
            "value": mode_arg,
        });

        // Fire-and-forget to conductor
        let _ = self.conductor.send_message(ConductorMsg::SubharnessProgress {
            correlation_id: self.correlation_id.clone(),
            kind: mode.to_string(),
            content: content.to_string(),
            metadata: payload,
        });

        // Also persist to event store for observability
        self.emit_event(
            "subharness.parent_message",
            serde_json::json!({
                "correlation_id": self.correlation_id,
                "kind": mode,
                "content": content,
                "key": path,
                "value": mode_arg,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );
    }
}

#[async_trait]
impl WorkerPort for SubharnessAdapter {
    fn get_model_role(&self) -> &str {
        "subharness"
    }

    fn get_tool_description(&self) -> String {
        r#"Available tools:

1. bash - Execute shell commands in the sandbox working directory
   Args:
   - command: string (required) - The shell command to execute

2. web_search - Search the web for information
   Args:
   - query: string (required) - The search query

3. fetch_url - Fetch and extract content from a URL
   Args:
   - path: string (required) - The URL to fetch (http:// or https://)

4. file_read - Read a local file within the sandbox
   Args:
   - path: string (required) - Relative path from sandbox root

5. file_write - Write or overwrite a file
   Args:
   - path: string (required) - Relative path from sandbox root
   - content: string (required) - Full content to write

6. file_edit - Edit specific text in an existing file
   Args:
   - path: string (required) - Relative path from sandbox root
   - old_text: string (required) - Text to find and replace
   - new_text: string (required) - Replacement text

7. message_writer - Report progress to the parent Conductor
   Use this to keep your orchestrator informed of intermediate findings,
   status changes, or important decisions. The conductor receives these
   in real time and may use them for run-level reporting.
   Args:
   - mode: string (required) - Report kind: "progress", "status", or "finding"
   - content: string (required) - The progress report body
   - path: string (optional) - Structured data key
   - mode_arg: string (optional) - Structured data value
"#
        .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        let context_summary = if self.context.is_null() || self.context == serde_json::Value::Object(serde_json::Map::new()) {
            String::new()
        } else {
            format!(
                "\n\nContext bundle provided by conductor:\n```json\n{}\n```",
                serde_json::to_string_pretty(&self.context).unwrap_or_default()
            )
        };

        format!(
            r#"You are a focused sub-agent executing a scoped objective.
Your output will be returned directly to the Conductor orchestrator.

Objective: {objective}
Correlation ID: {correlation_id}
{context_summary}

Guidelines:
- Execute the objective efficiently within a bounded step budget.
- Use bash to run shell commands (build, test, inspect, transform).
- Use web_search and fetch_url for information retrieval.
- Use file_read for reading local context files.
- Use file_write / file_edit for producing file-based outputs.
- Use message_writer to report significant progress, findings, or status
  changes to the conductor as you work. This keeps the orchestrator informed.
- When done, call `finished` with a concise, structured summary.
- Do not over-produce: the conductor needs clean, actionable output.
"#,
            objective = ctx.objective,
            correlation_id = self.correlation_id,
            context_summary = context_summary,
        )
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start = tokio::time::Instant::now();

        match tool_call {
            AgentToolCall::BashToolCall(call) => {
                let command = call.tool_args.command.as_str();
                let timeout_ms: u64 = 30_000;

                self.emit_event(
                    "subharness.bash_call",
                    serde_json::json!({
                        "correlation_id": self.correlation_id,
                        "command": command,
                        "step": ctx.step_number,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }),
                );

                match self.execute_bash(command, timeout_ms).await {
                    Ok((output, exit_code)) => Ok(ToolExecution {
                        tool_name: "bash".to_string(),
                        success: exit_code == 0,
                        output,
                        error: if exit_code == 0 {
                            None
                        } else {
                            Some(format!("exit status {exit_code}"))
                        },
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                    Err(e) => Ok(ToolExecution {
                        tool_name: "bash".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(e),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::WebSearchToolCall(call) => {
                let query = call.tool_args.query.clone();
                let request = ResearcherWebSearchRequest {
                    query: query.clone(),
                    objective: Some(ctx.objective.clone()),
                    provider: None,
                    max_results: None,
                    max_rounds: Some(1),
                    time_range: None,
                    include_domains: None,
                    exclude_domains: None,
                    timeout_ms: Some(30_000),
                    model_override: None,
                    reasoning: call.reasoning.clone(),
                };
                let selection = providers::parse_provider_selection(
                    request.provider.as_deref().or(Some("auto")),
                );
                let (outputs, calls, errors) = providers::run_provider_selection(
                    &self.http_client,
                    selection,
                    &query,
                    6,
                    None,
                    None,
                    None,
                )
                .await;
                let citations = providers::merge_citations(&outputs);
                let success = !citations.is_empty();
                let output = serde_json::json!({
                    "citations": citations,
                    "provider_calls": calls,
                    "errors": errors,
                });
                Ok(ToolExecution {
                    tool_name: call.tool_name.clone(),
                    success,
                    output: output.to_string(),
                    error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
            }
            AgentToolCall::FetchUrlToolCall(call) => {
                let url = call.tool_args.path.trim().to_string();
                if url.is_empty() {
                    return Err(HarnessError::ToolExecution(
                        "fetch_url: path cannot be empty".to_string(),
                    ));
                }
                let request = ResearcherFetchUrlRequest {
                    url: url.clone(),
                    timeout_ms: Some(30_000),
                    max_chars: None,
                };
                match providers::fetch_url(&request).await {
                    Ok(result) => {
                        let output = serde_json::json!({
                            "url": result.url,
                            "status_code": result.status_code,
                            "content_excerpt": result.content_excerpt,
                        });
                        Ok(ToolExecution {
                            tool_name: call.tool_name.clone(),
                            success: result.success,
                            output: output.to_string(),
                            error: None,
                            execution_time_ms: start.elapsed().as_millis() as u64,
                        })
                    }
                    Err(e) => Ok(ToolExecution {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(e.to_string()),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::FileReadToolCall(call) => {
                let path = call.tool_args.path.as_str();
                match validate_sandbox_path(path) {
                    Ok(full_path) => match tokio::fs::read_to_string(&full_path).await {
                        Ok(content) => {
                            let output = serde_json::json!({"path": path, "content": content});
                            Ok(ToolExecution {
                                tool_name: call.tool_name.clone(),
                                success: true,
                                output: output.to_string(),
                                error: None,
                                execution_time_ms: start.elapsed().as_millis() as u64,
                            })
                        }
                        Err(e) => Ok(ToolExecution {
                            tool_name: call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(format!("read failed: {e}")),
                            execution_time_ms: start.elapsed().as_millis() as u64,
                        }),
                    },
                    Err(e) => Ok(ToolExecution {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("invalid path: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::FileWriteToolCall(call) => {
                let path = call.tool_args.path.as_str();
                let content = call.tool_args.content.as_str();
                match validate_sandbox_path(path) {
                    Ok(full_path) => {
                        if let Some(parent) = full_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        match tokio::fs::write(&full_path, content).await {
                            Ok(_) => {
                                let output = serde_json::json!({"path": path, "size": content.len()});
                                Ok(ToolExecution {
                                    tool_name: call.tool_name.clone(),
                                    success: true,
                                    output: output.to_string(),
                                    error: None,
                                    execution_time_ms: start.elapsed().as_millis() as u64,
                                })
                            }
                            Err(e) => Ok(ToolExecution {
                                tool_name: call.tool_name.clone(),
                                success: false,
                                output: String::new(),
                                error: Some(format!("write failed: {e}")),
                                execution_time_ms: start.elapsed().as_millis() as u64,
                            }),
                        }
                    }
                    Err(e) => Ok(ToolExecution {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("invalid path: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::FileEditToolCall(call) => {
                let path = call.tool_args.path.as_str();
                let old_text = call.tool_args.old_text.as_str();
                let new_text = call.tool_args.new_text.as_str();
                match validate_sandbox_path(path) {
                    Ok(full_path) => match tokio::fs::read_to_string(&full_path).await {
                        Ok(content) => {
                            let new_content = content.replace(old_text, new_text);
                            if new_content == content {
                                return Ok(ToolExecution {
                                    tool_name: call.tool_name.clone(),
                                    success: false,
                                    output: String::new(),
                                    error: Some("old_text not found in file".to_string()),
                                    execution_time_ms: start.elapsed().as_millis() as u64,
                                });
                            }
                            match tokio::fs::write(&full_path, &new_content).await {
                                Ok(_) => {
                                    let output = serde_json::json!({"path": path});
                                    Ok(ToolExecution {
                                        tool_name: call.tool_name.clone(),
                                        success: true,
                                        output: output.to_string(),
                                        error: None,
                                        execution_time_ms: start.elapsed().as_millis() as u64,
                                    })
                                }
                                Err(e) => Ok(ToolExecution {
                                    tool_name: call.tool_name.clone(),
                                    success: false,
                                    output: String::new(),
                                    error: Some(format!("write failed: {e}")),
                                    execution_time_ms: start.elapsed().as_millis() as u64,
                                }),
                            }
                        }
                        Err(e) => Ok(ToolExecution {
                            tool_name: call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(format!("read failed: {e}")),
                            execution_time_ms: start.elapsed().as_millis() as u64,
                        }),
                    },
                    Err(e) => Ok(ToolExecution {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("invalid path: {e}")),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    }),
                }
            }
            AgentToolCall::MessageWriterToolCall(call) => {
                // Reinterpret message_writer as parent-reporting.
                // mode → report kind, content → message body.
                let mode = call.tool_args.mode.trim();
                let content = call.tool_args.content.trim();

                if content.is_empty() {
                    return Ok(ToolExecution {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("content cannot be empty".to_string()),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    });
                }

                self.send_progress_to_conductor(
                    mode,
                    content,
                    call.tool_args.path.as_deref(),
                    call.tool_args.mode_arg.as_deref(),
                );

                Ok(ToolExecution {
                    tool_name: call.tool_name.clone(),
                    success: true,
                    output: serde_json::json!({
                        "delivered": true,
                        "kind": mode,
                    })
                    .to_string(),
                    error: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
            }
            _ => Err(HarnessError::ToolExecution(format!(
                "Unknown tool: {}",
                Self::tool_name(tool_call)
            ))),
        }
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        _report: shared_types::WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        self.emit_event(
            "subharness.worker_report",
            serde_json::json!({
                "correlation_id": self.correlation_id,
                "loop_id": ctx.loop_id,
                "steps": ctx.step_number,
                "timestamp": chrono::Utc::now().to_rfc3339(),
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
            "subharness.progress",
            serde_json::json!({
                "correlation_id": self.correlation_id,
                "loop_id": ctx.loop_id,
                "phase": progress.phase,
                "message": progress.message,
                "step_index": progress.step_index,
                "timestamp": progress.timestamp,
            }),
        );
        Ok(())
    }
}
