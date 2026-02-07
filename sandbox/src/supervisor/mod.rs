//! Application Supervisor - Root of the supervision tree
//!
//! This module provides the ApplicationSupervisor which is the root
//! supervisor for the entire ChoirOS actor hierarchy.
//!
//! ## Architecture
//!
//! ApplicationSupervisor (one_for_one strategy)
//! └── SessionSupervisor (one_for_one strategy)
//!     ├── DesktopSupervisor
//!     ├── ChatSupervisor
//!     └── TerminalSupervisor
//!
//! ## Supervision Events
//!
//! The supervisor handles:
//! - `ActorStarted`: New child actor started
//! - `ActorFailed`: Child actor crashed/failed
//! - `ActorTerminated`: Child actor terminated normally
//!
//! ## Feature Flag
//!
//! This module is gated by the `supervision_refactor` feature flag.

pub mod chat;
pub mod desktop;
pub mod session;
pub mod terminal;

// Re-export from session module
pub use session::{
    SessionSupervisor, SessionSupervisorArgs, SessionSupervisorMsg, SessionSupervisorState,
};

// Re-export from desktop module
pub use desktop::{
    get_desktop, get_or_create_desktop, remove_desktop, DesktopInfo, DesktopSupervisor,
    DesktopSupervisorArgs, DesktopSupervisorMsg, DesktopSupervisorState,
};

// Re-export from chat module
pub use chat::{
    get_chat, get_chat_agent, get_or_create_chat, get_or_create_chat_agent, remove_chat,
    remove_chat_agent, ChatInfo, ChatSupervisor, ChatSupervisorArgs, ChatSupervisorMsg,
    ChatSupervisorState,
};

// Re-export from terminal module
pub use terminal::{
    get_or_create_terminal, get_terminal_info, list_terminals, remove_terminal, TerminalSupervisor,
    TerminalSupervisorArgs, TerminalSupervisorMsg, TerminalSupervisorState,
};

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tracing::{error, info};

use crate::actors::event_bus::{
    Event, EventBusActor, EventBusArguments, EventBusConfig, EventBusMsg, EventType,
};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::terminal::TerminalMsg;

/// Application supervisor - root of the supervision tree
#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

/// Application supervisor state
pub struct ApplicationState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub event_bus: Option<ActorRef<EventBusMsg>>,
    pub session_supervisor: Option<ActorRef<SessionSupervisorMsg>>,
    pub supervision_event_counts: SupervisionEventCounts,
    pub last_supervision_failure: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupervisionEventCounts {
    pub actor_started: u64,
    pub actor_failed: u64,
    pub actor_terminated: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSupervisorHealth {
    pub event_bus_healthy: bool,
    pub session_supervisor_healthy: bool,
    pub supervision_event_counts: SupervisionEventCounts,
    pub last_supervision_failure: Option<String>,
}

/// Messages handled by ApplicationSupervisor
#[derive(Debug)]
pub enum ApplicationSupervisorMsg {
    /// Supervision event from child actors
    Supervision(SupervisionEvent),
    /// Get or create a desktop actor for a user
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::desktop::DesktopActorMsg>>,
    },
    /// Get or create a chat actor
    GetOrCreateChat {
        actor_id: String,
        user_id: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::chat::ChatActorMsg>>,
    },
    /// Get or create a chat agent
    GetOrCreateChatAgent {
        agent_id: String,
        chat_actor_id: String,
        user_id: String,
        preload_session_id: Option<String>,
        preload_thread_id: Option<String>,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::chat_agent::ChatAgentMsg>>,
    },
    /// Get or create a terminal session
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::terminal::TerminalMsg>>,
    },
    /// Delegate a terminal command asynchronously via TerminalActor.
    DelegateTerminalTask {
        terminal_id: String,
        actor_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        command: String,
        timeout_ms: Option<u64>,
        session_id: Option<String>,
        thread_id: Option<String>,
        reply: RpcReplyPort<Result<shared_types::DelegatedTask, String>>,
    },
    /// Return health snapshot and supervision counters.
    GetHealth {
        reply: RpcReplyPort<ApplicationSupervisorHealth>,
    },
}

#[ractor::async_trait]
impl Actor for ApplicationSupervisor {
    type Msg = ApplicationSupervisorMsg;
    type State = ApplicationState;
    type Arguments = ActorRef<EventStoreMsg>;

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "ApplicationSupervisor received supervision event"
        );
        match &event {
            SupervisionEvent::ActorStarted(_) => {
                state.supervision_event_counts.actor_started += 1;
            }
            SupervisionEvent::ActorFailed(actor_cell, failure) => {
                state.supervision_event_counts.actor_failed += 1;
                state.last_supervision_failure =
                    Some(format!("actor_id={} error={failure}", actor_cell.get_id()));
            }
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                state.supervision_event_counts.actor_terminated += 1;
                if let Some(session_supervisor) = &state.session_supervisor {
                    if session_supervisor.get_id() == actor_cell.get_id() {
                        state.session_supervisor = None;
                    }
                }
                if let Some(event_bus) = &state.event_bus {
                    if event_bus.get_id() == actor_cell.get_id() {
                        state.event_bus = None;
                    }
                }
            }
            _ => {}
        }

        if let Some(event_bus) = state.event_bus.clone() {
            let supervision_event = match Event::new(
                EventType::Custom("supervision.event".to_string()),
                "supervisor.application.supervision",
                serde_json::json!({
                    "supervisor_id": myself.get_id().to_string(),
                    "event_debug": format!("{event:?}"),
                }),
                "application_supervisor",
            ) {
                Ok(event) => event,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to build supervision event payload");
                    return Ok(());
                }
            };

            if let Err(e) = ractor::cast!(
                event_bus,
                EventBusMsg::Publish {
                    event: supervision_event,
                    persist: false,
                }
            ) {
                tracing::warn!(error = %e, "Failed to publish supervision event");
            }
        }
        Ok(())
    }

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        event_store: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(
            supervisor = %myself.get_id(),
            "ApplicationSupervisor starting"
        );

        // Spawn EventBusActor as a supervised child for pub/sub and correlation-aware tracing.
        let event_bus_args = EventBusArguments {
            event_store: Some(event_store.clone()),
            config: EventBusConfig::default(),
        };

        let (event_bus, _handle) = Actor::spawn_linked(
            None, // No fixed name - allows multiple supervisors in tests
            EventBusActor,
            event_bus_args,
            myself.get_cell(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to spawn EventBusActor: {}", e);
            ActorProcessingErr::from(e)
        })?;

        // Spawn SessionSupervisor as a supervised child
        let session_supervisor_args = SessionSupervisorArgs {
            event_store: event_store.clone(),
            application_supervisor: myself.clone(),
        };

        let (session_supervisor, _handle) = Actor::spawn_linked(
            None, // No fixed name - allows multiple supervisors in tests
            SessionSupervisor,
            session_supervisor_args,
            myself.get_cell(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to spawn SessionSupervisor: {}", e);
            ActorProcessingErr::from(e)
        })?;

        info!(
            session_supervisor = %session_supervisor.get_id(),
            "SessionSupervisor spawned as child"
        );

        Ok(ApplicationState {
            event_store,
            event_bus: Some(event_bus),
            session_supervisor: Some(session_supervisor),
            supervision_event_counts: SupervisionEventCounts::default(),
            last_supervision_failure: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ApplicationSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.desktop.get_or_create.started",
                    EventType::Custom("supervisor.desktop.get_or_create.started".to_string()),
                    serde_json::json!({
                        "desktop_id": desktop_id,
                        "user_id": user_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                );

                if let Some(ref session_supervisor) = state.session_supervisor {
                    let desktop_args = crate::actors::desktop::DesktopArguments {
                        desktop_id: desktop_id.clone(),
                        user_id: user_id.clone(),
                        event_store: state.event_store.clone(),
                    };

                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateDesktop {
                            desktop_id: desktop_id.clone(),
                            user_id: user_id.clone(),
                            args: desktop_args,
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.desktop.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.desktop.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "desktop_id": desktop_id,
                                    "user_id": user_id,
                                    "actor_id": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.desktop.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.desktop.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "desktop_id": desktop_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            error!(
                                desktop_id = %desktop_id,
                                error = %e,
                                "Failed to get or create desktop via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.chat.get_or_create.started",
                    EventType::Custom("supervisor.chat.get_or_create.started".to_string()),
                    serde_json::json!({
                        "actor_id": actor_id,
                        "user_id": user_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                );

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChat {
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.chat.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "actor_id": actor_id,
                                    "user_id": user_id,
                                    "chat_actor_ref": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.chat.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "actor_id": actor_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            error!(
                                actor_id = %actor_id,
                                error = %e,
                                "Failed to get or create chat via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                chat_actor_id,
                user_id,
                preload_session_id,
                preload_thread_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.chat_agent.get_or_create.started",
                    EventType::Custom("supervisor.chat_agent.get_or_create.started".to_string()),
                    serde_json::json!({
                        "agent_id": agent_id,
                        "chat_actor_id": chat_actor_id,
                        "user_id": user_id,
                        "preload_session_id": preload_session_id,
                        "preload_thread_id": preload_thread_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                );

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChatAgent {
                            agent_id: agent_id.clone(),
                            chat_actor_id: chat_actor_id.clone(),
                            user_id: user_id.clone(),
                            preload_session_id: preload_session_id.clone(),
                            preload_thread_id: preload_thread_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat_agent.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.chat_agent.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "agent_id": agent_id,
                                    "chat_actor_id": chat_actor_id,
                                    "user_id": user_id,
                                    "chat_agent_ref": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat_agent.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.chat_agent.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "agent_id": agent_id,
                                    "chat_actor_id": chat_actor_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            error!(
                                agent_id = %agent_id,
                                error = %e,
                                "Failed to get or create chat agent via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.terminal.get_or_create.started",
                    EventType::Custom("supervisor.terminal.get_or_create.started".to_string()),
                    serde_json::json!({
                        "terminal_id": terminal_id,
                        "user_id": user_id,
                        "shell": shell,
                        "working_dir": working_dir,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                );

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: shell.clone(),
                            working_dir: working_dir.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(result) => match result {
                            Ok(actor_ref) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.terminal.get_or_create.completed",
                                    EventType::Custom(
                                        "supervisor.terminal.get_or_create.completed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "terminal_id": terminal_id,
                                        "user_id": user_id,
                                        "terminal_ref": actor_ref.get_id().to_string(),
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                );
                                let _ = reply.send(actor_ref);
                            }
                            Err(e) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.terminal.get_or_create.failed",
                                    EventType::Custom(
                                        "supervisor.terminal.get_or_create.failed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "terminal_id": terminal_id,
                                        "user_id": user_id,
                                        "error": e,
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                );
                                error!(
                                    terminal_id = %terminal_id,
                                    error = %e,
                                    "Failed to get or create terminal via SessionSupervisor"
                                );
                                return Err(ActorProcessingErr::from(std::io::Error::other(e)));
                            }
                        },
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.terminal.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.terminal.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "terminal_id": terminal_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            );
                            error!(
                                terminal_id = %terminal_id,
                                error = %e,
                                "Failed to reach SessionSupervisor for terminal"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::DelegateTerminalTask {
                terminal_id,
                actor_id,
                user_id,
                shell,
                working_dir,
                command,
                timeout_ms,
                session_id,
                thread_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                let task_id = ulid::Ulid::new().to_string();
                let task = shared_types::DelegatedTask {
                    task_id: task_id.clone(),
                    correlation_id: correlation_id.clone(),
                    actor_id: actor_id.clone(),
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    kind: shared_types::DelegatedTaskKind::TerminalCommand,
                    payload: serde_json::json!({
                        "command": command,
                        "shell": shell,
                        "working_dir": working_dir,
                        "user_id": user_id,
                        "timeout_ms": timeout_ms,
                    }),
                };

                Self::publish_worker_event(
                    state.event_bus.clone(),
                    shared_types::EVENT_TOPIC_WORKER_TASK_STARTED,
                    EventType::WorkerSpawned,
                    serde_json::json!({
                        "task": task,
                        "status": shared_types::DelegatedTaskStatus::Accepted,
                        "started_at": chrono::Utc::now().to_rfc3339(),
                    }),
                    correlation_id.clone(),
                    actor_id.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                );

                let session_supervisor = match &state.session_supervisor {
                    Some(s) => s.clone(),
                    None => {
                        Self::publish_worker_event(
                            state.event_bus.clone(),
                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                            EventType::WorkerFailed,
                            serde_json::json!({
                                "task_id": task_id,
                                "status": shared_types::DelegatedTaskStatus::Failed,
                                "error": "SessionSupervisor not available",
                                "finished_at": chrono::Utc::now().to_rfc3339(),
                            }),
                            correlation_id,
                            actor_id.clone(),
                            session_id.clone(),
                            thread_id.clone(),
                        );
                        let _ = reply.send(Err("SessionSupervisor not available".to_string()));
                        return Ok(());
                    }
                };

                let event_bus = state.event_bus.clone();
                let task_for_reply = task.clone();
                let _ = reply.send(Ok(task_for_reply));

                tokio::spawn(async move {
                    let terminal_ref = match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: shell.clone(),
                            working_dir: working_dir.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(Ok(terminal_ref)) => terminal_ref,
                        Ok(Err(e)) => {
                            Self::publish_worker_event(
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e,
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                        Err(e) => {
                            Self::publish_worker_event(
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                    };

                    match ractor::call!(terminal_ref, |start_reply| TerminalMsg::Start {
                        reply: start_reply
                    }) {
                        Ok(Ok(()))
                        | Ok(Err(crate::actors::terminal::TerminalError::AlreadyRunning)) => {}
                        Ok(Err(e)) => {
                            Self::publish_worker_event(
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                        Err(e) => {
                            Self::publish_worker_event(
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                    }

                    Self::publish_worker_event(
                        event_bus.clone(),
                        shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                        EventType::WorkerProgress,
                        serde_json::json!({
                            "task_id": task_id,
                            "status": shared_types::DelegatedTaskStatus::Running,
                            "message": "terminal ready; dispatching command",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                        correlation_id.clone(),
                        actor_id.clone(),
                        session_id.clone(),
                        thread_id.clone(),
                    );

                    let timeout_ms = timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000);
                    Self::publish_worker_event(
                        event_bus.clone(),
                        shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                        EventType::WorkerProgress,
                        serde_json::json!({
                            "task_id": task_id,
                            "status": shared_types::DelegatedTaskStatus::Running,
                            "phase": "terminal_agent_dispatch",
                            "message": "terminal agent accepted request and is running",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                        correlation_id.clone(),
                        actor_id.clone(),
                        session_id.clone(),
                        thread_id.clone(),
                    );

                    let (result_tx, mut result_rx) = tokio::sync::oneshot::channel();
                    let terminal_ref_for_task = terminal_ref.clone();
                    let command_for_task = command.clone();
                    tokio::spawn(async move {
                        let call_result = ractor::call!(terminal_ref_for_task, |agent_reply| {
                            TerminalMsg::RunAgenticTask {
                                objective: command_for_task,
                                timeout_ms: Some(timeout_ms),
                                max_steps: Some(4),
                                reply: agent_reply,
                            }
                        });
                        let _ = result_tx.send(call_result);
                    });

                    let start_time = tokio::time::Instant::now();
                    let hard_deadline = start_time
                        + std::time::Duration::from_millis(timeout_ms.saturating_add(20_000));
                    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(2));
                    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            _ = heartbeat.tick() => {
                                let elapsed_ms = tokio::time::Instant::now()
                                    .duration_since(start_time)
                                    .as_millis() as u64;
                                Self::publish_worker_event(
                                    event_bus.clone(),
                                    shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                                    EventType::WorkerProgress,
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "status": shared_types::DelegatedTaskStatus::Running,
                                        "phase": "terminal_agent_running",
                                        "elapsed_ms": elapsed_ms,
                                        "message": "terminal agent is still running",
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                    }),
                                    correlation_id.clone(),
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );
                            }
                            _ = tokio::time::sleep_until(hard_deadline) => {
                                Self::publish_worker_event(
                                    event_bus,
                                    shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                    EventType::WorkerFailed,
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "status": shared_types::DelegatedTaskStatus::Failed,
                                        "error": format!("terminal agent did not return within {}ms", timeout_ms.saturating_add(20_000)),
                                        "finished_at": chrono::Utc::now().to_rfc3339(),
                                    }),
                                    correlation_id,
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );
                                return;
                            }
                            result = &mut result_rx => {
                                match result {
                                    Ok(Ok(Ok(result))) => {
                                        if !result.success {
                                            Self::publish_worker_event(
                                                event_bus,
                                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                EventType::WorkerFailed,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                                    "error": match result.exit_code {
                                                        Some(code) => format!("terminal command exited with status {code}"),
                                                        None => "terminal agent task failed".to_string(),
                                                    },
                                                    "output": result.summary,
                                                    "reasoning": result.reasoning,
                                                    "executed_commands": result.executed_commands,
                                                    "steps": result.steps,
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id,
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }
                                        Self::publish_worker_event(
                                            event_bus,
                                            shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED,
                                            EventType::WorkerComplete,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Completed,
                                                "output": result.summary,
                                                "reasoning": result.reasoning,
                                                "executed_commands": result.executed_commands,
                                                "steps": result.steps,
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id,
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Ok(Ok(Err(e))) => {
                                        Self::publish_worker_event(
                                            event_bus,
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": e.to_string(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id,
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Ok(Err(e)) => {
                                        Self::publish_worker_event(
                                            event_bus,
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": e.to_string(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id,
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Err(_) => {
                                        Self::publish_worker_event(
                                            event_bus,
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": "terminal agent result channel closed".to_string(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id,
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                }
                            }
                        }
                    }
                });
            }
            ApplicationSupervisorMsg::GetHealth { reply } => {
                let _ = reply.send(ApplicationSupervisorHealth {
                    event_bus_healthy: state.event_bus.is_some(),
                    session_supervisor_healthy: state.session_supervisor.is_some(),
                    supervision_event_counts: state.supervision_event_counts.clone(),
                    last_supervision_failure: state.last_supervision_failure.clone(),
                });
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "ApplicationSupervisor stopping");

        Ok(())
    }
}

impl ApplicationSupervisor {
    fn emit_request_event(
        &self,
        state: &ApplicationState,
        topic: &str,
        event_type: EventType,
        payload: serde_json::Value,
        correlation_id: String,
    ) {
        let Some(event_bus) = &state.event_bus else {
            return;
        };

        let event = match Event::new(event_type, topic, payload, "application_supervisor")
            .map(|evt| evt.with_correlation_id(correlation_id))
        {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!(error = %e, topic, "Failed to build supervisor event");
                return;
            }
        };

        if let Err(e) = ractor::cast!(
            event_bus,
            EventBusMsg::Publish {
                event,
                persist: true,
            }
        ) {
            tracing::warn!(error = %e, topic, "Failed to publish supervisor event");
        }
    }

    fn publish_worker_event(
        event_bus: Option<ActorRef<EventBusMsg>>,
        topic: &str,
        event_type: EventType,
        payload: serde_json::Value,
        correlation_id: String,
        source_actor_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) {
        let Some(event_bus) = event_bus else {
            return;
        };

        let payload_with_correlation = match payload {
            serde_json::Value::Object(mut obj) => {
                obj.insert(
                    "correlation_id".to_string(),
                    serde_json::Value::String(correlation_id.clone()),
                );
                serde_json::Value::Object(obj)
            }
            other => serde_json::json!({
                "value": other,
                "correlation_id": correlation_id,
            }),
        };
        let event_payload =
            shared_types::with_scope(payload_with_correlation, session_id, thread_id);
        let event = match Event::new(event_type, topic, event_payload, source_actor_id)
            .map(|evt| evt.with_correlation_id(correlation_id))
        {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!(error = %e, topic, "Failed to build worker event");
                return;
            }
        };

        if let Err(e) = ractor::cast!(
            event_bus,
            EventBusMsg::Publish {
                event,
                persist: true,
            }
        ) {
            tracing::warn!(error = %e, topic, "Failed to publish worker event");
        }
    }
}
