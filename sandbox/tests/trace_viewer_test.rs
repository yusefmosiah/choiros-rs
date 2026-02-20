//! Trace Viewer Integration Tests
//!
//! Covers backend event queryability and timeline semantics required by
//! docs/design/2026-02-19-agent-trajectory-viewer.md.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

use sandbox::actors::event_store::{AppendEvent, EventStoreMsg};
use sandbox::actors::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;

async fn setup_test_app() -> (
    axum::Router,
    tempfile::TempDir,
    Arc<AppState>,
    ractor::ActorRef<EventStoreMsg>,
) {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("trace_viewer_test.db");
    let db_path_str = db_path.to_str().expect("db path string");

    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path_str.to_string()),
    )
    .await
    .expect("spawn event store");

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let api_state = api::ApiState {
        app_state: app_state.clone(),
        ws_sessions,
    };
    let app = api::router().with_state(api_state);
    (app, temp_dir, app_state, event_store)
}

async fn append(
    event_store: &ractor::ActorRef<EventStoreMsg>,
    event_type: &str,
    payload: Value,
    actor_id: &str,
) {
    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: actor_id.to_string(),
            user_id: "user-1".to_string(),
        },
        reply
    })
    .expect("append rpc")
    .expect("append result");
}

async fn json_response(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.expect("request");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let value: Value = serde_json::from_slice(&body).expect("json body");
    (status, value)
}

async fn query_events_with_retry(
    app: &axum::Router,
    uri: &str,
    min_events: usize,
    attempts: usize,
) -> Vec<Value> {
    for _ in 0..attempts {
        let (status, body) = json_response(
            app,
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let events = body["events"].as_array().expect("events array").to_vec();
        if events.len() >= min_events {
            return events;
        }
        sleep(Duration::from_millis(40)).await;
    }

    let (status, body) = json_response(
        app,
        Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body["events"].as_array().expect("events array").to_vec()
}

#[tokio::test]
async fn test_delegation_events_are_queryable() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-delegation-1";

    append(
        &event_store,
        "conductor.worker.call",
        json!({
            "run_id": run_id,
            "worker_type": "terminal",
            "worker_objective": "List source files"
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.capability.completed",
        json!({
            "run_id": run_id,
            "capability": "terminal",
            "_meta": { "lane": "control" },
            "data": {
                "call_id": "call-1",
                "summary": "listed files"
            },
            "success": true
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.capability.failed",
        json!({
            "run_id": run_id,
            "capability": "researcher",
            "_meta": { "lane": "control" },
            "data": {
                "call_id": "call-2",
                "error": "timeout",
                "failure_kind": "timeout"
            },
            "success": false
        }),
        "conductor:default",
    )
    .await;

    let (status_workers, body_workers) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=conductor.worker&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status_workers, StatusCode::OK);
    let worker_events = body_workers["events"].as_array().expect("events array");
    assert_eq!(worker_events.len(), 1);
    assert_eq!(worker_events[0]["payload"]["run_id"], run_id);
    assert_eq!(worker_events[0]["payload"]["worker_type"], "terminal");

    let (status_caps, body_caps) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=conductor.capability&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status_caps, StatusCode::OK);
    let capability_events = body_caps["events"].as_array().expect("events array");
    assert_eq!(capability_events.len(), 2);
    let completed = capability_events
        .iter()
        .find(|event| event["event_type"] == "conductor.capability.completed")
        .expect("completed event");
    assert_eq!(completed["payload"]["data"]["call_id"], "call-1");
    assert_eq!(completed["payload"]["_meta"]["lane"], "control");
}

#[tokio::test]
async fn test_run_status_derivable_from_terminal_events() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id_a = "run-terminal-a";
    let run_id_b = "run-terminal-b";

    append(
        &event_store,
        "conductor.task.started",
        json!({ "run_id": run_id_a, "status": "running" }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.task.completed",
        json!({ "run_id": run_id_a, "status": "completed" }),
        "conductor:default",
    )
    .await;

    append(
        &event_store,
        "conductor.task.started",
        json!({ "run_id": run_id_b, "status": "running" }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.task.failed",
        json!({
            "run_id": run_id_b,
            "status": "failed",
            "error_message": "tool timeout"
        }),
        "conductor:default",
    )
    .await;

    let (_status_a, body_a) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?run_id=run-terminal-a&limit=50")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let events_a = body_a["events"].as_array().expect("events");
    let terminal_a = events_a
        .iter()
        .find(|event| event["event_type"] == "conductor.task.completed")
        .expect("completed terminal");
    assert_eq!(terminal_a["payload"]["status"], "completed");

    let (_status_b, body_b) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?run_id=run-terminal-b&limit=50")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let events_b = body_b["events"].as_array().expect("events");
    let terminal_b = events_b
        .iter()
        .find(|event| event["event_type"] == "conductor.task.failed")
        .expect("failed terminal");
    assert_eq!(terminal_b["payload"]["status"], "failed");
}

#[tokio::test]
async fn test_capability_call_id_correlation() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-call-correlation";

    append(
        &event_store,
        "conductor.worker.call",
        json!({
            "run_id": run_id,
            "worker_type": "terminal",
            "worker_objective": "read file"
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.capability.completed",
        json!({
            "run_id": run_id,
            "capability": "terminal",
            "data": { "call_id": "cap-call-123", "summary": "done" }
        }),
        "conductor:default",
    )
    .await;

    let events = query_events_with_retry(
        &app,
        "/logs/events?event_type_prefix=conductor.capability&run_id=run-call-correlation&limit=20",
        1,
        25,
    )
    .await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["payload"]["data"]["call_id"], "cap-call-123");
}

#[tokio::test]
async fn test_worker_lifecycle_events_round_trip() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-lifecycle-roundtrip";

    append(
        &event_store,
        "worker.task.started",
        json!({
            "run_id": run_id,
            "task_id": "task-1",
            "worker_id": "worker:terminal:1",
            "phase": "analyze",
            "objective": "inspect files"
        }),
        "worker:terminal:1",
    )
    .await;
    append(
        &event_store,
        "worker.task.progress",
        json!({
            "run_id": run_id,
            "task_id": "task-1",
            "worker_id": "worker:terminal:1",
            "phase": "analyze",
            "message": "reading Cargo.toml"
        }),
        "worker:terminal:1",
    )
    .await;
    append(
        &event_store,
        "worker.task.progress",
        json!({
            "run_id": run_id,
            "task_id": "task-1",
            "worker_id": "worker:terminal:1",
            "phase": "analyze",
            "message": "reading README"
        }),
        "worker:terminal:1",
    )
    .await;
    append(
        &event_store,
        "worker.task.completed",
        json!({
            "run_id": run_id,
            "task_id": "task-1",
            "worker_id": "worker:terminal:1",
            "phase": "analyze",
            "summary": "2 files analyzed"
        }),
        "worker:terminal:1",
    )
    .await;
    append(
        &event_store,
        "worker.task.failed",
        json!({
            "run_id": run_id,
            "task_id": "task-2",
            "worker_id": "worker:terminal:2",
            "phase": "execute",
            "error": "permission denied"
        }),
        "worker:terminal:2",
    )
    .await;

    let events = query_events_with_retry(
        &app,
        "/logs/events?event_type_prefix=worker.task&run_id=run-lifecycle-roundtrip&limit=50",
        4,
        25,
    )
    .await;
    assert!(
        events.len() >= 4,
        "expected at least 4 worker.task events, got {}",
        events.len()
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "worker.task.progress"),
        "expected at least one worker.task.progress event"
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "worker.task.completed"),
        "expected a worker.task.completed event"
    );
    let started = events
        .iter()
        .find(|event| event["event_type"] == "worker.task.started")
        .expect("started event");
    assert_eq!(started["payload"]["task_id"], "task-1");
    assert_eq!(started["payload"]["worker_id"], "worker:terminal:1");
    assert_eq!(started["payload"]["objective"], "inspect files");

    let failed = events
        .iter()
        .find(|event| event["event_type"] == "worker.task.failed")
        .expect("failed event");
    assert_eq!(failed["payload"]["task_id"], "task-2");
    assert_eq!(failed["payload"]["error"], "permission denied");
}

#[tokio::test]
async fn test_worker_finding_and_learning_events() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-finding-learning";

    append(
        &event_store,
        "worker.task.finding",
        json!({
            "run_id": run_id,
            "task_id": "task-f1",
            "worker_id": "worker:researcher:1",
            "finding_id": "finding-1",
            "claim": "A race condition is possible",
            "confidence": 0.82,
            "evidence_refs": ["file.rs:10"]
        }),
        "worker:researcher:1",
    )
    .await;
    append(
        &event_store,
        "worker.task.learning",
        json!({
            "run_id": run_id,
            "task_id": "task-f1",
            "worker_id": "worker:researcher:1",
            "learning_id": "learning-1",
            "insight": "Need stronger idempotency guard",
            "confidence": 0.77
        }),
        "worker:researcher:1",
    )
    .await;

    let (_status, body) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=worker.task&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let events = body["events"].as_array().expect("events");
    let finding = events
        .iter()
        .find(|event| event["event_type"] == "worker.task.finding")
        .expect("finding event");
    assert_eq!(finding["payload"]["finding_id"], "finding-1");
    assert_eq!(finding["payload"]["claim"], "A race condition is possible");
    assert_eq!(finding["payload"]["confidence"], 0.82);

    let learning = events
        .iter()
        .find(|event| event["event_type"] == "worker.task.learning")
        .expect("learning event");
    assert_eq!(learning["payload"]["learning_id"], "learning-1");
    assert_eq!(
        learning["payload"]["insight"],
        "Need stronger idempotency guard"
    );
}

#[tokio::test]
async fn test_run_graph_includes_worker_counts() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-timeline-workers";

    append(
        &event_store,
        "conductor.run.started",
        json!({ "run_id": run_id, "objective": "timeline test" }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.worker.call",
        json!({
            "run_id": run_id,
            "worker_type": "terminal",
            "worker_objective": "list files"
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.worker.call",
        json!({
            "run_id": run_id,
            "worker_type": "researcher",
            "worker_objective": "analyze docs"
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "conductor.task.completed",
        json!({ "run_id": run_id, "status": "completed" }),
        "conductor:default",
    )
    .await;

    let (status, body) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/conductor/runs/run-timeline-workers/timeline")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["run_id"], run_id);
    assert!(body["events"].as_array().expect("events").len() >= 3);
    assert!(
        body["summary"]["event_counts_by_category"]["agent_objectives"]
            .as_i64()
            .unwrap_or_default()
            >= 2
    );

    let objective_capabilities: Vec<String> = body["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|event| event["category"] == "agent_objectives")
        .filter_map(|event| {
            event["data"]["capability"]
                .as_str()
                .map(ToString::to_string)
        })
        .collect();
    assert!(objective_capabilities
        .iter()
        .any(|value| value == "terminal"));
    assert!(objective_capabilities
        .iter()
        .any(|value| value == "researcher"));
}

#[tokio::test]
async fn test_duration_ms_present_on_completed_events() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;
    let run_id = "run-duration-roundtrip";

    append(
        &event_store,
        "llm.call.completed",
        json!({
            "run_id": run_id,
            "trace_id": "trace-d1",
            "duration_ms": 1234,
            "role": "conductor",
            "function_name": "respond",
            "model_used": "test-model"
        }),
        "conductor:default",
    )
    .await;
    append(
        &event_store,
        "worker.tool.result",
        json!({
            "run_id": run_id,
            "tool_trace_id": "tool-d1",
            "tool_name": "file_read",
            "duration_ms": 567,
            "success": true
        }),
        "terminal:default",
    )
    .await;

    let (_status_llm, llm_body) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=llm.call&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let llm_event = llm_body["events"]
        .as_array()
        .expect("events")
        .iter()
        .find(|event| event["event_type"] == "llm.call.completed")
        .expect("llm completed");
    assert_eq!(llm_event["payload"]["duration_ms"], 1234);

    let (_status_tool, tool_body) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=worker.tool&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let tool_event = tool_body["events"]
        .as_array()
        .expect("events")
        .iter()
        .find(|event| event["event_type"] == "worker.tool.result")
        .expect("tool result");
    assert_eq!(tool_event["payload"]["duration_ms"], 567);
}

#[tokio::test]
async fn test_token_counts_present_on_llm_completed() {
    let (app, _tmp, _state, event_store) = setup_test_app().await;

    append(
        &event_store,
        "llm.call.completed",
        json!({
            "run_id": "run-token-roundtrip",
            "trace_id": "trace-tok-1",
            "role": "conductor",
            "function_name": "respond",
            "model_used": "test-model",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cached_input_tokens": 25,
                "total_tokens": 150
            }
        }),
        "conductor:default",
    )
    .await;

    let (status, body) = json_response(
        &app,
        Request::builder()
            .method("GET")
            .uri("/logs/events?event_type_prefix=llm.call.completed&limit=20")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    let usage = &events[0]["payload"]["usage"];
    assert_eq!(usage["input_tokens"], 100);
    assert_eq!(usage["output_tokens"], 50);
    assert_eq!(usage["cached_input_tokens"], 25);
    assert_eq!(usage["total_tokens"], 150);
}
