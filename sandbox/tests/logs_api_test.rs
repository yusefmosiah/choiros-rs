//! Logs API Integration Tests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

use sandbox::actors::event_store::{AppendEvent, EventStoreMsg};
use sandbox::actors::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;

async fn setup_test_app() -> (
    axum::Router,
    tempfile::TempDir,
    ractor::ActorRef<EventStoreMsg>,
) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path_str.to_string()),
    )
    .await
    .expect("Failed to create event store");

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state);
    (app, temp_dir, event_store)
}

async fn json_response(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.expect("Request failed");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("Failed to read body")
        .to_bytes();
    let value: Value = serde_json::from_slice(&body).expect("Invalid JSON response");
    (status, value)
}

#[tokio::test]
async fn test_logs_events_returns_filtered_results() {
    let (app, _temp_dir, event_store) = setup_test_app().await;

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: "worker.task.started".to_string(),
            payload: serde_json::json!({"task_id":"t1"}),
            actor_id: "supervisor-1".to_string(),
            user_id: "user-1".to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: "chat.user_msg".to_string(),
            payload: serde_json::json!({"text":"hello"}),
            actor_id: "chat-1".to_string(),
            user_id: "user-1".to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/logs/events?event_type_prefix=worker.task&actor_id=supervisor-1&limit=10")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().expect("events array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "worker.task.started");
}

#[tokio::test]
async fn test_logs_events_limit_is_capped() {
    let (app, _temp_dir, event_store) = setup_test_app().await;

    for idx in 0..5 {
        let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.progress".to_string(),
                payload: serde_json::json!({"idx": idx}),
                actor_id: "supervisor-1".to_string(),
                user_id: "user-1".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();
    }

    let req = Request::builder()
        .method("GET")
        .uri("/logs/events?limit=2")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().expect("events array");
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_logs_events_jsonl_export() {
    let (app, _temp_dir, event_store) = setup_test_app().await;

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: "watcher.alert.failure_spike".to_string(),
            payload: serde_json::json!({"rule":"worker_failure_spike"}),
            actor_id: "watcher:default".to_string(),
            user_id: "system".to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/logs/events.jsonl?event_type_prefix=watcher.alert")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("Failed to read body")
        .to_bytes();
    let text = String::from_utf8(body.to_vec()).expect("utf8 body");
    let first_line = text.lines().next().expect("one line jsonl");
    let line_json: Value = serde_json::from_str(first_line).expect("jsonl line parse");

    assert!(
        content_type.starts_with("application/x-ndjson"),
        "unexpected content-type: {content_type}"
    );
    assert_eq!(line_json["event_type"], "watcher.alert.failure_spike");
}
