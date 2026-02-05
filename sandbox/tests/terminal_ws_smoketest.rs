//! Terminal WebSocket Smoke Test
//!
//! Verifies that a terminal WebSocket connection can be established
//! and that basic input produces output.

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use sandbox::actor_manager::AppState;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;

fn test_terminal_id() -> String {
    format!("test-terminal-{}", uuid::Uuid::new_v4())
}

fn test_user_id() -> String {
    format!("test-user-{}", uuid::Uuid::new_v4())
}

struct TestServer {
    addr: SocketAddr,
    _temp_dir: tempfile::TempDir,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn start_test_server() -> TestServer {
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

    let app: Router = api::router().with_state(api_state);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().expect("Failed to get addr");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .expect("Server failed");
    });

    TestServer {
        addr,
        _temp_dir: temp_dir,
        handle,
    }
}

fn ws_url(addr: SocketAddr, path: &str) -> String {
    format!("ws://{addr}{path}")
}

async fn recv_json(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    total_timeout: Duration,
) -> Value {
    let start = Instant::now();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= total_timeout {
            panic!("Timeout waiting for frame");
        }
        let remaining = total_timeout - elapsed;

        match timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).expect("Invalid JSON");
                return value;
            }
            Ok(Some(Ok(Message::Close(_)))) => panic!("Connection closed"),
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("Frame error: {e:?}"),
            Ok(None) => panic!("Stream ended"),
            Err(_) => panic!("Timeout waiting for frame"),
        }
    }
}

async fn send_json(
    ws: &mut (impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
    msg: Value,
) {
    let text = msg.to_string();
    ws.send(Message::Text(text)).await.expect("Send error");
}

#[cfg(unix)]
#[tokio::test]
async fn test_terminal_ws_smoke() {
    let server = start_test_server().await;
    let terminal_id = test_terminal_id();
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/terminal/{terminal_id}?user_id={user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let info = recv_json(&mut ws, Duration::from_secs(5)).await;
    assert_eq!(info["type"], "info");
    assert_eq!(info["terminal_id"], terminal_id);

    send_json(&mut ws, json!({"type":"input","data":"echo hi\r"})).await;

    let start = Instant::now();
    let mut saw_hi = false;
    while start.elapsed() < Duration::from_secs(5) {
        let msg = recv_json(&mut ws, Duration::from_secs(5)).await;
        if msg["type"] == "output" {
            if let Some(data) = msg["data"].as_str() {
                if data.contains("hi") {
                    saw_hi = true;
                    break;
                }
            }
        }
    }

    assert!(saw_hi, "Expected output containing 'hi'");
}
