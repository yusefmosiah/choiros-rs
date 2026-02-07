//! Desktop API Integration Tests
//!
//! Tests full HTTP request/response cycles for desktop endpoints

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

/// Generate a unique test desktop ID
fn test_desktop_id() -> String {
    format!("test-desktop-{}", uuid::Uuid::new_v4())
}

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

#[tokio::test]
async fn test_health_check() {
    let (app, _temp_dir) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "choiros-sandbox");
}

#[tokio::test]
async fn test_get_desktop_state_empty() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert!(body["desktop"].is_object());
    assert!(body["desktop"]["windows"].is_array());
    assert!(body["desktop"]["apps"].is_array());
}

#[tokio::test]
async fn test_register_app() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_open_window_success() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();

    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert!(body["window"].is_object());
    assert_eq!(body["window"]["title"], "Chat Window");
    assert_eq!(body["window"]["app_id"], "test-chat");
}

#[tokio::test]
async fn test_open_window_preserves_viewer_props() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "writer",
        "name": "Writer",
        "icon": "📝",
        "component_code": "WriterApp",
        "default_width": 800,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let (_status, _body) = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "writer",
        "title": "README.md",
        "props": {
            "viewer": {
                "kind": "text",
                "resource": {
                    "uri": "file:///workspace/README.md",
                    "mime": "text/markdown"
                },
                "capabilities": {
                    "readonly": false
                }
            }
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (status, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["window"]["props"]["viewer"]["kind"], "text");
    assert_eq!(
        body["window"]["props"]["viewer"]["resource"]["uri"],
        "file:///workspace/README.md"
    );
}

#[tokio::test]
async fn test_open_window_unknown_app_fails() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let open_req = json!({
        "app_id": "unknown-app",
        "title": "Test Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_get_windows_empty() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    let windows = body["windows"].as_array().unwrap();
    assert!(windows.is_empty());
}

#[tokio::test]
async fn test_get_windows_after_open() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let windows = body["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["id"], window_id);
}

#[tokio::test]
async fn test_close_window() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(&format!("/desktop/{desktop_id}/windows/{window_id}"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .body(Body::empty())
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let windows = body["windows"].as_array().unwrap();
    assert!(windows.is_empty());
}

#[tokio::test]
async fn test_move_window() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let move_req = json!({
        "x": 200,
        "y": 150
    });

    let req = Request::builder()
        .method("PATCH")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/position"
        ))
        .header("content-type", "application/json")
        .body(Body::from(move_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_resize_window() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let resize_req = json!({
        "width": 800,
        "height": 600
    });

    let req = Request::builder()
        .method("PATCH")
        .uri(&format!("/desktop/{desktop_id}/windows/{window_id}/size"))
        .header("content-type", "application/json")
        .body(Body::from(resize_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_resize_window_invalid_bounds_rejected() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let resize_req = json!({
        "width": 100,
        "height": 100
    });

    let req = Request::builder()
        .method("PATCH")
        .uri(&format!("/desktop/{desktop_id}/windows/{window_id}/size"))
        .header("content-type", "application/json")
        .body(Body::from(resize_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_focus_window() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req1 = json!({
        "app_id": "test-chat",
        "title": "Window 1",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req1.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req2 = json!({
        "app_id": "test-chat",
        "title": "Window 2",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req2.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows/{window_id}/focus"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_minimize_maximize_restore_endpoints() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/minimize"
        ))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());

    let req = Request::builder()
        .method("POST")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/maximize"
        ))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());

    let req = Request::builder()
        .method("POST")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/restore"
        ))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());

    let req = Request::builder()
        .method("POST")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/maximize"
        ))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert!(body["window"]["maximized"].as_bool().unwrap());

    let req = Request::builder()
        .method("POST")
        .uri(&format!(
            "/desktop/{desktop_id}/windows/{window_id}/restore"
        ))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert!(body["from"].is_string());
}

#[tokio::test]
async fn test_new_window_endpoints_reject_unknown_window() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();
    let bad_id = "nope";

    for action in ["minimize", "maximize", "restore"] {
        let req = Request::builder()
            .method("POST")
            .uri(&format!("/desktop/{desktop_id}/windows/{bad_id}/{action}"))
            .body(Body::empty())
            .unwrap();

        let (status, body) = json_response(&app, req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body["success"].as_bool().unwrap());
    }
}

#[tokio::test]
async fn test_get_apps_empty() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    let apps = body["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0]["id"], "chat");
}

#[tokio::test]
async fn test_get_apps_after_register() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let apps = body["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    let app_ids: Vec<String> = apps
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert!(app_ids.contains(&"test-chat".to_string()));
}

#[tokio::test]
async fn test_desktop_state_persists_events() {
    let (app, _temp_dir) = setup_test_app().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/apps"))
        .header("content-type", "application/json")
        .body(Body::from(app_def.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/desktop/{desktop_id}/windows"))
        .header("content-type", "application/json")
        .body(Body::from(open_req.to_string()))
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/desktop/{desktop_id}"))
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    assert!(body["success"].as_bool().unwrap());
    let desktop = &body["desktop"];
    assert_eq!(desktop["windows"].as_array().unwrap().len(), 1);
    assert_eq!(desktop["windows"][0]["id"], window_id);
    let apps = desktop["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    let app_ids: Vec<String> = apps
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert!(app_ids.contains(&"test-chat".to_string()));
}

#[tokio::test]
async fn test_get_user_preferences_default_theme() {
    let (app, _temp_dir) = setup_test_app().await;
    let user_id = "test-user";

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/user/{user_id}/preferences"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert_eq!(body["theme"], "dark");
}

#[tokio::test]
async fn test_update_and_get_user_preferences_theme() {
    let (app, _temp_dir) = setup_test_app().await;
    let user_id = "test-user";

    let update_req = json!({
        "theme": "light"
    });

    let req = Request::builder()
        .method("PATCH")
        .uri(&format!("/user/{user_id}/preferences"))
        .header("content-type", "application/json")
        .body(Body::from(update_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert_eq!(body["theme"], "light");

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/user/{user_id}/preferences"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert_eq!(body["theme"], "light");
}

#[tokio::test]
async fn test_update_user_preferences_rejects_invalid_theme() {
    let (app, _temp_dir) = setup_test_app().await;
    let user_id = "test-user";

    let update_req = json!({
        "theme": "solarized"
    });

    let req = Request::builder()
        .method("PATCH")
        .uri(&format!("/user/{user_id}/preferences"))
        .header("content-type", "application/json")
        .body(Body::from(update_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "theme must be 'light' or 'dark'");
}
