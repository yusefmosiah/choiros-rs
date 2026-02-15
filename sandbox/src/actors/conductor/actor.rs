//! Conductor actor shell.
//!
//! This file intentionally stays thin: message matching + dependency wiring.
//! Runtime logic lives in `conductor/runtime/*`.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::collections::HashMap;
use std::sync::Arc;

use crate::actors::conductor::{
    model_gateway::{BamlConductorModelGateway, SharedConductorModelGateway},
    protocol::ConductorMsg,
    state::ConductorState as RunStateStore,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::run_writer::{
    DocumentVersion, Overlay, OverlayStatus, RunWriterMsg, VersionSource,
};
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::{WriterMsg, WriterSource};

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
    /// Optional writer actor for event-driven writer authority.
    pub writer_actor: Option<ActorRef<WriterMsg>>,
}

/// Internal state for ConductorActor.
pub struct ConductorState {
    pub(crate) tasks: RunStateStore,
    pub(crate) event_store: ActorRef<EventStoreMsg>,
    pub(crate) researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub(crate) terminal_actor: Option<ActorRef<TerminalMsg>>,
    pub(crate) writer_actor: Option<ActorRef<WriterMsg>>,
    pub(crate) model_gateway: SharedConductorModelGateway,
    pub(crate) run_writers: HashMap<String, ActorRef<RunWriterMsg>>,
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
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
            writer_actor: args.writer_actor,
            model_gateway,
            run_writers: HashMap::new(),
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
            ConductorMsg::ListRunWriterVersions { run_id, reply } => {
                let result = self.handle_list_run_writer_versions(state, run_id).await;
                let _ = reply.send(result);
            }
            ConductorMsg::GetRunWriterVersion {
                run_id,
                version_id,
                reply,
            } => {
                let result = self
                    .handle_get_run_writer_version(state, run_id, version_id)
                    .await;
                let _ = reply.send(result);
            }
            ConductorMsg::ListRunWriterOverlays {
                run_id,
                base_version_id,
                status,
                reply,
            } => {
                let result = self
                    .handle_list_run_writer_overlays(state, run_id, base_version_id, status)
                    .await;
                let _ = reply.send(result);
            }
            ConductorMsg::CreateRunWriterVersion {
                run_id,
                parent_version_id,
                content,
                source,
                reply,
            } => {
                let result = self
                    .handle_create_run_writer_version(
                        state,
                        run_id,
                        parent_version_id,
                        content,
                        source,
                    )
                    .await;
                let _ = reply.send(result);
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
    async fn handle_submit_user_prompt(
        &self,
        state: &mut ConductorState,
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
    ) -> Result<crate::actors::writer::WriterQueueAck, crate::actors::conductor::ConductorError>
    {
        if prompt_diff.is_empty() {
            return Err(crate::actors::conductor::ConductorError::InvalidRequest(
                "prompt_diff cannot be empty".to_string(),
            ));
        }

        let writer_actor = state.writer_actor.as_ref().ok_or_else(|| {
            crate::actors::conductor::ConductorError::ActorUnavailable(
                "writer actor unavailable".to_string(),
            )
        })?;
        let run_writer = state.run_writers.get(&run_id).cloned().ok_or_else(|| {
            crate::actors::conductor::ConductorError::NotFound(format!(
                "run writer not found for run_id={run_id}"
            ))
        })?;

        let head = ractor::call!(run_writer.clone(), |reply| RunWriterMsg::GetHeadVersion {
            reply
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| {
            crate::actors::conductor::ConductorError::WorkerFailed(format!(
                "failed to read run-writer head version: {e}"
            ))
        })?;
        if base_version_id != head.version_id {
            return Err(crate::actors::conductor::ConductorError::InvalidRequest(
                format!(
                    "stale base_version_id: expected {}, got {}",
                    head.version_id, base_version_id
                ),
            ));
        }

        let message_id = format!("{run_id}:user:prompt:{}", ulid::Ulid::new());
        let ack = ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
            message_id,
            kind: "human_prompt".to_string(),
            run_writer_actor: run_writer,
            run_id: run_id.clone(),
            section_id: "user".to_string(),
            source: WriterSource::User,
            content: "user_prompt_diff".to_string(),
            base_version_id: Some(base_version_id),
            prompt_diff: Some(prompt_diff),
            overlay_id: None,
            reply,
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| {
            crate::actors::conductor::ConductorError::WorkerFailed(format!(
                "writer enqueue failed: {e}"
            ))
        })?;

        Ok(ack)
    }

    async fn handle_list_run_writer_versions(
        &self,
        state: &mut ConductorState,
        run_id: String,
    ) -> Result<Vec<DocumentVersion>, crate::actors::conductor::ConductorError> {
        let run_writer = state.run_writers.get(&run_id).cloned().ok_or_else(|| {
            crate::actors::conductor::ConductorError::NotFound(format!(
                "run writer not found for run_id={run_id}"
            ))
        })?;
        ractor::call!(run_writer, |reply| RunWriterMsg::ListVersions { reply })
            .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
            .map_err(|e| crate::actors::conductor::ConductorError::WorkerFailed(e.to_string()))
    }

    async fn handle_get_run_writer_version(
        &self,
        state: &mut ConductorState,
        run_id: String,
        version_id: u64,
    ) -> Result<DocumentVersion, crate::actors::conductor::ConductorError> {
        let run_writer = state.run_writers.get(&run_id).cloned().ok_or_else(|| {
            crate::actors::conductor::ConductorError::NotFound(format!(
                "run writer not found for run_id={run_id}"
            ))
        })?;
        ractor::call!(run_writer, |reply| RunWriterMsg::GetVersion {
            version_id,
            reply
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| crate::actors::conductor::ConductorError::WorkerFailed(e.to_string()))
    }

    async fn handle_list_run_writer_overlays(
        &self,
        state: &mut ConductorState,
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
    ) -> Result<Vec<Overlay>, crate::actors::conductor::ConductorError> {
        let run_writer = state.run_writers.get(&run_id).cloned().ok_or_else(|| {
            crate::actors::conductor::ConductorError::NotFound(format!(
                "run writer not found for run_id={run_id}"
            ))
        })?;
        ractor::call!(run_writer, |reply| RunWriterMsg::ListOverlays {
            base_version_id,
            status,
            reply
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| crate::actors::conductor::ConductorError::WorkerFailed(e.to_string()))
    }

    async fn handle_create_run_writer_version(
        &self,
        state: &mut ConductorState,
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
    ) -> Result<DocumentVersion, crate::actors::conductor::ConductorError> {
        let run_writer = state.run_writers.get(&run_id).cloned().ok_or_else(|| {
            crate::actors::conductor::ConductorError::NotFound(format!(
                "run writer not found for run_id={run_id}"
            ))
        })?;
        ractor::call!(run_writer, |reply| RunWriterMsg::CreateVersion {
            run_id: run_id.clone(),
            parent_version_id,
            content,
            source,
            reply,
        })
        .map_err(|e| crate::actors::conductor::ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| crate::actors::conductor::ConductorError::WorkerFailed(e.to_string()))
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
    use std::collections::HashMap;
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
                researcher_actor: None,
                terminal_actor: None,
                writer_actor: None,
                model_gateway: gateway,
                run_writers: HashMap::new(),
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
