use ractor::{ActorProcessingErr, ActorRef};

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{CapabilityWorkerOutput, ConductorError, ConductorMsg},
};

impl ConductorActor {
    pub(crate) async fn handle_capability_call_finished(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: String,
        call_id: String,
        agenda_item_id: String,
        capability: String,
        result: Result<CapabilityWorkerOutput, ConductorError>,
    ) -> Result<(), ActorProcessingErr> {
        let (task_id, correlation_id) = if let Some(run) = state.tasks.get_run(&run_id) {
            (run.task_id.clone(), run.correlation_id.clone())
        } else {
            (run_id.clone(), run_id.clone())
        };

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
                    &task_id,
                    &call_id,
                    &capability,
                    "research capability completed",
                )
                .await;
                events::emit_worker_result(
                    &state.event_store,
                    &task_id,
                    &correlation_id,
                    "researcher",
                    true,
                    "research capability completed",
                )
                .await;
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
                        &task_id,
                        &call_id,
                        &capability,
                        "terminal capability completed",
                    )
                    .await;
                    events::emit_worker_result(
                        &state.event_store,
                        &task_id,
                        &correlation_id,
                        "terminal",
                        true,
                        "terminal capability completed",
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
                        &task_id,
                        &call_id,
                        &capability,
                        &err,
                        Some(shared_types::FailureKind::Unknown),
                    )
                    .await;
                    events::emit_worker_result(
                        &state.event_store,
                        &task_id,
                        &correlation_id,
                        "terminal",
                        false,
                        &err,
                    )
                    .await;
                }
            }
            Err(err) => {
                let err_text = err.to_string();
                state
                    .tasks
                    .update_capability_call(
                        &run_id,
                        &call_id,
                        shared_types::CapabilityCallStatus::Failed,
                        Some(err_text.clone()),
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
                    &task_id,
                    &call_id,
                    &capability,
                    &err_text,
                    Some(shared_types::FailureKind::Unknown),
                )
                .await;
                events::emit_worker_result(
                    &state.event_store,
                    &task_id,
                    &correlation_id,
                    &capability,
                    false,
                    &err_text,
                )
                .await;
            }
        }

        let _ = state
            .tasks
            .transition_run_status(&run_id, shared_types::ConductorRunStatus::Running);
        let _ = myself.send_message(ConductorMsg::DispatchReady { run_id });
        Ok(())
    }
}
