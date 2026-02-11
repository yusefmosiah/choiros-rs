//! Run-level observability API endpoints.
//!
//! Provides ordered run timelines for debugging, testing, and UI observability.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use ractor::ActorRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

use super::ApiState;
use crate::actors::event_store::EventStoreMsg;

#[derive(Debug, Deserialize)]
pub struct RunTimelineQuery {
    pub required_milestones: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTimeline {
    pub run_id: String,
    pub objective: String,
    pub status: String,
    pub timeline: Vec<TimelineEvent>,
    pub artifacts: Vec<RunArtifact>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub seq: i64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunArtifact {
    pub artifact_id: String,
    pub artifact_type: String,
    pub summary: String,
}

pub async fn get_run_timeline(
    Path(run_id): Path<String>,
    State(state): State<ApiState>,
    Query(query): Query<RunTimelineQuery>,
) -> impl IntoResponse {
    match build_run_timeline_from_store(state.app_state.event_store(), &run_id).await {
        Ok(timeline) => {
            let required = parse_required_milestones(query.required_milestones.as_deref());
            let missing = missing_required_milestones(&timeline.timeline, &required);
            if !missing.is_empty() {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "error": "missing required milestones",
                        "run_id": run_id,
                        "missing_milestones": missing,
                        "timeline": timeline,
                    })),
                )
                    .into_response();
            }
            (StatusCode::OK, Json(json!(timeline))).into_response()
        }
        Err(err) if err.starts_with("not_found:") => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": err.trim_start_matches("not_found:") })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

pub(crate) async fn build_run_timeline_from_store(
    event_store: ActorRef<EventStoreMsg>,
    run_id: &str,
) -> Result<RunTimeline, String> {
    let events = fetch_run_events(event_store, run_id).await?;
    if events.is_empty() {
        return Err(format!("not_found:run '{run_id}' not found"));
    }

    let objective = extract_objective(&events).unwrap_or_else(|| "unknown".to_string());
    let status = derive_run_status(&events);

    let timeline = events
        .iter()
        .map(|event| TimelineEvent {
            seq: event.seq,
            timestamp: event.timestamp,
            event_type: event.event_type.clone(),
            summary: extract_event_summary(&event.event_type, &event.payload),
        })
        .collect::<Vec<_>>();

    let artifacts = extract_artifacts(&events);

    Ok(RunTimeline {
        run_id: run_id.to_string(),
        objective,
        status,
        timeline,
        artifacts,
    })
}

async fn fetch_run_events(
    event_store: ActorRef<EventStoreMsg>,
    run_id: &str,
) -> Result<Vec<shared_types::Event>, String> {
    let mut since_seq = 0_i64;
    let mut collected = Vec::new();

    loop {
        let page = match ractor::call!(event_store, |reply| EventStoreMsg::GetRecentEvents {
            since_seq,
            limit: 1000,
            event_type_prefix: None,
            actor_id: None,
            user_id: None,
            reply,
        }) {
            Ok(Ok(events)) => events,
            Ok(Err(err)) => return Err(format!("EventStore error: {err}")),
            Err(err) => return Err(format!("RPC error: {err}")),
        };

        if page.is_empty() {
            break;
        }

        for event in &page {
            if event_belongs_to_run(event, run_id) {
                collected.push(event.clone());
            }
        }

        let last_seq = page.last().map(|e| e.seq).unwrap_or(since_seq);
        if last_seq <= since_seq {
            break;
        }
        since_seq = last_seq;
    }

    Ok(collected)
}

fn parse_required_milestones(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

fn missing_required_milestones(timeline: &[TimelineEvent], required: &[String]) -> Vec<String> {
    required
        .iter()
        .filter(|needle| {
            !timeline
                .iter()
                .any(|event| event.event_type == **needle || event.event_type.contains(*needle))
        })
        .cloned()
        .collect()
}

fn event_belongs_to_run(event: &shared_types::Event, run_id: &str) -> bool {
    payload_str(&event.payload, &["run_id"]) == Some(run_id)
        || payload_str(&event.payload, &["task_id"]) == Some(run_id)
        || payload_str(&event.payload, &["task", "task_id"]) == Some(run_id)
        || payload_str(&event.payload, &["data", "run_id"]) == Some(run_id)
        || payload_str(&event.payload, &["data", "task_id"]) == Some(run_id)
}

fn extract_objective(events: &[shared_types::Event]) -> Option<String> {
    for event in events {
        if let Some(objective) = payload_string(&event.payload, &["objective"]) {
            return Some(objective);
        }
        if let Some(objective) = payload_string(&event.payload, &["data", "objective"]) {
            return Some(objective);
        }
        if let Some(objective) = payload_string(&event.payload, &["worker_objective"]) {
            return Some(objective);
        }
    }
    None
}

fn derive_run_status(events: &[shared_types::Event]) -> String {
    let mut status = "running".to_string();

    for event in events {
        match event.event_type.as_str() {
            "conductor.task.completed" => return "completed".to_string(),
            "conductor.task.failed" => return "failed".to_string(),
            "conductor.run.blocked" | "conductor.capability.blocked" => {
                status = "blocked".to_string();
            }
            _ => {
                if let Some(s) = payload_str(&event.payload, &["status"]) {
                    status = s.to_string();
                }
            }
        }
    }

    status
}

fn extract_event_summary(event_type: &str, payload: &Value) -> String {
    let candidates = [
        ["summary"].as_slice(),
        ["message"].as_slice(),
        ["reason"].as_slice(),
        ["result_summary"].as_slice(),
        ["error_message"].as_slice(),
        ["worker_objective"].as_slice(),
        ["objective"].as_slice(),
        ["data", "summary"].as_slice(),
        ["data", "message"].as_slice(),
        ["data", "reason"].as_slice(),
        ["data", "error"].as_slice(),
        ["data", "claim"].as_slice(),
        ["data", "insight"].as_slice(),
    ];

    for path in candidates {
        if let Some(value) = payload_string(payload, path) {
            return value;
        }
    }

    let payload_fallback = payload.to_string();
    let truncated = if payload_fallback.len() > 200 {
        format!("{}...", &payload_fallback[..200])
    } else {
        payload_fallback
    };
    format!("{event_type}: {truncated}")
}

fn extract_artifacts(events: &[shared_types::Event]) -> Vec<RunArtifact> {
    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();

    for event in events {
        if let Some(report_path) = payload_string(&event.payload, &["report_path"]) {
            let key = format!("report_path:{report_path}");
            if seen.insert(key) {
                artifacts.push(RunArtifact {
                    artifact_id: format!("artifact-report-{}", event.seq),
                    artifact_type: "report_path".to_string(),
                    summary: report_path,
                });
            }
        }

        if matches!(
            event.event_type.as_str(),
            "conductor.worker.result"
                | "conductor.capability.completed"
                | "worker.task.finding"
                | "worker.task.learning"
        ) {
            let summary = extract_event_summary(&event.event_type, &event.payload);
            let key = format!("{}:{summary}", event.event_type);
            if seen.insert(key) {
                artifacts.push(RunArtifact {
                    artifact_id: format!("artifact-{}-{}", event.event_type, event.seq),
                    artifact_type: event.event_type.clone(),
                    summary,
                });
            }
        }
    }

    artifacts
}

fn payload_string(payload: &Value, path: &[&str]) -> Option<String> {
    payload_str(payload, path).map(ToString::to_string)
}

fn payload_str<'a>(payload: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = payload;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{AppendEvent, EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    #[tokio::test]
    async fn test_build_run_timeline_orders_events_and_extracts_artifacts() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let run_id = "run_123";
        let events = vec![
            AppendEvent {
                event_type: "conductor.task.started".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "objective": "test objective",
                    "status": "started",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            AppendEvent {
                event_type: "conductor.worker.result".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "result_summary": "worker finished",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            AppendEvent {
                event_type: "conductor.task.completed".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "status": "completed",
                    "report_path": "/tmp/report.md",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
        ];

        for event in events {
            let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append { event, reply })
                .unwrap()
                .unwrap();
        }

        let timeline = build_run_timeline_from_store(store_ref.clone(), run_id)
            .await
            .unwrap();
        assert_eq!(timeline.run_id, run_id);
        assert_eq!(timeline.objective, "test objective");
        assert_eq!(timeline.status, "completed");
        assert_eq!(timeline.timeline.len(), 3);
        assert!(timeline.timeline[0].seq < timeline.timeline[1].seq);
        assert!(timeline.timeline[1].seq < timeline.timeline[2].seq);
        assert!(timeline
            .artifacts
            .iter()
            .any(|a| a.artifact_type == "report_path"));

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_build_run_timeline_returns_not_found_for_unknown_run() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let event = AppendEvent {
            event_type: "conductor.task.started".to_string(),
            payload: json!({ "task_id": "different_run", "objective": "x" }),
            actor_id: "conductor:test".to_string(),
            user_id: "system".to_string(),
        };
        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append { event, reply })
            .unwrap()
            .unwrap();

        let err = build_run_timeline_from_store(store_ref.clone(), "missing_run")
            .await
            .unwrap_err();
        assert!(err.starts_with("not_found:"));

        store_ref.stop(None);
    }

    #[test]
    fn test_missing_required_milestones_detects_absent_events() {
        let timeline = vec![
            TimelineEvent {
                seq: 1,
                timestamp: chrono::Utc::now(),
                event_type: "conductor.task.started".to_string(),
                summary: "started".to_string(),
            },
            TimelineEvent {
                seq: 2,
                timestamp: chrono::Utc::now(),
                event_type: "conductor.worker.call".to_string(),
                summary: "call".to_string(),
            },
        ];

        let missing = missing_required_milestones(
            &timeline,
            &[
                "conductor.task.started".to_string(),
                "conductor.task.completed".to_string(),
            ],
        );

        assert_eq!(missing, vec!["conductor.task.completed".to_string()]);
    }
}
