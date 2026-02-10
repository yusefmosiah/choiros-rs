use std::sync::Arc;

use async_trait::async_trait;
use ractor::{Actor, ActorRef};

use crate::actors::conductor::policy::ConductorPolicy;
use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorError, ConductorMsg};
use crate::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
use crate::baml_client::types::{
    ConductorDecisionOutput, ConductorObjectiveRefineOutput, DecisionType,
};

#[derive(Debug)]
pub(crate) struct TestPolicy;

#[async_trait]
impl ConductorPolicy for TestPolicy {
    async fn decide_next_action(
        &self,
        _run: &shared_types::ConductorRunState,
        _available_capabilities: &[String],
    ) -> Result<ConductorDecisionOutput, ConductorError> {
        Ok(ConductorDecisionOutput {
            decision_type: DecisionType::Continue,
            target_agenda_item_ids: vec![],
            new_agenda_items: vec![],
            capability: None,
            objective: None,
            retry_policy: None,
            completion_reason: None,
            confidence: 1.0,
            rationale: "test policy continue".to_string(),
        })
    }

    async fn refine_objective_for_capability(
        &self,
        raw_objective: &str,
        capability: &str,
    ) -> Result<ConductorObjectiveRefineOutput, ConductorError> {
        Ok(ConductorObjectiveRefineOutput {
            refined_objective: format!("{capability}: {raw_objective}"),
            success_criteria: vec!["test".to_string()],
            estimated_steps: 1,
            confidence: 1.0,
        })
    }
}

pub(crate) async fn setup_test_conductor(
    researcher_actor: Option<ActorRef<ResearcherMsg>>,
    terminal_actor: Option<ActorRef<TerminalMsg>>,
) -> (ActorRef<ConductorMsg>, ActorRef<EventStoreMsg>) {
    let (store_ref, _store_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let args = ConductorArguments {
        event_store: store_ref.clone(),
        researcher_actor,
        terminal_actor,
        policy: Some(Arc::new(TestPolicy)),
    };

    let (conductor_ref, _conductor_handle) =
        Actor::spawn(None, ConductorActor, args).await.unwrap();
    (conductor_ref, store_ref)
}
