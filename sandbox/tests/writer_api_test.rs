//! Writer API Integration Tests
//!
//! Tests full HTTP request/response cycles for Writer API endpoints

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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

static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_test_path(prefix: &str, ext: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let count = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{nanos}_{count}.{ext}")
}

fn revision_sidecar_path(doc_path: &str) -> PathBuf {
    let safe_name = doc_path.replace('/', "__");
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".writer_revisions")
        .join(format!("{safe_name}.rev"))
}

async fn cleanup_writer_artifacts(app: &axum::Router, path: &str) {
    let delete_req = json!({"path": path});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(app, req).await;
    let _ = tokio::fs::remove_file(revision_sidecar_path(path)).await;
}

// ============================================================================
// Open Document Tests
// ============================================================================

#[tokio::test]
async fn test_open_existing_file() {
    let (app, _temp_dir) = setup_test_app().await;
    let path = unique_test_path("writer_test_file", "md");

    // Create a test file first
    let create_req = json!({
        "path": &path,
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
        "path": &path
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"].as_str(), Some(path.as_str()));
    assert_eq!(body["content"], "# Test Document\n\nThis is a test.");
    assert_eq!(body["mime"], "text/markdown");
    assert!(body["revision"].as_u64().is_some());
    assert_eq!(body["revision"], 1); // Initial revision
    assert_eq!(body["readonly"], false);

    // Cleanup
    cleanup_writer_artifacts(&app, &path).await;
}

#[tokio::test]
async fn test_open_not_found() {
    let (app, _temp_dir) = setup_test_app().await;
    let path = unique_test_path("nonexistent_writer_file", "md");

    let open_req = json!({
        "path": &path
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
    let path = unique_test_path("writer_save_test", "md");

    // Create a test file first
    let create_req = json!({
        "path": &path,
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
        "path": &path
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
        "path": &path,
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
    assert_eq!(body["path"].as_str(), Some(path.as_str()));
    assert_eq!(body["saved"], true);
    assert_eq!(body["revision"], 2); // Revision incremented

    // Verify content was saved
    let req = Request::builder()
        .method("GET")
        .uri(format!("/files/content?path={path}"))
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["content"], "Updated content");

    // Cleanup
    cleanup_writer_artifacts(&app, &path).await;
}

#[tokio::test]
async fn test_save_conflict() {
    let (app, _temp_dir) = setup_test_app().await;
    let path = unique_test_path("writer_conflict_test", "md");

    // Create a test file first
    let create_req = json!({
        "path": &path,
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
        "path": &path,
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
        "path": &path,
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
    cleanup_writer_artifacts(&app, &path).await;
}

#[tokio::test]
async fn test_save_new_file_with_base_rev_zero_succeeds() {
    let (app, _temp_dir) = setup_test_app().await;
    let path = unique_test_path("writer_save_as_new_file", "md");

    let save_req = json!({
        "path": &path,
        "base_rev": 0,
        "content": "Created via save as"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/writer/save")
        .header("content-type", "application/json")
        .body(Body::from(save_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["saved"], true);
    assert_eq!(body["revision"], 1);

    // Verify the file can be opened at revision 1
    let open_req = json!({
        "path": &path
    });
    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], 1);
    assert_eq!(body["content"], "Created via save as");

    // Cleanup
    cleanup_writer_artifacts(&app, &path).await;
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
    let path = unique_test_path("preview_test", "md");

    // Create a markdown file
    let create_req = json!({
        "path": &path,
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
        "path": &path
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
    cleanup_writer_artifacts(&app, &path).await;
}

#[tokio::test]
async fn test_preview_path_not_found() {
    let (app, _temp_dir) = setup_test_app().await;
    let path = unique_test_path("nonexistent_preview", "md");

    let preview_req = json!({
        "path": &path
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
    let path = unique_test_path("revision_test", "md");

    // Create a test file
    let create_req = json!({
        "path": &path,
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
        "path": &path
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
            "path": &path,
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
        assert_eq!(
            status,
            StatusCode::OK,
            "Save failed at revision {}",
            expected_revision - 1
        );
        assert_eq!(body["revision"], expected_revision);
    }

    // Cleanup
    cleanup_writer_artifacts(&app, &path).await;
}

#[tokio::test]
async fn test_mime_type_detection() {
    let (app, _temp_dir) = setup_test_app().await;
    let markdown_path = unique_test_path("test", "md");
    let text_path = unique_test_path("test", "txt");
    let rust_path = unique_test_path("test", "rs");

    // Test markdown
    let create_req = json!({
        "path": &markdown_path,
        "content": "# Markdown"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": &markdown_path});
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
        "path": &text_path,
        "content": "Plain text"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": &text_path});
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
        "path": &rust_path,
        "content": "fn main() {}"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({"path": &rust_path});
    let req = Request::builder()
        .method("POST")
        .uri("/writer/open")
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["mime"], "text/rust");

    // Cleanup
    for file in [&markdown_path, &text_path, &rust_path] {
        cleanup_writer_artifacts(&app, file).await;
    }
}
