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
//!     ├── TerminalSupervisor
//!     └── ResearcherSupervisor
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

/// Application supervisor - root of the supervision tree
#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

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
    /// Get or create a terminal session
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: RpcReplyPort<ractor::ActorRef<crate::actors::terminal::TerminalMsg>>,
    },
    /// Get or create a researcher actor
    GetOrCreateResearcher {
        researcher_id: String,
        user_id: String,
        reply: RpcReplyPort<
            Result<ractor::ActorRef<crate::actors::researcher::ResearcherMsg>, String>,
        >,
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
            ApplicationSupervisorMsg::GetOrCreateResearcher {
                researcher_id,
                user_id,
                reply,
            } => {
                let correlation_id = ulid::Ulid::new().to_string();
                self.emit_request_event(
                    state,
                    "supervisor.researcher.get_or_create.started",
                    EventType::Custom("supervisor.researcher.get_or_create.started".to_string()),
                    serde_json::json!({
                        "researcher_id": researcher_id,
                        "user_id": user_id,
                        "supervisor_id": myself.get_id().to_string(),
                    }),
                    correlation_id.clone(),
                )
                .await;

                if let Some(ref session_supervisor) = state.session_supervisor {
                    match ractor::call!(session_supervisor, |ss_reply| {
                        SessionSupervisorMsg::GetOrCreateResearcher {
                            researcher_id: researcher_id.clone(),
                            user_id: user_id.clone(),
                            reply: ss_reply,
                        }
                    }) {
                        Ok(result) => match result {
                            Ok(actor_ref) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.researcher.get_or_create.completed",
                                    EventType::Custom(
                                        "supervisor.researcher.get_or_create.completed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "researcher_id": researcher_id,
                                        "user_id": user_id,
                                        "researcher_ref": actor_ref.get_id().to_string(),
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                )
                                .await;
                                let _ = reply.send(Ok(actor_ref));
                            }
                            Err(e) => {
                                self.emit_request_event(
                                    state,
                                    "supervisor.researcher.get_or_create.failed",
                                    EventType::Custom(
                                        "supervisor.researcher.get_or_create.failed".to_string(),
                                    ),
                                    serde_json::json!({
                                        "researcher_id": researcher_id,
                                        "user_id": user_id,
                                        "error": e,
                                        "supervisor_id": myself.get_id().to_string(),
                                    }),
                                    correlation_id,
                                )
                                .await;
                                let _ = reply.send(Err(e));
                                return Ok(());
                            }
                        },
                        Err(e) => {
                            self.emit_request_event(
                                state,
                                "supervisor.researcher.get_or_create.failed",
                                EventType::Custom(
                                    "supervisor.researcher.get_or_create.failed".to_string(),
                                ),
                                serde_json::json!({
                                    "researcher_id": researcher_id,
                                    "user_id": user_id,
                                    "error": e.to_string(),
                                    "supervisor_id": myself.get_id().to_string(),
                                }),
                                correlation_id,
                            )
                            .await;
                            let _ = reply.send(Err(e.to_string()));
                            return Ok(());
                        }
                    }
                } else {
                    let _ = reply.send(Err("SessionSupervisor not available".to_string()));
                    return Ok(());
                }
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
