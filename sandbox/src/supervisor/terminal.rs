//! Terminal Supervisor - manages TerminalActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::event_store::EventStoreMsg;
use crate::actors::terminal::{TerminalActor, TerminalArguments, TerminalInfo, TerminalMsg};

#[derive(Debug, Default)]
pub struct TerminalSupervisor;

pub struct TerminalSupervisorState {
    pub terminals: HashMap<String, ActorRef<TerminalMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug, Clone)]
pub struct TerminalSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug)]
pub enum TerminalSupervisorMsg {
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<Result<ActorRef<TerminalMsg>, String>>,
    },
    GetTerminalInfo {
        terminal_id: String,
        reply: RpcReplyPort<Option<TerminalInfo>>,
    },
    GetTerminal {
        terminal_id: String,
        reply: RpcReplyPort<Option<ActorRef<TerminalMsg>>>,
    },
    ListTerminals {
        reply: RpcReplyPort<Vec<TerminalInfo>>,
    },
    RemoveTerminal {
        terminal_id: String,
    },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for TerminalSupervisor {
    type Msg = TerminalSupervisorMsg;
    type State = TerminalSupervisorState;
    type Arguments = TerminalSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "TerminalSupervisor starting");
        Ok(TerminalSupervisorState {
            terminals: HashMap::new(),
            event_store: args.event_store,
        })
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let SupervisionEvent::ActorTerminated(actor_cell, _, _)
        | SupervisionEvent::ActorFailed(actor_cell, _) = &event
        {
            let actor_id = actor_cell.get_id();
            state
                .terminals
                .retain(|_, terminal| terminal.get_id() != actor_id);
        }
        info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "TerminalSupervisor supervision event"
        );
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TerminalSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            } => {
                if let Some(terminal) = state.terminals.get(&terminal_id) {
                    let _ = reply.send(Ok(terminal.clone()));
                    return Ok(());
                }

                let actor_name = format!("terminal:{terminal_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<TerminalMsg> = cell.into();
                    state.terminals.insert(terminal_id, actor_ref.clone());
                    let _ = reply.send(Ok(actor_ref));
                    return Ok(());
                }

                let args = TerminalArguments {
                    terminal_id: terminal_id.clone(),
                    user_id,
                    shell,
                    working_dir,
                    event_store: state.event_store.clone(),
                };

                match Actor::spawn_linked(Some(actor_name), TerminalActor, args, myself.get_cell())
                    .await
                {
                    Ok((actor_ref, _)) => {
                        state.terminals.insert(terminal_id, actor_ref.clone());
                        let _ = reply.send(Ok(actor_ref));
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to spawn TerminalActor");
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
            }
            TerminalSupervisorMsg::GetTerminalInfo { terminal_id, reply } => {
                let info = if let Some(terminal) = state.terminals.get(&terminal_id) {
                    ractor::call!(terminal, |port| TerminalMsg::GetInfo { reply: port }).ok()
                } else {
                    None
                };
                let _ = reply.send(info);
            }
            TerminalSupervisorMsg::GetTerminal { terminal_id, reply } => {
                let _ = reply.send(state.terminals.get(&terminal_id).cloned());
            }
            TerminalSupervisorMsg::ListTerminals { reply } => {
                let mut infos = Vec::new();
                for terminal in state.terminals.values() {
                    if let Ok(info) =
                        ractor::call!(terminal, |port| TerminalMsg::GetInfo { reply: port })
                    {
                        infos.push(info);
                    }
                }
                let _ = reply.send(infos);
            }
            TerminalSupervisorMsg::RemoveTerminal { terminal_id } => {
                state.terminals.remove(&terminal_id);
            }
            TerminalSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
        }

        Ok(())
    }
}

pub async fn get_or_create_terminal(
    supervisor: &ActorRef<TerminalSupervisorMsg>,
    terminal_id: impl Into<String>,
    user_id: impl Into<String>,
    shell: impl Into<String>,
    working_dir: impl Into<String>,
) -> Result<Result<ActorRef<TerminalMsg>, String>, ractor::RactorErr<TerminalSupervisorMsg>> {
    ractor::call!(supervisor, |reply| {
        TerminalSupervisorMsg::GetOrCreateTerminal {
            terminal_id: terminal_id.into(),
            user_id: user_id.into(),
            shell: shell.into(),
            working_dir: working_dir.into(),
            reply,
        }
    })
}

pub async fn get_terminal_info(
    supervisor: &ActorRef<TerminalSupervisorMsg>,
    terminal_id: impl Into<String>,
) -> Result<Option<TerminalInfo>, ractor::RactorErr<TerminalSupervisorMsg>> {
    ractor::call!(supervisor, |reply| TerminalSupervisorMsg::GetTerminalInfo {
        terminal_id: terminal_id.into(),
        reply,
    })
}

pub async fn list_terminals(
    supervisor: &ActorRef<TerminalSupervisorMsg>,
) -> Result<Vec<TerminalInfo>, ractor::RactorErr<TerminalSupervisorMsg>> {
    ractor::call!(supervisor, |reply| TerminalSupervisorMsg::ListTerminals {
        reply,
    })
}

pub async fn remove_terminal(
    supervisor: &ActorRef<TerminalSupervisorMsg>,
    terminal_id: impl Into<String>,
) -> Result<(), ractor::RactorErr<TerminalSupervisorMsg>> {
    supervisor
        .cast(TerminalSupervisorMsg::RemoveTerminal {
            terminal_id: terminal_id.into(),
        })
        .map_err(ractor::RactorErr::from)
}
