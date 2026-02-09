//! ResearcherActor - provider-isolated web research capability.
//!
//! This actor exposes two contracts:
//! - uactor -> actor: `RunAgenticTask` objective delegation
//! - appactor -> toolactor: `RunWebSearchTool` typed tool request

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    researcher_id: String,
    #[allow(dead_code)]
    event_store: ractor::ActorRef<EventStoreMsg>,
    current_model: String,
    model_registry: ModelRegistry,
}

#[derive(Debug)]
pub enum ResearcherMsg {
    RunAgenticTask {
        objective: String,
        timeout_ms: Option<u64>,
        max_results: Option<u32>,
        model_override: Option<String>,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        reply: RpcReplyPort<Result<ResearcherResult, ResearcherError>>,
    },
    RunWebSearchTool {
        request: ResearcherWebSearchRequest,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
        reply: RpcReplyPort<Result<ResearcherResult, ResearcherError>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherWebSearchRequest {
    pub query: String,
    pub objective: Option<String>,
    pub provider: Option<String>,
    pub max_results: Option<u32>,
    pub time_range: Option<String>,
    pub include_domains: Option<Vec<String>>,
    pub exclude_domains: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    pub model_override: Option<String>,
    pub reasoning: Option<String>,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchObjectiveStatus {
    Complete,
    Incomplete,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchProvider {
    Tavily,
    Brave,
    Exa,
}

impl SearchProvider {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Tavily => "tavily",
            Self::Brave => "brave",
            Self::Exa => "exa",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderSelection {
    AutoSequential,
    Single(SearchProvider),
    Parallel(Vec<SearchProvider>),
}

#[derive(Debug, Clone)]
struct ProviderSearchOutput {
    provider: SearchProvider,
    citations: Vec<ResearchCitation>,
    raw_results_count: usize,
    latency_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::{ProviderSelection, ResearcherActor, SearchProvider};

    #[test]
    fn parse_provider_selection_defaults_to_auto() {
        assert_eq!(
            ResearcherActor::parse_provider_selection(None),
            ProviderSelection::AutoSequential
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("")),
            ProviderSelection::AutoSequential
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("auto")),
            ProviderSelection::AutoSequential
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("unknown-provider")),
            ProviderSelection::AutoSequential
        );
    }

    #[test]
    fn parse_provider_selection_single_provider() {
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("brave")),
            ProviderSelection::Single(SearchProvider::Brave)
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("exa")),
            ProviderSelection::Single(SearchProvider::Exa)
        );
    }

    #[test]
    fn parse_provider_selection_all_and_parallel_lists() {
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("all")),
            ProviderSelection::Parallel(vec![
                SearchProvider::Tavily,
                SearchProvider::Brave,
                SearchProvider::Exa
            ])
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("tavily,exa")),
            ProviderSelection::Parallel(vec![SearchProvider::Tavily, SearchProvider::Exa])
        );
        assert_eq!(
            ResearcherActor::parse_provider_selection(Some("tavily,tavily,brave")),
            ProviderSelection::Parallel(vec![SearchProvider::Tavily, SearchProvider::Brave])
        );
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
        Ok(ResearcherState {
            researcher_id: args.researcher_id,
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
                model_override,
                progress_tx,
                reply,
            } => {
                let request = ResearcherWebSearchRequest {
                    query: objective,
                    objective: None,
                    provider: Some("auto".to_string()),
                    max_results,
                    time_range: None,
                    include_domains: None,
                    exclude_domains: None,
                    timeout_ms,
                    model_override,
                    reasoning: Some("uactor->actor objective delegation".to_string()),
                };
                let result = self.handle_web_search(state, request, progress_tx).await;
                let _ = reply.send(result);
            }
            ResearcherMsg::RunWebSearchTool {
                request,
                progress_tx,
                reply,
            } => {
                let result = self.handle_web_search(state, request, progress_tx).await;
                let _ = reply.send(result);
            }
        }
        Ok(())
    }
}

impl ResearcherActor {
    fn relevance_tokens(query: &str) -> Vec<String> {
        // STOP words are generic English words to filter out for relevance scoring.
        // Note: Domain-specific terms should NOT be added here.
        const STOP: &[&str] = &[
            "the", "a", "an", "and", "or", "for", "to", "of", "in", "on", "at", "is", "are", "be",
            "with", "as", "by", "from", "today", "now", "current", "latest", "what", "whats",
        ];
        query
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter_map(|part| {
                let lowered = part.trim().to_ascii_lowercase();
                if lowered.len() < 3 || STOP.contains(&lowered.as_str()) {
                    None
                } else {
                    Some(lowered)
                }
            })
            .collect()
    }

    fn assess_objective_completion(
        query: &str,
        citations: &[ResearchCitation],
        provider_calls: &[ResearchProviderCall],
        success: bool,
    ) -> (
        ResearchObjectiveStatus,
        String,
        Option<String>,
        Option<String>,
    ) {
        if !success {
            return (
                ResearchObjectiveStatus::Blocked,
                "All providers failed or returned unusable responses.".to_string(),
                Some("conductor".to_string()),
                None,
            );
        }

        if citations.is_empty() {
            return (
                ResearchObjectiveStatus::Incomplete,
                "No citations were returned, so objective evidence is insufficient.".to_string(),
                Some("terminal".to_string()),
                Some(format!(
                    "Use terminal tools to complete this objective with verifiable, up-to-date evidence: {}",
                    query
                )),
            );
        }

        let tokens = Self::relevance_tokens(query);
        let mut matched = 0usize;
        for token in &tokens {
            if citations.iter().any(|c| {
                let haystack = format!(
                    "{} {} {}",
                    c.title.to_ascii_lowercase(),
                    c.snippet.to_ascii_lowercase(),
                    c.url.to_ascii_lowercase()
                );
                haystack.contains(token)
            }) {
                matched += 1;
            }
        }
        let coverage = if tokens.is_empty() {
            1.0
        } else {
            matched as f64 / tokens.len() as f64
        };

        let avg_score = {
            let scored = citations
                .iter()
                .filter_map(|c| c.score)
                .take(6)
                .collect::<Vec<_>>();
            if scored.is_empty() {
                None
            } else {
                Some(scored.iter().sum::<f64>() / scored.len() as f64)
            }
        };

        let provider_successes = provider_calls.iter().filter(|c| c.succeeded).count();
        let low_confidence = avg_score.map(|s| s < 0.35).unwrap_or(false);
        let weak_coverage = coverage < 0.35;

        if provider_successes == 0 || weak_coverage || low_confidence {
            return (
                ResearchObjectiveStatus::Incomplete,
                format!(
                    "Research evidence is weak (coverage={:.2}, avg_score={:?}); additional execution needed.",
                    coverage, avg_score
                ),
                Some("terminal".to_string()),
                Some(format!(
                    "Use terminal tools to verify and complete this objective with current data: {}",
                    query
                )),
            );
        }

        (
            ResearchObjectiveStatus::Complete,
            "Sufficient citation coverage for objective completion.".to_string(),
            None,
            None,
        )
    }

    fn auto_provider_parallel_enabled() -> bool {
        match std::env::var("CHOIR_RESEARCHER_AUTO_PROVIDER_MODE")
            .unwrap_or_else(|_| "parallel".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "sequential" | "seq" | "false" | "0" => false,
            _ => true,
        }
    }

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

    fn emit_progress(
        progress_tx: &Option<mpsc::UnboundedSender<ResearcherProgress>>,
        phase: impl Into<String>,
        message: impl Into<String>,
        provider: Option<String>,
        model_used: Option<String>,
        result_count: Option<usize>,
    ) {
        if let Some(progress_tx) = progress_tx {
            let _ = progress_tx.send(ResearcherProgress {
                phase: phase.into(),
                message: message.into(),
                provider,
                model_used,
                result_count,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }
    }

    fn parse_provider_token(input: &str) -> Option<SearchProvider> {
        match input.trim().to_ascii_lowercase().as_str() {
            "tavily" => Some(SearchProvider::Tavily),
            "brave" => Some(SearchProvider::Brave),
            "exa" => Some(SearchProvider::Exa),
            _ => None,
        }
    }

    fn all_providers() -> Vec<SearchProvider> {
        vec![
            SearchProvider::Tavily,
            SearchProvider::Brave,
            SearchProvider::Exa,
        ]
    }

    fn parse_provider_selection(input: Option<&str>) -> ProviderSelection {
        let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
            return ProviderSelection::AutoSequential;
        };

        let lower = raw.to_ascii_lowercase();
        if lower == "auto" {
            return ProviderSelection::AutoSequential;
        }

        if lower == "all" || lower == "*" {
            return ProviderSelection::Parallel(Self::all_providers());
        }

        if lower.contains(',') {
            let mut seen = HashSet::<&'static str>::new();
            let mut providers = Vec::new();
            for token in lower.split(',') {
                if let Some(provider) = Self::parse_provider_token(token) {
                    let key = provider.as_str();
                    if seen.insert(key) {
                        providers.push(provider);
                    }
                }
            }
            return match providers.len() {
                0 => ProviderSelection::AutoSequential,
                1 => ProviderSelection::Single(providers[0]),
                _ => ProviderSelection::Parallel(providers),
            };
        }

        if let Some(single) = Self::parse_provider_token(&lower) {
            ProviderSelection::Single(single)
        } else {
            ProviderSelection::AutoSequential
        }
    }

    fn map_time_range_to_brave(value: Option<&str>) -> Option<String> {
        match value.map(|v| v.trim().to_ascii_lowercase()) {
            Some(v) if v == "day" || v == "d" => Some("pd".to_string()),
            Some(v) if v == "week" || v == "w" => Some("pw".to_string()),
            Some(v) if v == "month" || v == "m" => Some("pm".to_string()),
            Some(v) if v == "year" || v == "y" => Some("py".to_string()),
            Some(v) if !v.is_empty() => Some(v),
            _ => None,
        }
    }

    async fn handle_web_search(
        &self,
        state: &mut ResearcherState,
        request: ResearcherWebSearchRequest,
        progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
    ) -> Result<ResearcherResult, ResearcherError> {
        let query = request.query.trim().to_string();
        let objective = request
            .objective
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| query.clone());
        if query.is_empty() {
            return Err(ResearcherError::Validation(
                "web_search query cannot be empty".to_string(),
            ));
        }

        let max_results = request.max_results.unwrap_or(6).clamp(1, 20);
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

        Self::emit_progress(
            &progress_tx,
            "research_task_started",
            "researcher accepted request and is planning provider routing",
            None,
            Some(model_used.clone()),
            None,
        );

        let selection = match Self::parse_provider_selection(request.provider.as_deref()) {
            ProviderSelection::AutoSequential if Self::auto_provider_parallel_enabled() => {
                ProviderSelection::Parallel(Self::all_providers())
            }
            other => other,
        };
        let providers: Vec<SearchProvider> = match &selection {
            ProviderSelection::AutoSequential => Self::all_providers(),
            ProviderSelection::Single(provider) => vec![*provider],
            ProviderSelection::Parallel(list) => list.clone(),
        };

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| {
                ResearcherError::ProviderRequest("http_client".to_string(), e.to_string())
            })?;

        let mut calls = Vec::new();
        let mut successful_outputs: Vec<ProviderSearchOutput> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let is_parallel = matches!(&selection, ProviderSelection::Parallel(_));
        match selection {
            ProviderSelection::Parallel(ref parallel_providers) => {
                for provider in parallel_providers {
                    Self::emit_progress(
                        &progress_tx,
                        "research_provider_call",
                        format!("calling {} search provider", provider.as_str()),
                        Some(provider.as_str().to_string()),
                        Some(model_used.clone()),
                        None,
                    );
                }

                let outcomes = futures_util::future::join_all(parallel_providers.iter().map(
                    |provider| async {
                        let provider = *provider;
                        let started = tokio::time::Instant::now();
                        let result = self
                            .run_provider_search(
                                provider,
                                &http,
                                &query,
                                max_results,
                                request.time_range.as_deref(),
                                request.include_domains.as_deref(),
                                request.exclude_domains.as_deref(),
                            )
                            .await;
                        let elapsed = started.elapsed().as_millis() as u64;
                        (provider, elapsed, result)
                    },
                ))
                .await;

                for (provider, elapsed, result) in outcomes {
                    match result {
                        Ok(mut output) => {
                            output.latency_ms = elapsed;
                            calls.push(ResearchProviderCall {
                                provider: provider.as_str().to_string(),
                                latency_ms: elapsed,
                                result_count: output.citations.len(),
                                succeeded: true,
                                error: None,
                            });
                            Self::emit_progress(
                                &progress_tx,
                                "research_provider_result",
                                format!(
                                    "{} provider returned {} results",
                                    provider.as_str(),
                                    output.citations.len()
                                ),
                                Some(provider.as_str().to_string()),
                                Some(model_used.clone()),
                                Some(output.citations.len()),
                            );
                            successful_outputs.push(output);
                        }
                        Err(err) => {
                            let err_text = err.to_string();
                            errors.push(format!("{}: {}", provider.as_str(), err_text));
                            calls.push(ResearchProviderCall {
                                provider: provider.as_str().to_string(),
                                latency_ms: elapsed,
                                result_count: 0,
                                succeeded: false,
                                error: Some(err_text.clone()),
                            });
                            Self::emit_progress(
                                &progress_tx,
                                "research_provider_error",
                                format!("{} provider failed: {}", provider.as_str(), err_text),
                                Some(provider.as_str().to_string()),
                                Some(model_used.clone()),
                                None,
                            );
                        }
                    }
                }
            }
            _ => {
                for provider in providers {
                    Self::emit_progress(
                        &progress_tx,
                        "research_provider_call",
                        format!("calling {} search provider", provider.as_str()),
                        Some(provider.as_str().to_string()),
                        Some(model_used.clone()),
                        None,
                    );
                    let started = tokio::time::Instant::now();
                    let result = self
                        .run_provider_search(
                            provider,
                            &http,
                            &query,
                            max_results,
                            request.time_range.as_deref(),
                            request.include_domains.as_deref(),
                            request.exclude_domains.as_deref(),
                        )
                        .await;
                    let elapsed = started.elapsed().as_millis() as u64;

                    match result {
                        Ok(mut output) => {
                            output.latency_ms = elapsed;
                            calls.push(ResearchProviderCall {
                                provider: provider.as_str().to_string(),
                                latency_ms: elapsed,
                                result_count: output.citations.len(),
                                succeeded: true,
                                error: None,
                            });
                            Self::emit_progress(
                                &progress_tx,
                                "research_provider_result",
                                format!(
                                    "{} provider returned {} results",
                                    provider.as_str(),
                                    output.citations.len()
                                ),
                                Some(provider.as_str().to_string()),
                                Some(model_used.clone()),
                                Some(output.citations.len()),
                            );
                            successful_outputs.push(output);
                            break;
                        }
                        Err(err) => {
                            let err_text = err.to_string();
                            errors.push(format!("{}: {}", provider.as_str(), err_text));
                            calls.push(ResearchProviderCall {
                                provider: provider.as_str().to_string(),
                                latency_ms: elapsed,
                                result_count: 0,
                                succeeded: false,
                                error: Some(err_text.clone()),
                            });
                            Self::emit_progress(
                                &progress_tx,
                                "research_provider_error",
                                format!("{} provider failed: {}", provider.as_str(), err_text),
                                Some(provider.as_str().to_string()),
                                Some(model_used.clone()),
                                None,
                            );
                        }
                    }
                }
            }
        }

        if successful_outputs.is_empty() {
            let (objective_status, completion_reason, next_capability, next_objective) =
                Self::assess_objective_completion(&objective, &[], &calls, false);
            return Ok(ResearcherResult {
                summary: "All configured research providers failed for this query.".to_string(),
                success: false,
                objective_status,
                completion_reason,
                recommended_next_capability: next_capability,
                recommended_next_objective: next_objective,
                provider_used: None,
                model_used: Some(model_used),
                citations: Vec::new(),
                provider_calls: calls,
                raw_results_count: 0,
                error: Some(errors.join(" | ")),
                worker_report: None,
            });
        }

        let provider_used = if is_parallel {
            let successful = successful_outputs
                .iter()
                .map(|output| output.provider.as_str().to_string())
                .collect::<Vec<_>>();
            format!("parallel:{}", successful.join(","))
        } else {
            successful_outputs[0].provider.as_str().to_string()
        };

        let (citations, raw_results_count) = if is_parallel {
            let mut seen_urls = HashSet::<String>::new();
            let mut merged = Vec::new();
            let mut raw_count = 0usize;
            for output in successful_outputs {
                raw_count += output.raw_results_count;
                for citation in output.citations {
                    if seen_urls.insert(citation.url.clone()) {
                        merged.push(citation);
                    }
                }
            }
            (merged, raw_count)
        } else {
            let mut iter = successful_outputs.into_iter();
            if let Some(output) = iter.next() {
                (output.citations, output.raw_results_count)
            } else {
                (Vec::new(), 0)
            }
        };

        let summary = self.summarize_citations(&query, &provider_used, &citations);
        let (objective_status, completion_reason, next_capability, next_objective) =
            Self::assess_objective_completion(&objective, &citations, &calls, true);
        Self::emit_progress(
            &progress_tx,
            "research_task_completed",
            format!(
                "researcher completed synthesis (objective_status={})",
                match objective_status {
                    ResearchObjectiveStatus::Complete => "complete",
                    ResearchObjectiveStatus::Incomplete => "incomplete",
                    ResearchObjectiveStatus::Blocked => "blocked",
                }
            ),
            Some(provider_used.clone()),
            Some(model_used.clone()),
            Some(citations.len()),
        );

        let worker_report =
            self.build_worker_report(&state.researcher_id, &provider_used, &query, &citations);

        Ok(ResearcherResult {
            summary,
            success: true,
            objective_status,
            completion_reason,
            recommended_next_capability: next_capability,
            recommended_next_objective: next_objective,
            provider_used: Some(provider_used),
            model_used: Some(model_used),
            citations,
            provider_calls: calls,
            raw_results_count,
            error: None,
            worker_report: Some(worker_report),
        })
    }

    async fn run_provider_search(
        &self,
        provider: SearchProvider,
        http: &reqwest::Client,
        query: &str,
        max_results: u32,
        time_range: Option<&str>,
        include_domains: Option<&[String]>,
        exclude_domains: Option<&[String]>,
    ) -> Result<ProviderSearchOutput, ResearcherError> {
        match provider {
            SearchProvider::Tavily => {
                self.search_tavily(
                    http,
                    query,
                    max_results,
                    time_range,
                    include_domains,
                    exclude_domains,
                )
                .await
            }
            SearchProvider::Brave => {
                self.search_brave(http, query, max_results, time_range)
                    .await
            }
            SearchProvider::Exa => {
                self.search_exa(http, query, max_results, include_domains, exclude_domains)
                    .await
            }
        }
    }

    fn summarize_citations(
        &self,
        query: &str,
        provider_label: &str,
        citations: &[ResearchCitation],
    ) -> String {
        if citations.is_empty() {
            return format!(
                "No results were returned for query '{}' from {}.",
                query, provider_label
            );
        }

        let mut lines = Vec::new();
        lines.push(format!(
            "Research results for '{}' via {}:",
            query, provider_label
        ));
        for citation in citations.iter().take(5) {
            lines.push(format!(
                "- {} ({})",
                citation.title.trim(),
                citation.url.trim()
            ));
        }
        lines.join("\n")
    }

    fn build_worker_report(
        &self,
        researcher_id: &str,
        provider_label: &str,
        query: &str,
        citations: &[ResearchCitation],
    ) -> shared_types::WorkerTurnReport {
        let findings = citations
            .iter()
            .take(2)
            .map(|citation| shared_types::WorkerFinding {
                finding_id: ulid::Ulid::new().to_string(),
                claim: format!("Relevant source found: {}", citation.title),
                confidence: 0.72,
                evidence_refs: vec![citation.url.clone()],
                novel: Some(true),
            })
            .collect::<Vec<_>>();

        let learnings = if citations.len() >= 2 {
            vec![shared_types::WorkerLearning {
                learning_id: ulid::Ulid::new().to_string(),
                insight: format!(
                    "Provider {} returned multiple corroborating sources for '{}'",
                    provider_label, query
                ),
                confidence: 0.62,
                supports: findings
                    .iter()
                    .map(|finding| finding.finding_id.clone())
                    .collect(),
                changes_plan: Some(false),
            }]
        } else {
            Vec::new()
        };

        shared_types::WorkerTurnReport {
            turn_id: ulid::Ulid::new().to_string(),
            worker_id: researcher_id.to_string(),
            task_id: ulid::Ulid::new().to_string(),
            worker_role: Some("researcher".to_string()),
            status: shared_types::WorkerTurnStatus::Completed,
            summary: Some(format!(
                "Completed web research using {} with {} citations",
                provider_label,
                citations.len()
            )),
            findings,
            learnings,
            escalations: Vec::new(),
            artifacts: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    async fn search_tavily(
        &self,
        http: &reqwest::Client,
        query: &str,
        max_results: u32,
        time_range: Option<&str>,
        include_domains: Option<&[String]>,
        exclude_domains: Option<&[String]>,
    ) -> Result<ProviderSearchOutput, ResearcherError> {
        let api_key = std::env::var("TAVILY_API_KEY")
            .map_err(|_| ResearcherError::MissingApiKey("TAVILY_API_KEY".to_string()))?;

        let mut body = serde_json::json!({
            "query": query,
            "search_depth": "basic",
            "max_results": max_results,
            "include_answer": false,
            "include_raw_content": false
        });
        if let Some(time_range) = time_range {
            body["time_range"] = serde_json::Value::String(time_range.to_string());
        }
        if let Some(include_domains) = include_domains {
            body["include_domains"] = serde_json::to_value(include_domains)
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
        }
        if let Some(exclude_domains) = exclude_domains {
            body["exclude_domains"] = serde_json::to_value(exclude_domains)
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
        }

        let response = http
            .post("https://api.tavily.com/search")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ResearcherError::ProviderRequest("tavily".to_string(), e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ResearcherError::ProviderRequest(
                "tavily".to_string(),
                format!("status {}: {}", status, body),
            ));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ResearcherError::ProviderParse("tavily".to_string(), e.to_string()))?;
        let results = payload
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ResearcherError::ProviderParse(
                    "tavily".to_string(),
                    "missing results array".to_string(),
                )
            })?;

        let mut citations = Vec::new();
        for row in results {
            let url = row
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if url.is_empty() {
                continue;
            }
            let title = row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let snippet = row
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let published_at = row
                .get("published_date")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let score = row.get("score").and_then(|v| v.as_f64());
            citations.push(ResearchCitation {
                id: url.clone(),
                provider: "tavily".to_string(),
                title,
                url,
                snippet,
                published_at,
                score,
            });
        }

        Ok(ProviderSearchOutput {
            provider: SearchProvider::Tavily,
            raw_results_count: citations.len(),
            citations,
            latency_ms: 0,
        })
    }

    async fn search_brave(
        &self,
        http: &reqwest::Client,
        query: &str,
        max_results: u32,
        time_range: Option<&str>,
    ) -> Result<ProviderSearchOutput, ResearcherError> {
        let api_key = std::env::var("BRAVE_API_KEY")
            .map_err(|_| ResearcherError::MissingApiKey("BRAVE_API_KEY".to_string()))?;

        let mut request = http
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &max_results.to_string())]);
        if let Some(freshness) = Self::map_time_range_to_brave(time_range) {
            request = request.query(&[("freshness", freshness)]);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ResearcherError::ProviderRequest("brave".to_string(), e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ResearcherError::ProviderRequest(
                "brave".to_string(),
                format!("status {}: {}", status, body),
            ));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ResearcherError::ProviderParse("brave".to_string(), e.to_string()))?;

        let results = payload
            .get("web")
            .and_then(|v| v.get("results"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ResearcherError::ProviderParse(
                    "brave".to_string(),
                    "missing web.results array".to_string(),
                )
            })?;

        let mut citations = Vec::new();
        for row in results {
            let url = row
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if url.is_empty() {
                continue;
            }
            let title = row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let snippet = row
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let published_at = row
                .get("age")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            citations.push(ResearchCitation {
                id: url.clone(),
                provider: "brave".to_string(),
                title,
                url,
                snippet,
                published_at,
                score: None,
            });
        }

        Ok(ProviderSearchOutput {
            provider: SearchProvider::Brave,
            raw_results_count: citations.len(),
            citations,
            latency_ms: 0,
        })
    }

    async fn search_exa(
        &self,
        http: &reqwest::Client,
        query: &str,
        max_results: u32,
        include_domains: Option<&[String]>,
        exclude_domains: Option<&[String]>,
    ) -> Result<ProviderSearchOutput, ResearcherError> {
        let api_key = std::env::var("EXA_API_KEY")
            .map_err(|_| ResearcherError::MissingApiKey("EXA_API_KEY".to_string()))?;

        let mut body = serde_json::json!({
            "query": query,
            "numResults": max_results,
            "type": "auto",
            "contents": { "text": true }
        });
        if let Some(include_domains) = include_domains {
            body["includeDomains"] = serde_json::to_value(include_domains)
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
        }
        if let Some(exclude_domains) = exclude_domains {
            body["excludeDomains"] = serde_json::to_value(exclude_domains)
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
        }

        let response = http
            .post("https://api.exa.ai/search")
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ResearcherError::ProviderRequest("exa".to_string(), e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ResearcherError::ProviderRequest(
                "exa".to_string(),
                format!("status {}: {}", status, body),
            ));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ResearcherError::ProviderParse("exa".to_string(), e.to_string()))?;
        let results = payload
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ResearcherError::ProviderParse(
                    "exa".to_string(),
                    "missing results array".to_string(),
                )
            })?;

        let mut citations = Vec::new();
        for row in results {
            let url = row
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if url.is_empty() {
                continue;
            }
            let title = row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();

            let snippet = row
                .get("text")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    row.get("highlights")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                })
                .unwrap_or_default()
                .to_string();

            let published_at = row
                .get("publishedDate")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let score = row.get("score").and_then(|v| v.as_f64()).or_else(|| {
                row.get("highlightScores")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_f64())
            });
            citations.push(ResearchCitation {
                id: row
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&url)
                    .to_string(),
                provider: "exa".to_string(),
                title,
                url,
                snippet,
                published_at,
                score,
            });
        }

        Ok(ProviderSearchOutput {
            provider: SearchProvider::Exa,
            raw_results_count: citations.len(),
            citations,
            latency_ms: 0,
        })
    }
}
