//! Desktop WebSocket Integration Tests

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use ractor::Actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;

fn test_desktop_id() -> String {
    format!("test-desktop-{}", uuid::Uuid::new_v4())
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
) -> Value {
    let timeout_duration = Duration::from_secs(5);

    loop {
        match timeout(timeout_duration, ws.next()).await {
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

async fn wait_for_type(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    expected_type: &str,
) -> Value {
    for _ in 0..10 {
        let msg = recv_json(ws).await;
        if msg["type"] == expected_type {
            return msg;
        }
    }

    panic!("did not receive message type {expected_type}");
}

async fn send_json(
    ws: &mut (impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
    msg: Value,
) {
    ws.send(Message::Text(msg.to_string()))
        .await
        .expect("Send error");
}

async fn post_json(
    client: &reqwest::Client,
    addr: SocketAddr,
    path: &str,
    payload: Value,
) -> reqwest::Response {
    client
        .post(format!("http://{addr}{path}"))
        .json(&payload)
        .send()
        .await
        .expect("request failed")
}

#[tokio::test]
async fn test_desktop_ws_emits_window_delta_after_mutation() {
    let server = start_test_server().await;
    let desktop_id = test_desktop_id();
    let client = reqwest::Client::new();

    let register = json!({
        "id": "test-chat",
        "name": "Test Chat",
        "icon": "💬",
        "component_code": "ChatView",
        "default_width": 400,
        "default_height": 600
    });
    let _ = post_json(
        &client,
        server.addr,
        &format!("/desktop/{desktop_id}/apps"),
        register,
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(server.addr, "/ws"))
        .await
        .expect("ws connect failed");
    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "subscribe",
            "desktop_id": desktop_id,
        }),
    )
    .await;

    let _ = wait_for_type(&mut ws, "desktop_state").await;

    let open_resp = post_json(
        &client,
        server.addr,
        &format!("/desktop/{desktop_id}/windows"),
        json!({
            "app_id": "test-chat",
            "title": "Chat",
            "props": null
        }),
    )
    .await
    .json::<Value>()
    .await
    .unwrap();

    let window_id = open_resp["window"]["id"].as_str().unwrap();

    let opened = wait_for_type(&mut ws, "window_opened").await;
    assert_eq!(opened["window"]["id"], window_id);

    let _ = client
        .patch(format!(
            "http://{}/desktop/{}/windows/{}/position",
            server.addr, desktop_id, window_id
        ))
        .json(&json!({ "x": 321, "y": 123 }))
        .send()
        .await
        .unwrap();

    let moved = wait_for_type(&mut ws, "window_moved").await;
    assert_eq!(moved["window_id"], window_id);
    assert_eq!(moved["x"], 321);
    assert_eq!(moved["y"], 123);
}

#[tokio::test]
async fn test_desktop_ws_delta_order_matches_mutation_order() {
    let server = start_test_server().await;
    let desktop_id = test_desktop_id();
    let client = reqwest::Client::new();

    let _ = post_json(
        &client,
        server.addr,
        &format!("/desktop/{desktop_id}/apps"),
        json!({
            "id": "test-chat",
            "name": "Test Chat",
            "icon": "💬",
            "component_code": "ChatView",
            "default_width": 400,
            "default_height": 600
        }),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(server.addr, "/ws"))
        .await
        .expect("ws connect failed");
    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "subscribe",
            "desktop_id": desktop_id,
        }),
    )
    .await;

    let _ = wait_for_type(&mut ws, "desktop_state").await;

    let open_resp = post_json(
        &client,
        server.addr,
        &format!("/desktop/{desktop_id}/windows"),
        json!({
            "app_id": "test-chat",
            "title": "Chat",
            "props": null
        }),
    )
    .await
    .json::<Value>()
    .await
    .unwrap();

    let window_id = open_resp["window"]["id"].as_str().unwrap();
    let _ = wait_for_type(&mut ws, "window_opened").await;

    let _ = client
        .post(format!(
            "http://{}/desktop/{}/windows/{}/minimize",
            server.addr, desktop_id, window_id
        ))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!(
            "http://{}/desktop/{}/windows/{}/restore",
            server.addr, desktop_id, window_id
        ))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!(
            "http://{}/desktop/{}/windows/{}/maximize",
            server.addr, desktop_id, window_id
        ))
        .send()
        .await
        .unwrap();

    let msg1 = wait_for_type(&mut ws, "window_minimized").await;
    let msg2 = wait_for_type(&mut ws, "window_restored").await;
    let msg3 = wait_for_type(&mut ws, "window_maximized").await;

    assert_eq!(msg1["window_id"], window_id);
    assert_eq!(msg2["window_id"], window_id);
    assert_eq!(msg3["window_id"], window_id);
}
