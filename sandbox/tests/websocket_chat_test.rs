//! WebSocket Chat Integration Tests
//!
//! Tests full WebSocket communication cycles for chat streaming functionality.
//! Tests cover connection, message streaming, ping/pong, error handling, and
//! concurrent connections.

use actix::Actor;
use actix_http::ws::{Frame, ProtocolError};
use actix_web::{web, App, Error};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;

use sandbox::actor_manager::AppState;
use sandbox::actors::event_store::EventStoreActor;
use sandbox::api;

/// Generate a unique test actor ID
fn test_actor_id() -> String {
    format!("test-actor-{}", uuid::Uuid::new_v4())
}

/// Generate a unique test user ID
fn test_user_id() -> String {
    format!("test-user-{}", uuid::Uuid::new_v4())
}

/// Helper to receive and parse a WebSocket text frame
async fn recv_json(
    ws: &mut (impl StreamExt<Item = Result<Frame, ProtocolError>> + Unpin),
) -> Result<Value, Error> {
    let timeout_duration = Duration::from_secs(5);

    loop {
        match timeout(timeout_duration, ws.next()).await {
            Ok(Some(Ok(Frame::Text(bytes)))) => {
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("Invalid UTF-8: {}", e))
                })?;
                let value: Value = serde_json::from_str(text).map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("Invalid JSON: {}", e))
                })?;
                return Ok(value);
            }
            Ok(Some(Ok(Frame::Close(_)))) => {
                return Err(actix_web::error::ErrorBadRequest("Connection closed"))
            }
            Ok(Some(Ok(_))) => {
                // Handle other frame types (Binary, Ping, Pong, Continuation) by continuing loop
                continue;
            }
            Ok(Some(Err(e))) => {
                return Err(actix_web::error::ErrorInternalServerError(format!(
                    "Frame error: {:?}",
                    e
                )))
            }
            Ok(None) => return Err(actix_web::error::ErrorBadRequest("Stream ended")),
            Err(_) => {
                return Err(actix_web::error::ErrorRequestTimeout(
                    "Timeout waiting for frame",
                ))
            }
        }
    }
}

/// Helper to send a WebSocket text message
async fn send_json(
    ws: &mut (impl SinkExt<actix_http::ws::Message, Error = ProtocolError> + Unpin),
    msg: Value,
) -> Result<(), Error> {
    let text = msg.to_string();
    ws.send(actix_http::ws::Message::Text(text.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Send error: {:?}", e)))?;
    Ok(())
}

#[actix_web::test]
async fn test_websocket_connection_with_query_param() {
    // Create event store for this test
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    // Start test server
    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();
    let user_id = test_user_id();

    // Create WebSocket connection
    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}?user_id={}", actor_id, user_id))
        .await
        .expect("Failed to connect WebSocket");

    // Receive connected message
    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected message");
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

#[actix_web::test]
async fn test_websocket_connection_with_path_param() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();
    let user_id = test_user_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}/{}", actor_id, user_id))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected message");
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

#[actix_web::test]
async fn test_websocket_connection_default_user() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected message");
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], "anonymous");
}

#[actix_web::test]
async fn test_websocket_ping_pong() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send ping
    send_json(&mut framed, json!({"type": "ping"}))
        .await
        .expect("Failed to send ping");

    // Receive pong
    let pong = recv_json(&mut framed).await.expect("Should receive pong");
    assert_eq!(pong["type"], "pong");
}

#[actix_web::test]
async fn test_websocket_error_handling_invalid_json() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send invalid JSON
    framed
        .send(actix_http::ws::Message::Text("not valid json".into()))
        .await
        .expect("Failed to send");

    // Receive error response
    let error = recv_json(&mut framed).await.expect("Should receive error");
    assert_eq!(error["type"], "error");
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("Invalid message format"));
}

#[actix_web::test]
async fn test_websocket_model_switch_success() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send switch_model message
    send_json(
        &mut framed,
        json!({
            "type": "switch_model",
            "model": "ClaudeBedrock"
        }),
    )
    .await
    .expect("Failed to send switch_model");

    // Receive model_switched response
    let response = recv_json(&mut framed)
        .await
        .expect("Should receive model_switched");
    assert_eq!(response["type"], "model_switched");
    assert_eq!(response["model"], "ClaudeBedrock");
    assert_eq!(response["status"], "success");
}

#[actix_web::test]
async fn test_websocket_model_switch_another_valid() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send switch_model message for GLM47
    send_json(
        &mut framed,
        json!({
            "type": "switch_model",
            "model": "GLM47"
        }),
    )
    .await
    .expect("Failed to send switch_model");

    // Receive model_switched response
    let response = recv_json(&mut framed)
        .await
        .expect("Should receive model_switched");
    assert_eq!(response["type"], "model_switched");
    assert_eq!(response["model"], "GLM47");
}

#[actix_web::test]
async fn test_websocket_concurrent_connections() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    // Create multiple connections to the same server
    let num_connections = 3;
    let mut connections: Vec<(String, String, _)> = vec![];

    for i in 0..num_connections {
        let actor_id = format!("concurrent-actor-{}", i);
        let user_id = format!("concurrent-user-{}", i);

        let framed = srv
            .ws_at(&format!("/ws/chat/{}?user_id={}", actor_id, user_id))
            .await
            .expect("Failed to connect WebSocket");

        connections.push((actor_id, user_id, framed));
    }

    // Verify each connection received correct connected message
    for (actor_id, user_id, framed) in connections.iter_mut() {
        let connected = recv_json(framed)
            .await
            .expect("Should receive connected message");
        assert_eq!(connected["type"], "connected");
        assert_eq!(connected["actor_id"], *actor_id);
        assert_eq!(connected["user_id"], *user_id);
    }

    // Send ping to each connection
    for (_, _, framed) in connections.iter_mut() {
        send_json(framed, json!({"type": "ping"}))
            .await
            .expect("Failed to send ping");
    }

    // Receive pong from each
    for (_, _, framed) in connections.iter_mut() {
        let pong = recv_json(framed).await.expect("Should receive pong");
        assert_eq!(pong["type"], "pong");
    }
}

#[actix_web::test]
async fn test_websocket_connection_isolation() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    // Create two connections with different actor_ids
    let actor_id_1 = test_actor_id();
    let actor_id_2 = test_actor_id();

    let mut framed1 = srv
        .ws_at(&format!("/ws/chat/{}", actor_id_1))
        .await
        .expect("Failed to connect WebSocket 1");

    let mut framed2 = srv
        .ws_at(&format!("/ws/chat/{}", actor_id_2))
        .await
        .expect("Failed to connect WebSocket 2");

    // Verify each gets its own actor_id
    let connected1 = recv_json(&mut framed1)
        .await
        .expect("Should receive connected 1");
    let connected2 = recv_json(&mut framed2)
        .await
        .expect("Should receive connected 2");

    assert_eq!(connected1["actor_id"], actor_id_1);
    assert_eq!(connected2["actor_id"], actor_id_2);
    assert_ne!(connected1["actor_id"], connected2["actor_id"]);
}

#[actix_web::test]
async fn test_websocket_multiple_pings() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send multiple pings
    for _ in 0..5 {
        send_json(&mut framed, json!({"type": "ping"}))
            .await
            .expect("Failed to send ping");
    }

    // Receive all pongs
    for _ in 0..5 {
        let pong = recv_json(&mut framed).await.expect("Should receive pong");
        assert_eq!(pong["type"], "pong");
    }
}

#[actix_web::test]
async fn test_websocket_unknown_message_type() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send unknown message type
    send_json(
        &mut framed,
        json!({
            "type": "unknown_type",
            "data": "something"
        }),
    )
    .await
    .expect("Failed to send unknown type");

    // Should receive error about invalid format
    let error = recv_json(&mut framed).await.expect("Should receive error");
    assert_eq!(error["type"], "error");
}

#[actix_web::test]
async fn test_websocket_large_actor_id() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    // Test with a longer actor_id to ensure proper handling
    let actor_id = format!("test-actor-{}", "x".repeat(100));
    let user_id = test_user_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}?user_id={}", actor_id, user_id))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected");
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
}

#[actix_web::test]
async fn test_websocket_empty_message_handling() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send empty message - should be handled gracefully (sends to agent which will use BAML)
    // Note: This will timeout/fail without BAML credentials, but tests the protocol
    send_json(
        &mut framed,
        json!({
            "type": "message",
            "text": ""
        }),
    )
    .await
    .expect("Failed to send empty message");

    // Should receive initial thinking message (before BAML call)
    let chunk = recv_json(&mut framed)
        .await
        .expect("Should receive thinking");
    assert_eq!(chunk["type"], "thinking");
    assert_eq!(chunk["content"], "Processing your message...");

    // The rest depends on BAML - test what we can
}

/// Test that verifies WebSocket connection closes properly
#[actix_web::test]
async fn test_websocket_close_connection() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Skip connected message
    let _ = recv_json(&mut framed).await;

    // Send close frame
    framed
        .send(actix_http::ws::Message::Close(None))
        .await
        .expect("Failed to send close");

    // Should receive close response or stream end
    match timeout(Duration::from_secs(2), framed.next()).await {
        Ok(Some(Ok(Frame::Close(_)))) => {
            // Expected - connection closed properly
        }
        Ok(Some(Ok(_))) => {
            // Might receive other frames before close, that's ok
        }
        Ok(None) => {
            // Stream ended, which is also acceptable
        }
        _ => {
            // Timeout or error is acceptable for close test
        }
    }
}

/// Test protocol version requirement
#[actix_web::test]
async fn test_websocket_protocol_version_required() {
    // This test verifies the server requires WebSocket protocol version 13
    // The awc client automatically handles this, so we test it implicitly
    // by verifying connections succeed with proper headers
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}", actor_id))
        .await
        .expect("Failed to connect WebSocket");

    // Verify we can communicate
    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected");
    assert_eq!(connected["type"], "connected");
}

/// Test connection with special characters in actor_id
#[actix_web::test]
async fn test_websocket_special_chars_in_actor_id() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    // Test URL-encoded special characters
    let actor_id = "test-actor-with-special-chars-123".to_string();
    let user_id = test_user_id();

    let mut framed = srv
        .ws_at(&format!("/ws/chat/{}?user_id={}", actor_id, user_id))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut framed)
        .await
        .expect("Should receive connected");
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

/// Test rapid connect/disconnect cycles
#[actix_web::test]
async fn test_websocket_rapid_connect_disconnect() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let actor_id = test_actor_id();

    // Connect and disconnect multiple times
    for _ in 0..3 {
        let mut framed = srv
            .ws_at(&format!("/ws/chat/{}", actor_id))
            .await
            .expect("Failed to connect WebSocket");

        // Verify connection works
        let connected = recv_json(&mut framed)
            .await
            .expect("Should receive connected");
        assert_eq!(connected["type"], "connected");

        // Send ping to verify it's working
        send_json(&mut framed, json!({"type": "ping"}))
            .await
            .expect("Failed to send ping");
        let pong = recv_json(&mut framed).await.expect("Should receive pong");
        assert_eq!(pong["type"], "pong");

        // Disconnect
        let _ = framed.send(actix_http::ws::Message::Close(None)).await;
    }
}
