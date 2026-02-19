//! ActorHarnessActor — scoped sub-agent executor for Conductor delegation.
//!
//! A ActorHarnessActor is a one-shot actor:
//! 1. Receives a single `ActorHarnessMsg::Execute`.
//! 2. Runs `AgentHarness` with `HarnessProfile::ActorHarness`.
//! 3. Emits `actor_harness.execute` and `actor_harness.result` events.
//! 4. Sends typed `ConductorMsg::ActorHarnessComplete` (or `ActorHarnessFailed`) back to Conductor.
//! 5. Stops itself.
//!
//! The actor never calls back into Conductor other than through those two
//! message variants, keeping the contract bounded and unambiguous.

mod adapter;

pub use adapter::ActorHarnessAdapter;

use async_trait::async_trait;
use chrono::Utc;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::agent_harness::{AgentHarness, HarnessProfile, ObjectiveStatus};
use crate::actors::conductor::protocol::{ConductorMsg, ActorHarnessResult};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::observability::llm_trace::LlmTraceEmitter;

// ─── Public re-exports for use-sites ───────────────────────────────────────

pub use crate::actors::conductor::protocol::ActorHarnessMsg;

// ─── Actor shell ───────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ActorHarnessActor;

/// Arguments used when spawning a `ActorHarnessActor`.
#[derive(Clone)]
pub struct ActorHarnessArguments {
    pub event_store: ActorRef<EventStoreMsg>,
}

/// Internal actor state (minimal — all runtime data arrives in the message).
pub struct ActorHarnessState {
    pub(crate) event_store: ActorRef<EventStoreMsg>,
}

#[async_trait]
impl Actor for ActorHarnessActor {
    type Msg = ActorHarnessMsg;
    type State = ActorHarnessState;
    type Arguments = ActorHarnessArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ActorHarnessState {
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
            ActorHarnessMsg::Execute {
                objective,
                context,
                correlation_id,
                reply_to,
            } => {
                emit_actor_harness_execute(&state.event_store, &correlation_id, &objective, &context)
                    .await;

                let result = run_actor_harness(
                    state,
                    objective.clone(),
                    context,
                    correlation_id.clone(),
                    reply_to.clone(),
                )
                .await;

                match result {
                    Ok(subharness_result) => {
                        emit_actor_harness_result(
                            &state.event_store,
                            &correlation_id,
                            &objective,
                            &subharness_result,
                        )
                        .await;
                        let _ = reply_to.send_message(ConductorMsg::ActorHarnessComplete {
                            correlation_id,
                            result: subharness_result,
                        });
                    }
                    Err(reason) => {
                        let _ = reply_to.send_message(ConductorMsg::ActorHarnessFailed {
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

async fn run_actor_harness(
    state: &ActorHarnessState,
    objective: String,
    context: serde_json::Value,
    correlation_id: String,
    conductor: ActorRef<ConductorMsg>,
) -> Result<ActorHarnessResult, String> {
    let model_registry = ModelRegistry::new();
    let trace_emitter = LlmTraceEmitter::new(state.event_store.clone());
    let config = HarnessProfile::ActorHarness.default_config();

    let adapter = ActorHarnessAdapter::new(
        state.event_store.clone(),
        conductor,
        correlation_id.clone(),
        context.clone(),
    );

    let harness = AgentHarness::with_config(adapter, model_registry, config, trace_emitter);

    let agent_result = harness
        .run(
            format!("actor_harness:{}", correlation_id),
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

    Ok(ActorHarnessResult {
        output: agent_result.summary,
        citations: vec![],
        objective_satisfied,
        completion_reason: Some(agent_result.completion_reason),
        steps_taken: agent_result.steps_taken as u32,
    })
}

// ─── Event emission ─────────────────────────────────────────────────────────

async fn emit_actor_harness_execute(
    event_store: &ActorRef<EventStoreMsg>,
    correlation_id: &str,
    objective: &str,
    context: &serde_json::Value,
) {
    let payload = serde_json::json!({
        "corr_id": correlation_id,
        "objective": objective,
        "context_keys": context.as_object().map(|m| m.keys().collect::<Vec<_>>()).unwrap_or_default(),
        "timestamp": Utc::now().to_rfc3339(),
    });
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "actor_harness.execute".to_string(),
            payload,
            actor_id: format!("actor_harness:{}", correlation_id),
            user_id: "system".to_string(),
        },
    });
}

async fn emit_actor_harness_result(
    event_store: &ActorRef<EventStoreMsg>,
    correlation_id: &str,
    objective: &str,
    result: &ActorHarnessResult,
) {
    let payload = serde_json::json!({
        "corr_id": correlation_id,
        "objective": objective,
        "objective_satisfied": result.objective_satisfied,
        "steps_taken": result.steps_taken,
        "completion_reason": result.completion_reason,
        "output_excerpt": result.output.chars().take(300).collect::<String>(),
        "timestamp": Utc::now().to_rfc3339(),
    });
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "actor_harness.result".to_string(),
            payload,
            actor_id: format!("actor_harness:{}", correlation_id),
            user_id: "system".to_string(),
        },
    });
}
