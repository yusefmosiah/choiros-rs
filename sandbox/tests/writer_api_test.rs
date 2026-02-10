//! Writer API Integration Tests
//!
//! Tests full HTTP request/response cycles for Writer API endpoints

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
// Open Document Tests
// ============================================================================

#[tokio::test]
async fn test_open_existing_file() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file first
    let create_req = json!({
        "path": "writer_test_file.md",
        "content": "# Test Document\n\nThis is a test."
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Open the document via Writer API
    let open_req = json!({
        "path": "writer_test_file.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "writer_test_file.md");
    assert_eq!(body["content"], "# Test Document\n\nThis is a test.");
    assert_eq!(body["mime"], "text/markdown");
    assert!(body["revision"].as_u64().is_some());
    assert_eq!(body["revision"], 1); // Initial revision
    assert_eq!(body["readonly"], false);

    // Cleanup
    let delete_req = json!({"path": "writer_test_file.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_open_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let open_req = json!({
        "path": "nonexistent_writer_file.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_open_is_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    // Try to open a directory as a document
    let open_req = json!({
        "path": "src"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "IS_DIRECTORY");
}

#[tokio::test]
async fn test_open_path_traversal() {
    let (app, _temp_dir) = setup_test_app().await;

    let open_req = json!({
        "path": "../Cargo.toml"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

// ============================================================================
// Save Document Tests
// ============================================================================

#[tokio::test]
async fn test_save_success() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file first
    let create_req = json!({
        "path": "writer_save_test.md",
        "content": "Initial content"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Open to get revision
    let open_req = json!({
        "path": "writer_save_test.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    let revision = body["revision"].as_u64().unwrap();
    assert_eq!(revision, 1);

    // Save with correct revision
    let save_req = json!({
        "path": "writer_save_test.md",
        "base_rev": revision,
        "content": "Updated content"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "writer_save_test.md");
    assert_eq!(body["saved"], true);
    assert_eq!(body["revision"], 2); // Revision incremented

    // Verify content was saved
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=writer_save_test.md")
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["content"], "Updated content");

    // Cleanup
    let delete_req = json!({"path": "writer_save_test.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_save_conflict() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file first
    let create_req = json!({
        "path": "writer_conflict_test.md",
        "content": "Initial content"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Client A opens and saves
    let save_req = json!({
        "path": "writer_conflict_test.md",
        "base_rev": 1,
        "content": "Client A changes"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], 2);

    // Client B tries to save with stale revision
    let save_req = json!({
        "path": "writer_conflict_test.md",
        "base_rev": 1, // Stale revision
        "content": "Client B changes"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "CONFLICT");
    assert_eq!(body["current_revision"], 2);
    assert_eq!(body["current_content"], "Client A changes");

    // Cleanup
    let delete_req = json!({"path": "writer_conflict_test.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_save_path_traversal() {
    let (app, _temp_dir) = setup_test_app().await;

    let save_req = json!({
        "path": "../escape.txt",
        "base_rev": 1,
        "content": "Escaped!"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_save_is_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    let save_req = json!({
        "path": "src",
        "base_rev": 1,
        "content": "Can't save to directory"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "IS_DIRECTORY");
}

// ============================================================================
// Preview Tests
// ============================================================================

#[tokio::test]
async fn test_preview_content() {
    let (app, _temp_dir) = setup_test_app().await;

    let preview_req = json!({
        "content": "# Hello World\n\nThis is **bold** text."
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/preview")
        .header("content-type", "application/json")
        .body(Body::from(preview_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);

    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<h1>Hello World</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
}

#[tokio::test]
async fn test_preview_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a markdown file
    let create_req = json!({
        "path": "preview_test.md",
        "content": "# Preview Test\n\n- Item 1\n- Item 2"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Preview by path
    let preview_req = json!({
        "path": "preview_test.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/preview")
        .header("content-type", "application/json")
        .body(Body::from(preview_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);

    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<h1>Preview Test</h1>"));
    assert!(html.contains("<ul>"));

    // Cleanup
    let delete_req = json!({"path": "preview_test.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_preview_path_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let preview_req = json!({
        "path": "nonexistent_preview.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/preview")
        .header("content-type", "application/json")
        .body(Body::from(preview_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_preview_path_traversal() {
    let (app, _temp_dir) = setup_test_app().await;

    let preview_req = json!({
        "path": "../Cargo.toml"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/preview")
        .header("content-type", "application/json")
        .body(Body::from(preview_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

// ============================================================================
// Sandbox Boundary Tests
// ============================================================================

#[tokio::test]
async fn test_sandbox_boundary_open() {
    let (app, _temp_dir) = setup_test_app().await;

    // Try to escape sandbox with open
    let open_req = json!({
        "path": "src/../../../etc/passwd"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_sandbox_boundary_save() {
    let (app, _temp_dir) = setup_test_app().await;

    // Try to escape sandbox with save
    let save_req = json!({
        "path": "./././../escape.txt",
        "base_rev": 1,
        "content": "Escaped!"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_revision_increments_on_save() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "revision_test.md",
        "content": "Version 1"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Open to get initial revision
    let open_req = json!({
        "path": "revision_test.md"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["revision"], 1);

    // Save multiple times, checking revision increments
    for expected_revision in 2..=5 {
        let save_req = json!({
            "path": "revision_test.md",
            "base_rev": expected_revision - 1,
            "content": format!("Version {}", expected_revision)
        });

        let req = Request::builder()
            .method("POST")
            .uri("/writer/save")
            .header("content-type", "application/json")
            .body(Body::from(save_req.to_string()))
            .unwrap();

        let (status, body) = json_response(&app, req).await;
        assert_eq!(status, StatusCode::OK, "Save failed at revision {}", expected_revision - 1);
        assert_eq!(body["revision"], expected_revision);
    }

    // Cleanup
    let delete_req = json!({"path": "revision_test.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_mime_type_detection() {
    let (app, _temp_dir) = setup_test_app().await;

    // Test markdown
    let create_req = json!({
        "path": "test.md",
        "content": "# Markdown"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": "test.md"});
    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["mime"], "text/markdown");

    // Test plain text
    let create_req = json!({
        "path": "test.txt",
        "content": "Plain text"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": "test.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["mime"], "text/plain");

    // Test Rust file
    let create_req = json!({
        "path": "test.rs",
        "content": "fn main() {}"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": "test.rs"});
    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["mime"], "text/rust");

    // Cleanup
    for file in ["test.md", "test.txt", "test.rs"] {
        let delete_req = json!({"path": file});
        let req = Request::builder()
            .method("POST")
            .uri("/files/delete")
            .header("content-type", "application/json")
            .body(Body::from(delete_req.to_string()))
            .unwrap();
        let _ = json_response(&app, req).await;
    }
}
