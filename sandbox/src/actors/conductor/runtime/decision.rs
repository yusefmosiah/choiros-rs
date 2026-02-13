use ractor::{ActorProcessingErr, ActorRef};

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg},
    workers::{call_researcher, call_terminal},
};
use crate::baml_client::types::{ConductorAction, ConductorDecision};

impl ConductorActor {
    pub(crate) async fn handle_dispatch_ready(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: &str,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(run_id = %run_id, "Handling DispatchReady message");
        if let Err(e) = state.tasks.update_agenda_item_readiness(run_id) {
            tracing::error!(run_id = %run_id, error = %e, "Failed to update agenda readiness");
        }

        let ready = state.tasks.get_ready_agenda_items(run_id);
        tracing::info!(run_id = %run_id, ready_count = ready.len(), "Dispatching ready agenda items");

        match self.make_policy_decision(state, run_id).await {
            Ok(decision) => {
                if let Err(e) = self.apply_decision(myself, state, run_id, decision).await {
                    tracing::error!(run_id = %run_id, error = %e, "Failed to apply decision");
                }
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = %e, "Policy decision failed");
                self.emit_decision_failure(run_id, &e.to_string()).await;
                let _ = state
                    .tasks
                    .transition_run_status(run_id, shared_types::ConductorRunStatus::Blocked);
            }
        }

        Ok(())
    }

    pub(crate) async fn make_policy_decision(
        &self,
        state: &ConductorState,
        run_id: &str,
    ) -> Result<ConductorDecision, ConductorError> {
        let run = state
            .tasks
            .get_run(run_id)
            .ok_or_else(|| ConductorError::NotFound(run_id.to_string()))?;
        let capabilities = self.available_capabilities(state);
        let decision = state.policy.decide_next_action(run, &capabilities).await?;

        self.emit_policy_event(run_id, "ConductorDecide", &decision)
            .await;
        Ok(decision)
    }

    fn available_capabilities(&self, state: &ConductorState) -> Vec<String> {
        let mut capabilities = Vec::new();
        if state.terminal_actor.is_some() {
            capabilities.push("terminal".to_string());
        }
        if state.researcher_actor.is_some() {
            capabilities.push("researcher".to_string());
        }
        capabilities
    }

    pub(crate) async fn apply_decision(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: &str,
        decision: ConductorDecision,
    ) -> Result<(), ConductorError> {
        tracing::info!(
            run_id = %run_id,
            action = %decision.action,
            "Applying conductor policy decision"
        );

        match decision.action {
            ConductorAction::SpawnWorker => {
                // Extract worker details from args
                let capability = decision.args.as_ref()
                    .and_then(|args| args.get("capability"))
                    .cloned()
                    .unwrap_or_else(|| "terminal".to_string());
                let objective = decision.args.as_ref()
                    .and_then(|args| args.get("objective"))
                    .cloned()
                    .unwrap_or_default();

                // Create a minimal agenda item for the worker
                let item = shared_types::ConductorAgendaItem {
                    item_id: ulid::Ulid::new().to_string(),
                    capability,
                    objective,
                    priority: 5,
                    depends_on: vec![],
                    status: shared_types::AgendaItemStatus::Pending,
                    created_at: chrono::Utc::now(),
                    started_at: None,
                    completed_at: None,
                };

                state
                    .tasks
                    .add_agenda_items(run_id, vec![item.clone()])
                    .map_err(|e| {
                        ConductorError::PolicyError(format!("Failed to add agenda item: {e}"))
                    })?;

                state
                    .tasks
                    .update_agenda_item(
                        run_id,
                        &item.item_id,
                        shared_types::AgendaItemStatus::Running,
                    )
                    .map_err(|e| {
                        ConductorError::PolicyError(format!(
                            "Failed to update agenda item: {e}"
                        ))
                    })?;

                self.spawn_capability_call(myself, state, run_id, item)
                    .await?;

                let _ = state.tasks.transition_run_status(
                    run_id,
                    shared_types::ConductorRunStatus::WaitingForCalls,
                );
            }
            ConductorAction::UpdateDraft => {
                // The document update is handled by the worker directly
                // This action signals the conductor to continue monitoring
                tracing::info!(run_id = %run_id, "Conductor decision: Update draft");
                let _ = myself.send_message(ConductorMsg::DispatchReady {
                    run_id: run_id.to_string(),
                });
            }
            ConductorAction::Complete => {
                state
                    .tasks
                    .transition_run_status(run_id, shared_types::ConductorRunStatus::Completed)
                    .map_err(|e| {
                        ConductorError::PolicyError(format!("Failed to complete run: {e}"))
                    })?;
                let reason = Some(decision.reason.clone());
                self.finalize_run_as_completed(state, run_id, reason.clone())
                    .await?;
                self.emit_run_complete(run_id, reason)
                    .await?;
            }
            ConductorAction::Block => {
                state
                    .tasks
                    .transition_run_status(run_id, shared_types::ConductorRunStatus::Blocked)
                    .map_err(|e| {
                        ConductorError::PolicyError(format!("Failed to block run: {e}"))
                    })?;
                let reason = Some(decision.reason.clone());
                self.finalize_run_as_blocked(state, run_id, reason.clone())
                    .await?;
                self.emit_run_blocked(run_id, reason)
                    .await?;
            }
        }

        let decision_record = shared_types::ConductorDecision {
            decision_id: ulid::Ulid::new().to_string(),
            decision_type: match decision.action {
                ConductorAction::SpawnWorker => shared_types::DecisionType::Dispatch,
                ConductorAction::UpdateDraft => shared_types::DecisionType::Continue,
                ConductorAction::Complete => shared_types::DecisionType::Complete,
                ConductorAction::Block => shared_types::DecisionType::Block,
            },
            reason: decision.reason.clone(),
            timestamp: chrono::Utc::now(),
            affected_agenda_items: vec![],
            new_agenda_items: vec![],
        };
        state
            .tasks
            .record_decision(run_id, decision_record)
            .map_err(|e| ConductorError::PolicyError(format!("Failed to record decision: {e}")))?;

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
                ConductorError::PolicyError(format!("Failed to register capability call: {e}"))
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
                ConductorError::PolicyError(format!("Failed to set capability call running: {e}"))
            })?;

        if let Some((task_id, correlation_id)) = state
            .tasks
            .get_run(run_id)
            .map(|run| (run.task_id.clone(), run.correlation_id.clone()))
        {
            events::emit_worker_call(
                &state.event_store,
                &task_id,
                &correlation_id,
                &item.capability,
                &item.objective,
            )
            .await;
            events::emit_progress(
                &state.event_store,
                run_id,
                &task_id,
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
        let researcher = state.researcher_actor.clone();
        let terminal = state.terminal_actor.clone();

        tokio::spawn(async move {
            let result = match capability.as_str() {
                "researcher" => match researcher {
                    Some(researcher_ref) => call_researcher(
                        &researcher_ref,
                        objective,
                        Some(60_000),
                        Some(8),
                        Some(3),
                    )
                    .await
                    .map(crate::actors::conductor::protocol::CapabilityWorkerOutput::Researcher),
                    None => Err(ConductorError::WorkerFailed(
                        "Researcher capability requested but actor is unavailable".to_string(),
                    )),
                },
                "terminal" => match terminal {
                    Some(terminal_ref) => call_terminal(
                        &terminal_ref,
                        objective,
                        None,
                        Some(60_000),
                        Some(6),
                    )
                    .await
                    .map(crate::actors::conductor::protocol::CapabilityWorkerOutput::Terminal),
                    None => Err(ConductorError::WorkerFailed(
                        "Terminal capability requested but actor is unavailable".to_string(),
                    )),
                },
                unknown => Err(ConductorError::WorkerFailed(format!(
                    "Unsupported capability '{unknown}'"
                ))),
            };

            let _ = conductor_ref.send_message(ConductorMsg::CapabilityCallFinished {
                run_id: run_id_owned,
                call_id: call_id_owned,
                agenda_item_id,
                capability,
                result,
            });
        });

        tracing::info!(run_id = %run_id, call_id = %call_id, capability = %item.capability, "Spawned capability call");

        Ok(())
    }
}
