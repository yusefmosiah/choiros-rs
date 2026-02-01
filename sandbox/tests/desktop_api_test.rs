//! Desktop API Integration Tests
//!
//! Tests full HTTP request/response cycles for desktop endpoints

use actix::Actor;
use actix_web::{http::StatusCode, test, web, App};
use sandbox::actor_manager::AppState;
use sandbox::actors::EventStoreActor;
use sandbox::api;
use serde_json::json;

/// Macro to set up a test app with isolated database
///
/// This macro avoids type erasure issues by expanding the setup code inline.
/// Usage: let (app, _temp_dir) = setup_test_app!().await;
#[macro_export]
macro_rules! setup_test_app {
    () => {{
        async {
            // Create temp directory for isolated test database
            let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
            let db_path = temp_dir.path().join("test_events.db");
            let db_path_str = db_path.to_str().expect("Invalid database path");

            // Create event store with test database
            let event_store = EventStoreActor::new(db_path_str)
                .await
                .expect("Failed to create event store")
                .start();

            // Create app state
            let app_state = web::Data::new(AppState::new(event_store));

            // Create app with all routes
            let app = test::init_service(
                App::new()
                    .app_data(app_state.clone())
                    .route("/health", web::get().to(api::health_check))
                    .configure(api::config),
            )
            .await;

            (app, temp_dir)
        }
    }};
}

/// Generate a unique test desktop ID
fn test_desktop_id() -> String {
    format!("test-desktop-{}", uuid::Uuid::new_v4())
}

#[actix_web::test]
async fn test_health_check() {
    let (app, _temp_dir) = setup_test_app!().await;

    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "choiros-sandbox");
}

#[actix_web::test]
async fn test_get_desktop_state_empty() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    assert!(body["desktop"].is_object());
    assert!(body["desktop"]["windows"].is_array());
    assert!(body["desktop"]["apps"].is_array());
}

#[actix_web::test]
async fn test_register_app() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
}

#[actix_web::test]
async fn test_open_window_success() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // First register an app
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Now open a window
    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    assert!(body["window"].is_object());
    assert_eq!(body["window"]["title"], "Chat Window");
    assert_eq!(body["window"]["app_id"], "test-chat");
}

#[actix_web::test]
async fn test_open_window_unknown_app_fails() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Try to open window for unregistered app
    let open_req = json!({
        "app_id": "unknown-app",
        "title": "Test Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(!body["success"].as_bool().unwrap());
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

#[actix_web::test]
async fn test_get_windows_empty() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    let windows = body["windows"].as_array().unwrap();
    assert!(windows.is_empty());
}

#[actix_web::test]
async fn test_get_windows_after_open() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Setup: register app and open window
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Test: get windows
    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    let windows = body["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["id"], window_id);
}

#[actix_web::test]
async fn test_close_window() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Setup: register app and open window
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Test: close window
    let req = test::TestRequest::delete()
        .uri(&format!("/desktop/{}/windows/{}", desktop_id, window_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());

    // Verify window is gone
    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let windows = body["windows"].as_array().unwrap();
    assert!(windows.is_empty());
}

#[actix_web::test]
async fn test_move_window() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Setup: register app and open window
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Test: move window
    let move_req = json!({
        "x": 200,
        "y": 150
    });

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/desktop/{}/windows/{}/position",
            desktop_id, window_id
        ))
        .set_json(&move_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
}

#[actix_web::test]
async fn test_resize_window() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Setup: register app and open window
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Test: resize window
    let resize_req = json!({
        "width": 800,
        "height": 600
    });

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/desktop/{}/windows/{}/size",
            desktop_id, window_id
        ))
        .set_json(&resize_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
}

#[actix_web::test]
async fn test_focus_window() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Setup: register app and open two windows
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req1 = json!({
        "app_id": "test-chat",
        "title": "Window 1",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req1)
        .to_request();
    test::call_service(&app, req).await;

    let open_req2 = json!({
        "app_id": "test-chat",
        "title": "Window 2",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req2)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Test: focus window
    let req = test::TestRequest::post()
        .uri(&format!(
            "/desktop/{}/windows/{}/focus",
            desktop_id, window_id
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
}

#[actix_web::test]
async fn test_get_apps_empty() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    let apps = body["apps"].as_array().unwrap();
    // DesktopActor automatically registers a default "chat" app
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0]["id"], "chat");
}

#[actix_web::test]
async fn test_get_apps_after_register() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Register an app
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    // Get apps
    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    let apps = body["apps"].as_array().unwrap();
    // DesktopActor has 1 default app + 1 registered app = 2 total
    assert_eq!(apps.len(), 2);
    // Check that our registered app is present
    let app_ids: Vec<String> = apps
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert!(app_ids.contains(&"test-chat".to_string()));
}

#[actix_web::test]
async fn test_desktop_state_persists_events() {
    let (app, _temp_dir) = setup_test_app!().await;
    let desktop_id = test_desktop_id();

    // Register app and open window
    let app_def = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/apps", desktop_id))
        .set_json(&app_def)
        .to_request();
    test::call_service(&app, req).await;

    let open_req = json!({
        "app_id": "test-chat",
        "title": "Chat Window",
        "props": null
    });

    let req = test::TestRequest::post()
        .uri(&format!("/desktop/{}/windows", desktop_id))
        .set_json(&open_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let window_id = body["window"]["id"].as_str().unwrap();

    // Get full desktop state
    let req = test::TestRequest::get()
        .uri(&format!("/desktop/{}", desktop_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert!(body["success"].as_bool().unwrap());
    let desktop = &body["desktop"];
    assert_eq!(desktop["windows"].as_array().unwrap().len(), 1);
    assert_eq!(desktop["windows"][0]["id"], window_id);
    // DesktopActor has 1 default app + 1 registered app = 2 total
    let apps = desktop["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    // Check that our registered app is present
    let app_ids: Vec<String> = apps
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert!(app_ids.contains(&"test-chat".to_string()));
}
