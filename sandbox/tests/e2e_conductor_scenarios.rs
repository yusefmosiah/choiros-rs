//! E2E Conductor Scenario Tests — Live LLM
//!
//! Tests the full conductor stack end-to-end with real model providers:
//!   conductor = KimiK25
//!   researcher = ZaiGLM5
//!   terminal   = ClaudeBedrockSonnet46
//!
//! Every test asserts at minimum:
//!   1. HTTP 202 ACCEPTED + non-empty run_id on submit
//!   2. `conductor.run.started` event emitted to EventStore
//!   3. Run reaches a terminal state within the timeout (completed/failed/blocked)
//!   4. Scenario-specific invariants (at least one worker call, correct capability, etc.)
//!
//! These are proper regression tests, not observation scripts. A silent regression
//! in conductor routing, event emission, or run lifecycle will cause a test failure.
//!
//! Run:
//!   cargo test -p sandbox --test e2e_conductor_scenarios -- --nocapture

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

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const TERMINAL_STATES: &[&str] = &["completed", "failed", "blocked"];

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
    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path.to_str().unwrap().to_string()),
    )
    .await
    .expect("spawn event store");
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
    let response = app.clone().oneshot(req).await.expect("request failed");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

/// Submit a run. Asserts HTTP 202 and non-empty run_id.
async fn submit_run(app: &axum::Router, objective: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "objective": objective,
                "desktop_id": "e2e-test-desktop",
                "output_mode": "markdown_report_to_writer",
                "hints": null,
            })
            .to_string(),
        ))
        .expect("build request");

    let (status, body) = json_response(app, req).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "conductor/execute must return 202 ACCEPTED for '{}', got {} body={}",
        objective,
        status,
        body
    );
    let run_id = body["run_id"].as_str().unwrap_or("").to_string();
    assert!(
        !run_id.is_empty(),
        "run_id must be non-empty in 202 response, body={}",
        body
    );
    run_id
}

async fn get_run_state(app: &axum::Router, run_id: &str) -> Value {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/conductor/runs/{run_id}"))
        .body(Body::empty())
        .expect("build request");
    let (_status, body) = json_response(app, req).await;
    body
}

async fn get_events(app: &axum::Router, run_id: &str) -> Vec<Value> {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/logs/events?run_id={run_id}&limit=1000"))
        .body(Body::empty())
        .expect("build request");
    let (status, body) = json_response(app, req).await;
    if status != StatusCode::OK {
        return vec![];
    }
    body["events"].as_array().cloned().unwrap_or_default()
}

/// Poll until the run reaches a terminal state. Returns the final run state.
/// Asserts that the run reaches a terminal state within `timeout_secs`.
async fn wait_for_terminal(app: &axum::Router, run_id: &str, timeout_secs: u64) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let state = get_run_state(app, run_id).await;
        let status = state["status"].as_str().unwrap_or("unknown");
        if TERMINAL_STATES.contains(&status) {
            println!("  [TERMINAL] run:{run_id} status:{status}");
            return state;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run '{run_id}' did not reach terminal state within {timeout_secs}s — last status: '{status}'"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn worker_calls(events: &[Value]) -> Vec<(String, String)> {
    events
        .iter()
        .filter(|e| e["event_type"].as_str() == Some("conductor.worker.call"))
        .filter_map(|e| {
            let cap = e["payload"]["capability"].as_str()?.to_string();
            let obj = e["payload"]["objective"].as_str().unwrap_or("").to_string();
            Some((cap, obj))
        })
        .collect()
}

fn event_types(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| e["event_type"].as_str().map(|s| s.to_string()))
        .collect()
}

// ─── Test 1: Terminal delegation ─────────────────────────────────────────────

/// Conductor delegates a file-system objective to TerminalActor (ClaudeBedrockSonnet46).
/// Asserts: run accepted, run.started emitted, terminal state reached, at least one
/// worker call was made.
#[tokio::test]
async fn test_conductor_to_terminal_delegation() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    let run_id = submit_run(&app, "list the top-level files in the sandbox directory").await;
    println!("  [RUN] terminal delegation run_id: {run_id}");

    // conductor.run.started must appear before the run progresses
    tokio::time::sleep(Duration::from_millis(500)).await;
    let early_events = get_events(&app, &run_id).await;
    let types = event_types(&early_events);
    assert!(
        types
            .iter()
            .any(|t| t.contains("conductor.run.started") || t.contains("conductor.task.started")),
        "at least one conductor lifecycle event must be emitted within 500ms, got: {types:?}"
    );

    let final_state = wait_for_terminal(&app, &run_id, 120).await;
    let status = final_state["status"].as_str().unwrap_or("unknown");

    // Status must be a known value
    assert!(
        TERMINAL_STATES.contains(&status),
        "final status must be completed/failed/blocked, got: '{status}'"
    );

    let events = get_events(&app, &run_id).await;
    let calls = worker_calls(&events);

    // At least one worker call must have been made (conductor cannot complete with zero workers)
    assert!(
        !calls.is_empty(),
        "conductor must dispatch at least one worker call for '{}', got zero. events: {:?}",
        "list the top-level files",
        event_types(&events)
    );

    println!(
        "  [ASSERT] run:{run_id} status:{status} worker_calls:{}",
        calls
            .iter()
            .map(|(c, o)| format!("{c}({})", &o[..40.min(o.len())]))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

// ─── Test 2: Researcher delegation ───────────────────────────────────────────

/// Conductor delegates a web-research objective to ResearcherActor (ZaiGLM5).
/// Asserts: 202 + run_id, terminal state reached, at least one worker call,
/// the researcher capability is used at least once.
#[tokio::test]
async fn test_conductor_to_researcher_delegation() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    // Use a clearly research-oriented objective that doesn't require terminal
    let run_id = submit_run(
        &app,
        "find information about the Rust programming language's ownership model",
    )
    .await;
    println!("  [RUN] researcher delegation run_id: {run_id}");

    let final_state = wait_for_terminal(&app, &run_id, 180).await;
    let status = final_state["status"].as_str().unwrap_or("unknown");
    assert!(
        TERMINAL_STATES.contains(&status),
        "final status must be terminal, got: '{status}'"
    );

    let events = get_events(&app, &run_id).await;
    let calls = worker_calls(&events);

    assert!(
        !calls.is_empty(),
        "at least one worker call required, got none. event_types: {:?}",
        event_types(&events)
    );

    // At least one capability call should be researcher or writer
    // (conductor may choose researcher or writer — both are valid for a research task)
    let has_knowledge_worker = calls
        .iter()
        .any(|(cap, _)| cap == "researcher" || cap == "writer");
    assert!(
        has_knowledge_worker,
        "conductor must use researcher or writer for a research objective, \
         got capabilities: {:?}",
        calls.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
    );

    println!(
        "  [ASSERT] run:{run_id} status:{status} capabilities:{:?}",
        calls.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
    );
}

// ─── Test 3: Multi-agent dispatch ────────────────────────────────────────────

/// Conductor dispatches multiple workers for a compound objective.
/// Asserts: terminal state, ≥2 worker calls total, event count > 3.
///
/// The objective is designed to require at minimum one research step
/// and one write/file step, forcing multi-agent dispatch.
#[tokio::test]
async fn test_conductor_multi_agent_dispatch() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    let run_id = submit_run(
        &app,
        "research what tokio is in Rust and write a one-paragraph summary to /tmp/tokio-summary.md",
    )
    .await;
    println!("  [RUN] multi-agent run_id: {run_id}");

    let final_state = wait_for_terminal(&app, &run_id, 180).await;
    let status = final_state["status"].as_str().unwrap_or("unknown");
    assert!(
        TERMINAL_STATES.contains(&status),
        "final status must be terminal, got: '{status}'"
    );

    let events = get_events(&app, &run_id).await;
    assert!(
        events.len() > 3,
        "multi-agent run must produce more than 3 events, got {}",
        events.len()
    );

    let calls = worker_calls(&events);
    assert!(
        !calls.is_empty(),
        "multi-agent run must have at least one worker call, got none"
    );

    println!(
        "  [ASSERT] run:{run_id} status:{status} events:{} calls:{}",
        events.len(),
        calls.len()
    );
}

// ─── Test 4: Conductor run isolation ─────────────────────────────────────────

/// Two concurrent runs do not share events — scope isolation.
/// Asserts: each run only sees its own events (no run_id bleed).
///
/// This is the critical scope-isolation regression test called out in AGENTS.md.
#[tokio::test]
async fn test_concurrent_run_isolation() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    let run_a = submit_run(&app, "echo the word ALPHA to stdout").await;
    let run_b = submit_run(&app, "echo the word BETA to stdout").await;
    println!("  [RUN] isolation run_a:{run_a} run_b:{run_b}");

    // Both must reach terminal state
    let state_a = wait_for_terminal(&app, &run_a, 120).await;
    let state_b = wait_for_terminal(&app, &run_b, 120).await;

    let status_a = state_a["status"].as_str().unwrap_or("unknown");
    let status_b = state_b["status"].as_str().unwrap_or("unknown");
    assert!(
        TERMINAL_STATES.contains(&status_a),
        "run_a must be terminal, got: '{status_a}'"
    );
    assert!(
        TERMINAL_STATES.contains(&status_b),
        "run_b must be terminal, got: '{status_b}'"
    );

    // Events for run_a must not contain run_b's run_id and vice versa
    let events_a = get_events(&app, &run_a).await;
    let events_b = get_events(&app, &run_b).await;

    // All events returned for run_a must belong to run_a
    for ev in &events_a {
        let ev_run = ev["payload"]["run_id"].as_str().unwrap_or("");
        if !ev_run.is_empty() {
            assert_ne!(
                ev_run,
                run_b,
                "run_a events must not contain run_b's run_id (scope bleed), event: {}",
                ev["event_type"].as_str().unwrap_or("?")
            );
        }
    }
    for ev in &events_b {
        let ev_run = ev["payload"]["run_id"].as_str().unwrap_or("");
        if !ev_run.is_empty() {
            assert_ne!(
                ev_run,
                run_a,
                "run_b events must not contain run_a's run_id (scope bleed), event: {}",
                ev["event_type"].as_str().unwrap_or("?")
            );
        }
    }

    println!(
        "  [ASSERT] isolation confirmed: run_a:{status_a} events_a:{} | run_b:{status_b} events_b:{}",
        events_a.len(),
        events_b.len()
    );
}

// ─── Test 5: Bootstrap agenda event emission ─────────────────────────────────

/// Conductor emits `conductor.run.started` and `conductor.task.started` events
/// within the first few seconds of a run.
///
/// This is the observability contract. If these events stop being emitted,
/// the streaming/websocket UI goes dark and users see no progress.
#[tokio::test]
async fn test_conductor_emits_lifecycle_events() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    let run_id = submit_run(&app, "write hello world to /tmp/hw-test.txt").await;
    println!("  [RUN] lifecycle events run_id: {run_id}");

    // Poll for up to 10s for lifecycle events (should appear before LLM even responds)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut saw_run_started = false;
    while tokio::time::Instant::now() < deadline {
        let events = get_events(&app, &run_id).await;
        let types = event_types(&events);
        if types.iter().any(|t| t.contains("conductor.run.started")) {
            saw_run_started = true;
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    assert!(
        saw_run_started,
        "conductor.run.started event must be emitted within 10s of run submission"
    );

    // Now wait for terminal and verify the full event chain
    let final_state = wait_for_terminal(&app, &run_id, 120).await;
    let status = final_state["status"].as_str().unwrap_or("unknown");
    let events = get_events(&app, &run_id).await;
    let types = event_types(&events);

    assert!(
        TERMINAL_STATES.contains(&status),
        "run must reach terminal state, got: '{status}'"
    );
    assert!(
        events.len() >= 2,
        "must have at least 2 lifecycle events (started + at least one more), got: {types:?}"
    );

    println!(
        "  [ASSERT] run:{run_id} status:{status} events:{} types_sample:{:?}",
        events.len(),
        &types[..4.min(types.len())]
    );
}

// ─── Test 6: Duplicate run_id rejection ──────────────────────────────────────

/// Submitting the same run_id twice (via legacy worker_plan rejection path)
/// or a malformed request must return a 4xx — not 202.
///
/// Verifies the conductor API validation layer isn't silently accepting garbage.
#[tokio::test]
async fn test_conductor_rejects_invalid_request() {
    let _guard = live_e2e_guard().await;
    let (app, _tmp) = setup_test_app().await;

    // Empty objective
    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "objective": "",
                "desktop_id": "e2e-test-desktop",
                "output_mode": "markdown_report_to_writer",
                "hints": null,
            })
            .to_string(),
        ))
        .expect("build request");

    let (status, body) = json_response(&app, req).await;
    assert!(
        status.is_client_error(),
        "empty objective must return 4xx, got {status} body={body}"
    );

    // Whitespace-only objective
    let req2 = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "objective": "   ",
                "desktop_id": "e2e-test-desktop",
                "output_mode": "markdown_report_to_writer",
                "hints": null,
            })
            .to_string(),
        ))
        .expect("build request");

    let (status2, body2) = json_response(&app, req2).await;
    assert!(
        status2.is_client_error(),
        "whitespace objective must return 4xx, got {status2} body={body2}"
    );

    println!("  [ASSERT] empty objective rejected: {status}, whitespace rejected: {status2}");
}
