//! Conductor policy authority and BAML integration.
//!
//! This module owns model resolution + LLM policy calls so the actor can stay
//! focused on state transitions and message coordination.

use async_trait::async_trait;
use std::sync::Arc;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::model_config::{ModelRegistry, ModelResolutionContext};
use crate::baml_client::types::{
    ConductorAgendaItem as BamlAgendaItem, ConductorArtifact as BamlArtifact,
    ConductorCapabilityCall as BamlCapabilityCall, ConductorDecisionInput, ConductorDecisionOutput,
    ConductorObjectiveRefineInput, ConductorObjectiveRefineOutput, EventSummary, WorkerOutput,
};
use crate::baml_client::{ClientRegistry, B};

pub type SharedConductorPolicy = Arc<dyn ConductorPolicy>;

#[async_trait]
pub trait ConductorPolicy: Send + Sync {
    async fn decide_next_action(
        &self,
        run: &shared_types::ConductorRunState,
        available_capabilities: &[String],
    ) -> Result<ConductorDecisionOutput, ConductorError>;

    async fn refine_objective_for_capability(
        &self,
        raw_objective: &str,
        capability: &str,
    ) -> Result<ConductorObjectiveRefineOutput, ConductorError>;
}

#[derive(Debug, Default)]
pub struct BamlConductorPolicy {
    registry: ModelRegistry,
}

impl BamlConductorPolicy {
    pub fn new() -> Self {
        Self {
            registry: ModelRegistry::new(),
        }
    }

    fn resolve_client_registry_for_role(
        &self,
        role: &str,
    ) -> Result<ClientRegistry, ConductorError> {
        let resolved = self
            .registry
            .resolve_for_role(role, &ModelResolutionContext::default())
            .map_err(|e| ConductorError::PolicyError(format!("Model resolution failed: {e}")))?;

        self.registry
            .create_runtime_client_registry_for_model(&resolved.config.id)
            .map_err(|e| {
                ConductorError::PolicyError(format!("Client registry creation failed: {e}"))
            })
    }

    fn build_decision_input(
        run: &shared_types::ConductorRunState,
        available_capabilities: &[String],
    ) -> ConductorDecisionInput {
        let agenda: Vec<BamlAgendaItem> = run
            .agenda
            .iter()
            .map(|item| BamlAgendaItem {
                id: item.item_id.clone(),
                capability: item.capability.clone(),
                objective: item.objective.clone(),
                dependencies: item.depends_on.clone(),
                status: format!("{:?}", item.status),
                priority: item.priority as i64,
            })
            .collect();

        let active_calls: Vec<BamlCapabilityCall> = run
            .active_calls
            .iter()
            .map(|call| BamlCapabilityCall {
                call_id: call.call_id.clone(),
                agenda_item_id: call.agenda_item_id.clone().unwrap_or_default(),
                capability: call.capability.clone(),
                objective: call.objective.clone(),
                status: format!("{:?}", call.status),
            })
            .collect();

        let artifacts: Vec<BamlArtifact> = run
            .artifacts
            .iter()
            .map(|artifact| BamlArtifact {
                artifact_id: artifact.artifact_id.clone(),
                name: artifact.artifact_id.clone(),
                content_type: artifact
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                summary: format!(
                    "{:?} artifact from {}",
                    artifact.kind, artifact.source_call_id
                ),
            })
            .collect();

        let worker_outputs: Vec<WorkerOutput> = run
            .artifacts
            .iter()
            .map(|artifact| {
                let summary = artifact
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("No summary")
                    .to_string();
                WorkerOutput {
                    call_id: artifact.source_call_id.clone(),
                    agenda_item_id: run
                        .active_calls
                        .iter()
                        .find(|c| c.call_id == artifact.source_call_id)
                        .and_then(|c| c.agenda_item_id.clone())
                        .unwrap_or_default(),
                    status: "completed".to_string(),
                    result_summary: summary,
                    artifacts_produced: vec![],
                    followup_recommendations: vec![],
                }
            })
            .collect();

        let recent_events = vec![EventSummary {
            event_id: format!("run-context-{}", run.run_id),
            event_type: "conductor.run.context".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload: serde_json::json!({
                "capabilities": available_capabilities
            })
            .to_string(),
        }];

        ConductorDecisionInput {
            run_id: run.run_id.clone(),
            task_id: run.task_id.clone(),
            objective: run.objective.clone(),
            run_status: format!("{:?}", run.status),
            agenda,
            active_calls,
            artifacts,
            recent_events,
            worker_outputs,
        }
    }
}

#[async_trait]
impl ConductorPolicy for BamlConductorPolicy {
    async fn decide_next_action(
        &self,
        run: &shared_types::ConductorRunState,
        available_capabilities: &[String],
    ) -> Result<ConductorDecisionOutput, ConductorError> {
        let client_registry = self.resolve_client_registry_for_role("conductor")?;
        let input = Self::build_decision_input(run, available_capabilities);
        B.ConductorDecideNextAction
            .with_client_registry(&client_registry)
            .call(&input)
            .await
            .map_err(|e| ConductorError::PolicyError(format!("BAML policy call failed: {e}")))
    }

    async fn refine_objective_for_capability(
        &self,
        raw_objective: &str,
        capability: &str,
    ) -> Result<ConductorObjectiveRefineOutput, ConductorError> {
        let client_registry = self.resolve_client_registry_for_role("conductor")?;
        let input = ConductorObjectiveRefineInput {
            raw_objective: raw_objective.to_string(),
            context: vec![],
            target_capability: capability.to_string(),
        };
        B.ConductorRefineObjective
            .with_client_registry(&client_registry)
            .call(&input)
            .await
            .map_err(|e| ConductorError::PolicyError(format!("Objective refine failed: {e}")))
    }
}
