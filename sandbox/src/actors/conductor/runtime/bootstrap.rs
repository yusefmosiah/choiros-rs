use ractor::ActorRef;
use shared_types::{ConductorExecuteRequest, ConductorTaskState, ConductorTaskStatus};

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    file_tools,
    protocol::{ConductorError, ConductorMsg},
};

impl ConductorActor {
    pub(crate) async fn handle_execute_task(
        &self,
        myself: ActorRef<ConductorMsg>,
        state: &mut ConductorState,
        request: ConductorExecuteRequest,
    ) -> Result<ConductorTaskState, ConductorError> {
        let task_id = ulid::Ulid::new().to_string();
        let correlation_id = request
            .correlation_id
            .clone()
            .unwrap_or_else(|| task_id.clone());

        tracing::info!(
            task_id = %task_id,
            correlation_id = %correlation_id,
            objective = %request.objective,
            "Executing new conductor task"
        );

        let now = chrono::Utc::now();
        let task_state = ConductorTaskState {
            task_id: task_id.clone(),
            status: ConductorTaskStatus::Queued,
            objective: request.objective.clone(),
            desktop_id: request.desktop_id.clone(),
            output_mode: request.output_mode,
            correlation_id: correlation_id.clone(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            report_path: None,
            toast: None,
            error: None,
        };
        state.tasks.insert_task(task_state)?;

        if state.terminal_actor.is_none() && state.researcher_actor.is_none() {
            return Err(ConductorError::InvalidRequest(
                "No worker actors available for Conductor default policy".to_string(),
            ));
        }

        events::emit_task_started(
            &state.event_store,
            &task_id,
            &correlation_id,
            &request.objective,
            &request.desktop_id,
        )
        .await;

        let initial_agenda = self.build_initial_agenda(state, &request, &task_id).await?;

        // Create initial draft document
        let document_path = match file_tools::create_initial_draft(&task_id, &request.objective).await {
            Ok(path) => path,
            Err(e) => {
                tracing::error!(task_id = %task_id, error = %e, "Failed to create initial draft");
                return Err(e);
            }
        };

        let run = shared_types::ConductorRunState {
            run_id: task_id.clone(),
            task_id: task_id.clone(),
            objective: request.objective.clone(),
            status: shared_types::ConductorRunStatus::Running,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: initial_agenda.clone(),
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path,
            output_mode: request.output_mode,
            desktop_id: request.desktop_id.clone(),
            correlation_id: correlation_id.clone(),
        };
        state.tasks.insert_run(run);

        state.tasks.transition_to_running(&task_id)?;
        events::emit_task_progress(
            &state.event_store,
            &task_id,
            &correlation_id,
            "running",
            "run_bootstrap",
            Some(serde_json::json!({
                "run_id": &task_id,
                "agenda_items": initial_agenda.len(),
            })),
        )
        .await;

        state.tasks.transition_to_waiting_worker(&task_id)?;
        events::emit_task_progress(
            &state.event_store,
            &task_id,
            &correlation_id,
            "waiting_worker",
            "worker_execution",
            None,
        )
        .await;

        events::emit_wake_event(
            &state.event_store,
            "conductor.run.started",
            &task_id,
            &task_id,
            "conductor",
            "run_start",
            serde_json::json!({
                "objective": request.objective,
                "desktop_id": request.desktop_id,
            }),
        )
        .await;

        let _ = myself.send_message(ConductorMsg::DispatchReady {
            run_id: task_id.clone(),
        });

        Ok(state
            .tasks
            .get_task(&task_id)
            .cloned()
            .expect("task must exist after insertion"))
    }

    pub(crate) async fn build_initial_agenda(
        &self,
        state: &ConductorState,
        request: &ConductorExecuteRequest,
        task_id: &str,
    ) -> Result<Vec<shared_types::ConductorAgendaItem>, ConductorError> {
        let now = chrono::Utc::now();
        let mut items = Vec::new();

        if let Some(worker_plan) = &request.worker_plan {
            if !worker_plan.is_empty() {
                return Err(ConductorError::InvalidRequest(
                    "worker_plan is deprecated in full-agentic mode; omit worker_plan and let conductor policy dispatch capabilities"
                        .to_string(),
                ));
            }
        }

        let mut available_capabilities = Vec::new();
        if state.terminal_actor.is_some() {
            available_capabilities.push("terminal".to_string());
        }
        if state.researcher_actor.is_some() {
            available_capabilities.push("researcher".to_string());
        }
        if available_capabilities.is_empty() {
            return Err(ConductorError::InvalidRequest(
                "No worker actors available for Conductor default policy".to_string(),
            ));
        }

        let bootstrap = state
            .policy
            .bootstrap_agenda(&request.objective, &available_capabilities)
            .await?;

        let mut selected_capabilities = Vec::new();
        for capability in bootstrap.dispatch_capabilities {
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
            let reason = bootstrap
                .block_reason
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(bootstrap.rationale);
            return Err(ConductorError::InvalidRequest(format!(
                "Conductor bootstrap policy blocked run: {reason}"
            )));
        }

        for (idx, capability) in selected_capabilities.into_iter().enumerate() {
            let refined = state
                .policy
                .refine_objective_for_capability(&request.objective, &capability)
                .await?;
            items.push(shared_types::ConductorAgendaItem {
                item_id: format!("{task_id}:seed:{idx}:{capability}"),
                capability,
                objective: refined.refined_objective,
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
