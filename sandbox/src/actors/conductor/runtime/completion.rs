use ractor::ActorProcessingErr;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{CapabilityWorkerOutput, ConductorError},
};
use crate::actors::writer::{SectionState, WriterInboundEnvelope, WriterMsg, WriterSource};
use shared_types::EventImportance;

impl ConductorActor {
    fn writer_section_for_capability(capability: &str) -> String {
        match capability {
            "researcher" | "terminal" => capability.to_string(),
            _ => "conductor".to_string(),
        }
    }

    fn writer_source_for_capability(capability: &str) -> WriterSource {
        match capability {
            "researcher" => WriterSource::Researcher,
            "terminal" => WriterSource::Terminal,
            _ => WriterSource::Conductor,
        }
    }

    pub(crate) async fn enqueue_capability_inbound(
        &self,
        state: &ConductorState,
        run_id: &str,
        call_id: &str,
        capability: &str,
        kind: &str,
        content: String,
    ) {
        if content.trim().is_empty() {
            return;
        }

        let section_id = Self::writer_section_for_capability(capability);
        let source = Self::writer_source_for_capability(capability);
        let message_id = format!(
            "conductor:{}:{}:{}:{}",
            run_id,
            call_id,
            kind,
            ulid::Ulid::new()
        );
        let source_label = match source {
            WriterSource::Writer => "writer",
            WriterSource::Researcher => "researcher",
            WriterSource::Terminal => "terminal",
            WriterSource::User => "user",
            WriterSource::Conductor => "conductor",
        }
        .to_string();

        let Some(writer_actor) = state.writer_actor.clone() else {
            events::emit_telemetry_event(
                &state.event_store,
                "conductor.writer.enqueue.failed",
                run_id,
                capability,
                "writer_enqueue",
                EventImportance::High,
                serde_json::json!({
                    "call_id": call_id,
                    "kind": kind,
                    "message_id": message_id,
                    "target_section_id": section_id,
                    "source": source_label,
                    "error": "writer actor unavailable",
                }),
            )
            .await;
            return;
        };

        let envelope = WriterInboundEnvelope {
            message_id: message_id.clone(),
            correlation_id: call_id.to_string(),
            kind: kind.to_string(),
            run_id: run_id.to_string(),
            section_id: section_id.clone(),
            source,
            content,
            base_version_id: None,
            prompt_diff: None,
            overlay_id: None,
            session_id: None,
            thread_id: None,
            call_id: Some(call_id.to_string()),
            origin_actor: Some("conductor".to_string()),
        };

        match ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
            envelope,
            reply
        }) {
            Ok(Ok(ack)) => {
                events::emit_telemetry_event(
                    &state.event_store,
                    "conductor.writer.enqueue",
                    run_id,
                    capability,
                    "writer_enqueue",
                    EventImportance::Normal,
                    serde_json::json!({
                        "call_id": call_id,
                        "kind": kind,
                        "message_id": ack.message_id,
                        "accepted": ack.accepted,
                        "duplicate": ack.duplicate,
                        "queue_len": ack.queue_len,
                        "revision": ack.revision,
                        "target_section_id": section_id,
                        "source": source_label,
                    }),
                )
                .await;
            }
            Ok(Err(err)) => {
                tracing::warn!(
                    run_id = %run_id,
                    call_id = %call_id,
                    capability = %capability,
                    error = %err,
                    "Failed to enqueue capability inbound for writer"
                );
                events::emit_telemetry_event(
                    &state.event_store,
                    "conductor.writer.enqueue.failed",
                    run_id,
                    capability,
                    "writer_enqueue",
                    EventImportance::High,
                    serde_json::json!({
                        "call_id": call_id,
                        "kind": kind,
                        "message_id": message_id,
                        "target_section_id": section_id,
                        "source": source_label,
                        "error": err.to_string(),
                    }),
                )
                .await;
            }
            Err(err) => {
                tracing::warn!(
                    run_id = %run_id,
                    call_id = %call_id,
                    capability = %capability,
                    error = %err,
                    "Writer actor call failed while enqueueing capability inbound"
                );
                events::emit_telemetry_event(
                    &state.event_store,
                    "conductor.writer.enqueue.failed",
                    run_id,
                    capability,
                    "writer_enqueue",
                    EventImportance::High,
                    serde_json::json!({
                        "call_id": call_id,
                        "kind": kind,
                        "message_id": message_id,
                        "target_section_id": section_id,
                        "source": source_label,
                        "error": err.to_string(),
                    }),
                )
                .await;
            }
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
        match result {
            Ok(CapabilityWorkerOutput::Researcher(output)) => {
                let mut writer_content = format!(
                    "Researcher capability completed.\nSummary: {}\nObjective status: {:?}\nCompletion reason: {}",
                    output.summary.clone(),
                    output.objective_status.clone(),
                    output.completion_reason.clone()
                );
                if let Some(next_capability) = output.recommended_next_capability.as_ref() {
                    writer_content
                        .push_str(&format!("\nRecommended next capability: {next_capability}"));
                }
                if let Some(next_objective) = output.recommended_next_objective.as_ref() {
                    writer_content
                        .push_str(&format!("\nRecommended next objective: {next_objective}"));
                }
                if !output.citations.is_empty() {
                    writer_content.push_str("\nCitations:");
                    for citation in output.citations.iter().take(8) {
                        writer_content
                            .push_str(&format!("\n- [{}]({})", citation.title, citation.url));
                    }
                }

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
                self.enqueue_capability_inbound(
                    state,
                    &run_id,
                    &call_id,
                    &capability,
                    "capability_completed",
                    writer_content,
                )
                .await;
            }
            Ok(CapabilityWorkerOutput::Terminal(output)) => {
                if output.success {
                    let mut writer_content = format!(
                        "Terminal capability completed.\nSummary: {}",
                        output.summary.clone()
                    );
                    if let Some(reasoning) = output.reasoning.as_ref() {
                        writer_content.push_str(&format!("\nReasoning: {reasoning}"));
                    }
                    if !output.executed_commands.is_empty() {
                        writer_content.push_str("\nExecuted commands:");
                        for command in output.executed_commands.iter().take(10) {
                            writer_content.push_str(&format!("\n- `{command}`"));
                        }
                    }

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
                    self.enqueue_capability_inbound(
                        state,
                        &run_id,
                        &call_id,
                        &capability,
                        "capability_completed",
                        writer_content,
                    )
                    .await;
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
                    self.enqueue_capability_inbound(
                        state,
                        &run_id,
                        &call_id,
                        &capability,
                        "capability_failed",
                        format!("Terminal capability failed.\nSummary: {err}"),
                    )
                    .await;
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
                    let section_id = Self::writer_section_for_capability(&capability);
                    let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                        run_id: run_id.clone(),
                        section_id,
                        state: SectionState::Failed,
                        reply,
                    });
                }
                self.enqueue_capability_inbound(
                    state,
                    &run_id,
                    &call_id,
                    &capability,
                    "capability_failed",
                    format!("Capability failed.\nCapability: {capability}\nError: {err_text}"),
                )
                .await;
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
