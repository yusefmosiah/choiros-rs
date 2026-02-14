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
        let writer = state.writer_actor.clone();
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
                                            RunWriterMsg::ReportSectionProgress {
                                                run_id: run_id_for_progress.clone(),
                                                source: "researcher".to_string(),
                                                section_id: "researcher".to_string(),
                                                phase: progress.phase.clone(),
                                                message: progress.message.clone(),
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
                            writer.clone(),
                            run_writer.clone(),
                            Some(run_id_owned.clone()),
                            Some(call_id_owned.clone()),
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
                                        let _ = ractor::call!(run_writer, |reply| {
                                            RunWriterMsg::ReportSectionProgress {
                                                run_id: run_id_for_progress.clone(),
                                                source: "terminal".to_string(),
                                                section_id: "terminal".to_string(),
                                                phase: progress.phase.clone(),
                                                message: progress.message.clone(),
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
                            Some(run_id_owned.clone()),
                            Some(call_id_owned.clone()),
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
