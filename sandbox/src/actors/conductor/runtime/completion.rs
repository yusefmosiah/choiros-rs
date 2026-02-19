use ractor::ActorProcessingErr;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{CapabilityWorkerOutput, ConductorError},
};

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
                let _ = writer_content;
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
                    let _ = writer_content;
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
                }
            }
            Ok(CapabilityWorkerOutput::Writer(output)) => {
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
                        kind: shared_types::ArtifactKind::JsonData,
                        reference: format!("call://{}", call_id),
                        mime_type: Some("application/json".to_string()),
                        created_at: chrono::Utc::now(),
                        source_call_id: call_id.clone(),
                        metadata: Some(serde_json::json!({
                            "capability": "writer",
                            "summary": output.summary,
                            "delegated_capabilities": output.delegated_capabilities,
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
                        "writer orchestration completed",
                    )
                    .await;
                    events::emit_worker_result(
                        &state.event_store,
                        &run_id,
                        "writer",
                        true,
                        "writer orchestration completed",
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
                        Some(shared_types::FailureKind::Provider),
                    )
                    .await;
                    events::emit_worker_result(&state.event_store, &run_id, "writer", false, &err)
                        .await;
                }
            }
            Ok(CapabilityWorkerOutput::ImmediateResponse(message)) => {
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
                    kind: shared_types::ArtifactKind::JsonData,
                    reference: format!("call://{}", call_id),
                    mime_type: Some("application/json".to_string()),
                    created_at: chrono::Utc::now(),
                    source_call_id: call_id.clone(),
                    metadata: Some(serde_json::json!({
                        "capability": "immediate_response",
                        "message": message,
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
                    "immediate response completed",
                )
                .await;
                events::emit_worker_result(
                    &state.event_store,
                    &run_id,
                    "immediate_response",
                    true,
                    "immediate response completed",
                )
                .await;
            }
            Ok(CapabilityWorkerOutput::Harness(_)) => {
                let err = "Harness capability output is not implemented yet".to_string();
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
                events::emit_worker_result(&state.event_store, &run_id, &capability, false, &err)
                    .await;
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
