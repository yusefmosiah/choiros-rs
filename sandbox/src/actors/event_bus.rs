//! EventBusActor - Pub/sub event distribution using ractor Process Groups
//!
//! This actor provides topic-based publish/subscribe functionality, leveraging
//! ractor's native Process Groups (PG) for efficient message broadcasting.
//!
//! # Architecture
//!
//! - Uses `ractor::pg` for topic-based pub/sub (no custom subscriber management)
//! - Integrates with EventStoreActor for optional event persistence
//! - Supports wildcard topic patterns (e.g., "worker.*")
//! - Maintains subscription stats for monitoring/debugging
//!
//! # Example
//!
//! ```rust
//! // Subscribe to a topic
//! cast!(event_bus, EventBusMsg::Subscribe {
//!     topic: "worker.complete".to_string(),
//!     subscriber: my_actor_ref.clone(),
//! })?;
//!
//! // Publish an event
//! let event = Event::new(EventType::WorkerComplete, "worker.complete", payload, "source")?;
//! cast!(event_bus, EventBusMsg::Publish { event, persist: true })?;
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ractor::{
    cast, Actor, ActorProcessingErr, ActorRef, RpcReplyPort,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing;



// ============================================================================
// Data Types
// ============================================================================

/// Core event type for the event bus
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// Unique event identifier (ULID)
    pub id: String,

    /// Event type classification
    pub event_type: EventType,

    /// Topic for routing (hierarchical, e.g., "worker.task.complete")
    pub topic: String,

    /// Event payload (JSON value)
    pub payload: serde_json::Value,

    /// Timestamp in UTC
    pub timestamp: DateTime<Utc>,

    /// Source actor or user identifier
    pub source: String,

    /// Optional correlation ID for request tracing
    pub correlation_id: Option<String>,
}

impl Event {
    /// Create a new event with auto-generated ID and timestamp
    pub fn new(
        event_type: EventType,
        topic: impl Into<String>,
        payload: impl Serialize,
        source: impl Into<String>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: ulid::Ulid::new().to_string(),
            event_type,
            topic: topic.into(),
            payload: serde_json::to_value(payload)?,
            timestamp: Utc::now(),
            source: source.into(),
            correlation_id: None,
        })
    }

    /// Check if this event matches a topic pattern
    /// Supports wildcards: "worker.*" matches "worker.task", "worker.job"
    pub fn matches_topic(&self, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.ends_with(".*") {
            let prefix = &pattern[..pattern.len() - 2];
            self.topic.starts_with(prefix)
                && (self.topic.len() == prefix.len()
                    || self.topic[prefix.len()..].starts_with('.'))
        } else {
            self.topic == pattern
        }
    }

    /// Set correlation ID (builder pattern)
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

/// Standardized event types for the automatic computer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Worker lifecycle
    WorkerSpawned,
    WorkerProgress,
    WorkerComplete,
    WorkerFailed,
    WorkerCancelled,

    // Findings and results
    FindingNew,
    FindingUpdated,

    // Chat and messaging
    ChatMessage,
    ChatTyping,

    // File system
    FileChanged,
    FileCreated,
    FileDeleted,

    // User interactions
    UserInput,
    UserCommand,

    // System
    SystemHeartbeat,
    SystemError,

    // Custom (for extensibility)
    #[serde(rename = "custom")]
    Custom(String),
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::WorkerSpawned => write!(f, "worker_spawned"),
            EventType::WorkerProgress => write!(f, "worker_progress"),
            EventType::WorkerComplete => write!(f, "worker_complete"),
            EventType::WorkerFailed => write!(f, "worker_failed"),
            EventType::WorkerCancelled => write!(f, "worker_cancelled"),
            EventType::FindingNew => write!(f, "finding_new"),
            EventType::FindingUpdated => write!(f, "finding_updated"),
            EventType::ChatMessage => write!(f, "chat_message"),
            EventType::ChatTyping => write!(f, "chat_typing"),
            EventType::FileChanged => write!(f, "file_changed"),
            EventType::FileCreated => write!(f, "file_created"),
            EventType::FileDeleted => write!(f, "file_deleted"),
            EventType::UserInput => write!(f, "user_input"),
            EventType::UserCommand => write!(f, "user_command"),
            EventType::SystemHeartbeat => write!(f, "system_heartbeat"),
            EventType::SystemError => write!(f, "system_error"),
            EventType::Custom(s) => write!(f, "custom.{}", s),
        }
    }
}

// ============================================================================
// EventBusActor
// ============================================================================

/// Messages handled by EventBusActor
#[derive(Debug)]
pub enum EventBusMsg {
    /// Publish an event to a topic
    Publish {
        event: Event,
        /// Whether to persist to EventStore
        persist: bool,
    },

    /// Subscribe an actor to a topic
    Subscribe {
        topic: String,
        subscriber: ActorRef<Event>,
    },

    /// Unsubscribe an actor from a topic
    Unsubscribe {
        topic: String,
        subscriber: ActorRef<Event>,
    },

    /// Get list of subscribers for a topic (for debugging)
    GetSubscribers {
        topic: String,
        reply: RpcReplyPort<Vec<ractor::ActorId>>,
    },

    /// Query recent events from EventStore
    QueryEvents {
        topic: String,
        since: DateTime<Utc>,
        limit: usize,
        reply: RpcReplyPort<Vec<Event>>,
    },
}



/// Configuration for EventBusActor
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Maximum events to buffer per slow subscriber
    pub max_buffer_size: usize,

    /// Whether to persist all events by default
    pub default_persist: bool,

    /// Topics to exclude from persistence
    pub no_persist_topics: HashSet<String>,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 1000,
            default_persist: true,
            no_persist_topics: HashSet::new(),
        }
    }
}

/// Arguments for spawning EventBusActor
#[derive(Debug, Clone)]
pub struct EventBusArguments {
    /// Reference to EventStoreActor for persistence (optional for testing)
    pub event_store: Option<ActorRef<crate::actors::EventStoreMsg>>,

    /// Configuration
    pub config: EventBusConfig,
}

/// State for EventBusActor
pub struct EventBusState {
    /// Reference to EventStoreActor for persistence (optional for testing)
    event_store: Option<ActorRef<crate::actors::EventStoreMsg>>,

    /// Cache of topic -> subscriber count (for metrics/debugging)
    subscription_stats: HashMap<String, usize>,

    /// Configuration
    config: EventBusConfig,
}

/// Actor that provides pub/sub event distribution
#[derive(Debug, Default)]
pub struct EventBusActor;

#[async_trait]
impl Actor for EventBusActor {
    type Msg = EventBusMsg;
    type State = EventBusState;
    type Arguments = EventBusArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "EventBusActor starting"
        );

        Ok(EventBusState {
            event_store: args.event_store,
            subscription_stats: HashMap::new(),
            config: args.config,
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "EventBusActor started successfully"
        );
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            EventBusMsg::Publish { event, persist } => {
                self.handle_publish(event, persist, state).await
            }
            EventBusMsg::Subscribe { topic, subscriber } => {
                self.handle_subscribe(topic, subscriber, state).await
            }
            EventBusMsg::Unsubscribe { topic, subscriber } => {
                self.handle_unsubscribe(topic, subscriber, state).await
            }
            EventBusMsg::GetSubscribers { topic, reply } => {
                self.handle_get_subscribers(topic, reply, state).await
            }
            EventBusMsg::QueryEvents {
                topic,
                since,
                limit,
                reply,
            } => self.handle_query_events(topic, since, limit, reply, state).await,
        }
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "EventBusActor stopped"
        );
        Ok(())
    }
}

impl EventBusActor {
    async fn handle_publish(
        &self,
        event: Event,
        persist: bool,
        state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!(
            event_id = %event.id,
            topic = %event.topic,
            event_type = %event.event_type,
            "Publishing event"
        );

        // Persist if requested and not in no-persist list
        let should_persist = persist
            && state.config.default_persist
            && !state.config.no_persist_topics.contains(&event.topic);

        if should_persist {
            if let Some(ref store_ref) = state.event_store {
                // Convert Event to EventStore format and append
                let store_event = crate::actors::AppendEvent {
                    event_type: event.event_type.to_string(),
                    payload: event.payload.clone(),
                    actor_id: event.source.clone(),
                    user_id: "system".to_string(), // TODO: Extract from event
                };
                
                // Fire-and-forget persistence (don't block publish on store)
                let store_ref = store_ref.clone();
                tokio::spawn(async move {
                    if let Err(e) = cast!(store_ref, crate::actors::EventStoreMsg::Append(store_event)) {
                        tracing::warn!("Failed to persist event: {}", e);
                    }
                });
            } else {
                tracing::debug!("Event persistence skipped: no event store configured");
            }
        }

        // Broadcast to exact topic subscribers via Process Groups
        self.broadcast_to_topic(&event.topic, &event).await?;

        // Broadcast to wildcard subscribers
        self.broadcast_to_wildcards(&event).await?;

        Ok(())
    }

    async fn handle_subscribe(
        &self,
        topic: String,
        subscriber: ActorRef<Event>,
        state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!(
            topic = %topic,
            subscriber_id = %subscriber.get_id(),
            "Subscribing actor to topic"
        );

        // Join the Process Group for this topic
        ractor::pg::join(topic.clone(), vec![subscriber.get_cell()]);

        // Update stats
        *state.subscription_stats.entry(topic.clone()).or_insert(0) += 1;

        tracing::info!(
            topic = %topic,
            subscriber = %subscriber.get_id(),
            "Actor subscribed to topic"
        );

        Ok(())
    }

    async fn handle_unsubscribe(
        &self,
        topic: String,
        subscriber: ActorRef<Event>,
        state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!(
            topic = %topic,
            subscriber_id = %subscriber.get_id(),
            "Unsubscribing actor from topic"
        );

        // Leave the Process Group
        ractor::pg::leave(topic.clone(), vec![subscriber.get_cell()]);

        // Update stats
        if let Some(count) = state.subscription_stats.get_mut(&topic) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.subscription_stats.remove(&topic);
            }
        }

        tracing::info!(
            topic = %topic,
            subscriber = %subscriber.get_id(),
            "Actor unsubscribed from topic"
        );

        Ok(())
    }

    async fn handle_get_subscribers(
        &self,
        topic: String,
        reply: RpcReplyPort<Vec<ractor::ActorId>>,
        _state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        // Get members of the process group for this topic
        let members = ractor::pg::get_members(&topic);
        let actor_ids: Vec<ractor::ActorId> = members
            .iter()
            .map(|cell| cell.get_id())
            .collect();

        // Send reply (ignore errors - caller may have timed out)
        let _ = reply.send(actor_ids);

        Ok(())
    }

    async fn handle_query_events(
        &self,
        _topic: String,
        _since: DateTime<Utc>,
        _limit: usize,
        reply: RpcReplyPort<Vec<Event>>,
        _state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        // TODO: Implement query against EventStoreActor
        // For now, return empty list
        let _ = reply.send(Vec::new());
        Ok(())
    }

    async fn broadcast_to_topic(
        &self,
        topic: &str,
        event: &Event,
    ) -> Result<(), ActorProcessingErr> {
        // Get all members of the process group for this topic
        let members = ractor::pg::get_members(&topic.to_string());
        
        for member in members {
            let actor_id = member.get_id();
            // Convert ActorCell to ActorRef<Event> and send
            let actor_ref: ActorRef<Event> = member.into();
            if let Err(e) = ractor::cast!(actor_ref, event.clone()) {
                tracing::warn!(
                    topic = %topic,
                    actor_id = %actor_id,
                    error = %e,
                    "Failed to send event to subscriber"
                );
            }
        }

        Ok(())
    }

    async fn broadcast_to_wildcards(
        &self,
        event: &Event,
    ) -> Result<(), ActorProcessingErr> {
        // Handle wildcard patterns like "worker.*"
        // Split topic and broadcast to parent patterns
        let parts: Vec<&str> = event.topic.split('.').collect();

        for i in 1..parts.len() {
            let wildcard_topic = format!("{}.*", parts[..i].join("."));
            self.broadcast_to_topic(&wildcard_topic, event).await?;
        }

        // Also broadcast to root wildcard
        self.broadcast_to_topic("*", event).await?;

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convenience function to publish an event
pub async fn publish_event(
    event_bus: &ActorRef<EventBusMsg>,
    event: Event,
    persist: bool,
) -> Result<(), ractor::RactorErr<EventBusMsg>> {
    cast!(event_bus, EventBusMsg::Publish { event, persist })
}

/// Convenience function to subscribe to a topic
pub async fn subscribe(
    event_bus: &ActorRef<EventBusMsg>,
    topic: impl Into<String>,
    subscriber: ActorRef<Event>,
) -> Result<(), ractor::RactorErr<EventBusMsg>> {
    cast!(
        event_bus,
        EventBusMsg::Subscribe {
            topic: topic.into(),
            subscriber,
        }
    )
}

/// Convenience function to unsubscribe from a topic
pub async fn unsubscribe(
    event_bus: &ActorRef<EventBusMsg>,
    topic: impl Into<String>,
    subscriber: ActorRef<Event>,
) -> Result<(), ractor::RactorErr<EventBusMsg>> {
    cast!(
        event_bus,
        EventBusMsg::Unsubscribe {
            topic: topic.into(),
            subscriber,
        }
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_matches_topic_exact() {
        let event = Event {
            id: "test".to_string(),
            event_type: EventType::Custom("test".to_string()),
            topic: "worker.task.complete".to_string(),
            payload: json!({}),
            timestamp: Utc::now(),
            source: "test".to_string(),
            correlation_id: None,
        };

        assert!(event.matches_topic("worker.task.complete"));
        assert!(!event.matches_topic("worker.task"));
        assert!(!event.matches_topic("worker.task.complete.extra"));
    }

    #[test]
    fn test_event_matches_topic_wildcard() {
        let event = Event {
            id: "test".to_string(),
            event_type: EventType::Custom("test".to_string()),
            topic: "worker.task.complete".to_string(),
            payload: json!({}),
            timestamp: Utc::now(),
            source: "test".to_string(),
            correlation_id: None,
        };

        assert!(event.matches_topic("worker.*"));
        assert!(event.matches_topic("worker.task.*"));
        assert!(event.matches_topic("*"));
        assert!(!event.matches_topic("other.*"));
        assert!(!event.matches_topic("worker.other.*"));
    }

    #[test]
    fn test_event_new() {
        let event = Event::new(
            EventType::WorkerComplete,
            "test.topic",
            json!({"key": "value"}),
            "test-source",
        )
        .unwrap();

        assert_eq!(event.event_type, EventType::WorkerComplete);
        assert_eq!(event.topic, "test.topic");
        assert_eq!(event.payload, json!({"key": "value"}));
        assert_eq!(event.source, "test-source");
        assert!(event.correlation_id.is_none());
        assert!(!event.id.is_empty());
    }

    #[test]
    fn test_event_with_correlation_id() {
        let event = Event::new(
            EventType::WorkerComplete,
            "test.topic",
            json!({}),
            "test-source",
        )
        .unwrap()
        .with_correlation_id("corr-123");

        assert_eq!(event.correlation_id, Some("corr-123".to_string()));
    }
}
