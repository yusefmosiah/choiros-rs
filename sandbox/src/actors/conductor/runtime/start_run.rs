use ractor::{ActorProcessingErr, ActorRef};
use shared_types::ConductorExecuteRequest;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    protocol::{ConductorError, ConductorMsg},
};
use crate::actors::writer::{WriterMsg, WriterSource};

impl ConductorActor {
    fn run_document_path(run_id: &str) -> String {
        format!("conductor/runs/{run_id}/draft.md")
    }

    async fn ensure_run_document_for_run(
        &self,
        state: &mut ConductorState,
        run_id: &str,
        desktop_id: &str,
        objective: &str,
    ) -> Result<(), ConductorError> {
        let writer_actor = self.resolve_writer_actor_for_run(state, run_id).await?;
        ractor::call!(writer_actor, |reply| WriterMsg::EnsureRunDocument {
            run_id: run_id.to_string(),
            desktop_id: desktop_id.to_string(),
            objective: objective.to_string(),
            reply,
        })
        .map_err(|e| ConductorError::ActorUnavailable(e.to_string()))?
        .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
    }

    fn capability_contract_prefix(capability: &str) -> &'static str {
        match capability {
            "immediate_response" => {
                "Capability Contract (immediate_response): respond directly and briefly to the user objective. Use plain text, no markdown tables, and no worker delegation."
            }
            "writer" => {
                "Capability Contract (writer): app-agent orchestration and synthesis authority. You may delegate to internal workers (researcher/terminal) as needed, but Conductor does not route workers directly. Produce revision-ready synthesis context for Writer document updates."
            }
            _ => "Capability Contract: execute only within your assigned capability scope.",
        }
    }

    pub(crate) fn objective_with_capability_contract(
        &self,
        capability: &str,
        objective: String,
    ) -> String {
        let prefix = Self::capability_contract_prefix(&capability.to_ascii_lowercase());
        format!("{prefix}\n\nObjective:\n{objective}")
    }

    pub(crate) async fn handle_execute_task(
        &self,
        myself: ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        request: ConductorExecuteRequest,
    ) -> Result<shared_types::ConductorRunState, ConductorError> {
        let run_id = ulid::Ulid::new().to_string();

        tracing::info!(
            run_id = %run_id,
            objective = %request.objective,
            "Executing new conductor run"
        );

        let now = chrono::Utc::now();

        events::emit_prompt_received(
            &state.event_store,
            &run_id,
            &request.objective,
            &request.desktop_id,
        )
        .await;

        events::emit_task_started(
            &state.event_store,
            &run_id,
            &request.objective,
            &request.desktop_id,
        )
        .await;

        let document_path = Self::run_document_path(&run_id);
        self.ensure_run_document_for_run(state, &run_id, &request.desktop_id, &request.objective)
            .await?;

        let bootstrap_note = format!(
            "This draft will become a coherent comparison based on incoming evidence.\n\
             The run has started and writer orchestration is gathering evidence and updates.\n\n\
             Objective: {}\n\
             Run ID: `{}`",
            request.objective, run_id
        );
        let writer_actor = self.resolve_writer_actor_for_run(state, &run_id).await?;
        match ractor::call!(writer_actor, |reply| WriterMsg::ApplyText {
            run_id: run_id.clone(),
            section_id: "conductor".to_string(),
            source: WriterSource::Conductor,
            content: bootstrap_note,
            proposal: false,
            reply,
        }) {
            Ok(Ok(_revision)) => {}
            Ok(Err(e)) => {
                return Err(ConductorError::WorkerFailed(format!(
                    "Failed to initialize run document via WriterActor: {e}"
                )));
            }
            Err(e) => {
                return Err(ConductorError::WorkerFailed(format!(
                    "WriterActor bootstrap call failed: {e}"
                )));
            }
        }

        let run = shared_types::ConductorRunState {
            run_id: run_id.clone(),
            objective: request.objective.clone(),
            status: shared_types::ConductorRunStatus::Running,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path,
            output_mode: request.output_mode,
            desktop_id: request.desktop_id.clone(),
        };
        state.tasks.insert_run(run.clone());

        events::emit_task_progress(
            &state.event_store,
            &run_id,
            "running",
            "run_start",
            Some(serde_json::json!({
                "run_id": &run_id,
                "agenda_items": 0,
            })),
        )
        .await;

        let _ = myself.send_message(ConductorMsg::StartRun {
            run_id: run_id.clone(),
            request,
        });

        Ok(run)
    }

    pub(crate) async fn handle_start_run(
        &self,
        myself: &ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        run_id: String,
        request: ConductorExecuteRequest,
    ) -> Result<(), ActorProcessingErr> {
        let initial_agenda = match self
            .conduct_initial_assignments(state, &request, &run_id)
            .await
        {
            Ok(items) => items,
            Err(err) => {
                let shared_error: shared_types::ConductorError = err.clone().into();
                let _ = state
                    .tasks
                    .transition_run_status(&run_id, shared_types::ConductorRunStatus::Failed);
                events::emit_task_failed(
                    &state.event_store,
                    &run_id,
                    &shared_error.code,
                    &shared_error.message,
                    shared_error.failure_kind,
                )
                .await;
                tracing::error!(
                    run_id = %run_id,
                    error = %err,
                    "Conductor start/conduct step failed"
                );
                return Ok(());
            }
        };

        state
            .tasks
            .add_agenda_items(&run_id, initial_agenda.clone())
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

        events::emit_task_progress(
            &state.event_store,
            &run_id,
            "waiting_worker",
            "worker_execution",
            Some(serde_json::json!({
                "agenda_items": initial_agenda.len(),
            })),
        )
        .await;

        events::emit_control_event(
            &state.event_store,
            "conductor.run.started",
            &run_id,
            "conductor",
            "run_start",
            serde_json::json!({
                "objective": request.objective,
                "desktop_id": request.desktop_id,
            }),
        )
        .await;

        self.dispatch_seed_agenda(myself, state, &run_id).await?;
        Ok(())
    }

    pub(crate) async fn conduct_initial_assignments(
        &self,
        state: &ConductorState,
        request: &ConductorExecuteRequest,
        run_id: &str,
    ) -> Result<Vec<shared_types::ConductorAgendaItem>, ConductorError> {
        let now = chrono::Utc::now();
        let mut items = Vec::new();

        let mut available_capabilities = Vec::new();
        if state.writer_supervisor.is_some() {
            available_capabilities.push("immediate_response".to_string());
            available_capabilities.push("writer".to_string());
        }
        if available_capabilities.is_empty() {
            return Err(ConductorError::ActorUnavailable(
                "No app-agent capabilities available for Conductor model gateway".to_string(),
            ));
        }

        // Phase 5.4 — retrieve context snapshot from MemoryActor.
        // Prepend top context items to the objective so the model has retrieval-grounded context.
        // 500ms timeout — if memory is slow or unavailable, continue without it.
        let enriched_objective = if let Some(memory) = &state.memory_actor {
            let snapshot_result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                async {
                    ractor::call!(memory, |reply| crate::actors::memory::MemoryMsg::GetContextSnapshot {
                        run_id: run_id.to_string(),
                        query: request.objective.clone(),
                        max_items: 4,
                        reply,
                    })
                },
            )
            .await;

            match snapshot_result {
                Ok(Ok(snapshot)) if !snapshot.items.is_empty() => {
                    let ctx_lines: Vec<String> = snapshot
                        .items
                        .iter()
                        .map(|item| {
                            format!(
                                "[{kind}] {src}: {excerpt}",
                                kind = item.kind,
                                src = item.source_ref,
                                excerpt = &item.content[..item.content.len().min(120)],
                            )
                        })
                        .collect();
                    format!(
                        "Retrieved context (relevance-ranked):\n{}\n\nObjective: {}",
                        ctx_lines.join("\n"),
                        request.objective
                    )
                }
                _ => request.objective.clone(),
            }
        } else {
            request.objective.clone()
        };

        let conduct_output = state
            .model_gateway
            .conduct_assignments(Some(run_id), &enriched_objective, &available_capabilities)
            .await?;
        let mut selected_capabilities = Vec::new();
        for capability in conduct_output.dispatch_capabilities {
            let normalized = capability.trim().to_ascii_lowercase();
            if normalized.is_empty()
                || !available_capabilities
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&normalized))
                || selected_capabilities
                    .iter()
                    .any(|c: &String| c.eq_ignore_ascii_case(&normalized))
            {
                continue;
            }
            selected_capabilities.push(normalized);
        }

        if selected_capabilities.is_empty() {
            let reason = conduct_output
                .block_reason
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(conduct_output.rationale);
            return Err(ConductorError::InvalidRequest(format!(
                "Conductor conduct step blocked run: {reason}"
            )));
        }

        for (idx, capability) in selected_capabilities.into_iter().enumerate() {
            let objective =
                self.objective_with_capability_contract(&capability, request.objective.clone());
            items.push(shared_types::ConductorAgendaItem {
                item_id: format!("{run_id}:seed:{idx}:{capability}"),
                capability,
                objective,
                priority: idx as u8,
                depends_on: vec![],
                status: shared_types::AgendaItemStatus::Ready,
                created_at: now,
                started_at: None,
                completed_at: None,
            });
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::ConductorActor;

    #[test]
    fn test_objective_with_capability_contract_immediate_response() {
        let actor = ConductorActor;
        let objective =
            actor.objective_with_capability_contract("immediate_response", "ping".into());
        assert!(objective.contains("Capability Contract (immediate_response)"));
        assert!(objective.contains("respond directly and briefly"));
        assert!(objective.contains("Objective:\nping"));
    }

    #[test]
    fn test_objective_with_capability_contract_writer() {
        let actor = ConductorActor;
        let objective =
            actor.objective_with_capability_contract("writer", "Find latest release".into());
        assert!(objective.contains("Capability Contract (writer)"));
        assert!(objective.contains("Conductor does not route workers directly"));
        assert!(objective.contains("Objective:\nFind latest release"));
    }

    #[test]
    fn test_objective_with_capability_contract_default() {
        let actor = ConductorActor;
        let objective = actor.objective_with_capability_contract("unknown", "Run tests".into());
        assert!(objective
            .contains("Capability Contract: execute only within your assigned capability scope."));
        assert!(objective.contains("Objective:\nRun tests"));
    }

    #[test]
    fn test_objective_with_capability_contract_case_insensitive() {
        let actor = ConductorActor;
        let objective =
            actor.objective_with_capability_contract("ImMeDiAtE_ReSpOnSe", "Summarize".into());
        assert!(objective.contains("Capability Contract (immediate_response)"));
    }
}
