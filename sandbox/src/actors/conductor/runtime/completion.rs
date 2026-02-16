use ractor::ActorProcessingErr;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{CapabilityWorkerOutput, ConductorError},
};
use crate::actors::run_writer::SectionState;
use crate::actors::writer::WriterMsg;

impl ConductorActor {
    pub(crate) async fn handle_capability_call_finished(
        &self,
        state: &mut ConductorState,
        run_id: String,
        call_id: String,
        agenda_item_id: String,
        capability: String,
        result: Result<CapabilityWorkerOutput, ConductorError>,
    ) -> Result<(), ActorProcessingErr> {
        match result {
            Ok(CapabilityWorkerOutput::Researcher(output)) => {
                state
                    .tasks
                    .update_capability_call(
                        &run_id,
                        &call_id,
                        shared_types::CapabilityCallStatus::Completed,
                        None,
                    )
                    .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                state
                    .tasks
                    .update_agenda_item(
                        &run_id,
                        &agenda_item_id,
                        shared_types::AgendaItemStatus::Completed,
                    )
                    .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                let artifact = shared_types::ConductorArtifact {
                    artifact_id: ulid::Ulid::new().to_string(),
                    kind: shared_types::ArtifactKind::SearchResults,
                    reference: format!("call://{}", call_id),
                    mime_type: Some("application/json".to_string()),
                    created_at: chrono::Utc::now(),
                    source_call_id: call_id.clone(),
                    metadata: Some(serde_json::json!({
                        "capability": "researcher",
                        "summary": output.summary,
                        "objective_status": output.objective_status,
                        "completion_reason": output.completion_reason,
                        "recommended_next_capability": output.recommended_next_capability,
                        "recommended_next_objective": output.recommended_next_objective,
                        "citations": output.citations,
                    })),
                };
                state
                    .tasks
                    .add_artifact(&run_id, artifact)
                    .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                events::emit_capability_completed(
                    &state.event_store,
                    &run_id,
                    &call_id,
                    &capability,
                    "research capability completed",
                )
                .await;
                events::emit_worker_result(
                    &state.event_store,
                    &run_id,
                    "researcher",
                    true,
                    "research capability completed",
                )
                .await;
                if let Some(writer_actor) = state.writer_actor.clone() {
                    let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                        run_id: run_id.clone(),
                        section_id: "researcher".to_string(),
                        state: SectionState::Complete,
                        reply,
                    });
                }
            }
            Ok(CapabilityWorkerOutput::Terminal(output)) => {
                if output.success {
                    state
                        .tasks
                        .update_capability_call(
                            &run_id,
                            &call_id,
                            shared_types::CapabilityCallStatus::Completed,
                            None,
                        )
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                    state
                        .tasks
                        .update_agenda_item(
                            &run_id,
                            &agenda_item_id,
                            shared_types::AgendaItemStatus::Completed,
                        )
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                    let artifact = shared_types::ConductorArtifact {
                        artifact_id: ulid::Ulid::new().to_string(),
                        kind: shared_types::ArtifactKind::TerminalOutput,
                        reference: format!("call://{}", call_id),
                        mime_type: Some("application/json".to_string()),
                        created_at: chrono::Utc::now(),
                        source_call_id: call_id.clone(),
                        metadata: Some(serde_json::json!({
                            "capability": "terminal",
                            "summary": output.summary,
                            "reasoning": output.reasoning,
                            "executed_commands": output.executed_commands,
                            "steps": output.steps,
                        })),
                    };
                    state
                        .tasks
                        .add_artifact(&run_id, artifact)
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                    events::emit_capability_completed(
                        &state.event_store,
                        &run_id,
                        &call_id,
                        &capability,
                        "terminal capability completed",
                    )
                    .await;
                    events::emit_worker_result(
                        &state.event_store,
                        &run_id,
                        "terminal",
                        true,
                        "terminal capability completed",
                    )
                    .await;
                    if let Some(writer_actor) = state.writer_actor.clone() {
                        let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                            run_id: run_id.clone(),
                            section_id: "terminal".to_string(),
                            state: SectionState::Complete,
                            reply,
                        });
                    }
                } else {
                    let err = output.summary.clone();
                    state
                        .tasks
                        .update_capability_call(
                            &run_id,
                            &call_id,
                            shared_types::CapabilityCallStatus::Failed,
                            Some(err.clone()),
                        )
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                    state
                        .tasks
                        .update_agenda_item(
                            &run_id,
                            &agenda_item_id,
                            shared_types::AgendaItemStatus::Failed,
                        )
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                    events::emit_capability_failed(
                        &state.event_store,
                        &run_id,
                        &call_id,
                        &capability,
                        &err,
                        Some(shared_types::FailureKind::Unknown),
                    )
                    .await;
                    events::emit_worker_result(
                        &state.event_store,
                        &run_id,
                        "terminal",
                        false,
                        &err,
                    )
                    .await;
                    if let Some(writer_actor) = state.writer_actor.clone() {
                        let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                            run_id: run_id.clone(),
                            section_id: "terminal".to_string(),
                            state: SectionState::Failed,
                            reply,
                        });
                    }
                }
            }
            Err(err) => {
                let (call_status, agenda_status, failure_kind, blocked_reason) = match &err {
                    ConductorError::WorkerBlocked(reason) => (
                        shared_types::CapabilityCallStatus::Blocked,
                        shared_types::AgendaItemStatus::Blocked,
                        Some(shared_types::FailureKind::Provider),
                        Some(reason.clone()),
                    ),
                    _ => (
                        shared_types::CapabilityCallStatus::Failed,
                        shared_types::AgendaItemStatus::Failed,
                        Some(shared_types::FailureKind::Unknown),
                        None,
                    ),
                };

                let err_text = err.to_string();
                state
                    .tasks
                    .update_capability_call(&run_id, &call_id, call_status, Some(err_text.clone()))
                    .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                state
                    .tasks
                    .update_agenda_item(&run_id, &agenda_item_id, agenda_status)
                    .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

                if let Some(reason) = blocked_reason {
                    events::emit_capability_blocked(
                        &state.event_store,
                        &run_id,
                        &call_id,
                        &capability,
                        &reason,
                    )
                    .await;
                } else {
                    events::emit_capability_failed(
                        &state.event_store,
                        &run_id,
                        &call_id,
                        &capability,
                        &err_text,
                        failure_kind,
                    )
                    .await;
                }
                events::emit_worker_result(
                    &state.event_store,
                    &run_id,
                    &capability,
                    false,
                    &err_text,
                )
                .await;

                if let Some(writer_actor) = state.writer_actor.clone() {
                    let section_id = match capability.as_str() {
                        "researcher" | "terminal" => capability.clone(),
                        _ => "conductor".to_string(),
                    };
                    let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                        run_id: run_id.clone(),
                        section_id,
                        state: SectionState::Failed,
                        reply,
                    });
                }
            }
        }

        self.finalize_run_if_quiescent(state, &run_id).await?;
        Ok(())
    }

    async fn finalize_run_if_quiescent(
        &self,
        state: &mut ConductorState,
        run_id: &str,
    ) -> Result<(), ActorProcessingErr> {
        let active_calls = state.tasks.get_run_active_calls(run_id).len();
        if active_calls > 0 {
            let _ = state
                .tasks
                .transition_run_status(run_id, shared_types::ConductorRunStatus::WaitingForCalls);
            return Ok(());
        }

        let run = state
            .tasks
            .get_run(run_id)
            .cloned()
            .ok_or_else(|| ActorProcessingErr::from(format!("run not found: {run_id}")))?;

        let has_failed_items = run.agenda.iter().any(|item| {
            matches!(
                item.status,
                shared_types::AgendaItemStatus::Failed | shared_types::AgendaItemStatus::Blocked
            )
        });

        if has_failed_items {
            state
                .tasks
                .transition_run_status(run_id, shared_types::ConductorRunStatus::Blocked)
                .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
            self.finalize_run_as_blocked(
                state,
                run_id,
                Some("one or more worker calls failed".to_string()),
            )
            .await
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
            return Ok(());
        }

        state
            .tasks
            .transition_run_status(run_id, shared_types::ConductorRunStatus::Completed)
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        self.finalize_run_as_completed(
            state,
            run_id,
            Some("all worker calls completed".to_string()),
        )
        .await
        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        Ok(())
    }
}
