# Handoff: Event Bus Implementation

**Created:** 2026-02-01  
**Continues from:** `docs/handoffs/2026-02-01-183056-docs-coherence-critique.md`  
**Status:** Ready for implementation

---

## Context

We've defined the automatic computer architecture. Now we need to build the event bus - the pub/sub system that enables async workers and observability.

**Key Problem:** Current EventStoreActor only stores events. We need it to broadcast events to subscribers.

**Reference:** `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md`

---

## Current State

**EventStoreActor** (`sandbox/src/actors/event_store.rs`):
- Stores events in SQLite/libsql
- Supports querying by actor_id
- No pub/sub capability

**What we need:**
- Broadcast events to multiple subscribers
- Support for topics (not just actor isolation)
- WebSocket integration for dashboard
- Event schema standardization

---

## Implementation Plan

### Phase 1: Event Bus Actor

Create new `EventBusActor`:

```rust
pub struct EventBusActor {
    subscribers: HashMap<String, Vec<Recipient<Event>>>,
    event_store: Addr<EventStoreActor>,
}

impl EventBusActor {
    pub fn subscribe(&mut self, topic: String, subscriber: Recipient<Event>);
    pub fn publish(&self, topic: String, event: Event);
    pub fn unsubscribe(&mut self, topic: String, subscriber: Recipient<Event>);
}
```

### Phase 2: Event Schema

Standardize event types:

```rust
pub enum EventType {
    WorkerSpawned,
    WorkerProgress,
    WorkerComplete,
    WorkerFailed,
    FindingNew,
    ChatMessage,
    FileChanged,
    UserInput,
}

pub struct Event {
    pub id: String,
    pub event_type: EventType,
    pub topic: String,
    pub payload: JsonValue,
    pub timestamp: DateTime<Utc>,
    pub source: String, // actor_id or user_id
}
```

### Phase 3: WebSocket Integration

Dashboard connects via WebSocket:

```rust
// WebSocket handler
async fn ws_chat(
    req: HttpRequest,
    stream: web::Payload,
    event_bus: web::Data<Addr<EventBusActor>>,
) -> Result<HttpResponse, Error> {
    // Subscribe to topics
    // Stream events to client
    // Handle client messages
}
```

### Phase 4: Worker Integration

Workers emit events:

```rust
// In worker
event_bus.do_send(PublishEvent {
    topic: "findings.new".to_string(),
    event: Event {
        event_type: EventType::FindingNew,
        payload: json!({"category": "SECURITY", "description": "..."}),
        ...
    }
});
```

---

## Files to Modify

1. **sandbox/src/actors/event_bus.rs** - New file
2. **sandbox/src/actors/mod.rs** - Add EventBusActor
3. **sandbox/src/api/websocket.rs** - Integrate with event bus
4. **sandbox/src/main.rs** - Start EventBusActor
5. **skills/actorcode/dashboard/app.js** - Consume WebSocket events

---

## Success Criteria

### Functional
- [ ] EventBusActor can publish/subscribe
- [ ] Multiple subscribers receive same event
- [ ] WebSocket streams events to dashboard
- [ ] Workers can emit events
- [ ] Dashboard shows real-time updates
- [ ] Events persisted to SQLite for replay

### Testing (RECOVERY STANDARD - Must Be Higher)

**Unit Tests:**
- [ ] EventBusActor publish/subscribe/unsubscribe
- [ ] Event schema validation
- [ ] Topic matching (exact and wildcard)
- [ ] Subscriber cleanup on disconnect

**Integration Tests:**
- [ ] WebSocket event streaming
- [ ] Worker → EventBus → Dashboard flow
- [ ] Event persistence and replay
- [ ] Multiple concurrent subscribers

**Property-Based Tests:**
- [ ] Event ordering guarantees
- [ ] No message loss under load
- [ ] Subscriber isolation (one slow subscriber doesn't block others)

**Fuzz Tests:**
- [ ] Random event payloads
- [ ] Malformed topic names
- [ ] Rapid subscribe/unsubscribe cycles
- [ ] Memory leaks under sustained load

**Agentic Red Teaming:**
- [ ] Spawn workers that intentionally crash mid-event
- [ ] Subscribe to non-existent topics
- [ ] Publish events with huge payloads
- [ ] Rapid topic creation/destruction
- [ ] Simulate network partitions

**Load/Performance Tests (Testarossa):**
- [ ] 10k events/second throughput
- [ ] 100 concurrent subscribers
- [ ] Memory usage under sustained load
- [ ] Latency percentiles (p50, p95, p99)

---

## Open Questions

1. **Persistence**: Should all events be stored or only some?
2. **Topics**: Hierarchical ("worker.*") or flat?
3. **Backpressure**: What if dashboard can't keep up?
4. **Security**: Can any client subscribe to any topic?

---

## Related Resources

- `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Architecture overview
- `docs/dev-blog/2026-02-01-why-agents-need-actors.md` - Why we need this
- `skills/actorcode/dashboard/` - Dashboard that will consume events
- `sandbox/src/actors/event_store.rs` - Current event storage

---

**Recovery Principle:** After system failure, testing standards must be STRICTER. No half-assed work.

**Next Step:** Implement EventBusActor with comprehensive test suite
