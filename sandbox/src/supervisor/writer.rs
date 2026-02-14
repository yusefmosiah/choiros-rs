//! Writer Supervisor - manages WriterActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::{WriterActor, WriterArguments, WriterMsg};

#[derive(Debug, Default)]
pub struct WriterSupervisor;

pub struct WriterSupervisorState {
    pub writers: HashMap<String, ActorRef<WriterMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
}

#[derive(Debug, Clone)]
pub struct WriterSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
}

#[derive(Debug)]
pub enum WriterSupervisorMsg {
    GetOrCreateWriter {
        writer_id: String,
        user_id: String,
        researcher_actor: Option<ActorRef<ResearcherMsg>>,
        terminal_actor: Option<ActorRef<TerminalMsg>>,
        reply: RpcReplyPort<Result<ActorRef<WriterMsg>, String>>,
    },
    GetWriter {
        writer_id: String,
        reply: RpcReplyPort<Option<ActorRef<WriterMsg>>>,
    },
    RemoveWriter {
        writer_id: String,
    },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for WriterSupervisor {
    type Msg = WriterSupervisorMsg;
    type State = WriterSupervisorState;
    type Arguments = WriterSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "WriterSupervisor starting");
        Ok(WriterSupervisorState {
            writers: HashMap::new(),
            event_store: args.event_store,
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
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
                .writers
                .retain(|_, writer| writer.get_id() != actor_id);
        }
        info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "WriterSupervisor supervision event"
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
            WriterSupervisorMsg::GetOrCreateWriter {
                writer_id,
                user_id,
                researcher_actor,
                terminal_actor,
                reply,
            } => {
                if let Some(writer) = state.writers.get(&writer_id) {
                    let _ = reply.send(Ok(writer.clone()));
                    return Ok(());
                }

                let actor_name = format!("writer:{writer_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<WriterMsg> = cell.into();
                    state.writers.insert(writer_id, actor_ref.clone());
                    let _ = reply.send(Ok(actor_ref));
                    return Ok(());
                }

                let args = WriterArguments {
                    writer_id: writer_id.clone(),
                    user_id,
                    event_store: state.event_store.clone(),
                    researcher_actor: researcher_actor.or_else(|| state.researcher_actor.clone()),
                    terminal_actor: terminal_actor.or_else(|| state.terminal_actor.clone()),
                };

                match Actor::spawn_linked(Some(actor_name), WriterActor, args, myself.get_cell())
                    .await
                {
                    Ok((actor_ref, _)) => {
                        state.writers.insert(writer_id, actor_ref.clone());
                        let _ = reply.send(Ok(actor_ref));
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to spawn WriterActor");
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
            }
            WriterSupervisorMsg::GetWriter { writer_id, reply } => {
                let _ = reply.send(state.writers.get(&writer_id).cloned());
            }
            WriterSupervisorMsg::RemoveWriter { writer_id } => {
                state.writers.remove(&writer_id);
            }
            WriterSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
        }
        Ok(())
    }
}
