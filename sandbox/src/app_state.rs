use ractor::{Actor, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorMsg};
use crate::actors::desktop::{DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
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

    pub async fn get_or_create_researcher(
        &self,
        researcher_id: String,
        user_id: String,
    ) -> Result<ActorRef<ResearcherMsg>, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateResearcher {
                researcher_id,
                user_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())?
    }

    pub async fn get_or_create_terminal_with_args(
        &self,
        args: crate::actors::terminal::TerminalArguments,
    ) -> Result<ActorRef<TerminalMsg>, String> {
        self.get_or_create_terminal(args.terminal_id, args.user_id, args.shell, args.working_dir)
            .await
    }

    pub async fn delegate_terminal_task(
        &self,
        terminal_id: String,
        actor_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        command: String,
        timeout_ms: Option<u64>,
        model_override: Option<String>,
        objective: Option<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<shared_types::DelegatedTask, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::DelegateTerminalTask {
                terminal_id,
                actor_id,
                user_id,
                shell,
                working_dir,
                command,
                timeout_ms,
                model_override,
                objective,
                session_id,
                thread_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())?
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn delegate_research_task(
        &self,
        researcher_id: String,
        actor_id: String,
        user_id: String,
        query: String,
        objective: Option<String>,
        provider: Option<String>,
        max_results: Option<u32>,
        time_range: Option<String>,
        include_domains: Option<Vec<String>>,
        exclude_domains: Option<Vec<String>>,
        timeout_ms: Option<u64>,
        model_override: Option<String>,
        reasoning: Option<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<shared_types::DelegatedTask, String> {
        let supervisor = self.ensure_supervisor().await?;
        ractor::call!(supervisor, |reply| {
            ApplicationSupervisorMsg::DelegateResearchTask {
                researcher_id,
                actor_id,
                user_id,
                query,
                objective,
                provider,
                max_results,
                time_range,
                include_domains,
                exclude_domains,
                timeout_ms,
                model_override,
                reasoning,
                session_id,
                thread_id,
                reply,
            }
        })
        .map_err(|e| e.to_string())?
    }

    pub fn desktop_args(&self, desktop_id: String, user_id: String) -> DesktopArguments {
        DesktopArguments {
            desktop_id,
            user_id,
            event_store: self.event_store(),
        }
    }

    pub async fn ensure_conductor(&self) -> Result<ActorRef<ConductorMsg>, String> {
        let mut guard = self.inner.conductor_actor.lock().await;
        if let Some(conductor) = guard.as_ref() {
            return Ok(conductor.clone());
        }

        let disable_workers = std::env::var("CHOIR_DISABLE_CONDUCTOR_WORKERS")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                normalized == "1" || normalized == "true" || normalized == "yes"
            })
            .unwrap_or(false);

        let mut worker_errors: Vec<String> = Vec::new();

        let researcher_actor = if disable_workers {
            worker_errors.push(
                "researcher unavailable: disabled by CHOIR_DISABLE_CONDUCTOR_WORKERS".to_string(),
            );
            None
        } else {
            match self
                .get_or_create_researcher("conductor-researcher".to_string(), "system".to_string())
                .await
            {
                Ok(actor) => Some(actor),
                Err(err) => {
                    worker_errors.push(format!("researcher unavailable: {err}"));
                    None
                }
            }
        };

        let terminal_actor = if disable_workers {
            worker_errors.push(
                "terminal unavailable: disabled by CHOIR_DISABLE_CONDUCTOR_WORKERS".to_string(),
            );
            None
        } else {
            match self
                .get_or_create_terminal(
                    "conductor-terminal".to_string(),
                    "system".to_string(),
                    "/bin/zsh".to_string(),
                    env!("CARGO_MANIFEST_DIR").to_string(),
                )
                .await
            {
                Ok(actor) => Some(actor),
                Err(err) => {
                    worker_errors.push(format!("terminal unavailable: {err}"));
                    None
                }
            }
        };

        if researcher_actor.is_none() && terminal_actor.is_none() {
            let detail = if worker_errors.is_empty() {
                "unknown worker creation failure".to_string()
            } else {
                worker_errors.join("; ")
            };
            return Err(format!(
                "No worker actors available for Conductor default policy ({detail})"
            ));
        }

        let (conductor, _) = Actor::spawn(
            Some(format!("conductor:{}", ulid::Ulid::new())),
            ConductorActor,
            ConductorArguments {
                event_store: self.inner.event_store.clone(),
                researcher_actor,
                terminal_actor,
                policy: None,
            },
        )
        .await
        .map_err(|e| e.to_string())?;

        *guard = Some(conductor.clone());
        Ok(conductor)
    }
}
