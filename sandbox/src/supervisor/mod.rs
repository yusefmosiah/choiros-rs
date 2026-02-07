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

use crate::actors::event_store::EventStoreMsg;

/// Application supervisor - root of the supervision tree
#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

/// Application supervisor state
pub struct ApplicationState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub session_supervisor: Option<ActorRef<SessionSupervisorMsg>>,
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
        user_id: String,
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
        if let SupervisionEvent::ActorTerminated(actor_cell, _, _) = event {
            if let Some(session_supervisor) = &state.session_supervisor {
                if session_supervisor.get_id() == actor_cell.get_id() {
                    state.session_supervisor = None;
                }
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

        // Spawn SessionSupervisor as a supervised child
        let session_supervisor_args = SessionSupervisorArgs {
            event_store: event_store.clone(),
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
            session_supervisor: Some(session_supervisor),
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
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
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
                        "SessionSupervisor not available"
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChat {
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
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
                        "SessionSupervisor not available"
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                user_id,
                reply,
            } => {
                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChatAgent {
                            agent_id: agent_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
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
                        "SessionSupervisor not available"
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
                                let _ = reply.send(actor_ref);
                            }
                            Err(e) => {
                                error!(
                                    terminal_id = %terminal_id,
                                    error = %e,
                                    "Failed to get or create terminal via SessionSupervisor"
                                );
                                return Err(ActorProcessingErr::from(std::io::Error::other(e)));
                            }
                        },
                        Err(e) => {
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
                        "SessionSupervisor not available"
                    )));
                }
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
