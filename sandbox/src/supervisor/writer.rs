//! Writer Supervisor - manages WriterActor instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::event_store::EventStoreMsg;
use crate::actors::writer::{WriterActor, WriterArguments, WriterMsg};
use crate::supervisor::researcher::ResearcherSupervisorMsg;
use crate::supervisor::terminal::TerminalSupervisorMsg;

#[derive(Debug, Default)]
pub struct WriterSupervisor;

pub struct WriterSupervisorState {
    pub writers: HashMap<String, ActorRef<WriterMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    pub terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
}

#[derive(Debug, Clone)]
pub struct WriterSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    pub terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
}

#[derive(Debug)]
pub enum WriterSupervisorMsg {
    GetOrCreateWriter {
        writer_id: String,
        user_id: String,
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

    // -----------------------------------------------------------------------
    // Phase 2.6 — WriterSupervisor typed run-registry messages
    //
    // These replace GetOrCreateWriter / GetWriter / RemoveWriter in the
    // target architecture (Phase 3+) where WriterActor is spawned per
    // run_id and resolved by run_id, not writer_id.
    // -----------------------------------------------------------------------
    /// Resolve the ActorRef<WriterMsg> for a given run_id.
    /// Returns `None` if no WriterActor is registered for that run.
    Resolve {
        run_id: String,
        reply: RpcReplyPort<Option<ActorRef<WriterMsg>>>,
    },
    /// Register a newly spawned WriterActor under its run_id.
    Register {
        run_id: String,
        actor_ref: ActorRef<WriterMsg>,
    },
    /// Deregister a WriterActor when its run completes or the window closes.
    Deregister {
        run_id: String,
    },
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
            researcher_supervisor: args.researcher_supervisor,
            terminal_supervisor: args.terminal_supervisor,
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
                    researcher_supervisor: state.researcher_supervisor.clone(),
                    terminal_supervisor: state.terminal_supervisor.clone(),
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
                if let Some(actor_ref) = state.writers.remove(&writer_id) {
                    actor_ref.stop(None);
                }
            }
            WriterSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }

            // Phase 2.6 — run-registry variants (handlers are stubs; full
            // implementation wired in Phase 3+ when WriterActor is fully ephemeral).
            WriterSupervisorMsg::Resolve { run_id, reply } => {
                let _ = reply.send(state.writers.get(&run_id).cloned());
            }
            WriterSupervisorMsg::Register { run_id, actor_ref } => {
                state.writers.insert(run_id, actor_ref);
            }
            WriterSupervisorMsg::Deregister { run_id } => {
                state.writers.remove(&run_id);
            }
        }
        Ok(())
    }
}
