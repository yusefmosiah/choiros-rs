//! Chat API Integration Tests
//!
//! Tests full HTTP request/response cycles for chat endpoints

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

use sandbox::actor_manager::AppState;
use sandbox::actors::event_store::{AppendEvent, EventStoreMsg};
use sandbox::actors::{EventStoreActor, EventStoreArguments};
use sandbox::api;

/// Generate a unique test chat ID
fn test_chat_id() -> String {
    format!("test-chat-{}", uuid::Uuid::new_v4())
}

async fn setup_test_app() -> (
    axum::Router,
    tempfile::TempDir,
    ractor::ActorRef<EventStoreMsg>,
) {
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

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state);
    (app, temp_dir, event_store)
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
async fn test_send_message_success() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": "Hello, world!"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    assert!(body["temp_id"].is_string());
}

#[tokio::test]
async fn test_send_empty_message_rejected() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": ""
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_get_messages_empty() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id = test_chat_id();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id}/messages"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());
    let messages = body["messages"].as_array().unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
async fn test_send_and_get_messages() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": "Test message"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let _temp_id = body["temp_id"].as_str().unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id}/messages"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["success"].as_bool().unwrap());

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Test message");
    assert!(messages[0]["id"].is_string());
}

#[tokio::test]
async fn test_send_multiple_messages() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id = test_chat_id();

    for i in 1..=3 {
        let message_req = json!({
            "actor_id": chat_id,
            "user_id": "test-user",
            "text": format!("Message {}", i)
        });

        let req = Request::builder()
            .method("POST")
            .uri("/chat/send")
            .header("content-type", "application/json")
            .body(Body::from(message_req.to_string()))
            .unwrap();

        let _ = json_response(&app, req).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id}/messages"))
        .body(Body::empty())
        .unwrap();

    let (_status, body) = json_response(&app, req).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);

    let texts: Vec<String> = messages
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_string())
        .collect();
    assert!(texts.contains(&"Message 1".to_string()));
    assert!(texts.contains(&"Message 2".to_string()));
    assert!(texts.contains(&"Message 3".to_string()));
}

#[tokio::test]
async fn test_different_chat_isolation() {
    let (app, _temp_dir, _event_store) = setup_test_app().await;
    let chat_id1 = test_chat_id();
    let chat_id2 = test_chat_id();

    let message_req = json!({
        "actor_id": chat_id1,
        "user_id": "test-user",
        "text": "Chat 1 message"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let message_req = json!({
        "actor_id": chat_id2,
        "user_id": "test-user",
        "text": "Chat 2 message"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .unwrap();
    let _ = json_response(&app, req).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id1}/messages"))
        .body(Body::empty())
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Chat 1 message");

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id2}/messages"))
        .body(Body::empty())
        .unwrap();
    let (_status, body) = json_response(&app, req).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "Chat 2 message");
}

#[tokio::test]
async fn test_get_messages_includes_tool_events_as_system_messages() {
    let (app, _temp_dir, event_store) = setup_test_app().await;
    let chat_id = test_chat_id();
    let user_id = "test-user";

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
            payload: json!("Find current weather"),
            actor_id: chat_id.clone(),
            user_id: user_id.to_string(),
        },
        reply,
    })
    .unwrap()
    .unwrap();

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: shared_types::EVENT_CHAT_TOOL_CALL.to_string(),
            payload: json!({
                "tool_name": "weather",
                "tool_args": "{\"location\":\"San Francisco, CA\"}",
                "reasoning": "Need live weather"
            }),
            actor_id: chat_id.clone(),
            user_id: user_id.to_string(),
        },
        reply,
    })
    .unwrap()
    .unwrap();

    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: shared_types::EVENT_CHAT_TOOL_RESULT.to_string(),
            payload: json!({
                "tool_name": "weather",
                "success": true,
                "output": "{\"temp\":\"61F\"}"
            }),
            actor_id: chat_id.clone(),
            user_id: user_id.to_string(),
        },
        reply,
    })
    .unwrap()
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/chat/{chat_id}/messages"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);

    assert_eq!(messages[1]["sender"], "System");
    assert!(messages[1]["text"]
        .as_str()
        .unwrap_or_default()
        .starts_with("__tool_call__:"));

    assert_eq!(messages[2]["sender"], "System");
    assert!(messages[2]["text"]
        .as_str()
        .unwrap_or_default()
        .starts_with("__tool_result__:"));
}
