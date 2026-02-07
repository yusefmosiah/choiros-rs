//! Session Supervisor - manages domain supervisors

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tracing::{error, info};

use crate::actors::chat::ChatActorMsg;
use crate::actors::chat_agent::ChatAgentMsg;
use crate::actors::desktop::{DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::terminal::TerminalMsg;
use crate::supervisor::chat::{ChatSupervisor, ChatSupervisorArgs, ChatSupervisorMsg};
use crate::supervisor::desktop::{DesktopSupervisor, DesktopSupervisorArgs, DesktopSupervisorMsg};
use crate::supervisor::terminal::{
    TerminalSupervisor, TerminalSupervisorArgs, TerminalSupervisorMsg,
};

#[derive(Debug, Default)]
pub struct SessionSupervisor;

#[derive(Debug, Clone)]
pub struct SessionSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
}

pub struct SessionSupervisorState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub desktop_supervisor: Option<ActorRef<DesktopSupervisorMsg>>,
    pub chat_supervisor: Option<ActorRef<ChatSupervisorMsg>>,
    pub terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
}

#[derive(Debug)]
pub enum SessionSupervisorMsg {
    Supervision(SupervisionEvent),
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        args: DesktopArguments,
        reply: RpcReplyPort<ActorRef<DesktopActorMsg>>,
    },
    GetOrCreateChat {
        actor_id: String,
        user_id: String,
        reply: RpcReplyPort<ActorRef<ChatActorMsg>>,
    },
    GetOrCreateChatAgent {
        agent_id: String,
        chat_actor_id: String,
        user_id: String,
        preload_session_id: Option<String>,
        preload_thread_id: Option<String>,
        reply: RpcReplyPort<ActorRef<ChatAgentMsg>>,
    },
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<Result<ActorRef<TerminalMsg>, String>>,
    },
}

#[ractor::async_trait]
impl Actor for SessionSupervisor {
    type Msg = SessionSupervisorMsg;
    type State = SessionSupervisorState;
    type Arguments = SessionSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "SessionSupervisor starting");

        let (desktop_supervisor, _) = Actor::spawn_linked(
            None,
            DesktopSupervisor,
            DesktopSupervisorArgs {
                event_store: args.event_store.clone(),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        let (chat_supervisor, _) = Actor::spawn_linked(
            None,
            ChatSupervisor,
            ChatSupervisorArgs {
                event_store: args.event_store.clone(),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        let (terminal_supervisor, _) = Actor::spawn_linked(
            None,
            TerminalSupervisor,
            TerminalSupervisorArgs {
                event_store: args.event_store.clone(),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        Ok(SessionSupervisorState {
            event_store: args.event_store,
            desktop_supervisor: Some(desktop_supervisor),
            chat_supervisor: Some(chat_supervisor),
            terminal_supervisor: Some(terminal_supervisor),
        })
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        info!(event = ?event, "SessionSupervisor supervision event");
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SessionSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
            SessionSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                args,
                reply,
            } => {
                if let Some(desktop_supervisor) = &state.desktop_supervisor {
                    match ractor::call!(desktop_supervisor, |ds_reply| {
                        DesktopSupervisorMsg::GetOrCreateDesktop {
                            desktop_id: desktop_id.clone(),
                            user_id: user_id.clone(),
                            args: args.clone(),
                            reply: ds_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => return Err(ActorProcessingErr::from(e)),
                    }
                } else {
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "DesktopSupervisor not available",
                    )));
                }
            }
            SessionSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                if let Some(chat_supervisor) = &state.chat_supervisor {
                    match ractor::call!(chat_supervisor, |chat_reply| {
                        ChatSupervisorMsg::GetOrCreateChat {
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                            reply: chat_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => return Err(ActorProcessingErr::from(e)),
                    }
                } else {
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "ChatSupervisor not available",
                    )));
                }
            }
            SessionSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                chat_actor_id,
                user_id,
                preload_session_id,
                preload_thread_id,
                reply,
            } => {
                if let Some(chat_supervisor) = &state.chat_supervisor {
                    match ractor::call!(chat_supervisor, |chat_reply| {
                        ChatSupervisorMsg::GetOrCreateChatAgent {
                            agent_id: agent_id.clone(),
                            chat_actor_id: chat_actor_id.clone(),
                            user_id: user_id.clone(),
                            preload_session_id: preload_session_id.clone(),
                            preload_thread_id: preload_thread_id.clone(),
                            reply: chat_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => return Err(ActorProcessingErr::from(e)),
                    }
                } else {
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "ChatSupervisor not available",
                    )));
                }
            }
            SessionSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            } => {
                if let Some(terminal_supervisor) = &state.terminal_supervisor {
                    match ractor::call!(terminal_supervisor, |ts_reply| {
                        TerminalSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: shell.clone(),
                            working_dir: working_dir.clone(),
                            reply: ts_reply,
                        }
                    }) {
                        Ok(result) => {
                            let _ = reply.send(result);
                        }
                        Err(e) => {
                            error!(error = %e, "Terminal supervisor RPC failed");
                            let _ = reply.send(Err(e.to_string()));
                        }
                    }
                } else {
                    let _ = reply.send(Err("TerminalSupervisor not available".to_string()));
                }
            }
        }
        Ok(())
    }
}
