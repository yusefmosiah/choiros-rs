use ractor::ActorProcessingErr;
use std::collections::BTreeSet;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{CapabilityWorkerOutput, ConductorError},
};
use crate::actors::run_writer::{RunWriterMsg, SectionState};
use crate::actors::writer::{WriterMsg, WriterSource};

impl ConductorActor {
    fn writer_source_for_section(section: &str) -> WriterSource {
        match section {
            "researcher" => WriterSource::Researcher,
            "terminal" => WriterSource::Terminal,
            "user" => WriterSource::User,
            _ => WriterSource::Conductor,
        }
    }

    async fn enqueue_writer_message(
        state: &ConductorState,
        run_id: &str,
        call_id: &str,
        section_id: &str,
        kind: &str,
        content: &str,
        run_writer: &Option<ractor::ActorRef<RunWriterMsg>>,
    ) {
        let Some(writer_actor) = state.writer_actor.clone() else {
            return;
        };
        let Some(run_writer_actor) = run_writer.clone() else {
            return;
        };
        if content.trim().is_empty() {
            return;
        }

        let message_id = format!("{call_id}:{section_id}:{kind}");
        let source = Self::writer_source_for_section(section_id);
        let result = ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
            message_id: message_id.clone(),
            kind: kind.to_string(),
            run_writer_actor: run_writer_actor.clone(),
            run_id: run_id.to_string(),
            section_id: section_id.to_string(),
            source,
            content: content.to_string(),
            reply,
        });
        if let Err(error) = result {
            tracing::warn!(
                run_id = %run_id,
                section_id = %section_id,
                message_id = %message_id,
                error = %error,
                "Failed to enqueue writer inbox message"
            );
        }
    }

    pub(crate) async fn handle_capability_call_finished(
        &self,
        state: &mut ConductorState,
        run_id: String,
        call_id: String,
        agenda_item_id: String,
        capability: String,
        result: Result<CapabilityWorkerOutput, ConductorError>,
    ) -> Result<(), ActorProcessingErr> {
        let run_writer = state.run_writers.get(&run_id).cloned();
        let section_id = capability.to_ascii_lowercase();

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
                Self::enqueue_writer_message(
                    state,
                    &run_id,
                    &call_id,
                    "researcher",
                    "worker_summary",
                    &output.summary,
                    &run_writer,
                )
                .await;

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
                if let Some(run_writer) = run_writer.clone() {
                    let _ = ractor::call!(run_writer, |reply| RunWriterMsg::MarkSectionState {
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
                    Self::enqueue_writer_message(
                        state,
                        &run_id,
                        &call_id,
                        "terminal",
                        "worker_summary",
                        &output.summary,
                        &run_writer,
                    )
                    .await;

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
                    if let Some(run_writer) = run_writer.clone() {
                        let _ = ractor::call!(run_writer, |reply| RunWriterMsg::MarkSectionState {
                            run_id: run_id.clone(),
                            section_id: "terminal".to_string(),
                            state: SectionState::Complete,
                            reply,
                        });
                    }
                } else {
                    let err = output.summary.clone();
                    Self::enqueue_writer_message(
                        state,
                        &run_id,
                        &call_id,
                        "terminal",
                        "worker_failure",
                        &err,
                        &run_writer,
                    )
                    .await;
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
                    if let Some(run_writer) = run_writer.clone() {
                        let _ = ractor::call!(run_writer, |reply| RunWriterMsg::MarkSectionState {
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
                Self::enqueue_writer_message(
                    state,
                    &run_id,
                    &call_id,
                    match section_id.as_str() {
                        "researcher" => "researcher",
                        "terminal" => "terminal",
                        _ => "conductor",
                    },
                    "worker_error",
                    &err_text,
                    &run_writer,
                )
                .await;
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
                if let Some(run_writer) = run_writer {
                    let section = match section_id.as_str() {
                        "researcher" | "terminal" => section_id.clone(),
                        _ => "conductor".to_string(),
                    };
                    let _ = ractor::call!(run_writer, |reply| RunWriterMsg::MarkSectionState {
                        run_id: run_id.clone(),
                        section_id: section,
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

        if let Some(run_writer) = state.run_writers.get(run_id).cloned() {
            let sections: BTreeSet<String> = run
                .agenda
                .iter()
                .map(|item| item.capability.to_ascii_lowercase())
                .filter(|capability| capability == "researcher" || capability == "terminal")
                .collect();

            for section in sections {
                match ractor::call!(run_writer, |reply| RunWriterMsg::CommitProposal {
                    section_id: section.clone(),
                    reply,
                }) {
                    Ok(Ok(revision)) => {
                        tracing::info!(
                            run_id = %run_id,
                            section = %section,
                            revision = revision,
                            "Writer proposal committed"
                        );
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(
                            run_id = %run_id,
                            section = %section,
                            error = %error,
                            "Writer proposal commit failed"
                        );
                        state
                            .tasks
                            .transition_run_status(
                                run_id,
                                shared_types::ConductorRunStatus::Blocked,
                            )
                            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                        self.finalize_run_as_blocked(
                            state,
                            run_id,
                            Some(format!(
                                "writer failed to commit section '{section}': {error}"
                            )),
                        )
                        .await
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                        return Ok(());
                    }
                    Err(error) => {
                        tracing::warn!(
                            run_id = %run_id,
                            section = %section,
                            error = %error,
                            "Writer commit RPC failed"
                        );
                        state
                            .tasks
                            .transition_run_status(
                                run_id,
                                shared_types::ConductorRunStatus::Blocked,
                            )
                            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                        self.finalize_run_as_blocked(
                            state,
                            run_id,
                            Some(format!("writer RPC failed while committing '{section}'")),
                        )
                        .await
                        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
                        return Ok(());
                    }
                }
            }
        }

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
            Some("all worker calls completed; writer committed proposals".to_string()),
        )
        .await
        .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        Ok(())
    }
}
