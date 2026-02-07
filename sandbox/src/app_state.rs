use ractor::{Actor, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::actors::chat::ChatActorMsg;
use crate::actors::chat_agent::ChatAgentMsg;
use crate::actors::desktop::{DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::terminal::TerminalMsg;
use crate::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    event_store: ActorRef<EventStoreMsg>,
    application_supervisor: Mutex<Option<ActorRef<ApplicationSupervisorMsg>>>,
}

impl AppState {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                event_store,
                application_supervisor: Mutex::new(None),
            }),
        }
    }

    pub fn event_store(&self) -> ActorRef<EventStoreMsg> {
        self.inner.event_store.clone()
    }

    pub async fn ensure_supervisor(&self) -> Result<ActorRef<ApplicationSupervisorMsg>, String> {
        let mut guard = self.inner.application_supervisor.lock().await;
        if let Some(supervisor) = guard.as_ref() {
            return Ok(supervisor.clone());
        }

        let (supervisor, _) = Actor::spawn(
            Some(format!("application_supervisor:{}", ulid::Ulid::new())),
            ApplicationSupervisor,
            self.inner.event_store.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;

        *guard = Some(supervisor.clone());
        Ok(supervisor)
    }

    pub async fn get_or_create_desktop(
        &self,
        desktop_id: String,
        user_id: String,
    ) -> Result<ActorRef<DesktopActorMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())
    }

    pub async fn get_or_create_chat(
        &self,
        actor_id: String,
        user_id: String,
    ) -> Result<ActorRef<ChatActorMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())
    }

    pub async fn get_or_create_chat_agent(
        &self,
        agent_id: String,
        user_id: String,
    ) -> Result<ActorRef<ChatAgentMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                user_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())
    }

    pub async fn get_or_create_terminal(
        &self,
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
    ) -> Result<ActorRef<TerminalMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            }
        })
        .map_err(|e| e.to_string())
    }

    pub async fn get_or_create_terminal_with_args(
        &self,
        args: crate::actors::terminal::TerminalArguments,
    ) -> Result<ActorRef<TerminalMsg>, String> {
        self.get_or_create_terminal(args.terminal_id, args.user_id, args.shell, args.working_dir)
            .await
    }

    pub fn desktop_args(&self, desktop_id: String, user_id: String) -> DesktopArguments {
        DesktopArguments {
            desktop_id,
            user_id,
            event_store: self.event_store(),
        }
    }
}
