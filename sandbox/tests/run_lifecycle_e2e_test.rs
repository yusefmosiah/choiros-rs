//! Live E2E tests for conductor run lifecycle and observability.
//!
//! These tests intentionally exercise the real conductor policy + worker loop
//! with external model providers.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tower::ServiceExt;

use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;
use sandbox::runtime_env::ensure_tls_cert_env;

const LIVE_RUN_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
static LIVE_E2E_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn live_e2e_guard() -> tokio::sync::MutexGuard<'static, ()> {
    LIVE_E2E_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

async fn setup_test_app() -> (axum::Router, tempfile::TempDir) {
    let _ = ensure_tls_cert_env();

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

    let app_state = Arc::new(AppState::new(event_store));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state);
    (app, temp_dir)
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

async fn submit_live_run(app: &axum::Router, objective: &str) -> String {
    let execute_req = json!({
        "objective": objective,
        "desktop_id": "test-desktop-live-e2e",
        "output_mode": "markdown_report_to_writer",
        "hints": null,
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .expect("build execute request");

    let (status, body) = json_response(app, req).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "expected accepted execute response, got body={body}"
    );

    body["run_id"]
        .as_str()
        .expect("run_id should be present")
        .to_string()
}

async fn get_run_status(app: &axum::Router, run_id: &str) -> String {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/conductor/runs/{run_id}"))
        .body(Body::empty())
        .expect("build run status request");

    let (status, body) = json_response(app, req).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "expected run status 200 for {run_id}, got body={body}"
    );

    body["status"]
        .as_str()
        .expect("status field should exist")
        .to_string()
}

async fn get_events_by_run_id(app: &axum::Router, run_id: &str) -> Vec<Value> {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/logs/events?run_id={run_id}&limit=1000"))
        .body(Body::empty())
        .expect("build logs request");

    let (status, body) = json_response(app, req).await;
    assert_eq!(status, StatusCode::OK, "logs API error: {body}");

    body["events"]
        .as_array()
        .expect("events should be an array")
        .clone()
}

async fn wait_for_event_count(app: &axum::Router, run_id: &str, min_count: usize) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + LIVE_RUN_TIMEOUT;

    loop {
        let events = get_events_by_run_id(app, run_id).await;
        if events.len() >= min_count {
            return events;
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("run {run_id} did not produce at least {min_count} events in time");
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_for_event_type(app: &axum::Router, run_id: &str, needle: &str) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + LIVE_RUN_TIMEOUT;

    loop {
        let events = get_events_by_run_id(app, run_id).await;
        if events.iter().any(|event| {
            event["event_type"]
                .as_str()
                .map(|event_type| event_type.contains(needle))
                .unwrap_or(false)
        }) {
            return events;
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("run {run_id} did not emit event containing '{needle}'");
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test]
async fn test_live_basic_run_flow_emits_required_milestones() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app().await;

    let run_id = submit_live_run(
        &app,
        "Run a simple terminal validation and summarize outcome in one line",
    )
    .await;

    let _ = wait_for_event_type(&app, &run_id, "conductor.task.started").await;
    let _ = wait_for_event_type(&app, &run_id, "conductor.run.started").await;
    let events = wait_for_event_type(&app, &run_id, "conductor.worker.call").await;
    let status = get_run_status(&app, &run_id).await;

    let event_types: Vec<&str> = events
        .iter()
        .map(|e| e["event_type"].as_str().unwrap_or(""))
        .collect();

    assert!(
        event_types
            .iter()
            .any(|e| e.contains("conductor.task.started")),
        "missing conductor.task.started event: {:?}",
        event_types
    );
    assert!(
        event_types
            .iter()
            .any(|e| e.contains("conductor.worker.call")),
        "missing conductor.worker.call event: {:?}",
        event_types
    );

    assert!(
        matches!(
            status.as_str(),
            "initializing"
                | "running"
                | "waiting_for_calls"
                | "completing"
                | "completed"
                | "failed"
                | "blocked"
        ),
        "unexpected run status for {run_id}: {status}"
    );
}

#[tokio::test]
async fn test_live_run_id_is_stable_across_events() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app().await;

    let run_id = submit_live_run(
        &app,
        "Run terminal validation and return one concise status line",
    )
    .await;

    let events = wait_for_event_count(&app, &run_id, 3).await;

    let mut mismatched = Vec::new();
    for event in &events {
        let payload_run_id = event["payload"]
            .get("run_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                event["payload"]
                    .get("task")
                    .and_then(|t| t.get("run_id"))
                    .and_then(|v| v.as_str())
            });

        if let Some(found) = payload_run_id {
            if found != run_id {
                mismatched.push((event["event_type"].clone(), found.to_string()));
            }
        }
    }

    assert!(
        mismatched.is_empty(),
        "found mismatched run IDs: {:?}",
        mismatched
    );
}

#[tokio::test]
async fn test_live_stream_produces_events_before_terminal_state() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app().await;

    let run_id = submit_live_run(
        &app,
        "Gather evidence for current Rust async debugging workflows and summarize",
    )
    .await;

    let deadline = tokio::time::Instant::now() + LIVE_RUN_TIMEOUT;
    let mut saw_non_terminal_events_while_running = false;

    loop {
        let status = get_run_status(&app, &run_id).await;
        let events = get_events_by_run_id(&app, &run_id).await;

        if status != "completed" && status != "failed" && status != "blocked" && !events.is_empty()
        {
            saw_non_terminal_events_while_running = true;
            break;
        }

        if status == "completed" || status == "failed" || status == "blocked" {
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("run {run_id} did not provide observable streaming progress in time");
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    assert!(
        saw_non_terminal_events_while_running,
        "no events observed while run {run_id} was in non-terminal state"
    );
}

#[tokio::test]
async fn test_live_concurrent_runs_have_isolated_run_ids() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app().await;

    let run1 = submit_live_run(&app, "Run terminal check for concurrency path one").await;
    let run2 = submit_live_run(&app, "Run terminal check for concurrency path two").await;

    assert_ne!(run1, run2, "concurrent runs must have unique run IDs");

    let req1 = Request::builder()
        .method("GET")
        .uri(format!("/conductor/runs/{run1}"))
        .body(Body::empty())
        .expect("build status request 1");
    let (_, body1) = json_response(&app, req1).await;

    let req2 = Request::builder()
        .method("GET")
        .uri(format!("/conductor/runs/{run2}"))
        .body(Body::empty())
        .expect("build status request 2");
    let (_, body2) = json_response(&app, req2).await;

    assert_eq!(body1["run_id"], run1);
    assert_eq!(body2["run_id"], run2);
}
