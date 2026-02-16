//! Conductor Supervisor - manages ConductorActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorMsg};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::WriterMsg;

#[derive(Debug, Default)]
pub struct ConductorSupervisor;

pub struct ConductorSupervisorState {
    pub conductors: HashMap<String, ActorRef<ConductorMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
    pub writer_actor: Option<ActorRef<WriterMsg>>,
}

#[derive(Debug, Clone)]
pub struct ConductorSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
    pub writer_actor: Option<ActorRef<WriterMsg>>,
}

#[derive(Debug)]
pub enum ConductorSupervisorMsg {
    GetOrCreateConductor {
        conductor_id: String,
        user_id: String,
        researcher_actor: Option<ActorRef<ResearcherMsg>>,
        terminal_actor: Option<ActorRef<TerminalMsg>>,
        writer_actor: Option<ActorRef<WriterMsg>>,
        reply: RpcReplyPort<Result<ActorRef<ConductorMsg>, String>>,
    },
    GetConductor {
        conductor_id: String,
        reply: RpcReplyPort<Option<ActorRef<ConductorMsg>>>,
    },
    RemoveConductor {
        conductor_id: String,
    },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for ConductorSupervisor {
    type Msg = ConductorSupervisorMsg;
    type State = ConductorSupervisorState;
    type Arguments = ConductorSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "ConductorSupervisor starting");
        Ok(ConductorSupervisorState {
            conductors: HashMap::new(),
            event_store: args.event_store,
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
            writer_actor: args.writer_actor,
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
                .conductors
                .retain(|_, conductor| conductor.get_id() != actor_id);
        }
        info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "ConductorSupervisor supervision event"
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
            ConductorSupervisorMsg::GetOrCreateConductor {
                conductor_id,
                user_id,
                researcher_actor,
                terminal_actor,
                writer_actor,
                reply,
            } => {
                if let Some(conductor) = state.conductors.get(&conductor_id) {
                    let _ = reply.send(Ok(conductor.clone()));
                    return Ok(());
                }

                let actor_name = format!("conductor:{conductor_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<ConductorMsg> = cell.into();
                    state.conductors.insert(conductor_id, actor_ref.clone());
                    let _ = reply.send(Ok(actor_ref));
                    return Ok(());
                }

                let args = ConductorArguments {
                    event_store: state.event_store.clone(),
                    researcher_actor: researcher_actor.or_else(|| state.researcher_actor.clone()),
                    terminal_actor: terminal_actor.or_else(|| state.terminal_actor.clone()),
                    writer_actor: writer_actor.or_else(|| state.writer_actor.clone()),
                };

                match Actor::spawn_linked(Some(actor_name), ConductorActor, args, myself.get_cell())
                    .await
                {
                    Ok((actor_ref, _)) => {
                        info!(
                            conductor_id = %conductor_id,
                            user_id = %user_id,
                            actor_id = %actor_ref.get_id(),
                            "Spawned ConductorActor"
                        );
                        state.conductors.insert(conductor_id, actor_ref.clone());
                        let _ = reply.send(Ok(actor_ref));
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            conductor_id = %conductor_id,
                            user_id = %user_id,
                            "Failed to spawn ConductorActor"
                        );
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
            }
            ConductorSupervisorMsg::GetConductor {
                conductor_id,
                reply,
            } => {
                let _ = reply.send(state.conductors.get(&conductor_id).cloned());
            }
            ConductorSupervisorMsg::RemoveConductor { conductor_id } => {
                state.conductors.remove(&conductor_id);
            }
            ConductorSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
        }
        Ok(())
    }
}
