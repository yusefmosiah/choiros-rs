//! Actor Manager - maintains persistent, supervised actor instances using ractor
//!
//! PREDICTION: A centralized manager with thread-safe registry can provide
//! actor-per-chat persistence with fault tolerance, matching Elixir OTP patterns.
//!
//! EXPERIMENT:
//! 1. DashMap for concurrent actor_id -> ActorRef lookup
//! 2. ractor supervision for fault-tolerant actors
//! 3. ActorRef<MessageType> instead of Addr<ActorType>
//!
//! OBSERVE:
//! - Same actor_id always returns same actor instance
//! - Actor restarts on panic but preserves identity
//! - Thread-safe concurrent access

use dashmap::DashMap;
use ractor::{Actor, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;

// Re-export message types from actors
pub use crate::actors::chat::{ChatActor, ChatActorArguments, ChatActorMsg};
pub use crate::actors::chat_agent::{ChatAgent, ChatAgentArguments, ChatAgentMsg};
pub use crate::actors::desktop::{DesktopActor, DesktopActorMsg, DesktopArguments};
pub use crate::actors::event_store::EventStoreMsg;
pub use crate::actors::terminal::{TerminalActor, TerminalArguments, TerminalMsg};

/// Global manager for persistent actor instances
#[derive(Clone)]
pub struct ActorManager {
    chat_actors: Arc<DashMap<String, ActorRef<ChatActorMsg>>>,
    chat_agents: Arc<DashMap<String, ActorRef<ChatAgentMsg>>>,
    desktop_actors: Arc<DashMap<String, ActorRef<DesktopActorMsg>>>,
    terminal_actors: Arc<DashMap<String, ActorRef<TerminalMsg>>>,
    terminal_create_lock: Arc<Mutex<()>>,
    event_store: ActorRef<EventStoreMsg>,
}

impl ActorManager {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        Self {
            chat_actors: Arc::new(DashMap::new()),
            chat_agents: Arc::new(DashMap::new()),
            desktop_actors: Arc::new(DashMap::new()),
            terminal_actors: Arc::new(DashMap::new()),
            terminal_create_lock: Arc::new(Mutex::new(())),
            event_store: event_store.clone(),
        }
    }

    /// Get the EventStoreActor reference
    pub fn event_store(&self) -> ActorRef<EventStoreMsg> {
        self.event_store.clone()
    }

    /// Get existing ChatActor or create new instance
    pub async fn get_or_create_chat(
        &self,
        actor_id: String,
        user_id: String,
    ) -> ActorRef<ChatActorMsg> {
        // Fast path: check if exists
        if let Some(entry) = self.chat_actors.get(&actor_id) {
            return entry.clone();
        }

        // Slow path: create new actor
        let event_store = self.event_store.clone();
        let actor_id_clone = actor_id.clone();

        let (chat_ref, _handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: actor_id_clone,
                user_id,
                event_store,
            },
        )
        .await
        .expect("Failed to spawn ChatActor");

        // Store in registry
        self.chat_actors.insert(actor_id, chat_ref.clone());

        chat_ref
    }

    /// Get existing ChatActor if it exists
    #[allow(dead_code)]
    pub fn get_chat(&self, actor_id: &str) -> Option<ActorRef<ChatActorMsg>> {
        self.chat_actors.get(actor_id).map(|e| e.clone())
    }

    /// Get existing DesktopActor or create new instance
    pub async fn get_or_create_desktop(
        &self,
        desktop_id: String,
        user_id: String,
    ) -> ActorRef<DesktopActorMsg> {
        // Fast path: check if exists
        if let Some(entry) = self.desktop_actors.get(&desktop_id) {
            return entry.clone();
        }

        // Slow path: create new actor
        let event_store = self.event_store.clone();
        let desktop_id_clone = desktop_id.clone();

        let (desktop_ref, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: desktop_id_clone,
                user_id,
                event_store,
            },
        )
        .await
        .expect("Failed to spawn DesktopActor");

        // Store in registry
        self.desktop_actors.insert(desktop_id, desktop_ref.clone());

        desktop_ref
    }

    /// Get existing DesktopActor if it exists
    #[allow(dead_code)]
    pub fn get_desktop(&self, desktop_id: &str) -> Option<ActorRef<DesktopActorMsg>> {
        self.desktop_actors.get(desktop_id).map(|e| e.clone())
    }

    /// Get existing ChatAgent or create new instance
    pub async fn get_or_create_chat_agent(
        &self,
        agent_id: String,
        user_id: String,
    ) -> ActorRef<ChatAgentMsg> {
        // Fast path: check if exists
        if let Some(entry) = self.chat_agents.get(&agent_id) {
            return entry.clone();
        }

        // Slow path: create new actor
        let event_store = self.event_store.clone();
        let agent_id_clone = agent_id.clone();

        let (agent_ref, _handle) = Actor::spawn(
            None,
            ChatAgent::new(),
            ChatAgentArguments {
                actor_id: agent_id_clone,
                user_id,
                event_store,
            },
        )
        .await
        .expect("Failed to spawn ChatAgent");

        // Store in registry
        self.chat_agents.insert(agent_id, agent_ref.clone());

        agent_ref
    }

    /// Get existing ChatAgent if it exists
    #[allow(dead_code)]
    pub fn get_chat_agent(&self, agent_id: &str) -> Option<ActorRef<ChatAgentMsg>> {
        self.chat_agents.get(agent_id).map(|e| e.clone())
    }

    /// Get existing TerminalActor or create new instance
    pub async fn get_or_create_terminal(
        &self,
        terminal_id: &str,
        args: TerminalArguments,
    ) -> Result<ActorRef<TerminalMsg>, ractor::ActorProcessingErr> {
        // Fast path: check if exists
        if let Some(entry) = self.terminal_actors.get(terminal_id) {
            return Ok(entry.clone());
        }

        // Serialize slow-path creation to avoid duplicate actors for the same terminal_id.
        let _create_guard = self.terminal_create_lock.lock().await;
        if let Some(entry) = self.terminal_actors.get(terminal_id) {
            return Ok(entry.clone());
        }

        let terminal_id_clone = terminal_id.to_string();
        let (terminal_ref, _handle) = Actor::spawn(None, TerminalActor, args).await?;

        // Store in registry
        self.terminal_actors
            .insert(terminal_id_clone, terminal_ref.clone());

        Ok(terminal_ref)
    }

    /// Get existing TerminalActor if it exists
    #[allow(dead_code)]
    pub fn get_terminal(&self, terminal_id: &str) -> Option<ActorRef<TerminalMsg>> {
        self.terminal_actors.get(terminal_id).map(|e| e.clone())
    }

    /// Remove a terminal actor from the registry
    pub fn remove_terminal(&self, terminal_id: &str) -> Option<ActorRef<TerminalMsg>> {
        self.terminal_actors
            .remove(terminal_id)
            .map(|entry| entry.1)
    }
}

/// Shared application state for HTTP handlers
pub struct AppState {
    pub actor_manager: ActorManager,
}

impl AppState {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        Self {
            actor_manager: ActorManager::new(event_store),
        }
    }
}
