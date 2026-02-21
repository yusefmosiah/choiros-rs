//! Conductor run state durability — Phase 4.4.
//!
//! Projects `ConductorRunState` from the event store on actor restart,
//! replaying `conductor.task.*` lifecycle events to rebuild in-memory state
//! for recent runs after process restart.

use crate::actors::conductor::actor::{ConductorActor, ConductorState};
use crate::actors::event_store::{get_latest_seq, get_recent_events};
use shared_types::{ConductorOutputMode, ConductorRunState, ConductorRunStatus};
use std::collections::HashMap;

/// Number of recent conductor lifecycle events to scan for recovery.
const RUN_RECOVERY_SCAN_LIMIT: i64 = 1000;

fn payload_field<'a>(payload: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    payload.get(key).or_else(|| payload.get("data")?.get(key))
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload_field(payload, key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn payload_output_mode(payload: &serde_json::Value) -> Option<ConductorOutputMode> {
    payload_field(payload, "output_mode")
        .cloned()
        .and_then(|value| serde_json::from_value::<ConductorOutputMode>(value).ok())
}

fn map_status_from_event(
    event_type: &str,
    payload: &serde_json::Value,
) -> Option<ConductorRunStatus> {
    match event_type {
        "conductor.run.started" => Some(ConductorRunStatus::Running),
        "conductor.task.started" => Some(ConductorRunStatus::Running),
        "conductor.task.progress" => {
            let raw = payload_string(payload, "status")?;
            match raw.as_str() {
                "running" => Some(ConductorRunStatus::Running),
                "waiting_worker" | "waiting_calls" | "waiting_for_calls" => {
                    Some(ConductorRunStatus::WaitingForCalls)
                }
                "completing" => Some(ConductorRunStatus::Completing),
                "completed" => Some(ConductorRunStatus::Completed),
                "failed" => Some(ConductorRunStatus::Failed),
                "blocked" => Some(ConductorRunStatus::Blocked),
                _ => Some(ConductorRunStatus::Running),
            }
        }
        "conductor.task.completed" => Some(ConductorRunStatus::Completed),
        "conductor.task.failed" => {
            let error_code = payload_string(payload, "error_code").unwrap_or_default();
            if error_code == "RUN_BLOCKED" {
                Some(ConductorRunStatus::Blocked)
            } else {
                Some(ConductorRunStatus::Failed)
            }
        }
        _ => None,
    }
}

impl ConductorActor {
    /// Attempt to restore in-progress run states from the event store.
    ///
    /// Called from `post_start`. Errors are logged but never propagated —
    /// durability is best-effort; a clean state is always safe to start with.
    pub(crate) async fn restore_run_states(&self, state: &mut ConductorState) {
        let latest_seq = match get_latest_seq(&state.event_store).await {
            Ok(Ok(Some(seq))) => seq,
            Ok(Ok(None)) => return,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Run state recovery: latest-seq query error");
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Run state recovery: latest-seq query failed");
                return;
            }
        };
        let since_seq = (latest_seq - RUN_RECOVERY_SCAN_LIMIT).max(0);
        let events = match get_recent_events(
            &state.event_store,
            since_seq,
            RUN_RECOVERY_SCAN_LIMIT,
            None,
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

        let mut projected: HashMap<String, ConductorRunState> = HashMap::new();

        for event in events {
            if !event.event_type.starts_with("conductor.task.")
                && event.event_type != "conductor.run.started"
            {
                continue;
            }
            // Extract run_id from payload.
            let run_id = match event.payload.get("run_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let status = match map_status_from_event(&event.event_type, &event.payload) {
                Some(value) => value,
                None => continue,
            };

            let run = projected
                .entry(run_id.clone())
                .or_insert_with(|| ConductorRunState {
                    run_id: run_id.clone(),
                    objective: "(unknown)".to_string(),
                    status: ConductorRunStatus::Initializing,
                    created_at: event.timestamp,
                    updated_at: event.timestamp,
                    completed_at: None,
                    agenda: vec![],
                    active_calls: vec![],
                    artifacts: vec![],
                    decision_log: vec![],
                    document_path: format!("conductor/runs/{run_id}/draft.md"),
                    output_mode: ConductorOutputMode::Auto,
                    desktop_id: "unknown".to_string(),
                });

            if let Some(objective) = payload_string(&event.payload, "objective") {
                if !objective.trim().is_empty() {
                    run.objective = objective;
                }
            }
            if let Some(desktop_id) = payload_string(&event.payload, "desktop_id") {
                if !desktop_id.trim().is_empty() {
                    run.desktop_id = desktop_id;
                }
            }
            if let Some(output_mode) = payload_output_mode(&event.payload) {
                run.output_mode = output_mode;
            }
            if let Some(document_path) = payload_string(&event.payload, "document_path") {
                if !document_path.trim().is_empty() {
                    run.document_path = document_path;
                }
            }

            run.status = status;
            run.updated_at = event.timestamp;
            if matches!(
                status,
                ConductorRunStatus::Completed
                    | ConductorRunStatus::Failed
                    | ConductorRunStatus::Blocked
            ) {
                run.completed_at = Some(event.timestamp);
            }
        }

        let mut restored = 0usize;
        for run in projected.into_values() {
            if state.tasks.get_run(&run.run_id).is_none() {
                state.tasks.insert_run(run);
                restored += 1;
            }
        }

        if restored > 0 {
            tracing::info!(restored, "Restored run states from event store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorMsg};
    use crate::actors::event_store::{
        AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
    };
    use ractor::{call, Actor};

    async fn append_event(
        store: &ractor::ActorRef<EventStoreMsg>,
        event_type: &str,
        payload: serde_json::Value,
    ) {
        let _ = call!(store, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: event_type.to_string(),
                payload,
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            reply,
        })
        .expect("append rpc failed")
        .expect("append failed");
    }

    #[tokio::test]
    async fn restores_run_objective_from_task_started_payload() {
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("event store spawn failed");

        append_event(
            &event_store,
            "conductor.task.started",
            serde_json::json!({
                "run_id": "run-alpha",
                "objective": "Investigate persistence bug",
                "desktop_id": "desktop-alpha",
                "status": "started",
            }),
        )
        .await;
        append_event(
            &event_store,
            "conductor.task.completed",
            serde_json::json!({
                "run_id": "run-alpha",
                "status": "completed",
            }),
        )
        .await;

        let (conductor, _handle) = Actor::spawn(
            None,
            ConductorActor,
            ConductorArguments {
                event_store: event_store.clone(),
                writer_supervisor: None,
                memory_actor: None,
            },
        )
        .await
        .expect("conductor spawn failed");

        let runs = call!(conductor, |reply| ConductorMsg::ListRuns { reply })
            .expect("list runs rpc failed");
        let restored = runs
            .into_iter()
            .find(|run| run.run_id == "run-alpha")
            .expect("restored run not found");

        assert_eq!(restored.objective, "Investigate persistence bug");
        assert_eq!(restored.desktop_id, "desktop-alpha");
        assert_eq!(restored.status, ConductorRunStatus::Completed);

        conductor.stop(None);
        event_store.stop(None);
    }

    #[test]
    fn payload_field_reads_nested_control_event_data() {
        let payload = serde_json::json!({
            "run_id": "run-nested",
            "data": {
                "objective": "Nested objective",
                "desktop_id": "desktop-nested"
            }
        });

        assert_eq!(
            payload_string(&payload, "objective").as_deref(),
            Some("Nested objective")
        );
        assert_eq!(
            payload_string(&payload, "desktop_id").as_deref(),
            Some("desktop-nested")
        );
    }
}
