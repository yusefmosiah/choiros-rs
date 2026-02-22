//! Compatibility endpoints for legacy Dioxus dev clients.
//!
//! Some clients still probe `/_dioxus` for HMR websocket updates.
//! In production/static mode we do not emit HMR events, but we keep
//! the socket open to avoid reconnect/reload loops caused by 404s.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};

pub async fn hmr_websocket(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_hmr_socket)
}

async fn handle_hmr_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

    // Tell the client the socket is alive; no hot-reload events are sent.
    if sender
        .send(Message::Text(
            r#"{"type":"connected","mode":"static","hmr":false}"#.to_string().into(),
        ))
        .await
        .is_err()
    {
        return;
    }

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Ping(data) => {
                if sender.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Message::Text(_) | Message::Binary(_) | Message::Pong(_) => {}
            Message::Close(_) => break,
        }
    }
}
