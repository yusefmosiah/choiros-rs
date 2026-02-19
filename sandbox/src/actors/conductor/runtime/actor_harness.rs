//! Conductor subharness completion and progress handlers (Phase 4.3).
//!
//! Wires `ConductorMsg::ActorHarnessComplete`, `ActorHarnessFailed`, and
//! `ActorHarnessProgress` into the existing capability call / run
//! finalization machinery.

use ractor::ActorProcessingErr;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::events;
use crate::actors::conductor::protocol::ActorHarnessResult;
use crate::actors::event_store::{AppendEvent, EventStoreMsg};

impl ConductorActor {
    /// Handle successful subharness completion.
    ///
    /// The `correlation_id` is the `call_id` that was registered when the
    /// subharness was spawned (see `spawn_capability_call` in `decision.rs`).
    pub(crate) async fn handle_actor_harness_complete(
        &self,
        state: &mut ConductorState,
        correlation_id: String,
        result: ActorHarnessResult,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            correlation_id = %correlation_id,
            steps_taken = result.steps_taken,
            objective_satisfied = result.objective_satisfied,
            "ActorHarnessActor completed"
        );

        // Resolve which run owns this correlation.
        let run_id = match state.tasks.get_run_id_for_call(&correlation_id) {
            Some(id) => id.to_string(),
            None => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    "ActorHarnessComplete: no matching capability call found; ignoring"
                );
                return Ok(());
            }
        };

        // Resolve agenda item tied to this call.
        let agenda_item_id = state
            .tasks
            .get_agenda_item_id_for_call(&run_id, &correlation_id)
            .unwrap_or_else(|| correlation_id.clone());

        // Mark the capability call completed.
        let _ = state.tasks.update_capability_call(
            &run_id,
            &correlation_id,
            shared_types::CapabilityCallStatus::Completed,
            None,
        );
        let _ = state.tasks.update_agenda_item(
            &run_id,
            &agenda_item_id,
            shared_types::AgendaItemStatus::Completed,
        );

        // Persist artifact.
        let artifact = shared_types::ConductorArtifact {
            artifact_id: ulid::Ulid::new().to_string(),
            kind: shared_types::ArtifactKind::JsonData,
            reference: format!("call://{}", correlation_id),
            mime_type: Some("application/json".to_string()),
            created_at: chrono::Utc::now(),
            source_call_id: correlation_id.clone(),
            metadata: Some(serde_json::json!({
                "capability": "actor_harness",
                "objective_satisfied": result.objective_satisfied,
                "steps_taken": result.steps_taken,
                "completion_reason": result.completion_reason,
                "output_excerpt": result.output.chars().take(300).collect::<String>(),
            })),
        };
        let _ = state.tasks.add_artifact(&run_id, artifact);

        events::emit_capability_completed(
            &state.event_store,
            &run_id,
            &correlation_id,
            "actor_harness",
            &format!(
                "subharness completed (satisfied={})",
                result.objective_satisfied
            ),
        )
        .await;

        self.finalize_run_if_quiescent_actor_harness(state, &run_id)
            .await?;
        Ok(())
    }

    /// Handle subharness failure.
    pub(crate) async fn handle_actor_harness_failed(
        &self,
        state: &mut ConductorState,
        correlation_id: String,
        reason: String,
    ) -> Result<(), ActorProcessingErr> {
        tracing::warn!(
            correlation_id = %correlation_id,
            reason = %reason,
            "ActorHarnessActor failed"
        );

        let run_id = match state.tasks.get_run_id_for_call(&correlation_id) {
            Some(id) => id.to_string(),
            None => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    "ActorHarnessFailed: no matching capability call found; ignoring"
                );
                return Ok(());
            }
        };

        let agenda_item_id = state
            .tasks
            .get_agenda_item_id_for_call(&run_id, &correlation_id)
            .unwrap_or_else(|| correlation_id.clone());

        let _ = state.tasks.update_capability_call(
            &run_id,
            &correlation_id,
            shared_types::CapabilityCallStatus::Failed,
            Some(reason.clone()),
        );
        let _ = state.tasks.update_agenda_item(
            &run_id,
            &agenda_item_id,
            shared_types::AgendaItemStatus::Failed,
        );

        events::emit_capability_failed(
            &state.event_store,
            &run_id,
            &correlation_id,
            "actor_harness",
            &reason,
            Some(shared_types::FailureKind::Unknown),
        )
        .await;

        self.finalize_run_if_quiescent_actor_harness(state, &run_id)
            .await?;
        Ok(())
    }

    /// Handle in-flight progress from a running ActorHarnessActor.
    ///
    /// This is non-blocking and advisory — the conductor persists the report
    /// to the event store for observability and run-document enrichment but
    /// does not alter run state.
    pub(crate) async fn handle_actor_harness_progress(
        &self,
        state: &ConductorState,
        correlation_id: String,
        kind: String,
        content: String,
        metadata: serde_json::Value,
    ) {
        tracing::debug!(
            correlation_id = %correlation_id,
            kind = %kind,
            "ActorHarnessActor progress"
        );

        // Resolve which run owns this correlation for the event actor_id.
        let run_id = state
            .tasks
            .get_run_id_for_call(&correlation_id)
            .unwrap_or_default()
            .to_string();

        let _ = state.event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "actor_harness.progress.received".to_string(),
                payload: serde_json::json!({
                    "correlation_id": correlation_id,
                    "run_id": run_id,
                    "kind": kind,
                    "content": content,
                    "metadata": metadata,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
                actor_id: format!("conductor:{}", run_id),
                user_id: "system".to_string(),
            },
        });
    }

    /// Same semantics as `finalize_run_if_quiescent` in `completion.rs`
    /// but callable from the subharness path without borrowing issues.
    async fn finalize_run_if_quiescent_actor_harness(
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
                Some("one or more subharness calls failed".to_string()),
            )
            .await
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        } else {
            state
                .tasks
                .transition_run_status(run_id, shared_types::ConductorRunStatus::Completed)
                .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
            self.finalize_run_as_completed(
                state,
                run_id,
                Some("all subharness calls completed".to_string()),
            )
            .await
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        }

        Ok(())
    }
}
