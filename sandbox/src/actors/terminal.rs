//! TerminalActor - manages terminal sessions for opencode integration
//!
//! PREDICTION: Terminal sessions can be managed as actors with PTY support,
//! enabling opencode (and other tools) to spawn and interact with terminals
//! through a unified API.
//!
//! EXPERIMENT:
//! 1. TerminalActor spawns PTY processes (bash, zsh, etc.)
//! 2. Input/output streamed via WebSocket or actor messages
//! 3. Sessions persist and can be reattached
//! 4. Multiple terminals per user, managed by DesktopActor windows
//!
//! OBSERVE:
//! - PTY processes survive actor restarts (via persistence)
//! - Output streaming works for long-running commands
//! - Session reattachment works across reconnections
//! - Integration with opencode CLI

use async_trait::async_trait;
use portable_pty::{CommandBuilder, PtySize};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::HashMap;
use std::io::{Read, Write};
use tokio::sync::mpsc;

use crate::actors::event_store::EventStoreMsg;

/// Actor that manages terminal sessions
#[derive(Debug, Default)]
pub struct TerminalActor;

/// Arguments for spawning TerminalActor
#[derive(Debug, Clone)]
pub struct TerminalArguments {
    pub terminal_id: String,
    pub user_id: String,
    pub shell: String, // e.g., "/bin/bash" or "/bin/zsh"
    pub working_dir: String,
    pub event_store: ActorRef<EventStoreMsg>,
}

/// State for TerminalActor
pub struct TerminalState {
    terminal_id: String,
    user_id: String,
    shell: String,
    working_dir: String,
    /// PTY master handle (for I/O and resize)
    #[allow(dead_code)]
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    /// Channel for sending input to PTY
    input_tx: Option<mpsc::Sender<String>>,
    /// Channel for receiving output from PTY
    output_rx: Option<mpsc::Receiver<String>>,
    /// Buffer of recent output (for new connections)
    output_buffer: Vec<String>,
    /// Whether terminal is running
    is_running: bool,
    /// Exit code when process ends
    exit_code: Option<i32>,
    /// Environment variables
    env_vars: HashMap<String, String>,
    /// Terminal dimensions
    rows: u16,
    cols: u16,
    event_store: ActorRef<EventStoreMsg>,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by TerminalActor
#[derive(Debug)]
pub enum TerminalMsg {
    /// Start the terminal session (spawn PTY)
    Start {
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Send input to terminal (keyboard input)
    SendInput {
        input: String,
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Get recent output (for new connections)
    GetOutput {
        reply: RpcReplyPort<Vec<String>>,
    },
    /// Subscribe to output stream (returns channel receiver)
    SubscribeOutput {
        reply: RpcReplyPort<mpsc::Receiver<String>>,
    },
    /// Resize terminal (rows, cols)
    Resize {
        rows: u16,
        cols: u16,
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Get terminal info
    GetInfo {
        reply: RpcReplyPort<TerminalInfo>,
    },
    /// Stop/kill terminal
    Stop {
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Internal: output received from PTY
    OutputReceived {
        data: String,
    },
    /// Internal: process exited
    ProcessExited {
        exit_code: Option<i32>,
    },
}

/// Terminal information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalInfo {
    pub terminal_id: String,
    pub user_id: String,
    pub shell: String,
    pub working_dir: String,
    pub is_running: bool,
    pub exit_code: Option<i32>,
    pub rows: u16,
    pub cols: u16,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error, Clone)]
pub enum TerminalError {
    #[error("Terminal not running")]
    NotRunning,

    #[error("Terminal already running")]
    AlreadyRunning,

    #[error("Failed to spawn PTY: {0}")]
    SpawnFailed(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("PTY not supported on this platform")]
    PtyNotSupported,
}

impl From<std::io::Error> for TerminalError {
    fn from(e: std::io::Error) -> Self {
        TerminalError::Io(e.to_string())
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

#[async_trait]
impl Actor for TerminalActor {
    type Msg = TerminalMsg;
    type State = TerminalState;
    type Arguments = TerminalArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(TerminalState {
            terminal_id: args.terminal_id,
            user_id: args.user_id,
            shell: args.shell,
            working_dir: args.working_dir,
            pty_master: None,
            input_tx: None,
            output_rx: None,
            output_buffer: Vec::with_capacity(1000), // Keep last 1000 lines
            is_running: false,
            exit_code: None,
            env_vars: HashMap::new(),
            rows: 24,
            cols: 80,
            event_store: args.event_store,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TerminalMsg::Start { reply } => {
                if state.is_running {
                    let _ = reply.send(Err(TerminalError::AlreadyRunning));
                    return Ok(());
                }

                match spawn_pty(
                    &state.shell,
                    &state.working_dir,
                    state.rows,
                    state.cols,
                    myself.clone(),
                )
                .await
                {
                    Ok((pty_master, input_tx, output_rx)) => {
                        state.pty_master = Some(pty_master);
                        state.input_tx = Some(input_tx);
                        state.output_rx = Some(output_rx);
                        state.is_running = true;
                        state.exit_code = None;
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }

            TerminalMsg::SendInput { input, reply } => {
                if !state.is_running {
                    let _ = reply.send(Err(TerminalError::NotRunning));
                    return Ok(());
                }

                if let Some(ref tx) = state.input_tx {
                    match tx.send(input).await {
                        Ok(_) => {
                            let _ = reply.send(Ok(()));
                        }
                        Err(_) => {
                            let _ = reply.send(Err(TerminalError::Io(
                                "Failed to send input".to_string(),
                            )));
                        }
                    }
                } else {
                    let _ = reply.send(Err(TerminalError::NotRunning));
                }
            }

            TerminalMsg::GetOutput { reply } => {
                let _ = reply.send(state.output_buffer.clone());
            }

            TerminalMsg::SubscribeOutput { reply } => {
                // Create a new channel for this subscriber
                let (_tx, rx) = mpsc::channel::<String>(1000);

                // TODO: Implement proper broadcast using tokio::sync::broadcast
                // For now, return an empty channel
                let _ = reply.send(rx);
            }

            TerminalMsg::Resize { rows, cols, reply } => {
                state.rows = rows;
                state.cols = cols;

                if let Some(ref mut pty_master) = state.pty_master {
                    match pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        Ok(_) => {
                            let _ = reply.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = reply.send(Err(TerminalError::Io(e.to_string())));
                        }
                    }
                } else {
                    let _ = reply.send(Ok(())); // Not running yet, just store dimensions
                }
            }

            TerminalMsg::GetInfo { reply } => {
                let info = TerminalInfo {
                    terminal_id: state.terminal_id.clone(),
                    user_id: state.user_id.clone(),
                    shell: state.shell.clone(),
                    working_dir: state.working_dir.clone(),
                    is_running: state.is_running,
                    exit_code: state.exit_code,
                    rows: state.rows,
                    cols: state.cols,
                };
                let _ = reply.send(info);
            }

            TerminalMsg::Stop { reply } => {
                state.pty_master = None;
                state.is_running = false;
                state.input_tx = None;
                state.output_rx = None;
                let _ = reply.send(Ok(()));
            }

            TerminalMsg::OutputReceived { data } => {
                // Add to buffer, keeping only last 1000 lines
                state.output_buffer.push(data.clone());
                if state.output_buffer.len() > 1000 {
                    state.output_buffer.remove(0);
                }

                // TODO: Emit event to EventStore
                let _ = data;
            }

            TerminalMsg::ProcessExited { exit_code } => {
                state.is_running = false;
                state.exit_code = exit_code;
                state.pty_master = None;
                state.input_tx = None;
                state.output_rx = None;
                // TODO: Emit event to EventStore
            }
        }

        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Clean up PTY if still running
        state.pty_master = None;
        Ok(())
    }
}

// ============================================================================
// PTY Implementation
// ============================================================================

/// Spawn a PTY process and return handles
async fn spawn_pty(
    shell: &str,
    working_dir: &str,
    rows: u16,
    cols: u16,
    actor_ref: ActorRef<TerminalMsg>,
) -> Result<
    (
        Box<dyn portable_pty::MasterPty + Send>,
        mpsc::Sender<String>,
        mpsc::Receiver<String>,
    ),
    TerminalError,
> {
    // Select the appropriate PTY system for the platform
    let pty_system = portable_pty::native_pty_system();

    // Open a PTY
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;

    // Build the command
    let mut cmd_builder = CommandBuilder::new(shell);
    cmd_builder.cwd(std::path::Path::new(working_dir));

    // Spawn the shell in the PTY
    let mut child = pair
        .slave
        .spawn_command(cmd_builder)
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;

    // Create channels for communication
    let (input_tx, mut input_rx) = mpsc::channel::<String>(100);
    let (output_tx, output_rx) = mpsc::channel::<String>(1000);

    // Get handles for I/O
    let mut master_writer = pair.master.take_writer().map_err(|e| {
        TerminalError::SpawnFailed(format!("Failed to get PTY writer: {}", e))
    })?;

    let mut master_reader = pair.master.try_clone_reader().map_err(|e| {
        TerminalError::SpawnFailed(format!("Failed to clone PTY reader: {}", e))
    })?;

    // Spawn input task: read from channel, write to PTY (blocking I/O in spawn_blocking)
    tokio::task::spawn_blocking(move || {
        while let Some(input) = input_rx.blocking_recv() {
            if master_writer.write_all(input.as_bytes()).is_err() {
                break;
            }
            if master_writer.flush().is_err() {
                break;
            }
        }
    });

    // Spawn output task: read from PTY, send to actor and subscribers
    let actor = actor_ref.clone();
    let output_tx_clone = output_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match master_reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF - process exited
                    let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
                    break;
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();

                    // Send to actor for buffering
                    let _ = actor.send_message(TerminalMsg::OutputReceived {
                        data: data.clone(),
                    });

                    // Send to subscribers - use blocking_send since we're in spawn_blocking
                    let _ = output_tx_clone.blocking_send(data);
                }
                Err(_) => {
                    // Read error
                    let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
                    break;
                }
            }
        }
    });

    // Spawn exit monitor task
    let actor = actor_ref.clone();
    tokio::task::spawn_blocking(move || {
        // Wait for the child process to exit
        match child.wait() {
            Ok(exit_status) => {
                let exit_code = Some(exit_status.exit_code() as i32);
                let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code });
            }
            Err(_) => {
                let _ = actor.send_message(TerminalMsg::ProcessExited { exit_code: None });
            }
        }
    });

    Ok((pair.master, input_tx, output_rx))
}
