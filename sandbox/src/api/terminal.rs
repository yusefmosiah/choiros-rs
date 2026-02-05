//! Terminal WebSocket handler - streams terminal I/O via WebSocket
//!
//! This module provides WebSocket endpoints for terminal sessions,
//! enabling real-time bidirectional communication between the browser
//! and PTY processes.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::actor_manager::AppState;
use crate::actors::terminal::{TerminalArguments, TerminalMsg};
use crate::api::ApiState;

/// WebSocket message types for terminal communication
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerminalWsMessage {
    /// Input from client (keyboard)
    #[serde(rename = "input")]
    Input { data: String },
    /// Output to client (terminal data)
    #[serde(rename = "output")]
    Output { data: String },
    /// Resize terminal
    #[serde(rename = "resize")]
    Resize { rows: u16, cols: u16 },
    /// Terminal info
    #[serde(rename = "info")]
    Info { terminal_id: String, is_running: bool },
    /// Error message
    #[serde(rename = "error")]
    Error { message: String },
}

/// Query parameters for terminal WebSocket connection
#[derive(Debug, Deserialize, Clone)]
pub struct TerminalWsQuery {
    user_id: String,
    #[serde(default = "default_shell")]
    shell: String,
    #[serde(default = "default_working_dir")]
    working_dir: String,
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

fn default_working_dir() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/".to_string())
}

/// WebSocket handler for terminal sessions
pub async fn terminal_websocket(
    ws: WebSocketUpgrade,
    Path(terminal_id): Path<String>,
    Query(query): Query<TerminalWsQuery>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let app_state = state.app_state.clone();
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, app_state, terminal_id, query))
}

async fn handle_terminal_socket(
    socket: WebSocket,
    app_state: std::sync::Arc<AppState>,
    terminal_id: String,
    query: TerminalWsQuery,
) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let actor_manager = app_state.actor_manager.clone();
    let event_store = actor_manager.event_store();

    let terminal_actor = match actor_manager
        .get_or_create_terminal(
            &terminal_id,
            TerminalArguments {
                terminal_id: terminal_id.clone(),
                user_id: query.user_id.clone(),
                shell: query.shell.clone(),
                working_dir: query.working_dir.clone(),
                event_store,
            },
        )
        .await
    {
        Ok(actor) => actor,
        Err(e) => {
            let _ = send_terminal_message(
                &tx,
                TerminalWsMessage::Error {
                    message: format!("Failed to create terminal: {e}"),
                },
            );
            writer.abort();
            return;
        }
    };

    match ractor::call!(terminal_actor, |reply| TerminalMsg::Start { reply }) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = send_terminal_message(
                &tx,
                TerminalWsMessage::Error {
                    message: format!("Failed to start terminal: {e}"),
                },
            );
            writer.abort();
            return;
        }
        Err(e) => {
            let _ = send_terminal_message(
                &tx,
                TerminalWsMessage::Error {
                    message: format!("Failed to start terminal: {e:?}"),
                },
            );
            writer.abort();
            return;
        }
    }

    let output_rx = match ractor::call!(
        terminal_actor,
        |reply| TerminalMsg::SubscribeOutput { reply }
    ) {
        Ok(rx) => rx,
        Err(e) => {
            let _ = send_terminal_message(
                &tx,
                TerminalWsMessage::Error {
                    message: format!("Failed to subscribe to output: {e:?}"),
                },
            );
            writer.abort();
            return;
        }
    };

    if let Ok(buffer) = ractor::call!(terminal_actor, |reply| TerminalMsg::GetOutput { reply }) {
        for data in buffer {
            let _ = send_terminal_message(&tx, TerminalWsMessage::Output { data });
        }
    }

    let info = match ractor::call!(terminal_actor, |reply| TerminalMsg::GetInfo { reply }) {
        Ok(info) => info,
        Err(_) => {
            writer.abort();
            return;
        }
    };

    let _ = send_terminal_message(
        &tx,
        TerminalWsMessage::Info {
            terminal_id: info.terminal_id,
            is_running: info.is_running,
        },
    );

    let mut output_rx = output_rx;
    let tx_clone = tx.clone();
    let forward_task = tokio::spawn(async move {
        loop {
            match output_rx.recv().await {
                Ok(data) => {
                    let _ = send_terminal_message(&tx_clone, TerminalWsMessage::Output { data });
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => match serde_json::from_str::<TerminalWsMessage>(&text) {
                Ok(TerminalWsMessage::Input { data }) => {
                    let _ = ractor::call!(
                        terminal_actor,
                        |reply| TerminalMsg::SendInput { input: data, reply }
                    );
                }
                Ok(TerminalWsMessage::Resize { rows, cols }) => {
                    let _ = ractor::call!(
                        terminal_actor,
                        |reply| TerminalMsg::Resize { rows, cols, reply }
                    );
                }
                _ => {
                    let _ = send_terminal_message(
                        &tx,
                        TerminalWsMessage::Error {
                            message: "Unknown message type".to_string(),
                        },
                    );
                }
            },
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    forward_task.abort();
    writer.abort();
}

fn send_terminal_message(tx: &mpsc::UnboundedSender<Message>, msg: TerminalWsMessage) -> bool {
    match serde_json::to_string(&msg) {
        Ok(text) => tx.send(Message::Text(text.into())).is_ok(),
        Err(e) => {
            tracing::error!("Failed to serialize terminal WS message: {}", e);
            false
        }
    }
}

/// HTTP handler to create a new terminal session
pub async fn create_terminal(
    State(state): State<ApiState>,
    Path(terminal_id): Path<String>,
) -> impl IntoResponse {
    let actor_manager = &state.app_state.actor_manager;
    let event_store = actor_manager.event_store();

    let args = TerminalArguments {
        terminal_id: terminal_id.clone(),
        user_id: "anonymous".to_string(),
        shell: default_shell(),
        working_dir: default_working_dir(),
        event_store,
    };

    match actor_manager.get_or_create_terminal(&terminal_id, args).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "terminal_id": terminal_id,
                "status": "created"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to create terminal: {e:?}")
            })),
        )
            .into_response(),
    }
}

/// HTTP handler to get terminal info
pub async fn get_terminal_info(
    State(state): State<ApiState>,
    Path(terminal_id): Path<String>,
) -> impl IntoResponse {
    let actor_manager = &state.app_state.actor_manager;
    let event_store = actor_manager.event_store();

    let args = TerminalArguments {
        terminal_id: terminal_id.clone(),
        user_id: "anonymous".to_string(),
        shell: default_shell(),
        working_dir: default_working_dir(),
        event_store,
    };

    let terminal_actor = match actor_manager.get_or_create_terminal(&terminal_id, args).await {
        Ok(actor) => actor,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to get terminal: {e:?}")
                })),
            )
                .into_response();
        }
    };

    match ractor::call!(terminal_actor, |reply| TerminalMsg::GetInfo { reply }) {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to get info: {e:?}")
            })),
        )
            .into_response(),
    }
}

/// HTTP handler to stop a terminal session
pub async fn stop_terminal(
    State(state): State<ApiState>,
    Path(terminal_id): Path<String>,
) -> impl IntoResponse {
    let actor_manager = &state.app_state.actor_manager;
    let event_store = actor_manager.event_store();

    let args = TerminalArguments {
        terminal_id: terminal_id.clone(),
        user_id: "anonymous".to_string(),
        shell: default_shell(),
        working_dir: default_working_dir(),
        event_store,
    };

    let terminal_actor = match actor_manager.get_or_create_terminal(&terminal_id, args).await {
        Ok(actor) => actor,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to get terminal: {e:?}")
                })),
            )
                .into_response();
        }
    };

    match ractor::call!(terminal_actor, |reply| TerminalMsg::Stop { reply }) {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "terminal_id": terminal_id,
                "status": "stopped"
            })),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to stop terminal: {e:?}")
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to stop terminal: {e:?}")
            })),
        )
            .into_response(),
    }
}
