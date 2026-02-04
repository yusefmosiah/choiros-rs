//! Chat API Integration Tests
//!
//! Tests full HTTP request/response cycles for chat endpoints

use actix_web::{http::StatusCode, test, web, App};
use ractor::Actor;
use sandbox::actor_manager::AppState;
use sandbox::actors::{EventStoreActor, EventStoreArguments};
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

            // Create event store with test database using ractor
            let (event_store, _handle) = Actor::spawn(
                None,
                EventStoreActor,
                EventStoreArguments::File(db_path_str.to_string()),
            )
            .await
            .expect("Failed to create event store");

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

/// Generate a unique test chat ID
fn test_chat_id() -> String {
    format!("test-chat-{}", uuid::Uuid::new_v4())
}

#[actix_web::test]
async fn test_send_message_success() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": "Hello, world!"
    });

    let req = test::TestRequest::post()
        .uri("/chat/send")
        .set_json(&message_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    assert!(body["temp_id"].is_string());
}

#[actix_web::test]
async fn test_send_empty_message_rejected() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": ""
    });

    let req = test::TestRequest::post()
        .uri("/chat/send")
        .set_json(&message_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(!body["success"].as_bool().unwrap());
}

#[actix_web::test]
async fn test_get_messages_empty() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id = test_chat_id();

    let req = test::TestRequest::get()
        .uri(&format!("/chat/{chat_id}/messages"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    let messages = body["messages"].as_array().unwrap();
    assert!(messages.is_empty());
}

#[actix_web::test]
async fn test_send_and_get_messages() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id = test_chat_id();

    // Send a message
    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": "Test message"
    });

    let req = test::TestRequest::post()
        .uri("/chat/send")
        .set_json(&message_req)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let temp_id = body["temp_id"].as_str().unwrap();

    // Wait a bit for the async event persistence to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Get messages
    let req = test::TestRequest::get()
        .uri(&format!("/chat/{chat_id}/messages"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Test message");
    // The persisted message has a different ID than the temp_id (which is expected)
    // Just verify that the message has a valid ID
    assert!(messages[0]["id"].is_string());
}

#[actix_web::test]
async fn test_send_multiple_messages() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id = test_chat_id();

    // Send multiple messages
    for i in 1..=3 {
        let message_req = json!({
            "actor_id": chat_id,
            "user_id": "test-user",
            "text": format!("Message {}", i)
        });

        let req = test::TestRequest::post()
            .uri("/chat/send")
            .set_json(&message_req)
            .to_request();

        test::call_service(&app, req).await;
    }

    // Wait a bit for the async event persistence to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Get messages and verify all are present (order not guaranteed due to HashMap)
    let req = test::TestRequest::get()
        .uri(&format!("/chat/{chat_id}/messages"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);

    // Verify all expected messages are present regardless of order
    let texts: Vec<String> = messages
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_string())
        .collect();
    assert!(texts.contains(&"Message 1".to_string()));
    assert!(texts.contains(&"Message 2".to_string()));
    assert!(texts.contains(&"Message 3".to_string()));
}

#[actix_web::test]
async fn test_different_chat_isolation() {
    let (app, _temp_dir) = setup_test_app!().await;
    let chat_id1 = test_chat_id();
    let chat_id2 = test_chat_id();

    // Send message to first chat
    let message_req = json!({
        "actor_id": chat_id1,
        "user_id": "test-user",
        "text": "Chat 1 message"
    });

    let req = test::TestRequest::post()
        .uri("/chat/send")
        .set_json(&message_req)
        .to_request();
    test::call_service(&app, req).await;

    // Send message to second chat
    let message_req = json!({
        "actor_id": chat_id2,
        "user_id": "test-user",
        "text": "Chat 2 message"
    });

    let req = test::TestRequest::post()
        .uri("/chat/send")
        .set_json(&message_req)
        .to_request();
    test::call_service(&app, req).await;

    // Verify each chat has only its own message
    let req = test::TestRequest::get()
        .uri(&format!("/chat/{chat_id1}/messages"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Chat 1 message");

    let req = test::TestRequest::get()
        .uri(&format!("/chat/{chat_id2}/messages"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Chat 2 message");
}
