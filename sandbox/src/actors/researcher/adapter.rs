//! ResearcherAdapter - AgentAdapter implementation for ResearcherActor
//!
//! This adapter bridges the ResearcherActor to the unified agent harness,
//! providing researcher-specific tool execution and event emission.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::actors::agent_harness::{
    AgentAdapter, AgentProgress, ExecutionContext, HarnessError, ToolExecution,
};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::baml_client::types::AgentToolCall;

use super::{
    providers, ResearcherFetchUrlRequest, ResearcherProgress, ResearcherState,
    ResearcherWebSearchRequest,
};

/// Adapter that connects ResearcherActor to the unified agent harness
pub struct ResearcherAdapter {
    state: ResearcherState,
    progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
    http_client: reqwest::Client,
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
        })
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
   - provider: string (optional) - Provider to use: "tavily", "brave", "exa", "auto"
   - max_results: number (optional) - Max results to return (1-20, default: 6)
   - time_range: string (optional) - Time filter: "day", "week", "month", "year"
   - include_domains: string[] (optional) - Domains to include
   - exclude_domains: string[] (optional) - Domains to exclude

2. fetch_url - Fetch and extract content from a URL
   Args:
   - url: string (required) - The URL to fetch
   - max_chars: number (optional) - Max characters to extract (default: 8000)
"#
        .to_string()
    }

    fn get_system_context(&self, ctx: &ExecutionContext) -> String {
        format!(
            r#"You are a research agent. Your goal is to gather information to fulfill the objective.

Current step: {}/{}
Model: {}
Objective: {}

Guidelines:
- Use web_search to find relevant information
- Use fetch_url to retrieve detailed content from specific URLs
- Be thorough but efficient - aim to complete within the step budget
- Synthesize findings into clear, actionable insights
- If you cannot find sufficient information, report what you found and note gaps
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
                // Try to get query from web_search struct first, then fall back to flat fields
                let query = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.query.clone())
                    .or_else(|| args.query.clone())
                    .ok_or_else(|| HarnessError::ToolExecution("Missing query argument".to_string()))?;

                // Extract provider and other args from web_search or flat fields
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
                let model = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.model.clone())
                    .or(args.model.clone());
                let reasoning = args
                    .web_search
                    .as_ref()
                    .and_then(|ws| ws.reasoning.clone())
                    .or(tool_call.reasoning.clone());

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
                    model_override: model,
                    reasoning,
                };

                // Parse provider selection
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

                // Run provider selection
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

                // Merge citations for output
                let citations = providers::merge_citations(&outputs);
                let success = !citations.is_empty();

                // Emit provider call progress
                for call in &calls {
                    let phase = if call.succeeded {
                        "research_provider_result"
                    } else {
                        "research_provider_error"
                    };
                    let message = if call.succeeded {
                        format!("{} provider returned {} results", call.provider, call.result_count)
                    } else {
                        format!(
                            "{} provider failed: {}",
                            call.provider,
                            call.error.clone().unwrap_or_default()
                        )
                    };

                    if let Some(tx) = &self.progress_tx {
                        let _ = tx.send(ResearcherProgress {
                            phase: phase.to_string(),
                            message: message.clone(),
                            provider: Some(call.provider.clone()),
                            model_used: Some(ctx.model_used.clone()),
                            result_count: Some(call.result_count),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        });
                    }

                    self.emit_event(
                        "worker.task.progress",
                        serde_json::json!({
                            "task_id": ctx.loop_id,
                            "worker_id": ctx.worker_id,
                            "phase": phase,
                            "message": message,
                            "provider": call.provider,
                            "model_used": ctx.model_used,
                            "result_count": call.result_count,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                    );
                }

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
                // For fetch_url, we use the 'path' field to carry the URL
                let url = args
                    .path
                    .as_ref()
                    .ok_or_else(|| HarnessError::ToolExecution("Missing path (url) argument".to_string()))?;

                let request = ResearcherFetchUrlRequest {
                    url: url.clone(),
                    timeout_ms: Some(30_000),
                    max_chars: Some(8000),
                };

                // Emit progress
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

                        // Emit progress
                        if let Some(tx) = &self.progress_tx {
                            let _ = tx.send(ResearcherProgress {
                                phase: "fetch_url_result".to_string(),
                                message: format!(
                                    "Fetched {} status={} chars={}",
                                    result.url,
                                    result.status_code,
                                    result.content_excerpt.len()
                                ),
                                provider: None,
                                model_used: Some(ctx.model_used.clone()),
                                result_count: Some(result.content_excerpt.len()),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }

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
        // Emit to event store
        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "report": report,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.report.received", payload);

        // Also send via progress channel if available
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(ResearcherProgress {
                phase: "worker_report".to_string(),
                message: format!("Worker report with {} findings", report.findings.len()),
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
        // Send via progress channel
        if let Some(tx) = &self.progress_tx {
            let researcher_progress = self.to_researcher_progress(&progress);
            let _ = tx.send(researcher_progress);
        }

        // Emit to event store
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

    async fn emit_finding(
        &self,
        ctx: &ExecutionContext,
        finding: shared_types::WorkerFinding,
    ) -> Result<(), HarnessError> {
        // Emit to event store
        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "phase": "finding",
            "finding_id": finding.finding_id,
            "claim": finding.claim,
            "confidence": finding.confidence,
            "evidence_refs": finding.evidence_refs,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.finding", payload);

        Ok(())
    }

    async fn emit_learning(
        &self,
        ctx: &ExecutionContext,
        learning: shared_types::WorkerLearning,
    ) -> Result<(), HarnessError> {
        // Emit to event store
        let payload = serde_json::json!({
            "task_id": ctx.loop_id,
            "worker_id": ctx.worker_id,
            "phase": "learning",
            "learning_id": learning.learning_id,
            "insight": learning.insight,
            "confidence": learning.confidence,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.emit_event("worker.task.learning", payload);

        Ok(())
    }
}
