//! Session Supervisor - manages domain supervisors

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tracing::{error, info};

use crate::actors::conductor::ConductorMsg;
use crate::actors::desktop::{DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::WriterMsg;
use crate::supervisor::conductor::{
    ConductorSupervisor, ConductorSupervisorArgs, ConductorSupervisorMsg,
};
use crate::supervisor::desktop::{DesktopSupervisor, DesktopSupervisorArgs, DesktopSupervisorMsg};
use crate::supervisor::researcher::{
    ResearcherSupervisor, ResearcherSupervisorArgs, ResearcherSupervisorMsg,
};
use crate::supervisor::terminal::{
    TerminalSupervisor, TerminalSupervisorArgs, TerminalSupervisorMsg,
};
use crate::supervisor::writer::{WriterSupervisor, WriterSupervisorArgs, WriterSupervisorMsg};
use crate::supervisor::ApplicationSupervisorMsg;

#[derive(Debug, Default)]
pub struct SessionSupervisor;

#[derive(Debug, Clone)]
pub struct SessionSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub application_supervisor: ActorRef<ApplicationSupervisorMsg>,
}

pub struct SessionSupervisorState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub conductor_supervisor: Option<ActorRef<ConductorSupervisorMsg>>,
    pub desktop_supervisor: Option<ActorRef<DesktopSupervisorMsg>>,
    pub terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
    pub researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    pub writer_supervisor: Option<ActorRef<WriterSupervisorMsg>>,
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
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<Result<ActorRef<TerminalMsg>, String>>,
    },
    GetOrCreateResearcher {
        researcher_id: String,
        user_id: String,
        reply: RpcReplyPort<Result<ActorRef<ResearcherMsg>, String>>,
    },
    GetOrCreateWriter {
        writer_id: String,
        user_id: String,
        reply: RpcReplyPort<Result<ActorRef<WriterMsg>, String>>,
    },
    GetOrCreateConductor {
        conductor_id: String,
        user_id: String,
        reply: RpcReplyPort<Result<ActorRef<ConductorMsg>, String>>,
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

        let (researcher_supervisor, _) = Actor::spawn_linked(
            None,
            ResearcherSupervisor,
            ResearcherSupervisorArgs {
                event_store: args.event_store.clone(),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        let (writer_supervisor, _) = Actor::spawn_linked(
            None,
            WriterSupervisor,
            WriterSupervisorArgs {
                event_store: args.event_store.clone(),
                researcher_supervisor: Some(researcher_supervisor.clone()),
                terminal_supervisor: Some(terminal_supervisor.clone()),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        let (conductor_supervisor, _) = Actor::spawn_linked(
            None,
            ConductorSupervisor,
            ConductorSupervisorArgs {
                event_store: args.event_store.clone(),
                writer_supervisor: Some(writer_supervisor.clone()),
            },
            myself.get_cell(),
        )
        .await
        .map_err(ActorProcessingErr::from)?;

        Ok(SessionSupervisorState {
            event_store: args.event_store,
            conductor_supervisor: Some(conductor_supervisor),
            desktop_supervisor: Some(desktop_supervisor),
            terminal_supervisor: Some(terminal_supervisor),
            researcher_supervisor: Some(researcher_supervisor),
            writer_supervisor: Some(writer_supervisor),
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
            SessionSupervisorMsg::GetOrCreateResearcher {
                researcher_id,
                user_id,
                reply,
            } => {
                if let Some(researcher_supervisor) = &state.researcher_supervisor {
                    match ractor::call!(researcher_supervisor, |rs_reply| {
                        ResearcherSupervisorMsg::GetOrCreateResearcher {
                            researcher_id: researcher_id.clone(),
                            user_id: user_id.clone(),
                            reply: rs_reply,
                        }
                    }) {
                        Ok(result) => {
                            let _ = reply.send(result);
                        }
                        Err(e) => {
                            error!(error = %e, "Researcher supervisor RPC failed");
                            let _ = reply.send(Err(e.to_string()));
                        }
                    }
                } else {
                    let _ = reply.send(Err("ResearcherSupervisor not available".to_string()));
                }
            }
            SessionSupervisorMsg::GetOrCreateWriter {
                writer_id,
                user_id,
                reply,
            } => {
                if let Some(writer_supervisor) = &state.writer_supervisor {
                    match ractor::call!(writer_supervisor, |ws_reply| {
                        WriterSupervisorMsg::GetOrCreateWriter {
                            writer_id: writer_id.clone(),
                            user_id: user_id.clone(),
                            reply: ws_reply,
                        }
                    }) {
                        Ok(result) => {
                            let _ = reply.send(result);
                        }
                        Err(e) => {
                            error!(error = %e, "Writer supervisor RPC failed");
                            let _ = reply.send(Err(e.to_string()));
                        }
                    }
                } else {
                    let _ = reply.send(Err("WriterSupervisor not available".to_string()));
                }
            }
            SessionSupervisorMsg::GetOrCreateConductor {
                conductor_id,
                user_id,
                reply,
            } => {
                if let Some(conductor_supervisor) = &state.conductor_supervisor {
                    match ractor::call!(conductor_supervisor, |cs_reply| {
                        ConductorSupervisorMsg::GetOrCreateConductor {
                            conductor_id: conductor_id.clone(),
                            user_id: user_id.clone(),
                            reply: cs_reply,
                        }
                    }) {
                        Ok(result) => {
                            let _ = reply.send(result);
                        }
                        Err(e) => {
                            error!(error = %e, "Conductor supervisor RPC failed");
                            let _ = reply.send(Err(e.to_string()));
                        }
                    }
                } else {
                    let _ = reply.send(Err("ConductorSupervisor not available".to_string()));
                }
            }
        }
        Ok(())
    }
}
