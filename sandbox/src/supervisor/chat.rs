//! Chat Supervisor - manages ChatActor and ChatAgent instances

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use tracing::{error, info};

use crate::actors::chat::{ChatActor, ChatActorArguments, ChatActorMsg};
use crate::actors::chat_agent::{ChatAgent, ChatAgentArguments, ChatAgentMsg};
use crate::actors::event_store::EventStoreMsg;
use crate::supervisor::ApplicationSupervisorMsg;

#[derive(Debug, Default)]
pub struct ChatSupervisor;

#[derive(Debug, Clone)]
pub struct ChatInfo {
    pub actor_ref: ActorRef<ChatActorMsg>,
    pub actor_id: String,
    pub user_id: String,
}

pub struct ChatSupervisorState {
    pub chats: HashMap<String, ChatInfo>,
    pub agents: HashMap<String, ActorRef<ChatAgentMsg>>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub application_supervisor: Option<ActorRef<ApplicationSupervisorMsg>>,
}

#[derive(Debug, Clone)]
pub struct ChatSupervisorArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub application_supervisor: Option<ActorRef<ApplicationSupervisorMsg>>,
}

#[derive(Debug)]
pub enum ChatSupervisorMsg {
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
    GetChat {
        actor_id: String,
        reply: RpcReplyPort<Option<ActorRef<ChatActorMsg>>>,
    },
    GetChatAgent {
        agent_id: String,
        reply: RpcReplyPort<Option<ActorRef<ChatAgentMsg>>>,
    },
    RemoveChat {
        actor_id: String,
    },
    RemoveChatAgent {
        agent_id: String,
    },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for ChatSupervisor {
    type Msg = ChatSupervisorMsg;
    type State = ChatSupervisorState;
    type Arguments = ChatSupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "ChatSupervisor starting");
        Ok(ChatSupervisorState {
            chats: HashMap::new(),
            agents: HashMap::new(),
            event_store: args.event_store,
            application_supervisor: args.application_supervisor,
        })
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match &event {
            SupervisionEvent::ActorTerminated(actor_cell, _, _)
            | SupervisionEvent::ActorFailed(actor_cell, _) => {
                let child_id = actor_cell.get_id();
                state
                    .chats
                    .retain(|_, info| info.actor_ref.get_id() != child_id);
                state.agents.retain(|_, agent| agent.get_id() != child_id);
            }
            _ => {}
        }

        info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "ChatSupervisor supervision event"
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
            ChatSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                if let Some(info) = state.chats.get(&actor_id) {
                    let _ = reply.send(info.actor_ref.clone());
                    return Ok(());
                }

                let actor_name = format!("chat:{actor_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<ChatActorMsg> = cell.into();
                    state.chats.insert(
                        actor_id.clone(),
                        ChatInfo {
                            actor_ref: actor_ref.clone(),
                            actor_id,
                            user_id,
                        },
                    );
                    let _ = reply.send(actor_ref);
                    return Ok(());
                }

                let args = ChatActorArguments {
                    actor_id: actor_id.clone(),
                    user_id: user_id.clone(),
                    event_store: state.event_store.clone(),
                };
                match Actor::spawn_linked(Some(actor_name), ChatActor, args, myself.get_cell())
                    .await
                {
                    Ok((actor_ref, _)) => {
                        state.chats.insert(
                            actor_id.clone(),
                            ChatInfo {
                                actor_ref: actor_ref.clone(),
                                actor_id,
                                user_id,
                            },
                        );
                        let _ = reply.send(actor_ref);
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to spawn ChatActor");
                        return Err(ActorProcessingErr::from(e));
                    }
                }
            }
            ChatSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                chat_actor_id,
                user_id,
                preload_session_id,
                preload_thread_id,
                reply,
            } => {
                if let Some(agent) = state.agents.get(&agent_id) {
                    let _ = reply.send(agent.clone());
                    return Ok(());
                }

                let actor_name = format!("agent:{agent_id}");
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    let actor_ref: ActorRef<ChatAgentMsg> = cell.into();
                    state.agents.insert(agent_id, actor_ref.clone());
                    let _ = reply.send(actor_ref);
                    return Ok(());
                }

                let args = ChatAgentArguments {
                    actor_id: chat_actor_id,
                    user_id,
                    event_store: state.event_store.clone(),
                    preload_session_id,
                    preload_thread_id,
                    application_supervisor: state.application_supervisor.clone(),
                };
                match Actor::spawn_linked(
                    Some(actor_name),
                    ChatAgent::new(),
                    args,
                    myself.get_cell(),
                )
                .await
                {
                    Ok((actor_ref, _)) => {
                        state.agents.insert(agent_id, actor_ref.clone());
                        let _ = reply.send(actor_ref);
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to spawn ChatAgent");
                        return Err(ActorProcessingErr::from(e));
                    }
                }
            }
            ChatSupervisorMsg::GetChat { actor_id, reply } => {
                let result = state
                    .chats
                    .get(&actor_id)
                    .map(|info| info.actor_ref.clone());
                let _ = reply.send(result);
            }
            ChatSupervisorMsg::GetChatAgent { agent_id, reply } => {
                let _ = reply.send(state.agents.get(&agent_id).cloned());
            }
            ChatSupervisorMsg::RemoveChat { actor_id } => {
                state.chats.remove(&actor_id);
            }
            ChatSupervisorMsg::RemoveChatAgent { agent_id } => {
                state.agents.remove(&agent_id);
            }
            ChatSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
        }
        Ok(())
    }
}

pub async fn get_or_create_chat(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    actor_id: impl Into<String>,
    user_id: impl Into<String>,
) -> Result<ActorRef<ChatActorMsg>, ractor::RactorErr<ChatSupervisorMsg>> {
    ractor::call!(supervisor, |reply| ChatSupervisorMsg::GetOrCreateChat {
        actor_id: actor_id.into(),
        user_id: user_id.into(),
        reply,
    })
}

pub async fn get_or_create_chat_agent(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    agent_id: impl Into<String>,
    chat_actor_id: impl Into<String>,
    user_id: impl Into<String>,
    preload_session_id: Option<String>,
    preload_thread_id: Option<String>,
) -> Result<ActorRef<ChatAgentMsg>, ractor::RactorErr<ChatSupervisorMsg>> {
    ractor::call!(supervisor, |reply| {
        ChatSupervisorMsg::GetOrCreateChatAgent {
            agent_id: agent_id.into(),
            chat_actor_id: chat_actor_id.into(),
            user_id: user_id.into(),
            preload_session_id,
            preload_thread_id,
            reply,
        }
    })
}

pub async fn get_chat(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    actor_id: impl Into<String>,
) -> Result<Option<ActorRef<ChatActorMsg>>, ractor::RactorErr<ChatSupervisorMsg>> {
    ractor::call!(supervisor, |reply| ChatSupervisorMsg::GetChat {
        actor_id: actor_id.into(),
        reply,
    })
}

pub async fn get_chat_agent(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    agent_id: impl Into<String>,
) -> Result<Option<ActorRef<ChatAgentMsg>>, ractor::RactorErr<ChatSupervisorMsg>> {
    ractor::call!(supervisor, |reply| ChatSupervisorMsg::GetChatAgent {
        agent_id: agent_id.into(),
        reply,
    })
}

pub async fn remove_chat(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    actor_id: impl Into<String>,
) -> Result<(), ractor::RactorErr<ChatSupervisorMsg>> {
    supervisor
        .cast(ChatSupervisorMsg::RemoveChat {
            actor_id: actor_id.into(),
        })
        .map_err(ractor::RactorErr::from)
}

pub async fn remove_chat_agent(
    supervisor: &ActorRef<ChatSupervisorMsg>,
    agent_id: impl Into<String>,
) -> Result<(), ractor::RactorErr<ChatSupervisorMsg>> {
    supervisor
        .cast(ChatSupervisorMsg::RemoveChatAgent {
            agent_id: agent_id.into(),
        })
        .map_err(ractor::RactorErr::from)
}
