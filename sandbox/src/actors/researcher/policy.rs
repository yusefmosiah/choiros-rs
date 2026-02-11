use crate::actors::model_config::ModelRegistry;
use crate::baml_client::{types, B};

use super::{ResearchCitation, ResearchObjectiveStatus, ResearchProviderCall, ResearcherError};

#[derive(Debug, Clone)]
pub(crate) struct PlannerDecision {
    pub action: ResearchAction,
    pub query: Option<String>,
    pub provider: Option<String>,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    pub reason: String,
    pub status: ResearchStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResearchAction {
    Search,
    FetchUrl,
    FileRead,
    FileWrite,
    FileEdit,
    Complete,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResearchStatus {
    Ongoing,
    Complete,
    Blocked,
}

impl ResearchStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, ResearchStatus::Complete | ResearchStatus::Blocked)
    }
}

impl From<types::ResearchStatus> for ResearchStatus {
    fn from(status: types::ResearchStatus) -> Self {
        match status {
            types::ResearchStatus::Ongoing => ResearchStatus::Ongoing,
            types::ResearchStatus::Complete => ResearchStatus::Complete,
            types::ResearchStatus::Blocked => ResearchStatus::Blocked,
        }
    }
}

impl From<types::ResearchAction> for ResearchAction {
    fn from(action: types::ResearchAction) -> Self {
        match action {
            types::ResearchAction::Search => ResearchAction::Search,
            types::ResearchAction::FetchUrl => ResearchAction::FetchUrl,
            types::ResearchAction::FileRead => ResearchAction::FileRead,
            types::ResearchAction::FileWrite => ResearchAction::FileWrite,
            types::ResearchAction::FileEdit => ResearchAction::FileEdit,
            types::ResearchAction::Complete => ResearchAction::Complete,
            types::ResearchAction::Block => ResearchAction::Block,
        }
    }
}

/// Plan the next research step using simplified BAML
pub(crate) async fn plan_step(
    model_registry: &ModelRegistry,
    model_used: &str,
    objective: &str,
    current_query: &str,
    round: usize,
    max_rounds: usize,
    working_draft_path: &str,
    last_error: Option<&str>,
) -> Result<PlannerDecision, ResearcherError> {
    let client_registry = model_registry
        .create_runtime_client_registry_for_model(model_used)
        .map_err(|e| ResearcherError::Policy(format!("client registry creation failed: {e}")))?;

    let input = types::ResearcherPlanInput {
        objective: objective.to_string(),
        current_query: current_query.to_string(),
        round: round as i64,
        max_rounds: max_rounds as i64,
        working_draft_path: working_draft_path.to_string(),
        last_error: last_error.map(str::to_string),
    };

    let output = B
        .ResearcherPlanStep
        .with_client_registry(&client_registry)
        .call(&input)
        .await
        .map_err(|e| ResearcherError::Policy(format!("ResearcherPlanStep failed: {e}")))?;

    Ok(PlannerDecision {
        action: output.action.into(),
        query: output.query,
        provider: output.provider,
        url: output.url,
        file_path: output.file_path,
        content: output.content,
        old_text: output.old_text,
        new_text: output.new_text,
        reason: output.reason,
        status: output.status.into(),
    })
}
