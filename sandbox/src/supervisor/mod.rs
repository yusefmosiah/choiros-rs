//! Application Supervisor - Root of the supervision tree
//!
//! This module provides the ApplicationSupervisor which is the root
//! supervisor for the entire ChoirOS actor hierarchy.
//!
//! ## Architecture
//!
//! ApplicationSupervisor (one_for_one strategy)
//! └── SessionSupervisor (one_for_one strategy)
//!     ├── DesktopSupervisor
//!     ├── ChatSupervisor
//!     └── TerminalSupervisor
//!
//! ## Supervision Events
//!
//! The supervisor handles:
//! - `ActorStarted`: New child actor started
//! - `ActorFailed`: Child actor crashed/failed
//! - `ActorTerminated`: Child actor terminated normally
//!
//! ## Feature Flag
//!
//! This module is gated by the `supervision_refactor` feature flag.

pub mod chat;
pub mod desktop;
pub mod researcher;
pub mod session;
pub mod terminal;

// Re-export from session module
pub use session::{
    SessionSupervisor, SessionSupervisorArgs, SessionSupervisorMsg, SessionSupervisorState,
};

// Re-export from desktop module
pub use desktop::{
    get_desktop, get_or_create_desktop, remove_desktop, DesktopInfo, DesktopSupervisor,
    DesktopSupervisorArgs, DesktopSupervisorMsg, DesktopSupervisorState,
};

// Re-export from researcher module
pub use researcher::{
    ResearcherSupervisor, ResearcherSupervisorArgs, ResearcherSupervisorMsg,
    ResearcherSupervisorState,
};

// Re-export from chat module
pub use chat::{
    get_chat, get_chat_agent, get_or_create_chat, get_or_create_chat_agent, remove_chat,
    remove_chat_agent, ChatInfo, ChatSupervisor, ChatSupervisorArgs, ChatSupervisorMsg,
    ChatSupervisorState,
};

// Re-export from terminal module
pub use terminal::{
    get_or_create_terminal, get_terminal_info, list_terminals, remove_terminal, TerminalSupervisor,
    TerminalSupervisorArgs, TerminalSupervisorMsg, TerminalSupervisorState,
};

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::{HashMap, VecDeque};
use tracing::{error, info};

use crate::actors::event_bus::{
    Event, EventBusActor, EventBusArguments, EventBusConfig, EventBusMsg, EventType,
};
use crate::actors::event_relay::{EventRelayActor, EventRelayArguments, EventRelayMsg};
use crate::actors::event_store::EventStoreMsg;
use crate::actors::researcher::{
    ResearchObjectiveStatus, ResearchProviderCall, ResearcherMsg, ResearcherProgress,
    ResearcherResult, ResearcherWebSearchRequest,
};
use crate::actors::terminal::{
    TerminalAgentProgress, TerminalAgentResult, TerminalError, TerminalMsg,
};

/// Application supervisor - root of the supervision tree
#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

struct FailureClassification {
    kind: shared_types::FailureKind,
    retriable: bool,
    hint: &'static str,
}

/// Application supervisor state
pub struct ApplicationState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub event_bus: Option<ActorRef<EventBusMsg>>,
    pub event_relay: Option<ActorRef<EventRelayMsg>>,
    pub session_supervisor: Option<ActorRef<SessionSupervisorMsg>>,
    pub supervision_event_counts: SupervisionEventCounts,
    pub last_supervision_failure: Option<String>,
    pub worker_signal_policy: WorkerSignalPolicy,
    pub recent_signal_keys: VecDeque<(String, chrono::DateTime<chrono::Utc>)>,
    pub escalation_cooldowns: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupervisionEventCounts {
    pub actor_started: u64,
    pub actor_failed: u64,
    pub actor_terminated: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSupervisorHealth {
    pub event_bus_healthy: bool,
    pub event_relay_healthy: bool,
    pub session_supervisor_healthy: bool,
    pub supervision_event_counts: SupervisionEventCounts,
    pub last_supervision_failure: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkerSignalPolicy {
    pub max_findings_per_turn: usize,
    pub max_learnings_per_turn: usize,
    pub max_escalations_per_turn: usize,
    pub max_artifacts_per_turn: usize,
    pub min_confidence: f64,
    pub duplicate_window_seconds: i64,
    pub escalation_cooldown_seconds: i64,
}

impl WorkerSignalPolicy {
    fn from_env() -> Self {
        let mut policy = Self::default();
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_MAX_FINDINGS") {
            if let Ok(parsed) = raw.parse::<usize>() {
                policy.max_findings_per_turn = parsed.clamp(1, 10);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_MAX_LEARNINGS") {
            if let Ok(parsed) = raw.parse::<usize>() {
                policy.max_learnings_per_turn = parsed.clamp(1, 10);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_MAX_ESCALATIONS") {
            if let Ok(parsed) = raw.parse::<usize>() {
                policy.max_escalations_per_turn = parsed.clamp(1, 10);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_MAX_ARTIFACTS") {
            if let Ok(parsed) = raw.parse::<usize>() {
                policy.max_artifacts_per_turn = parsed.clamp(1, 25);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_MIN_CONFIDENCE") {
            if let Ok(parsed) = raw.parse::<f64>() {
                policy.min_confidence = parsed.clamp(0.0, 1.0);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_DUP_WINDOW_SEC") {
            if let Ok(parsed) = raw.parse::<i64>() {
                policy.duplicate_window_seconds = parsed.clamp(10, 86_400);
            }
        }
        if let Ok(raw) = std::env::var("CHOIR_SIGNAL_ESCALATION_COOLDOWN_SEC") {
            if let Ok(parsed) = raw.parse::<i64>() {
                policy.escalation_cooldown_seconds = parsed.clamp(5, 86_400);
            }
        }
        policy
    }
}

impl Default for WorkerSignalPolicy {
    fn default() -> Self {
        Self {
            max_findings_per_turn: 2,
            max_learnings_per_turn: 1,
            max_escalations_per_turn: 1,
            max_artifacts_per_turn: 8,
            min_confidence: 0.55,
            duplicate_window_seconds: 900,
            escalation_cooldown_seconds: 90,
        }
    }
}

/// Messages handled by ApplicationSupervisor
#[derive(Debug)]
pub enum ApplicationSupervisorMsg {
    /// Supervision event from child actors
    Supervision(SupervisionEvent),
    /// Get or create a desktop actor for a user
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::desktop::DesktopActorMsg>>,
    },
    /// Get or create a chat actor
    GetOrCreateChat {
        actor_id: String,
        user_id: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::chat::ChatActorMsg>>,
    },
    /// Get or create a chat agent
    GetOrCreateChatAgent {
        agent_id: String,
        chat_actor_id: String,
        user_id: String,
        preload_session_id: Option<String>,
        preload_thread_id: Option<String>,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::chat_agent::ChatAgentMsg>>,
    },
    /// Get or create a terminal session
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::terminal::TerminalMsg>>,
    },
    /// Delegate a terminal command asynchronously via TerminalActor.
    DelegateTerminalTask {
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
        reply: RpcReplyPort<Result<shared_types::DelegatedTask, String>>,
    },
    /// Delegate a typed web search task asynchronously via ResearcherActor.
    DelegateResearchTask {
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
        reply: RpcReplyPort<Result<shared_types::DelegatedTask, String>>,
    },
    /// Ingest a typed worker turn report and emit canonical signal events.
    IngestWorkerTurnReport {
        actor_id: String,
        user_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        report: shared_types::WorkerTurnReport,
        reply: RpcReplyPort<Result<shared_types::WorkerTurnReportIngestResult, String>>,
    },
    /// Return health snapshot and supervision counters.
    GetHealth {
        reply: RpcReplyPort<ApplicationSupervisorHealth>,
    },
}

#[ractor::async_trait]
impl Actor for ApplicationSupervisor {
    type Msg = ApplicationSupervisorMsg;
    type State = ApplicationState;
    type Arguments = ActorRef<EventStoreMsg>;

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            supervisor = %myself.get_id(),
            event = ?event,
            "ApplicationSupervisor received supervision event"
        );
        match &event {
            SupervisionEvent::ActorStarted(_) => {
                state.supervision_event_counts.actor_started += 1;
            }
            SupervisionEvent::ActorFailed(actor_cell, failure) => {
                state.supervision_event_counts.actor_failed += 1;
                state.last_supervision_failure =
                    Some(format!("actor_id={} error={failure}", actor_cell.get_id()));
            }
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                state.supervision_event_counts.actor_terminated += 1;
                let mut event_bus_terminated = false;
                let mut event_relay_terminated = false;

                if let Some(session_supervisor) = &state.session_supervisor {
                    if session_supervisor.get_id() == actor_cell.get_id() {
                        state.session_supervisor = None;
                    }
                }
                if let Some(event_bus) = &state.event_bus {
                    if event_bus.get_id() == actor_cell.get_id() {
                        state.event_bus = None;
                        event_bus_terminated = true;
                    }
                }
                if let Some(event_relay) = &state.event_relay {
                    if event_relay.get_id() == actor_cell.get_id() {
                        state.event_relay = None;
                        event_relay_terminated = true;
                    }
                }

                if event_bus_terminated {
                    match Actor::spawn_linked(
                        None,
                        EventBusActor,
                        EventBusArguments {
                            event_store: None,
                            config: EventBusConfig::default(),
                        },
                        myself.get_cell(),
                    )
                    .await
                    {
                        Ok((event_bus, _)) => {
                            tracing::info!(
                                event_bus_id = %event_bus.get_id(),
                                "respawned EventBusActor after termination"
                            );
                            state.event_bus = Some(event_bus.clone());
                            if let Some(event_relay) = state.event_relay.clone() {
                                let _ = ractor::cast!(
                                    event_relay,
                                    EventRelayMsg::SetEventBus {
                                        event_bus: event_bus.clone(),
                                    }
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to respawn EventBusActor");
                        }
                    }
                }

                if event_relay_terminated {
                    if let Some(event_bus) = state.event_bus.clone() {
                        match Actor::spawn_linked(
                            None,
                            EventRelayActor,
                            EventRelayArguments {
                                event_store: state.event_store.clone(),
                                event_bus,
                                poll_interval_ms: 120,
                            },
                            myself.get_cell(),
                        )
                        .await
                        {
                            Ok((event_relay, _)) => {
                                tracing::info!(
                                    event_relay_id = %event_relay.get_id(),
                                    "respawned EventRelayActor after termination"
                                );
                                state.event_relay = Some(event_relay);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "failed to respawn EventRelayActor");
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        if let Some(event_bus) = state.event_bus.clone() {
            let supervision_event = match Event::new(
                EventType::Custom("supervision.event".to_string()),
                "supervisor.application.supervision",
                serde_json::json!({
                    "supervisor_id": myself.get_id().to_string(),
                    "event_debug": format!("{event:?}"),
                }),
                "application_supervisor",
            ) {
                Ok(event) => event,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to build supervision event payload");
                    return Ok(());
                }
            };

            if let Err(e) = ractor::cast!(
                event_bus,
                EventBusMsg::Publish {
                    event: supervision_event,
                    persist: false,
                }
            ) {
                tracing::warn!(error = %e, "Failed to publish supervision event");
            }
        }
        Ok(())
    }

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        event_store: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(
            supervisor = %myself.get_id(),
            "ApplicationSupervisor starting"
        );

        // Spawn EventBusActor as a supervised child for pub/sub and correlation-aware tracing.
        let event_bus_args = EventBusArguments {
            event_store: None,
            config: EventBusConfig::default(),
        };

        let (event_bus, _handle) = Actor::spawn_linked(
            None, // No fixed name - allows multiple supervisors in tests
            EventBusActor,
            event_bus_args,
            myself.get_cell(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to spawn EventBusActor: {}", e);
            ActorProcessingErr::from(e)
        })?;

        // Spawn SessionSupervisor as a supervised child
        let session_supervisor_args = SessionSupervisorArgs {
            event_store: event_store.clone(),
            application_supervisor: myself.clone(),
        };

        let (session_supervisor, _handle) = Actor::spawn_linked(
            None, // No fixed name - allows multiple supervisors in tests
            SessionSupervisor,
            session_supervisor_args,
            myself.get_cell(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to spawn SessionSupervisor: {}", e);
            ActorProcessingErr::from(e)
        })?;

        info!(
            session_supervisor = %session_supervisor.get_id(),
            "SessionSupervisor spawned as child"
        );

        // Spawn EventRelayActor as a supervised child to relay committed EventStore events
        // to EventBus (ADR-0001).
        let relay_args = EventRelayArguments {
            event_store: event_store.clone(),
            event_bus: event_bus.clone(),
            poll_interval_ms: 120,
        };
        let (event_relay, _handle) =
            Actor::spawn_linked(None, EventRelayActor, relay_args, myself.get_cell())
                .await
                .map_err(|e| {
                    tracing::error!("Failed to spawn EventRelayActor: {}", e);
                    ActorProcessingErr::from(e)
                })?;

        Ok(ApplicationState {
            event_store,
            event_bus: Some(event_bus),
            event_relay: Some(event_relay),
            session_supervisor: Some(session_supervisor),
            supervision_event_counts: SupervisionEventCounts::default(),
            last_supervision_failure: None,
            worker_signal_policy: WorkerSignalPolicy::from_env(),
            recent_signal_keys: VecDeque::new(),
            escalation_cooldowns: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ApplicationSupervisorMsg::Supervision(event) => {
                self.handle_supervisor_evt(myself, event, state).await?;
            }
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.desktop.get_or_create.started",
                    EventType::Custom("supervisor.desktop.get_or_create.started".to_string()),
                    serde_json::json!({
                        "desktop_id": desktop_id,
                        "user_id": user_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                )
                .await;

                if let Some(ref session_supervisor) = state.session_supervisor {
                    let desktop_args = crate::actors::desktop::DesktopArguments {
                        desktop_id: desktop_id.clone(),
                        user_id: user_id.clone(),
                        event_store: state.event_store.clone(),
                    };

                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateDesktop {
                            desktop_id: desktop_id.clone(),
                            user_id: user_id.clone(),
                            args: desktop_args,
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.desktop.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.desktop.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "desktop_id": desktop_id,
                                    "user_id": user_id,
                                    "actor_id": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.desktop.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.desktop.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "desktop_id": desktop_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            error!(
                                desktop_id = %desktop_id,
                                error = %e,
                                "Failed to get or create desktop via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.chat.get_or_create.started",
                    EventType::Custom("supervisor.chat.get_or_create.started".to_string()),
                    serde_json::json!({
                        "actor_id": actor_id,
                        "user_id": user_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                )
                .await;

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChat {
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.chat.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "actor_id": actor_id,
                                    "user_id": user_id,
                                    "chat_actor_ref": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.chat.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "actor_id": actor_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            error!(
                                actor_id = %actor_id,
                                error = %e,
                                "Failed to get or create chat via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                chat_actor_id,
                user_id,
                preload_session_id,
                preload_thread_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.chat_agent.get_or_create.started",
                    EventType::Custom("supervisor.chat_agent.get_or_create.started".to_string()),
                    serde_json::json!({
                        "agent_id": agent_id,
                        "chat_actor_id": chat_actor_id,
                        "user_id": user_id,
                        "preload_session_id": preload_session_id,
                        "preload_thread_id": preload_thread_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                )
                .await;

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateChatAgent {
                            agent_id: agent_id.clone(),
                            chat_actor_id: chat_actor_id.clone(),
                            user_id: user_id.clone(),
                            preload_session_id: preload_session_id.clone(),
                            preload_thread_id: preload_thread_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(actor_ref) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat_agent.get_or_create.completed",
                                EventType::Custom(
                                    "supervisor.chat_agent.get_or_create.completed".to_string(),
                                ),
                                serde_json::json!({
                                    "agent_id": agent_id,
                                    "chat_actor_id": chat_actor_id,
                                    "user_id": user_id,
                                    "chat_agent_ref": actor_ref.get_id().to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            let _ = reply.send(actor_ref);
                        }
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.chat_agent.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.chat_agent.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "agent_id": agent_id,
                                    "chat_actor_id": chat_actor_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            error!(
                                agent_id = %agent_id,
                                error = %e,
                                "Failed to get or create chat agent via SessionSupervisor"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
            ApplicationSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.terminal.get_or_create.started",
                    EventType::Custom("supervisor.terminal.get_or_create.started".to_string()),
                    serde_json::json!({
                        "terminal_id": terminal_id,
                        "user_id": user_id,
                        "shell": shell,
                        "working_dir": working_dir,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                )
                .await;

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: shell.clone(),
                            working_dir: working_dir.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(result) => match result {
                            Ok(actor_ref) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.terminal.get_or_create.completed",
                                    EventType::Custom(
                                        "supervisor.terminal.get_or_create.completed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "terminal_id": terminal_id,
                                        "user_id": user_id,
                                        "terminal_ref": actor_ref.get_id().to_string(),
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                )
                                .await;
                                let _ = reply.send(actor_ref);
                            }
                            Err(e) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.terminal.get_or_create.failed",
                                    EventType::Custom(
                                        "supervisor.terminal.get_or_create.failed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "terminal_id": terminal_id,
                                        "user_id": user_id,
                                        "error": e,
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                )
                                .await;
                                error!(
                                    terminal_id = %terminal_id,
                                    error = %e,
                                    "Failed to get or create terminal via SessionSupervisor"
                                );
                                return Err(ActorProcessingErr::from(std::io::Error::other(e)));
                            }
                        },
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.terminal.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.terminal.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "terminal_id": terminal_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            error!(
                                terminal_id = %terminal_id,
                                error = %e,
                                "Failed to reach SessionSupervisor for terminal"
                            );
                            return Err(ActorProcessingErr::from(e));
                        }
                    }
                } else {
                    error!("SessionSupervisor not available");
                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        "SessionSupervisor not available",
                    )));
                }
            }
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
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                let task_id = ulid::Ulid::new().to_string();
                let task = shared_types::DelegatedTask {
                    task_id: task_id.clone(),
                    correlation_id: correlation_id.clone(),
                    actor_id: actor_id.clone(),
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    kind: shared_types::DelegatedTaskKind::TerminalCommand,
                    payload: serde_json::json!({
                        "command": command,
                        "shell": shell,
                        "working_dir": working_dir,
                        "objective": objective.clone(),
                        "user_id": user_id,
                        "timeout_ms": timeout_ms,
                        "model_override": model_override.clone(),
                    }),
                };
                let event_store = state.event_store.clone();

                Self::publish_worker_event(
                    event_store.clone(),
                    state.event_bus.clone(),
                    shared_types::EVENT_TOPIC_WORKER_TASK_STARTED,
                    EventType::WorkerSpawned,
                    serde_json::json!({
                        "task": task,
                        "status": shared_types::DelegatedTaskStatus::Accepted,
                        "model_requested": model_override.clone(),
                        "emitter_actor": "application_supervisor",
                        "started_at": chrono::Utc::now().to_rfc3339(),
                    }),
                    correlation_id.clone(),
                    actor_id.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                );

                let session_supervisor = match &state.session_supervisor {
                    Some(s) => s.clone(),
                    None => {
                        Self::publish_worker_event(
                            event_store.clone(),
                            state.event_bus.clone(),
                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                            EventType::WorkerFailed,
                            serde_json::json!({
                                "task_id": task_id,
                                "status": shared_types::DelegatedTaskStatus::Failed,
                                "error": "SessionSupervisor not available",
                                "emitter_actor": "application_supervisor",
                                "finished_at": chrono::Utc::now().to_rfc3339(),
                            }),
                            correlation_id,
                            actor_id.clone(),
                            session_id.clone(),
                            thread_id.clone(),
                        );
                        let _ = reply.send(Err("SessionSupervisor not available".to_string()));
                        return Ok(());
                    }
                };

                let event_bus = state.event_bus.clone();
                let task_for_reply = task.clone();
                let _ = reply.send(Ok(task_for_reply));

                tokio::spawn(async move {
                    let terminal_ref = match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: shell.clone(),
                            working_dir: working_dir.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(Ok(terminal_ref)) => terminal_ref,
                        Ok(Err(e)) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e,
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                        Err(e) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                    };

                    match ractor::call!(terminal_ref, |start_reply| TerminalMsg::Start {
                        reply: start_reply
                    }) {
                        Ok(Ok(()))
                        | Ok(Err(crate::actors::terminal::TerminalError::AlreadyRunning)) => {}
                        Ok(Err(e)) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                        Err(e) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus,
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id,
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                    }

                    let timeout_ms = timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000);
                    Self::publish_worker_event(
                        event_store.clone(),
                        event_bus.clone(),
                        shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                        EventType::WorkerProgress,
                        serde_json::json!({
                            "task_id": task_id,
                            "status": shared_types::DelegatedTaskStatus::Running,
                            "phase": "terminal_agent_dispatch",
                            "message": "terminal agent accepted request and is running",
                            "model_requested": model_override.clone(),
                            "emitter_actor": "application_supervisor",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                        correlation_id.clone(),
                        actor_id.clone(),
                        session_id.clone(),
                        thread_id.clone(),
                    );

                    let (result_tx, mut result_rx) = tokio::sync::oneshot::channel();
                    let (progress_tx, mut progress_rx) =
                        tokio::sync::mpsc::unbounded_channel::<TerminalAgentProgress>();
                    let terminal_ref_for_task = terminal_ref.clone();
                    let command_for_task = command.clone();
                    let model_override_for_task = model_override.clone();
                    tokio::spawn(async move {
                        let call_result = ractor::call!(terminal_ref_for_task, |agent_reply| {
                            TerminalMsg::RunBashTool {
                                request: crate::actors::terminal::TerminalBashToolRequest {
                                    cmd: command_for_task,
                                    timeout_ms: Some(timeout_ms),
                                    model_override: model_override_for_task,
                                    reasoning: Some(objective.clone().unwrap_or_else(|| {
                                        "Typed bash delegation from appactor->toolactor contract"
                                            .to_string()
                                    })),
                                },
                                progress_tx: Some(progress_tx),
                                reply: agent_reply,
                            }
                        });
                        let _ = result_tx.send(call_result);
                    });

                    let start_time = tokio::time::Instant::now();
                    let hard_deadline = start_time
                        + std::time::Duration::from_millis(timeout_ms.saturating_add(20_000));
                    let terminal_emitter_actor = format!("terminal:{terminal_id}");

                    loop {
                        tokio::select! {
                                Some(progress) = progress_rx.recv() => {
                                    let elapsed_ms = tokio::time::Instant::now()
                                        .duration_since(start_time)
                                        .as_millis() as u64;
                                    Self::publish_worker_event(
                        event_store.clone(),
                        event_bus.clone(),
                                        shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                                        EventType::WorkerProgress,
                                        serde_json::json!({
                                            "task_id": task_id,
                                            "status": shared_types::DelegatedTaskStatus::Running,
                                            "phase": progress.phase,
                                            "message": progress.message,
                                            "reasoning": progress.reasoning,
                                            "command": progress.command,
                                            "model_requested": model_override.clone(),
                                            "model_used": progress.model_used,
                                            "output_excerpt": progress.output_excerpt,
                                            "exit_code": progress.exit_code,
                                            "step_index": progress.step_index,
                                            "step_total": progress.step_total,
                                            "emitter_actor": terminal_emitter_actor.clone(),
                                            "elapsed_ms": elapsed_ms,
                                            "timestamp": progress.timestamp,
                                        }),
                                        correlation_id.clone(),
                                        actor_id.clone(),
                                        session_id.clone(),
                                        thread_id.clone(),
                                    );
                                }
                                _ = tokio::time::sleep_until(hard_deadline) => {
                                    let elapsed_ms = tokio::time::Instant::now()
                                        .duration_since(start_time)
                                        .as_millis() as u64;
                                    Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                        shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                        EventType::WorkerFailed,
                                        serde_json::json!({
                                            "task_id": task_id,
                                            "status": shared_types::DelegatedTaskStatus::Failed,
                                            "error": format!("terminal agent did not return within {}ms", timeout_ms.saturating_add(20_000)),
                                            "failure_kind": shared_types::FailureKind::Timeout,
                                            "failure_retriable": true,
                                            "failure_hint": "Terminal agent exceeded hard deadline. Check command latency and model/tool timeouts.",
                                            "failure_origin": "application_supervisor",
                                            "emitter_actor": "application_supervisor",
                                            "duration_ms": elapsed_ms,
                                            "finished_at": chrono::Utc::now().to_rfc3339(),
                                        }),
                                        correlation_id,
                                        actor_id.clone(),
                                        session_id.clone(),
                                        thread_id.clone(),
                                    );
                                    return;
                                }
                                result = &mut result_rx => {
                                    match result {
                                        Ok(Ok(Ok(result))) => {
                                            let elapsed_ms = tokio::time::Instant::now()
                                                .duration_since(start_time)
                                                .as_millis() as u64;
                                            if !result.success {
                                                let failure =
                                                    Self::classify_terminal_failure(result.exit_code);
                                                Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                                    shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                    EventType::WorkerFailed,
                                                    serde_json::json!({
                                                        "task_id": task_id,
                                                        "status": shared_types::DelegatedTaskStatus::Failed,
                                                        "error": match result.exit_code {
                                                            Some(code) => format!("terminal command exited with status {code}"),
                                                            None => "terminal agent task failed".to_string(),
                                                        },
                                                        "error_code": result.exit_code,
                                                        "failure_kind": failure.kind,
                                                        "failure_retriable": failure.retriable,
                                                        "failure_hint": failure.hint,
                                                        "failure_origin": "terminal_command",
                                                        "output": result.summary,
                                                        "reasoning": result.reasoning,
                                                        "model_requested": model_override.clone(),
                                                        "model_used": result.model_used,
                                                        "emitter_actor": terminal_emitter_actor.clone(),
                                                        "executed_commands": result.executed_commands,
                                                        "steps": result.steps,
                                                        "duration_ms": elapsed_ms,
                                                        "finished_at": chrono::Utc::now().to_rfc3339(),
                                                    }),
                                                    correlation_id,
                                                    actor_id.clone(),
                                                    session_id.clone(),
                                                    thread_id.clone(),
                                                );
                                                return;
                                            }
                                            Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                                shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED,
                                                EventType::WorkerComplete,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Completed,
                                                    "output": result.summary,
                                                    "reasoning": result.reasoning,
                                                    "model_requested": model_override.clone(),
                                                    "model_used": result.model_used,
                                                    "emitter_actor": terminal_emitter_actor.clone(),
                                                    "executed_commands": result.executed_commands,
                                                    "steps": result.steps,
                                                    "duration_ms": elapsed_ms,
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id,
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }
                                        Ok(Ok(Err(e))) => {
                                            let elapsed_ms = tokio::time::Instant::now()
                                                .duration_since(start_time)
                                                .as_millis() as u64;
                                            let failure = Self::classify_terminal_actor_error(&e);
                                            Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                EventType::WorkerFailed,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                                    "error": e.to_string(),
                                                    "failure_kind": failure.kind,
                                                    "failure_retriable": failure.retriable,
                                                    "failure_hint": failure.hint,
                                                    "failure_origin": "terminal_actor",
                                                    "emitter_actor": "application_supervisor",
                                                    "duration_ms": elapsed_ms,
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id,
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }
                                        Ok(Err(e)) => {
                                            let elapsed_ms = tokio::time::Instant::now()
                                                .duration_since(start_time)
                                                .as_millis() as u64;
                                            Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                EventType::WorkerFailed,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                                    "error": e.to_string(),
                                                    "failure_kind": shared_types::FailureKind::Unknown,
                                                    "failure_retriable": true,
                                                    "failure_hint": "Check actor RPC path and backpressure; retry may succeed if transient.",
                                                    "failure_origin": "application_supervisor",
                                                    "emitter_actor": "application_supervisor",
                                                    "duration_ms": elapsed_ms,
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id,
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }
                                        Err(_) => {
                                            let elapsed_ms = tokio::time::Instant::now()
                                                .duration_since(start_time)
                                                .as_millis() as u64;
                                            Self::publish_worker_event(
                        event_store.clone(),
                        event_bus,
                                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                EventType::WorkerFailed,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                                    "error": "terminal agent result channel closed".to_string(),
                                                    "failure_kind": shared_types::FailureKind::Unknown,
                                                    "failure_retriable": true,
                                                    "failure_hint": "Terminal task channel closed early; check runtime cancellation and actor lifetimes.",
                                                    "failure_origin": "application_supervisor",
                                                    "emitter_actor": "application_supervisor",
                                                    "duration_ms": elapsed_ms,
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id,
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }
                                    }
                                }
                            }
                    }
                });
            }
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
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                let task_id = ulid::Ulid::new().to_string();
                let task = shared_types::DelegatedTask {
                    task_id: task_id.clone(),
                    correlation_id: correlation_id.clone(),
                    actor_id: actor_id.clone(),
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    kind: shared_types::DelegatedTaskKind::ResearchQuery,
                    payload: serde_json::json!({
                        "query": query,
                        "objective": objective,
                        "provider": provider,
                        "max_results": max_results,
                        "time_range": time_range,
                        "include_domains": include_domains,
                        "exclude_domains": exclude_domains,
                        "timeout_ms": timeout_ms,
                        "model_override": model_override.clone(),
                        "reasoning": reasoning,
                        "user_id": user_id,
                    }),
                };

                Self::publish_worker_event(
                    state.event_store.clone(),
                    state.event_bus.clone(),
                    shared_types::EVENT_TOPIC_WORKER_TASK_STARTED,
                    EventType::WorkerSpawned,
                    serde_json::json!({
                        "task": task,
                        "status": shared_types::DelegatedTaskStatus::Accepted,
                        "model_requested": model_override.clone(),
                        "emitter_actor": "application_supervisor",
                        "started_at": chrono::Utc::now().to_rfc3339(),
                    }),
                    correlation_id.clone(),
                    actor_id.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                );

                Self::publish_worker_event(
                    state.event_store.clone(),
                    state.event_bus.clone(),
                    shared_types::EVENT_TOPIC_RESEARCH_TASK_STARTED,
                    EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_STARTED.to_string()),
                    serde_json::json!({
                        "task_id": task_id.clone(),
                        "status": "accepted",
                        "query": query,
                        "objective": objective,
                        "provider": provider,
                        "max_results": max_results,
                        "time_range": time_range,
                        "include_domains": include_domains,
                        "exclude_domains": exclude_domains,
                        "model_requested": model_override.clone(),
                        "emitter_actor": "application_supervisor",
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }),
                    correlation_id.clone(),
                    actor_id.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                );

                let session_supervisor = match &state.session_supervisor {
                    Some(s) => s.clone(),
                    None => {
                        Self::publish_worker_event(
                            state.event_store.clone(),
                            state.event_bus.clone(),
                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                            EventType::WorkerFailed,
                            serde_json::json!({
                                "task_id": task_id,
                                "status": shared_types::DelegatedTaskStatus::Failed,
                                "error": "SessionSupervisor not available",
                                "failure_kind": shared_types::FailureKind::Unknown,
                                "failure_retriable": true,
                                "failure_hint": "SessionSupervisor missing; restart supervisor tree and retry.",
                                "failure_origin": "application_supervisor",
                                "emitter_actor": "application_supervisor",
                                "finished_at": chrono::Utc::now().to_rfc3339(),
                            }),
                            correlation_id.clone(),
                            actor_id.clone(),
                            session_id.clone(),
                            thread_id.clone(),
                        );
                        let _ = reply.send(Err("SessionSupervisor not available".to_string()));
                        return Ok(());
                    }
                };

                let event_store = state.event_store.clone();
                let event_bus = state.event_bus.clone();
                let myself_ref = myself.clone();
                let task_for_reply = task.clone();
                let _ = reply.send(Ok(task_for_reply));

                tokio::spawn(async move {
                    let researcher_ref = match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateResearcher {
                            researcher_id: researcher_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(Ok(researcher_ref)) => researcher_ref,
                        Ok(Err(e)) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus.clone(),
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e,
                                    "failure_kind": shared_types::FailureKind::Provider,
                                    "failure_retriable": true,
                                    "failure_hint": "Researcher actor failed to start; inspect supervisor logs.",
                                    "failure_origin": "session_supervisor",
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id.clone(),
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                        Err(e) => {
                            Self::publish_worker_event(
                                event_store.clone(),
                                event_bus.clone(),
                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                EventType::WorkerFailed,
                                serde_json::json!({
                                    "task_id": task_id,
                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                    "error": e.to_string(),
                                    "failure_kind": shared_types::FailureKind::Unknown,
                                    "failure_retriable": true,
                                    "failure_hint": "Session supervisor RPC failed while acquiring Researcher actor.",
                                    "failure_origin": "application_supervisor",
                                    "emitter_actor": "application_supervisor",
                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                }),
                                correlation_id.clone(),
                                actor_id.clone(),
                                session_id.clone(),
                                thread_id.clone(),
                            );
                            return;
                        }
                    };

                    let timeout_ms = timeout_ms.unwrap_or(45_000).clamp(3_000, 120_000);
                    Self::publish_worker_event(
                        event_store.clone(),
                        event_bus.clone(),
                        shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                        EventType::WorkerProgress,
                        serde_json::json!({
                            "task_id": task_id,
                            "status": shared_types::DelegatedTaskStatus::Running,
                            "phase": "researcher_actor_dispatch",
                            "message": "researcher actor accepted request and is running",
                            "model_requested": model_override.clone(),
                            "emitter_actor": "application_supervisor",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                        correlation_id.clone(),
                        actor_id.clone(),
                        session_id.clone(),
                        thread_id.clone(),
                    );

                    let (result_tx, mut result_rx) = tokio::sync::oneshot::channel();
                    let (progress_tx, mut progress_rx) =
                        tokio::sync::mpsc::unbounded_channel::<ResearcherProgress>();
                    let researcher_ref_for_task = researcher_ref.clone();
                    let query_for_task = query.clone();
                    let objective_for_task = objective.clone();
                    let provider_for_task = provider.clone();
                    let time_range_for_task = time_range.clone();
                    let include_domains_for_task = include_domains.clone();
                    let exclude_domains_for_task = exclude_domains.clone();
                    let model_override_for_task = model_override.clone();
                    let reasoning_for_task = reasoning.clone();

                    tokio::spawn(async move {
                        let call_result =
                            ractor::call!(researcher_ref_for_task, |research_reply| {
                                ResearcherMsg::RunWebSearchTool {
                                    request: ResearcherWebSearchRequest {
                                        query: query_for_task,
                                        objective: objective_for_task,
                                        provider: provider_for_task,
                                        max_results,
                                        time_range: time_range_for_task,
                                        include_domains: include_domains_for_task,
                                        exclude_domains: exclude_domains_for_task,
                                        timeout_ms: Some(timeout_ms),
                                        model_override: model_override_for_task,
                                        reasoning: reasoning_for_task,
                                    },
                                    progress_tx: Some(progress_tx),
                                    reply: research_reply,
                                }
                            });
                        let _ = result_tx.send(call_result);
                    });

                    let start_time = tokio::time::Instant::now();
                    let hard_deadline = start_time
                        + std::time::Duration::from_millis(timeout_ms.saturating_add(20_000));
                    let researcher_emitter_actor = format!("researcher:{researcher_id}");

                    loop {
                        tokio::select! {
                            Some(progress) = progress_rx.recv() => {
                                let elapsed_ms = tokio::time::Instant::now()
                                    .duration_since(start_time)
                                    .as_millis() as u64;

                                Self::publish_worker_event(
                                    event_store.clone(),
                                    event_bus.clone(),
                                    shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS,
                                    EventType::WorkerProgress,
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "status": shared_types::DelegatedTaskStatus::Running,
                                        "phase": progress.phase.clone(),
                                        "message": progress.message.clone(),
                                        "provider": progress.provider.clone(),
                                        "model_requested": model_override.clone(),
                                        "model_used": progress.model_used.clone(),
                                        "result_count": progress.result_count,
                                        "emitter_actor": researcher_emitter_actor.clone(),
                                        "elapsed_ms": elapsed_ms,
                                        "timestamp": progress.timestamp,
                                    }),
                                    correlation_id.clone(),
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );

                                Self::publish_worker_event(
                                    event_store.clone(),
                                    event_bus.clone(),
                                    shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS,
                                    EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS.to_string()),
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "phase": progress.phase.clone(),
                                        "message": progress.message.clone(),
                                        "provider": progress.provider.clone(),
                                        "model_requested": model_override.clone(),
                                        "model_used": progress.model_used.clone(),
                                        "result_count": progress.result_count,
                                        "emitter_actor": researcher_emitter_actor.clone(),
                                        "elapsed_ms": elapsed_ms,
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                    }),
                                    correlation_id.clone(),
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );

                                match progress.phase.as_str() {
                                    "research_provider_call" => {
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_CALL,
                                            EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_CALL.to_string()),
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "provider": progress.provider.clone(),
                                                "message": progress.message.clone(),
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                    }
                                    "research_provider_result" => {
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_RESULT,
                                            EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_RESULT.to_string()),
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "provider": progress.provider.clone(),
                                                "result_count": progress.result_count,
                                                "message": progress.message.clone(),
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                    }
                                    "research_provider_error" => {
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_ERROR,
                                            EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_PROVIDER_ERROR.to_string()),
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "provider": progress.provider.clone(),
                                                "message": progress.message.clone(),
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            _ = tokio::time::sleep_until(hard_deadline) => {
                                let elapsed_ms = tokio::time::Instant::now()
                                    .duration_since(start_time)
                                    .as_millis() as u64;
                                Self::publish_worker_event(
                                    event_store.clone(),
                                    event_bus.clone(),
                                    shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED,
                                    EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED.to_string()),
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "status": "failed",
                                        "error": format!("research task did not return within {}ms", timeout_ms.saturating_add(20_000)),
                                        "failure_kind": shared_types::FailureKind::Timeout,
                                        "failure_retriable": true,
                                        "failure_hint": "Research task exceeded hard deadline; inspect provider latency and key health.",
                                        "failure_origin": "application_supervisor",
                                        "emitter_actor": "application_supervisor",
                                        "duration_ms": elapsed_ms,
                                        "finished_at": chrono::Utc::now().to_rfc3339(),
                                    }),
                                    correlation_id.clone(),
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );
                                Self::publish_worker_event(
                                    event_store.clone(),
                                    event_bus.clone(),
                                    shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                    EventType::WorkerFailed,
                                    serde_json::json!({
                                        "task_id": task_id,
                                        "status": shared_types::DelegatedTaskStatus::Failed,
                                        "error": format!("research task did not return within {}ms", timeout_ms.saturating_add(20_000)),
                                        "failure_kind": shared_types::FailureKind::Timeout,
                                        "failure_retriable": true,
                                        "failure_hint": "Research task exceeded hard deadline; inspect provider latency and key health.",
                                        "failure_origin": "application_supervisor",
                                        "model_requested": model_override.clone(),
                                        "emitter_actor": "application_supervisor",
                                        "duration_ms": elapsed_ms,
                                        "finished_at": chrono::Utc::now().to_rfc3339(),
                                    }),
                                    correlation_id.clone(),
                                    actor_id.clone(),
                                    session_id.clone(),
                                    thread_id.clone(),
                                );
                                return;
                            }
                            result = &mut result_rx => {
                                match result {
                                    Ok(Ok(Ok(result))) => {
                                        let elapsed_ms = tokio::time::Instant::now()
                                            .duration_since(start_time)
                                            .as_millis() as u64;

                                        let mut final_result = result;

                                        if Self::research_terminal_escalation_enabled()
                                            && Self::should_escalate_research_result(&final_result)
                                        {
                                            let escalation_timeout_ms = Self::research_terminal_escalation_timeout_ms(timeout_ms);
                                            let escalation_objective = Self::build_research_terminal_objective(&query, &final_result);

                                            Self::publish_worker_event(
                                                event_store.clone(),
                                                event_bus.clone(),
                                                shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS,
                                                EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS.to_string()),
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "phase": "research_terminal_escalation_call",
                                                    "message": "research objective incomplete; escalating to terminal capability",
                                                    "model_requested": model_override.clone(),
                                                    "model_used": final_result.model_used.clone(),
                                                    "objective_status": final_result.objective_status,
                                                    "completion_reason": final_result.completion_reason,
                                                    "recommended_next_capability": final_result.recommended_next_capability,
                                                    "timeout_ms": escalation_timeout_ms,
                                                    "emitter_actor": "application_supervisor",
                                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id.clone(),
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );

                                            match Self::run_research_terminal_escalation(
                                                session_supervisor.clone(),
                                                actor_id.clone(),
                                                user_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                                model_override.clone(),
                                                escalation_timeout_ms,
                                                escalation_objective,
                                            )
                                            .await
                                            {
                                                Ok(terminal_result) => {
                                                    let escalation_summary = terminal_result.summary.clone();
                                                    let mut provider_calls = final_result.provider_calls.clone();
                                                    provider_calls.push(ResearchProviderCall {
                                                        provider: "terminal_escalation".to_string(),
                                                        latency_ms: escalation_timeout_ms,
                                                        result_count: terminal_result.steps.len(),
                                                        succeeded: terminal_result.success,
                                                        error: if terminal_result.success {
                                                            None
                                                        } else {
                                                            Some(terminal_result.summary.clone())
                                                        },
                                                    });

                                                    final_result.summary = escalation_summary;
                                                    final_result.provider_used = Some(match final_result.provider_used {
                                                        Some(existing) => format!("{existing}+terminal"),
                                                        None => "terminal".to_string(),
                                                    });
                                                    final_result.model_used = terminal_result
                                                        .model_used
                                                        .clone()
                                                        .or(final_result.model_used);
                                                    final_result.provider_calls = provider_calls;
                                                    final_result.objective_status = if terminal_result.success {
                                                        ResearchObjectiveStatus::Complete
                                                    } else {
                                                        ResearchObjectiveStatus::Incomplete
                                                    };
                                                    final_result.completion_reason = if terminal_result.success {
                                                        "Research completed via terminal escalation.".to_string()
                                                    } else {
                                                        "Terminal escalation did not fully complete objective.".to_string()
                                                    };
                                                    final_result.recommended_next_capability = if terminal_result.success {
                                                        None
                                                    } else {
                                                        Some("conductor".to_string())
                                                    };
                                                    final_result.recommended_next_objective = if terminal_result.success {
                                                        None
                                                    } else {
                                                        Some("Review terminal escalation output and reprompt with tighter constraints.".to_string())
                                                    };

                                                    Self::publish_worker_event(
                                                        event_store.clone(),
                                                        event_bus.clone(),
                                                        shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS,
                                                        EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS.to_string()),
                                                        serde_json::json!({
                                                            "task_id": task_id,
                                                            "phase": "research_terminal_escalation_result",
                                                            "message": if terminal_result.success {
                                                                "terminal escalation completed objective"
                                                            } else {
                                                                "terminal escalation returned partial result"
                                                            },
                                                            "model_requested": model_override.clone(),
                                                            "model_used": final_result.model_used.clone(),
                                                            "objective_status": final_result.objective_status,
                                                            "completion_reason": final_result.completion_reason,
                                                            "emitter_actor": "application_supervisor",
                                                            "timestamp": chrono::Utc::now().to_rfc3339(),
                                                        }),
                                                        correlation_id.clone(),
                                                        actor_id.clone(),
                                                        session_id.clone(),
                                                        thread_id.clone(),
                                                    );
                                                }
                                                Err(escalation_error) => {
                                                    Self::publish_worker_event(
                                                        event_store.clone(),
                                                        event_bus.clone(),
                                                        shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS,
                                                        EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS.to_string()),
                                                        serde_json::json!({
                                                            "task_id": task_id,
                                                            "phase": "research_terminal_escalation_failed",
                                                            "message": format!("terminal escalation failed: {}", escalation_error),
                                                            "model_requested": model_override.clone(),
                                                            "model_used": final_result.model_used.clone(),
                                                            "objective_status": final_result.objective_status,
                                                            "completion_reason": final_result.completion_reason,
                                                            "emitter_actor": "application_supervisor",
                                                            "timestamp": chrono::Utc::now().to_rfc3339(),
                                                        }),
                                                        correlation_id.clone(),
                                                        actor_id.clone(),
                                                        session_id.clone(),
                                                        thread_id.clone(),
                                                    );
                                                }
                                            }
                                        }

                                        if let Some(report) = final_result.worker_report.clone() {
                                            let _ = ractor::call!(myself_ref.clone(), |ingest_reply| {
                                                ApplicationSupervisorMsg::IngestWorkerTurnReport {
                                                    actor_id: actor_id.clone(),
                                                    user_id: user_id.clone(),
                                                    session_id: session_id.clone(),
                                                    thread_id: thread_id.clone(),
                                                    report,
                                                    reply: ingest_reply,
                                                }
                                            });
                                        }

                                        if !final_result.success {
                                            Self::publish_worker_event(
                                                event_store.clone(),
                                                event_bus.clone(),
                                                shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED,
                                                EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED.to_string()),
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": "failed",
                                                    "error": final_result.error.clone().unwrap_or_else(|| "research task failed".to_string()),
                                                    "model_requested": model_override.clone(),
                                                    "model_used": final_result.model_used.clone(),
                                                    "provider_calls": final_result.provider_calls,
                                                    "objective_status": final_result.objective_status,
                                                    "completion_reason": final_result.completion_reason,
                                                    "recommended_next_capability": final_result.recommended_next_capability,
                                                    "recommended_next_objective": final_result.recommended_next_objective,
                                                    "duration_ms": elapsed_ms,
                                                    "emitter_actor": researcher_emitter_actor.clone(),
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id.clone(),
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            Self::publish_worker_event(
                                                event_store.clone(),
                                                event_bus.clone(),
                                                shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                                EventType::WorkerFailed,
                                                serde_json::json!({
                                                    "task_id": task_id,
                                                    "status": shared_types::DelegatedTaskStatus::Failed,
                                                    "error": final_result.error.unwrap_or_else(|| "research task failed".to_string()),
                                                    "failure_kind": shared_types::FailureKind::Provider,
                                                    "failure_retriable": true,
                                                    "failure_hint": "All configured providers failed or returned invalid responses.",
                                                    "failure_origin": "researcher_actor",
                                                    "model_requested": model_override.clone(),
                                                    "model_used": final_result.model_used,
                                                    "provider_calls": final_result.provider_calls,
                                                    "objective_status": final_result.objective_status,
                                                    "completion_reason": final_result.completion_reason,
                                                    "recommended_next_capability": final_result.recommended_next_capability,
                                                    "recommended_next_objective": final_result.recommended_next_objective,
                                                    "duration_ms": elapsed_ms,
                                                    "emitter_actor": researcher_emitter_actor.clone(),
                                                    "finished_at": chrono::Utc::now().to_rfc3339(),
                                                }),
                                                correlation_id.clone(),
                                                actor_id.clone(),
                                                session_id.clone(),
                                                thread_id.clone(),
                                            );
                                            return;
                                        }

                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED,
                                            EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED.to_string()),
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": "completed",
                                                "summary": final_result.summary.clone(),
                                                "provider_used": final_result.provider_used.clone(),
                                                "citations": final_result.citations.clone(),
                                                "model_requested": model_override.clone(),
                                                "model_used": final_result.model_used.clone(),
                                                "provider_calls": final_result.provider_calls.clone(),
                                                "objective_status": final_result.objective_status,
                                                "completion_reason": final_result.completion_reason,
                                                "recommended_next_capability": final_result.recommended_next_capability,
                                                "recommended_next_objective": final_result.recommended_next_objective,
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );

                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED,
                                            EventType::WorkerComplete,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Completed,
                                                "output": final_result.summary,
                                                "provider_used": final_result.provider_used,
                                                "citations": final_result.citations,
                                                "model_requested": model_override.clone(),
                                                "model_used": final_result.model_used,
                                                "provider_calls": final_result.provider_calls,
                                                "objective_status": final_result.objective_status,
                                                "completion_reason": final_result.completion_reason,
                                                "recommended_next_capability": final_result.recommended_next_capability,
                                                "recommended_next_objective": final_result.recommended_next_objective,
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Ok(Ok(Err(e))) => {
                                        let elapsed_ms = tokio::time::Instant::now()
                                            .duration_since(start_time)
                                            .as_millis() as u64;
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED,
                                            EventType::Custom(shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED.to_string()),
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": "failed",
                                                "error": e.to_string(),
                                                "failure_kind": shared_types::FailureKind::Provider,
                                                "failure_retriable": false,
                                                "failure_hint": "Researcher actor returned an error while processing web_search.",
                                                "failure_origin": "researcher_actor",
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": e.to_string(),
                                                "failure_kind": shared_types::FailureKind::Provider,
                                                "failure_retriable": false,
                                                "failure_hint": "Researcher actor returned an error while processing web_search.",
                                                "failure_origin": "researcher_actor",
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": researcher_emitter_actor.clone(),
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Ok(Err(e)) => {
                                        let elapsed_ms = tokio::time::Instant::now()
                                            .duration_since(start_time)
                                            .as_millis() as u64;
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": e.to_string(),
                                                "failure_kind": shared_types::FailureKind::Unknown,
                                                "failure_retriable": true,
                                                "failure_hint": "Researcher RPC failed; retry may succeed if transient.",
                                                "failure_origin": "application_supervisor",
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": "application_supervisor",
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                    Err(_) => {
                                        let elapsed_ms = tokio::time::Instant::now()
                                            .duration_since(start_time)
                                            .as_millis() as u64;
                                        Self::publish_worker_event(
                                            event_store.clone(),
                                            event_bus.clone(),
                                            shared_types::EVENT_TOPIC_WORKER_TASK_FAILED,
                                            EventType::WorkerFailed,
                                            serde_json::json!({
                                                "task_id": task_id,
                                                "status": shared_types::DelegatedTaskStatus::Failed,
                                                "error": "researcher result channel closed".to_string(),
                                                "failure_kind": shared_types::FailureKind::Unknown,
                                                "failure_retriable": true,
                                                "failure_hint": "Researcher result channel closed early; inspect actor lifecycle.",
                                                "failure_origin": "application_supervisor",
                                                "duration_ms": elapsed_ms,
                                                "emitter_actor": "application_supervisor",
                                                "finished_at": chrono::Utc::now().to_rfc3339(),
                                            }),
                                            correlation_id.clone(),
                                            actor_id.clone(),
                                            session_id.clone(),
                                            thread_id.clone(),
                                        );
                                        return;
                                    }
                                }
                            }
                        }
                    }
                });
            }
            ApplicationSupervisorMsg::IngestWorkerTurnReport {
                actor_id,
                user_id,
                session_id,
                thread_id,
                report,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                Self::publish_worker_event(
                    state.event_store.clone(),
                    state.event_bus.clone(),
                    shared_types::EVENT_TOPIC_WORKER_REPORT_RECEIVED,
                    EventType::Custom(shared_types::EVENT_TOPIC_WORKER_REPORT_RECEIVED.to_string()),
                    serde_json::json!({
                        "turn_id": report.turn_id.clone(),
                        "task_id": report.task_id.clone(),
                        "worker_id": report.worker_id.clone(),
                        "worker_role": report.worker_role.clone(),
                        "status": report.status.clone(),
                        "summary": report.summary.clone(),
                        "report": report.clone(),
                        "ingested_by": "application_supervisor",
                        "ingested_at": chrono::Utc::now().to_rfc3339(),
                        "requested_by": user_id,
                    }),
                    correlation_id.clone(),
                    actor_id.clone(),
                    session_id.clone(),
                    thread_id.clone(),
                );
                let ingest = Self::ingest_worker_turn_report(
                    state,
                    &actor_id,
                    report,
                    correlation_id,
                    session_id,
                    thread_id,
                );
                let _ = reply.send(Ok(ingest));
            }
            ApplicationSupervisorMsg::GetHealth { reply } => {
                let _ = reply.send(ApplicationSupervisorHealth {
                    event_bus_healthy: state.event_bus.is_some(),
                    event_relay_healthy: state.event_relay.is_some(),
                    session_supervisor_healthy: state.session_supervisor.is_some(),
                    supervision_event_counts: state.supervision_event_counts.clone(),
                    last_supervision_failure: state.last_supervision_failure.clone(),
                });
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        info!(supervisor = %myself.get_id(), "ApplicationSupervisor stopping");

        Ok(())
    }
}

impl ApplicationSupervisor {
    fn research_terminal_escalation_enabled() -> bool {
        match std::env::var("CHOIR_RESEARCH_ENABLE_TERMINAL_ESCALATION")
            .unwrap_or_else(|_| "1".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "0" | "false" | "off" | "no" => false,
            _ => true,
        }
    }

    fn research_terminal_escalation_timeout_ms(default_timeout_ms: u64) -> u64 {
        let fallback = default_timeout_ms
            .saturating_add(15_000)
            .clamp(8_000, 90_000);
        std::env::var("CHOIR_RESEARCH_TERMINAL_ESCALATION_TIMEOUT_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .map(|v| v.clamp(8_000, 120_000))
            .unwrap_or(fallback)
    }

    fn research_terminal_escalation_max_steps() -> u8 {
        std::env::var("CHOIR_RESEARCH_TERMINAL_ESCALATION_MAX_STEPS")
            .ok()
            .and_then(|raw| raw.parse::<u8>().ok())
            .map(|v| v.clamp(1, 8))
            .unwrap_or(4)
    }

    fn should_escalate_research_result(result: &ResearcherResult) -> bool {
        if !result.success {
            return false;
        }
        if result.recommended_next_capability.as_deref() != Some("terminal") {
            return false;
        }
        matches!(
            result.objective_status,
            ResearchObjectiveStatus::Incomplete | ResearchObjectiveStatus::Blocked
        )
    }

    fn build_research_terminal_objective(query: &str, result: &ResearcherResult) -> String {
        let citations_preview = result
            .citations
            .iter()
            .take(5)
            .map(|citation| format!("- {} ({})", citation.title.trim(), citation.url.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        let recommended_objective = result
            .recommended_next_objective
            .clone()
            .unwrap_or_else(|| query.to_string());

        format!(
            "Objective: Complete this user research request with current, verifiable facts.\n\
             User query: {query}\n\
             Research status: {:?}\n\
             Completion reason: {}\n\
             Prior citations (may be incomplete/noisy):\n{}\n\
             Follow-up objective: {}\n\
             Instructions: use minimal safe terminal commands to verify time-sensitive facts and produce a concise final answer with key facts.",
            result.objective_status,
            result.completion_reason,
            citations_preview,
            recommended_objective
        )
    }

    async fn run_research_terminal_escalation(
        session_supervisor: ActorRef<SessionSupervisorMsg>,
        actor_id: String,
        user_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        model_override: Option<String>,
        timeout_ms: u64,
        objective: String,
    ) -> Result<TerminalAgentResult, String> {
        let terminal_id = match (&session_id, &thread_id) {
            (Some(session_id), Some(thread_id)) => {
                format!("term:{}:{}:{}", actor_id, session_id, thread_id)
            }
            _ => format!("term:{}", actor_id),
        };

        let terminal_ref = ractor::call!(session_supervisor, |ss_reply| {
            SessionSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell: "/bin/zsh".to_string(),
                working_dir: ".".to_string(),
                reply: ss_reply,
            }
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        ractor::call!(terminal_ref, |terminal_reply| TerminalMsg::RunAgenticTask {
            objective,
            timeout_ms: Some(timeout_ms),
            max_steps: Some(Self::research_terminal_escalation_max_steps()),
            model_override,
            progress_tx: None,
            reply: terminal_reply,
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }

    fn classify_terminal_failure(exit_code: Option<i32>) -> FailureClassification {
        match exit_code {
            Some(6) | Some(7) | Some(35) | Some(52) | Some(56) => FailureClassification {
                kind: shared_types::FailureKind::Network,
                retriable: true,
                hint: "Network/API endpoint failure. Retry or switch endpoint/provider.",
            },
            Some(28) => FailureClassification {
                kind: shared_types::FailureKind::Timeout,
                retriable: true,
                hint: "Connection timeout. Retry and validate outbound network reachability.",
            },
            Some(126) => FailureClassification {
                kind: shared_types::FailureKind::Validation,
                retriable: false,
                hint: "Command permission denied. Verify executable permissions and policy.",
            },
            Some(127) => FailureClassification {
                kind: shared_types::FailureKind::Validation,
                retriable: false,
                hint: "Command not found in runtime PATH.",
            },
            Some(130) => FailureClassification {
                kind: shared_types::FailureKind::Unknown,
                retriable: true,
                hint: "Process interrupted. Retry unless user/system cancellation was intentional.",
            },
            Some(137) => FailureClassification {
                kind: shared_types::FailureKind::Unknown,
                retriable: true,
                hint: "Process killed (likely resource or signal). Check host limits and retries.",
            },
            Some(_) => FailureClassification {
                kind: shared_types::FailureKind::Provider,
                retriable: false,
                hint: "Command returned non-zero exit. Inspect output and command arguments.",
            },
            None => FailureClassification {
                kind: shared_types::FailureKind::Unknown,
                retriable: false,
                hint: "Failure had no exit code. Inspect worker and terminal actor logs.",
            },
        }
    }

    fn classify_terminal_actor_error(error: &TerminalError) -> FailureClassification {
        match error {
            TerminalError::Timeout(_) => FailureClassification {
                kind: shared_types::FailureKind::Timeout,
                retriable: true,
                hint: "Terminal command timed out; retry with bounded command or longer timeout.",
            },
            TerminalError::InvalidInput(_) | TerminalError::PtyNotSupported => {
                FailureClassification {
                    kind: shared_types::FailureKind::Validation,
                    retriable: false,
                    hint:
                        "Invalid terminal request payload or runtime capability for delegated tool.",
                }
            }
            TerminalError::AlreadyRunning
            | TerminalError::NotRunning
            | TerminalError::SpawnFailed(_) => FailureClassification {
                kind: shared_types::FailureKind::Unknown,
                retriable: true,
                hint: "Terminal runtime state was not ready; retry after supervisor stabilization.",
            },
            TerminalError::Io(_) => FailureClassification {
                kind: shared_types::FailureKind::Unknown,
                retriable: true,
                hint: "Terminal I/O error occurred while executing delegated command.",
            },
        }
    }

    fn normalize_signal_key(value: &str) -> String {
        value
            .trim()
            .to_ascii_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn prune_signal_windows(state: &mut ApplicationState, now: chrono::DateTime<chrono::Utc>) {
        let duplicate_window = state.worker_signal_policy.duplicate_window_seconds.max(1);
        while let Some((_, seen_at)) = state.recent_signal_keys.front() {
            if now.signed_duration_since(*seen_at).num_seconds() > duplicate_window {
                state.recent_signal_keys.pop_front();
            } else {
                break;
            }
        }

        let escalation_window = state
            .worker_signal_policy
            .escalation_cooldown_seconds
            .max(1);
        state.escalation_cooldowns.retain(|_, seen_at| {
            now.signed_duration_since(*seen_at).num_seconds() <= escalation_window
        });
    }

    fn is_recent_duplicate(state: &ApplicationState, key: &str) -> bool {
        state
            .recent_signal_keys
            .iter()
            .any(|(existing, _)| existing == key)
    }

    fn remember_signal_key(
        state: &mut ApplicationState,
        key: String,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        state.recent_signal_keys.push_back((key, now));
    }

    fn emit_worker_signal_rejection(
        state: &ApplicationState,
        source_actor_id: &str,
        correlation_id: &str,
        session_id: Option<String>,
        thread_id: Option<String>,
        rejection: &shared_types::WorkerSignalRejection,
    ) {
        Self::publish_worker_event(
            state.event_store.clone(),
            state.event_bus.clone(),
            shared_types::EVENT_TOPIC_WORKER_SIGNAL_REJECTED,
            EventType::Custom(shared_types::EVENT_TOPIC_WORKER_SIGNAL_REJECTED.to_string()),
            serde_json::json!({
                "signal_type": rejection.signal_type,
                "signal_id": rejection.signal_id,
                "reason": rejection.reason,
                "detail": rejection.detail,
                "rejected_at": chrono::Utc::now().to_rfc3339(),
            }),
            correlation_id.to_string(),
            source_actor_id.to_string(),
            session_id,
            thread_id,
        );
    }

    fn ingest_worker_turn_report(
        state: &mut ApplicationState,
        source_actor_id: &str,
        report: shared_types::WorkerTurnReport,
        correlation_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> shared_types::WorkerTurnReportIngestResult {
        let policy = state.worker_signal_policy.clone();
        let now = chrono::Utc::now();
        Self::prune_signal_windows(state, now);
        let turn_id = report.turn_id.clone();
        let task_id = report.task_id.clone();
        let worker_id = report.worker_id.clone();
        let worker_role = report.worker_role.clone();
        let status = report.status.clone();

        let mut ingest = shared_types::WorkerTurnReportIngestResult {
            accepted_findings: 0,
            accepted_learnings: 0,
            accepted_escalations: 0,
            accepted_artifacts: 0,
            escalation_notified: false,
            rejections: Vec::new(),
        };

        for (idx, finding) in report.findings.iter().enumerate() {
            let reject = if idx >= policy.max_findings_per_turn {
                Some((
                    shared_types::WorkerSignalRejectReason::MaxPerTurnExceeded,
                    format!("max findings per turn is {}", policy.max_findings_per_turn),
                ))
            } else if finding.claim.trim().is_empty() {
                Some((
                    shared_types::WorkerSignalRejectReason::InvalidPayload,
                    "finding claim is empty".to_string(),
                ))
            } else if finding.evidence_refs.is_empty() {
                Some((
                    shared_types::WorkerSignalRejectReason::MissingEvidence,
                    "finding requires at least one evidence reference".to_string(),
                ))
            } else if finding.confidence < policy.min_confidence {
                Some((
                    shared_types::WorkerSignalRejectReason::LowConfidence,
                    format!(
                        "confidence {} below threshold {}",
                        finding.confidence, policy.min_confidence
                    ),
                ))
            } else {
                let dedup_key = format!("finding:{}", Self::normalize_signal_key(&finding.claim));
                if Self::is_recent_duplicate(state, &dedup_key) {
                    Some((
                        shared_types::WorkerSignalRejectReason::DuplicateWithinWindow,
                        "duplicate finding within dedup window".to_string(),
                    ))
                } else {
                    Self::remember_signal_key(state, dedup_key, now);
                    None
                }
            };

            if let Some((reason, detail)) = reject {
                ingest.rejections.push(shared_types::WorkerSignalRejection {
                    signal_type: shared_types::WorkerSignalType::Finding,
                    signal_id: finding.finding_id.clone(),
                    reason,
                    detail,
                });
                continue;
            }

            ingest.accepted_findings += 1;
            let topic = if report.worker_role.as_deref() == Some("researcher") {
                shared_types::EVENT_TOPIC_RESEARCH_FINDING_CREATED
            } else {
                shared_types::EVENT_TOPIC_WORKER_FINDING_CREATED
            };
            Self::publish_worker_event(
                state.event_store.clone(),
                state.event_bus.clone(),
                topic,
                EventType::Custom(topic.to_string()),
                serde_json::json!({
                    "turn_id": turn_id.clone(),
                    "task_id": task_id.clone(),
                    "worker_id": worker_id.clone(),
                    "worker_role": worker_role.clone(),
                    "status": status.clone(),
                    "finding": finding,
                    "accepted_at": chrono::Utc::now().to_rfc3339(),
                }),
                correlation_id.clone(),
                source_actor_id.to_string(),
                session_id.clone(),
                thread_id.clone(),
            );
        }

        for (idx, learning) in report.learnings.iter().enumerate() {
            let reject = if idx >= policy.max_learnings_per_turn {
                Some((
                    shared_types::WorkerSignalRejectReason::MaxPerTurnExceeded,
                    format!(
                        "max learnings per turn is {}",
                        policy.max_learnings_per_turn
                    ),
                ))
            } else if learning.insight.trim().is_empty() {
                Some((
                    shared_types::WorkerSignalRejectReason::InvalidPayload,
                    "learning insight is empty".to_string(),
                ))
            } else if learning.confidence < policy.min_confidence {
                Some((
                    shared_types::WorkerSignalRejectReason::LowConfidence,
                    format!(
                        "confidence {} below threshold {}",
                        learning.confidence, policy.min_confidence
                    ),
                ))
            } else {
                let dedup_key =
                    format!("learning:{}", Self::normalize_signal_key(&learning.insight));
                if Self::is_recent_duplicate(state, &dedup_key) {
                    Some((
                        shared_types::WorkerSignalRejectReason::DuplicateWithinWindow,
                        "duplicate learning within dedup window".to_string(),
                    ))
                } else {
                    Self::remember_signal_key(state, dedup_key, now);
                    None
                }
            };

            if let Some((reason, detail)) = reject {
                ingest.rejections.push(shared_types::WorkerSignalRejection {
                    signal_type: shared_types::WorkerSignalType::Learning,
                    signal_id: learning.learning_id.clone(),
                    reason,
                    detail,
                });
                continue;
            }

            ingest.accepted_learnings += 1;
            let topic = if report.worker_role.as_deref() == Some("researcher") {
                shared_types::EVENT_TOPIC_RESEARCH_LEARNING_CREATED
            } else {
                shared_types::EVENT_TOPIC_WORKER_LEARNING_CREATED
            };
            Self::publish_worker_event(
                state.event_store.clone(),
                state.event_bus.clone(),
                topic,
                EventType::Custom(topic.to_string()),
                serde_json::json!({
                    "turn_id": turn_id.clone(),
                    "task_id": task_id.clone(),
                    "worker_id": worker_id.clone(),
                    "worker_role": worker_role.clone(),
                    "status": status.clone(),
                    "learning": learning,
                    "accepted_at": chrono::Utc::now().to_rfc3339(),
                }),
                correlation_id.clone(),
                source_actor_id.to_string(),
                session_id.clone(),
                thread_id.clone(),
            );
        }

        for (idx, escalation) in report.escalations.iter().enumerate() {
            let reject = if idx >= policy.max_escalations_per_turn {
                Some((
                    shared_types::WorkerSignalRejectReason::MaxPerTurnExceeded,
                    format!(
                        "max escalations per turn is {}",
                        policy.max_escalations_per_turn
                    ),
                ))
            } else if escalation.reason.trim().is_empty() {
                Some((
                    shared_types::WorkerSignalRejectReason::InvalidPayload,
                    "escalation reason is empty".to_string(),
                ))
            } else {
                let cooldown_key = format!(
                    "escalation:{:?}:{}",
                    escalation.kind,
                    Self::normalize_signal_key(&escalation.reason)
                );
                match state.escalation_cooldowns.get(&cooldown_key) {
                    Some(last_seen)
                        if now.signed_duration_since(*last_seen).num_seconds()
                            < policy.escalation_cooldown_seconds =>
                    {
                        Some((
                            shared_types::WorkerSignalRejectReason::EscalationCooldown,
                            format!(
                                "escalation cooldown {}s active",
                                policy.escalation_cooldown_seconds
                            ),
                        ))
                    }
                    _ => {
                        state.escalation_cooldowns.insert(cooldown_key, now);
                        None
                    }
                }
            };

            if let Some((reason, detail)) = reject {
                ingest.rejections.push(shared_types::WorkerSignalRejection {
                    signal_type: shared_types::WorkerSignalType::Escalation,
                    signal_id: escalation.escalation_id.clone(),
                    reason,
                    detail,
                });
                continue;
            }

            ingest.accepted_escalations += 1;
            ingest.escalation_notified = true;
            Self::publish_worker_event(
                state.event_store.clone(),
                state.event_bus.clone(),
                shared_types::EVENT_TOPIC_WORKER_SIGNAL_ESCALATION_REQUESTED,
                EventType::Custom(
                    shared_types::EVENT_TOPIC_WORKER_SIGNAL_ESCALATION_REQUESTED.to_string(),
                ),
                serde_json::json!({
                    "turn_id": turn_id.clone(),
                    "task_id": task_id.clone(),
                    "worker_id": worker_id.clone(),
                    "worker_role": worker_role.clone(),
                    "status": status.clone(),
                    "escalation": escalation,
                    "notified_target": "conductor",
                    "accepted_at": chrono::Utc::now().to_rfc3339(),
                }),
                correlation_id.clone(),
                source_actor_id.to_string(),
                session_id.clone(),
                thread_id.clone(),
            );
        }

        for (idx, artifact) in report.artifacts.iter().enumerate() {
            let reject = if idx >= policy.max_artifacts_per_turn {
                Some((
                    shared_types::WorkerSignalRejectReason::MaxPerTurnExceeded,
                    format!(
                        "max artifacts per turn is {}",
                        policy.max_artifacts_per_turn
                    ),
                ))
            } else if artifact.reference.trim().is_empty() {
                Some((
                    shared_types::WorkerSignalRejectReason::InvalidPayload,
                    "artifact reference is empty".to_string(),
                ))
            } else {
                None
            };

            if let Some((reason, detail)) = reject {
                ingest.rejections.push(shared_types::WorkerSignalRejection {
                    signal_type: shared_types::WorkerSignalType::Artifact,
                    signal_id: artifact.artifact_id.clone(),
                    reason,
                    detail,
                });
                continue;
            }

            ingest.accepted_artifacts += 1;
            Self::publish_worker_event(
                state.event_store.clone(),
                state.event_bus.clone(),
                shared_types::EVENT_TOPIC_ARTIFACT_CREATED,
                EventType::Custom(shared_types::EVENT_TOPIC_ARTIFACT_CREATED.to_string()),
                serde_json::json!({
                    "turn_id": turn_id.clone(),
                    "task_id": task_id.clone(),
                    "worker_id": worker_id.clone(),
                    "worker_role": worker_role.clone(),
                    "artifact": artifact,
                    "accepted_at": chrono::Utc::now().to_rfc3339(),
                }),
                correlation_id.clone(),
                source_actor_id.to_string(),
                session_id.clone(),
                thread_id.clone(),
            );
        }

        for rejection in &ingest.rejections {
            Self::emit_worker_signal_rejection(
                state,
                source_actor_id,
                &correlation_id,
                session_id.clone(),
                thread_id.clone(),
                rejection,
            );
        }

        ingest
    }

    fn extract_task_id(payload: &serde_json::Value) -> Option<String> {
        if let Some(task_id) = payload.get("task_id").and_then(|v| v.as_str()) {
            return Some(task_id.to_string());
        }
        payload
            .get("task")
            .and_then(|v| v.get("task_id"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    }

    fn with_observability_metadata(
        payload: serde_json::Value,
        correlation_id: &str,
        interface_kind: &str,
        task_id: Option<&str>,
    ) -> serde_json::Value {
        let span_id = ulid::Ulid::new().to_string();

        match payload {
            serde_json::Value::Object(mut obj) => {
                if interface_kind == "appactor_toolactor" {
                    Self::normalize_worker_model_fields(&mut obj);
                }
                obj.entry("trace_id".to_string())
                    .or_insert(serde_json::Value::String(correlation_id.to_string()));
                obj.entry("span_id".to_string())
                    .or_insert(serde_json::Value::String(span_id));
                obj.entry("interface_kind".to_string())
                    .or_insert(serde_json::Value::String(interface_kind.to_string()));
                if let Some(task_id) = task_id {
                    obj.entry("task_id".to_string())
                        .or_insert(serde_json::Value::String(task_id.to_string()));
                }
                serde_json::Value::Object(obj)
            }
            other => serde_json::json!({
                "value": other,
                "trace_id": correlation_id,
                "span_id": span_id,
                "interface_kind": interface_kind,
                "task_id": task_id,
            }),
        }
    }

    fn normalize_worker_model_fields(obj: &mut serde_json::Map<String, serde_json::Value>) {
        let requested = obj
            .get("model_requested")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());

        obj.insert(
            "model_requested".to_string(),
            serde_json::Value::String(requested.clone()),
        );

        let used = obj
            .get("model_used")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| {
                if requested == "none" {
                    "direct_command".to_string()
                } else {
                    requested.clone()
                }
            });

        obj.insert("model_used".to_string(), serde_json::Value::String(used));
    }

    async fn emit_request_event(
        &self,
        state: &ApplicationState,
        topic: &str,
        event_type: EventType,
        payload: serde_json::Value,
        correlation_id: String,
    ) {
        let payload =
            Self::with_observability_metadata(payload, &correlation_id, "uactor_actor", None);

        let event = match Event::new(event_type, topic, payload, "application_supervisor")
            .map(|evt| evt.with_correlation_id(correlation_id))
        {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!(error = %e, topic, "Failed to build supervisor event");
                return;
            }
        };

        // Canonical write path: EventStore first.
        let append_result = ractor::call!(state.event_store, |reply| EventStoreMsg::Append {
            event: crate::actors::AppendEvent {
                event_type: topic.to_string(),
                payload: event.payload.clone(),
                actor_id: event.source.clone(),
                user_id: "system".to_string(),
            },
            reply
        });

        match append_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, topic, "Failed to persist supervisor event");
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, topic, "EventStore RPC failed for supervisor event");
                return;
            }
        }

        // Fanout happens via EventRelayActor from committed EventStore rows (ADR-0001).
        let _ = event;
    }

    fn publish_worker_event(
        event_store: ActorRef<EventStoreMsg>,
        _event_bus: Option<ActorRef<EventBusMsg>>,
        topic: &str,
        event_type: EventType,
        payload: serde_json::Value,
        correlation_id: String,
        source_actor_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) {
        let payload_with_correlation = match payload {
            serde_json::Value::Object(mut obj) => {
                obj.insert(
                    "correlation_id".to_string(),
                    serde_json::Value::String(correlation_id.clone()),
                );
                serde_json::Value::Object(obj)
            }
            other => serde_json::json!({
                "value": other,
                "correlation_id": correlation_id,
            }),
        };
        let task_id = Self::extract_task_id(&payload_with_correlation);
        let payload_with_observability = Self::with_observability_metadata(
            payload_with_correlation,
            &correlation_id,
            "appactor_toolactor",
            task_id.as_deref(),
        );
        let event_payload =
            shared_types::with_scope(payload_with_observability, session_id, thread_id);
        let event = match Event::new(event_type, topic, event_payload, source_actor_id)
            .map(|evt| evt.with_correlation_id(correlation_id))
        {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!(error = %e, topic, "Failed to build worker event");
                return;
            }
        };

        let topic = topic.to_string();
        tokio::spawn(async move {
            // Canonical write path: EventStore first.
            let append_result = ractor::call!(event_store, |reply| EventStoreMsg::Append {
                event: crate::actors::AppendEvent {
                    event_type: topic.clone(),
                    payload: event.payload.clone(),
                    actor_id: event.source.clone(),
                    user_id: "system".to_string(),
                },
                reply
            });

            match append_result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, topic, "Failed to persist worker event");
                    return;
                }
                Err(e) => {
                    tracing::warn!(error = %e, topic, "EventStore RPC failed for worker event");
                    return;
                }
            }

            // Fanout happens via EventRelayActor from committed EventStore rows (ADR-0001).
            let _ = event;
        });
    }
}
