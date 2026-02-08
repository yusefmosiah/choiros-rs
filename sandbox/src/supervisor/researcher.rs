//! Researcher Supervisor - manages ResearcherActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::{ResearcherActor, ResearcherArguments, ResearcherMsg};

#[derive(Debug, Default)]
pub struct ResearcherSupervisor;

pub struct ResearcherSupervisorState {
    pub researchers: HashMap<String, ActorRef<ResearcherMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug, Clone)]
pub struct ResearcherSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug)]
pub enum ResearcherSupervisorMsg {
    GetOrCreateResearcher {
        researcher_id: String,
        user_id: String,
        reply: RpcReplyPort<Result<ActorRef<ResearcherMsg>, String>>,
    },
    GetResearcher {
        researcher_id: String,
        reply: RpcReplyPort<Option<ActorRef<ResearcherMsg>>>,
    },
    RemoveResearcher {
        researcher_id: String,
    },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for ResearcherSupervisor {
    type Msg = ResearcherSupervisorMsg;
    type State = ResearcherSupervisorState;
    type Arguments = ResearcherSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "ResearcherSupervisor starting");
        Ok(ResearcherSupervisorState {
            researchers: HashMap::new(),
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
                .researchers
                .retain(|_, researcher| researcher.get_id() != actor_id);
        }
        info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "ResearcherSupervisor supervision event"
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
            ResearcherSupervisorMsg::GetOrCreateResearcher {
                researcher_id,
                user_id,
                reply,
            } => {
                if let Some(researcher) = state.researchers.get(&researcher_id) {
                    let _ = reply.send(Ok(researcher.clone()));
                    return Ok(());
                }

                let actor_name = format!("researcher:{researcher_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<ResearcherMsg> = cell.into();
                    state.researchers.insert(researcher_id, actor_ref.clone());
                    let _ = reply.send(Ok(actor_ref));
                    return Ok(());
                }

                let args = ResearcherArguments {
                    researcher_id: researcher_id.clone(),
                    user_id,
                    event_store: state.event_store.clone(),
                };

                match Actor::spawn_linked(
                    Some(actor_name),
                    ResearcherActor,
                    args,
                    myself.get_cell(),
                )
                .await
                {
                    Ok((actor_ref, _)) => {
                        state.researchers.insert(researcher_id, actor_ref.clone());
                        let _ = reply.send(Ok(actor_ref));
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to spawn ResearcherActor");
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
            }
            ResearcherSupervisorMsg::GetResearcher {
                researcher_id,
                reply,
            } => {
                let _ = reply.send(state.researchers.get(&researcher_id).cloned());
            }
            ResearcherSupervisorMsg::RemoveResearcher { researcher_id } => {
                state.researchers.remove(&researcher_id);
            }
            ResearcherSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
        }
        Ok(())
    }
}
