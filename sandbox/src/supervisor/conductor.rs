//! Conductor Supervisor - manages ConductorActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{debug, error, info};

use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorMsg};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::memory::MemoryMsg;
use crate::supervisor::writer::WriterSupervisorMsg;

#[derive(Debug, Default)]
pub struct ConductorSupervisor;

fn lookup_running_conductor(actor_name: &str) -> Option<ActorRef<ConductorMsg>> {
    let cell = ractor::registry::where_is(actor_name.to_string())?;
    let actor_ref: ActorRef<ConductorMsg> = cell.into();
    (actor_ref.get_status() == ractor::ActorStatus::Running).then_some(actor_ref)
}

pub struct ConductorSupervisorState {
    pub conductors: HashMap<String, ActorRef<ConductorMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub writer_supervisor: Option<ActorRef<WriterSupervisorMsg>>,
    pub memory_actor: Option<ActorRef<MemoryMsg>>,
}

#[derive(Debug, Clone)]
pub struct ConductorSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub writer_supervisor: Option<ActorRef<WriterSupervisorMsg>>,
    pub memory_actor: Option<ActorRef<MemoryMsg>>,
}

#[derive(Debug)]
pub enum ConductorSupervisorMsg {
    GetOrCreateConductor {
        conductor_id: String,
        user_id: String,
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
            writer_supervisor: args.writer_supervisor,
            memory_actor: args.memory_actor,
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
                reply,
            } => {
                if let Some(conductor) = state.conductors.get(&conductor_id).cloned() {
                    if conductor.get_status() == ractor::ActorStatus::Running {
                        let _ = reply.send(Ok(conductor));
                        return Ok(());
                    }
                    state.conductors.remove(&conductor_id);
                }

                let actor_name = format!("conductor:{conductor_id}");
                if let Some(actor_ref) = lookup_running_conductor(&actor_name) {
                    state.conductors.insert(conductor_id, actor_ref.clone());
                    let _ = reply.send(Ok(actor_ref));
                    return Ok(());
                }

                let args = ConductorArguments {
                    event_store: state.event_store.clone(),
                    writer_supervisor: state.writer_supervisor.clone(),
                    memory_actor: state.memory_actor.clone(),
                };

                match Actor::spawn_linked(
                    Some(actor_name.clone()),
                    ConductorActor,
                    args,
                    myself.get_cell(),
                )
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
                        if let Some(actor_ref) = lookup_running_conductor(&actor_name) {
                            debug!(
                                error = %e,
                                conductor_id = %conductor_id,
                                user_id = %user_id,
                                actor_id = %actor_ref.get_id(),
                                "Conductor spawn raced with an existing actor; reusing running actor"
                            );
                            state.conductors.insert(conductor_id, actor_ref.clone());
                            let _ = reply.send(Ok(actor_ref));
                            return Ok(());
                        }
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
