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

async fn append_event(
    event_store: &ractor::ActorRef<EventStoreMsg>,
    event_type: &str,
    payload: Value,
    actor_id: &str,
    user_id: &str,
) {
    let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: actor_id.to_string(),
            user_id: user_id.to_string(),
        },
        reply
    })
    .unwrap()
    .unwrap();
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

    append_event(
        &server.event_store,
        "worker.task.progress",
        serde_json::json!({"step":"ignore"}),
        "supervisor:test",
        "system",
    )
    .await;

    append_event(
        &server.event_store,
        "watcher.alert.failure_spike",
        serde_json::json!({"rule":"worker_failure_spike"}),
        "watcher:default",
        "system",
    )
    .await;

    let event = recv_json(&mut ws).await;
    assert_eq!(event["type"], "event");
    assert_eq!(event["event_type"], "watcher.alert.failure_spike");
    assert_eq!(event["actor_id"], "watcher:default");
    assert_eq!(event["payload"]["rule"], "worker_failure_spike");
}

#[tokio::test]
async fn test_logs_ws_streams_only_events_in_requested_scope() {
    let server = start_test_server().await;
    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        "/ws/logs/events?session_id=session-a&thread_id=thread-a&run_id=run-a&correlation_id=corr-a&poll_ms=50",
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["session_id"], "session-a");
    assert_eq!(connected["thread_id"], "thread-a");
    assert_eq!(connected["run_id"], "run-a");
    assert_eq!(connected["correlation_id"], "corr-a");

    append_event(
        &server.event_store,
        "interaction.user_msg",
        serde_json::json!({
            "text":"wrong scope",
            "run_id":"run-b",
            "correlation_id":"corr-b",
            "scope": {
                "session_id":"session-b",
                "thread_id":"thread-b"
            }
        }),
        "thread-b",
        "user-1",
    )
    .await;

    append_event(
        &server.event_store,
        "interaction.user_msg",
        serde_json::json!({
            "text":"correct scope",
            "run_id":"run-a",
            "correlation_id":"corr-a",
            "scope": {
                "session_id":"session-a",
                "thread_id":"thread-a"
            }
        }),
        "thread-a",
        "user-1",
    )
    .await;

    let event = recv_json(&mut ws).await;
    assert_eq!(event["type"], "event");
    assert_eq!(event["payload"]["text"], "correct scope");
    assert_eq!(event["payload"]["run_id"], "run-a");
    assert_eq!(event["payload"]["correlation_id"], "corr-a");

    let next_frame = timeout(Duration::from_millis(250), ws.next()).await;
    assert!(
        next_frame.is_err(),
        "unexpected extra websocket frame after scoped match"
    );
}
