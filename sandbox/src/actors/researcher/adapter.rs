//! ResearcherAdapter - AgentAdapter implementation for ResearcherActor
//!
//! This adapter bridges the ResearcherActor to the unified agent harness,
//! providing researcher-specific tool execution and event emission.
//!
//! Tools available:
//! - web_search: Search the web
//! - fetch_url: Fetch specific URLs
//! - file_read: Read local files
//! - file_write: Write/create files
//! - file_edit: Edit existing files
//!
//! Writer-First Integration (Phase D):
//! - When run_writer_actor is set, writes to run document paths are delegated
//! - Run document path pattern: conductor/runs/{run_id}/draft.md
//! - Workers send typed patches via RunWriterActor instead of direct writes

use async_trait::async_trait;
use ractor::ActorRef;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::actors::agent_harness::{
    AgentAdapter, AgentProgress, ExecutionContext, HarnessError, ToolExecution,
};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::run_writer::{PatchOp, PatchOpKind, RunWriterMsg};
use crate::baml_client::types::AgentToolCall;

use super::{
    providers, ResearcherFetchUrlRequest, ResearcherProgress, ResearcherState,
    ResearcherWebSearchRequest,
};

/// Sandbox root for file operations
fn sandbox_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Validate path is within sandbox
fn validate_sandbox_path(user_path: &str) -> Result<PathBuf, String> {
    // Reject absolute paths
    if user_path.starts_with('/') || user_path.starts_with('\\') || user_path.contains(':') {
        return Err("Absolute paths not allowed".to_string());
    }

    // Reject path traversal
    if user_path.contains("..") {
        return Err("Path traversal not allowed".to_string());
    }

    let sandbox = sandbox_root();
    let full_path = sandbox.join(user_path);

    // Ensure it's still within sandbox
    let canonical = full_path.canonicalize().unwrap_or(full_path.clone());
    let sandbox_canonical = sandbox.canonicalize().unwrap_or(sandbox.clone());

    if !canonical.starts_with(&sandbox_canonical) {
        return Err("Path escapes sandbox".to_string());
    }

    Ok(full_path)
}

const RUN_DOC_PATTERN: &str = "conductor/runs/";

fn is_run_document_path(path: &str) -> bool {
    path.starts_with(RUN_DOC_PATTERN) && path.ends_with("/draft.md")
}

/// Adapter that connects ResearcherActor to the unified agent harness
pub struct ResearcherAdapter {
    state: ResearcherState,
    progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
    http_client: reqwest::Client,
    run_writer_actor: Option<ActorRef<RunWriterMsg>>,
    run_id: Option<String>,
}

impl ResearcherAdapter {
    pub fn new(
        state: ResearcherState,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        timeout_ms: u64,
    ) -> Result<Self, HarnessError> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| HarnessError::Adapter(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            state,
            progress_tx,
            http_client,
            run_writer_actor: None,
            run_id: None,
        })
    }

    pub fn with_run_writer(
        mut self,
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
    ) -> Self {
        self.run_writer_actor = Some(run_writer_actor);
        self.run_id = Some(run_id);
        self
    }

    /// Get access to the model registry for provider selection
    pub fn model_registry(&self) -> &ModelRegistry {
        &self.state.model_registry
    }

    /// Get the current model preference
    pub fn current_model(&self) -> &str {
        &self.state.current_model
    }

    /// Convert harness progress to ResearcherProgress and emit
    fn to_researcher_progress(&self, progress: &AgentProgress) -> ResearcherProgress {
        ResearcherProgress {
            phase: progress.phase.clone(),
            message: progress.message.clone(),
            provider: None,
            model_used: progress.model_used.clone(),
            result_count: progress.step_index,
            timestamp: progress.timestamp.clone(),
        }
    }

    /// Emit event to EventStore
    fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.state.researcher_id.clone(),
            user_id: self.state.user_id.clone(),
        };
        let _ = self
            .state
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    /// Emit document update event for live streaming
    fn emit_document_update(&self, task_id: &str, path: &str, content_excerpt: &str) {
        let payload = serde_json::json!({
            "task_id": task_id,
            "worker_id": self.state.researcher_id,
            "phase": "document_update",
            "path": path,
            "content_excerpt": content_excerpt.chars().take(500).collect::<String>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.document_update", payload);
    }

    async fn send_patch_to_run_writer(
        &self,
        section_id: &str,
        content: &str,
        proposal: bool,
    ) -> Result<(), String> {
        use ractor::call;

        let (run_writer, run_id) = match (&self.run_writer_actor, &self.run_id) {
            (Some(rw), Some(rid)) => (rw, rid),
            _ => return Err("RunWriterActor not configured".to_string()),
        };

        let ops = vec![PatchOp {
            kind: PatchOpKind::Append,
            position: None,
            text: Some(content.to_string()),
        }];

        let result = call!(run_writer, |reply| RunWriterMsg::ApplyPatch {
            run_id: run_id.clone(),
            source: "researcher".to_string(),
            section_id: section_id.to_string(),
            ops,
            proposal,
            reply,
        })
        .map_err(|e| format!("RunWriterActor call failed: {e}"))?;

        result.map(|_| ()).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl AgentAdapter for ResearcherAdapter {
    fn get_model_role(&self) -> &str {
        "researcher"
    }

    fn get_tool_description(&self) -> String {
        r#"Available tools for research:

1. web_search - Search the web for information
   Args:
   - query: string (required) - The search query
   - provider: string (optional) - Provider: "tavily", "brave", "exa", "auto"
   - max_results: number (optional) - Max results (1-20, default: 6)
   - time_range: string (optional) - Filter: "day", "week", "month", "year"
   - include_domains: string[] (optional) - Domains to include
   - exclude_domains: string[] (optional) - Domains to exclude

2. fetch_url - Fetch and extract content from a URL
   Args:
   - url: string (required) - The URL to fetch
   - max_chars: number (optional) - Max chars to extract (default: 8000)

3. file_read - Read a local file within the sandbox
   Args:
   - path: string (required) - Relative path from sandbox root

4. file_write - Write or overwrite a file
   Args:
   - path: string (required) - Relative path from sandbox root
   - content: string (required) - Full content to write

5. file_edit - Edit specific text in an existing file
   Args:
   - path: string (required) - Relative path from sandbox root
   - old_text: string (required) - Text to find and replace
   - new_text: string (required) - Replacement text
"#
        .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            r#"You are a research agent. Your goal is to gather information and maintain a working draft document.

Current step: {}/{}
Model: {}
Objective: {}

Guidelines:
- Capability boundary:
  - You are the external research capability.
  - Handle web information gathering, citation, and synthesis.
  - Do not attempt shell orchestration or terminal-style execution planning.
- Use web_search to find relevant information online
- Use fetch_url to retrieve detailed content from specific URLs
- Use file_read to reference existing documents, code, or previous research
- Use file_write to create your working draft (overwrites existing)
- Use file_edit to refine specific sections without rewriting everything
- Maintain your working draft - it should evolve as you learn
- Write findings immediately - don't wait until the end
- Cite sources inline as markdown links: [title](url)
- Put the most important finding first (don't bury the lede)
- Use freeform markdown - no forced structure
"#,
            ctx.step_number, ctx.max_steps, ctx.model_used, ctx.objective
        )
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start_time = tokio::time::Instant::now();

        match tool_call.tool_name.as_str() {
            "web_search" => {
                let args = &tool_call.tool_args;
                let query = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.query.clone())
                    .or_else(|| args.query.clone())
                    .ok_or_else(|| {
                        HarnessError::ToolExecution("Missing query argument".to_string())
                    })?;

                let provider = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.provider.clone())
                    .or_else(|| args.provider.clone());
                let max_results = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.max_results)
                    .or(args.max_results)
                    .map(|v| v as u32);
                let time_range = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.time_range.clone())
                    .or_else(|| args.time_range.clone());
                let include_domains = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.include_domains.clone())
                    .or_else(|| args.include_domains.clone());
                let exclude_domains = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.exclude_domains.clone())
                    .or_else(|| args.exclude_domains.clone());

                let request = ResearcherWebSearchRequest {
                    query: query.clone(),
                    objective: Some(ctx.objective.clone()),
                    provider,
                    max_results,
                    max_rounds: Some(1),
                    time_range,
                    include_domains,
                    exclude_domains,
                    timeout_ms: Some(30_000),
                    model_override: None,
                    reasoning: tool_call.reasoning.clone(),
                };

                let provider_str = request.provider.as_deref().unwrap_or("auto");
                let selection = providers::parse_provider_selection(Some(provider_str));

                // Emit progress
                if let Some(tx) = &self.progress_tx {
                    let _ = tx.send(ResearcherProgress {
                        phase: "web_search".to_string(),
                        message: format!("Searching for: {}", query),
                        provider: Some(provider_str.to_string()),
                        model_used: Some(ctx.model_used.clone()),
                        result_count: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                let max_results = request.max_results.unwrap_or(6).clamp(1, 20);
                let (outputs, calls, errors) = providers::run_provider_selection(
                    &self.http_client,
                    selection,
                    &query,
                    max_results,
                    request.time_range.as_deref(),
                    request.include_domains.as_deref(),
                    request.exclude_domains.as_deref(),
                )
                .await;

                let elapsed = start_time.elapsed().as_millis() as u64;
                let citations = providers::merge_citations(&outputs);
                let success = !citations.is_empty();

                let output = serde_json::json!({
                    "citations": citations,
                    "provider_calls": calls,
                    "errors": errors,
                });

                Ok(ToolExecution {
                    tool_name: tool_call.tool_name.clone(),
                    success,
                    output: output.to_string(),
                    error: if errors.is_empty() {
                        None
                    } else {
                        Some(errors.join("; "))
                    },
                    execution_time_ms: elapsed,
                })
            }

            "fetch_url" => {
                let args = &tool_call.tool_args;
                let url = args.path.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing url argument".to_string())
                })?;

                let request = ResearcherFetchUrlRequest {
                    url: url.clone(),
                    timeout_ms: Some(30_000),
                    max_chars: Some(8000),
                };

                if let Some(tx) = &self.progress_tx {
                    let _ = tx.send(ResearcherProgress {
                        phase: "fetch_url".to_string(),
                        message: format!("Fetching: {}", url),
                        provider: None,
                        model_used: Some(ctx.model_used.clone()),
                        result_count: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                match providers::fetch_url(&request).await {
                    Ok(result) => {
                        let elapsed = start_time.elapsed().as_millis() as u64;

                        let output = serde_json::json!({
                            "url": result.url,
                            "final_url": result.final_url,
                            "status_code": result.status_code,
                            "content_type": result.content_type,
                            "content_excerpt": result.content_excerpt,
                            "content_length": result.content_length,
                        });

                        Ok(ToolExecution {
                            tool_name: tool_call.tool_name.clone(),
                            success: result.success,
                            output: output.to_string(),
                            error: None,
                            execution_time_ms: elapsed,
                        })
                    }
                    Err(e) => {
                        let elapsed = start_time.elapsed().as_millis() as u64;
                        Ok(ToolExecution {
                            tool_name: tool_call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(e.to_string()),
                            execution_time_ms: elapsed,
                        })
                    }
                }
            }

            "file_read" => {
                let args = &tool_call.tool_args;
                let path = args.path.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing path argument".to_string())
                })?;

                if let Some(tx) = &self.progress_tx {
                    let _ = tx.send(ResearcherProgress {
                        phase: "file_read".to_string(),
                        message: format!("Reading file: {}", path),
                        provider: None,
                        model_used: Some(ctx.model_used.clone()),
                        result_count: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                match validate_sandbox_path(path) {
                    Ok(full_path) => match tokio::fs::read_to_string(&full_path).await {
                        Ok(content) => {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            let output = serde_json::json!({
                                "path": path,
                                "content": content,
                                "size": content.len(),
                            });

                            Ok(ToolExecution {
                                tool_name: tool_call.tool_name.clone(),
                                success: true,
                                output: output.to_string(),
                                error: None,
                                execution_time_ms: elapsed,
                            })
                        }
                        Err(e) => {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            Ok(ToolExecution {
                                tool_name: tool_call.tool_name.clone(),
                                success: false,
                                output: String::new(),
                                error: Some(format!("Failed to read file: {}", e)),
                                execution_time_ms: elapsed,
                            })
                        }
                    },
                    Err(e) => {
                        let elapsed = start_time.elapsed().as_millis() as u64;
                        Ok(ToolExecution {
                            tool_name: tool_call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(format!("Invalid path: {}", e)),
                            execution_time_ms: elapsed,
                        })
                    }
                }
            }

            "file_write" => {
                let args = &tool_call.tool_args;
                let path = args.path.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing path argument".to_string())
                })?;
                let content = args.content.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing content argument".to_string())
                })?;

                if let Some(tx) = &self.progress_tx {
                    let _ = tx.send(ResearcherProgress {
                        phase: "file_write".to_string(),
                        message: format!("Writing file: {} ({} chars)", path, content.len()),
                        provider: None,
                        model_used: Some(ctx.model_used.clone()),
                        result_count: Some(content.len()),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                if is_run_document_path(path) {
                    if let Some(_run_writer) = &self.run_writer_actor {
                        match self
                            .send_patch_to_run_writer("researcher", content, true)
                            .await
                        {
                            Ok(_) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                let output = serde_json::json!({
                                    "path": path,
                                    "size": content.len(),
                                    "via_run_writer": true,
                                });

                                self.emit_document_update(&ctx.loop_id, path, content);

                                Ok(ToolExecution {
                                    tool_name: tool_call.tool_name.clone(),
                                    success: true,
                                    output: output.to_string(),
                                    error: None,
                                    execution_time_ms: elapsed,
                                })
                            }
                            Err(e) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                Ok(ToolExecution {
                                    tool_name: tool_call.tool_name.clone(),
                                    success: false,
                                    output: String::new(),
                                    error: Some(format!("RunWriterActor patch failed: {}", e)),
                                    execution_time_ms: elapsed,
                                })
                            }
                        }
                    } else {
                        let elapsed = start_time.elapsed().as_millis() as u64;
                        Ok(ToolExecution {
                            tool_name: tool_call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(
                                "Run document writes must go through RunWriterActor".to_string(),
                            ),
                            execution_time_ms: elapsed,
                        })
                    }
                } else {
                    match validate_sandbox_path(path) {
                        Ok(full_path) => {
                            if let Some(parent) = full_path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }

                            match tokio::fs::write(&full_path, content).await {
                                Ok(_) => {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    let output = serde_json::json!({
                                        "path": path,
                                        "size": content.len(),
                                    });

                                    self.emit_document_update(&ctx.loop_id, path, content);

                                    Ok(ToolExecution {
                                        tool_name: tool_call.tool_name.clone(),
                                        success: true,
                                        output: output.to_string(),
                                        error: None,
                                        execution_time_ms: elapsed,
                                    })
                                }
                                Err(e) => {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    Ok(ToolExecution {
                                        tool_name: tool_call.tool_name.clone(),
                                        success: false,
                                        output: String::new(),
                                        error: Some(format!("Failed to write file: {}", e)),
                                        execution_time_ms: elapsed,
                                    })
                                }
                            }
                        }
                        Err(e) => {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            Ok(ToolExecution {
                                tool_name: tool_call.tool_name.clone(),
                                success: false,
                                output: String::new(),
                                error: Some(format!("Invalid path: {}", e)),
                                execution_time_ms: elapsed,
                            })
                        }
                    }
                }
            }

            "file_edit" => {
                let args = &tool_call.tool_args;
                let path = args.path.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing path argument".to_string())
                })?;
                let old_text = args.old_text.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing old_text argument".to_string())
                })?;
                let new_text = args.new_text.as_ref().ok_or_else(|| {
                    HarnessError::ToolExecution("Missing new_text argument".to_string())
                })?;

                if let Some(tx) = &self.progress_tx {
                    let _ = tx.send(ResearcherProgress {
                        phase: "file_edit".to_string(),
                        message: format!("Editing file: {}", path),
                        provider: None,
                        model_used: Some(ctx.model_used.clone()),
                        result_count: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                if is_run_document_path(path) {
                    if self.run_writer_actor.is_some() {
                        let edit_content =
                            format!("\n[EDIT] Replace:\n{}\n\nWith:\n{}\n", old_text, new_text);
                        match self
                            .send_patch_to_run_writer("researcher", &edit_content, true)
                            .await
                        {
                            Ok(_) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                let output = serde_json::json!({
                                    "path": path,
                                    "via_run_writer": true,
                                });

                                Ok(ToolExecution {
                                    tool_name: tool_call.tool_name.clone(),
                                    success: true,
                                    output: output.to_string(),
                                    error: None,
                                    execution_time_ms: elapsed,
                                })
                            }
                            Err(e) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                Ok(ToolExecution {
                                    tool_name: tool_call.tool_name.clone(),
                                    success: false,
                                    output: String::new(),
                                    error: Some(format!("RunWriterActor patch failed: {}", e)),
                                    execution_time_ms: elapsed,
                                })
                            }
                        }
                    } else {
                        let elapsed = start_time.elapsed().as_millis() as u64;
                        Ok(ToolExecution {
                            tool_name: tool_call.tool_name.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(
                                "Run document edits must go through RunWriterActor".to_string(),
                            ),
                            execution_time_ms: elapsed,
                        })
                    }
                } else {
                    match validate_sandbox_path(path) {
                        Ok(full_path) => match tokio::fs::read_to_string(&full_path).await {
                            Ok(content) => {
                                let new_content = content.replace(old_text, new_text);

                                if new_content == content {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    return Ok(ToolExecution {
                                        tool_name: tool_call.tool_name.clone(),
                                        success: false,
                                        output: String::new(),
                                        error: Some("old_text not found in file".to_string()),
                                        execution_time_ms: elapsed,
                                    });
                                }

                                match tokio::fs::write(&full_path, &new_content).await {
                                    Ok(_) => {
                                        let elapsed = start_time.elapsed().as_millis() as u64;
                                        let output = serde_json::json!({
                                            "path": path,
                                            "old_size": content.len(),
                                            "new_size": new_content.len(),
                                        });

                                        self.emit_document_update(&ctx.loop_id, path, &new_content);

                                        Ok(ToolExecution {
                                            tool_name: tool_call.tool_name.clone(),
                                            success: true,
                                            output: output.to_string(),
                                            error: None,
                                            execution_time_ms: elapsed,
                                        })
                                    }
                                    Err(e) => {
                                        let elapsed = start_time.elapsed().as_millis() as u64;
                                        Ok(ToolExecution {
                                            tool_name: tool_call.tool_name.clone(),
                                            success: false,
                                            output: String::new(),
                                            error: Some(format!("Failed to write file: {}", e)),
                                            execution_time_ms: elapsed,
                                        })
                                    }
                                }
                            }
                            Err(e) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                Ok(ToolExecution {
                                    tool_name: tool_call.tool_name.clone(),
                                    success: false,
                                    output: String::new(),
                                    error: Some(format!("Failed to read file: {}", e)),
                                    execution_time_ms: elapsed,
                                })
                            }
                        },
                        Err(e) => {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            Ok(ToolExecution {
                                tool_name: tool_call.tool_name.clone(),
                                success: false,
                                output: String::new(),
                                error: Some(format!("Invalid path: {}", e)),
                                execution_time_ms: elapsed,
                            })
                        }
                    }
                }
            }

            _ => Err(HarnessError::ToolExecution(format!(
                "Unknown tool: {}",
                tool_call.tool_name
            ))),
        }
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        // Researcher executes all tools locally
        false
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: shared_types::WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "report": report,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.report.received", payload);

        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(ResearcherProgress {
                phase: "worker_report".to_string(),
                message: format!("Research complete with {} findings", report.findings.len()),
                provider: None,
                model_used: Some(ctx.model_used.clone()),
                result_count: Some(report.findings.len()),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }

        Ok(())
    }

    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        if let Some(tx) = &self.progress_tx {
            let researcher_progress = self.to_researcher_progress(&progress);
            let _ = tx.send(researcher_progress);
        }

        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "phase": progress.phase,
            "message": progress.message,
            "model_used": progress.model_used,
            "timestamp": progress.timestamp,
        });
        self.emit_event("worker.task.progress", payload);

        Ok(())
    }
}
