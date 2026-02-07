//! Desktop Supervisor - Manages DesktopActor instances
//!
//! The DesktopSupervisor is responsible for:
//! - Creating and managing DesktopActor instances per user
//! - Automatic restart of failed DesktopActors (one_for_one strategy)
//! - Tracking restart intensity (max 3 restarts per 60 seconds)
//! - Using ractor registry for actor discovery (desktop:{desktop_id})

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::actors::desktop::{DesktopActor, DesktopActorMsg, DesktopArguments};
use crate::actors::event_store::EventStoreMsg;

/// Maximum restarts allowed within the period
const MAX_RESTARTS: u32 = 3;
/// Time window for restart intensity tracking (60 seconds)
const RESTART_PERIOD: Duration = Duration::from_secs(60);

/// Desktop supervisor - manages DesktopActor instances
#[derive(Debug, Default)]
pub struct DesktopSupervisor;

/// Desktop actor info stored in supervisor state
#[derive(Debug, Clone)]
pub struct DesktopInfo {
    /// Actor reference
    pub actor_ref: ActorRef<DesktopActorMsg>,
    /// Desktop ID
    pub desktop_id: String,
    /// User ID
    pub user_id: String,
    /// Arguments used to spawn the actor (for restarts)
    pub args: DesktopArguments,
}

/// Desktop supervisor state
pub struct DesktopSupervisorState {
    /// Desktop ID -> DesktopInfo mapping
    pub desktops: HashMap<String, DesktopInfo>,
    /// Restart tracking: ActorId -> (restart_count, window_start)
    pub restart_counts: HashMap<ractor::ActorId, (u32, Instant)>,
    /// Event store reference (passed to child actors)
    pub event_store: ActorRef<EventStoreMsg>,
}

/// Arguments for spawning DesktopSupervisor
#[derive(Debug, Clone)]
pub struct DesktopSupervisorArgs {
    /// Event store actor reference
    pub event_store: ActorRef<EventStoreMsg>,
}

/// Messages handled by DesktopSupervisor
#[derive(Debug)]
pub enum DesktopSupervisorMsg {
    /// Get existing DesktopActor or create a new one
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        args: DesktopArguments,
        reply: RpcReplyPort<ActorRef<DesktopActorMsg>>,
    },
    /// Get existing DesktopActor if it exists
    GetDesktop {
        desktop_id: String,
        reply: RpcReplyPort<Option<ActorRef<DesktopActorMsg>>>,
    },
    /// Remove a desktop from tracking (called on clean shutdown)
    RemoveDesktop { desktop_id: String },
    /// Supervision event from child actors
    Supervision(SupervisionEvent),
}

impl DesktopSupervisor {
    /// Check if an actor should be restarted based on intensity
    fn should_restart(
        &self,
        actor_id: ractor::ActorId,
        state: &mut DesktopSupervisorState,
    ) -> bool {
        let now = Instant::now();

        match state.restart_counts.get_mut(&actor_id) {
            Some((count, window_start)) => {
                // Check if we're within the restart window
                if now.duration_since(*window_start) > RESTART_PERIOD {
                    // Window expired, reset counter
                    *count = 1;
                    *window_start = now;
                    true
                } else if *count < MAX_RESTARTS {
                    // Within window, still have restarts left
                    *count += 1;
                    true
                } else {
                    // Max restarts exceeded
                    warn!(
                        actor_id = %actor_id,
                        restarts = *count,
                        "Max restart intensity exceeded - will not restart actor"
                    );
                    false
                }
            }
            None => {
                // First restart for this actor
                state.restart_counts.insert(actor_id, (1, now));
                true
            }
        }
    }

    /// Get desktop_id from actor cell by looking up in our state
    fn find_desktop_id_by_actor(
        &self,
        actor_id: ractor::ActorId,
        state: &DesktopSupervisorState,
    ) -> Option<String> {
        state
            .desktops
            .iter()
            .find(|(_, info)| info.actor_ref.get_id() == actor_id)
            .map(|(desktop_id, _)| desktop_id.clone())
    }

    /// Handle supervision events from child actors
    async fn handle_supervision_event(
        &self,
        myself: ActorRef<DesktopSupervisorMsg>,
        event: SupervisionEvent,
        state: &mut DesktopSupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        match event {
            SupervisionEvent::ActorStarted(actor_cell) => {
                info!(
                    supervisor = %myself.get_id(),
                    child_actor = %actor_cell.get_id(),
                    "DesktopActor started"
                );
            }
            SupervisionEvent::ActorFailed(actor_cell, error) => {
                warn!(
                    supervisor = %myself.get_id(),
                    failed_actor = %actor_cell.get_id(),
                    error = %error,
                    "DesktopActor failed - evaluating restart"
                );

                let actor_id = actor_cell.get_id();

                if self.should_restart(actor_id, state) {
                    // Find the desktop info
                    if let Some(desktop_id) = self.find_desktop_id_by_actor(actor_id, state) {
                        // Get the info before removing
                        let info = if let Some(info) = state.desktops.get(&desktop_id) {
                            info.clone()
                        } else {
                            warn!(
                                actor_id = %actor_id,
                                desktop_id = %desktop_id,
                                "Desktop info not found during restart"
                            );
                            return Ok(());
                        };

                        info!(
                            desktop_id = %desktop_id,
                            "Restarting DesktopActor"
                        );

                        // Remove the old entry
                        state.desktops.remove(&desktop_id);

                        // Spawn new actor with same arguments
                        let actor_name = format!("desktop:{desktop_id}");
                        match Actor::spawn_linked(
                            Some(actor_name),
                            DesktopActor,
                            info.args.clone(),
                            myself.get_cell(),
                        )
                        .await
                        {
                            Ok((new_ref, _)) => {
                                let new_actor_id = new_ref.get_id();
                                // Update the stored reference
                                state.desktops.insert(
                                    desktop_id.clone(),
                                    DesktopInfo {
                                        actor_ref: new_ref.clone(),
                                        desktop_id: desktop_id.clone(),
                                        user_id: info.user_id.clone(),
                                        args: info.args.clone(),
                                    },
                                );
                                info!(
                                    desktop_id = %desktop_id,
                                    new_actor_id = %new_actor_id,
                                    "DesktopActor restarted successfully"
                                );
                            }
                            Err(e) => {
                                error!(
                                    desktop_id = %desktop_id,
                                    error = %e,
                                    "Failed to restart DesktopActor"
                                );
                                return Err(ActorProcessingErr::from(e));
                            }
                        }
                    } else {
                        warn!(
                            actor_id = %actor_id,
                            "Received ActorFailed for unknown actor - not tracking this desktop"
                        );
                    }
                } else {
                    // Max restarts exceeded - escalate by stopping ourselves
                    error!(
                        actor_id = %actor_cell.get_id(),
                        max_restarts = MAX_RESTARTS,
                        period_secs = RESTART_PERIOD.as_secs(),
                        "Max restart intensity exceeded - escalating"
                    );

                    // Remove the desktop from tracking
                    if let Some(desktop_id) = self.find_desktop_id_by_actor(actor_id, state) {
                        state.desktops.remove(&desktop_id);
                    }

                    return Err(ActorProcessingErr::from(std::io::Error::other(
                        format!("Max restart intensity exceeded for actor {actor_id}")
                    )));
                }
            }
            SupervisionEvent::ActorTerminated(actor_cell, _actor_state, exit_reason) => {
                info!(
                    supervisor = %myself.get_id(),
                    terminated_actor = %actor_cell.get_id(),
                    reason = ?exit_reason,
                    "DesktopActor terminated"
                );

                // Clean up tracking
                let actor_id = actor_cell.get_id();
                if let Some(desktop_id) = self.find_desktop_id_by_actor(actor_id, state) {
                    state.desktops.remove(&desktop_id);
                    state.restart_counts.remove(&actor_id);
                    info!(
                        desktop_id = %desktop_id,
                        "Cleaned up DesktopActor tracking"
                    );
                }
            }
            _ => {
                tracing::debug!(
                    supervisor = %myself.get_id(),
                    event = ?event,
                    "Received supervision event"
                );
            }
        }

        Ok(())
    }
}

#[ractor::async_trait]
impl Actor for DesktopSupervisor {
    type Msg = DesktopSupervisorMsg;
    type State = DesktopSupervisorState;
    type Arguments = DesktopSupervisorArgs;

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match &event {
            SupervisionEvent::ActorStarted(cell) => {
                tracing::info!("DesktopActor started: {}", cell.get_id());
            }
            SupervisionEvent::ActorFailed(cell, err) => {
                tracing::error!("DesktopActor failed: {} - {}", cell.get_id(), err);
            }
            SupervisionEvent::ActorTerminated(cell, _, reason) => {
                tracing::info!("DesktopActor terminated: {} - {:?}", cell.get_id(), reason);
            }
            _ => {}
        }
        // Delegate to existing handler and return result to stay alive
        self.handle_supervision_event(myself, event, state).await
    }

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(
            supervisor = %myself.get_id(),
            "DesktopSupervisor starting"
        );

        Ok(DesktopSupervisorState {
            desktops: HashMap::new(),
            restart_counts: HashMap::new(),
            event_store: args.event_store,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            DesktopSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                args,
                reply,
            } => {
                // First, check if we have this desktop in our state
                if let Some(info) = state.desktops.get(&desktop_id) {
                    info!(
                        desktop_id = %desktop_id,
                        actor_id = %info.actor_ref.get_id(),
                        "Found existing DesktopActor in supervisor state"
                    );
                    let _ = reply.send(info.actor_ref.clone());
                    return Ok(());
                }

                // Check registry using where_is
                let actor_name = format!("desktop:{desktop_id}");
                if let Some(actor_cell) = ractor::registry::where_is(actor_name.clone()) {
                    info!(
                        desktop_id = %desktop_id,
                        actor_id = %actor_cell.get_id(),
                        "Found existing DesktopActor in registry"
                    );
                    let actor_ref: ActorRef<DesktopActorMsg> = actor_cell.into();

                    // Store in our state
                    state.desktops.insert(
                        desktop_id.clone(),
                        DesktopInfo {
                            actor_ref: actor_ref.clone(),
                            desktop_id: desktop_id.clone(),
                            user_id: user_id.clone(),
                            args: args.clone(),
                        },
                    );

                    let _ = reply.send(actor_ref);
                    return Ok(());
                }

                // Need to create a new actor
                info!(
                    desktop_id = %desktop_id,
                    user_id = %user_id,
                    "Creating new DesktopActor"
                );

                match Actor::spawn_linked(
                    Some(actor_name),
                    DesktopActor,
                    args.clone(),
                    myself.get_cell(),
                )
                .await
                {
                    Ok((actor_ref, _)) => {
                        info!(
                            desktop_id = %desktop_id,
                            actor_id = %actor_ref.get_id(),
                            "DesktopActor spawned successfully"
                        );

                        // Store in state
                        state.desktops.insert(
                            desktop_id.clone(),
                            DesktopInfo {
                                actor_ref: actor_ref.clone(),
                                desktop_id: desktop_id.clone(),
                                user_id: user_id.clone(),
                                args,
                            },
                        );

                        let _ = reply.send(actor_ref);
                    }
                    Err(e) => {
                        error!(
                            desktop_id = %desktop_id,
                            error = %e,
                            "Failed to spawn DesktopActor"
                        );
                        return Err(ActorProcessingErr::from(e));
                    }
                }
            }
            DesktopSupervisorMsg::GetDesktop { desktop_id, reply } => {
                let result = state
                    .desktops
                    .get(&desktop_id)
                    .map(|info| info.actor_ref.clone());
                let _ = reply.send(result);
            }
            DesktopSupervisorMsg::RemoveDesktop { desktop_id } => {
                if let Some(info) = state.desktops.remove(&desktop_id) {
                    info!(
                        desktop_id = %desktop_id,
                        actor_id = %info.actor_ref.get_id(),
                        "Removed DesktopActor from tracking"
                    );
                    state.restart_counts.remove(&info.actor_ref.get_id());
                }
            }
            DesktopSupervisorMsg::Supervision(event) => {
                self.handle_supervision_event(myself, event, state).await?;
            }
        }

        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        info!(
            supervisor = %myself.get_id(),
            desktop_count = state.desktops.len(),
            "DesktopSupervisor stopping"
        );

        // Child actors will be automatically stopped by ractor when we stop
        // but we log what we're tracking
        for (desktop_id, info) in &state.desktops {
            info!(
                desktop_id = %desktop_id,
                actor_id = %info.actor_ref.get_id(),
                "DesktopActor will be stopped"
            );
        }

        Ok(())
    }
}

/// Convenience function to get or create a desktop actor via the supervisor
pub async fn get_or_create_desktop(
    supervisor: &ActorRef<DesktopSupervisorMsg>,
    desktop_id: impl Into<String>,
    user_id: impl Into<String>,
    args: DesktopArguments,
) -> Result<ActorRef<DesktopActorMsg>, ractor::RactorErr<DesktopSupervisorMsg>> {
    let desktop_id = desktop_id.into();
    let user_id = user_id.into();

    ractor::call!(supervisor, |reply| {
        DesktopSupervisorMsg::GetOrCreateDesktop {
            desktop_id,
            user_id,
            args,
            reply,
        }
    })
}

/// Convenience function to get a desktop actor if it exists
pub async fn get_desktop(
    supervisor: &ActorRef<DesktopSupervisorMsg>,
    desktop_id: impl Into<String>,
) -> Result<Option<ActorRef<DesktopActorMsg>>, ractor::RactorErr<DesktopSupervisorMsg>> {
    ractor::call!(supervisor, |reply| DesktopSupervisorMsg::GetDesktop {
        desktop_id: desktop_id.into(),
        reply,
    })
}

/// Convenience function to remove a desktop from supervisor tracking
pub async fn remove_desktop(
    supervisor: &ActorRef<DesktopSupervisorMsg>,
    desktop_id: impl Into<String>,
) -> Result<(), ractor::RactorErr<DesktopSupervisorMsg>> {
    supervisor
        .cast(DesktopSupervisorMsg::RemoveDesktop {
            desktop_id: desktop_id.into(),
        })
        .map_err(ractor::RactorErr::from)
}
