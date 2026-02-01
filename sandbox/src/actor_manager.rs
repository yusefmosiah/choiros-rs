//! Actor Manager - maintains persistent, supervised actor instances
//!
//! PREDICTION: A centralized manager with thread-safe registry can provide
//! actor-per-chat persistence with fault tolerance, matching Elixir OTP patterns.
//!
//! EXPERIMENT:
//! 1. DashMap for concurrent actor_id -> Addr lookup
//! 2. Supervisor::start for fault-tolerant actors
//! 3. SystemService trait for global singleton manager
//!
//! OBSERVE:
//! - Same actor_id always returns same actor instance
//! - Actor restarts on panic but preserves identity
//! - Thread-safe concurrent access

use actix::{Actor, Addr, Supervisor};
use dashmap::DashMap;
use std::sync::Arc;

use crate::actors::{ChatActor, ChatAgent, DesktopActor, EventStoreActor};

/// Global manager for persistent actor instances
pub struct ActorManager {
    chat_actors: Arc<DashMap<String, Addr<ChatActor>>>,
    chat_agents: Arc<DashMap<String, Addr<ChatAgent>>>,
    desktop_actors: Arc<DashMap<String, Addr<DesktopActor>>>,
    event_store: Addr<EventStoreActor>,
}

impl ActorManager {
    pub fn new(event_store: Addr<EventStoreActor>) -> Self {
        Self {
            chat_actors: Arc::new(DashMap::new()),
            chat_agents: Arc::new(DashMap::new()),
            desktop_actors: Arc::new(DashMap::new()),
            event_store: event_store.clone(),
        }
    }

    /// Get the EventStoreActor address
    pub fn event_store(&self) -> Addr<EventStoreActor> {
        self.event_store.clone()
    }

    /// Get existing ChatActor or create supervised instance
    pub fn get_or_create_chat(&self, actor_id: String, user_id: String) -> Addr<ChatActor> {
        // Fast path: check if exists
        if let Some(entry) = self.chat_actors.get(&actor_id) {
            return entry.clone();
        }

        // Slow path: create new supervised actor
        let event_store = self.event_store.clone();
        let actor_id_clone = actor_id.clone();
        let chat_addr =
            Supervisor::start(move |_| ChatActor::new(actor_id_clone, user_id, event_store));

        // Store in registry
        self.chat_actors.insert(actor_id, chat_addr.clone());

        chat_addr
    }

    /// Get existing ChatActor if it exists
    #[allow(dead_code)]
    pub fn get_chat(&self, actor_id: &str) -> Option<Addr<ChatActor>> {
        self.chat_actors.get(actor_id).map(|e| e.clone())
    }

    /// Get existing DesktopActor or create supervised instance
    pub fn get_or_create_desktop(&self, desktop_id: String, user_id: String) -> Addr<DesktopActor> {
        // Fast path: check if exists
        if let Some(entry) = self.desktop_actors.get(&desktop_id) {
            return entry.clone();
        }

        // Slow path: create new supervised actor
        let event_store = self.event_store.clone();
        let desktop_id_clone = desktop_id.clone();
        let desktop_addr =
            Supervisor::start(move |_| DesktopActor::new(desktop_id_clone, user_id, event_store));

        // Store in registry
        self.desktop_actors.insert(desktop_id, desktop_addr.clone());

        desktop_addr
    }

    /// Get existing DesktopActor if it exists
    #[allow(dead_code)]
    pub fn get_desktop(&self, desktop_id: &str) -> Option<Addr<DesktopActor>> {
        self.desktop_actors.get(desktop_id).map(|e| e.clone())
    }

    /// Get existing ChatAgent or create supervised instance
    pub fn get_or_create_chat_agent(&self, agent_id: String, user_id: String) -> Addr<ChatAgent> {
        // Fast path: check if exists
        if let Some(entry) = self.chat_agents.get(&agent_id) {
            return entry.clone();
        }

        // Slow path: create new supervised actor
        let event_store = self.event_store.clone();
        let agent_id_clone = agent_id.clone();
        let agent_addr =
            Supervisor::start(move |_| ChatAgent::new(agent_id_clone, user_id, event_store));

        // Store in registry
        self.chat_agents.insert(agent_id, agent_addr.clone());

        agent_addr
    }

    /// Get existing ChatAgent if it exists
    #[allow(dead_code)]
    pub fn get_chat_agent(&self, agent_id: &str) -> Option<Addr<ChatAgent>> {
        self.chat_agents.get(agent_id).map(|e| e.clone())
    }
}

impl Actor for ActorManager {
    type Context = actix::Context<Self>;
}

/// Shared application state for HTTP handlers
pub struct AppState {
    pub actor_manager: ActorManager,
}

impl AppState {
    pub fn new(event_store: Addr<EventStoreActor>) -> Self {
        Self {
            actor_manager: ActorManager::new(event_store),
        }
    }
}
