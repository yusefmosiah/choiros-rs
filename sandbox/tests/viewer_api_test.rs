//! Viewer API integration tests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

use sandbox::actor_manager::AppState;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use sandbox::api;

fn file_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

async fn setup_test_app() -> (
    axum::Router,
    ractor::ActorRef<EventStoreMsg>,
    tempfile::TempDir,
) {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("invalid path");

    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path_str.to_string()),
    )
    .await
    .expect("failed to create event store");

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state);
    (app, event_store, temp_dir)
}

async fn json_response(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.expect("request failed");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("failed to read body")
        .to_bytes();
    let value: Value = serde_json::from_slice(&body).expect("invalid json");
    (status, value)
}

#[tokio::test]
async fn test_get_viewer_content_happy_path() {
    let (app, _event_store, temp_dir) = setup_test_app().await;
    let file_path = temp_dir.path().join("README.md");
    std::fs::write(&file_path, "# Hello viewer\n").expect("failed to write file");
    let uri = file_uri(&file_path);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/viewer/content?uri={uri}"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["uri"], uri);
    assert_eq!(body["mime"], "text/markdown");
    assert_eq!(body["content"], "# Hello viewer\n");
    assert_eq!(body["revision"]["rev"], 0);
}

#[tokio::test]
async fn test_patch_viewer_content_valid_base_rev() {
    let (app, _event_store, temp_dir) = setup_test_app().await;
    let file_path = temp_dir.path().join("notes.txt");
    std::fs::write(&file_path, "initial").expect("failed to write file");
    let uri = file_uri(&file_path);

    let patch_req = json!({
        "uri": uri,
        "base_rev": 0,
        "content": "updated text",
        "window_id": "window-1",
        "user_id": "user-1"
    });

    let req = Request::builder()
        .method("PATCH")
        .uri("/viewer/content")
        .header("content-type", "application/json")
        .body(Body::from(patch_req.to_string()))
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["revision"]["rev"], 1);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/viewer/content?uri={uri}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, get_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "updated text");
    assert_eq!(body["revision"]["rev"], 1);
}

#[tokio::test]
async fn test_patch_viewer_content_conflict() {
    let (app, _event_store, temp_dir) = setup_test_app().await;
    let file_path = temp_dir.path().join("conflict.md");
    std::fs::write(&file_path, "v1").expect("failed to write file");
    let uri = file_uri(&file_path);

    let patch_req_1 = json!({
        "uri": uri,
        "base_rev": 0,
        "content": "v2",
        "window_id": "window-a",
        "user_id": "user-1"
    });
    let req1 = Request::builder()
        .method("PATCH")
        .uri("/viewer/content")
        .header("content-type", "application/json")
        .body(Body::from(patch_req_1.to_string()))
        .unwrap();
    let (_status, _body) = json_response(&app, req1).await;

    let patch_req_2 = json!({
        "uri": uri,
        "base_rev": 0,
        "content": "stale save",
        "window_id": "window-b",
        "user_id": "user-1"
    });
    let req2 = Request::builder()
        .method("PATCH")
        .uri("/viewer/content")
        .header("content-type", "application/json")
        .body(Body::from(patch_req_2.to_string()))
        .unwrap();
    let (status, body) = json_response(&app, req2).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "revision_conflict");
    assert_eq!(body["latest"]["content"], "v2");
    assert_eq!(body["latest"]["revision"]["rev"], 1);
}

#[tokio::test]
async fn test_patch_viewer_content_appends_event_with_required_payload() {
    let (app, event_store, temp_dir) = setup_test_app().await;
    let file_path = temp_dir.path().join("audit.txt");
    std::fs::write(&file_path, "old").expect("failed to write file");
    let uri = file_uri(&file_path);

    let patch_req = json!({
        "uri": uri,
        "base_rev": 0,
        "content": "new",
        "window_id": "window-42",
        "user_id": "user-7"
    });
    let req = Request::builder()
        .method("PATCH")
        .uri("/viewer/content")
        .header("content-type", "application/json")
        .body(Body::from(patch_req.to_string()))
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let events = ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
        actor_id: format!("viewer:{uri}"),
        since_seq: 0,
        reply,
    })
    .expect("rpc failed")
    .expect("event store failed");

    let saved = events
        .into_iter()
        .find(|evt| evt.event_type == shared_types::EVENT_VIEWER_CONTENT_SAVED)
        .expect("missing viewer.content_saved event");

    assert_eq!(saved.payload["uri"], uri);
    assert_eq!(saved.payload["base_rev"], 0);
    assert_eq!(saved.payload["new_rev"], 1);
    assert_eq!(saved.payload["window_id"], "window-42");
    assert_eq!(saved.payload["user_id"], "user-7");
    assert!(saved.payload["mime"].as_str().unwrap().starts_with("text/"));
    assert!(!saved.payload["content_hash"].as_str().unwrap().is_empty());
}
