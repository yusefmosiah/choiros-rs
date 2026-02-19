//! Conductor actor shell.
//!
//! This file intentionally stays thin: message matching + dependency wiring.
//! Runtime logic lives in `conductor/runtime/*`.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::sync::Arc;

use crate::actors::conductor::{
    model_gateway::{BamlConductorModelGateway, SharedConductorModelGateway},
    protocol::ConductorMsg,
    registry,
    state::ConductorState as RunStateStore,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::writer::{WriterError, WriterMsg};
use crate::supervisor::writer::WriterSupervisorMsg;

/// ConductorActor - main orchestration actor.
#[derive(Debug, Default)]
pub struct ConductorActor;

/// Arguments for spawning ConductorActor.
#[derive(Clone)]
pub struct ConductorArguments {
    /// Event store actor reference for persistence.
    pub event_store: ActorRef<EventStoreMsg>,
    /// Optional writer supervisor for run-scoped writer lifecycle.
    pub writer_supervisor: Option<ActorRef<WriterSupervisorMsg>>,
    /// Optional memory actor for context retrieval (Phase 5.4).
    pub memory_actor: Option<ActorRef<crate::actors::memory::MemoryMsg>>,
}

/// Internal state for ConductorActor.
pub struct ConductorState {
    pub(crate) tasks: RunStateStore,
    pub(crate) event_store: ActorRef<EventStoreMsg>,
    pub(crate) writer_supervisor: Option<ActorRef<WriterSupervisorMsg>>,
    pub(crate) memory_actor: Option<ActorRef<crate::actors::memory::MemoryMsg>>,
    pub(crate) model_gateway: SharedConductorModelGateway,
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
        let model_gateway = Arc::new(BamlConductorModelGateway::new(args.event_store.clone()));
        Ok(ConductorState {
            tasks: RunStateStore::new(),
            event_store: args.event_store,
            writer_supervisor: args.writer_supervisor,
            memory_actor: args.memory_actor,
            model_gateway,
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "ConductorActor started successfully");
        // Phase 4.4 — restore any in-progress run states from the event store.
        self.restore_run_states(state).await;
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
            ConductorMsg::StartRun { run_id, request } => {
                self.handle_start_run(&myself, state, run_id, request)
                    .await?;
            }
            ConductorMsg::GetRunState { run_id, reply } => {
                let _ = reply.send(state.tasks.get_run(&run_id).cloned());
            }
            ConductorMsg::CapabilityCallFinished {
                run_id,
                call_id,
                agenda_item_id,
                capability,
                result,
            } => {
                self.handle_capability_call_finished(
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
                self.handle_process_event(state, run_id, event_type, payload, metadata)
                    .await?;
            }
            ConductorMsg::SubmitUserPrompt {
                run_id,
                prompt_diff,
                base_version_id,
                reply,
            } => {
                let result = self
                    .handle_submit_user_prompt(state, run_id, prompt_diff, base_version_id)
                    .await;
                let _ = reply.send(result);
            }

            // Phase 4 — ActorHarnessActor completion messages.
            ConductorMsg::ActorHarnessComplete {
                correlation_id,
                result,
            } => {
                self.handle_actor_harness_complete(state, correlation_id, result)
                    .await?;
            }
            ConductorMsg::ActorHarnessFailed {
                correlation_id,
                reason,
            } => {
                self.handle_actor_harness_failed(state, correlation_id, reason)
                    .await?;
            }

            // Phase 4 — ActorHarnessActor in-flight progress.
            ConductorMsg::ActorHarnessProgress {
                correlation_id,
                kind,
                content,
                metadata,
            } => {
                self.handle_actor_harness_progress(state, correlation_id, kind, content, metadata)
                    .await;
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
    pub(crate) async fn resolve_writer_actor_for_run(
        &self,
        state: &ConductorState,
        run_id: &str,
    ) -> Result<ActorRef<WriterMsg>, crate::actors::conductor::ConductorError> {
        if let Some(writer_actor) = registry::lookup_writer_actor_for_run(run_id) {
            return Ok(writer_actor);
        }
        if let Some(writer_supervisor) = state.writer_supervisor.as_ref() {
            let writer_id = registry::run_writer_id(run_id);
            let result = ractor::call!(writer_supervisor, |reply| {
                WriterSupervisorMsg::GetOrCreateWriter {
                    writer_id,
                    user_id: "system".to_string(),
                    reply,
                }
            })
            .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
            .map_err(crate::actors::conductor::ConductorError::ActorUnavailable)?;
            return Ok(result);
        }
        if let Some(writer_actor) = registry::lookup_writer_actor() {
            return Ok(writer_actor);
        }
        Err(crate::actors::conductor::ConductorError::ActorUnavailable(
            "writer actor unavailable".to_string(),
        ))
    }

    fn map_writer_error(error: WriterError) -> crate::actors::conductor::ConductorError {
        match error {
            WriterError::Validation(message) => {
                if message.contains("document runtime not found") {
                    crate::actors::conductor::ConductorError::NotFound(message)
                } else {
                    crate::actors::conductor::ConductorError::InvalidRequest(message)
                }
            }
            WriterError::ActorUnavailable(message) => {
                crate::actors::conductor::ConductorError::ActorUnavailable(message)
            }
            other => crate::actors::conductor::ConductorError::WorkerFailed(other.to_string()),
        }
    }

    async fn handle_submit_user_prompt(
        &self,
        state: &mut ConductorState,
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
    ) -> Result<crate::actors::writer::WriterQueueAck, crate::actors::conductor::ConductorError>
    {
        let writer_actor = self.resolve_writer_actor_for_run(state, &run_id).await?;
        ractor::call!(writer_actor, |reply| WriterMsg::SubmitUserPrompt {
            run_id,
            prompt_diff,
            base_version_id,
            reply
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(Self::map_writer_error)
    }

    async fn handle_process_event(
        &self,
        state: &mut ConductorState,
        run_id: String,
        event_type: String,
        payload: serde_json::Value,
        metadata: shared_types::EventMetadata,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!(
            run_id = %run_id,
            event_type = %event_type,
            lane = ?metadata.lane,
            "Processing event"
        );

        if let Some(metadata_run_id) = metadata.run_id.as_ref() {
            if metadata_run_id != &run_id {
                tracing::warn!(
                    run_id = %run_id,
                    metadata_run_id = %metadata_run_id,
                    event_type = %event_type,
                    "Ignoring event with mismatched run provenance"
                );
                return Ok(());
            }
        }

        if state.tasks.get_run(&run_id).is_none() {
            tracing::debug!(
                run_id = %run_id,
                event_type = %event_type,
                "Ignoring event for unknown run"
            );
            return Ok(());
        }

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
                "category": "event_signal",
            })),
        };
        if let Err(err) = state.tasks.add_artifact(&run_id, event_artifact) {
            tracing::warn!(run_id = %run_id, error = %err, "Failed to persist event artifact");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ConductorActor, ConductorState};
    use crate::actors::conductor::model_gateway::{
        ConductorModelGateway, SharedConductorModelGateway,
    };
    use crate::actors::conductor::protocol::ConductorError;
    use crate::actors::conductor::state::ConductorState as RunStateStore;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
    use crate::baml_client::types::ConductorBootstrapOutput;
    use async_trait::async_trait;
    use ractor::Actor;
    use shared_types::{
        ConductorOutputMode, ConductorRunState, ConductorRunStatus, EventImportance, EventLane,
        EventMetadata,
    };
    use std::sync::Arc;

    #[derive(Default)]
    struct CountingGateway;

    #[async_trait]
    impl ConductorModelGateway for CountingGateway {
        async fn conduct_assignments(
            &self,
            _run_id: Option<&str>,
            _raw_objective: &str,
            _available_capabilities: &[String],
        ) -> Result<ConductorBootstrapOutput, ConductorError> {
            Err(ConductorError::ModelGatewayError(
                "conduct_assignments should not be called in handle_process_event tests"
                    .to_string(),
            ))
        }

        async fn immediate_response(
            &self,
            _run_id: Option<&str>,
            _objective: &str,
        ) -> Result<String, ConductorError> {
            Err(ConductorError::ModelGatewayError(
                "immediate_response should not be called in handle_process_event tests".to_string(),
            ))
        }
    }

    fn test_run(run_id: &str) -> ConductorRunState {
        let now = chrono::Utc::now();
        ConductorRunState {
            run_id: run_id.to_string(),
            objective: "test objective".to_string(),
            status: ConductorRunStatus::Running,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "/tmp/test-draft.md".to_string(),
            output_mode: ConductorOutputMode::Auto,
            desktop_id: "desktop-test".to_string(),
        }
    }

    async fn test_state_with_gateway(
        gateway: SharedConductorModelGateway,
    ) -> (ConductorState, ractor::ActorRef<EventStoreMsg>) {
        let (event_store, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        (
            ConductorState {
                tasks: RunStateStore::new(),
                event_store: event_store.clone(),
                writer_supervisor: None,
                memory_actor: None,
                model_gateway: gateway,
            },
            event_store,
        )
    }

    #[tokio::test]
    async fn test_process_event_control_lane_persists_artifact_without_model_decision() {
        let gateway = Arc::new(CountingGateway::default());
        let (mut state, event_store) = test_state_with_gateway(gateway.clone()).await;
        state
            .tasks
            .insert_run(test_run("run_control_artifact_only"));

        let metadata = EventMetadata {
            lane: EventLane::Control,
            importance: EventImportance::High,
            run_id: Some("run_control_artifact_only".to_string()),
            call_id: Some("call_1".to_string()),
            capability: Some("terminal".to_string()),
            phase: Some("completion".to_string()),
        };

        let actor = ConductorActor;
        actor
            .handle_process_event(
                &mut state,
                "run_control_artifact_only".to_string(),
                "conductor.capability.completed".to_string(),
                serde_json::json!({
                    "call_id": "call_1",
                    "summary": "Command completed"
                }),
                metadata,
            )
            .await
            .unwrap();

        let run = state.tasks.get_run("run_control_artifact_only").unwrap();
        assert_eq!(run.status, ConductorRunStatus::Running);
        assert_eq!(run.artifacts.len(), 1);
        assert!(run.decision_log.is_empty());

        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_process_event_mismatched_provenance_is_ignored() {
        let gateway = Arc::new(CountingGateway::default());
        let (mut state, event_store) = test_state_with_gateway(gateway.clone()).await;
        state.tasks.insert_run(test_run("run_provenance_test"));

        let metadata = EventMetadata {
            lane: EventLane::Control,
            importance: EventImportance::High,
            run_id: Some("other_run".to_string()),
            call_id: Some("call_1".to_string()),
            capability: Some("terminal".to_string()),
            phase: Some("completion".to_string()),
        };

        let actor = ConductorActor;
        actor
            .handle_process_event(
                &mut state,
                "run_provenance_test".to_string(),
                "conductor.capability.completed".to_string(),
                serde_json::json!({ "call_id": "call_1" }),
                metadata,
            )
            .await
            .unwrap();

        let run = state.tasks.get_run("run_provenance_test").unwrap();
        assert!(run.artifacts.is_empty());
        assert!(run.decision_log.is_empty());
        assert_eq!(run.status, ConductorRunStatus::Running);

        event_store.stop(None);
    }
}
