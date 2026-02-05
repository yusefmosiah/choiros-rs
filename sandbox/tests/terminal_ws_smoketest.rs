//! Terminal WebSocket Smoke Test
//!
//! Verifies that a terminal WebSocket connection can be established
//! and that basic input produces output.

use actix_http::ws::{Frame, ProtocolError};
use actix_web::{web, App, Error};
use futures::{SinkExt, StreamExt};
use ractor::Actor;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::time::timeout;

use sandbox::actor_manager::AppState;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::api;

fn test_terminal_id() -> String {
    format!("test-terminal-{}", uuid::Uuid::new_v4())
}

fn test_user_id() -> String {
    format!("test-user-{}", uuid::Uuid::new_v4())
}

async fn recv_json(
    ws: &mut (impl StreamExt<Item = Result<Frame, ProtocolError>> + Unpin),
    total_timeout: Duration,
) -> Result<Value, Error> {
    let start = Instant::now();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= total_timeout {
            return Err(actix_web::error::ErrorRequestTimeout(
                "Timeout waiting for frame",
            ));
        }
        let remaining = total_timeout - elapsed;

        match timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Frame::Text(bytes)))) => {
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!(
                        "Invalid UTF-8: {e}"
                    ))
                })?;
                let value: Value = serde_json::from_str(text).map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!(
                        "Invalid JSON: {e}"
                    ))
                })?;
                return Ok(value);
            }
            Ok(Some(Ok(Frame::Close(_)))) => {
                return Err(actix_web::error::ErrorBadRequest("Connection closed"))
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => {
                return Err(actix_web::error::ErrorInternalServerError(format!(
                    "Frame error: {e:?}"
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

async fn send_json(
    ws: &mut (impl SinkExt<actix_http::ws::Message, Error = ProtocolError> + Unpin),
    msg: Value,
) -> Result<(), Error> {
    let text = msg.to_string();
    ws.send(actix_http::ws::Message::Text(text.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!(
            "Send error: {e:?}"
        )))?;
    Ok(())
}

#[cfg(unix)]
#[actix_web::test]
async fn test_terminal_ws_smoke() {
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

    let app_state = web::Data::new(AppState::new(event_store));

    let mut srv = actix_test::start(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    });

    let terminal_id = test_terminal_id();
    let user_id = test_user_id();

    let mut framed = srv
        .ws_at(&format!(
            "/ws/terminal/{terminal_id}?user_id={user_id}"
        ))
        .await
        .expect("Failed to connect WebSocket");

    let info = recv_json(&mut framed, Duration::from_secs(5))
        .await
        .expect("Should receive info message");
    assert_eq!(info["type"], "info");
    assert_eq!(info["terminal_id"], terminal_id);

    send_json(
        &mut framed,
        json!({"type":"input","data":"echo hi\r"}),
    )
    .await
    .expect("Failed to send input");

    let start = Instant::now();
    let mut saw_hi = false;
    while start.elapsed() < Duration::from_secs(5) {
        let msg = recv_json(&mut framed, Duration::from_secs(5))
            .await
            .expect("Failed to receive output message");
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
