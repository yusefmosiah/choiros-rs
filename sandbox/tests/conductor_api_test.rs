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
    let value: Value = serde_json::from_slice(&body).expect("Invalid JSON response");
    (status, value)
}

// ============================================================================
// Conductor Execute Tests
// ============================================================================

#[tokio::test]
async fn test_conductor_execute_endpoint() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Generate a short terminal-backed report",
        "desktop_id": "test-desktop-001",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Print a greeting in terminal",
            "terminal_command": "echo conductor-test",
            "timeout_ms": 5000,
            "max_steps": 1
        }],
        "hints": null,
        "correlation_id": "test-correlation-001"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(body["task_id"].as_str().is_some());
    assert_eq!(body["status"], "waiting_worker");
    assert_eq!(body["correlation_id"], "test-correlation-001");
    assert!(body["report_path"].is_null());
    assert!(body["writer_window_props"].is_null());
    assert!(body["error"].is_null());
}

#[tokio::test]
async fn test_conductor_execute_without_correlation_id() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Generate a short terminal-backed report",
        "desktop_id": "test-desktop-002",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Print a greeting in terminal",
            "terminal_command": "echo conductor-test",
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

    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(body["task_id"].as_str().is_some());
    // Correlation ID should be auto-generated
    assert!(body["correlation_id"].as_str().is_some());
    assert!(!body["correlation_id"].as_str().unwrap().is_empty());
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

// ============================================================================
// Conductor Get Task Status Tests
// ============================================================================

#[tokio::test]
async fn test_conductor_get_task_status_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/conductor/tasks/non-existent-task-id")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "TASK_NOT_FOUND");
    assert_eq!(body["error"]["message"], "Task not found");
    assert_eq!(body["task_id"], "non-existent-task-id");
}

#[tokio::test]
async fn test_conductor_get_task_status_empty_task_id() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/conductor/tasks/")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    // Empty path segment will result in 404 Not Found (route mismatch)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_conductor_get_task_status_with_whitespace_task_id() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a task first
    let execute_req = json!({
        "objective": "Generate a short terminal-backed report",
        "desktop_id": "test-desktop-005",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Print a greeting in terminal",
            "terminal_command": "echo conductor-test",
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

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    // Try to get status with whitespace-only task ID
    let req = Request::builder()
        .method("GET")
        .uri("/conductor/tasks/%20%20%20") // URL-encoded spaces
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    // Whitespace-only task ID returns 400 Bad Request (validation rejects empty task IDs)
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_object());
}

#[tokio::test]
async fn test_conductor_get_task_status_existing_task() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Generate a short terminal-backed report",
        "desktop_id": "test-desktop-009",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Print a greeting in terminal",
            "terminal_command": "echo conductor-test",
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
    assert_eq!(status, StatusCode::ACCEPTED);
    let task_id = body["task_id"].as_str().expect("task_id should be present");

    let req = Request::builder()
        .method("GET")
        .uri(format!("/conductor/tasks/{task_id}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["task_id"], task_id);
    assert!(body["status"].is_string());
}

// ============================================================================
// Conductor Response Structure Tests
// ============================================================================

#[tokio::test]
async fn test_conductor_execute_response_structure() {
    let (app, _temp_dir) = setup_test_app().await;

    let execute_req = json!({
        "objective": "Generate a short terminal-backed report",
        "desktop_id": "test-desktop-006",
        "output_mode": "markdown_report_to_writer",
        "worker_plan": [{
            "worker_type": "terminal",
            "objective": "Print a greeting in terminal",
            "terminal_command": "echo conductor-test",
            "timeout_ms": 5000,
            "max_steps": 1
        }],
        "correlation_id": "test-response-structure"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/execute")
        .header("content-type", "application/json")
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::ACCEPTED);

    // Verify all expected fields are present
    assert!(body["task_id"].is_string(), "task_id should be a string");
    assert!(body["status"].is_string(), "status should be a string");
    assert!(
        body["report_path"].is_null(),
        "report_path should be null for in-flight tasks"
    );
    assert!(
        body["writer_window_props"].is_null(),
        "writer_window_props should be null for in-flight tasks"
    );
    assert!(
        body["correlation_id"].is_string(),
        "correlation_id should be a string"
    );
    assert!(
        body["error"].is_null(),
        "error should be null for successful requests"
    );

    // Verify task_id is a valid ULID format (26 characters)
    let task_id = body["task_id"].as_str().unwrap();
    assert_eq!(
        task_id.len(),
        26,
        "task_id should be a ULID (26 characters)"
    );

    // Verify status is one of the expected values
    let status_str = body["status"].as_str().unwrap();
    let valid_statuses = ["queued", "running", "waiting_worker", "completed", "failed"];
    assert!(
        valid_statuses.contains(&status_str),
        "status should be one of {:?}, got {}",
        valid_statuses,
        status_str
    );
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

    // Verify error response structure
    assert!(
        body["task_id"].is_string(),
        "task_id should be present in error response"
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

// ============================================================================
// Conductor Content Type Tests
// ============================================================================

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
        // No content-type header
        .body(Body::from(execute_req.to_string()))
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    // Axum will reject requests without proper content-type
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
    // Axum rejects invalid JSON with 400 Bad Request
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Conductor Method Not Allowed Tests
// ============================================================================

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
async fn test_conductor_tasks_post_not_allowed() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/conductor/tasks/some-task-id")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
