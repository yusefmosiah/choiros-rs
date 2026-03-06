# Event Schema Design for Multi-Agent LLM Systems

## Narrative Summary (1-minute read)

This report provides practical recommendations for extending ChoirOS's existing event schema to support distributed tracing and causality tracking in multi-agent LLM workflows. The design builds on your current EventStoreActor foundation and OpenTelemetry best practices, adding trace/span context, event taxonomy, and versioning strategies. Key recommendations include: (1) Add W3C-compliant trace/span IDs to enable cross-actor causality tracking, (2) Distinguish between envelope metadata (immutable) and payload data (mutable) for clearer semantics, (3) Implement a hierarchical event taxonomy with 18 types covering actor lifecycle, model calls, and tool interactions, (4) Use both wall-clock timestamps (for debugging) and logical sequence numbers (for ordering), and (5) Adopt a schema version field with backward compatibility through additive fields only.

## What Changed

This is a new design document proposing enhancements to ChoirOS's event schema. No changes have been implemented yet.

## What To Do Next

1. Review and approve the proposed envelope field additions (trace_id, span_id, parent_span_id)
2. Implement the new event types in `shared-types/src/lib.rs`
3. Update EventStoreActor schema with migrations for new fields
4. Add trace context propagation to all actor message handlers
5. Create schema versioning policy document

---

## Executive Summary

ChoirOS uses Rust actors (ractor) with an event sourcing pattern via EventStoreActor. The current schema provides solid foundations but lacks explicit causality tracking, distributed tracing support, and comprehensive event taxonomy for multi-agent LLM workflows. This report addresses these gaps by:

1. **Distributed Tracing Integration**: Adapting OpenTelemetry/W3C Trace Context for actor systems
2. **ID Strategies**: Defining trace_id, span_id, parent_span_id, correlation_id, and causality_id semantics
3. **Event Taxonomy**: Providing 18+ event types covering agent lifecycle, model calls, and tool interactions
4. **Versioning Strategy**: Establishing forward-compatible schema evolution patterns
5. **Clock Strategies**: Distinguishing between logical ordering (seq) and wall-clock timing (timestamp)
6. **Idempotency**: Providing deduplication strategies for high-concurrency scenarios

---

## Recommended Envelope Field List

### Core Envelope (Immutable Metadata)

| Field | Type | Semantics | Required | Notes |
|-------|------|-----------|----------|--------|
| `seq` | `i64` | Global logical sequence number | Yes | Strictly increasing, provides total order |
| `event_id` | `String` | Unique event identifier | Yes | ULID for time-sortable uniqueness |
| `timestamp` | `DateTime<Utc>` | Wall-clock time of event | Yes | ISO 8601, for debugging/SLOs |
| `event_type` | `String` | Event type discriminator | Yes | Hierarchical (e.g., `actor.spawned`) |
| `schema_version` | `u32` | Schema version at event creation | Yes | Enables graceful migration |
| `actor_id` | `ActorId` | Producer actor ID | Yes | Source of event |
| `trace_id` | `String` | Distributed trace root ID | Recommended | W3C TraceContext 16-byte hex |
| `span_id` | `String` | Current span ID | Recommended | W3C TraceContext 8-byte hex |
| `parent_span_id` | `String` | Parent span ID | Optional | Links to causality chain |
| `correlation_id` | `String` | Business-level correlation | Optional | User request ID, session ID |
| `causality_id` | `String` | Operation causality token | Optional | Links across async boundaries |
| `event_kind` | `EventKind` | Semantic event classification | Recommended | See enum below |
| `status` | `EventStatus` | Event outcome status | Recommended | See enum below |
| `duration_ms` | `Option<u64>` | Operation duration | Optional | For completed operations |

### Payload (Mutable Event-Specific Data)

| Field | Type | Semantics |
|-------|------|-----------|
| `payload` | `serde_json::Value` | Event-specific structured data |
| `session_id` | `Option<String>` | Session scope (existing) |
| `thread_id` | `Option<String>` | Thread scope (existing) |

### Enums for Semantic Classification

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Actor lifecycle events (spawned, stopped, crashed)
    ActorLifecycle,
    /// LLM model invocations (start, completion, error)
    ModelInvocation,
    /// Tool execution (start, result, error)
    ToolExecution,
    /// Message passing between actors
    ActorMessage,
    /// Policy/decision events (access control, routing)
    PolicyDecision,
    /// System/administrative events (startup, shutdown)
    SystemEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Started,
    InProgress,
    Completed,
    Failed { error: String },
    Cancelled,
    Retry { attempt: u32 },
}
```

---

## Event Type Hierarchy

### Category 1: Actor Lifecycle (4 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `actor.spawned` | Actor created | `{ actor_type: String, supervisor: String, args: Value }` |
| `actor.started` | Actor initialized | `{ initialization_ms: u64 }` |
| `actor.stopped` | Actor gracefully stopped | `{ reason: String, final_state: Value }` |
| `actor.crashed` | Actor panic/failure | `{ error: String, stack_trace: String }` |

### Category 2: Model Invocation (5 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `model.invoke.start` | LLM call initiated | `{ model: String, prompt_tokens: u32, provider: String }` |
| `model.invoke.stream_chunk` | Streaming response chunk | `{ chunk_index: u32, delta_tokens: u32 }` |
| `model.invoke.complete` | LLM call finished | `{ completion_tokens: u32, finish_reason: String }` |
| `model.invoke.error` | LLM call failed | `{ error: String, retry_count: u32, model: String }` |
| `model.selection` | Model chosen for task | `{ model: String, reason: String, alternatives: [String] }` |

### Category 3: Tool Execution (4 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `tool.call.start` | Tool invocation started | `{ tool_name: String, args: Value, contract_version: u32 }` |
| `tool.call.result` | Tool succeeded | `{ result: Value, duration_ms: u64 }` |
| `tool.call.error` | Tool execution failed | `{ error: String, error_type: String }` |
| `tool.call.timeout` | Tool timed out | `{ timeout_ms: u32, partial_result: Value }` |

### Category 4: Actor Message Flow (2 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `actor.message.send` | Actor sent message | `{ to_actor: ActorId, msg_type: String, size_bytes: u32 }` |
| `actor.message.receive` | Actor received message | `{ from_actor: ActorId, msg_type: String, queue_depth: u32 }` |

### Category 5: Policy & Decisions (2 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `policy.allow` | Access/control granted | `{ policy: String, resource: String, context: Value }` |
| `policy.deny` | Access/control denied | `{ policy: String, reason: String, violation: String }` |

### Category 6: System Events (2 types)

| Event Type | Semantics | Payload Fields |
|-----------|-----------|----------------|
| `system.startup` | System initialization | `{ version: String, config_hash: String }` |
| `system.shutdown` | System termination | `{ reason: String, duration_seconds: u64 }` |

**Total: 19 event types** covering all ChoirOS multi-agent flows.

---

## Causality Model

### Parent/Child Span Relationships

```
Trace: TraceId = "4bf92f3577b34da6a3ce929d0e0e4736"

[HTTP Request]                    SpanId="00f067aa0ba902b"
    └─> [ChatAgent]               SpanId="00f067aa0ba902c"  parent="00f067aa0ba902b"
         └─> [ModelInvoke]         SpanId="00f067aa0ba902d"  parent="00f067aa0ba902c"
              └─> [ToolCall]       SpanId="00f067aa0ba902e"  parent="00f067aa0ba902d"
```

### ID Semantics

| ID Type | Purpose | Generation | Format | Lifetime |
|---------|---------|------------|--------|----------|
| `trace_id` | Root operation across all actors | At HTTP ingress point | W3C 16-byte hex (32 chars) | Entire request lifecycle |
| `span_id` | Unique span within trace | Per event | W3C 8-byte hex (16 chars) | Single operation |
| `parent_span_id` | Direct causality link | Inherited from parent | Same as span_id | Immutable |
| `correlation_id` | Business request tracking | At user request | UUID/ULID | End-user session |
| `causality_id` | Async operation continuity | At async spawn | UUID | Cross-boundary ops |
| `event_id` | Deduplication & lookup | Per event | ULID | Forever |

### Span Linking Patterns

**1. Synchronous Parent/Child:**
```rust
// In ChatActor processing message
let child_span_id = ulid::Ulid::new().to_string();
let event = Event {
    seq: /* assigned by store */,
    trace_id: Some(trace_id.clone()),
    span_id: child_span_id.clone(),
    parent_span_id: Some(current_span_id.clone()),
    event_type: "model.invoke.start".to_string(),
    event_kind: EventKind::ModelInvocation,
    status: EventStatus::Started,
    // ...
};
```

**2. Async Spawning (run_async):**
```rust
// Supervisor spawning parallel worker
let causality_id = ulid::Ulid::new().to_string();
spawn_worker(worker, CorrelationContext {
    trace_id: parent_trace_id.clone(),
    causality_id: causality_id.clone(),
    parent_span_id: parent_span_id.clone(),
});
// Worker creates new span, links via causality_id
```

**3. Cross-Process (uActor → Actor):**
```rust
// Meta envelope with trace context
let meta = ActorMessageMeta {
    trace_id: trace_id,
    span_id: span_id,
    causality_id: causality_id,
};
send_to_actor(actor_id, Message { meta, payload });
```

---

## Example Traces

### Trace 1: uActor → Actor (Meta/Secure Envelope)

**Scenario:** External system sends secure command to Supervisor

```
Event Stream:
1. [HTTP] actor.message.receive
   - seq: 1001
   - trace_id: "4bf92f3577b34da6a3ce929d0e0e4736"
   - span_id: "00f067aa0ba902b"
   - parent_span_id: null
   - event_type: "actor.message.receive"
   - actor_id: "ApplicationSupervisor"
   - event_kind: ActorMessage
   - status: Completed
   - payload: { from_actor: "uActor", msg_type: "SuperviseCommand", 
                meta: { secure_envelope: true, signature: "..." } }

2. [HTTP] policy.allow
   - seq: 1002
   - trace_id: "4bf92f3577b34da6a3ce929d0e0e4736"
   - span_id: "00f067aa0ba902c"
   - parent_span_id: "00f067aa0ba902b"
   - event_type: "policy.allow"
   - actor_id: "ApplicationSupervisor"
   - event_kind: PolicyDecision
   - status: Completed
   - payload: { policy: "supervisor_access", resource: "spawn_session" }

3. [HTTP] actor.spawned
   - seq: 1003
   - trace_id: "4bf92f3577b34da6a3ce929d0e0e4736"
   - span_id: "00f067aa0ba902d"
   - parent_span_id: "00f067aa0ba902c"
   - event_type: "actor.spawned"
   - actor_id: "SessionSupervisor"
   - event_kind: ActorLifecycle
   - status: Completed
   - payload: { actor_type: "SessionSupervisor", 
                supervisor: "ApplicationSupervisor" }
```

### Trace 2: AppActor → ToolActor (Typed Tool Contract)

**Scenario:** ChatAgent calls filesystem tool via ToolActor

```
Event Stream:
1. [HTTP] model.invoke.complete
   - seq: 2001
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902b"
   - parent_span_id: "00f067aa0ba902a"  // ChatAgent span
   - event_type: "model.invoke.complete"
   - actor_id: "ChatActor"
   - event_kind: ModelInvocation
   - status: Completed
   - payload: { completion_tokens: 15, 
                tool_calls: [{ tool: "read_file", args: { path: "/tmp/data.txt" } }] }

2. [HTTP] actor.message.send
   - seq: 2002
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902c"
   - parent_span_id: "00f067aa0ba902b"
   - event_type: "actor.message.send"
   - actor_id: "ChatActor"
   - event_kind: ActorMessage
   - status: Completed
   - payload: { to_actor: "ToolActor", msg_type: "TypedToolCall",
                contract: { version: 1, tool: "read_file" } }

3. [HTTP] actor.message.receive
   - seq: 2003
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902d"
   - parent_span_id: "00f067aa0ba902c"
   - event_type: "actor.message.receive"
   - actor_id: "ToolActor"
   - event_kind: ActorMessage
   - status: Completed
   - payload: { from_actor: "ChatActor", msg_type: "TypedToolCall" }

4. [HTTP] tool.call.start
   - seq: 2004
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902e"
   - parent_span_id: "00f067aa0ba902d"
   - event_type: "tool.call.start"
   - actor_id: "ToolActor"
   - event_kind: ToolExecution
   - status: Started
   - payload: { tool_name: "read_file", 
                args: { path: "/tmp/data.txt" }, 
                contract_version: 1 }

5. [HTTP] tool.call.result
   - seq: 2005
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902e"  // Same span, updating
   - parent_span_id: "00f067aa0ba902d"
   - event_type: "tool.call.result"
   - actor_id: "ToolActor"
   - event_kind: ToolExecution
   - status: Completed
   - payload: { result: { content: "file contents..." }, duration_ms: 12 }

6. [HTTP] actor.message.send
   - seq: 2006
   - trace_id: "004067aa0ba902b766872651a637492"
   - span_id: "00f067aa0ba902f"
   - parent_span_id: "00f067aa0ba902e"
   - event_type: "actor.message.send"
   - actor_id: "ToolActor"
   - event_kind: ActorMessage
   - status: Completed
   - payload: { to_actor: "ChatActor", msg_type: "TypedToolResult" }
```

---

## Versioning Strategy

### Schema Version Field

**Field:** `schema_version: u32` (required)

**Semantics:** Indicates the version of the event schema in use when the event was created.

**Rules:**
1. Increment schema_version only on **breaking changes**
2. **Additive changes** (new optional fields) do not require version increment
3. **Renaming/removing fields** requires version increment + migration
4. Consumers must handle all versions ≤ current version
5. Producers must use latest schema_version

### Compatibility Matrix

| Schema Version | Changes | Backward Compatible | Forward Compatible |
|---------------|----------|---------------------|--------------------|
| 1 | Initial schema | N/A | N/A |
| 2 | Add trace_id, span_id (optional) | Yes (old code ignores) | No (new data missing) |
| 3 | Make trace_id required | No (old events missing) | Yes |
| 4 | Rename `user_id` → `user_principal_id` | No | Yes (if alias preserved) |

### Migration Strategy

**Step 1: Add New Optional Fields (No Version Increment)**
```sql
-- Schema v1 → v2 (additive)
ALTER TABLE events ADD COLUMN trace_id TEXT;
ALTER TABLE events ADD COLUMN span_id TEXT;
ALTER TABLE events ADD COLUMN parent_span_id TEXT;
-- schema_version remains 1 (optional fields)
```

**Step 2: Make Fields Required (Version Increment)**
```sql
-- Schema v2 → v3 (breaking)
ALTER TABLE events ADD COLUMN schema_version INTEGER DEFAULT 3;
-- Backfill existing events
UPDATE events SET trace_id = '00000000000000000000000000000000000' 
WHERE trace_id IS NULL;
-- Remove NULL constraint
-- Update producers to set schema_version=3
```

**Step 3: Field Removal/Rename (Version Increment + Alias)**
```sql
-- Schema v3 → v4 (breaking)
ALTER TABLE events ADD COLUMN user_principal_id TEXT;
-- Migrate data
UPDATE events SET user_principal_id = user_id;
-- Keep old column for compatibility (grace period)
-- Update consumers to prefer user_principal_id
-- Later migration: DROP COLUMN user_id
```

### Event Payload Versioning

**Pattern:** Version payload schemas independently of envelope:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "payload_version")]
enum ToolCallPayload {
    #[serde(rename = "1")]
    V1 { tool: String, args: Value },
    
    #[serde(rename = "2")]
    V2 { 
        tool: String, 
        args: Value, 
        timeout_ms: Option<u32>,  // New field
        retry_policy: Option<RetryPolicy>,  // New field
    },
}

// Event envelope
pub struct Event {
    // ... envelope fields
    pub payload: ToolCallPayload,  // Self-describing version
}
```

---

## Wall-Clock vs Logical Clocks

### Wall-Clock: `timestamp: DateTime<Utc>`

**Purpose:** 
- Debugging (human-readable)
- SLA/SLO measurement
- Anomaly detection (latency spikes)
- Cross-system correlation (logs, metrics)

**Precision:** Millisecond minimum, nanosecond preferred

**Usage:**
```rust
let now = Utc::now();
let duration_ms = now.signed_duration_since(start_time).num_milliseconds();
```

**Challenges:**
- Clock drift across machines
- NTP adjustments causing backward time
- Not suitable for ordering

### Logical Clock: `seq: i64`

**Purpose:**
- Total ordering (strict monotonic)
- Event deduplication
- Replication consistency
- Causality enforcement

**Generation:** Monotonic counter at EventStoreActor

**Usage:**
```sql
-- Guaranteed global ordering
SELECT seq, timestamp, event_type 
FROM events 
WHERE actor_id = ?1 
ORDER BY seq ASC;
```

**Properties:**
- Single writer (EventStoreActor) guarantees monotonicity
- Survives clock resets
- Enables replay/replication

### Hybrid Strategy: Use Both

**Primary Ordering:** `seq` (for correctness)

**Secondary Timing:** `timestamp` (for observability)

**Example Query:**
```rust
// Find events within time window, ordered logically
let events = db.query(
    "SELECT seq, timestamp, event_type 
     FROM events 
     WHERE timestamp BETWEEN ?1 AND ?2 
     ORDER BY seq ASC",
    [start_time, end_time]
)?;
```

### Duration Tracking

**Pattern:** Store `duration_ms` as event attribute for completed operations:

```rust
pub struct Event {
    // ...
    pub duration_ms: Option<u64>,  // Calculated after completion
}

// When creating completion event
let completion_event = Event {
    // ...
    duration_ms: Some(
        end_timestamp.signed_duration_since(start_timestamp).num_milliseconds() as u64
    ),
};
```

**Benefits:**
- Enables SLO calculations (p50, p95, p99 latency)
- Detects performance regressions
- Identifies slow operations without external timing

---

## Idempotency and Deduplication

### Event-Level Idempotency

**Strategy:** Use `event_id` ULID for deduplication

**Implementation:**
```sql
-- Unique constraint on event_id
CREATE UNIQUE INDEX idx_events_event_id ON events(event_id);

-- Idempotent insert
INSERT INTO events (event_id, seq, ...)
VALUES (?1, ?2, ...)
ON CONFLICT (event_id) DO NOTHING;
```

**Actor-Side Pattern:**
```rust
async fn handle_message(&mut self, msg: Message, ctx: &Context) {
    let event_id = compute_event_id(&msg);  // Deterministic or ULID
    
    // Check if already processed
    if self.processed_events.contains(&event_id) {
        tracing::warn!("Duplicate event: {}", event_id);
        return;
    }
    
    // Process and mark
    self.process(msg).await?;
    self.processed_events.insert(event_id);
    
    // Persist event (idempotent)
    event_store.append(AppendEvent { event_id, ... }).await?;
}
```

### Stream-Level Deduplication

**Strategy:** Track `seq` per actor to prevent replay

```rust
pub struct ActorState {
    last_processed_seq: i64,
}

async fn process_events(&mut self, new_events: Vec<Event>) {
    for event in new_events {
        if event.seq <= self.last_processed_seq {
            continue;  // Already processed
        }
        self.handle_event(event).await?;
        self.last_processed_seq = event.seq;
    }
}
```

### High-Concurrency Considerations

**Challenge:** Multiple actors appending events concurrently

**Solution 1: EventStoreActor Serialization**
- EventStoreActor is single-threaded
- Guarantees monotonic `seq`
- Natural dedup via unique constraint

**Solution 2: Actor-Level Batching**
```rust
// Batch appends to reduce contention
let mut batch = Vec::new();
for task in tasks {
    batch.append(create_event(task));
}
event_store.append_batch(batch).await?;
```

**Solution 3: Conflict Detection via Trace Context**
```rust
// Detect duplicate work using causality_id
if self.active_causality_ids.contains(&causality_id) {
    return Err(ActorError::DuplicateOperation(causality_id));
}
```

### Retention and Cleanup

**Strategy:** Periodic cleanup of deduplication state

```rust
async fn cleanup_old_processed_events(&mut self, cutoff: DateTime<Utc>) {
    self.processed_events
        .retain(|(_, timestamp)| timestamp > &cutoff);
}
```

**Recommendation:** Keep 24-48 hours of deduplication state in memory; longer retention via EventStore queries.

---

## Implementation Recommendations

### Phase 1: Foundation (Week 1-2)

1. **Add trace context fields** to `Event` struct (optional initially)
   - `trace_id`, `span_id`, `parent_span_id`
   - `schema_version`, `event_kind`, `status`

2. **Implement trace context propagation** in message handlers
   - Extract from incoming messages
   - Attach to outgoing messages
   - Generate new spans for local operations

3. **Database migration** for new fields
   - Add columns as NULL
   - Index on `trace_id`, `(actor_id, trace_id)`

### Phase 2: Event Taxonomy (Week 3-4)

1. **Define new event types** in `shared-types/src/lib.rs`
   - Add 18-19 types covering all flows
   - Create payload schemas for each type

2. **Update event producers** to use new taxonomy
   - ChatActor: `model.invoke.*`, `tool.call.*`
   - TerminalActor: `tool.call.*`, `worker.task.*`
   - Supervisors: `actor.spawned`, `policy.*`

3. **Add event kind/status enums**
   - Enforce classification
   - Standardize status reporting

### Phase 3: Observability (Week 5-6)

1. **Implement span linking** across async boundaries
   - Use `causality_id` for `run_async` workers
   - Link `uActor → Actor` via secure envelope

2. **Add duration tracking** for completed operations
   - Calculate and store `duration_ms`
   - Enable latency histograms

3. **Build trace visualization** tool
   - Query by `trace_id`
   - Display parent/child relationships
   - Show timelines

### Phase 4: Versioning & Migration (Week 7-8)

1. **Document schema versioning policy**
   - When to increment
   - Backward/forward compatibility rules
   - Migration procedures

2. **Implement migration framework**
   - EventStoreActor auto-migration
   - Backfill scripts
   - Version validation

3. **Add idempotency guards**
   - Event-level deduplication
   - Actor-level `seq` tracking
   - Cleanup procedures

---

## Conclusion

This event schema design provides ChoirOS with enterprise-grade observability for multi-agent LLM workflows. By combining OpenTelemetry's proven trace context model with ractor's actor semantics, we enable:

- **Causality Tracking:** Clear parent/child relationships across actor boundaries
- **Distributed Tracing:** End-to-end visibility from HTTP request to tool execution
- **Scalable Taxonomy:** 19 event types covering all ChoirOS flows
- **Graceful Evolution:** Versioned schema with clear migration paths
- **Reliable Ordering:** Hybrid logical/wall-clock strategy
- **Robust Idempotency:** Deduplication at event and stream levels

The design builds incrementally on ChoirOS's existing EventStoreActor foundation, allowing phased adoption without disrupting current functionality. Each recommendation is grounded in production-proven patterns from OpenTelemetry, W3C, and distributed systems best practices.

---

## Appendix: Reference Implementation

### Complete Event Struct (Proposed)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../dioxus-desktop/src/types/generated.ts")]
pub struct Event {
    // === Core Envelope (Immutable) ===
    pub seq: i64,
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub schema_version: u32,
    pub actor_id: ActorId,
    
    // === Trace Context (Causality) ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causality_id: Option<String>,
    
    // === Classification ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<EventKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<EventStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    
    // === Payload (Mutable) ===
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}
```

### Trace Context Propagation Helper

```rust
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub correlation_id: Option<String>,
}

impl TraceContext {
    pub fn new_root() -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().as_simple().to_string(),
            span_id: ulid::Ulid::new().to_string(),
            parent_span_id: None,
            correlation_id: None,
        }
    }
    
    pub fn child_span(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: ulid::Ulid::new().to_string(),
            parent_span_id: Some(self.span_id.clone()),
            correlation_id: self.correlation_id.clone(),
        }
    }
}

// Usage in actor
async fn handle_request(&mut self, ctx: &mut Context<Self::Msg>, msg: Request) {
    let trace_ctx = msg.trace_context.unwrap_or_else(TraceContext::new_root);
    let child_span = trace_ctx.child_span();
    
    // Append event with trace context
    let event = Event {
        trace_id: Some(child_span.trace_id),
        span_id: Some(child_span.span_id),
        parent_span_id: child_span.parent_span_id,
        // ...
    };
    
    event_store.append(event).await?;
}
```

### Database Migration Script

```sql
-- Migration: Add trace context fields (schema v1 → v2)
-- This migration is additive (backward compatible)

BEGIN TRANSACTION;

-- Add trace context columns
ALTER TABLE events ADD COLUMN trace_id TEXT;
ALTER TABLE events ADD COLUMN span_id TEXT;
ALTER TABLE events ADD COLUMN parent_span_id TEXT;

-- Add classification columns
ALTER TABLE events ADD COLUMN event_kind TEXT;
ALTER TABLE events ADD COLUMN event_status TEXT;
ALTER TABLE events ADD COLUMN duration_ms INTEGER;

-- Add schema version column (default to 1 for existing)
ALTER TABLE events ADD COLUMN schema_version INTEGER DEFAULT 1;

-- Add indexes for trace queries
CREATE INDEX IF NOT EXISTS idx_events_trace_id ON events(trace_id);
CREATE INDEX IF NOT EXISTS idx_events_actor_trace ON events(actor_id, trace_id);

COMMIT;
```

---

**Document Version:** 1.0  
**Date:** 2025-02-08  
**Author:** ChoirOS Architecture Team  
**Status:** Draft for Review
