//! Integration tests for Conductor API endpoints
//!
//! Tests full HTTP request/response cycles for Conductor endpoints

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;

async fn setup_test_app() -> (axum::Router, tempfile::TempDir) {
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
    let value: Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(&body).to_string();
        json!({
            "error": {
                "message": text
            }
        })
    });
    (status, value)
}

#[tokio::test]
async fn test_conductor_execute_validation_empty_objective() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "",
        "desktop_id": "test-desktop-003",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_object());
    assert_eq!(body["status"], "failed");
}

#[tokio::test]
async fn test_conductor_execute_validation_whitespace_objective() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "   ",
        "desktop_id": "test-desktop-004",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_object());
}

#[tokio::test]
async fn test_conductor_execute_validation_empty_desktop_id() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Research Rust async patterns",
        "desktop_id": "",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_object());
}

#[tokio::test]
async fn test_conductor_execute_validation_whitespace_desktop_id() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Research Rust async patterns",
        "desktop_id": "   ",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_object());
}

#[tokio::test]
async fn test_conductor_get_run_status_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/conductor/runs/non-existent-run-id")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RUN_NOT_FOUND");
    assert_eq!(body["error"]["message"], "Run not found");
    assert_eq!(body["run_id"], "non-existent-run-id");
}

#[tokio::test]
async fn test_conductor_get_run_status_empty_run_id() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/conductor/runs/")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_conductor_error_response_structure() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "",
        "desktop_id": "test-desktop-007",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert!(
        body["run_id"].is_string(),
        "run_id should be present in error response"
    );
    assert_eq!(body["status"], "failed");
    assert!(body["error"].is_object(), "error should be an object");
    assert!(
        body["error"]["code"].is_string(),
        "error.code should be a string"
    );
    assert!(
        body["error"]["message"].is_string(),
        "error.message should be a string"
    );
}

#[tokio::test]
async fn test_conductor_execute_missing_content_type() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Research Rust async patterns",
        "desktop_id": "test-desktop-008",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn test_conductor_execute_invalid_json() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from("not valid json"))
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_conductor_execute_get_not_allowed() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/conductor/execute")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_conductor_runs_post_not_allowed() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/runs/some-run-id")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_conductor_execute_response_has_no_update_draft_action() {
    use sandbox::baml_client::types::ConductorAction;

    let action_names: Vec<&str> = vec![
        "SpawnWorker",
        "AwaitWorker",
        "MergeCanon",
        "Complete",
        "Block",
    ];

    let actions = vec![
        ConductorAction::SpawnWorker,
        ConductorAction::AwaitWorker,
        ConductorAction::MergeCanon,
        ConductorAction::Complete,
        ConductorAction::Block,
    ];

    for action in &actions {
        let action_str = action.to_string();
        assert!(
            action_names.contains(&action_str.as_str()),
            "ConductorAction::{} should be in known action list",
            action_str
        );
        assert_ne!(
            action_str, "UpdateDraft",
            "ConductorAction::UpdateDraft must NOT exist"
        );
    }

    let parsed = "UpdateDraft".parse::<ConductorAction>();
    assert!(
        parsed.is_err(),
        "Parsing 'UpdateDraft' should fail - it must not exist"
    );
}

#[tokio::test]
async fn test_conductor_execute_rejects_legacy_worker_plan_field() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Test legacy field rejection",
        "desktop_id": "test-desktop-legacy",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Should be rejected",
            "terminal_command": "echo test",
            "timeout_ms": 5000,
            "max_steps": 1
        }]
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("unknown field"));
}

#[tokio::test]
#[ignore = "Mutates process env for deterministic no-worker path"]
async fn test_conductor_execute_no_workers_returns_service_unavailable() {
    let (app, _temp_dir) = setup_test_app().await;
    std::env::set_var("CHOIR_DISABLE_CONDUCTOR_WORKERS", "1");

    let execute_req = json!({
        "objective": "Test unavailable workers",
        "desktop_id": "test-desktop-no-workers",
        "output_mode": "auto"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    std::env::remove_var("CHOIR_DISABLE_CONDUCTOR_WORKERS");

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "ACTOR_NOT_AVAILABLE");
}

#[tokio::test]
#[ignore = "Requires live conductor execution - run with --ignored flag"]
async fn test_conductor_execute_returns_writer_start_fields() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Generate a terminal-backed report for Writer",
        "desktop_id": "test-desktop-writer-fields",
        "output_mode": "markdown_report_to_writer"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::ACCEPTED);

    assert!(
        body["run_id"].is_string(),
        "run_id must be present for Writer integration"
    );

    assert!(
        body["document_path"].is_string() && !body["document_path"].as_str().unwrap().is_empty(),
        "document_path must be present and non-empty for accepted runs"
    );

    assert!(
        body["writer_window_props"].is_object(),
        "writer_window_props must be present for accepted runs"
    );
    assert!(
        body["writer_window_props"]["path"].is_string()
            && !body["writer_window_props"]["path"]
                .as_str()
                .unwrap()
                .is_empty(),
        "writer_window_props.path must be present and non-empty"
    );

    assert!(body["status"].is_string(), "status must be present");
    let status_str = body["status"].as_str().unwrap();
    assert!(
        matches!(
            status_str,
            "initializing" | "running" | "waiting_for_calls" | "completing"
        ),
        "status for accepted run should be a non-terminal state, got {}",
        status_str
    );
}
