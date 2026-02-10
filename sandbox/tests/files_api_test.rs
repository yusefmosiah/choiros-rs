//! Files API Integration Tests
//!
//! Tests full HTTP request/response cycles for file system endpoints

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
// List Directory Tests
// ============================================================================

#[tokio::test]
async fn test_list_directory_root() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/list")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    assert!(body["total_count"].as_u64().is_some());
    assert_eq!(body["path"], "");
}

#[tokio::test]
async fn test_list_directory_with_path() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/list?path=src")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    assert_eq!(body["path"], "src");
}

#[tokio::test]
async fn test_list_directory_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/list?path=nonexistent_dir_xyz")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_list_directory_not_a_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    // First create a file
    let create_req = json!({
        "path": "test_file_for_listing.txt",
        "content": "test"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to list the file as a directory
    let req = Request::builder()
        .method("GET")
        .uri("/files/list?path=test_file_for_listing.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "NOT_A_DIRECTORY");

    // Cleanup
    let delete_req = json!({
        "path": "test_file_for_listing.txt"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_list_directory_recursive() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/list?path=src&recursive=true")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    let count = body["total_count"].as_u64().unwrap();
    assert!(count > 0);
}

// ============================================================================
// Get Metadata Tests
// ============================================================================

#[tokio::test]
async fn test_get_metadata_file() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "metadata_test.txt",
        "content": "Hello, World!"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Get metadata
    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=metadata_test.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "metadata_test.txt");
    assert_eq!(body["path"], "metadata_test.txt");
    assert_eq!(body["is_file"], true);
    assert_eq!(body["is_dir"], false);
    assert_eq!(body["size"], 13);
    assert!(body["created_at"].as_str().is_some());
    assert!(body["modified_at"].as_str().is_some());
    assert!(body["permissions"].as_str().is_some());

    // Cleanup
    let delete_req = json!({"path": "metadata_test.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_get_metadata_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=src")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "src");
    assert_eq!(body["path"], "src");
    assert_eq!(body["is_file"], false);
    assert_eq!(body["is_dir"], true);
}

#[tokio::test]
async fn test_get_metadata_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=nonexistent_file_xyz.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

// ============================================================================
// Get Content Tests
// ============================================================================

#[tokio::test]
async fn test_get_content_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "content_test.txt",
        "content": "Hello, World!"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Get content
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=content_test.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "content_test.txt");
    assert_eq!(body["content"], "Hello, World!");
    assert_eq!(body["size"], 13);
    assert_eq!(body["is_truncated"], false);
    assert_eq!(body["encoding"], "utf-8");

    // Cleanup
    let delete_req = json!({"path": "content_test.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_get_content_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=nonexistent_file_xyz.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_get_content_not_a_file() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=src")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "NOT_A_FILE");
}

#[tokio::test]
async fn test_get_content_with_offset_and_limit() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file with more content
    let create_req = json!({
        "path": "content_offset_test.txt",
        "content": "Hello, World! This is a test."
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Get content with offset
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=content_offset_test.txt&offset=7&limit=5")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "World");
    assert_eq!(body["size"], 5);
    assert_eq!(body["is_truncated"], true);

    // Cleanup
    let delete_req = json!({"path": "content_offset_test.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Create File Tests
// ============================================================================

#[tokio::test]
async fn test_create_file_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    let create_req = json!({
        "path": "new_test_file.txt",
        "content": "Test content"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "new_test_file.txt");
    assert_eq!(body["created"], true);
    assert_eq!(body["size"], 12);

    // Cleanup
    let delete_req = json!({"path": "new_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_create_file_empty() {
    let (app, _temp_dir) = setup_test_app().await;

    let create_req = json!({
        "path": "empty_test_file.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "empty_test_file.txt");
    assert_eq!(body["created"], true);
    assert_eq!(body["size"], 0);

    // Cleanup
    let delete_req = json!({"path": "empty_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_create_file_already_exists() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create the file first
    let create_req = json!({
        "path": "duplicate_test_file.txt",
        "content": "original"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to create again without overwrite
    let create_req = json!({
        "path": "duplicate_test_file.txt",
        "content": "duplicate"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "ALREADY_EXISTS");

    // Cleanup
    let delete_req = json!({"path": "duplicate_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_create_file_with_overwrite() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create the file first
    let create_req = json!({
        "path": "overwrite_test_file.txt",
        "content": "original"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Overwrite with new content
    let create_req = json!({
        "path": "overwrite_test_file.txt",
        "content": "overwritten content",
        "overwrite": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "overwrite_test_file.txt");
    assert_eq!(body["created"], true);
    assert_eq!(body["size"], 19);

    // Verify content was overwritten
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=overwrite_test_file.txt")
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["content"], "overwritten content");

    // Cleanup
    let delete_req = json!({"path": "overwrite_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Write File Tests
// ============================================================================

#[tokio::test]
async fn test_write_file_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "write_test_file.txt",
        "content": "initial"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Write new content
    let write_req = json!({
        "path": "write_test_file.txt",
        "content": "new content"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/write")
        .header("content-type", "application/json")
        .body(Body::from(write_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "write_test_file.txt");
    assert_eq!(body["bytes_written"], 11);
    assert_eq!(body["size"], 11);

    // Cleanup
    let delete_req = json!({"path": "write_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_write_file_append() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "append_test_file.txt",
        "content": "first"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Append content
    let write_req = json!({
        "path": "append_test_file.txt",
        "content": " second",
        "append": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/write")
        .header("content-type", "application/json")
        .body(Body::from(write_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bytes_written"], 7);
    assert_eq!(body["size"], 12); // "first second"

    // Verify content
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=append_test_file.txt")
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["content"], "first second");

    // Cleanup
    let delete_req = json!({"path": "append_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_write_file_create_if_missing() {
    let (app, _temp_dir) = setup_test_app().await;

    // Write to non-existent file (should create by default)
    let write_req = json!({
        "path": "auto_create_test_file.txt",
        "content": "auto created"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/write")
        .header("content-type", "application/json")
        .body(Body::from(write_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bytes_written"], 12);

    // Cleanup
    let delete_req = json!({"path": "auto_create_test_file.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_write_file_not_found_no_create() {
    let (app, _temp_dir) = setup_test_app().await;

    let write_req = json!({
        "path": "nonexistent_write_file.txt",
        "content": "test",
        "create_if_missing": false
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/write")
        .header("content-type", "application/json")
        .body(Body::from(write_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_write_file_not_a_file() {
    let (app, _temp_dir) = setup_test_app().await;

    let write_req = json!({
        "path": "src",
        "content": "test"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/write")
        .header("content-type", "application/json")
        .body(Body::from(write_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "NOT_A_FILE");
}

// ============================================================================
// Create Directory Tests
// ============================================================================

#[tokio::test]
async fn test_create_directory_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    let mkdir_req = json!({
        "path": "test_new_directory"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "test_new_directory");
    assert_eq!(body["created"], true);

    // Cleanup
    let delete_req = json!({
        "path": "test_new_directory",
        "recursive": true
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_create_directory_recursive() {
    let (app, _temp_dir) = setup_test_app().await;

    let mkdir_req = json!({
        "path": "test/nested/directory",
        "recursive": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "test/nested/directory");
    assert_eq!(body["created"], true);

    // Cleanup
    let delete_req = json!({
        "path": "test",
        "recursive": true
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_create_directory_already_exists() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create directory first
    let mkdir_req = json!({
        "path": "duplicate_test_directory"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to create again
    let mkdir_req = json!({
        "path": "duplicate_test_directory"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "ALREADY_EXISTS");

    // Cleanup
    let delete_req = json!({
        "path": "duplicate_test_directory",
        "recursive": true
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Rename Tests
// ============================================================================

#[tokio::test]
async fn test_rename_file_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "rename_source.txt",
        "content": "test"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Rename the file
    let rename_req = json!({
        "source": "rename_source.txt",
        "target": "rename_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/rename")
        .header("content-type", "application/json")
        .body(Body::from(rename_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"], "rename_source.txt");
    assert_eq!(body["target"], "rename_target.txt");
    assert_eq!(body["renamed"], true);

    // Verify old path doesn't exist
    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=rename_source.txt")
        .body(Body::empty())
        .unwrap();

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Verify new path exists
    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=rename_target.txt")
        .body(Body::empty())
        .unwrap();

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);

    // Cleanup
    let delete_req = json!({"path": "rename_target.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_rename_source_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let rename_req = json!({
        "source": "nonexistent_source.txt",
        "target": "rename_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/rename")
        .header("content-type", "application/json")
        .body(Body::from(rename_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_rename_target_already_exists() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create source file
    let create_req = json!({
        "path": "rename_conflict_source.txt",
        "content": "source"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Create target file
    let create_req = json!({
        "path": "rename_conflict_target.txt",
        "content": "target"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to rename without overwrite
    let rename_req = json!({
        "source": "rename_conflict_source.txt",
        "target": "rename_conflict_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/rename")
        .header("content-type", "application/json")
        .body(Body::from(rename_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "ALREADY_EXISTS");

    // Cleanup
    let delete_req = json!({"path": "rename_conflict_source.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let delete_req = json!({"path": "rename_conflict_target.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Delete Tests
// ============================================================================

#[tokio::test]
async fn test_delete_file_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "delete_test_file.txt",
        "content": "delete me"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Delete the file
    let delete_req = json!({"path": "delete_test_file.txt"});

    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "delete_test_file.txt");
    assert_eq!(body["deleted"], true);
    assert_eq!(body["type"], "file");

    // Verify file is gone
    let req = Request::builder()
        .method("GET")
        .uri("/files/metadata?path=delete_test_file.txt")
        .body(Body::empty())
        .unwrap();

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_directory_recursive() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a directory with content
    let mkdir_req = json!({
        "path": "delete_test_dir",
        "recursive": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let create_req = json!({
        "path": "delete_test_dir/file.txt",
        "content": "inside"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Delete the directory recursively
    let delete_req = json!({
        "path": "delete_test_dir",
        "recursive": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], "delete_test_dir");
    assert_eq!(body["deleted"], true);
    assert_eq!(body["type"], "directory");
}

#[tokio::test]
async fn test_delete_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let delete_req = json!({"path": "nonexistent_delete_file.txt"});

    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

// ============================================================================
// Copy Tests
// ============================================================================

#[tokio::test]
async fn test_copy_file_happy_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a test file
    let create_req = json!({
        "path": "copy_source.txt",
        "content": "copy this"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Copy the file
    let copy_req = json!({
        "source": "copy_source.txt",
        "target": "copy_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/copy")
        .header("content-type", "application/json")
        .body(Body::from(copy_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"], "copy_source.txt");
    assert_eq!(body["target"], "copy_target.txt");
    assert_eq!(body["copied"], true);
    assert_eq!(body["size"], 9);

    // Verify target exists with same content
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=copy_target.txt")
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert_eq!(body["content"], "copy this");

    // Cleanup
    let delete_req = json!({"path": "copy_source.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let delete_req = json!({"path": "copy_target.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_copy_source_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let copy_req = json!({
        "source": "nonexistent_copy_source.txt",
        "target": "copy_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/copy")
        .header("content-type", "application/json")
        .body(Body::from(copy_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_copy_source_is_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    let copy_req = json!({
        "source": "src",
        "target": "copy_target_dir"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/copy")
        .header("content-type", "application/json")
        .body(Body::from(copy_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "NOT_A_FILE");
}

#[tokio::test]
async fn test_copy_target_already_exists() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create source file
    let create_req = json!({
        "path": "copy_conflict_source.txt",
        "content": "source"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Create target file
    let create_req = json!({
        "path": "copy_conflict_target.txt",
        "content": "target"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to copy without overwrite
    let copy_req = json!({
        "source": "copy_conflict_source.txt",
        "target": "copy_conflict_target.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/copy")
        .header("content-type", "application/json")
        .body(Body::from(copy_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "ALREADY_EXISTS");

    // Cleanup
    let delete_req = json!({"path": "copy_conflict_source.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let delete_req = json!({"path": "copy_conflict_target.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Path Traversal Tests
// ============================================================================

#[tokio::test]
async fn test_path_traversal_absolute_path() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=/etc/passwd")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_path_traversal_parent_directory() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=../Cargo.toml")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_path_traversal_nested_escape() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=src/../../../etc/passwd")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_path_traversal_create_file() {
    let (app, _temp_dir) = setup_test_app().await;

    let create_req = json!({
        "path": "../escape_test.txt",
        "content": "escaped"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_path_traversal_rename_source() {
    let (app, _temp_dir) = setup_test_app().await;

    let rename_req = json!({
        "source": "../Cargo.toml",
        "target": "safe.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/rename")
        .header("content-type", "application/json")
        .body(Body::from(rename_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");
}

#[tokio::test]
async fn test_path_traversal_rename_target() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a safe file first
    let create_req = json!({
        "path": "safe_rename_source.txt",
        "content": "safe"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Try to rename to an escaped path
    let rename_req = json!({
        "source": "safe_rename_source.txt",
        "target": "../escaped.txt"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/rename")
        .header("content-type", "application/json")
        .body(Body::from(rename_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PATH_TRAVERSAL");

    // Cleanup
    let delete_req = json!({"path": "safe_rename_source.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

// ============================================================================
// Sandbox Boundary Tests
// ============================================================================

#[tokio::test]
async fn test_sandbox_boundary_valid_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Valid paths within sandbox should work
    let create_req = json!({
        "path": "sandbox_test.txt",
        "content": "inside sandbox"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);

    // Cleanup
    let delete_req = json!({"path": "sandbox_test.txt"});
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_sandbox_boundary_normalized_path() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create parent directory first (parent must exist for create_file)
    let mkdir_req = json!({
        "path": "normalized",
        "recursive": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Paths with . and // should be normalized and work
    let create_req = json!({
        "path": "./normalized//test.txt",
        "content": "normalized path"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (status, _body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);

    // Verify file exists at normalized path
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=normalized/test.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "normalized path");

    // Cleanup
    let delete_req = json!({
        "path": "normalized",
        "recursive": true
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}

#[tokio::test]
async fn test_sandbox_boundary_valid_parent_traversal() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a nested directory structure
    let mkdir_req = json!({
        "path": "deep/nested/dir",
        "recursive": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/mkdir")
        .header("content-type", "application/json")
        .body(Body::from(mkdir_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Create a file in the nested dir
    let create_req = json!({
        "path": "deep/nested/dir/file.txt",
        "content": "deep file"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/files/create")
        .header("content-type", "application/json")
        .body(Body::from(create_req.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    // Use .. to reference parent (should work if it stays within sandbox)
    // deep/nested/dir/../dir/file.txt should resolve to deep/nested/dir/file.txt
    let req = Request::builder()
        .method("GET")
        .uri("/files/content?path=deep/nested/dir/../dir/file.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "deep file");

    // Cleanup
    let delete_req = json!({
        "path": "deep",
        "recursive": true
    });
    let req = Request::builder()
        .method("POST")
        .uri("/files/delete")
        .header("content-type", "application/json")
        .body(Body::from(delete_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;
}
