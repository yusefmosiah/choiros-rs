use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg},
    runtime::capability_call::{CapabilityCallActor, CapabilityCallArguments},
};
use crate::actors::subharness::{SubharnessActor, SubharnessArguments, SubharnessMsg};
use crate::actors::writer::SectionState;
use crate::actors::writer::WriterMsg;

impl ConductorActor {
    pub(crate) async fn dispatch_seed_agenda(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: &str,
    ) -> Result<(), ActorProcessingErr> {
        let Some(run) = state.tasks.get_run(run_id) else {
            tracing::debug!(run_id = %run_id, "Ignoring seed dispatch for unknown run");
            return Ok(());
        };

        if matches!(
            run.status,
            shared_types::ConductorRunStatus::Completed
                | shared_types::ConductorRunStatus::Failed
                | shared_types::ConductorRunStatus::Blocked
        ) {
            tracing::debug!(
                run_id = %run_id,
                status = ?run.status,
                "Ignoring seed dispatch for terminal run state"
            );
            return Ok(());
        }

        if let Err(error) = state.tasks.update_agenda_item_readiness(run_id) {
            tracing::warn!(
                run_id = %run_id,
                error = %error,
                "Failed to update agenda item readiness before seed dispatch"
            );
        }

        let ready_items: Vec<shared_types::ConductorAgendaItem> = state
            .tasks
            .get_ready_agenda_items(run_id)
            .into_iter()
            .cloned()
            .collect();

        for item in ready_items {
            state
                .tasks
                .update_agenda_item(
                    run_id,
                    &item.item_id,
                    shared_types::AgendaItemStatus::Running,
                )
                .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

            self.spawn_capability_call(myself, state, run_id, item)
                .await
                .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        }

        state
            .tasks
            .transition_run_status(run_id, shared_types::ConductorRunStatus::WaitingForCalls)
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        Ok(())
    }

    pub(crate) async fn spawn_capability_call(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: &str,
        item: shared_types::ConductorAgendaItem,
    ) -> Result<(), ConductorError> {
        let call_id = ulid::Ulid::new().to_string();

        let call = shared_types::ConductorCapabilityCall {
            call_id: call_id.clone(),
            capability: item.capability.clone(),
            objective: item.objective.clone(),
            status: shared_types::CapabilityCallStatus::Pending,
            started_at: chrono::Utc::now(),
            completed_at: None,
            parent_call_id: None,
            agenda_item_id: Some(item.item_id.clone()),
            artifact_ids: vec![],
            error: None,
        };

        state
            .tasks
            .register_capability_call(run_id, call)
            .map_err(|e| {
                ConductorError::ModelGatewayError(format!(
                    "Failed to register capability call: {e}"
                ))
            })?;
        state
            .tasks
            .update_capability_call(
                run_id,
                &call_id,
                shared_types::CapabilityCallStatus::Running,
                None,
            )
            .map_err(|e| {
                ConductorError::ModelGatewayError(format!(
                    "Failed to set capability call running: {e}"
                ))
            })?;

        if state.tasks.get_run(run_id).is_some() {
            events::emit_worker_call(
                &state.event_store,
                run_id,
                &item.capability,
                &item.objective,
            )
            .await;
            events::emit_progress(
                &state.event_store,
                run_id,
                &item.capability,
                "capability dispatched",
                None,
            )
            .await;
        }

        let conductor_ref = myself.clone();
        let run_id_owned = run_id.to_string();
        let call_id_owned = call_id.clone();
        let agenda_item_id = item.item_id.clone();
        let capability = item.capability.to_ascii_lowercase();
        let objective = item.objective.clone();
        let writer = match self
            .resolve_writer_actor_for_run(state, &run_id_owned)
            .await
        {
            Ok(actor) => Some(actor),
            Err(error) => {
                let _ = myself.send_message(ConductorMsg::CapabilityCallFinished {
                    run_id: run_id_owned,
                    call_id: call_id_owned,
                    agenda_item_id,
                    capability,
                    result: Err(error),
                });
                return Ok(());
            }
        };

        if let Some(writer_actor) = writer.clone() {
            let section_id = match capability.as_str() {
                "researcher" | "terminal" | "writer" => capability.clone(),
                _ => "conductor".to_string(),
            };
            let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                run_id: run_id_owned.clone(),
                section_id,
                state: SectionState::Running,
                reply,
            });
        }

        let args = CapabilityCallArguments {
            conductor_ref,
            model_gateway: state.model_gateway.clone(),
            writer_actor: writer,
            run_id: run_id_owned.clone(),
            call_id: call_id_owned.clone(),
            agenda_item_id: agenda_item_id.clone(),
            capability: capability.clone(),
            objective,
        };

        match Actor::spawn_linked(
            Some(format!(
                "conductor-capability-call:{}:{}",
                run_id_owned, call_id_owned
            )),
            CapabilityCallActor,
            args,
            myself.get_cell(),
        )
        .await
        {
            Ok((_worker_ref, _handle)) => {}
            Err(error) => {
                let _ = myself.send_message(ConductorMsg::CapabilityCallFinished {
                    run_id: run_id_owned,
                    call_id: call_id_owned,
                    agenda_item_id,
                    capability,
                    result: Err(ConductorError::WorkerFailed(format!(
                        "Failed to spawn capability call actor: {error}"
                    ))),
                });
            }
        }

        tracing::info!(run_id = %run_id, call_id = %call_id, capability = %item.capability, "Spawned capability call");

        Ok(())
    }

    /// Spawn a `SubharnessActor` for a scoped objective.
    ///
    /// Registers the subharness as a `ConductorCapabilityCall` (capability = "subharness")
    /// so the existing active-call and run-finalization machinery applies.
    /// The SubharnessActor sends `ConductorMsg::SubharnessComplete` back directly.
    pub(crate) async fn spawn_subharness_for_run(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: &str,
        item: shared_types::ConductorAgendaItem,
        context: serde_json::Value,
    ) -> Result<(), ConductorError> {
        let call_id = ulid::Ulid::new().to_string();

        let call = shared_types::ConductorCapabilityCall {
            call_id: call_id.clone(),
            capability: "subharness".to_string(),
            objective: item.objective.clone(),
            status: shared_types::CapabilityCallStatus::Running,
            started_at: chrono::Utc::now(),
            completed_at: None,
            parent_call_id: None,
            agenda_item_id: Some(item.item_id.clone()),
            artifact_ids: vec![],
            error: None,
        };

        state.tasks.register_capability_call(run_id, call).map_err(|e| {
            ConductorError::ModelGatewayError(format!("Failed to register subharness call: {e}"))
        })?;
        state
            .tasks
            .update_capability_call(
                run_id,
                &call_id,
                shared_types::CapabilityCallStatus::Running,
                None,
            )
            .ok();

        events::emit_worker_call(
            &state.event_store,
            run_id,
            "subharness",
            &item.objective,
        )
        .await;

        let args = SubharnessArguments {
            event_store: state.event_store.clone(),
        };

        let conductor_ref = myself.clone();
        let run_id_owned = run_id.to_string();
        let call_id_owned = call_id.clone();
        let objective = item.objective.clone();

        match Actor::spawn_linked(
            Some(format!("subharness:{}:{}", run_id_owned, call_id_owned)),
            SubharnessActor,
            args,
            myself.get_cell(),
        )
        .await
        {
            Ok((subharness_ref, _handle)) => {
                let _ = subharness_ref.send_message(SubharnessMsg::Execute {
                    objective,
                    context,
                    correlation_id: call_id_owned.clone(),
                    reply_to: conductor_ref,
                });
                tracing::info!(
                    run_id = %run_id,
                    call_id = %call_id_owned,
                    "Spawned SubharnessActor"
                );
            }
            Err(error) => {
                // Immediately notify conductor so the run can quiesce.
                let _ = myself.send_message(ConductorMsg::SubharnessFailed {
                    correlation_id: call_id_owned,
                    reason: format!("Failed to spawn SubharnessActor: {error}"),
                });
            }
        }

        Ok(())
    }
}
