use crate::actors::model_config::ModelRegistry;
use crate::baml_client::{types, B};

use super::{ResearchCitation, ResearchObjectiveStatus, ResearchProviderCall, ResearcherError};

#[derive(Debug, Clone)]
pub(crate) struct PlannerDecision {
    pub action: types::ResearcherNextAction,
    pub query: Option<String>,
    pub provider: Option<String>,
    pub fetch_url: Option<String>,
    pub max_results: Option<u32>,
    pub time_range: Option<String>,
    pub rationale: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct SynthesisResult {
    pub summary: String,
    pub objective_status: ResearchObjectiveStatus,
    pub completion_reason: String,
    pub recommended_next_capability: Option<String>,
    pub recommended_next_objective: Option<String>,
    pub key_findings: Vec<String>,
    pub gaps: Vec<String>,
    pub confidence: f64,
}

fn to_baml_citations(citations: &[ResearchCitation]) -> Vec<types::ResearcherCitationInput> {
    citations
        .iter()
        .take(20)
        .map(|citation| types::ResearcherCitationInput {
            provider: citation.provider.clone(),
            title: citation.title.clone(),
            url: citation.url.clone(),
            snippet: citation.snippet.clone(),
            published_at: citation.published_at.clone(),
            score: citation.score,
        })
        .collect()
}

fn to_baml_provider_calls(
    calls: &[ResearchProviderCall],
) -> Vec<types::ResearcherProviderCallSummary> {
    calls
        .iter()
        .map(|call| types::ResearcherProviderCallSummary {
            provider: call.provider.clone(),
            latency_ms: call.latency_ms as i64,
            result_count: call.result_count as i64,
            succeeded: call.succeeded,
            error: call.error.clone(),
        })
        .collect()
}

fn to_baml_fetched_pages(
    pages: &[super::ResearcherFetchUrlResult],
) -> Vec<types::ResearcherFetchedPageInput> {
    pages
        .iter()
        .map(|page| types::ResearcherFetchedPageInput {
            url: page.url.clone(),
            status_code: page.status_code as i64,
            content_excerpt: page.content_excerpt.clone(),
            success: page.success,
        })
        .collect()
}

fn map_status(status: &types::ResearcherObjectiveStatus) -> ResearchObjectiveStatus {
    match status {
        types::ResearcherObjectiveStatus::Complete => ResearchObjectiveStatus::Complete,
        types::ResearcherObjectiveStatus::Incomplete => ResearchObjectiveStatus::Incomplete,
        types::ResearcherObjectiveStatus::Blocked => ResearchObjectiveStatus::Blocked,
    }
}

pub(crate) async fn plan_step(
    model_registry: &ModelRegistry,
    model_used: &str,
    objective: &str,
    current_query: &str,
    round: usize,
    max_rounds: usize,
    provider_hint: Option<&str>,
    max_results_hint: Option<u32>,
    last_error: Option<&str>,
    calls: &[ResearchProviderCall],
    citations: &[ResearchCitation],
    fetched_pages: &[super::ResearcherFetchUrlResult],
) -> Result<PlannerDecision, ResearcherError> {
    let client_registry = model_registry
        .create_runtime_client_registry_for_model(model_used)
        .map_err(|e| ResearcherError::Policy(format!("client registry creation failed: {e}")))?;

    let input = types::ResearcherPlanInput {
        objective: objective.to_string(),
        current_query: current_query.to_string(),
        round: round as i64,
        max_rounds: max_rounds as i64,
        provider_hint: provider_hint.map(str::to_string),
        max_results_hint: max_results_hint.map(|v| v as i64),
        last_error: last_error.map(str::to_string),
        provider_calls: to_baml_provider_calls(calls),
        citations: to_baml_citations(citations),
        fetched_pages: to_baml_fetched_pages(fetched_pages),
    };

    let output = B
        .ResearcherPlanStep
        .with_client_registry(&client_registry)
        .call(&input)
        .await
        .map_err(|e| ResearcherError::Policy(format!("ResearcherPlanStep failed: {e}")))?;

    Ok(PlannerDecision {
        action: output.action,
        query: output.query,
        provider: output.provider,
        fetch_url: output.fetch_url,
        max_results: output.max_results.map(|v| v as u32),
        time_range: output.time_range,
        rationale: output.rationale,
        confidence: output.confidence,
    })
}

pub(crate) async fn summarize(
    model_registry: &ModelRegistry,
    model_used: &str,
    objective: &str,
    query: &str,
    provider_label: &str,
    citations: &[ResearchCitation],
    calls: &[ResearchProviderCall],
    fetched_pages: &[super::ResearcherFetchUrlResult],
    raw_results_count: usize,
    errors: &[String],
) -> Result<SynthesisResult, ResearcherError> {
    let client_registry = model_registry
        .create_runtime_client_registry_for_model(model_used)
        .map_err(|e| ResearcherError::Policy(format!("client registry creation failed: {e}")))?;

    let input = types::ResearcherSynthesisInput {
        objective: objective.to_string(),
        query: query.to_string(),
        provider_label: provider_label.to_string(),
        citations: to_baml_citations(citations),
        provider_calls: to_baml_provider_calls(calls),
        fetched_pages: to_baml_fetched_pages(fetched_pages),
        raw_results_count: raw_results_count as i64,
        errors: errors.to_vec(),
    };

    let output = B
        .ResearcherSummarizeEvidence
        .with_client_registry(&client_registry)
        .call(&input)
        .await
        .map_err(|e| ResearcherError::Policy(format!("ResearcherSummarizeEvidence failed: {e}")))?;

    Ok(SynthesisResult {
        summary: output.summary,
        objective_status: map_status(&output.objective_status),
        completion_reason: output.completion_reason,
        recommended_next_capability: output.recommended_next_capability,
        recommended_next_objective: output.recommended_next_objective,
        key_findings: output.key_findings,
        gaps: output.gaps,
        confidence: output.confidence,
    })
}
