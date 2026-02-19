//! ResearcherActor - policy-driven research loop using unified agent harness.
//!
//! Runtime shape:
//! 1) Harness runs agentic loop with BAML-based planning
//! 2) ResearcherAdapter executes tools (web search providers + fetch_url)
//! 3) Adapter emits structured progress/finding/learning events
//! 4) Harness synthesizes final response and generates WorkerTurnReport

mod adapter;
mod events;
pub(crate) mod providers;

// Policy module kept for backward compatibility with BAML types
// The researcher now uses the unified agent harness instead
pub mod policy;

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, RpcReplyPort};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::actors::agent_harness::{AgentHarness, AgentResult, HarnessConfig, ObjectiveStatus};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::writer::{
    WriterDelegateCapability, WriterDelegateResult, WriterError, WriterMsg,
};
use crate::observability::llm_trace::LlmTraceEmitter;

pub use adapter::ResearcherAdapter;

#[derive(Debug, Default)]
pub struct ResearcherActor;

#[derive(Debug, Clone)]
pub struct ResearcherArguments {
    pub researcher_id: String,
    pub user_id: String,
    pub event_store: ractor::ActorRef<EventStoreMsg>,
}

pub struct ResearcherState {
    pub(crate) researcher_id: String,
    pub(crate) user_id: String,
    pub(crate) event_store: ractor::ActorRef<EventStoreMsg>,
    current_model: String,
    model_registry: ModelRegistry,
}

#[derive(Debug)]
pub enum ResearcherMsg {
    RunAgenticTask {
        objective: String,
        timeout_ms: Option<u64>,
        max_results: Option<u32>,
        max_rounds: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        writer_actor: Option<ractor::ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
        reply: RpcReplyPort<Result<ResearcherResult, ResearcherError>>,
    },
    RunAgenticTaskDetached {
        objective: String,
        timeout_ms: Option<u64>,
        max_results: Option<u32>,
        max_rounds: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        writer_actor: Option<ractor::ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
    },
    RunWebSearchTool {
        request: ResearcherWebSearchRequest,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        reply: RpcReplyPort<Result<ResearcherResult, ResearcherError>>,
    },
    RunFetchUrlTool {
        request: ResearcherFetchUrlRequest,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        reply: RpcReplyPort<Result<ResearcherFetchUrlResult, ResearcherError>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherWebSearchRequest {
    pub query: String,
    pub objective: Option<String>,
    pub provider: Option<String>,
    pub max_results: Option<u32>,
    pub max_rounds: Option<u8>,
    pub time_range: Option<String>,
    pub include_domains: Option<Vec<String>>,
    pub exclude_domains: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    pub model_override: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherFetchUrlRequest {
    pub url: String,
    pub timeout_ms: Option<u64>,
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherProgress {
    pub phase: String,
    pub message: String,
    pub provider: Option<String>,
    pub model_used: Option<String>,
    pub result_count: Option<usize>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherFetchUrlResult {
    pub url: String,
    pub final_url: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub content_excerpt: String,
    pub content_length: usize,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCitation {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProviderCall {
    pub provider: String,
    pub latency_ms: u64,
    pub result_count: usize,
    pub succeeded: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherResult {
    pub summary: String,
    pub success: bool,
    pub objective_status: ResearchObjectiveStatus,
    pub completion_reason: String,
    pub recommended_next_capability: Option<String>,
    pub recommended_next_objective: Option<String>,
    pub provider_used: Option<String>,
    pub model_used: Option<String>,
    pub citations: Vec<ResearchCitation>,
    pub provider_calls: Vec<ResearchProviderCall>,
    pub raw_results_count: usize,
    pub error: Option<String>,
    pub worker_report: Option<shared_types::WorkerTurnReport>,
    /// Citation IDs emitted as citation.proposed events (for writer confirmation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_citation_ids: Vec<String>,
    /// Full stubs for external content publish trigger (3.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_citation_stubs: Vec<crate::actors::writer::ProposedCitationStub>,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum ResearcherError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("missing API key env var: {0}")]
    MissingApiKey(String),
    #[error("provider request failed ({0}): {1}")]
    ProviderRequest(String, String),
    #[error("provider response parse failed ({0}): {1}")]
    ProviderParse(String, String),
    #[error("all providers failed")]
    AllProvidersFailed,
    #[error("model resolution error: {0}")]
    ModelResolution(String),
    #[error("policy error: {0}")]
    Policy(String),
    #[error("harness error: {0}")]
    Harness(String),
}

impl From<crate::actors::agent_harness::HarnessError> for ResearcherError {
    fn from(e: crate::actors::agent_harness::HarnessError) -> Self {
        match e {
            crate::actors::agent_harness::HarnessError::ModelResolution(msg) => {
                ResearcherError::ModelResolution(msg)
            }
            crate::actors::agent_harness::HarnessError::Decision(msg) => {
                ResearcherError::Policy(format!("Decision failed: {msg}"))
            }
            crate::actors::agent_harness::HarnessError::ToolExecution(msg) => {
                ResearcherError::ProviderRequest("tool".to_string(), msg)
            }
            crate::actors::agent_harness::HarnessError::Timeout(ms) => {
                ResearcherError::ProviderRequest("timeout".to_string(), format!("{ms}ms"))
            }
            crate::actors::agent_harness::HarnessError::Blocked(reason) => {
                ResearcherError::Policy(format!("Blocked: {reason}"))
            }
            crate::actors::agent_harness::HarnessError::Adapter(msg) => {
                ResearcherError::Harness(msg)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchObjectiveStatus {
    Complete,
    Incomplete,
    Blocked,
}

impl From<ObjectiveStatus> for ResearchObjectiveStatus {
    fn from(status: ObjectiveStatus) -> Self {
        match status {
            ObjectiveStatus::Complete => ResearchObjectiveStatus::Complete,
            ObjectiveStatus::Incomplete => ResearchObjectiveStatus::Incomplete,
            ObjectiveStatus::Blocked => ResearchObjectiveStatus::Blocked,
        }
    }
}

#[async_trait]
impl Actor for ResearcherActor {
    type Msg = ResearcherMsg;
    type State = ResearcherState;
    type Arguments = ResearcherArguments;

    async fn pre_start(
        &self,
        _myself: ractor::ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let model_registry = ModelRegistry::new();
        Ok(ResearcherState {
            researcher_id: args.researcher_id,
            user_id: args.user_id,
            event_store: args.event_store,
            current_model: std::env::var("CHOIR_RESEARCHER_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| model_registry.default_model_for_callsite("researcher"))
                .or_else(|| model_registry.available_model_ids().into_iter().next())
                .unwrap_or_else(|| "unknown".to_string()),
            model_registry,
        })
    }

    async fn handle(
        &self,
        _myself: ractor::ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ResearcherMsg::RunAgenticTask {
                objective,
                timeout_ms,
                max_results: _,
                max_rounds,
                model_override,
                progress_tx,
                writer_actor,
                run_id,
                call_id,
                reply,
            } => {
                let writer_actor_for_run = writer_actor.clone();
                let run_id_for_run = run_id.clone();
                let call_id_for_run = call_id.clone();
                let result = self
                    .run_with_harness(
                        state,
                        objective,
                        timeout_ms,
                        max_rounds,
                        model_override,
                        progress_tx,
                        writer_actor_for_run,
                        run_id_for_run,
                        call_id_for_run,
                    )
                    .await;
                Self::emit_writer_completion(
                    writer_actor,
                    run_id.clone(),
                    call_id.clone(),
                    result.clone(),
                );
                let _ = reply.send(result);
            }
            ResearcherMsg::RunAgenticTaskDetached {
                objective,
                timeout_ms,
                max_results: _,
                max_rounds,
                model_override,
                progress_tx,
                writer_actor,
                run_id,
                call_id,
            } => {
                let result = self
                    .run_with_harness(
                        state,
                        objective,
                        timeout_ms,
                        max_rounds,
                        model_override,
                        progress_tx,
                        writer_actor.clone(),
                        run_id.clone(),
                        call_id.clone(),
                    )
                    .await;
                Self::emit_writer_completion(writer_actor, run_id, call_id, result);
            }
            ResearcherMsg::RunWebSearchTool {
                request,
                progress_tx,
                reply,
            } => {
                let result = self
                    .run_with_harness(
                        state,
                        request.query.clone(),
                        request.timeout_ms,
                        request.max_rounds,
                        request.model_override.clone(),
                        progress_tx,
                        None,
                        None,
                        None,
                    )
                    .await;
                let _ = reply.send(result);
            }
            ResearcherMsg::RunFetchUrlTool {
                request,
                progress_tx,
                reply,
            } => {
                let loop_id = ulid::Ulid::new().to_string();
                events::emit_progress(
                    state,
                    &progress_tx,
                    &loop_id,
                    "research_fetch_call",
                    format!("fetching {}", request.url),
                    None,
                    None,
                    None,
                );
                let result = providers::fetch_url(&request).await;
                if let Ok(ref ok) = result {
                    events::emit_progress(
                        state,
                        &progress_tx,
                        &loop_id,
                        "research_fetch_result",
                        format!(
                            "fetched {} status={} chars={}",
                            ok.url,
                            ok.status_code,
                            ok.content_excerpt.len()
                        ),
                        None,
                        None,
                        Some(ok.content_excerpt.len()),
                    );
                }
                let _ = reply.send(result);
            }
        }
        Ok(())
    }
}

impl ResearcherActor {
    /// Emit `citation.proposed` events for all citations extracted from a research run.
    /// Returns (citation_ids, citation_stubs) for writer confirmation (3.2) and external
    /// content publish trigger (3.4).
    fn emit_citation_proposed_events(
        event_store: &ractor::ActorRef<EventStoreMsg>,
        researcher_id: &str,
        user_id: &str,
        run_id: Option<&str>,
        loop_id: &str,
        citations: &[ResearchCitation],
    ) -> (
        Vec<String>,
        Vec<crate::actors::writer::ProposedCitationStub>,
    ) {
        let mut emitted_ids = Vec::new();
        let mut stubs = Vec::new();
        for citation in citations {
            let citation_id = ulid::Ulid::new().to_string();
            let cited_kind = "external_url".to_string();
            let cited_id = citation.url.clone();
            let citation_record = shared_types::CitationRecord {
                citation_id: citation_id.clone(),
                cited_id: cited_id.clone(),
                cited_kind: cited_kind.clone(),
                citing_run_id: run_id.unwrap_or("").to_string(),
                citing_loop_id: loop_id.to_string(),
                citing_actor: "researcher".to_string(),
                cite_kind: shared_types::CitationKind::RetrievedContext,
                confidence: citation.score.unwrap_or(0.0),
                excerpt: Some(citation.snippet.clone()),
                rationale: citation.title.clone(),
                status: shared_types::CitationStatus::Proposed,
                proposed_by: "researcher".to_string(),
                confirmed_by: None,
                confirmed_at: None,
                created_at: chrono::Utc::now(),
            };
            if let Ok(payload) = serde_json::to_value(&citation_record) {
                let _ = event_store.cast(EventStoreMsg::AppendAsync {
                    event: AppendEvent {
                        event_type: shared_types::EVENT_TOPIC_CITATION_PROPOSED.to_string(),
                        payload,
                        actor_id: researcher_id.to_string(),
                        user_id: user_id.to_string(),
                    },
                });
                emitted_ids.push(citation_id.clone());
                stubs.push(crate::actors::writer::ProposedCitationStub {
                    citation_id,
                    cited_kind,
                    cited_id,
                });
            }
        }
        (emitted_ids, stubs)
    }

    fn emit_writer_completion(
        writer_actor: Option<ractor::ActorRef<WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
        result: Result<ResearcherResult, ResearcherError>,
    ) {
        if let (Some(writer_actor), Some(run_id)) = (writer_actor, run_id) {
            let dispatch_id = call_id
                .clone()
                .and_then(|id| id.rsplit(':').next().map(ToString::to_string))
                .unwrap_or_else(|| ulid::Ulid::new().to_string());
            let completion = result
                .map(|research| WriterDelegateResult {
                    capability: WriterDelegateCapability::Researcher,
                    success: research.success,
                    summary: research.summary,
                    proposed_citation_ids: research.proposed_citation_ids,
                    proposed_citation_stubs: research.proposed_citation_stubs,
                })
                .map_err(|error| WriterError::WorkerFailed(error.to_string()));
            let _ = writer_actor.send_message(WriterMsg::DelegationWorkerCompleted {
                capability: WriterDelegateCapability::Researcher,
                run_id: Some(run_id),
                call_id,
                dispatch_id,
                result: completion,
            });
        }
    }

    async fn run_with_harness(
        &self,
        state: &ResearcherState,
        objective: String,
        timeout_ms: Option<u64>,
        max_rounds: Option<u8>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        writer_actor: Option<ractor::ActorRef<crate::actors::writer::WriterMsg>>,
        run_id: Option<String>,
        call_id: Option<String>,
    ) -> Result<ResearcherResult, ResearcherError> {
        let timeout = timeout_ms.unwrap_or(30_000).clamp(3_000, 120_000);
        let max_steps = max_rounds.unwrap_or(100).clamp(1, 100) as usize;

        let adapter_state = ResearcherState {
            researcher_id: state.researcher_id.clone(),
            user_id: state.user_id.clone(),
            event_store: state.event_store.clone(),
            current_model: state.current_model.clone(),
            model_registry: state.model_registry.clone(),
        };

        let adapter = ResearcherAdapter::new(adapter_state, progress_tx.clone(), timeout)?;

        let adapter = adapter.with_run_context(run_id.clone());
        let adapter = match writer_actor {
            Some(writer_actor_ref) => adapter.with_writer_actor(writer_actor_ref),
            None => adapter,
        };

        let config = HarnessConfig {
            timeout_budget_ms: timeout,
            max_steps,
            emit_progress: true,
            emit_worker_report: true,
        };

        let harness = AgentHarness::with_config(
            adapter,
            state.model_registry.clone(),
            config,
            LlmTraceEmitter::new(state.event_store.clone()),
        );

        let agent_result: AgentResult = harness
            .run(
                state.researcher_id.clone(),
                state.user_id.clone(),
                objective,
                model_override,
                None,
                run_id.clone(),
                call_id,
            )
            .await
            .map_err(ResearcherError::from)?;

        let mut citations = Vec::new();
        let mut provider_calls = Vec::new();

        for tool_exec in &agent_result.tool_executions {
            if tool_exec.tool_name == "web_search" {
                if let Ok(output) = serde_json::from_str::<serde_json::Value>(&tool_exec.output) {
                    if let Some(cits) = output.get("citations").and_then(|v| v.as_array()) {
                        for cit in cits {
                            if let Ok(citation) =
                                serde_json::from_value::<ResearchCitation>(cit.clone())
                            {
                                citations.push(citation);
                            }
                        }
                    }
                    if let Some(calls) = output.get("provider_calls").and_then(|v| v.as_array()) {
                        for call in calls {
                            if let Ok(provider_call) =
                                serde_json::from_value::<ResearchProviderCall>(call.clone())
                            {
                                provider_calls.push(provider_call);
                            }
                        }
                    }
                }
            }
        }

        let provider_used = if provider_calls.is_empty() {
            None
        } else {
            let labels: Vec<String> = provider_calls
                .iter()
                .filter(|c| c.succeeded)
                .map(|c| c.provider.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            if labels.is_empty() {
                None
            } else {
                Some(labels.join("->"))
            }
        };

        let raw_results_count = citations.len();

        // 3.1: Emit citation.proposed for each extracted citation; capture IDs and stubs
        let (proposed_citation_ids, proposed_citation_stubs) = if !citations.is_empty() {
            let loop_id = run_id.as_deref().unwrap_or("").to_string();
            Self::emit_citation_proposed_events(
                &state.event_store,
                &state.researcher_id,
                &state.user_id,
                run_id.as_deref(),
                &loop_id,
                &citations,
            )
        } else {
            (Vec::new(), Vec::new())
        };

        Ok(ResearcherResult {
            summary: agent_result.summary,
            success: agent_result.success,
            objective_status: agent_result.objective_status.into(),
            completion_reason: agent_result.completion_reason,
            recommended_next_capability: None,
            recommended_next_objective: None,
            provider_used,
            model_used: agent_result.model_used,
            citations,
            provider_calls,
            raw_results_count,
            error: None,
            worker_report: agent_result.worker_report,
            proposed_citation_ids,
            proposed_citation_stubs,
        })
    }
}
