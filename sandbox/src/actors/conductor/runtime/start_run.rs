use ractor::{ActorProcessingErr, ActorRef};
use shared_types::ConductorExecuteRequest;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events, file_tools,
    protocol::{ConductorError, ConductorMsg},
};

impl ConductorActor {
    fn capability_contract_prefix(capability: &str) -> &'static str {
        match capability {
            "researcher" => {
                "Capability Contract (researcher): external research only. Use research tools, citations, and source synthesis. Do not perform local shell orchestration."
            }
            "terminal" => {
                "Capability Contract (terminal): local execution only. Use shell/file/system inspection and execution. Do not perform general web research."
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

        if state.terminal_actor.is_none() && state.researcher_actor.is_none() {
            return Err(ConductorError::ActorUnavailable(
                "No worker actors available for Conductor default model gateway".to_string(),
            ));
        }

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

        // Create initial draft document
        let document_path =
            match file_tools::create_initial_draft(&run_id, &request.objective).await {
                Ok(path) => path,
                Err(e) => {
                    tracing::error!(run_id = %run_id, error = %e, "Failed to create initial draft");
                    return Err(e);
                }
            };

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
        if state.researcher_actor.is_some() {
            available_capabilities.push("researcher".to_string());
        }
        if state.terminal_actor.is_some() {
            available_capabilities.push("terminal".to_string());
        }
        if available_capabilities.is_empty() {
            return Err(ConductorError::ActorUnavailable(
                "No worker actors available for Conductor default model gateway".to_string(),
            ));
        }

        let conduct_output = state
            .model_gateway
            .conduct_assignments(Some(run_id), &request.objective, &available_capabilities)
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
    fn test_objective_with_capability_contract_researcher() {
        let actor = ConductorActor;
        let objective =
            actor.objective_with_capability_contract("researcher", "Find latest release".into());
        assert!(objective.contains("Capability Contract (researcher)"));
        assert!(objective.contains("external research only"));
        assert!(objective.contains("Objective:\nFind latest release"));
    }

    #[test]
    fn test_objective_with_capability_contract_terminal() {
        let actor = ConductorActor;
        let objective = actor.objective_with_capability_contract("terminal", "Run tests".into());
        assert!(objective.contains("Capability Contract (terminal)"));
        assert!(objective.contains("local execution only"));
        assert!(objective.contains("Objective:\nRun tests"));
    }

    #[test]
    fn test_objective_with_capability_contract_case_insensitive() {
        let actor = ConductorActor;
        let objective = actor.objective_with_capability_contract("ReSeArChEr", "Summarize".into());
        assert!(objective.contains("Capability Contract (researcher)"));
    }
}
