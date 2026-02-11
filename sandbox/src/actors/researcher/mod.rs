//! ResearcherActor - policy-driven research loop.
//!
//! Runtime shape:
//! 1) Planner LLM decides next action (`search`, `fetch_url`, `complete`, `block`)
//! 2) Researcher executes tools (web search providers + fetch_url)
//! 3) Researcher emits structured progress/finding/learning events
//! 4) Synthesis LLM decides terminal status and final summary

mod events;
mod policy;
mod providers;

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, RpcReplyPort};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::actors::event_store::EventStoreMsg;
use crate::actors::model_config::{
    load_model_policy, ModelConfigError, ModelRegistry, ModelResolutionContext,
};

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
        reply: RpcReplyPort<Result<ResearcherResult, ResearcherError>>,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchObjectiveStatus {
    Complete,
    Incomplete,
    Blocked,
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
        Ok(ResearcherState {
            researcher_id: args.researcher_id,
            user_id: args.user_id,
            event_store: args.event_store,
            current_model: std::env::var("CHOIR_RESEARCHER_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| load_model_policy().researcher_default_model)
                .unwrap_or_else(|| "ZaiGLM47".to_string()),
            model_registry: ModelRegistry::new(),
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
                max_results,
                max_rounds,
                model_override,
                progress_tx,
                reply,
            } => {
                let request = ResearcherWebSearchRequest {
                    query: objective,
                    objective: None,
                    provider: Some("auto".to_string()),
                    max_results,
                    max_rounds,
                    time_range: None,
                    include_domains: None,
                    exclude_domains: None,
                    timeout_ms,
                    model_override,
                    reasoning: Some("conductor_delegation".to_string()),
                };
                let _ = reply.send(self.handle_web_search(state, request, progress_tx).await);
            }
            ResearcherMsg::RunWebSearchTool {
                request,
                progress_tx,
                reply,
            } => {
                let _ = reply.send(self.handle_web_search(state, request, progress_tx).await);
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
    fn map_model_error(error: ModelConfigError) -> ResearcherError {
        match error {
            ModelConfigError::UnknownModel(model_id) => {
                ResearcherError::ModelResolution(format!("unknown model: {model_id}"))
            }
            ModelConfigError::MissingApiKey(env_var) => {
                ResearcherError::ModelResolution(format!("missing API key: {env_var}"))
            }
            ModelConfigError::NoFallbackAvailable => {
                ResearcherError::ModelResolution("no fallback model available".to_string())
            }
        }
    }

    fn build_worker_report(
        &self,
        state: &ResearcherState,
        loop_id: &str,
        provider_label: &str,
        query: &str,
        citations: &[ResearchCitation],
        key_findings: &[String],
        gaps: &[String],
        confidence: f64,
    ) -> shared_types::WorkerTurnReport {
        let findings = key_findings
            .iter()
            .enumerate()
            .map(|(idx, claim)| {
                let evidence = citations
                    .get(idx)
                    .map(|citation| vec![citation.url.clone()])
                    .unwrap_or_default();
                shared_types::WorkerFinding {
                    finding_id: ulid::Ulid::new().to_string(),
                    claim: claim.clone(),
                    confidence,
                    evidence_refs: evidence,
                    novel: Some(true),
                }
            })
            .collect::<Vec<_>>();

        let learnings = gaps
            .iter()
            .map(|gap| shared_types::WorkerLearning {
                learning_id: ulid::Ulid::new().to_string(),
                insight: gap.clone(),
                confidence,
                supports: findings.iter().map(|f| f.finding_id.clone()).collect(),
                changes_plan: Some(true),
            })
            .collect::<Vec<_>>();

        shared_types::WorkerTurnReport {
            turn_id: loop_id.to_string(),
            worker_id: state.researcher_id.clone(),
            task_id: loop_id.to_string(),
            worker_role: Some("researcher".to_string()),
            status: shared_types::WorkerTurnStatus::Completed,
            summary: Some(format!(
                "Research loop used {} for '{}' and produced {} citations",
                provider_label,
                query,
                citations.len()
            )),
            findings,
            learnings,
            escalations: Vec::new(),
            artifacts: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    async fn handle_web_search(
        &self,
        state: &mut ResearcherState,
        request: ResearcherWebSearchRequest,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
    ) -> Result<ResearcherResult, ResearcherError> {
        let query = request.query.trim().to_string();
        if query.is_empty() {
            return Err(ResearcherError::Validation(
                "web_search query cannot be empty".to_string(),
            ));
        }
        let objective = request
            .objective
            .as_ref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| query.clone());
        let max_results = request.max_results.unwrap_or(6).clamp(1, 20);
        let max_rounds = request.max_rounds.unwrap_or(3).clamp(1, 8) as usize;
        let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(3_000, 120_000);

        let resolved_model = state
            .model_registry
            .resolve_for_role(
                "researcher",
                &ModelResolutionContext {
                    request_model: request.model_override.clone(),
                    app_preference: Some(state.current_model.clone()),
                    user_preference: None,
                },
            )
            .map_err(Self::map_model_error)?;
        let model_used = resolved_model.config.id;
        let loop_id = ulid::Ulid::new().to_string();

        events::emit_started(state, &loop_id, &objective, &model_used);
        events::emit_progress(
            state,
            &progress_tx,
            &loop_id,
            "research_task_started",
            "researcher entered policy loop",
            None,
            Some(model_used.clone()),
            None,
        );

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| {
                ResearcherError::ProviderRequest("http_client".to_string(), e.to_string())
            })?;

        let mut current_query = query.clone();
        let mut successful_outputs = Vec::<providers::ProviderSearchOutput>::new();
        let mut calls = Vec::<ResearchProviderCall>::new();
        let mut fetched_pages = Vec::<ResearcherFetchUrlResult>::new();
        let mut errors = Vec::<String>::new();
        let mut last_error: Option<String> = None;

        for round in 1..=max_rounds {
            let citations = providers::merge_citations(&successful_outputs);
            let decision = policy::plan_step(
                &state.model_registry,
                &model_used,
                &objective,
                &current_query,
                round,
                max_rounds,
                request.provider.as_deref(),
                request.max_results,
                last_error.as_deref(),
                &calls,
                &citations,
                &fetched_pages,
            )
            .await?;

            events::emit_progress(
                state,
                &progress_tx,
                &loop_id,
                "research_policy_decision",
                format!(
                    "round {}/{} action={} confidence={:.2} rationale={}",
                    round, max_rounds, decision.action, decision.confidence, decision.rationale
                ),
                None,
                Some(model_used.clone()),
                Some(citations.len()),
            );

            match decision.action {
                crate::baml_client::types::ResearcherNextAction::Search => {
                    let query_for_round = decision
                        .query
                        .as_ref()
                        .map(|q| q.trim())
                        .filter(|q| !q.is_empty())
                        .ok_or_else(|| {
                            ResearcherError::Policy(
                                "ResearcherPlanStep returned Search without query".to_string(),
                            )
                        })?
                        .to_string();
                    let provider_directive = decision
                        .provider
                        .as_deref()
                        .or(request.provider.as_deref())
                        .unwrap_or("auto");
                    let selection = providers::parse_provider_selection(Some(provider_directive));
                    let round_max_results =
                        decision.max_results.unwrap_or(max_results).clamp(1, 20);
                    let round_time_range = decision
                        .time_range
                        .as_deref()
                        .or(request.time_range.as_deref());
                    current_query = query_for_round;

                    let (round_outputs, round_calls, round_errors) =
                        providers::run_provider_selection(
                            &http,
                            selection,
                            &current_query,
                            round_max_results,
                            round_time_range,
                            request.include_domains.as_deref(),
                            request.exclude_domains.as_deref(),
                        )
                        .await;

                    for call in &round_calls {
                        let phase = if call.succeeded {
                            "research_provider_result"
                        } else {
                            "research_provider_error"
                        };
                        let message = if call.succeeded {
                            format!(
                                "{} provider returned {} results",
                                call.provider, call.result_count
                            )
                        } else {
                            format!(
                                "{} provider failed: {}",
                                call.provider,
                                call.error.clone().unwrap_or_default()
                            )
                        };
                        events::emit_progress(
                            state,
                            &progress_tx,
                            &loop_id,
                            phase,
                            message,
                            Some(call.provider.clone()),
                            Some(model_used.clone()),
                            Some(call.result_count),
                        );
                    }

                    last_error = round_errors.last().cloned();
                    successful_outputs.extend(round_outputs);
                    calls.extend(round_calls);
                    errors.extend(round_errors);
                }
                crate::baml_client::types::ResearcherNextAction::FetchUrl => {
                    let url = decision
                        .fetch_url
                        .as_ref()
                        .map(|u| u.trim())
                        .filter(|u| !u.is_empty())
                        .ok_or_else(|| {
                            ResearcherError::Policy(
                                "ResearcherPlanStep returned FetchUrl without fetch_url"
                                    .to_string(),
                            )
                        })?
                        .to_string();
                    let fetch_request = ResearcherFetchUrlRequest {
                        url: url.clone(),
                        timeout_ms: request.timeout_ms,
                        max_chars: Some(8_000),
                    };
                    let fetched = providers::fetch_url(&fetch_request).await?;
                    events::emit_progress(
                        state,
                        &progress_tx,
                        &loop_id,
                        "research_fetch_result",
                        format!(
                            "fetched {} status={} chars={}",
                            fetched.url,
                            fetched.status_code,
                            fetched.content_excerpt.len()
                        ),
                        None,
                        Some(model_used.clone()),
                        Some(fetched.content_excerpt.len()),
                    );
                    fetched_pages.push(fetched);
                }
                crate::baml_client::types::ResearcherNextAction::Complete
                | crate::baml_client::types::ResearcherNextAction::Block => {
                    break;
                }
            }
        }

        let citations = providers::merge_citations(&successful_outputs);
        let provider_used = providers::provider_label_from_outputs(&successful_outputs)
            .unwrap_or_else(|| "none".to_string());
        let raw_results_count = successful_outputs
            .iter()
            .map(|o| o.raw_results_count)
            .sum::<usize>();

        let synthesis = policy::summarize(
            &state.model_registry,
            &model_used,
            &objective,
            &current_query,
            &provider_used,
            &citations,
            &calls,
            &fetched_pages,
            raw_results_count,
            &errors,
        )
        .await?;

        for finding in &synthesis.key_findings {
            let finding_id = ulid::Ulid::new().to_string();
            let evidence = citations
                .iter()
                .take(2)
                .map(|c| c.url.clone())
                .collect::<Vec<_>>();
            events::emit_finding(
                state,
                &loop_id,
                &finding_id,
                finding,
                synthesis.confidence,
                &evidence,
            );
        }
        for gap in &synthesis.gaps {
            events::emit_learning(
                state,
                &loop_id,
                &ulid::Ulid::new().to_string(),
                gap,
                synthesis.confidence,
            );
        }

        if synthesis.objective_status == ResearchObjectiveStatus::Blocked {
            events::emit_failed(state, &loop_id, &synthesis.completion_reason);
        } else {
            events::emit_completed(state, &loop_id, &synthesis.summary);
        }

        let worker_report = self.build_worker_report(
            state,
            &loop_id,
            &provider_used,
            &current_query,
            &citations,
            &synthesis.key_findings,
            &synthesis.gaps,
            synthesis.confidence,
        );

        Ok(ResearcherResult {
            summary: synthesis.summary,
            success: synthesis.objective_status != ResearchObjectiveStatus::Blocked,
            objective_status: synthesis.objective_status,
            completion_reason: synthesis.completion_reason,
            recommended_next_capability: synthesis.recommended_next_capability,
            recommended_next_objective: synthesis.recommended_next_objective,
            provider_used: if provider_used == "none" {
                None
            } else {
                Some(provider_used)
            },
            model_used: Some(model_used),
            citations,
            provider_calls: calls,
            raw_results_count,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join(" | "))
            },
            worker_report: Some(worker_report),
        })
    }
}
