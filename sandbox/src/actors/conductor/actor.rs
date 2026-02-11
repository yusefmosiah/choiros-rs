//! Conductor actor shell.
//!
//! This file intentionally stays thin: message matching + dependency wiring.
//! Runtime logic lives in `conductor/runtime/*`.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::sync::Arc;

use crate::actors::conductor::{
    policy::{BamlConductorPolicy, SharedConductorPolicy},
    protocol::ConductorMsg,
    state::ConductorState as TaskState,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;

/// ConductorActor - main orchestration actor.
#[derive(Debug, Default)]
pub struct ConductorActor;

/// Arguments for spawning ConductorActor.
#[derive(Clone)]
pub struct ConductorArguments {
    /// Event store actor reference for persistence.
    pub event_store: ActorRef<EventStoreMsg>,
    /// Optional researcher actor for delegation.
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    /// Optional terminal actor for delegation.
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
    /// Optional policy override (tests/cutovers).
    pub policy: Option<SharedConductorPolicy>,
}

/// Internal state for ConductorActor.
pub struct ConductorState {
    pub(crate) tasks: TaskState,
    pub(crate) event_store: ActorRef<EventStoreMsg>,
    pub(crate) researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub(crate) terminal_actor: Option<ActorRef<TerminalMsg>>,
    pub(crate) policy: SharedConductorPolicy,
}

#[async_trait]
impl Actor for ConductorActor {
    type Msg = ConductorMsg;
    type State = ConductorState;
    type Arguments = ConductorArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor starting");
        Ok(ConductorState {
            tasks: TaskState::new(),
            event_store: args.event_store,
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
            policy: args
                .policy
                .unwrap_or_else(|| Arc::new(BamlConductorPolicy::new())),
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor started successfully");
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ConductorMsg::ExecuteTask { request, reply } => {
                let _ = reply.send(self.handle_execute_task(myself, state, request).await);
            }
            ConductorMsg::GetTaskState { task_id, reply } => {
                let _ = reply.send(state.tasks.get_task(&task_id).cloned());
            }
            ConductorMsg::CapabilityCallFinished {
                run_id,
                call_id,
                agenda_item_id,
                capability,
                result,
            } => {
                self.handle_capability_call_finished(
                    &myself,
                    state,
                    run_id,
                    call_id,
                    agenda_item_id,
                    capability,
                    result,
                )
                .await?;
            }
            ConductorMsg::ProcessEvent {
                run_id,
                event_type,
                payload,
                metadata,
            } => {
                self.handle_process_event(&myself, state, run_id, event_type, payload, metadata)
                    .await?;
            }
            ConductorMsg::DispatchReady { run_id } => {
                self.handle_dispatch_ready(&myself, state, &run_id).await?;
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor stopped");
        Ok(())
    }
}

impl ConductorActor {
    async fn handle_process_event(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: String,
        event_type: String,
        payload: serde_json::Value,
        metadata: shared_types::EventMetadata,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!(
            run_id = %run_id,
            event_type = %event_type,
            wake_policy = ?metadata.wake_policy,
            "Processing event"
        );

        if state.tasks.get_run(&run_id).is_some() {
            let event_artifact = shared_types::ConductorArtifact {
                artifact_id: ulid::Ulid::new().to_string(),
                kind: shared_types::ArtifactKind::JsonData,
                reference: format!("event://{}", event_type),
                mime_type: Some("application/json".to_string()),
                created_at: chrono::Utc::now(),
                source_call_id: metadata
                    .call_id
                    .clone()
                    .unwrap_or_else(|| "event".to_string()),
                metadata: Some(serde_json::json!({
                    "event_type": event_type,
                    "event_payload": payload,
                    "event_metadata": metadata,
                    "category": "wake_signal",
                })),
            };
            if let Err(err) = state.tasks.add_artifact(&run_id, event_artifact) {
                tracing::warn!(run_id = %run_id, error = %err, "Failed to persist wake event artifact");
            }
        }

        if metadata.wake_policy == shared_types::WakePolicy::Wake {
            match self.make_policy_decision(state, &run_id).await {
                Ok(decision) => {
                    if let Err(e) = self.apply_decision(myself, state, &run_id, decision).await {
                        tracing::error!(run_id = %run_id, error = %e, "Failed to apply decision");
                    }
                }
                Err(e) => {
                    self.emit_decision_failure(&run_id, &e.to_string()).await;
                    let _ = state
                        .tasks
                        .transition_run_status(&run_id, shared_types::ConductorRunStatus::Blocked);
                }
            }
        }

        Ok(())
    }
}
