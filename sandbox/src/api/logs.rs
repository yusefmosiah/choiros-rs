//! Logs API endpoints
//!
//! Provides filtered event-log access for observability dashboards and watcher tooling.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use ractor::ActorRef;
use serde::Deserialize;
use serde_json::json;
use std::fmt::Write;

use super::ApiState;
use crate::actors::event_store::EventStoreMsg;

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub since_seq: Option<i64>,
    pub limit: Option<i64>,
    pub event_type_prefix: Option<String>,
    pub actor_id: Option<String>,
    pub user_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunLogQuery {
    pub since_seq: Option<i64>,
    pub limit: Option<i64>,
    pub actor_id: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub correlation_id: Option<String>,
}

async fn query_events(
    state: &ApiState,
    query: LogsQuery,
) -> Result<Vec<shared_types::Event>, String> {
    query_events_from_store(state.app_state.event_store(), query).await
}

pub(crate) async fn query_events_from_store(
    event_store: ActorRef<EventStoreMsg>,
    query: LogsQuery,
) -> Result<Vec<shared_types::Event>, String> {
    let since_seq = query.since_seq.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);

    match ractor::call!(event_store, |reply| {
        EventStoreMsg::GetRecentEvents {
            since_seq,
            limit,
            event_type_prefix: query.event_type_prefix,
            actor_id: query.actor_id,
            user_id: query.user_id,
            reply,
        }
    }) {
        Ok(Ok(mut events)) => {
            if let Some(ref run_id) = query.run_id {
                events.retain(|event| {
                    event
                        .payload
                        .get("run_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            event
                                .payload
                                .get("data")
                                .and_then(|d| d.get("run_id"))
                                .and_then(|v| v.as_str())
                        })
                        .map(|id| id == run_id)
                        .unwrap_or(false)
                });
            }
            Ok(events)
        }
        Ok(Err(err)) => Err(format!("EventStore error: {err}")),
        Err(err) => Err(format!("RPC error: {err}")),
    }
}

/// Get recent events with optional filters.
pub async fn get_events(
    State(state): State<ApiState>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    match query_events(&state, query).await {
        Ok(events) => (StatusCode::OK, Json(json!({ "events": events }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

#[derive(serde::Serialize)]
struct LatestSeqResponse {
    latest_seq: i64,
}

async fn event_exists_at_seq(
    event_store: ActorRef<EventStoreMsg>,
    seq: i64,
) -> Result<bool, String> {
    match ractor::call!(event_store, |reply| EventStoreMsg::GetEventBySeq {
        seq,
        reply
    }) {
        Ok(Ok(Some(_))) => Ok(true),
        Ok(Ok(None)) => Ok(false),
        Ok(Err(err)) => Err(format!("EventStore error: {err}")),
        Err(err) => Err(format!("RPC error: {err}")),
    }
}

async fn find_latest_seq(event_store: ActorRef<EventStoreMsg>) -> Result<i64, String> {
    if !event_exists_at_seq(event_store.clone(), 1).await? {
        return Ok(0);
    }

    let mut low = 1_i64;
    let mut high = 1_i64;

    loop {
        let next = high.saturating_mul(2);
        if next <= high {
            break;
        }
        if event_exists_at_seq(event_store.clone(), next).await? {
            low = next;
            high = next;
            continue;
        }
        high = next;
        break;
    }

    while low + 1 < high {
        let mid = low + ((high - low) / 2);
        if event_exists_at_seq(event_store.clone(), mid).await? {
            low = mid;
        } else {
            high = mid;
        }
    }

    Ok(low)
}

/// Get latest committed event sequence number.
pub async fn get_latest_seq(State(state): State<ApiState>) -> impl IntoResponse {
    match find_latest_seq(state.app_state.event_store()).await {
        Ok(latest_seq) => (
            StatusCode::OK,
            Json(json!(LatestSeqResponse { latest_seq })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

/// Export one run as a single markdown transcript.
///
/// Includes user prompts, system/model routing events, tool calls/results, worker lifecycle,
/// and assistant responses in chronological order.
pub async fn export_run_markdown(
    State(state): State<ApiState>,
    Query(query): Query<RunLogQuery>,
) -> impl IntoResponse {
    match build_run_markdown_from_store(state.app_state.event_store(), query).await {
        Ok(body) => (
            StatusCode::OK,
            [("content-type", "text/markdown; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(err) if err.starts_with("bad_request:") => (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/json")],
            json!({ "error": err.trim_start_matches("bad_request:") }).to_string(),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "application/json")],
            json!({ "error": err }).to_string(),
        )
            .into_response(),
    }
}

pub(crate) async fn build_run_markdown_from_store(
    event_store: ActorRef<EventStoreMsg>,
    query: RunLogQuery,
) -> Result<String, String> {
    if let Err(error) = validate_scope_pair(&query.session_id, &query.thread_id) {
        return Err(format!("bad_request:{error}"));
    }

    if query.actor_id.is_none()
        && query.run_id.is_none()
        && query.correlation_id.is_none()
        && query.session_id.is_none()
    {
        return Err(
            "bad_request:provide at least one selector: actor_id, run_id, session_id/thread_id, or correlation_id".to_string(),
        );
    }

    let base_query = LogsQuery {
        since_seq: query.since_seq,
        limit: Some(query.limit.unwrap_or(2000).clamp(1, 5000)),
        event_type_prefix: None,
        actor_id: query.actor_id.clone(),
        user_id: query.user_id.clone(),
        run_id: query.run_id.clone(),
    };

    let events = query_events_from_store(event_store, base_query).await?;
    let filtered = events
        .into_iter()
        .filter(|event| event_matches_run_filter(event, &query))
        .collect::<Vec<_>>();
    Ok(render_run_markdown(&filtered, &query))
}

pub(crate) fn validate_scope_pair(
    session_id: &Option<String>,
    thread_id: &Option<String>,
) -> Result<(), &'static str> {
    match (session_id, thread_id) {
        (Some(_), None) => Err("thread_id is required when session_id is provided"),
        (None, Some(_)) => Err("session_id is required when thread_id is provided"),
        _ => Ok(()),
    }
}

pub(crate) fn event_matches_run_filter(event: &shared_types::Event, query: &RunLogQuery) -> bool {
    if let (Some(session_id), Some(thread_id)) = (&query.session_id, &query.thread_id) {
        let payload_session = event
            .payload
            .get("scope")
            .and_then(|scope| scope.get("session_id"))
            .and_then(|v| v.as_str());
        let payload_thread = event
            .payload
            .get("scope")
            .and_then(|scope| scope.get("thread_id"))
            .and_then(|v| v.as_str());
        if payload_session != Some(session_id.as_str())
            || payload_thread != Some(thread_id.as_str())
        {
            return false;
        }
    }

    if let Some(correlation_id) = &query.correlation_id {
        let payload_correlation = event
            .payload
            .get("correlation_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                event
                    .payload
                    .get("task")
                    .and_then(|task| task.get("correlation_id").and_then(|v| v.as_str()))
            });
        if payload_correlation != Some(correlation_id.as_str()) {
            return false;
        }
    }

    if let Some(run_id) = &query.run_id {
        let payload_run = event
            .payload
            .get("run_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                event
                    .payload
                    .get("data")
                    .and_then(|data| data.get("run_id"))
                    .and_then(|v| v.as_str())
            });
        if payload_run != Some(run_id.as_str()) {
            return false;
        }
    }

    true
}

pub(crate) fn render_run_markdown(events: &[shared_types::Event], query: &RunLogQuery) -> String {
    let mut out = String::new();
    let _ = writeln!(&mut out, "# Run Log");
    let _ = writeln!(
        &mut out,
        "_Generated: {} UTC_",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
    );
    let _ = writeln!(&mut out);
    let _ = writeln!(&mut out, "## Filters");
    let _ = writeln!(
        &mut out,
        "- actor_id: `{}`",
        query.actor_id.clone().unwrap_or_else(|| "any".to_string())
    );
    let _ = writeln!(
        &mut out,
        "- session_id: `{}`",
        query
            .session_id
            .clone()
            .unwrap_or_else(|| "any".to_string())
    );
    let _ = writeln!(
        &mut out,
        "- thread_id: `{}`",
        query.thread_id.clone().unwrap_or_else(|| "any".to_string())
    );
    let _ = writeln!(
        &mut out,
        "- correlation_id: `{}`",
        query
            .correlation_id
            .clone()
            .unwrap_or_else(|| "any".to_string())
    );
    let _ = writeln!(
        &mut out,
        "- run_id: `{}`",
        query.run_id.clone().unwrap_or_else(|| "any".to_string())
    );
    let _ = writeln!(&mut out, "- events: `{}`", events.len());
    let _ = writeln!(&mut out);
    let _ = writeln!(&mut out, "## Timeline");
    let _ = writeln!(&mut out);

    for event in events {
        let worker_event = event.event_type.starts_with("worker.task.");
        let emitter = event
            .payload
            .get("emitter_actor")
            .and_then(|v| v.as_str())
            .unwrap_or(event.actor_id.as_str());
        if worker_event {
            let summary_hint = event
                .payload
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| event.payload.get("phase").and_then(|v| v.as_str()))
                .or_else(|| event.payload.get("status").and_then(|v| v.as_str()))
                .unwrap_or("worker update")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let _ = writeln!(&mut out, "<details>");
            let _ = writeln!(
                &mut out,
                "<summary>[{}] {} `{}` - {}</summary>",
                event.seq,
                event.timestamp.to_rfc3339(),
                event.event_type,
                summary_hint
            );
            let _ = writeln!(&mut out);
        } else {
            let _ = writeln!(
                &mut out,
                "### [{}] {} `{}`",
                event.seq,
                event.timestamp.to_rfc3339(),
                event.event_type
            );
        }
        let _ = writeln!(&mut out, "- scope actor: `{}`", event.actor_id.as_str());
        let _ = writeln!(&mut out, "- emitter: `{}`", emitter);
        let _ = writeln!(&mut out, "- user: `{}`", event.user_id);

        match event.event_type.as_str() {
            shared_types::EVENT_CHAT_USER_MSG => {
                if let Some(text) = shared_types::parse_chat_user_text(&event.payload) {
                    let _ = writeln!(&mut out);
                    let _ = writeln!(&mut out, "**User prompt**");
                    let _ = writeln!(&mut out);
                    let _ = writeln!(&mut out, "{}", text);
                }
            }
            shared_types::EVENT_CHAT_ASSISTANT_MSG => {
                let model = event
                    .payload
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let text = event
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let _ = writeln!(&mut out);
                let _ = writeln!(&mut out, "**Assistant message** (model `{}`)", model);
                let _ = writeln!(&mut out);
                let _ = writeln!(&mut out, "{}", text);
            }
            shared_types::EVENT_CHAT_TOOL_CALL => {
                let tool_name = event
                    .payload
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let reasoning = event
                    .payload
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let _ = writeln!(&mut out);
                let _ = writeln!(&mut out, "**Tool call** `{}`", tool_name);
                if !reasoning.is_empty() {
                    let _ = writeln!(&mut out, "- reasoning: {}", reasoning);
                }
            }
            shared_types::EVENT_CHAT_TOOL_RESULT => {
                let tool_name = event
                    .payload
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let success = event
                    .payload
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let output = event
                    .payload
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let _ = writeln!(&mut out);
                let _ = writeln!(
                    &mut out,
                    "**Tool result** `{}` success={}",
                    tool_name, success
                );
                if !output.is_empty() {
                    let _ = writeln!(&mut out);
                    let _ = writeln!(&mut out, "```text");
                    let _ = writeln!(&mut out, "{}", output);
                    let _ = writeln!(&mut out, "```");
                }
            }
            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED => {
                let error = event
                    .payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown failure");
                let kind = event
                    .payload
                    .get("failure_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let retriable = event
                    .payload
                    .get("failure_retriable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let hint = event
                    .payload
                    .get("failure_hint")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let duration_ms = event
                    .payload
                    .get("duration_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let _ = writeln!(&mut out);
                let _ = writeln!(
                    &mut out,
                    "**Worker failure** kind=`{kind}` retriable=`{retriable}`"
                );
                let _ = writeln!(&mut out, "- error: {}", error);
                if duration_ms > 0 {
                    let _ = writeln!(&mut out, "- duration_ms: {}", duration_ms);
                }
                if !hint.is_empty() {
                    let _ = writeln!(&mut out, "- hint: {}", hint);
                }
            }
            shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED => {
                let duration_ms = event
                    .payload
                    .get("duration_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = event
                    .payload
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let _ = writeln!(&mut out);
                let _ = writeln!(&mut out, "**Worker completed**");
                if duration_ms > 0 {
                    let _ = writeln!(&mut out, "- duration_ms: {}", duration_ms);
                }
                if !output.is_empty() {
                    let _ = writeln!(&mut out, "- output: {}", output);
                }
            }
            _ => {}
        }

        let _ = writeln!(&mut out);
        let _ = writeln!(&mut out, "<details>");
        let _ = writeln!(&mut out, "<summary>Raw payload</summary>");
        let _ = writeln!(&mut out);
        let _ = writeln!(&mut out, "```json");
        let payload_json = serde_json::to_string_pretty(&event.payload)
            .unwrap_or_else(|_| event.payload.to_string());
        let _ = writeln!(&mut out, "{}", payload_json);
        let _ = writeln!(&mut out, "```");
        let _ = writeln!(&mut out, "</details>");
        let _ = writeln!(&mut out);
        if worker_event {
            let _ = writeln!(&mut out, "</details>");
            let _ = writeln!(&mut out);
        }
    }

    out
}

/// Export recent events as JSONL/NDJSON (one event per line).
pub async fn export_events_jsonl(
    State(state): State<ApiState>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    match query_events(&state, query).await {
        Ok(events) => {
            let mut out = String::new();
            for event in events {
                match serde_json::to_string(&event) {
                    Ok(line) => {
                        out.push_str(&line);
                        out.push('\n');
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            [("content-type", "application/json")],
                            json!({ "error": format!("Serialization error: {e}") }).to_string(),
                        )
                            .into_response();
                    }
                }
            }
            (
                StatusCode::OK,
                [("content-type", "application/x-ndjson; charset=utf-8")],
                out,
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "application/json")],
            json!({ "error": err }).to_string(),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{AppendEvent, EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    #[tokio::test]
    async fn test_find_latest_seq_empty_store_returns_zero() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let latest = find_latest_seq(store_ref.clone()).await.unwrap();
        assert_eq!(latest, 0);

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_find_latest_seq_returns_tail_sequence() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        for idx in 0..5 {
            let event = AppendEvent {
                event_type: "chat.user_msg".to_string(),
                payload: serde_json::json!({ "idx": idx }),
                actor_id: "chat:test".to_string(),
                user_id: "user:test".to_string(),
            };
            let appended = ractor::call!(store_ref, |reply| EventStoreMsg::Append { event, reply })
                .unwrap()
                .unwrap();
            assert_eq!(appended.seq, idx + 1);
        }

        let latest = find_latest_seq(store_ref.clone()).await.unwrap();
        assert_eq!(latest, 5);

        store_ref.stop(None);
    }
}
