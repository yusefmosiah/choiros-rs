use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg},
    workers::{call_researcher, call_terminal},
};
use crate::actors::researcher::ResearcherProgress;
use crate::actors::run_writer::{RunWriterActor, RunWriterArguments, RunWriterMsg};
use crate::actors::terminal::TerminalAgentProgress;
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
                let capability = decision
                    .args
                    .as_ref()
                    .and_then(|args| args.get("capability"))
                    .cloned()
                    .unwrap_or_else(|| "terminal".to_string());
                let objective = decision
                    .args
                    .as_ref()
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
                        ConductorError::PolicyError(format!("Failed to update agenda item: {e}"))
                    })?;

                self.spawn_capability_call(myself, state, run_id, item)
                    .await?;

                let _ = state.tasks.transition_run_status(
                    run_id,
                    shared_types::ConductorRunStatus::WaitingForCalls,
                );
            }
            ConductorAction::AwaitWorker => {
                tracing::info!(run_id = %run_id, "Conductor decision: Await worker completion");
                let _ = state.tasks.transition_run_status(
                    run_id,
                    shared_types::ConductorRunStatus::WaitingForCalls,
                );
            }
            ConductorAction::MergeCanon => {
                tracing::info!(run_id = %run_id, "Conductor decision: Merge canon from completed workers");
                if let Some(run_writer) = state.run_writers.get(run_id).cloned() {
                    let run_id_owned = run_id.to_string();
                    tokio::spawn(async move {
                        use ractor::call;
                        for section in ["researcher", "terminal"] {
                            let result = call!(run_writer, |reply| RunWriterMsg::CommitProposal {
                                section_id: section.to_string(),
                                reply,
                            });
                            match result {
                                Ok(Ok(revision)) => {
                                    tracing::info!(
                                        run_id = %run_id_owned,
                                        section = section,
                                        revision = revision,
                                        "Committed proposal to canon"
                                    );
                                }
                                Ok(Err(e)) => {
                                    tracing::debug!(
                                        run_id = %run_id_owned,
                                        section = section,
                                        error = %e,
                                        "No proposal to commit for section"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        run_id = %run_id_owned,
                                        section = section,
                                        error = %e,
                                        "Failed to call RunWriterActor for commit"
                                    );
                                }
                            }
                        }
                    });
                }
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
                self.emit_run_complete(run_id, reason).await?;
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
                self.emit_run_blocked(run_id, reason).await?;
            }
        }

        let decision_record = shared_types::ConductorDecision {
            decision_id: ulid::Ulid::new().to_string(),
            decision_type: match decision.action {
                ConductorAction::SpawnWorker => shared_types::DecisionType::Dispatch,
                ConductorAction::AwaitWorker => shared_types::DecisionType::Continue,
                ConductorAction::MergeCanon => shared_types::DecisionType::Continue,
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

        let run_writer = if !state.run_writers.contains_key(run_id) {
            let run_metadata = state.tasks.get_run(run_id).cloned().ok_or_else(|| {
                ConductorError::NotFound(format!(
                    "run metadata unavailable while spawning run writer: {run_id}"
                ))
            })?;
            let run_writer_args = RunWriterArguments {
                run_id: run_id.to_string(),
                desktop_id: run_metadata.desktop_id.clone(),
                objective: run_metadata.objective.clone(),
                session_id: run_metadata.desktop_id.clone(),
                thread_id: run_metadata.run_id.clone(),
                root_dir: Some(env!("CARGO_MANIFEST_DIR").to_string()),
                event_store: state.event_store.clone(),
            };
            match ractor::Actor::spawn(
                Some(format!("run-writer-{}", run_id)),
                RunWriterActor,
                run_writer_args,
            )
            .await
            {
                Ok((actor_ref, _handle)) => {
                    state
                        .run_writers
                        .insert(run_id.to_string(), actor_ref.clone());
                    Some(actor_ref)
                }
                Err(e) => {
                    tracing::warn!(run_id = %run_id, error = %e, "Failed to spawn RunWriterActor");
                    None
                }
            }
        } else {
            state.run_writers.get(run_id).cloned()
        };

        let conductor_ref = myself.clone();
        let run_id_owned = run_id.to_string();
        let call_id_owned = call_id.clone();
        let agenda_item_id = item.item_id.clone();
        let capability = item.capability.to_ascii_lowercase();
        let objective = item.objective.clone();
        let researcher = state.researcher_actor.clone();
        let terminal = state.terminal_actor.clone();
        let run_writer_for_logs = run_writer.clone();

        if let Some(run_writer) = run_writer.clone() {
            let section_id = match capability.as_str() {
                "researcher" | "terminal" => capability.clone(),
                _ => "conductor".to_string(),
            };
            let _ = ractor::call!(run_writer, |reply| RunWriterMsg::MarkSectionState {
                run_id: run_id.to_string(),
                section_id,
                state: crate::actors::run_writer::SectionState::Running,
                reply,
            });
        }

        tokio::spawn(async move {
            let result = match capability.as_str() {
                "researcher" => match researcher {
                    Some(researcher_ref) => {
                        let progress_tx = if run_writer_for_logs.is_some() {
                            let (tx, mut rx) = mpsc::unbounded_channel::<ResearcherProgress>();
                            let run_writer_for_progress = run_writer_for_logs.clone();
                            let run_id_for_progress = run_id_owned.clone();
                            tokio::spawn(async move {
                                while let Some(progress) = rx.recv().await {
                                    if let Some(run_writer) = run_writer_for_progress.clone() {
                                        let _ = ractor::call!(run_writer, |reply| {
                                            RunWriterMsg::AppendLogLine {
                                                run_id: run_id_for_progress.clone(),
                                                source: "researcher".to_string(),
                                                section_id: "researcher".to_string(),
                                                text: format!(
                                                    "{}: {}",
                                                    progress.phase, progress.message
                                                ),
                                                proposal: true,
                                                reply,
                                            }
                                        });
                                    }
                                }
                            });
                            Some(tx)
                        } else {
                            None
                        };

                        call_researcher(
                            &researcher_ref,
                            objective,
                            Some(60_000),
                            Some(8),
                            Some(3),
                            progress_tx,
                            run_writer.clone(),
                            Some(run_id_owned.clone()),
                        )
                        .await
                        .map(crate::actors::conductor::protocol::CapabilityWorkerOutput::Researcher)
                    }
                    None => Err(ConductorError::WorkerFailed(
                        "Researcher capability requested but actor is unavailable".to_string(),
                    )),
                },
                "terminal" => match terminal {
                    Some(terminal_ref) => {
                        let progress_tx = if run_writer_for_logs.is_some() {
                            let (tx, mut rx) = mpsc::unbounded_channel::<TerminalAgentProgress>();
                            let run_writer_for_progress = run_writer_for_logs.clone();
                            let run_id_for_progress = run_id_owned.clone();
                            tokio::spawn(async move {
                                while let Some(progress) = rx.recv().await {
                                    if let Some(run_writer) = run_writer_for_progress.clone() {
                                        let message = match &progress.command {
                                            Some(command) if !command.trim().is_empty() => {
                                                format!(
                                                    "{}: {} ({})",
                                                    progress.phase, progress.message, command
                                                )
                                            }
                                            _ => {
                                                format!("{}: {}", progress.phase, progress.message)
                                            }
                                        };
                                        let _ = ractor::call!(run_writer, |reply| {
                                            RunWriterMsg::AppendLogLine {
                                                run_id: run_id_for_progress.clone(),
                                                source: "terminal".to_string(),
                                                section_id: "terminal".to_string(),
                                                text: message,
                                                proposal: true,
                                                reply,
                                            }
                                        });
                                    }
                                }
                            });
                            Some(tx)
                        } else {
                            None
                        };

                        call_terminal(
                            &terminal_ref,
                            objective,
                            None,
                            Some(60_000),
                            Some(6),
                            progress_tx,
                        )
                        .await
                        .map(crate::actors::conductor::protocol::CapabilityWorkerOutput::Terminal)
                    }
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
