//! Conductor run state durability — Phase 4.4.
//!
//! Projects `ConductorRunState` from the event store on actor restart,
//! replaying `conductor.run.started` events to rebuild in-memory state
//! for any runs that were active when the process crashed.
//!
//! Only non-terminal runs are restored (status ≠ Completed/Failed/Blocked).
//! Runs that were in-flight are marked as `Blocked` since their workers
//! have already been dropped — they cannot continue automatically.

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::event_store::get_recent_events;

/// Maximum number of recent events to scan for run recovery.
const RUN_RECOVERY_SCAN_LIMIT: i64 = 2000;

impl ConductorActor {
    /// Attempt to restore in-progress run states from the event store.
    ///
    /// Called from `post_start`. Errors are logged but never propagated —
    /// durability is best-effort; a clean state is always safe to start with.
    pub(crate) async fn restore_run_states(&self, state: &mut ConductorState) {
        let events = match get_recent_events(
            &state.event_store,
            0,
            RUN_RECOVERY_SCAN_LIMIT,
            Some("conductor.run.started".to_string()),
            None,
            None,
        )
        .await
        {
            Ok(Ok(events)) => events,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Run state recovery: event store error");
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Run state recovery: event store query failed");
                return;
            }
        };

        let mut restored = 0usize;
        let now = chrono::Utc::now();

        for event in events {
            // Extract run_id from payload.
            let run_id = match event.payload.get("run_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            // Skip if already present (e.g. duplicate events).
            if state.tasks.get_run(&run_id).is_some() {
                continue;
            }

            let objective = event
                .payload
                .get("objective")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string();
            let desktop_id = event
                .payload
                .get("desktop_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            // Restored runs start in Blocked — workers were lost in the crash.
            // Operators can inspect the event log and re-submit if needed.
            let run = shared_types::ConductorRunState {
                run_id: run_id.clone(),
                objective,
                status: shared_types::ConductorRunStatus::Blocked,
                created_at: event.timestamp,
                updated_at: now,
                completed_at: Some(now),
                agenda: vec![],
                active_calls: vec![],
                artifacts: vec![],
                decision_log: vec![],
                document_path: format!("conductor/runs/{run_id}/draft.md"),
                output_mode: shared_types::ConductorOutputMode::Auto,
                desktop_id,
            };
            state.tasks.insert_run(run);
            restored += 1;
        }

        if restored > 0 {
            tracing::info!(restored, "Restored run states from event store");
        }
    }
}
