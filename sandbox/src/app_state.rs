use ractor::{Actor, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::actors::conductor::registry::run_writer_id;
use crate::actors::conductor::ConductorMsg;
use crate::actors::desktop::{DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::WriterMsg;
use crate::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    event_store: ActorRef<EventStoreMsg>,
    application_supervisor: Mutex<Option<ActorRef<ApplicationSupervisorMsg>>>,
    conductor_actor: Mutex<Option<ActorRef<ConductorMsg>>>,
}

impl AppState {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                event_store,
                application_supervisor: Mutex::new(None),
                conductor_actor: Mutex::new(None),
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

    pub async fn get_or_create_writer(
        &self,
        writer_id: String,
        user_id: String,
    ) -> Result<ActorRef<WriterMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateWriter {
                writer_id,
                user_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())?
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

    pub async fn ensure_run_writer(&self, run_id: &str) -> Result<ActorRef<WriterMsg>, String> {
        self.get_or_create_writer(run_writer_id(run_id), "system".to_string())
            .await
    }

    pub async fn ensure_conductor(&self) -> Result<ActorRef<ConductorMsg>, String> {
        let mut guard = self.inner.conductor_actor.lock().await;

        if let Some(conductor) = guard.as_ref() {
            if conductor.get_status() == ractor::ActorStatus::Running {
                return Ok(conductor.clone());
            }
            tracing::warn!(
                actor_id = %conductor.get_id(),
                status = ?conductor.get_status(),
                "Cached conductor actor is not running; refreshing reference"
            );
            *guard = None;
        }

        let actor_name = "conductor:conductor-default".to_string();
        if let Some(cell) = ractor::registry::where_is(actor_name) {
            let actor_ref: ActorRef<ConductorMsg> = cell.into();
            if actor_ref.get_status() == ractor::ActorStatus::Running {
                *guard = Some(actor_ref.clone());
                return Ok(actor_ref);
            }
        }

        let supervisor = self.ensure_supervisor().await?;
        let conductor = ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateConductor {
                conductor_id: "conductor-default".to_string(),
                user_id: "system".to_string(),
                reply,
            }
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        *guard = Some(conductor.clone());
        Ok(conductor)
    }
}
