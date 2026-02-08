//! WebSocket Logs Integration Tests

use axum::Router;
use futures_util::StreamExt;
use ractor::Actor;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use sandbox::api;
use sandbox::app_state::AppState;

struct TestServer {
    addr: SocketAddr,
    event_store: ractor::ActorRef<EventStoreMsg>,
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

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let api_state = api::ApiState {
        app_state: app_state.clone(),
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
        event_store,
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
                return serde_json::from_str(&text).expect("Invalid JSON");
            }
            Ok(Some(Ok(Message::Close(_)))) => panic!("Connection closed"),
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("Frame error: {e:?}"),
            Ok(None) => panic!("Stream ended"),
            Err(_) => panic!("Timeout waiting for frame"),
        }
    }
}

#[tokio::test]
async fn test_logs_ws_connected() {
    let server = start_test_server().await;
    let (mut ws, _) = connect_async(ws_url(server.addr, "/ws/logs/events"))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["since_seq"], 0);
}

#[tokio::test]
async fn test_logs_ws_streams_filtered_events() {
    let server = start_test_server().await;
    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        "/ws/logs/events?event_type_prefix=watcher.alert&poll_ms=50",
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");

    let _ = ractor::call!(server.event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: "worker.task.progress".to_string(),
            payload: serde_json::json!({"step":"ignore"}),
            actor_id: "supervisor:test".to_string(),
            user_id: "system".to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();

    let _ = ractor::call!(server.event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: "watcher.alert.failure_spike".to_string(),
            payload: serde_json::json!({"rule":"worker_failure_spike"}),
            actor_id: "watcher:default".to_string(),
            user_id: "system".to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();

    let event = recv_json(&mut ws).await;
    assert_eq!(event["type"], "event");
    assert_eq!(event["event_type"], "watcher.alert.failure_spike");
    assert_eq!(event["actor_id"], "watcher:default");
    assert_eq!(event["payload"]["rule"], "worker_failure_spike");
}
