use std::path::Path;

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::conductor::{
    events,
    output::{
        build_completion_toast, build_worker_output_from_run, build_writer_window_props,
        resolve_output_mode,
    },
    protocol::ConductorError,
};
use crate::actors::run_writer::SectionState;
use crate::actors::writer::WriterMsg;

impl ConductorActor {
    pub(crate) async fn finalize_run_as_completed(
        &self,
        state: &mut ConductorState,
        run_id: &str,
        completion_reason: Option<String>,
    ) -> Result<(), ConductorError> {
        let run = state
            .tasks
            .get_run(run_id)
            .cloned()
            .ok_or_else(|| ConductorError::NotFound(run_id.to_string()))?;

        let output = build_worker_output_from_run(&run);
        let report_path = self
            .write_report(&run.run_id, &output.report_content)
            .await?;
        let selected_mode = resolve_output_mode(run.output_mode, &output);
        let toast = build_completion_toast(selected_mode, &output, &report_path);

        if let Some(run_state) = state.tasks.get_run_mut(run_id) {
            run_state.output_mode = selected_mode;
            run_state.updated_at = chrono::Utc::now();
        }

        let writer_props =
            if selected_mode == shared_types::ConductorOutputMode::MarkdownReportToWriter {
                Some(build_writer_window_props(&report_path))
            } else {
                None
            };

        events::emit_task_completed(
            &state.event_store,
            &run.run_id,
            selected_mode,
            &report_path,
            writer_props.as_ref(),
            toast.as_ref(),
        )
        .await;

        if let Some(reason) = completion_reason {
            events::emit_progress(&state.event_store, run_id, "conductor", &reason, Some(100))
                .await;
        }

        if let Some(writer_actor) = state.writer_actor.clone() {
            let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                run_id: run_id.to_string(),
                section_id: "conductor".to_string(),
                state: SectionState::Complete,
                reply,
            });
        }

        Ok(())
    }

    pub(crate) async fn finalize_run_as_blocked(
        &self,
        state: &mut ConductorState,
        run_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConductorError> {
        let run = state
            .tasks
            .get_run(run_id)
            .cloned()
            .ok_or_else(|| ConductorError::NotFound(run_id.to_string()))?;
        let message =
            reason.unwrap_or_else(|| "Run blocked by conductor model gateway".to_string());
        let shared_error = shared_types::ConductorError {
            code: "RUN_BLOCKED".to_string(),
            message: message.clone(),
            failure_kind: Some(shared_types::FailureKind::Unknown),
        };
        events::emit_task_failed(
            &state.event_store,
            &run.run_id,
            &shared_error.code,
            &shared_error.message,
            shared_error.failure_kind,
        )
        .await;
        if let Some(writer_actor) = state.writer_actor.clone() {
            let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
                run_id: run_id.to_string(),
                section_id: "conductor".to_string(),
                state: SectionState::Failed,
                reply,
            });
        }
        Ok(())
    }

    pub(crate) async fn write_report(
        &self,
        run_id: &str,
        content: &str,
    ) -> Result<String, ConductorError> {
        let sandbox = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reports_dir = sandbox.join("reports");

        if let Err(e) = tokio::fs::create_dir_all(&reports_dir).await {
            return Err(ConductorError::ReportWriteFailed(format!(
                "Failed to create reports directory: {e}"
            )));
        }

        if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
            return Err(ConductorError::InvalidRequest(
                "Invalid run_id: contains path separators".to_string(),
            ));
        }

        let report_path = reports_dir.join(format!("{run_id}.md"));
        if let Err(e) = tokio::fs::write(&report_path, content).await {
            return Err(ConductorError::ReportWriteFailed(format!(
                "Failed to write report: {e}"
            )));
        }

        Ok(format!("reports/{run_id}.md"))
    }
}
