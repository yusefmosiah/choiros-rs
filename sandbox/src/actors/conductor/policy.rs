//! Conductor policy authority and BAML integration.
//!
//! This module owns model resolution + LLM policy calls so the actor can stay
//! focused on state transitions and message coordination.

use async_trait::async_trait;
use shared_types::{AgendaItemStatus, CapabilityCallStatus};
use std::sync::Arc;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::event_store::EventStoreMsg;
use crate::actors::model_config::{
    ModelRegistry, ModelResolutionContext, ProviderConfig, ResolvedModel,
};
use crate::baml_client::types::{
    ConductorBootstrapInput, ConductorBootstrapOutput, ConductorDecision, ConductorDecisionInput,
    ConductorObjectiveRefineInput, ConductorObjectiveRefineOutput,
};
use crate::baml_client::{ClientRegistry, B};
use crate::observability::llm_trace::{LlmCallScope, LlmTraceEmitter};

pub type SharedConductorPolicy = Arc<dyn ConductorPolicy>;

const CAPABILITY_ROUTING_GUIDANCE: &str = "Capability routing guidance:\n- Use researcher for external information gathering, web search, URL fetch, citations, source synthesis, and current-events/news questions.\n- Use terminal for local shell/file/system execution only (build, test, inspect, edit files, run local commands).\n- Never route web/news/current-events objectives to terminal when researcher is available.";

#[async_trait]
pub trait ConductorPolicy: Send + Sync {
    async fn bootstrap_agenda(
        &self,
        run_id: Option<&str>,
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

#[derive(Debug)]
pub struct BamlConductorPolicy {
    registry: ModelRegistry,
    trace_emitter: LlmTraceEmitter,
}

impl BamlConductorPolicy {
    pub fn new(event_store: ractor::ActorRef<EventStoreMsg>) -> Self {
        Self {
            registry: ModelRegistry::new(),
            trace_emitter: LlmTraceEmitter::new(event_store),
        }
    }

    fn resolve_for_role(
        &self,
        role: &str,
    ) -> Result<(ClientRegistry, ResolvedModel), ConductorError> {
        let resolved = self
            .registry
            .resolve_for_role(role, &ModelResolutionContext::default())
            .map_err(|e| ConductorError::PolicyError(format!("Model resolution failed: {e}")))?;

        let client_registry = self
            .registry
            .create_runtime_client_registry_for_model(&resolved.config.id)
            .map_err(|e| {
                ConductorError::PolicyError(format!("Client registry creation failed: {e}"))
            })?;

        Ok((client_registry, resolved))
    }

    fn provider_string(provider: &ProviderConfig) -> &'static str {
        match provider {
            ProviderConfig::AwsBedrock { .. } => "aws-bedrock",
            ProviderConfig::AnthropicCompatible { .. } => "anthropic",
            ProviderConfig::OpenAiGeneric { .. } => "openai-generic",
        }
    }

    fn build_decision_input(run: &shared_types::ConductorRunState) -> ConductorDecisionInput {
        let runtime_state = Self::format_run_state_summary(run);
        ConductorDecisionInput {
            run_id: run.run_id.clone(),
            objective: format!(
                "{}\n\n{}\n\nRuntime state:\n{}\n\nDecision constraints:\n- If there are active worker calls, return AwaitWorker.\n- Use SpawnWorker only when no active worker calls remain and additional work is required.\n- Prefer researcher for external/web/news/current-events objectives.\n- Use terminal only for local shell/file/system execution.",
                run.objective, CAPABILITY_ROUTING_GUIDANCE, runtime_state
            ),
            document_path: run.document_path.clone(),
            last_error: Self::latest_error(run),
        }
    }

    fn latest_error(run: &shared_types::ConductorRunState) -> Option<String> {
        run.active_calls
            .iter()
            .rev()
            .find_map(|call| match call.status {
                CapabilityCallStatus::Failed | CapabilityCallStatus::Blocked => call.error.clone(),
                _ => None,
            })
    }

    fn format_run_state_summary(run: &shared_types::ConductorRunState) -> String {
        let agenda_pending = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Pending)
            .count();
        let agenda_ready = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Ready)
            .count();
        let agenda_running = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Running)
            .count();
        let agenda_completed = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Completed)
            .count();
        let agenda_failed = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Failed)
            .count();
        let agenda_blocked = run
            .agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Blocked)
            .count();

        let active_calls = run
            .active_calls
            .iter()
            .filter(|call| {
                matches!(
                    call.status,
                    CapabilityCallStatus::Pending | CapabilityCallStatus::Running
                )
            })
            .count();
        let completed_calls = run
            .active_calls
            .iter()
            .filter(|call| call.status == CapabilityCallStatus::Completed)
            .count();
        let failed_calls = run
            .active_calls
            .iter()
            .filter(|call| call.status == CapabilityCallStatus::Failed)
            .count();
        let blocked_calls = run
            .active_calls
            .iter()
            .filter(|call| call.status == CapabilityCallStatus::Blocked)
            .count();

        let mut recent_calls: Vec<String> = run
            .active_calls
            .iter()
            .rev()
            .take(5)
            .map(|call| format!("{}:{}:{:?}", call.call_id, call.capability, call.status))
            .collect();
        if recent_calls.is_empty() {
            recent_calls.push("none".to_string());
        }

        let mut recent_decisions: Vec<String> = run
            .decision_log
            .iter()
            .rev()
            .take(5)
            .map(|decision| format!("{:?}: {}", decision.decision_type, decision.reason))
            .collect();
        if recent_decisions.is_empty() {
            recent_decisions.push("none".to_string());
        }

        format!(
            "- run_status: {:?}\n- agenda: pending={} ready={} running={} completed={} failed={} blocked={}\n- calls: active={} completed={} failed={} blocked={}\n- recent_calls: {}\n- recent_decisions: {}",
            run.status,
            agenda_pending,
            agenda_ready,
            agenda_running,
            agenda_completed,
            agenda_failed,
            agenda_blocked,
            active_calls,
            completed_calls,
            failed_calls,
            blocked_calls,
            recent_calls.join(", "),
            recent_decisions.join(" | ")
        )
    }
}

#[async_trait]
impl ConductorPolicy for BamlConductorPolicy {
    async fn bootstrap_agenda(
        &self,
        run_id: Option<&str>,
        raw_objective: &str,
        available_capabilities: &[String],
    ) -> Result<ConductorBootstrapOutput, ConductorError> {
        let (client_registry, resolved) = self.resolve_for_role("conductor")?;
        let model_used = resolved.config.id.as_str();
        let provider = Some(Self::provider_string(&resolved.config.provider));

        let system_context = format!("{raw_objective}\n\n{CAPABILITY_ROUTING_GUIDANCE}");
        let input = ConductorBootstrapInput {
            raw_objective: system_context.clone(),
            available_capabilities: available_capabilities.to_vec(),
        };

        let input_json = serde_json::json!({
            "raw_objective": raw_objective,
            "available_capabilities": available_capabilities,
        });
        let input_summary = format!(
            "Bootstrap agenda with {} capabilities",
            available_capabilities.len()
        );

        let ctx = self.trace_emitter.start_call(
            "conductor",
            "ConductorBootstrapAgenda",
            "conductor-policy",
            model_used,
            provider,
            &system_context,
            &input_json,
            &input_summary,
            Some(LlmCallScope {
                run_id: run_id.map(ToString::to_string),
                task_id: run_id.map(ToString::to_string),
                call_id: None,
                session_id: None,
                thread_id: None,
            }),
        );

        let result = B
            .ConductorBootstrapAgenda
            .with_client_registry(&client_registry)
            .call(&input)
            .await;

        match &result {
            Ok(output) => {
                let output_json = serde_json::json!({
                    "dispatch_capabilities": output.dispatch_capabilities,
                    "block_reason": output.block_reason,
                    "rationale": output.rationale,
                    "confidence": output.confidence,
                });
                self.trace_emitter.complete_call(
                    &ctx,
                    model_used,
                    provider,
                    &output_json,
                    "Bootstrap completed",
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call(
                    &ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                );
            }
        }

        result.map_err(|e| ConductorError::PolicyError(format!("Bootstrap agenda failed: {e}")))
    }

    async fn decide_next_action(
        &self,
        run: &shared_types::ConductorRunState,
        _available_capabilities: &[String],
    ) -> Result<ConductorDecision, ConductorError> {
        let (client_registry, resolved) = self.resolve_for_role("conductor")?;
        let model_used = resolved.config.id.as_str();
        let provider = Some(Self::provider_string(&resolved.config.provider));

        let input = Self::build_decision_input(run);
        let system_context = input.objective.clone();

        let scope = LlmCallScope {
            run_id: Some(run.run_id.clone()),
            task_id: Some(run.task_id.clone()),
            ..Default::default()
        };

        let input_json = serde_json::json!({
            "run_id": run.run_id,
            "objective": system_context,
            "document_path": run.document_path,
        });
        let input_summary = format!("Decide next action for run {}", run.run_id);

        let ctx = self.trace_emitter.start_call(
            "conductor",
            "ConductorDecide",
            "conductor-policy",
            model_used,
            provider,
            &system_context,
            &input_json,
            &input_summary,
            Some(scope),
        );

        let result = B
            .ConductorDecide
            .with_client_registry(&client_registry)
            .call(&input)
            .await;

        match &result {
            Ok(output) => {
                let output_json = serde_json::json!({
                    "action": format!("{:?}", output.action),
                    "args": output.args,
                    "reason": output.reason,
                });
                self.trace_emitter.complete_call(
                    &ctx,
                    model_used,
                    provider,
                    &output_json,
                    "Decision completed",
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call(
                    &ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                );
            }
        }

        result.map_err(|e| ConductorError::PolicyError(format!("BAML policy call failed: {e}")))
    }

    async fn refine_objective_for_capability(
        &self,
        raw_objective: &str,
        capability: &str,
    ) -> Result<ConductorObjectiveRefineOutput, ConductorError> {
        let (client_registry, resolved) = self.resolve_for_role("conductor")?;
        let model_used = resolved.config.id.as_str();
        let provider = Some(Self::provider_string(&resolved.config.provider));

        let input = ConductorObjectiveRefineInput {
            raw_objective: raw_objective.to_string(),
            context: vec![],
            target_capability: capability.to_string(),
        };

        let system_context = format!("Refine objective for capability: {capability}");
        let input_json = serde_json::json!({
            "raw_objective": raw_objective,
            "context": [],
            "target_capability": capability,
        });
        let input_summary = format!("Refine objective for {capability}");

        let ctx = self.trace_emitter.start_call(
            "conductor",
            "ConductorRefineObjective",
            "conductor-policy",
            model_used,
            provider,
            &system_context,
            &input_json,
            &input_summary,
            None,
        );

        let result = B
            .ConductorRefineObjective
            .with_client_registry(&client_registry)
            .call(&input)
            .await;

        match &result {
            Ok(output) => {
                let output_json = serde_json::json!({
                    "refined_objective": output.refined_objective,
                    "success_criteria": output.success_criteria,
                    "estimated_steps": output.estimated_steps,
                    "confidence": output.confidence,
                });
                self.trace_emitter.complete_call(
                    &ctx,
                    model_used,
                    provider,
                    &output_json,
                    "Refine completed",
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call(
                    &ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                );
            }
        }

        result.map_err(|e| ConductorError::PolicyError(format!("Objective refine failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ractor::Actor;
    use shared_types::{ConductorOutputMode, ConductorRunState, ConductorRunStatus};

    use super::{BamlConductorPolicy, CAPABILITY_ROUTING_GUIDANCE};
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};

    fn sample_run_state(objective: &str) -> ConductorRunState {
        ConductorRunState {
            run_id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            objective: objective.to_string(),
            status: ConductorRunStatus::Running,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "conductor/runs/run-1/draft.md".to_string(),
            output_mode: ConductorOutputMode::Auto,
            desktop_id: "desktop-1".to_string(),
            correlation_id: "corr-1".to_string(),
        }
    }

    #[test]
    fn test_build_decision_input_appends_capability_routing_guidance() {
        let run = sample_run_state("Research latest Rust release notes");
        let input = BamlConductorPolicy::build_decision_input(&run);

        assert!(input
            .objective
            .contains("Research latest Rust release notes"));
        assert!(input.objective.contains(CAPABILITY_ROUTING_GUIDANCE));
    }

    #[tokio::test]
    async fn test_policy_constructor_wires_trace_emitter() {
        let (event_store, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let policy = BamlConductorPolicy::new(event_store);
        let _ = policy.trace_emitter.clone();
    }

    #[test]
    fn test_provider_string_returns_correct_values() {
        use crate::actors::model_config::ProviderConfig;
        use std::collections::HashMap;

        let bedrock = ProviderConfig::AwsBedrock {
            model: "test".to_string(),
            region: "us-east-1".to_string(),
        };
        assert_eq!(
            BamlConductorPolicy::provider_string(&bedrock),
            "aws-bedrock"
        );

        let anthropic = ProviderConfig::AnthropicCompatible {
            base_url: "https://example.com".to_string(),
            api_key_env: "API_KEY".to_string(),
            model: "test".to_string(),
            headers: HashMap::new(),
        };
        assert_eq!(
            BamlConductorPolicy::provider_string(&anthropic),
            "anthropic"
        );

        let openai = ProviderConfig::OpenAiGeneric {
            base_url: "https://example.com".to_string(),
            api_key_env: "API_KEY".to_string(),
            model: "test".to_string(),
            headers: HashMap::new(),
        };
        assert_eq!(
            BamlConductorPolicy::provider_string(&openai),
            "openai-generic"
        );
    }

    #[tokio::test]
    async fn test_resolve_for_role_returns_model_info() {
        let (event_store, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();
        let policy = BamlConductorPolicy::new(event_store);
        let result = policy.resolve_for_role("conductor");
        assert!(result.is_ok());

        let (_registry, resolved) = result.unwrap();
        assert!(!resolved.config.id.is_empty());
    }
}
