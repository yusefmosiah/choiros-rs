//! SubharnessActor — scoped sub-agent executor for Conductor delegation.
//!
//! A SubharnessActor is a one-shot actor:
//! 1. Receives a single `SubharnessMsg::Execute`.
//! 2. Runs `AgentHarness` with `HarnessProfile::Subharness`.
//! 3. Emits `subharness.execute` and `subharness.result` events.
//! 4. Sends typed `ConductorMsg::SubharnessComplete` (or `SubharnessFailed`) back to Conductor.
//! 5. Stops itself.
//!
//! The actor never calls back into Conductor other than through those two
//! message variants, keeping the contract bounded and unambiguous.

mod adapter;

pub use adapter::SubharnessAdapter;

use async_trait::async_trait;
use chrono::Utc;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::agent_harness::{AgentHarness, HarnessProfile, ObjectiveStatus};
use crate::actors::conductor::protocol::{ConductorMsg, SubharnessResult};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::observability::llm_trace::LlmTraceEmitter;

// ─── Public re-exports for use-sites ───────────────────────────────────────

pub use crate::actors::conductor::protocol::SubharnessMsg;

// ─── Actor shell ───────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct SubharnessActor;

/// Arguments used when spawning a `SubharnessActor`.
#[derive(Clone)]
pub struct SubharnessArguments {
    pub event_store: ActorRef<EventStoreMsg>,
}

/// Internal actor state (minimal — all runtime data arrives in the message).
pub struct SubharnessState {
    pub(crate) event_store: ActorRef<EventStoreMsg>,
}

#[async_trait]
impl Actor for SubharnessActor {
    type Msg = SubharnessMsg;
    type State = SubharnessState;
    type Arguments = SubharnessArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(SubharnessState {
            event_store: args.event_store,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SubharnessMsg::Execute {
                objective,
                context,
                correlation_id,
                reply_to,
            } => {
                emit_subharness_execute(
                    &state.event_store,
                    &correlation_id,
                    &objective,
                    &context,
                )
                .await;

                let result = run_subharness(
                    state,
                    objective.clone(),
                    context,
                    correlation_id.clone(),
                    reply_to.clone(),
                )
                .await;

                match result {
                    Ok(subharness_result) => {
                        emit_subharness_result(
                            &state.event_store,
                            &correlation_id,
                            &objective,
                            &subharness_result,
                        )
                        .await;
                        let _ = reply_to.send_message(ConductorMsg::SubharnessComplete {
                            correlation_id,
                            result: subharness_result,
                        });
                    }
                    Err(reason) => {
                        let _ = reply_to.send_message(ConductorMsg::SubharnessFailed {
                            correlation_id,
                            reason,
                        });
                    }
                }

                myself.stop(None);
            }
        }
        Ok(())
    }
}

// ─── Harness execution ──────────────────────────────────────────────────────

async fn run_subharness(
    state: &SubharnessState,
    objective: String,
    context: serde_json::Value,
    correlation_id: String,
    conductor: ActorRef<ConductorMsg>,
) -> Result<SubharnessResult, String> {
    let model_registry = ModelRegistry::new();
    let trace_emitter = LlmTraceEmitter::new(state.event_store.clone());
    let config = HarnessProfile::Subharness.default_config();

    let adapter = SubharnessAdapter::new(
        state.event_store.clone(),
        conductor,
        correlation_id.clone(),
        context.clone(),
    );

    let harness = AgentHarness::with_config(adapter, model_registry, config, trace_emitter);

    let agent_result = harness
        .run(
            format!("subharness:{}", correlation_id),
            "system".to_string(),
            objective.clone(),
            None,
            None,
            None,
            Some(correlation_id.clone()),
        )
        .await
        .map_err(|e| e.to_string())?;

    let objective_satisfied = matches!(agent_result.objective_status, ObjectiveStatus::Complete);

    Ok(SubharnessResult {
        output: agent_result.summary,
        citations: vec![],
        objective_satisfied,
        completion_reason: Some(agent_result.completion_reason),
        steps_taken: agent_result.steps_taken as u32,
    })
}

// ─── Event emission ─────────────────────────────────────────────────────────

async fn emit_subharness_execute(
    event_store: &ActorRef<EventStoreMsg>,
    correlation_id: &str,
    objective: &str,
    context: &serde_json::Value,
) {
    let payload = serde_json::json!({
        "correlation_id": correlation_id,
        "objective": objective,
        "context_keys": context.as_object().map(|m| m.keys().collect::<Vec<_>>()).unwrap_or_default(),
        "timestamp": Utc::now().to_rfc3339(),
    });
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "subharness.execute".to_string(),
            payload,
            actor_id: format!("subharness:{}", correlation_id),
            user_id: "system".to_string(),
        },
    });
}

async fn emit_subharness_result(
    event_store: &ActorRef<EventStoreMsg>,
    correlation_id: &str,
    objective: &str,
    result: &SubharnessResult,
) {
    let payload = serde_json::json!({
        "correlation_id": correlation_id,
        "objective": objective,
        "objective_satisfied": result.objective_satisfied,
        "steps_taken": result.steps_taken,
        "completion_reason": result.completion_reason,
        "output_excerpt": result.output.chars().take(300).collect::<String>(),
        "timestamp": Utc::now().to_rfc3339(),
    });
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "subharness.result".to_string(),
            payload,
            actor_id: format!("subharness:{}", correlation_id),
            user_id: "system".to_string(),
        },
    });
}
