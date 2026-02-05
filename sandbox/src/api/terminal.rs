//! Terminal WebSocket handler - streams terminal I/O via WebSocket
//!
//! This module provides WebSocket endpoints for terminal sessions,
//! enabling real-time bidirectional communication between the browser
//! and PTY processes.

use actix_web::{get, web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::actor_manager::AppState;
use crate::actors::terminal::{TerminalArguments, TerminalMsg};

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
#[derive(Debug, Deserialize)]
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
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    query: web::Query<TerminalWsQuery>,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    let terminal_id = path.into_inner();
    let user_id = query.user_id.clone();
    let shell = query.shell.clone();
    let working_dir = query.working_dir.clone();
    let actor_manager = app_state.actor_manager.clone();

    // Spawn WebSocket handler task
    actix_web::rt::spawn(async move {
        // Get or create terminal actor
        let event_store = actor_manager.event_store();
        let terminal_actor = match actor_manager
            .get_or_create_terminal(
                &terminal_id,
                TerminalArguments {
                    terminal_id: terminal_id.clone(),
                    user_id: user_id.clone(),
                    shell: shell.clone(),
                    working_dir: working_dir.clone(),
                    event_store,
                },
            )
            .await
        {
            Ok(actor) => actor,
            Err(e) => {
                let error_msg = TerminalWsMessage::Error {
                    message: format!("Failed to create terminal: {}", e),
                };
                let _ = session
                    .text(serde_json::to_string(&error_msg).unwrap_or_default())
                    .await;
                return;
            }
        };

        // Start the terminal if not already running
        let start_result = ractor::call!(
            terminal_actor,
            |reply| TerminalMsg::Start { reply }
        );

        if let Err(e) = start_result {
            let error_msg = TerminalWsMessage::Error {
                message: format!("Failed to start terminal: {:?}", e),
            };
            let _ = session
                .text(serde_json::to_string(&error_msg).unwrap_or_default())
                .await;
            return;
        }

        // Subscribe to output
        let output_rx = match ractor::call!(
            terminal_actor,
            |reply| TerminalMsg::SubscribeOutput { reply }
        ) {
            Ok(rx) => rx,
            Err(e) => {
                let error_msg = TerminalWsMessage::Error {
                    message: format!("Failed to subscribe to output: {:?}", e),
                };
                let _ = session
                    .text(serde_json::to_string(&error_msg).unwrap_or_default())
                    .await;
                return;
            }
        };

        // Send buffered output to new client (best-effort)
        if let Ok(buffer) = ractor::call!(terminal_actor, |reply| TerminalMsg::GetOutput { reply })
        {
            for data in buffer {
                let output_msg = TerminalWsMessage::Output { data };
                let _ = session
                    .text(serde_json::to_string(&output_msg).unwrap_or_default())
                    .await;
            }
        }

        // Get terminal info
        let info = match ractor::call!(
            terminal_actor,
            |reply| TerminalMsg::GetInfo { reply }
        ) {
            Ok(info) => info,
            Err(_) => {
                let _ = session.close(None).await;
                return;
            }
        };

        // Send info to client
        let info_msg = TerminalWsMessage::Info {
            terminal_id: info.terminal_id,
            is_running: info.is_running,
        };
        let _ = session
            .text(serde_json::to_string(&info_msg).unwrap_or_default())
            .await;

        // Spawn output forwarding task
        let mut output_rx = output_rx;
        let mut session_clone = session.clone();
        let forward_task = actix_web::rt::spawn(async move {
            loop {
                match output_rx.recv().await {
                    Ok(data) => {
                        let output_msg = TerminalWsMessage::Output { data };
                        if session_clone
                            .text(serde_json::to_string(&output_msg).unwrap_or_default())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Skip lagged messages; keep the stream alive.
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Handle incoming WebSocket messages
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                Message::Text(text) => {
                    match serde_json::from_str::<TerminalWsMessage>(&text) {
                        Ok(TerminalWsMessage::Input { data }) => {
                            // Send input to terminal
                            let _ = ractor::call!(
                                terminal_actor,
                                |reply| TerminalMsg::SendInput { input: data, reply }
                            );
                        }
                        Ok(TerminalWsMessage::Resize { rows, cols }) => {
                            // Resize terminal
                            let _ = ractor::call!(
                                terminal_actor,
                                |reply| TerminalMsg::Resize { rows, cols, reply }
                            );
                        }
                        _ => {
                            // Unknown message type
                            let error_msg = TerminalWsMessage::Error {
                                message: "Unknown message type".to_string(),
                            };
                            let _ = session
                                .text(serde_json::to_string(&error_msg).unwrap_or_default())
                                .await;
                        }
                    }
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }

        // Cancel output forwarding task
        forward_task.abort();

        // Close session
        let _ = session.close(None).await;
    });

    Ok(response)
}

/// HTTP handler to create a new terminal session
#[get("/api/terminals/{terminal_id}")]
pub async fn create_terminal(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let terminal_id = path.into_inner();
    let actor_manager = &app_state.actor_manager;
    let event_store = actor_manager.event_store();

    let args = TerminalArguments {
        terminal_id: terminal_id.clone(),
        user_id: "anonymous".to_string(), // TODO: Get from auth
        shell: default_shell(),
        working_dir: default_working_dir(),
        event_store,
    };

    match actor_manager.get_or_create_terminal(&terminal_id, args).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "terminal_id": terminal_id,
            "status": "created"
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to create terminal: {:?}", e)
        })),
    }
}

/// HTTP handler to get terminal info
#[get("/api/terminals/{terminal_id}/info")]
pub async fn get_terminal_info(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let terminal_id = path.into_inner();
    let actor_manager = &app_state.actor_manager;

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
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to get terminal: {:?}", e)
            }));
        }
    };

    match ractor::call!(
        terminal_actor,
        |reply| TerminalMsg::GetInfo { reply }
    ) {
        Ok(info) => HttpResponse::Ok().json(info),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to get info: {:?}", e)
        })),
    }
}

/// HTTP handler to stop a terminal session
#[get("/api/terminals/{terminal_id}/stop")]
pub async fn stop_terminal(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let terminal_id = path.into_inner();
    let actor_manager = &app_state.actor_manager;

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
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to get terminal: {:?}", e)
            }));
        }
    };

    match ractor::call!(
        terminal_actor,
        |reply| TerminalMsg::Stop { reply }
    ) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "terminal_id": terminal_id,
            "status": "stopped"
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to stop terminal: {:?}", e)
        })),
    }
}
