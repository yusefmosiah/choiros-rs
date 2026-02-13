//! Run-level observability API endpoints.
//!
//! Provides ordered run timelines for debugging, testing, and UI observability.
//!
//! Based on Conductor E2E intelligence report (2026-02-10):
//! - Events are categorized into conductor_decisions, agent_objectives, agent_planning, agent_results
//! - Timeline reflects actual Conductor behavior: bootstrap -> dispatch -> worker execution -> synthesis

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

/// Query parameters for run timeline endpoint
#[derive(Debug, Deserialize)]
pub struct RunTimelineQuery {
    /// Filter by event category (conductor_decisions, agent_objectives, agent_planning, agent_results)
    pub category: Option<String>,
    /// Require specific event types to be present (comma-separated)
    pub required_milestones: Option<String>,
}

/// Event categories based on actual Conductor behavior
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// Conductor policy decisions: Dispatch, Retry, SpawnFollowup, Continue, Complete, Block
    ConductorDecisions,
    /// Natural language objectives passed to agents
    AgentObjectives,
    /// Planning steps from agents (bootstrap, agenda creation)
    AgentPlanning,
    /// Findings, learnings, artifacts from agent execution
    AgentResults,
    /// System/telemetry events not in above categories
    System,
}

impl std::fmt::Display for EventCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventCategory::ConductorDecisions => write!(f, "conductor_decisions"),
            EventCategory::AgentObjectives => write!(f, "agent_objectives"),
            EventCategory::AgentPlanning => write!(f, "agent_planning"),
            EventCategory::AgentResults => write!(f, "agent_results"),
            EventCategory::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for EventCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "conductor_decisions" => Ok(EventCategory::ConductorDecisions),
            "agent_objectives" => Ok(EventCategory::AgentObjectives),
            "agent_planning" => Ok(EventCategory::AgentPlanning),
            "agent_results" => Ok(EventCategory::AgentResults),
            "system" => Ok(EventCategory::System),
            _ => Err(format!("unknown category: {}", s)),
        }
    }
}

/// Response structure for run timeline endpoint
#[derive(Debug, Clone, Serialize)]
pub struct RunTimelineResponse {
    pub run_id: String,
    pub events: Vec<TimelineEvent>,
    pub summary: RunSummary,
}

/// Individual timeline event with categorization
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub timestamp: String,
    pub category: EventCategory,
    pub event_type: String,
    pub data: Value,
}

/// Summary statistics for the run
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub objective: String,
    pub status: String,
    pub total_events: usize,
    pub event_counts_by_category: Value,
    pub decisions: Vec<DecisionSummary>,
    pub artifacts: Vec<ArtifactSummary>,
}

/// Summary of a conductor decision
#[derive(Debug, Clone, Serialize)]
pub struct DecisionSummary {
    pub decision_type: String,
    pub timestamp: String,
    pub reason: String,
}

/// Summary of an artifact produced during the run
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactSummary {
    pub artifact_id: String,
    pub artifact_type: String,
    pub summary: String,
}

/// Get run timeline endpoint
///
/// Returns a categorized timeline of events for a Conductor run.
/// Events are categorized based on actual Conductor behavior:
/// - `conductor_decisions`: Policy decisions (Dispatch, Retry, SpawnFollowup, Continue, Complete, Block)
/// - `agent_objectives`: Natural language objectives passed to agents
/// - `agent_planning`: Planning steps (bootstrap, agenda creation)
/// - `agent_results`: Findings, learnings, artifacts from execution
///
/// Query params:
/// - `category`: Filter by specific category
/// - `required_milestones`: Comma-separated list of required event types
pub async fn get_run_timeline(
    Path(run_id): Path<String>,
    State(state): State<ApiState>,
    Query(query): Query<RunTimelineQuery>,
) -> impl IntoResponse {
    // Build the new categorized timeline
    match build_categorized_timeline(state.app_state.event_store(), &run_id, &query).await {
        Ok(response) => {
            // Check required milestones if specified
            if let Some(required_str) = &query.required_milestones {
                let required: Vec<String> = required_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                let missing: Vec<String> = required
                    .iter()
                    .filter(|req| {
                        !response
                            .events
                            .iter()
                            .any(|e| e.event_type == **req || e.event_type.contains(*req))
                    })
                    .cloned()
                    .collect();

                if !missing.is_empty() {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({
                            "error": "missing required milestones",
                            "run_id": run_id,
                            "missing_milestones": missing,
                            "timeline": response,
                        })),
                    )
                        .into_response();
                }
            }

            (StatusCode::OK, Json(json!(response))).into_response()
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

/// Build a categorized timeline from EventStore
async fn build_categorized_timeline(
    event_store: ActorRef<EventStoreMsg>,
    run_id: &str,
    query: &RunTimelineQuery,
) -> Result<RunTimelineResponse, String> {
    let events = fetch_run_events(event_store, run_id).await?;
    if events.is_empty() {
        return Err(format!("not_found:run '{}' not found", run_id));
    }

    // Categorize and filter events
    let mut timeline_events: Vec<TimelineEvent> = events
        .iter()
        .filter_map(|event| categorize_event(event, query.category.as_deref()))
        .collect();

    // Sort by timestamp
    timeline_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Build summary
    let summary = build_run_summary(&events, &timeline_events, run_id);

    Ok(RunTimelineResponse {
        run_id: run_id.to_string(),
        events: timeline_events,
        summary,
    })
}

/// Categorize a single event based on its type and payload
fn categorize_event(
    event: &shared_types::Event,
    category_filter: Option<&str>,
) -> Option<TimelineEvent> {
    let category = determine_event_category(&event.event_type, &event.payload);

    // Apply category filter if specified
    if let Some(filter) = category_filter {
        let filter_cat = filter.parse::<EventCategory>().ok()?;
        if category != filter_cat {
            return None;
        }
    }

    let timestamp = event.timestamp.to_rfc3339();

    // Build the data payload - include relevant fields based on event type
    let data = build_event_data(&event.event_type, &event.payload, event.seq);

    Some(TimelineEvent {
        timestamp,
        category,
        event_type: event.event_type.clone(),
        data,
    })
}

/// Determine the category of an event based on its type and content
fn determine_event_category(event_type: &str, _payload: &Value) -> EventCategory {
    // Conductor decisions
    if event_type.contains("decision")
        || matches!(
            event_type,
            "conductor.capability.completed"
                | "conductor.capability.failed"
                | "conductor.capability.blocked"
                | "conductor.escalation"
        )
    {
        return EventCategory::ConductorDecisions;
    }

    // Agent objectives - worker calls with natural language objectives
    if event_type.contains("worker.call") || event_type.contains("bootstrap") {
        return EventCategory::AgentObjectives;
    }

    // Agent planning - agenda creation, task decomposition
    if event_type.contains("agenda") || event_type.contains("bootstrap.completed") {
        return EventCategory::AgentPlanning;
    }

    // Agent results - findings, learnings, artifacts, completions
    if event_type.contains("finding")
        || event_type.contains("learning")
        || event_type.contains("artifact")
        || event_type.contains("worker.result")
        || event_type.contains("task.completed")
        || event_type.contains("task.failed")
    {
        return EventCategory::AgentResults;
    }

    // Default to system
    EventCategory::System
}

/// Build the data payload for a timeline event
fn build_event_data(event_type: &str, payload: &Value, seq: i64) -> Value {
    let mut data = serde_json::Map::new();

    // Always include seq for reference
    data.insert("seq".to_string(), json!(seq));

    // Include relevant fields based on event type
    match event_type {
        "conductor.decision" => {
            if let Some(decision_type) = payload
                .get("decision_type")
                .or_else(|| payload.get("data").and_then(|d| d.get("decision_type")))
            {
                data.insert("decision_type".to_string(), decision_type.clone());
            }
            if let Some(reason) = payload
                .get("reason")
                .or_else(|| payload.get("data").and_then(|d| d.get("reason")))
            {
                data.insert("reason".to_string(), reason.clone());
            }
        }
        "conductor.worker.call" | "conductor.bootstrap.completed" => {
            if let Some(objective) = payload
                .get("worker_objective")
                .or_else(|| payload.get("objective"))
            {
                data.insert("objective".to_string(), objective.clone());
            }
            if let Some(capability) = payload
                .get("worker_type")
                .or_else(|| payload.get("capability"))
            {
                data.insert("capability".to_string(), capability.clone());
            }
        }
        "conductor.agenda.created" => {
            if let Some(agenda_data) = payload.get("data").or(Some(payload)) {
                data.insert("agenda".to_string(), agenda_data.clone());
            }
        }
        "conductor.finding" => {
            if let Some(claim) = payload
                .get("claim")
                .or_else(|| payload.get("data").and_then(|d| d.get("claim")))
            {
                data.insert("claim".to_string(), claim.clone());
            }
            if let Some(confidence) = payload
                .get("confidence")
                .or_else(|| payload.get("data").and_then(|d| d.get("confidence")))
            {
                data.insert("confidence".to_string(), confidence.clone());
            }
        }
        "conductor.learning" => {
            if let Some(insight) = payload
                .get("insight")
                .or_else(|| payload.get("data").and_then(|d| d.get("insight")))
            {
                data.insert("insight".to_string(), insight.clone());
            }
        }
        "conductor.worker.result" | "conductor.capability.completed" => {
            if let Some(summary) = payload
                .get("result_summary")
                .or_else(|| payload.get("summary"))
            {
                data.insert("summary".to_string(), summary.clone());
            }
            if let Some(success) = payload.get("success") {
                data.insert("success".to_string(), success.clone());
            }
        }
        "conductor.task.completed" | "conductor.task.failed" => {
            if let Some(status) = payload.get("status") {
                data.insert("status".to_string(), status.clone());
            }
            if let Some(report_path) = payload.get("report_path") {
                data.insert("report_path".to_string(), report_path.clone());
            }
            if let Some(error) = payload
                .get("error_message")
                .or_else(|| payload.get("error"))
            {
                data.insert("error".to_string(), error.clone());
            }
        }
        _ => {
            // For other events, include the full payload but limit size
            data.insert("payload".to_string(), payload.clone());
        }
    }

    Value::Object(data)
}

/// Build summary statistics for the run
fn build_run_summary(
    all_events: &[shared_types::Event],
    timeline_events: &[TimelineEvent],
    _run_id: &str,
) -> RunSummary {
    let objective = extract_objective(all_events).unwrap_or_else(|| "unknown".to_string());
    let status = derive_run_status(all_events);

    // Count events by category
    let mut conductor_decisions = 0;
    let mut agent_objectives = 0;
    let mut agent_planning = 0;
    let mut agent_results = 0;
    let mut system = 0;

    for event in timeline_events {
        match event.category {
            EventCategory::ConductorDecisions => conductor_decisions += 1,
            EventCategory::AgentObjectives => agent_objectives += 1,
            EventCategory::AgentPlanning => agent_planning += 1,
            EventCategory::AgentResults => agent_results += 1,
            EventCategory::System => system += 1,
        }
    }

    // Extract decisions
    let decisions: Vec<DecisionSummary> = all_events
        .iter()
        .filter(|e| e.event_type.contains("decision"))
        .filter_map(|e| {
            let decision_type = e
                .payload
                .get("decision_type")
                .or_else(|| e.payload.get("data").and_then(|d| d.get("decision_type")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let reason = e
                .payload
                .get("reason")
                .or_else(|| e.payload.get("data").and_then(|d| d.get("reason")))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            Some(DecisionSummary {
                decision_type: decision_type.to_string(),
                timestamp: e.timestamp.to_rfc3339(),
                reason: reason.to_string(),
            })
        })
        .collect();

    // Extract artifacts
    let artifacts = extract_artifact_summaries(all_events);

    RunSummary {
        objective,
        status,
        total_events: timeline_events.len(),
        event_counts_by_category: json!({
            "conductor_decisions": conductor_decisions,
            "agent_objectives": agent_objectives,
            "agent_planning": agent_planning,
            "agent_results": agent_results,
            "system": system,
        }),
        decisions,
        artifacts,
    }
}

/// Extract artifact summaries from events
fn extract_artifact_summaries(events: &[shared_types::Event]) -> Vec<ArtifactSummary> {
    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();

    for event in events {
        // Report paths
        if let Some(report_path) = payload_string(&event.payload, &["report_path"]) {
            let key = format!("report_path:{}", report_path);
            if seen.insert(key) {
                artifacts.push(ArtifactSummary {
                    artifact_id: format!("artifact-report-{}", event.seq),
                    artifact_type: "report_path".to_string(),
                    summary: report_path,
                });
            }
        }

        // Worker results
        if matches!(
            event.event_type.as_str(),
            "conductor.worker.result"
                | "conductor.capability.completed"
                | "worker.task.finding"
                | "worker.task.learning"
        ) {
            let summary = extract_event_summary(&event.event_type, &event.payload);
            let key = format!("{}:{}", event.event_type, summary);
            if seen.insert(key) {
                artifacts.push(ArtifactSummary {
                    artifact_id: format!("artifact-{}-{}", event.event_type, event.seq),
                    artifact_type: event.event_type.clone(),
                    summary,
                });
            }
        }
    }

    artifacts
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


    // ============================================================================
    // Categorized Timeline Tests
    // ============================================================================

    #[test]
    fn test_determine_event_category_conductor_decisions() {
        let payload = json!({});
        assert_eq!(
            determine_event_category("conductor.decision", &payload),
            EventCategory::ConductorDecisions
        );
        assert_eq!(
            determine_event_category("conductor.capability.completed", &payload),
            EventCategory::ConductorDecisions
        );
        assert_eq!(
            determine_event_category("conductor.capability.failed", &payload),
            EventCategory::ConductorDecisions
        );
        assert_eq!(
            determine_event_category("conductor.capability.blocked", &payload),
            EventCategory::ConductorDecisions
        );
        assert_eq!(
            determine_event_category("conductor.escalation", &payload),
            EventCategory::ConductorDecisions
        );
    }

    #[test]
    fn test_determine_event_category_agent_objectives() {
        let payload = json!({});
        assert_eq!(
            determine_event_category("conductor.worker.call", &payload),
            EventCategory::AgentObjectives
        );
        assert_eq!(
            determine_event_category("conductor.bootstrap.completed", &payload),
            EventCategory::AgentObjectives
        );
    }

    #[test]
    fn test_determine_event_category_agent_planning() {
        let payload = json!({});
        assert_eq!(
            determine_event_category("conductor.agenda.created", &payload),
            EventCategory::AgentPlanning
        );
    }

    #[test]
    fn test_determine_event_category_agent_results() {
        let payload = json!({});
        assert_eq!(
            determine_event_category("conductor.worker.result", &payload),
            EventCategory::AgentResults
        );
        assert_eq!(
            determine_event_category("conductor.task.completed", &payload),
            EventCategory::AgentResults
        );
        assert_eq!(
            determine_event_category("conductor.task.failed", &payload),
            EventCategory::AgentResults
        );
        assert_eq!(
            determine_event_category("conductor.finding", &payload),
            EventCategory::AgentResults
        );
        assert_eq!(
            determine_event_category("conductor.learning", &payload),
            EventCategory::AgentResults
        );
        assert_eq!(
            determine_event_category("conductor.artifact.created", &payload),
            EventCategory::AgentResults
        );
    }

    #[test]
    fn test_determine_event_category_system() {
        let payload = json!({});
        assert_eq!(
            determine_event_category("conductor.task.started", &payload),
            EventCategory::System
        );
        assert_eq!(
            determine_event_category("conductor.progress", &payload),
            EventCategory::System
        );
        assert_eq!(
            determine_event_category("terminal.tool.call", &payload),
            EventCategory::System
        );
    }

    #[test]
    fn test_build_event_data_decision() {
        let payload = json!({
            "decision_type": "Dispatch",
            "reason": "Both capabilities ready"
        });
        let data = build_event_data("conductor.decision", &payload, 42);

        assert_eq!(data.get("seq").unwrap(), 42);
        assert_eq!(data.get("decision_type").unwrap(), "Dispatch");
        assert_eq!(data.get("reason").unwrap(), "Both capabilities ready");
    }

    #[test]
    fn test_build_event_data_worker_call() {
        let payload = json!({
            "worker_objective": "Execute ls -la",
            "worker_type": "terminal"
        });
        let data = build_event_data("conductor.worker.call", &payload, 1);

        assert_eq!(data.get("seq").unwrap(), 1);
        assert_eq!(data.get("objective").unwrap(), "Execute ls -la");
        assert_eq!(data.get("capability").unwrap(), "terminal");
    }

    #[test]
    fn test_build_event_data_finding() {
        let payload = json!({
            "claim": "Rust 1.75 was released",
            "confidence": 0.95
        });
        let data = build_event_data("conductor.finding", &payload, 5);

        assert_eq!(data.get("seq").unwrap(), 5);
        assert_eq!(data.get("claim").unwrap(), "Rust 1.75 was released");
        assert_eq!(data.get("confidence").unwrap(), 0.95);
    }

    #[test]
    fn test_event_category_from_str() {
        assert_eq!(
            "conductor_decisions".parse::<EventCategory>().unwrap(),
            EventCategory::ConductorDecisions
        );
        assert_eq!(
            "agent_objectives".parse::<EventCategory>().unwrap(),
            EventCategory::AgentObjectives
        );
        assert_eq!(
            "agent_planning".parse::<EventCategory>().unwrap(),
            EventCategory::AgentPlanning
        );
        assert_eq!(
            "agent_results".parse::<EventCategory>().unwrap(),
            EventCategory::AgentResults
        );
        assert_eq!(
            "system".parse::<EventCategory>().unwrap(),
            EventCategory::System
        );
        assert!("unknown".parse::<EventCategory>().is_err());
    }

    #[test]
    fn test_event_category_display() {
        assert_eq!(
            EventCategory::ConductorDecisions.to_string(),
            "conductor_decisions"
        );
        assert_eq!(
            EventCategory::AgentObjectives.to_string(),
            "agent_objectives"
        );
        assert_eq!(EventCategory::AgentPlanning.to_string(), "agent_planning");
        assert_eq!(EventCategory::AgentResults.to_string(), "agent_results");
        assert_eq!(EventCategory::System.to_string(), "system");
    }

    #[tokio::test]
    async fn test_categorized_timeline_filters_by_category() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let run_id = "run_categorized";

        // Insert events of different categories
        let events = vec![
            // System event
            AppendEvent {
                event_type: "conductor.task.started".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "objective": "test objective",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            // Agent objective
            AppendEvent {
                event_type: "conductor.worker.call".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "worker_objective": "Execute ls",
                    "worker_type": "terminal",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            // Decision
            AppendEvent {
                event_type: "conductor.decision".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "decision_type": "Dispatch",
                    "reason": "Ready to execute",
                }),
                actor_id: "conductor:test".to_string(),
                user_id: "system".to_string(),
            },
            // Agent result
            AppendEvent {
                event_type: "conductor.task.completed".to_string(),
                payload: json!({
                    "task_id": run_id,
                    "status": "completed",
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

        // Test no filter - all events
        let query = RunTimelineQuery {
            category: None,
            required_milestones: None,
        };
        let response = build_categorized_timeline(store_ref.clone(), run_id, &query)
            .await
            .unwrap();
        assert_eq!(response.events.len(), 4);

        // Test filter by conductor_decisions
        let query = RunTimelineQuery {
            category: Some("conductor_decisions".to_string()),
            required_milestones: None,
        };
        let response = build_categorized_timeline(store_ref.clone(), run_id, &query)
            .await
            .unwrap();
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0].category, EventCategory::ConductorDecisions);

        // Test filter by agent_objectives
        let query = RunTimelineQuery {
            category: Some("agent_objectives".to_string()),
            required_milestones: None,
        };
        let response = build_categorized_timeline(store_ref.clone(), run_id, &query)
            .await
            .unwrap();
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0].category, EventCategory::AgentObjectives);

        // Test filter by agent_results
        let query = RunTimelineQuery {
            category: Some("agent_results".to_string()),
            required_milestones: None,
        };
        let response = build_categorized_timeline(store_ref.clone(), run_id, &query)
            .await
            .unwrap();
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0].category, EventCategory::AgentResults);

        // Test summary counts
        let query = RunTimelineQuery {
            category: None,
            required_milestones: None,
        };
        let response = build_categorized_timeline(store_ref.clone(), run_id, &query)
            .await
            .unwrap();
        assert_eq!(response.summary.total_events, 4);
        assert_eq!(
            response.summary.event_counts_by_category.get("conductor_decisions").unwrap(),
            1
        );
        assert_eq!(
            response.summary.event_counts_by_category.get("agent_objectives").unwrap(),
            1
        );
        assert_eq!(
            response.summary.event_counts_by_category.get("agent_results").unwrap(),
            1
        );
        assert_eq!(
            response.summary.event_counts_by_category.get("system").unwrap(),
            1
        );

        store_ref.stop(None);
    }
}
