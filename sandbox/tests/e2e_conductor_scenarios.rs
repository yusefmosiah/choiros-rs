//! E2E Conductor Scenario Tests - Observation-Focused
//!
//! These tests observe (not assert) Conductor behavior to understand how it
//! delegates to different workers (TerminalActor, ResearcherActor) and
//! synthesizes responses.
//!
//! IMPORTANT: These tests are OBSERVATIONAL ONLY. They use tracing::info!() to
//! document behavior but do NOT fail if behavior is unexpected. The goal is
//! to learn how Conductor actually works in practice.

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
use tracing;

use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;
use sandbox::runtime_env::ensure_tls_cert_env;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
static LIVE_E2E_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn live_e2e_guard() -> tokio::sync::MutexGuard<'static, ()> {
    LIVE_E2E_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Setup test app - Conductor and workers are spawned via ensure_conductor()
async fn setup_test_app_with_all_actors() -> (axum::Router, tempfile::TempDir) {
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

async fn submit_conductor_run(app: &axum::Router, objective: &str) -> String {
    let execute_req = json!({
        "objective": objective,
        "desktop_id": "test-desktop-e2e",
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

    tracing::info!(
        "[OBSERVATION] Submit run response: status={}, body={}",
        status,
        body
    );

    if status != StatusCode::ACCEPTED {
        tracing::warn!("[OBSERVATION] Expected ACCEPTED but got {:?}", status);
    }

    body["run_id"]
        .as_str()
        .expect("run_id should be present")
        .to_string()
}

async fn get_run_status(app: &axum::Router, run_id: &str) -> Value {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/conductor/runs/{run_id}"))
        .body(Body::empty())
        .expect("build run status request");

    let (status, body) = json_response(app, req).await;
    tracing::info!(
        "[OBSERVATION] Run status for {}: status={:?}, body={}",
        run_id,
        status,
        body
    );
    body
}

async fn get_events_by_run_id(app: &axum::Router, run_id: &str) -> Vec<Value> {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/logs/events?run_id={run_id}&limit=1000"))
        .body(Body::empty())
        .expect("build logs request");

    let (status, body) = json_response(app, req).await;
    if status != StatusCode::OK {
        tracing::warn!(
            "[OBSERVATION] Logs API error: status={:?}, body={}",
            status,
            body
        );
        return vec![];
    }

    body["events"]
        .as_array()
        .expect("events should be an array")
        .clone()
}

async fn wait_for_run_terminal_state(app: &axum::Router, run_id: &str, timeout_secs: u64) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let status = get_run_status(app, run_id).await;
        let status_str = status["status"].as_str().unwrap_or("unknown");

        if status_str == "completed" || status_str == "failed" || status_str == "blocked" {
            tracing::info!(
                "[OBSERVATION] Run {} reached terminal state: {}",
                run_id,
                status_str
            );
            return status;
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(
                "[OBSERVATION] Timeout waiting for terminal state. Last status: {}",
                status_str
            );
            return status;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn extract_worker_calls(events: &[Value]) -> Vec<(String, String, String)> {
    // Extract (event_type, capability, objective) tuples from events
    events
        .iter()
        .filter_map(|e| {
            let event_type = e["event_type"].as_str()?;
            let payload = &e["payload"];
            let capability = payload["capability"].as_str()?;
            let objective = payload["objective"].as_str()?;
            Some((
                event_type.to_string(),
                capability.to_string(),
                objective.to_string(),
            ))
        })
        .collect()
}

fn log_events_summary(events: &[Value]) {
    tracing::info!(
        "[OBSERVATION] ===== EVENT SUMMARY ({} events) =====",
        events.len()
    );

    for (i, event) in events.iter().enumerate() {
        let event_type = event["event_type"].as_str().unwrap_or("unknown");
        let payload = &event["payload"];

        // Log key fields based on event type
        match event_type {
            "conductor.task.started" => {
                tracing::info!(
                    "[OBSERVATION] Event {}: {} - objective='{}'",
                    i,
                    event_type,
                    payload["objective"].as_str().unwrap_or("N/A")
                );
            }
            "conductor.worker.call" => {
                tracing::info!(
                    "[OBSERVATION] Event {}: {} - capability='{}', objective='{}'",
                    i,
                    event_type,
                    payload["capability"].as_str().unwrap_or("N/A"),
                    payload["objective"].as_str().unwrap_or("N/A")
                );
            }
            "conductor.run.started" => {
                tracing::info!(
                    "[OBSERVATION] Event {}: {} - run_id='{}'",
                    i,
                    event_type,
                    payload["run_id"].as_str().unwrap_or("N/A")
                );
            }
            "conductor.task.completed" | "conductor.task.failed" => {
                tracing::info!(
                    "[OBSERVATION] Event {}: {} - status='{}'",
                    i,
                    event_type,
                    payload["status"].as_str().unwrap_or("N/A")
                );
            }
            _ => {
                tracing::info!("[OBSERVATION] Event {}: {}", i, event_type);
            }
        }
    }

    tracing::info!("[OBSERVATION] ===== END EVENT SUMMARY =====");
}

// ============================================================================
// Test 1: Conductor to Terminal Delegation
// ============================================================================

#[tokio::test]
async fn test_conductor_to_terminal_delegation() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor -> Terminal Delegation");
    tracing::info!("[OBSERVATION] Objective: 'list files in sandbox'");
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id = submit_conductor_run(&app, "list files in sandbox").await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Wait for run to complete or timeout
    let final_status = wait_for_run_terminal_state(&app, &run_id, 120).await;

    // Get all events
    let events = get_events_by_run_id(&app, &run_id).await;
    log_events_summary(&events);

    // Observe what natural language objective was passed to TerminalActor
    let worker_calls = extract_worker_calls(&events);
    tracing::info!("[OBSERVATION] Worker calls observed: {:?}", worker_calls);

    for (_event_type, capability, objective) in &worker_calls {
        if capability == "terminal" {
            tracing::info!(
                "[OBSERVATION] TerminalActor received objective: '{}'",
                objective
            );
            tracing::info!("[OBSERVATION] Original objective was: 'list files in sandbox'");

            // Note: We only OBSERVE, we don't assert
            if objective.contains("list") || objective.contains("files") {
                tracing::info!("[OBSERVATION] Objective appears to be related to original request");
            } else {
                tracing::warn!("[OBSERVATION] Objective may have been transformed significantly");
            }
        }
    }

    tracing::info!(
        "[OBSERVATION] Final run status: {:?}",
        final_status["status"]
    );
    tracing::info!(
        "[OBSERVATION] Run error (if any): {:?}",
        final_status["error"]
    );

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents how Conductor delegates to TerminalActor");
    tracing::info!("[OBSERVATION] for file system operations. Check logs above to see:");
    tracing::info!("[OBSERVATION] 1. What objective was passed to TerminalActor");
    tracing::info!("[OBSERVATION] 2. How Conductor decided to use Terminal vs Researcher");
    tracing::info!("[OBSERVATION] 3. What events were emitted during the flow");
}

// ============================================================================
// Test 2: Conductor to Researcher Delegation
// ============================================================================

#[tokio::test]
async fn test_conductor_to_researcher_delegation() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor -> Researcher Delegation");
    tracing::info!("[OBSERVATION] Objective: 'get weather information'");
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id = submit_conductor_run(&app, "get weather information").await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Wait for run to complete or timeout
    let final_status = wait_for_run_terminal_state(&app, &run_id, 120).await;

    // Get all events
    let events = get_events_by_run_id(&app, &run_id).await;
    log_events_summary(&events);

    // Observe what natural language directive was passed to ResearcherActor
    let worker_calls = extract_worker_calls(&events);
    tracing::info!("[OBSERVATION] Worker calls observed: {:?}", worker_calls);

    for (_event_type, capability, objective) in &worker_calls {
        if capability == "researcher" {
            tracing::info!(
                "[OBSERVATION] ResearcherActor received objective: '{}'",
                objective
            );
            tracing::info!("[OBSERVATION] Original objective was: 'get weather information'");

            // Note: We only OBSERVE, we don't assert
            if objective.contains("weather") {
                tracing::info!("[OBSERVATION] Objective appears to be related to original request");
            } else {
                tracing::warn!("[OBSERVATION] Objective may have been transformed significantly");
            }
        }
    }

    tracing::info!(
        "[OBSERVATION] Final run status: {:?}",
        final_status["status"]
    );
    tracing::info!(
        "[OBSERVATION] Run error (if any): {:?}",
        final_status["error"]
    );

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents how Conductor delegates to ResearcherActor");
    tracing::info!("[OBSERVATION] for web search operations. Check logs above to see:");
    tracing::info!("[OBSERVATION] 1. What objective was passed to ResearcherActor");
    tracing::info!("[OBSERVATION] 2. How Conductor decided to use Researcher vs Terminal");
    tracing::info!("[OBSERVATION] 3. What events were emitted during the flow");
}

// ============================================================================
// Test 3: Conductor Multi-Agent Delegation
// ============================================================================

#[tokio::test]
async fn test_conductor_multi_agent_delegation() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor Multi-Agent Delegation");
    tracing::info!(
        "[OBSERVATION] Objective: 'research superbowl weather then save results to file'"
    );
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id =
        submit_conductor_run(&app, "research superbowl weather then save results to file").await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Wait for run to complete or timeout
    let final_status = wait_for_run_terminal_state(&app, &run_id, 180).await;

    // Get all events
    let events = get_events_by_run_id(&app, &run_id).await;
    log_events_summary(&events);

    // Observe which workers were dispatched
    let worker_calls = extract_worker_calls(&events);
    tracing::info!("[OBSERVATION] Worker calls observed: {:?}", worker_calls);

    let mut saw_researcher = false;
    let mut saw_terminal = false;

    for (_event_type, capability, objective) in &worker_calls {
        match capability.as_str() {
            "researcher" => {
                saw_researcher = true;
                tracing::info!("[OBSERVATION] ResearcherActor received: '{}'", objective);
            }
            "terminal" => {
                saw_terminal = true;
                tracing::info!("[OBSERVATION] TerminalActor received: '{}'", objective);
            }
            _ => {
                tracing::info!(
                    "[OBSERVATION] Unknown capability '{}': '{}'",
                    capability,
                    objective
                );
            }
        }
    }

    // Document observations without assertions
    if saw_researcher && saw_terminal {
        tracing::info!("[OBSERVATION] Both Researcher and Terminal were dispatched (multi-agent)");
    } else if saw_researcher {
        tracing::info!("[OBSERVATION] Only Researcher was dispatched");
    } else if saw_terminal {
        tracing::info!("[OBSERVATION] Only Terminal was dispatched");
    } else {
        tracing::warn!(
            "[OBSERVATION] No worker calls observed - check if test timed out or failed"
        );
    }

    tracing::info!(
        "[OBSERVATION] Final run status: {:?}",
        final_status["status"]
    );
    tracing::info!(
        "[OBSERVATION] Run error (if any): {:?}",
        final_status["error"]
    );

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents how Conductor handles multi-step objectives");
    tracing::info!("[OBSERVATION] that may require both research and file operations. Check logs:");
    tracing::info!("[OBSERVATION] 1. Which capabilities were dispatched");
    tracing::info!("[OBSERVATION] 2. Order of dispatch (if sequential)");
    tracing::info!("[OBSERVATION] 3. How objectives were refined for each capability");
}

// ============================================================================
// Test 4: Conductor Synthesis
// ============================================================================

#[tokio::test]
async fn test_conductor_synthesis() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor Synthesis");
    tracing::info!(
        "[OBSERVATION] Objective: 'check current Rust version and summarize best practices'"
    );
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id = submit_conductor_run(
        &app,
        "check current Rust version and summarize best practices",
    )
    .await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Wait for run to complete or timeout
    let final_status = wait_for_run_terminal_state(&app, &run_id, 180).await;

    // Get all events
    let events = get_events_by_run_id(&app, &run_id).await;
    log_events_summary(&events);

    // Look for synthesis-related events
    let synthesis_events: Vec<&Value> = events
        .iter()
        .filter(|e| {
            let event_type = e["event_type"].as_str().unwrap_or("");
            event_type.contains("synthes")
                || event_type.contains("complete")
                || event_type.contains("final")
        })
        .collect();

    tracing::info!(
        "[OBSERVATION] Synthesis-related events: {}",
        synthesis_events.len()
    );
    for event in &synthesis_events {
        tracing::info!(
            "[OBSERVATION] Synthesis event: {}",
            event["event_type"].as_str().unwrap_or("unknown")
        );
    }

    // Check final run state for synthesis indicators
    if let Some(report_path) = final_status["report_path"].as_str() {
        tracing::info!("[OBSERVATION] Report was written to: {}", report_path);
    } else {
        tracing::info!("[OBSERVATION] No report path in final status");
    }

    if let Some(toast) = final_status["toast"].as_str() {
        tracing::info!("[OBSERVATION] Toast message: {}", toast);
    }

    tracing::info!(
        "[OBSERVATION] Final run status: {:?}",
        final_status["status"]
    );
    tracing::info!(
        "[OBSERVATION] Run error (if any): {:?}",
        final_status["error"]
    );

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents how Conductor synthesizes results from");
    tracing::info!("[OBSERVATION] multiple worker outputs into a coherent final response. Check:");
    tracing::info!("[OBSERVATION] 1. How many workers were involved");
    tracing::info!("[OBSERVATION] 2. What synthesis events were emitted");
    tracing::info!("[OBSERVATION] 3. Whether a report was generated");
    tracing::info!("[OBSERVATION] 4. Final run completion status");
}

// ============================================================================
// Test 5: Conductor Bootstrap Agenda Observation
// ============================================================================

#[tokio::test]
async fn test_conductor_bootstrap_agenda_observation() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor Bootstrap Agenda");
    tracing::info!("[OBSERVATION] Objective: 'create a todo list app in Rust with tests'");
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id = submit_conductor_run(&app, "create a todo list app in Rust with tests").await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Poll for events to see agenda creation
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut last_event_count = 0;

    loop {
        let events = get_events_by_run_id(&app, &run_id).await;

        if events.len() > last_event_count {
            tracing::info!(
                "[OBSERVATION] New events detected (total: {})",
                events.len()
            );

            // Log only new events
            for i in last_event_count..events.len() {
                let event = &events[i];
                let event_type = event["event_type"].as_str().unwrap_or("unknown");
                let payload = &event["payload"];

                if event_type.contains("bootstrap") || event_type.contains("agenda") {
                    tracing::info!(
                        "[OBSERVATION] Bootstrap/Agenda event: {} - {:?}",
                        event_type,
                        payload
                    );
                }
            }

            last_event_count = events.len();
        }

        // Check if we've reached a terminal state
        let status = get_run_status(&app, &run_id).await;
        let status_str = status["status"].as_str().unwrap_or("unknown");
        if status_str == "completed" || status_str == "failed" || status_str == "blocked" {
            tracing::info!("[OBSERVATION] Reached terminal state: {}", status_str);
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("[OBSERVATION] Timeout waiting for bootstrap");
            break;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Get final state
    let final_status = get_run_status(&app, &run_id).await;
    let final_events = get_events_by_run_id(&app, &run_id).await;

    tracing::info!("[OBSERVATION] Total events: {}", final_events.len());
    tracing::info!("[OBSERVATION] Final status: {:?}", final_status["status"]);

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents how Conductor bootstraps an agenda for");
    tracing::info!("[OBSERVATION] complex multi-step tasks. Check logs to see:");
    tracing::info!("[OBSERVATION] 1. What capabilities were selected in bootstrap");
    tracing::info!("[OBSERVATION] 2. How objectives were refined for each capability");
    tracing::info!("[OBSERVATION] 3. Event sequence during agenda creation");
}

// ============================================================================
// Test 6: Conductor Decision Types Observation
// ============================================================================

#[tokio::test]
async fn test_conductor_decision_types_observation() {
    let _guard = live_e2e_guard().await;
    let (app, _temp_dir) = setup_test_app_with_all_actors().await;

    tracing::info!("[OBSERVATION] ==========================================");
    tracing::info!("[OBSERVATION] TEST: Conductor Decision Types");
    tracing::info!(
        "[OBSERVATION] Objective: 'find the latest Rust release notes and extract key features'"
    );
    tracing::info!("[OBSERVATION] ==========================================");

    let run_id = submit_conductor_run(
        &app,
        "find the latest Rust release notes and extract key features",
    )
    .await;

    tracing::info!("[OBSERVATION] Run ID: {}", run_id);

    // Poll and observe decision-related events
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut observed_decisions = Vec::new();

    loop {
        let events = get_events_by_run_id(&app, &run_id).await;

        for event in &events {
            let event_type = event["event_type"].as_str().unwrap_or("");
            if event_type.contains("decision")
                && !observed_decisions.contains(&event_type.to_string())
            {
                observed_decisions.push(event_type.to_string());
                tracing::info!("[OBSERVATION] New decision type observed: {}", event_type);
                tracing::info!("[OBSERVATION] Decision payload: {:?}", event["payload"]);
            }
        }

        // Check if we've reached a terminal state
        let status = get_run_status(&app, &run_id).await;
        let status_str = status["status"].as_str().unwrap_or("unknown");
        if status_str == "completed" || status_str == "failed" || status_str == "blocked" {
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("[OBSERVATION] Timeout");
            break;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    tracing::info!(
        "[OBSERVATION] All observed decision types: {:?}",
        observed_decisions
    );

    // Document the behavior without asserting
    tracing::info!("[OBSERVATION] === BEHAVIOR DOCUMENTED ===");
    tracing::info!("[OBSERVATION] This test documents the types of decisions Conductor makes");
    tracing::info!("[OBSERVATION] during run execution. Decision types may include:");
    tracing::info!("[OBSERVATION] - Dispatch: Send work to a worker");
    tracing::info!("[OBSERVATION] - SpawnFollowup: Create additional agenda items");
    tracing::info!("[OBSERVATION] - Complete: Mark run as done");
    tracing::info!("[OBSERVATION] - Block: Mark run as blocked");
}
