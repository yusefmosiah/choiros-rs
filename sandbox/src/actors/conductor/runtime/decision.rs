use ractor::{ActorProcessingErr, ActorRef};

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg},
    workers::{call_researcher, call_terminal},
};
use crate::baml_client::types::{ConductorDecisionOutput, DecisionType};

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
    ) -> Result<ConductorDecisionOutput, ConductorError> {
        let run = state
            .tasks
            .get_run(run_id)
            .ok_or_else(|| ConductorError::NotFound(run_id.to_string()))?;
        let capabilities = self.available_capabilities(state);
        let decision = state.policy.decide_next_action(run, &capabilities).await?;

        self.emit_policy_event(run_id, "ConductorDecideNextAction", &decision)
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
        decision: ConductorDecisionOutput,
    ) -> Result<(), ConductorError> {
        tracing::info!(
            run_id = %run_id,
            decision_type = %decision.decision_type,
            "Applying conductor policy decision"
        );

        match decision.decision_type {
            DecisionType::Dispatch => {
                let items_to_dispatch: Vec<shared_types::ConductorAgendaItem> = {
                    if let Some(run) = state.tasks.get_run(run_id) {
                        decision
                            .target_agenda_item_ids
                            .iter()
                            .filter_map(|item_id| {
                                run.agenda.iter().find(|i| &i.item_id == item_id).cloned()
                            })
                            .collect()
                    } else {
                        vec![]
                    }
                };

                for item in items_to_dispatch {
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
                }
                let _ = state.tasks.transition_run_status(
                    run_id,
                    shared_types::ConductorRunStatus::WaitingForCalls,
                );
            }
            DecisionType::Retry => {
                let items_to_retry: Vec<shared_types::ConductorAgendaItem> = {
                    if let Some(run) = state.tasks.get_run(run_id) {
                        decision
                            .target_agenda_item_ids
                            .iter()
                            .filter_map(|item_id| {
                                run.agenda.iter().find(|i| &i.item_id == item_id).cloned()
                            })
                            .collect()
                    } else {
                        vec![]
                    }
                };

                for item in items_to_retry {
                    state
                        .tasks
                        .update_agenda_item(
                            run_id,
                            &item.item_id,
                            shared_types::AgendaItemStatus::Running,
                        )
                        .map_err(|e| {
                            ConductorError::PolicyError(format!("Failed to retry agenda item: {e}"))
                        })?;
                    self.spawn_capability_call(myself, state, run_id, item)
                        .await?;
                }
                let _ = state.tasks.transition_run_status(
                    run_id,
                    shared_types::ConductorRunStatus::WaitingForCalls,
                );
            }
            DecisionType::SpawnFollowup => {
                for new_item in &decision.new_agenda_items {
                    let item = shared_types::ConductorAgendaItem {
                        item_id: new_item.id.clone(),
                        capability: new_item.capability.clone(),
                        objective: new_item.objective.clone(),
                        priority: new_item.priority as u8,
                        depends_on: new_item.dependencies.clone(),
                        status: shared_types::AgendaItemStatus::Pending,
                        created_at: chrono::Utc::now(),
                        started_at: None,
                        completed_at: None,
                    };
                    state
                        .tasks
                        .add_agenda_items(run_id, vec![item])
                        .map_err(|e| {
                            ConductorError::PolicyError(format!("Failed to add agenda items: {e}"))
                        })?;
                }
                let _ = state.tasks.update_agenda_item_readiness(run_id);
                let _ = myself.send_message(ConductorMsg::DispatchReady {
                    run_id: run_id.to_string(),
                });
            }
            DecisionType::Continue => {
                tracing::info!(run_id = %run_id, "Conductor decision: Continue waiting");
            }
            DecisionType::Complete => {
                state
                    .tasks
                    .transition_run_status(run_id, shared_types::ConductorRunStatus::Completed)
                    .map_err(|e| {
                        ConductorError::PolicyError(format!("Failed to complete run: {e}"))
                    })?;
                self.finalize_run_as_completed(state, run_id, decision.completion_reason.clone())
                    .await?;
                self.emit_run_complete(run_id, decision.completion_reason)
                    .await?;
            }
            DecisionType::Block => {
                state
                    .tasks
                    .transition_run_status(run_id, shared_types::ConductorRunStatus::Blocked)
                    .map_err(|e| {
                        ConductorError::PolicyError(format!("Failed to block run: {e}"))
                    })?;
                self.finalize_run_as_blocked(state, run_id, decision.completion_reason.clone())
                    .await?;
                self.emit_run_blocked(run_id, decision.completion_reason)
                    .await?;
            }
        }

        let decision_record = shared_types::ConductorDecision {
            decision_id: ulid::Ulid::new().to_string(),
            decision_type: match decision.decision_type {
                DecisionType::Dispatch => shared_types::DecisionType::Dispatch,
                DecisionType::Retry => shared_types::DecisionType::Retry,
                DecisionType::SpawnFollowup => shared_types::DecisionType::SpawnFollowup,
                DecisionType::Continue => shared_types::DecisionType::Continue,
                DecisionType::Complete => shared_types::DecisionType::Complete,
                DecisionType::Block => shared_types::DecisionType::Block,
            },
            reason: decision.rationale.clone(),
            timestamp: chrono::Utc::now(),
            affected_agenda_items: decision.target_agenda_item_ids.clone(),
            new_agenda_items: decision
                .new_agenda_items
                .iter()
                .map(|i| i.id.clone())
                .collect(),
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
