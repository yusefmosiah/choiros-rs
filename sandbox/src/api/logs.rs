//! Logs API endpoints
//!
//! Provides filtered event-log access for observability dashboards and watcher tooling.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use super::ApiState;
use crate::actors::event_store::EventStoreMsg;

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub since_seq: Option<i64>,
    pub limit: Option<i64>,
    pub event_type_prefix: Option<String>,
    pub actor_id: Option<String>,
    pub user_id: Option<String>,
}

async fn query_events(
    state: &ApiState,
    query: LogsQuery,
) -> Result<Vec<shared_types::Event>, String> {
    let since_seq = query.since_seq.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);

    match ractor::call!(state.app_state.event_store(), |reply| {
        EventStoreMsg::GetRecentEvents {
            since_seq,
            limit,
            event_type_prefix: query.event_type_prefix,
            actor_id: query.actor_id,
            user_id: query.user_id,
            reply,
        }
    }) {
        Ok(Ok(events)) => Ok(events),
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
