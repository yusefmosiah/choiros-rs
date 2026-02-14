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
    AgentProgress, ExecutionContext, HarnessError, ToolExecution, WorkerPort,
};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::run_writer::{RunWriterMsg, SectionState};
use crate::actors::writer::{WriterMsg, WriterSource};
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
    writer_actor: Option<ActorRef<WriterMsg>>,
    run_writer_actor: Option<ActorRef<RunWriterMsg>>,
    run_id: Option<String>,
}

impl ResearcherAdapter {
    fn run_document_path(&self) -> Option<String> {
        self.run_id
            .as_ref()
            .map(|run_id| format!("conductor/runs/{run_id}/draft.md"))
    }

    fn has_run_writer(&self) -> bool {
        self.run_writer_actor.is_some() && self.run_id.is_some()
    }

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
            writer_actor: None,
            run_writer_actor: None,
            run_id: None,
        })
    }

    pub fn with_writer_actor(mut self, writer_actor: ActorRef<WriterMsg>) -> Self {
        self.writer_actor = Some(writer_actor);
        self
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

    fn resolve_writer_section(section_hint: Option<&str>) -> String {
        match section_hint
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .as_deref()
        {
            Some("conductor") => "conductor".to_string(),
            Some("terminal") => "terminal".to_string(),
            Some("user") => "user".to_string(),
            _ => "researcher".to_string(),
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

    fn writer_context(&self) -> Option<(ActorRef<WriterMsg>, ActorRef<RunWriterMsg>, String)> {
        Some((
            self.writer_actor.clone()?,
            self.run_writer_actor.clone()?,
            self.run_id.clone()?,
        ))
    }

    async fn writer_set_state(&self, state: SectionState) {
        let Some((writer_actor, run_writer_actor, run_id)) = self.writer_context() else {
            return;
        };
        let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
            run_writer_actor,
            run_id,
            section_id: "researcher".to_string(),
            state,
            reply,
        });
    }

    fn terminal_decision_has_required_writer_message(
        writer_mode_active: bool,
        tool_executions: &[ToolExecution],
    ) -> bool {
        if !writer_mode_active {
            return true;
        }
        tool_executions
            .iter()
            .any(|exec| exec.tool_name == "message_writer" && exec.success)
    }

    fn resolve_fetch_url_arg(tool_call: &AgentToolCall) -> Option<String> {
        let args = &tool_call.tool_args;
        args.path
            .as_ref()
            .or(args.query.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    async fn execute_message_writer(
        &self,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start_time = tokio::time::Instant::now();
        let writer_actor = match &self.writer_actor {
            Some(actor) => actor,
            None => {
                return Ok(ToolExecution {
                    tool_name: tool_call.tool_name.clone(),
                    success: false,
                    output: String::new(),
                    error: Some("WriterActor not configured for this run".to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                });
            }
        };
        let (run_writer_actor, run_id) = match (&self.run_writer_actor, &self.run_id) {
            (Some(actor), Some(run_id)) => (actor.clone(), run_id.clone()),
            _ => {
                return Ok(ToolExecution {
                    tool_name: tool_call.tool_name.clone(),
                    success: false,
                    output: String::new(),
                    error: Some("RunWriterActor not configured for this run".to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                });
            }
        };

        let args = &tool_call.tool_args;
        let section_id = Self::resolve_writer_section(args.path.as_deref());
        let content = args.content.clone().unwrap_or_default();
        let mode = args
            .old_text
            .clone()
            .unwrap_or_else(|| "proposal_append".to_string())
            .trim()
            .to_ascii_lowercase();
        let mode_arg = args.new_text.clone();

        let result = match mode.as_str() {
            "progress" => {
                let phase = mode_arg
                    .clone()
                    .unwrap_or_else(|| "update".to_string())
                    .trim()
                    .to_string();
                if content.trim().is_empty() {
                    Err("message_writer progress mode requires content".to_string())
                } else {
                    ractor::call!(writer_actor, |reply| WriterMsg::ReportProgress {
                        run_writer_actor: run_writer_actor.clone(),
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Researcher,
                        phase,
                        message: content.clone(),
                        reply,
                    })
                    .map_err(|e| format!("WriterActor call failed: {e}"))
                    .and_then(|inner| inner.map_err(|e| e.to_string()))
                    .map(|rev| {
                        serde_json::json!({
                            "mode": "progress",
                            "section_id": section_id,
                            "revision": rev,
                        })
                    })
                }
            }
            "state" => {
                let state = Self::parse_section_state(mode_arg.as_deref()).ok_or_else(|| {
                    "message_writer state mode requires new_text in {pending|running|complete|failed}"
                        .to_string()
                });
                match state {
                    Ok(state) => ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                        run_writer_actor: run_writer_actor.clone(),
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
                    Err(e) => Err(e),
                }
            }
            "canon_append" => {
                if content.trim().is_empty() {
                    Err("message_writer canon_append mode requires content".to_string())
                } else {
                    ractor::call!(writer_actor, |reply| WriterMsg::ApplyText {
                        run_writer_actor: run_writer_actor.clone(),
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Researcher,
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
                    let message_id = format!("{run_id}:researcher:tool:{}", ulid::Ulid::new());
                    ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
                        message_id,
                        kind: "researcher_tool_update".to_string(),
                        run_writer_actor: run_writer_actor.clone(),
                        run_id: run_id.clone(),
                        section_id: section_id.clone(),
                        source: WriterSource::Researcher,
                        content: content.clone(),
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
            _ => Err(format!(
                "Unknown message_writer mode '{}'. Supported: proposal_append, canon_append, progress, state",
                mode
            )),
        };

        let elapsed = start_time.elapsed().as_millis() as u64;
        match result {
            Ok(output) => Ok(ToolExecution {
                tool_name: tool_call.tool_name.clone(),
                success: true,
                output: output.to_string(),
                error: None,
                execution_time_ms: elapsed,
            }),
            Err(error) => Ok(ToolExecution {
                tool_name: tool_call.tool_name.clone(),
                success: false,
                output: String::new(),
                error: Some(error),
                execution_time_ms: elapsed,
            }),
        }
    }
}

#[async_trait]
impl WorkerPort for ResearcherAdapter {
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
   - path: string (required) - The URL to fetch (http:// or https://)
   - query: string (optional alias) - URL if model emits query instead of path
   - max_chars: number (optional) - Max chars to extract (default: 8000)
   Example:
   - tool=fetch_url, path="https://github.com/theonlyhennygod/zeroclaw"

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

6. message_writer - Send a typed actor message to the run writer
   Args:
   - path: string (optional) - section_id: conductor|researcher|terminal|user (default: researcher)
   - content: string (required for append/progress)
   - old_text: string (optional) - mode: proposal_append|canon_append|progress|state (default: proposal_append)
   - new_text: string (optional) - mode argument:
     - progress: phase string
     - state: pending|running|complete|failed
   Required behavior in run writer mode:
   - Use message_writer with old_text=\"proposal_append\" for substantive updates
   - Publish first substantive update by step 2 at latest
   - Publish again whenever you have new findings or changed conclusions
   - Before Complete/Block, publish a final proposal_append summary
   - Keep each update concise and incremental (delta from prior update), not a full report
   - If evidence conflicts with earlier claims, explicitly mark the old claim as superseded
   Examples:
   - Initial note:
     tool=message_writer, path=\"researcher\", old_text=\"proposal_append\",
     content=\"Plan: verify repo URL, compare architecture, then benchmark/runtime differences.\"
   - Findings update:
     tool=message_writer, path=\"researcher\", old_text=\"proposal_append\",
     content=\"New findings:\\n- ...\\n- ...\\nSources: [name](url)\"
   - Final handoff:
     tool=message_writer, path=\"researcher\", old_text=\"proposal_append\",
     content=\"Final delta summary:\\n- ...\\nUncertainty: ...\\nSources: ...\"
"#
        .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        let run_doc_hint = self
            .run_document_path()
            .map(|path| {
                format!(
                    "- Run writer mode is active: use `message_writer` for run-document updates\n\
                     - Canonical run document path: `{path}`\n\
                     - Do not create alternate draft files for conductor runs"
                )
            })
            .unwrap_or_default();
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
- If the objective/user input includes explicit URLs, fetch those URLs first.
- For URL verification, do not rely on search ranking/indexing alone.
- Mark a URL as unavailable only after fetch_url returns a non-success status or fetch error.
- Use file_read to reference existing documents, code, or previous research
- Use file_write to create your working draft (overwrites existing)
- Use file_edit to refine specific sections without rewriting everything
- Use message_writer for run-document updates when run writer mode is active
- Run writer mode protocol (strict):
  - Treat message_writer as your output channel to the researcher section.
  - Use mode proposal_append for substantive content updates.
  - Emit first substantive proposal_append by step 2 (latest).
  - Emit another proposal_append whenever findings materially change.
  - Emit a final proposal_append immediately before Complete or Block.
  - Never return Complete/Block with zero successful message_writer calls.
- Content quality protocol:
  - Do not output long, rigid report templates from researcher.
  - Send concise evidence deltas (what changed since last update).
  - Include source links for factual claims.
  - If a later fetch/search contradicts earlier text, explicitly mark the earlier claim as superseded.
  - Prefer uncertainty over false certainty when evidence is incomplete.
- Maintain your working draft - it should evolve as you learn
- Write findings immediately - don't wait until the end
- Cite sources inline as markdown links: [title](url)
- Put the most important finding first (don't bury the lede)
- Use freeform markdown - no forced structure
- Recommended loop shape in run writer mode:
  1) fetch_url for any explicit URLs in the objective/user message
  2) web_search to fill context gaps and discover corroborating sources
  3) message_writer proposal_append with concise findings + citations
  4) repeat until objective is satisfied, then final proposal_append and Complete
{}
"#,
            ctx.step_number, ctx.max_steps, ctx.model_used, ctx.objective, run_doc_hint
        )
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let start_time = tokio::time::Instant::now();

        match tool_call.tool_name.as_str() {
            "message_writer" => self.execute_message_writer(tool_call).await,

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
                let url = Self::resolve_fetch_url_arg(tool_call).ok_or_else(|| {
                    HarnessError::ToolExecution(
                        "Missing URL argument (expected path, optional alias: query)".to_string(),
                    )
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

                let is_run_doc_path = is_run_document_path(path)
                    || self
                        .run_document_path()
                        .as_ref()
                        .map(|p| p == path)
                        .unwrap_or(false);
                if is_run_doc_path && self.has_run_writer() {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    Ok(ToolExecution {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("Run document writes must use message_writer tool".to_string()),
                        execution_time_ms: elapsed,
                    })
                } else if is_run_doc_path {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    Ok(ToolExecution {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(
                            "Run document writes are unavailable without RunWriterActor"
                                .to_string(),
                        ),
                        execution_time_ms: elapsed,
                    })
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

                let is_run_doc_path = is_run_document_path(path)
                    || self
                        .run_document_path()
                        .as_ref()
                        .map(|p| p == path)
                        .unwrap_or(false);
                if is_run_doc_path && self.has_run_writer() {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    Ok(ToolExecution {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some("Run document edits must use message_writer tool".to_string()),
                        execution_time_ms: elapsed,
                    })
                } else if is_run_doc_path {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    Ok(ToolExecution {
                        tool_name: tool_call.tool_name.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(
                            "Run document edits are unavailable without RunWriterActor".to_string(),
                        ),
                        execution_time_ms: elapsed,
                    })
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

    fn validate_terminal_decision(
        &self,
        _ctx: &ExecutionContext,
        _decision: &crate::baml_client::types::AgentDecision,
        tool_executions: &[ToolExecution],
    ) -> Result<(), String> {
        let writer_mode_active = self.writer_actor.is_some() && self.has_run_writer();
        if !Self::terminal_decision_has_required_writer_message(writer_mode_active, tool_executions)
        {
            return Err(
                "Run writer mode requires at least one successful message_writer tool call before completion"
                    .to_string(),
            );
        }
        Ok(())
    }

    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: shared_types::WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "report": &report,
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

        match report.status {
            shared_types::WorkerTurnStatus::Completed => {
                self.writer_set_state(SectionState::Complete).await;
            }
            shared_types::WorkerTurnStatus::Failed | shared_types::WorkerTurnStatus::Blocked => {
                self.writer_set_state(SectionState::Failed).await;
            }
            shared_types::WorkerTurnStatus::Running => {
                self.writer_set_state(SectionState::Running).await;
            }
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
            "phase": &progress.phase,
            "message": &progress.message,
            "model_used": &progress.model_used,
            "timestamp": &progress.timestamp,
        });
        self.emit_event("worker.task.progress", payload);

        match progress.phase.as_str() {
            "started" => self.writer_set_state(SectionState::Running).await,
            "completed" => self.writer_set_state(SectionState::Complete).await,
            "failed" => self.writer_set_state(SectionState::Failed).await,
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_writer_section_defaults_to_researcher() {
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(None),
            "researcher".to_string()
        );
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(Some("")),
            "researcher".to_string()
        );
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(Some("unknown")),
            "researcher".to_string()
        );
    }

    #[test]
    fn resolve_writer_section_allows_known_sections() {
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(Some("Conductor")),
            "conductor".to_string()
        );
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(Some("terminal")),
            "terminal".to_string()
        );
        assert_eq!(
            ResearcherAdapter::resolve_writer_section(Some("user")),
            "user".to_string()
        );
    }

    #[test]
    fn parse_section_state_handles_supported_values() {
        assert_eq!(
            ResearcherAdapter::parse_section_state(Some("pending")),
            Some(SectionState::Pending)
        );
        assert_eq!(
            ResearcherAdapter::parse_section_state(Some("running")),
            Some(SectionState::Running)
        );
        assert_eq!(
            ResearcherAdapter::parse_section_state(Some("complete")),
            Some(SectionState::Complete)
        );
        assert_eq!(
            ResearcherAdapter::parse_section_state(Some("completed")),
            Some(SectionState::Complete)
        );
        assert_eq!(
            ResearcherAdapter::parse_section_state(Some("failed")),
            Some(SectionState::Failed)
        );
        assert_eq!(ResearcherAdapter::parse_section_state(Some("bogus")), None);
    }

    #[test]
    fn terminal_decision_requires_message_writer_when_writer_mode_active() {
        assert!(!ResearcherAdapter::terminal_decision_has_required_writer_message(true, &[]));
        assert!(ResearcherAdapter::terminal_decision_has_required_writer_message(false, &[]));
        assert!(
            ResearcherAdapter::terminal_decision_has_required_writer_message(
                true,
                &[ToolExecution {
                    tool_name: "message_writer".to_string(),
                    success: true,
                    output: "{}".to_string(),
                    error: None,
                    execution_time_ms: 1,
                }]
            )
        );
    }
}
