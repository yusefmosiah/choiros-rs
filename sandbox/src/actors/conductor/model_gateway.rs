//! Conductor model-gateway authority and BAML integration.
//!
//! This module owns model resolution + LLM gateway calls so the actor can stay
//! focused on state transitions and message coordination.

use async_trait::async_trait;
use std::sync::Arc;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::event_store::EventStoreMsg;
use crate::actors::model_config::{
    ModelRegistry, ModelResolutionContext, ProviderConfig, ResolvedModel,
};
use crate::baml_client::types::{ConductorBootstrapInput, ConductorBootstrapOutput};
use crate::baml_client::{new_collector, ClientRegistry, B};
use crate::observability::llm_trace::{token_usage_from_collector, LlmCallScope, LlmTraceEmitter};

pub type SharedConductorModelGateway = Arc<dyn ConductorModelGateway>;

const CAPABILITY_ROUTING_GUIDANCE: &str = "Capability routing guidance:\n- Use researcher for external information gathering, web search, URL fetch, citations, source synthesis, and current-events/news questions.\n- Use terminal for local shell/file/system execution and codebase research (build, test, inspect code/docs, architecture analysis, edit files, run local commands).\n- If objective needs both codebase evidence and external evidence, dispatch both terminal and researcher capabilities.\n- Never route web/news/current-events objectives to terminal when researcher is available.";

#[async_trait]
pub trait ConductorModelGateway: Send + Sync {
    async fn conduct_assignments(
        &self,
        run_id: Option<&str>,
        raw_objective: &str,
        available_capabilities: &[String],
    ) -> Result<ConductorBootstrapOutput, ConductorError>;
}

#[derive(Debug)]
pub struct BamlConductorModelGateway {
    registry: ModelRegistry,
    trace_emitter: LlmTraceEmitter,
}

impl BamlConductorModelGateway {
    pub fn new(event_store: ractor::ActorRef<EventStoreMsg>) -> Self {
        Self {
            registry: ModelRegistry::new(),
            trace_emitter: LlmTraceEmitter::new(event_store),
        }
    }

    fn resolve_for_callsite(
        &self,
        callsite: &str,
    ) -> Result<(ClientRegistry, ResolvedModel), ConductorError> {
        let resolved = self
            .registry
            .resolve_for_callsite(callsite, &ModelResolutionContext::default())
            .map_err(|e| {
                ConductorError::ModelGatewayError(format!("Model resolution failed: {e}"))
            })?;

        let client_registry = self
            .registry
            .create_runtime_client_registry_for_model(&resolved.config.id)
            .map_err(|e| {
                ConductorError::ModelGatewayError(format!("Client registry creation failed: {e}"))
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
}

#[async_trait]
impl ConductorModelGateway for BamlConductorModelGateway {
    async fn conduct_assignments(
        &self,
        run_id: Option<&str>,
        raw_objective: &str,
        available_capabilities: &[String],
    ) -> Result<ConductorBootstrapOutput, ConductorError> {
        let (client_registry, resolved) = self.resolve_for_callsite("conductor")?;
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
            "Conduct run assignments with {} capabilities",
            available_capabilities.len()
        );

        let ctx = self.trace_emitter.start_call(
            "conductor",
            "ConductorBootstrapAgenda",
            "conductor-model-gateway",
            model_used,
            provider,
            &system_context,
            &input_json,
            &input_summary,
            Some(LlmCallScope {
                run_id: run_id.map(ToString::to_string),
                task_id: None,
                call_id: None,
                session_id: None,
                thread_id: None,
            }),
        );

        let collector = new_collector("conductor.bootstrap_agenda");
        let result = B
            .ConductorBootstrapAgenda
            .with_client_registry(&client_registry)
            .with_collector(&collector)
            .call(&input)
            .await;
        let usage = token_usage_from_collector(&collector);

        match &result {
            Ok(output) => {
                let output_json = serde_json::json!({
                    "dispatch_capabilities": output.dispatch_capabilities,
                    "block_reason": output.block_reason,
                    "rationale": output.rationale,
                    "confidence": output.confidence,
                });
                self.trace_emitter.complete_call_with_usage(
                    &ctx,
                    model_used,
                    provider,
                    &output_json,
                    "Conduct assignments completed",
                    usage.clone(),
                );
            }
            Err(e) => {
                self.trace_emitter.fail_call_with_usage(
                    &ctx,
                    model_used,
                    provider,
                    None,
                    &e.to_string(),
                    None,
                    usage,
                );
            }
        }

        result.map_err(|e| {
            ConductorError::ModelGatewayError(format!(
                "Conduct assignments model-gateway call failed: {e}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use ractor::Actor;

    use super::BamlConductorModelGateway;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};

    #[tokio::test]
    async fn test_model_gateway_constructor_wires_trace_emitter() {
        let (event_store, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let model_gateway = BamlConductorModelGateway::new(event_store);
        let _ = model_gateway.trace_emitter.clone();
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
            BamlConductorModelGateway::provider_string(&bedrock),
            "aws-bedrock"
        );

        let anthropic = ProviderConfig::AnthropicCompatible {
            base_url: "https://example.com".to_string(),
            api_key_env: "API_KEY".to_string(),
            model: "test".to_string(),
            headers: HashMap::new(),
        };
        assert_eq!(
            BamlConductorModelGateway::provider_string(&anthropic),
            "anthropic"
        );

        let openai = ProviderConfig::OpenAiGeneric {
            base_url: "https://example.com".to_string(),
            api_key_env: "API_KEY".to_string(),
            model: "test".to_string(),
            headers: HashMap::new(),
        };
        assert_eq!(
            BamlConductorModelGateway::provider_string(&openai),
            "openai-generic"
        );
    }

    #[tokio::test]
    async fn test_resolve_for_callsite_returns_model_info() {
        let (event_store, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();
        let model_gateway = BamlConductorModelGateway::new(event_store);
        let result = model_gateway.resolve_for_callsite("conductor");
        assert!(result.is_ok());

        let (_registry, resolved) = result.unwrap();
        assert!(!resolved.config.id.is_empty());
    }
}
