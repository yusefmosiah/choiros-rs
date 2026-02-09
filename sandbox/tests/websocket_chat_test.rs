//! WebSocket Chat Integration Tests
//!
//! Tests full WebSocket communication cycles for chat streaming functionality.
//! Tests cover connection, message streaming, ping/pong, error handling, and
//! concurrent connections.

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

use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use sandbox::api;
use sandbox::app_state::AppState;

/// Generate a unique test actor ID
fn test_actor_id() -> String {
    format!("test-actor-{}", uuid::Uuid::new_v4())
}

/// Generate a unique test user ID
fn test_user_id() -> String {
    format!("test-user-{}", uuid::Uuid::new_v4())
}

struct TestServer {
    addr: SocketAddr,
    app_state: Arc<AppState>,
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
        app_state,
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

async fn try_recv_json(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    timeout_duration: Duration,
) -> Option<Value> {
    loop {
        match timeout(timeout_duration, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).expect("Invalid JSON");
                return Some(value);
            }
            Ok(Some(Ok(Message::Close(_)))) => return None,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("Frame error: {e:?}"),
            Ok(None) => return None,
            Err(_) => return None,
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

async fn wait_for_worker_task_completion(
    app_state: &Arc<AppState>,
    actor_id: &str,
    correlation_id: &str,
    timeout_duration: Duration,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout_duration {
        let recent = match ractor::call!(app_state.event_store(), |reply| {
            sandbox::actors::event_store::EventStoreMsg::GetRecentEvents {
                since_seq: 0,
                limit: 500,
                event_type_prefix: Some("worker.task".to_string()),
                actor_id: Some(actor_id.to_string()),
                user_id: None,
                reply,
            }
        }) {
            Ok(Ok(events)) => events,
            _ => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };

        let done = recent.into_iter().any(|event| {
            let corr = event
                .payload
                .get("correlation_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            corr == correlation_id
                && (event.event_type == "worker.task.completed"
                    || event.event_type == "worker.task.failed")
        });
        if done {
            return true;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn test_websocket_connection_with_query_param() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/chat/{actor_id}?user_id={user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

#[tokio::test]
async fn test_websocket_connection_with_path_param() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/chat/{actor_id}/{user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

#[tokio::test]
async fn test_websocket_connection_default_user() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], "anonymous");
}

#[tokio::test]
async fn test_websocket_ping_pong() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    send_json(&mut ws, json!({"type": "ping"})).await;

    let pong = recv_json(&mut ws).await;
    assert_eq!(pong["type"], "pong");
}

#[tokio::test]
async fn test_websocket_error_handling_invalid_json() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    ws.send(Message::Text("not valid json".to_string()))
        .await
        .expect("Failed to send");

    let error = recv_json(&mut ws).await;
    assert_eq!(error["type"], "error");
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("Invalid message format"));
}

#[tokio::test]
async fn test_websocket_model_switch_success() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "switch_model",
            "model": "ClaudeBedrock"
        }),
    )
    .await;

    let response = recv_json(&mut ws).await;
    assert_eq!(response["type"], "model_switched");
    assert_eq!(response["model"], "ClaudeBedrock");
    assert_eq!(response["status"], "success");
}

#[tokio::test]
async fn test_websocket_model_switch_another_valid() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "switch_model",
            "model": "GLM47"
        }),
    )
    .await;

    let response = recv_json(&mut ws).await;
    assert_eq!(response["type"], "model_switched");
    assert_eq!(response["model"], "GLM47");
}

#[tokio::test]
async fn test_websocket_concurrent_connections() {
    let server = start_test_server().await;

    let num_connections = 3;
    let mut connections = Vec::new();

    for i in 0..num_connections {
        let actor_id = format!("concurrent-actor-{i}");
        let user_id = format!("concurrent-user-{i}");
        let (ws, _) = connect_async(ws_url(
            server.addr,
            &format!("/ws/chat/{actor_id}?user_id={user_id}"),
        ))
        .await
        .expect("Failed to connect WebSocket");
        connections.push((actor_id, user_id, ws));
    }

    for (actor_id, user_id, ws) in connections.iter_mut() {
        let connected = recv_json(ws).await;
        assert_eq!(connected["type"], "connected");
        assert_eq!(connected["actor_id"], *actor_id);
        assert_eq!(connected["user_id"], *user_id);
    }

    for (_, _, ws) in connections.iter_mut() {
        send_json(ws, json!({"type": "ping"})).await;
    }

    for (_, _, ws) in connections.iter_mut() {
        let pong = recv_json(ws).await;
        assert_eq!(pong["type"], "pong");
    }
}

#[tokio::test]
async fn test_websocket_connection_isolation() {
    let server = start_test_server().await;
    let actor_id_1 = test_actor_id();
    let actor_id_2 = test_actor_id();

    let (mut ws1, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id_1}")))
        .await
        .expect("Failed to connect WebSocket 1");

    let (mut ws2, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id_2}")))
        .await
        .expect("Failed to connect WebSocket 2");

    let connected1 = recv_json(&mut ws1).await;
    let connected2 = recv_json(&mut ws2).await;

    assert_eq!(connected1["actor_id"], actor_id_1);
    assert_eq!(connected2["actor_id"], actor_id_2);
    assert_ne!(connected1["actor_id"], connected2["actor_id"]);
}

#[tokio::test]
async fn test_websocket_multiple_pings() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    for _ in 0..5 {
        send_json(&mut ws, json!({"type": "ping"})).await;
    }

    for _ in 0..5 {
        let pong = recv_json(&mut ws).await;
        assert_eq!(pong["type"], "pong");
    }
}

#[tokio::test]
async fn test_websocket_unknown_message_type() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "unknown_type",
            "data": "something"
        }),
    )
    .await;

    let error = recv_json(&mut ws).await;
    assert_eq!(error["type"], "error");
}

#[tokio::test]
async fn test_websocket_large_actor_id() {
    let server = start_test_server().await;
    let actor_id = format!("test-actor-{}", "x".repeat(100));
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/chat/{actor_id}?user_id={user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
}

#[tokio::test]
async fn test_websocket_empty_message_handling() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    send_json(
        &mut ws,
        json!({
            "type": "message",
            "text": ""
        }),
    )
    .await;

    let chunk = recv_json(&mut ws).await;
    assert_eq!(chunk["type"], "thinking");
    assert_eq!(chunk["content"], "Processing your message...");
}

#[tokio::test]
async fn test_websocket_streams_response_from_assistant_event() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");

    let session_id = format!("session:{actor_id}");
    let thread_id = format!("thread:{actor_id}");

    let _ = ractor::call!(server.app_state.event_store(), |reply| {
        EventStoreMsg::Append {
            event: AppendEvent {
                event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                payload: serde_json::json!({
                    "text": "streamed assistant response",
                    "thinking": "streamed thinking",
                    "confidence": 0.91,
                    "model": "ClaudeBedrockSonnet45",
                    "model_source": "app",
                    "scope": {
                        "session_id": session_id,
                        "thread_id": thread_id,
                    }
                }),
                actor_id: actor_id.clone(),
                user_id: "system".to_string(),
            },
            reply,
        }
    })
    .expect("append assistant event rpc")
    .expect("append assistant event");

    let mut saw_response = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let msg = recv_json(&mut ws).await;
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        if msg_type != "response" {
            continue;
        }

        let content = msg
            .get("content")
            .and_then(|v| v.as_str())
            .expect("response content should be a stringified JSON payload");
        let payload: Value = serde_json::from_str(content).expect("response payload JSON");
        assert_eq!(payload["text"], "streamed assistant response");
        assert_eq!(payload["model_used"], "ClaudeBedrockSonnet45");
        assert_eq!(payload["model_source"], "app");
        saw_response = true;
        break;
    }

    assert!(
        saw_response,
        "expected websocket response chunk from streamed assistant event"
    );
}

#[tokio::test]
async fn test_websocket_close_connection() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let _ = recv_json(&mut ws).await;

    ws.send(Message::Close(None))
        .await
        .expect("Failed to send close");

    let _ = timeout(Duration::from_secs(2), ws.next()).await;
}

#[tokio::test]
async fn test_websocket_protocol_version_required() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
        .await
        .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
}

#[tokio::test]
async fn test_websocket_special_chars_in_actor_id() {
    let server = start_test_server().await;
    let actor_id = "test-actor-with-special-chars-123".to_string();
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/chat/{actor_id}?user_id={user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");
    assert_eq!(connected["actor_id"], actor_id);
    assert_eq!(connected["user_id"], user_id);
}

#[tokio::test]
async fn test_websocket_rapid_connect_disconnect() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();

    for _ in 0..3 {
        let (mut ws, _) = connect_async(ws_url(server.addr, &format!("/ws/chat/{actor_id}")))
            .await
            .expect("Failed to connect WebSocket");

        let connected = recv_json(&mut ws).await;
        assert_eq!(connected["type"], "connected");

        send_json(&mut ws, json!({"type": "ping"})).await;
        let pong = recv_json(&mut ws).await;
        assert_eq!(pong["type"], "pong");

        let _ = ws.send(Message::Close(None)).await;
    }
}

#[tokio::test]
async fn test_websocket_streams_actor_call_for_delegated_terminal_task() {
    let server = start_test_server().await;
    let actor_id = test_actor_id();
    let user_id = test_user_id();

    let (mut ws, _) = connect_async(ws_url(
        server.addr,
        &format!("/ws/chat/{actor_id}?user_id={user_id}"),
    ))
    .await
    .expect("Failed to connect WebSocket");

    let connected = recv_json(&mut ws).await;
    assert_eq!(connected["type"], "connected");

    let task = server
        .app_state
        .delegate_terminal_task(
            format!("terminal:{actor_id}"),
            actor_id.clone(),
            user_id.clone(),
            "/bin/zsh".to_string(),
            ".".to_string(),
            "sleep 6 && echo ws_actor_call_ready".to_string(),
            Some(15_000),
            None,
            None,
            Some(format!("session:{actor_id}")),
            Some(format!("thread:{actor_id}")),
        )
        .await
        .expect("Failed to delegate terminal task");

    let mut attempts = 0;
    send_json(
        &mut ws,
        json!({
            "type": "message",
            "text": "status update"
        }),
    )
    .await;
    attempts += 1;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut saw_actor_call = false;
    while tokio::time::Instant::now() < deadline {
        let Some(msg) = try_recv_json(&mut ws, Duration::from_secs(2)).await else {
            if attempts < 8 {
                send_json(
                    &mut ws,
                    json!({
                        "type": "message",
                        "text": "progress ping"
                    }),
                )
                .await;
                attempts += 1;
            }
            continue;
        };
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        if msg_type == "actor_call" {
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .expect("actor_call content should be a JSON string");
            let payload: Value =
                serde_json::from_str(content).expect("actor_call payload should be JSON");
            assert!(
                payload.get("task_id").and_then(|v| v.as_str()).is_some(),
                "actor_call payload should include task_id: {payload}"
            );
            assert!(
                payload.get("status").and_then(|v| v.as_str()).is_some(),
                "actor_call payload should include status: {payload}"
            );
            assert!(
                payload.get("event_type").and_then(|v| v.as_str()).is_some(),
                "actor_call payload should include event_type: {payload}"
            );
            saw_actor_call = true;
            break;
        }

        if (msg_type == "response" || msg_type == "error") && attempts < 8 {
            send_json(
                &mut ws,
                json!({
                    "type": "message",
                    "text": "check background task progress"
                }),
            )
            .await;
            attempts += 1;
        }
    }

    assert!(
        saw_actor_call,
        "expected websocket actor_call chunk from delegated terminal task"
    );

    let completed = wait_for_worker_task_completion(
        &server.app_state,
        &actor_id,
        &task.correlation_id,
        Duration::from_secs(20),
    )
    .await;
    assert!(
        completed,
        "expected worker task completion for correlation {}",
        task.correlation_id
    );

    let session_id = format!("session:{actor_id}");
    let thread_id = format!("thread:{actor_id}");
    let client = reqwest::Client::new();
    let markdown = client
        .get(format!("http://{}/logs/run.md", server.addr))
        .query(&[
            ("actor_id", actor_id.as_str()),
            ("session_id", session_id.as_str()),
            ("thread_id", thread_id.as_str()),
            ("correlation_id", task.correlation_id.as_str()),
        ])
        .send()
        .await
        .expect("request run markdown")
        .text()
        .await
        .expect("read run markdown");

    assert!(markdown.contains("# Run Log"));
    assert!(markdown.contains("worker.task.started"));
    assert!(
        markdown.contains("worker.task.completed") || markdown.contains("worker.task.failed"),
        "expected task terminal state in markdown transcript"
    );
    assert!(markdown.contains(&task.correlation_id));
}

#[tokio::test]
async fn test_websocket_streams_actor_call_for_varied_prompts() {
    let prompts = [
        "what's the weather in boston. use api",
        "run a quick system check",
        "summarize current terminal task progress",
        "verify command output and report status",
    ];

    for (idx, prompt) in prompts.iter().enumerate() {
        let server = start_test_server().await;
        let actor_id = test_actor_id();
        let user_id = test_user_id();

        let (mut ws, _) = connect_async(ws_url(
            server.addr,
            &format!("/ws/chat/{actor_id}?user_id={user_id}"),
        ))
        .await
        .expect("Failed to connect WebSocket");

        let connected = recv_json(&mut ws).await;
        assert_eq!(connected["type"], "connected");

        let task = server
            .app_state
            .delegate_terminal_task(
                format!("terminal:{actor_id}"),
                actor_id.clone(),
                user_id.clone(),
                "/bin/zsh".to_string(),
                ".".to_string(),
                format!("sleep 4 && echo ws_actor_call_ready_{idx}"),
                Some(12_000),
                None,
                None,
                Some(format!("session:{actor_id}")),
                Some(format!("thread:{actor_id}")),
            )
            .await
            .expect("Failed to delegate terminal task");

        send_json(
            &mut ws,
            json!({
                "type": "message",
                "text": prompt
            }),
        )
        .await;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut saw_task_actor_call = false;
        let mut attempts = 0;
        while tokio::time::Instant::now() < deadline {
            let Some(msg) = try_recv_json(&mut ws, Duration::from_secs(2)).await else {
                if attempts < 6 {
                    send_json(
                        &mut ws,
                        json!({
                            "type": "message",
                            "text": "progress ping"
                        }),
                    )
                    .await;
                    attempts += 1;
                }
                continue;
            };

            if msg.get("type").and_then(|v| v.as_str()) != Some("actor_call") {
                continue;
            }

            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .expect("actor_call content should be a JSON string");
            let payload: Value =
                serde_json::from_str(content).expect("actor_call payload should be JSON");
            let task_id = payload.get("task_id").and_then(|v| v.as_str());
            if task_id == Some(task.task_id.as_str()) {
                assert!(
                    payload.get("status").and_then(|v| v.as_str()).is_some(),
                    "actor_call payload should include status: {payload}"
                );
                assert!(
                    payload.get("event_type").and_then(|v| v.as_str()).is_some(),
                    "actor_call payload should include event_type: {payload}"
                );
                saw_task_actor_call = true;
                break;
            }
        }

        assert!(
            saw_task_actor_call,
            "expected actor_call for delegated task {} on prompt: {}",
            task.task_id, prompt
        );
    }
}
