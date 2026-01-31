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

use crate::actors::{ChatActor, EventStoreActor};

/// Global manager for persistent actor instances
pub struct ActorManager {
    chat_actors: Arc<DashMap<String, Addr<ChatActor>>>,
    event_store: Addr<EventStoreActor>,
}

impl ActorManager {
    pub fn new(event_store: Addr<EventStoreActor>) -> Self {
        Self {
            chat_actors: Arc::new(DashMap::new()),
            event_store,
        }
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
    pub fn get_chat(&self, actor_id: &str) -> Option<Addr<ChatActor>> {
        self.chat_actors.get(actor_id).map(|e| e.clone())
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
