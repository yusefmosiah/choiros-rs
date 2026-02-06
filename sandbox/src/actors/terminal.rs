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
use portable_pty::{ChildKiller, CommandBuilder, PtySize};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::io::{Read, Write};
use tokio::sync::{broadcast, mpsc};

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
    /// Handle used to terminate spawned child process on stop
    child_killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    /// Channel for sending input to PTY
    input_tx: Option<mpsc::Sender<String>>,
    /// Broadcast channel for output from PTY
    output_tx: Option<broadcast::Sender<String>>,
    /// Buffer of recent output (for new connections)
    output_buffer: Vec<String>,
    /// Whether terminal is running
    is_running: bool,
    /// Exit code when process ends
    exit_code: Option<i32>,
    /// PID of the spawned shell process (if available)
    process_id: Option<u32>,
    /// Terminal dimensions
    rows: u16,
    cols: u16,
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
    GetOutput { reply: RpcReplyPort<Vec<String>> },
    /// Subscribe to output stream (returns channel receiver)
    SubscribeOutput {
        reply: RpcReplyPort<broadcast::Receiver<String>>,
    },
    /// Resize terminal (rows, cols)
    Resize {
        rows: u16,
        cols: u16,
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Get terminal info
    GetInfo { reply: RpcReplyPort<TerminalInfo> },
    /// Stop/kill terminal
    Stop {
        reply: RpcReplyPort<Result<(), TerminalError>>,
    },
    /// Internal: output received from PTY
    OutputReceived { data: String },
    /// Internal: process exited
    ProcessExited { exit_code: Option<i32> },
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
    pub process_id: Option<u32>,
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
            child_killer: None,
            input_tx: None,
            output_tx: None,
            output_buffer: Vec::with_capacity(1000), // Keep last 1000 lines
            is_running: false,
            exit_code: None,
            process_id: None,
            rows: 24,
            cols: 80,
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
                    Ok((pty_master, child_killer, input_tx, output_tx, process_id)) => {
                        state.pty_master = Some(pty_master);
                        state.child_killer = Some(child_killer);
                        state.input_tx = Some(input_tx);
                        state.output_tx = Some(output_tx);
                        state.is_running = true;
                        state.exit_code = None;
                        state.process_id = process_id;
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
                            let _ = reply
                                .send(Err(TerminalError::Io("Failed to send input".to_string())));
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
                if let Some(ref tx) = state.output_tx {
                    let rx = tx.subscribe();
                    let _ = reply.send(rx);
                } else {
                    // Not running yet; return a closed channel so callers can handle end-of-stream.
                    let (tx, rx) = broadcast::channel::<String>(1);
                    drop(tx);
                    let _ = reply.send(rx);
                }
            }

            TerminalMsg::Resize { rows, cols, reply } => {
                // Ignore pathological 0x0 resizes from transient client layout states.
                // Shared terminal sessions can be viewed by multiple browsers, so one
                // bad resize should not poison the PTY size for everyone.
                let rows = rows.max(2);
                let cols = cols.max(2);
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
                    process_id: state.process_id,
                    rows: state.rows,
                    cols: state.cols,
                };
                let _ = reply.send(info);
            }

            TerminalMsg::Stop { reply } => {
                if let Some(mut child_killer) = state.child_killer.take() {
                    if let Err(e) = child_killer.kill() {
                        tracing::warn!(
                            terminal_id = %state.terminal_id,
                            error = %e,
                            "Failed to kill terminal child process"
                        );
                    }
                }
                state.pty_master = None;
                state.is_running = false;
                state.input_tx = None;
                state.output_tx = None;
                state.process_id = None;
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
                state.child_killer = None;
                state.input_tx = None;
                state.output_tx = None;
                state.process_id = None;
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
        if let Some(mut child_killer) = state.child_killer.take() {
            let _ = child_killer.kill();
        }
        state.pty_master = None;
        state.input_tx = None;
        state.output_tx = None;
        state.process_id = None;
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
        Box<dyn ChildKiller + Send + Sync>,
        mpsc::Sender<String>,
        broadcast::Sender<String>,
        Option<u32>,
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
    let child_killer = child.clone_killer();
    let process_id = child.process_id();

    // Create channels for communication
    let (input_tx, mut input_rx) = mpsc::channel::<String>(100);
    let (output_tx, _output_rx) = broadcast::channel::<String>(1000);

    // Get handles for I/O
    let mut master_writer = pair
        .master
        .take_writer()
        .map_err(|e| TerminalError::SpawnFailed(format!("Failed to get PTY writer: {}", e)))?;

    let mut master_reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| TerminalError::SpawnFailed(format!("Failed to clone PTY reader: {}", e)))?;

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
                    let _ = actor.send_message(TerminalMsg::OutputReceived { data: data.clone() });

                    // Send to subscribers. Ignore errors if there are no listeners.
                    let _ = output_tx_clone.send(data);
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

    Ok((pair.master, child_killer, input_tx, output_tx, process_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;
    use tokio::time::{sleep, timeout, Duration, Instant};

    fn test_shell() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }

    fn test_working_dir() -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/".to_string())
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output();

        match output {
            Ok(output) => {
                output.status.success()
                    && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
            }
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    async fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !process_exists(pid) {
                return true;
            }
            sleep(Duration::from_millis(50)).await;
        }
        !process_exists(pid)
    }

    async fn wait_for_output(
        rx: &mut broadcast::Receiver<String>,
        needle: &str,
        timeout_duration: Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, rx.recv()).await {
                Ok(Ok(chunk)) => {
                    if chunk.contains(needle) {
                        return true;
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => return false,
                Err(_) => return false,
            }
        }
        false
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_stop_terminates_terminal_process() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-stop".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let info_before_stop = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info call failed");
        assert!(info_before_stop.is_running);
        let pid = info_before_stop
            .process_id
            .expect("terminal start should provide a process id");
        assert!(
            process_exists(pid),
            "expected process {pid} to exist after start"
        );

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        let exited = wait_for_process_exit(pid, Duration::from_secs(3)).await;
        assert!(exited, "terminal process {pid} still alive after stop");

        let info_after_stop = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info call failed");
        assert!(!info_after_stop.is_running);
        assert!(info_after_stop.process_id.is_none());

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_repeated_start_stop_cleans_up_each_process() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-restart".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        for _ in 0..5 {
            let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
                .expect("start call failed");
            assert!(
                start_result.is_ok(),
                "terminal failed to start: {start_result:?}"
            );

            let info = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
                .expect("get info call failed");
            let pid = info
                .process_id
                .expect("terminal start should provide a process id");
            assert!(
                process_exists(pid),
                "expected process {pid} to exist after start"
            );

            let stop_result = ractor::call!(terminal, |reply| TerminalMsg::Stop { reply })
                .expect("stop call failed");
            assert!(
                stop_result.is_ok(),
                "terminal failed to stop: {stop_result:?}"
            );

            let exited = wait_for_process_exit(pid, Duration::from_secs(3)).await;
            assert!(exited, "terminal process {pid} still alive after stop");
        }

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_terminal_output() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-multisub".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let mut rx_1 = ractor::call!(terminal, |reply| TerminalMsg::SubscribeOutput { reply })
            .expect("subscribe output #1 failed");
        let mut rx_2 = ractor::call!(terminal, |reply| TerminalMsg::SubscribeOutput { reply })
            .expect("subscribe output #2 failed");

        let marker = format!(
            "CHOIR_TERM_MULTI_SUB_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time error")
                .as_millis()
        );
        let command = format!("echo {marker}\n");

        let send_result = ractor::call!(terminal, |reply| TerminalMsg::SendInput {
            input: command,
            reply
        })
        .expect("send input call failed");
        assert!(
            send_result.is_ok(),
            "send input failed: {send_result:?}"
        );

        let got_1 = wait_for_output(&mut rx_1, &marker, Duration::from_secs(3)).await;
        let got_2 = wait_for_output(&mut rx_2, &marker, Duration::from_secs(3)).await;
        assert!(got_1, "first subscriber did not receive marker output");
        assert!(got_2, "second subscriber did not receive marker output");

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        terminal.stop(None);
        event_store.stop(None);
    }

    #[tokio::test]
    async fn test_resize_clamps_zero_dimensions() {
        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to start event store");

        let (terminal, _terminal_handle) = Actor::spawn(
            None,
            TerminalActor,
            TerminalArguments {
                terminal_id: "test-terminal-resize-clamp".to_string(),
                user_id: "test-user".to_string(),
                shell: test_shell(),
                working_dir: test_working_dir(),
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("failed to start terminal actor");

        let start_result = ractor::call!(terminal, |reply| TerminalMsg::Start { reply })
            .expect("start call failed");
        assert!(
            start_result.is_ok(),
            "terminal failed to start: {start_result:?}"
        );

        let resize_result = ractor::call!(terminal, |reply| TerminalMsg::Resize {
            rows: 0,
            cols: 0,
            reply
        })
        .expect("resize call failed");
        assert!(
            resize_result.is_ok(),
            "resize failed: {resize_result:?}"
        );

        let info = ractor::call!(terminal, |reply| TerminalMsg::GetInfo { reply })
            .expect("get info failed");
        assert!(
            info.rows >= 2 && info.cols >= 2,
            "expected clamped terminal size >= 2x2, got {}x{}",
            info.rows,
            info.cols
        );

        let stop_result =
            ractor::call!(terminal, |reply| TerminalMsg::Stop { reply }).expect("stop call failed");
        assert!(
            stop_result.is_ok(),
            "terminal failed to stop: {stop_result:?}"
        );

        terminal.stop(None);
        event_store.stop(None);
    }
}
