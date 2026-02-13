//! Conductor policy authority and BAML integration.
//!
//! This module owns model resolution + LLM policy calls so the actor can stay
//! focused on state transitions and message coordination.

use async_trait::async_trait;
use std::sync::Arc;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::model_config::{ModelRegistry, ModelResolutionContext};
use crate::baml_client::types::{
    ConductorBootstrapInput, ConductorBootstrapOutput, ConductorDecision,
    ConductorDecisionInput, ConductorObjectiveRefineInput, ConductorObjectiveRefineOutput,
};
use crate::baml_client::{ClientRegistry, B};

pub type SharedConductorPolicy = Arc<dyn ConductorPolicy>;

#[async_trait]
pub trait ConductorPolicy: Send + Sync {
    async fn bootstrap_agenda(
        &self,
        raw_objective: &str,
        available_capabilities: &[String],
    ) -> Result<ConductorBootstrapOutput, ConductorError>;

    async fn decide_next_action(
        &self,
        run: &shared_types::ConductorRunState,
        available_capabilities: &[String],
    ) -> Result<ConductorDecision, ConductorError>;

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

    fn build_decision_input(run: &shared_types::ConductorRunState) -> ConductorDecisionInput {
        ConductorDecisionInput {
            run_id: run.run_id.clone(),
            objective: run.objective.clone(),
            document_path: run.document_path.clone(),
            last_error: None, // Can be populated later if needed
        }
    }
}

#[async_trait]
impl ConductorPolicy for BamlConductorPolicy {
    async fn bootstrap_agenda(
        &self,
        raw_objective: &str,
        available_capabilities: &[String],
    ) -> Result<ConductorBootstrapOutput, ConductorError> {
        let client_registry = self.resolve_client_registry_for_role("conductor")?;
        let input = ConductorBootstrapInput {
            raw_objective: raw_objective.to_string(),
            available_capabilities: available_capabilities.to_vec(),
        };
        B.ConductorBootstrapAgenda
            .with_client_registry(&client_registry)
            .call(&input)
            .await
            .map_err(|e| ConductorError::PolicyError(format!("Bootstrap agenda failed: {e}")))
    }

    async fn decide_next_action(
        &self,
        run: &shared_types::ConductorRunState,
        _available_capabilities: &[String],
    ) -> Result<ConductorDecision, ConductorError> {
        let client_registry = self.resolve_client_registry_for_role("conductor")?;
        let input = Self::build_decision_input(run);
        B.ConductorDecide
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
