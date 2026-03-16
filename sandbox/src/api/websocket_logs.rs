//! WebSocket handler for live event-log streaming.
//!
//! Streams committed EventStore rows with optional filter query params.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::actors::event_store::EventStoreMsg;
use crate::api::logs::{event_matches_run_filter, validate_scope_pair, RunLogQuery};
use crate::api::ApiState;

pub async fn logs_websocket(
    ws: WebSocketUpgrade,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let since_seq = query
        .get("since_seq")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let limit = query
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(200)
        .clamp(1, 500);
    let event_type_prefix = query.get("event_type_prefix").cloned();
    let actor_id = query.get("actor_id").cloned();
    let user_id = query.get("user_id").cloned();
    let session_id = query.get("session_id").cloned();
    let thread_id = query.get("thread_id").cloned();
    let run_id = query.get("run_id").cloned();
    let correlation_id = query.get("correlation_id").cloned();
    let poll_ms = query
        .get("poll_ms")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(250)
        .clamp(50, 5_000);

    if let Err(error) = validate_scope_pair(&session_id, &thread_id) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
    }

    let run_filter = RunLogQuery {
        since_seq: None,
        limit: None,
        actor_id: actor_id.clone(),
        user_id: user_id.clone(),
        session_id,
        thread_id,
        run_id,
        correlation_id,
    };

    let event_store = state.app_state.event_store();
    ws.on_upgrade(move |socket| {
        handle_logs_socket(
            socket,
            event_store,
            since_seq,
            limit,
            event_type_prefix,
            actor_id,
            user_id,
            poll_ms,
            run_filter,
        )
    })
}

async fn handle_logs_socket(
    socket: WebSocket,
    event_store: ractor::ActorRef<EventStoreMsg>,
    mut since_seq: i64,
    limit: i64,
    event_type_prefix: Option<String>,
    actor_id: Option<String>,
    user_id: Option<String>,
    poll_ms: u64,
    run_filter: RunLogQuery,
) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let _ = tx.send(Message::Text(
        json!({
            "type": "connected",
            "since_seq": since_seq,
            "limit": limit,
            "event_type_prefix": event_type_prefix.clone(),
            "actor_id": actor_id.clone(),
            "user_id": user_id.clone(),
            "session_id": run_filter.session_id.clone(),
            "thread_id": run_filter.thread_id.clone(),
            "run_id": run_filter.run_id.clone(),
            "correlation_id": run_filter.correlation_id.clone(),
            "poll_ms": poll_ms,
        })
        .to_string()
        .into(),
    ));

    loop {
        tokio::select! {
            maybe_msg = receiver.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
                        if parsed.get("type").and_then(|v| v.as_str()) == Some("ping") {
                            let _ = tx.send(Message::Text(json!({"type":"pong"}).to_string().into()));
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = tx.send(Message::Pong(data));
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::warn!(error=%e, "logs websocket receive error");
                        break;
                    }
                }
            }
            _ = sleep(Duration::from_millis(poll_ms)) => {
                let recent = match ractor::call!(event_store, |reply| EventStoreMsg::GetRecentEvents {
                    since_seq,
                    limit,
                    event_type_prefix: event_type_prefix.clone(),
                    actor_id: actor_id.clone(),
                    user_id: user_id.clone(),
                    reply
                }) {
                    Ok(Ok(events)) => events,
                    Ok(Err(e)) => {
                        tracing::warn!(error=%e, "logs websocket query failed");
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(error=%e, "logs websocket event store RPC failed");
                        continue;
                    }
                };

                for event in recent {
                    since_seq = since_seq.max(event.seq);
                    if !event_matches_run_filter(&event, &run_filter) {
                        continue;
                    }
                    let _ = tx.send(Message::Text(
                        json!({
                            "type": "event",
                            "seq": event.seq,
                            "event_id": event.event_id,
                            "timestamp": event.timestamp.to_rfc3339(),
                            "event_type": event.event_type,
                            "actor_id": event.actor_id.0,
                            "user_id": event.user_id,
                            "payload": event.payload,
                        })
                        .to_string()
                        .into(),
                    ));
                }
            }
        }
    }

    writer.abort();
}
