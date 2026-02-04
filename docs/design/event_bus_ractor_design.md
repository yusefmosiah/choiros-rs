# Design Document: Ractor EventBusActor

**Status:** Draft  
**Created:** 2026-02-04  
**Author:** ChoirOS Team  
**Related:** `docs/handoffs/2026-02-01-event-bus-implementation.md`

---

## 1. Overview

The EventBusActor provides pub/sub event distribution for the ChoirOS automatic computer architecture. Unlike the original Actix-based design, this implementation leverages ractor's native Process Groups (PG) for topic-based messaging, resulting in simpler code and better supervision.

### 1.1 Goals
- Enable publish/subscribe event distribution across actors
- Support topic-based routing with wildcards
- Integrate with EventStoreActor for persistence
- Provide WebSocket streaming for dashboard
- Maintain strict ordering guarantees per topic
- Handle backpressure gracefully

### 1.2 Non-Goals
- Global event ordering across topics (not required)
- Exactly-once delivery (at-least-once is sufficient)
- Persistent subscriptions (actors must re-subscribe on restart)
- Event sourcing (handled by EventStoreActor)

---

## 2. Architecture

### 2.1 Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        EventBusActor                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  State:                                                 │   │
│  │  - event_store: ActorRef<EventStoreMsg>                │   │
│  │  - subscription_cache: HashMap<String, Vec<ActorId>>   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│          ┌───────────────────┼───────────────────┐             │
│          ▼                   ▼                   ▼             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   Publish    │  │  Subscribe   │  │ GetSubscribers│         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└─────────────────────────────────────────────────────────────────┘
           │                │                │
           ▼                ▼                ▼
    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
    │  ractor::pg  │  │  ractor::pg  │  │  Cache Query │
    │  broadcast() │  │  join()      │  │              │
    └──────────────┘  └──────────────┘  └──────────────┘
```

### 2.2 Actor Hierarchy

```
Supervisor (System)
    ├── EventBusActor (singleton)
    │   └── Manages topic subscriptions via ractor::pg
    │
    ├── EventStoreActor (singleton)
    │   └── Persists all published events
    │
    ├── WebSocketActor (per connection)
    │   └── Subscribes to topics, streams to client
    │
    └── WorkerActors (dynamic)
        └── Publish events, subscribe to topics
```

### 2.3 Message Flow

**Publish Flow:**
```
Publisher → EventBusActor::Publish
                │
                ├──► EventStoreActor::AppendEvent (persistence)
                │
                └──► ractor::pg::broadcast(topic, event)
                            │
                            └──► All subscribed actors receive Event
```

**Subscribe Flow:**
```
Subscriber → EventBusActor::Subscribe
                 │
                 ├──► ractor::pg::join(topic, subscriber_ref)
                 │
                 └──► Update subscription_cache
```

---

## 3. Data Types

### 3.1 Event

```rust
/// Core event type for the event bus
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    ) -> Result<Self, serde_json::Error>;
    
    /// Check if this event matches a topic pattern
    /// Supports wildcards: "worker.*" matches "worker.task", "worker.job"
    pub fn matches_topic(&self, pattern: &str) -> bool;
}
```

### 3.2 EventType

```rust
/// Standardized event types for the automatic computer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[strum(serialize_all = "snake_case")]
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
```

### 3.3 EventBus Messages

```rust
/// Messages handled by EventBusActor
#[derive(Debug, Clone)]
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
        reply: RpcReplyPort<Vec<ActorId>>,
    },
    
    /// Query recent events from EventStore
    QueryEvents {
        topic: String,
        since: DateTime<Utc>,
        limit: usize,
        reply: RpcReplyPort<Vec<Event>>,
    },
}

impl Message for EventBusMsg {}
```

---

## 4. EventBusActor Implementation

### 4.1 Actor State

```rust
pub struct EventBusActor {
    /// Reference to EventStoreActor for persistence
    event_store: ActorRef<EventStoreMsg>,
    
    /// Cache of topic -> subscriber count (for metrics/debugging)
    subscription_stats: HashMap<String, usize>,
    
    /// Configuration
    config: EventBusConfig,
}

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
```

### 4.2 Actor Implementation

```rust
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
        Ok(EventBusState {
            event_store: args.event_store,
            subscription_stats: HashMap::new(),
            config: args.config,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
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
            EventBusMsg::QueryEvents { topic, since, limit, reply } => {
                self.handle_query_events(topic, since, limit, reply, state).await
            }
        }
    }
}
```

### 4.3 Handler Methods

```rust
impl EventBusActor {
    async fn handle_publish(
        &self,
        event: Event,
        persist: bool,
        state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        // Persist if requested and not in no-persist list
        let should_persist = persist 
            && state.config.default_persist 
            && !state.config.no_persist_topics.contains(&event.topic);
        
        if should_persist {
            // Convert Event to EventStore format and append
            let store_event = self.event_to_store_event(&event);
            cast!(state.event_store, EventStoreMsg::Append(store_event))?;
        }
        
        // Broadcast to all subscribers via Process Groups
        // This uses ractor's native pub/sub
        pg::broadcast(&event.topic, event.clone()).await;
        
        // Also broadcast to wildcard subscribers
        // e.g., "worker.*" subscribers get "worker.task.complete" events
        self.broadcast_to_wildcards(&event).await;
        
        Ok(())
    }
    
    async fn handle_subscribe(
        &self,
        topic: String,
        subscriber: ActorRef<Event>,
        state: &mut EventBusState,
    ) -> Result<(), ActorProcessingErr> {
        // Join the Process Group for this topic
        pg::join(topic.clone(), vec![subscriber.get_cell()]).await;
        
        // Update stats
        *state.subscription_stats.entry(topic).or_insert(0) += 1;
        
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
        // Leave the Process Group
        pg::leave(topic.clone(), vec![subscriber.get_cell()]).await;
        
        // Update stats
        if let Some(count) = state.subscription_stats.get_mut(&topic) {
            *count = count.saturating_sub(1);
        }
        
        Ok(())
    }
    
    async fn broadcast_to_wildcards(&self, event: &Event) -> Result<(), ActorProcessingErr> {
        // Handle wildcard patterns like "worker.*"
        // Split topic and broadcast to parent patterns
        let parts: Vec<&str> = event.topic.split('.').collect();
        
        for i in 1..parts.len() {
            let wildcard_topic = format!("{}.*", parts[..i].join("."));
            pg::broadcast(&wildcard_topic, event.clone()).await;
        }
        
        Ok(())
    }
}
```

---

## 5. Integration Points

### 5.1 WebSocket Actor

```rust
/// Actor that bridges WebSocket connections to EventBus
pub struct WebSocketActor {
    /// WebSocket sender
    ws_sink: futures::stream::SplitSink<...>,
    
    /// Topics this connection is subscribed to
    subscribed_topics: Vec<String>,
    
    /// EventBus reference
    event_bus: ActorRef<EventBusMsg>,
}

#[async_trait]
impl Actor for WebSocketActor {
    type Msg = Event; // Receives events directly from EventBus
    type State = WebSocketState;
    type Arguments = WebSocketArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Subscribe to requested topics
        for topic in args.topics {
            cast!(args.event_bus, EventBusMsg::Subscribe {
                topic: topic.clone(),
                subscriber: myself.clone(),
            })?;
        }
        
        Ok(WebSocketState {
            ws_sink: args.ws_sink,
            subscribed_topics: args.topics,
            event_bus: args.event_bus,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Serialize event and send to WebSocket
        let json = serde_json::to_string(&message)?;
        state.ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }
    
    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Unsubscribe from all topics
        for topic in &state.subscribed_topics {
            cast!(state.event_bus, EventBusMsg::Unsubscribe {
                topic: topic.clone(),
                subscriber: myself.clone(),
            })?;
        }
        Ok(())
    }
}
```

### 5.2 Worker Integration

```rust
/// Trait for actors that publish events
pub trait EventPublisher {
    fn event_bus(&self) -> &ActorRef<EventBusMsg>;
    
    async fn emit_event(
        &self,
        event_type: EventType,
        topic: impl Into<String>,
        payload: impl Serialize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = Event::new(event_type, topic, payload, self.actor_id())?;
        cast!(self.event_bus(), EventBusMsg::Publish {
            event,
            persist: true,
        })?;
        Ok(())
    }
}
```

---

## 6. Testing Strategy

### 6.1 Unit Tests

See `sandbox/src/actors/event_bus_test.rs` for:
- Publish/subscribe roundtrip
- Topic matching with wildcards
- Subscriber cleanup on disconnect
- Event ordering within topic
- Backpressure handling

### 6.2 Integration Tests

See `tests/event_bus_integration_test.rs` for:
- WebSocket event streaming
- Worker → EventBus → Dashboard flow
- Event persistence and replay
- Multiple concurrent subscribers

### 6.3 Property-Based Tests

- Event ordering guarantees per topic
- No message loss under load
- Subscriber isolation (one slow subscriber doesn't block others)

### 6.4 Load Tests

- 10k events/second throughput
- 100 concurrent subscribers
- Memory usage under sustained load
- Latency percentiles (p50, p95, p99)

---

## 7. Migration Path

### Phase 0: EventBusActor (Current)
- Build EventBusActor with ractor
- Bridge to existing Actix EventStoreActor
- Test pub/sub functionality

### Phase 1: EventStoreActor
- Convert EventStoreActor to ractor
- Remove Actix bridge
- Unified ractor supervision tree

### Phase 2: Remaining Actors
- Convert ChatActor, ChatAgent, DesktopActor
- TerminalActor built fresh with ractor
- Remove all Actix dependencies

---

## 8. Open Questions

1. **Wildcard granularity**: Support "*" only at end, or full glob patterns?
2. **Event retention**: How long to persist events in EventStore?
3. **Backpressure strategy**: Drop events, buffer, or apply backpressure?
4. **Security**: Topic-level access control needed?

---

## 9. References

- [Ractor Documentation](https://docs.rs/ractor/latest/ractor/)
- [Ractor Process Groups](https://docs.rs/ractor/latest/ractor/pg/index.html)
- Original handoff: `docs/handoffs/2026-02-01-event-bus-implementation.md`
- Architecture: `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md`
