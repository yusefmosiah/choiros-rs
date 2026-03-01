# ChoirOS Multi-Agent Architecture Design

**Date:** 2026-02-06  
**Status:** Draft Design Document  
**Authors:** OpenCode + User Design Session  

---

## Executive Summary

ChoirOS is an actor-based multi-agent system where autonomous agents collaborate through a choir of actors. Each domain (Chat, Terminal, Desktop, Research, Docs) has its own supervision tree with independent lifecycle and failure isolation. Supervisors coordinate via direct RPC, while the EventBus provides cross-boundary observability and loose coupling.

**Key Principles:**
- **Supervision trees per actor type** - clear ownership boundaries
- **Deterministic and agentic handlers** - same actor can have both
- **Event sourcing is core** - all behavior traced for reconstruction
- **Hybrid communication** - direct RPC within trees, events across trees
- **Service actors are long-lived** - ResearcherActor, DocsUpdaterActor as infrastructure

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                  ApplicationSupervisor (Root)                    │
│  - Strategy: one_for_one, intensity=3, period=60s             │
│  - Global service registry                                      │
│  - EventBusActor (global pub/sub)                              │
│  - EventStoreActor (persistence)                               │
└─────────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ SupervisorWatchers│  │ SupervisorSession │  │ SupervisorServices│
│ one_for_one      │  │ one_for_one      │  │ one_for_one      │
│ intensity=5      │  │ intensity=5      │  │ intensity=5      │
└──────────────────┘  └──────────────────┘  └──────────────────┘
         │                    │                    │
         │                    ├─→ SupervisorChat │
         │                    │    one_for_one    │
         │                    │                    │
         │                    ├─→ SupervisorTerminal│
         │                    │    simple_one_for_one│
         │                    │                    │
         │                    └─→ SupervisorDesktop│
         │                         one_for_one    │
         │                                        │
         │                                        │
         │          ┌─────────────────────────────────┘
         │          │
         ▼          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    SERVICE ACTORS (Global)                        │
│  - ResearcherActor (web search, LLM inference)                 │
│  - DocsUpdaterActor (doc index, system model)                   │
│  - EventStoreActor (event sourcing)                             │
│  - EventBusActor (pub/sub, process groups)                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. Supervision Trees Per Actor Type

### 2.1 Why Per-Type Supervision?

**Problem:** Multiple concurrent actors of different types managing resources
- 3 Terminal windows, 2 controlled by ChatActor, 1 standalone
- Multiple ChatActors each with their own terminals
- Workers requesting research while watchers suggesting research
- Watchers detecting floundering across different supervision trees

**Solution:** Each actor type has its own supervisor
- Clear ownership boundaries
- Independent failure isolation
- Different supervision strategies per domain
- Natural resource partitioning

### 2.2 Supervisor Responsibilities

| Supervisor | Domain | Strategy | Children |
|------------|---------|----------|-----------|
| `SupervisorWatchers` | Observability | one_for_one | TestFailureWatcher, FlounderingWatcher, DocsStalenessWatcher |
| `SupervisorSession` | User sessions | one_for_one | SupervisorChat, SupervisorTerminal, SupervisorDesktop |
| `SupervisorChat` | Chat sessions | one_for_one | ChatActor (per conversation) |
| `SupervisorTerminal` | PTY sessions | simple_one_for_one | TerminalFactory → TerminalWorkers |
| `SupervisorDesktop` | Desktop instances | one_for_one | DesktopActor (per desktop) |
| `SupervisorServices` | Global services | one_for_one | ResearcherActor, DocsUpdaterActor |

**Supervisor Communication:**
- **Within supervision tree:** Direct RPC via `ractor::call!`
- **Across supervisors:** EventBusActor for pub/sub
- **Service discovery:** Global registry in ApplicationSupervisor

---

## 3. Dual Nature of Actors: Deterministic + Agentic

### 3.1 Deterministic Handlers
**Definition:** Predictable, stateless or state-machine-based handlers

**Examples:**
- `TerminalActor`: PTY I/O, process lifecycle
- `DesktopActor`: Window management, desktop state
- `EventStoreActor`: Event persistence, querying
- `DocsUpdaterActor` (indexing): Deterministic index updates

**Characteristics:**
- No LLM calls
- No external network I/O (except predictable APIs)
- Can be reconstructed from event stream
- Fast, low-latency

### 3.2 Agentic Handlers
**Definition:** LLM-powered, exploratory, decision-making handlers

**Examples:**
- `ChatActor`: Conversation reasoning, tool calling
- `ResearcherActor`: Web search, synthesis, summarization
- `DocsUpdaterActor` (indexing decisions): LLM-driven indexing strategy
- `WatcherActors`: Pattern recognition, anomaly detection

**Characteristics:**
- LLM inference
- External API calls (web search, code execution)
- Non-deterministic (different LLM responses)
- Can be retried with different context

### 3.3 Actor Pattern: Mixed Handlers

Same actor can have both deterministic and agentic handlers:

```rust
impl Actor for DocsUpdaterActor {
    type Msg = DocsUpdaterMsg;
    type State = DocsUpdaterState;

    async fn handle(&mut self, message: Self::Msg, state: &mut Self::State) {
        match message {
            // Deterministic handler
            DocsUpdaterMsg::UpdateIndex { doc_id, content } => {
                self.update_index_deterministic(doc_id, content, state);
            }

            // Agentic handler
            DocsUpdaterMsg::OptimizeIndexingStrategy { reply } => {
                let strategy = self.llm_infer_best_strategy(state).await;
                let _ = reply.send(strategy);
            }
        }
    }
}
```

---

## 4. Event Sourcing: Core Requirement

### 4.1 Why Event Sourcing?

**Reconstruction:** Can replay events to understand behavior
**Observability:** Full trace of system actions
**Debugging:** Reproduce issues from event stream
**Audit:** Immutable log of what happened, when, why

### 4.2 Event Design: Operational vs Observational

**Operational Events** (drive behavior):
- `TerminalOutput`: TerminalActor sends to update UI
- `DesktopStateChanged`: DesktopActor sends to refresh state
- `WindowFocused`: Window focus event handled by components

**Observational Events** (inform decisions):
- `ResearchCompleted`: Worker reads result for their task
- `WorkerFloundering`: Watcher signal to supervisor
- `TestsFailing`: Watcher signal for intervention

**Event Structure:**
```rust
pub struct Event {
    pub event_id: String,        // ULID for ordering
    pub event_type: EventType,    // TerminalOutput, ResearchCompleted, etc.
    pub topic: String,            // "terminal.output", "research.*"
    pub payload: Value,           // JSON payload
    pub actor_id: String,         // Which actor emitted
    pub correlation_id: Option<String>, // Link related events
    pub timestamp: DateTime<Utc>,
    pub persist: bool,            // Whether to store in EventStore
}
```

### 4.3 EventBus Filtering

**Process Groups with Wildcards:**
```rust
// TerminalActor publishes to specific topic
event_bus.publish(Event::new(
    EventType::TerminalOutput,
    "terminal.term-123.output",
    payload,
    "terminal-123",
)?, true).await?;

// ChatActor subscribes to all terminal events
event_bus.subscribe("terminal.*", chat_actor_ref).await?;

// FlounderingWatcher subscribes to worker status
event_bus.subscribe("worker.*.status", watcher_ref).await?;
```

**Filtering Logic (from ractor::pg):**
- Exact match: `"terminal.term-123.output"`
- Wildcard: `"terminal.*"` matches all terminal events
- Hierarchical: `"worker.*.status"` matches `"worker.abc.status"`, `"worker.xyz.status"`

---

## 5. Concurrency: Multiple Workers Requesting

### 5.1 Problem Scenario

```
Terminal #1: Worker A watching tests fail
Terminal #2: Worker B watching tests fail
Terminal #3: Worker C running build

→ Workers A & B both send "need research" signals
→ Both watchers detect floundering
→ 4 concurrent requests to ResearcherActor!
```

### 5.2 Solution: Request Deduplication

**ResearcherActor Queue with Deduplication:**

```rust
pub struct ResearcherActorState {
    queue: ResearchQueue,
    in_progress: HashSet<RequestId>,
    max_concurrent: usize,  // e.g., 3 researchers
}

pub struct ResearchRequest {
    request_id: String,
    query: String,
    context: ResearchContext,
    requester: ActorRef<ActorMsg>,
    timestamp: DateTime<Utc>,
}

impl ResearcherActor {
    async fn handle_request(&mut self, req: ResearchRequest, state: &mut State) {
        // Deduplication: Check if already queued/in-progress
        if state.in_progress.contains(&req.request_id) {
            // Send duplicate notification
            let _ = req.requester.cast(ResearcherMsg::Duplicate {
                request_id: req.request_id,
            }).await;
            return;
        }

        // Add to queue
        state.queue.push(req);
        state.in_progress.insert(req.request_id.clone());

        // Process if under concurrency limit
        if state.in_progress.len() <= state.max_concurrent {
            self.process_next(state).await;
        }
    }
}
```

**Correlation ID for Request Tracking:**

```rust
// Worker generates correlation ID
let correlation_id = ULID::new().to_string();

// Send request
researcher.send(ResearcherMsg::Research {
    correlation_id: correlation_id.clone(),
    query: "terminal error: permission denied".to_string(),
    reply: myself.clone(),
}).await?;

// Subscribe to results
event_bus.subscribe(&format!("research.{}", correlation_id), myself.clone()).await?;

// Receive result via event bus
Event { topic: "research.abc123", payload } => {
    self.handle_research_result(correlation_id, payload);
}
```

### 5.3 Load Balancing Across Multiple Researchers

**Spawn Multiple ResearcherActors:**

```rust
// SupervisorServices spawns researcher pool
for i in 0..MAX_RESEARCHERS {
    let (researcher, _) = Actor::spawn_linked(
        Some(format!("researcher-{}", i)),
        ResearcherActor,
        ResearcherArgs {
            event_store: event_store.clone(),
            index: i,
        },
        myself.get_cell(),
    ).await?;
    
    state.researchers.push(researcher);
}

// Round-robin or least-busy selection
impl ResearcherPool {
    fn select_researcher(&self) -> &ActorRef<ResearcherMsg> {
        // Round-robin
        let idx = self.current % self.researchers.len();
        self.current += 1;
        &self.researchers[idx]
    }
}
```

---

## 6. Watcher + Worker Collaboration

### 6.1 Problem Scenario

```
Worker A: Requests research on error X
Watcher B: Observes Worker A floundering, suggests research
→ Duplicate signals to Supervisor!
```

### 6.2 Solution: Watchers Signal, Not Request

**Watcher Pattern:**
```rust
impl WatcherActor {
    async fn observe(&mut self, event: Event, state: &mut State) {
        // Detect floundering
        if self.is_worker_floundering(&event, state) {
            // Emit signal event (don't request directly)
            event_bus.publish(Event::new(
                EventType::WatcherSignal,
                "watcher.floundering",
                json!({
                    "worker_id": event.actor_id,
                    "severity": "high",
                    "reason": "tests failing for 5m",
                }),
                self.actor_id.clone(),
            )?, false).await;  // Don't persist ephemeral signals
        }
    }
}
```

**Supervisor Pattern:**
```rust
impl SupervisorSession {
    async fn handle_watcher_signal(&mut self, signal: Event, state: &mut State) {
        match signal.event_type {
            EventType::WatcherSignal => {
                // Check if worker already requested help
                if state.worker_requests.contains_key(&signal.actor_id) {
                    // Worker already handling it, ignore watcher
                    info!("Watcher signal ignored: worker already requested help");
                    return;
                }

                // Proactive intervention
                self.call_researcher(signal.payload, state).await;
            }
        }
    }
}
```

### 6.3 Event Flow

```
┌─────────────┐    request    ┌──────────────────┐    RPC     ┌──────────────┐
│  Worker A   │ ───────────────▶│  SupervisorChat  │ ─────────▶│ Researcher   │
└─────────────┘                └──────────────────┘            └──────────────┘
       │                                                               │
       │ emits operational events                                        │ publishes
       ▼                                                               ▼
┌───────────────────────────────────────────────────────────────────────────────────┐
│                         EventBusActor                                       │
│  Topics:                                                                  │
│  - "worker.*.status" (worker status updates)                             │
│  - "terminal.*.output" (terminal output)                                   │
│  - "watcher.floundering" (watcher signals)                                │
│  - "research.*" (research results)                                         │
└───────────────────────────────────────────────────────────────────────────────────┘
       ▲                                                               ▲
       │ subscribes                                                     │ subscribes
┌─────────────┐                                                ┌──────────────┐
│ Watcher B   │                                                │ Worker A     │
│ (LLM agent) │◀─────────────────────────────────────────────────────────────────│
└─────────────┘   observes worker.status, emits watcher.floundering signal     │
                                                                              │
                                                                              │ receives
                                                                              ▼
                                                                      Event {
                                                                        topic: "research.abc123",
                                                                        payload: {...}
                                                                      }
```

---

## 7. Cross-Type Actor Control: Chat → Multiple Terminals

### 7.1 Problem Scenario

```
ChatActor #1 controls 2 terminals:
  - Terminal #1: Running `cargo test`
  - Terminal #2: Running `npm test`

ChatActor #2 controls 1 terminal:
  - Terminal #3: Running `make build`

→ How does ownership work? Lifecycle? Restart?
```

### 7.2 Solution: Session-Level Ownership

**Pattern: SessionSupervisor owns terminals, not ChatActor**

```rust
// SessionSupervisor spawns TerminalSupervisor per user
let (terminal_supervisor, _) = Actor::spawn_linked(
    Some(format!("terminal-supervisor-{}", user_id)),
    TerminalSupervisor,
    TerminalSupervisorArgs {
        user_id: user_id.clone(),
        event_store: event_store.clone(),
    },
    myself.get_cell(),  // SessionSupervisor supervises
).await?;

// TerminalSupervisor uses factory for worker pool
let factory = Factory::<String, TerminalMsg, ...>::spawn_linked(...).await?;
```

**ChatActor requests terminals, doesn't own them:**

```rust
// ChatActor requests terminal from supervisor
let terminal_ref = ractor::call!(session_supervisor, |reply| {
    SessionSupervisorMsg::GetOrCreateTerminal {
        terminal_id: "term-1".to_string(),
        user_id: user_id.clone(),
        reply,
    }
}).await?;

// Send commands to terminal
terminal_ref.cast(TerminalMsg::SendInput {
    input: "ls\n".to_string(),
}).await?;
```

**Lifecycle Semantics:**

| Scenario | Behavior |
|-----------|----------|
| ChatActor dies | Terminals continue running (SessionSupervisor owns them) |
| Terminal dies | ChatActor gets error, can request new terminal |
| SessionSupervisor dies | Both ChatActor and terminals die (cascading) |
| Worker requests terminal | Goes through SessionSupervisor (no ChatActor needed) |

### 7.3 Factory Pattern for Terminal Workers

**ractor::factory for High Cardinality:**

```rust
let factory = Factory::<
    String,           // Key: terminal_id
    TerminalMsg,      // Message type
    TerminalWorkerArgs,// Arguments
    TerminalSupervisorMsg, // StopAll, etc.
    TerminalSupervisorState,
>::default()
.worker_builder(worker_builder)
.router(KeyPersistentRouting::default())  // Same key → same worker
.start("terminal-factory", myself.get_cell())
.await?;
```

**Benefits:**
- Dynamic worker creation (terminals started on-demand)
- Key-based routing (same terminal_id → same worker)
- Automatic cleanup (factory stops all workers)

---

## 8. Service Actors: Researcher & DocsUpdater

### 8.1 ResearcherActor

**Responsibilities:**
- Web search (Google, docs.rs, Stack Overflow)
- LLM inference (research synthesis, summarization)
- Cache research results
- Emit `ResearchCompleted` events

**Message Protocol:**
```rust
pub enum ResearcherMsg {
    // Agentic handler
    Research {
        correlation_id: String,
        query: String,
        context: ResearchContext,
        reply: Option<ActorRef<ResearcherMsg>>,  // Direct reply or event bus
    },

    // Deterministic handler
    GetCached {
        query_hash: String,
        reply: RpcReplyPort<Option<ResearchResult>>,
    },

    // Agentic handler
    OptimizeSearchStrategy {
        reply: RpcReplyPort<SearchStrategy>,
    },
}
```

**State:**
```rust
pub struct ResearcherActorState {
    cache: LruCache<String, ResearchResult>,
    in_progress: HashMap<String, Instant>,  // request_id → start time
    max_concurrent: usize,
}
```

**Event Emission:**
```rust
impl ResearcherActor {
    async fn complete_research(&mut self, result: ResearchResult) {
        // Emit to event bus for all subscribers
        event_bus.publish(Event::new(
            EventType::ResearchCompleted,
            &format!("research.{}", result.correlation_id),
            json!(result),
            self.actor_id.clone(),
        )?, true).await;  // Persist research results
    }
}
```

### 8.2 DocsUpdaterActor

**Responsibilities:**
- Maintain in-memory index of entire system
- Update index on doc changes
- Answer queries about system state
- LLM-driven indexing strategy optimization

**Message Protocol:**
```rust
pub enum DocsUpdaterMsg {
    // Deterministic handler
    UpdateIndex {
        doc_id: String,
        content: String,
        metadata: DocMetadata,
    },

    // Deterministic handler
    QueryIndex {
        query: String,
        reply: RpcReplyPort<Vec<DocMatch>>,
    },

    // Agentic handler
    OptimizeIndexingStrategy {
        reply: RpcReplyPort<IndexingStrategy>,
    },

    // Agentic handler
    AnalyzeSystemHealth {
        reply: RpcReplyPort<SystemHealthReport>,
    },
}
```

**State:**
```rust
pub struct DocsUpdaterActorState {
    index: InvertedIndex,  // Fast lookups
    docs: HashMap<String, DocMetadata>,
    last_updated: DateTime<Utc>,
}
```

**Indexable Content:**
- Code documentation (Rust doc comments)
- Design documents (ARCHITECTURE_SPECIFICATION.md)
- Handoffs and retrospectives
- Research reports
- Test results and bug reports

---

## 9. Communication Patterns: RPC vs Events vs Multi-Agent

### 9.1 Direct RPC (Within Supervision Tree)

### 9.1 Direct RPC (Within Supervision Tree)

**Use when:**
- Request/response needed
- Strong ordering guarantees
- Point-to-point communication
- Within same supervisor domain

**Pattern:**
```rust
// ChatActor requests terminal
let terminal_ref = ractor::call!(session_supervisor, |reply| {
    SessionSupervisorMsg::GetOrCreateTerminal {
        terminal_id: "term-1".to_string(),
        user_id: user_id.clone(),
        reply,
    }
}).await?;
```

**Pros:**
- Type-safe (compile-time checking)
- Synchronous-like (async but waits for reply)
- Clear error handling (Result<T, E>)
- No need for correlation IDs

**Cons:**
- Tight coupling (caller knows receiver type)
- Blocking (await on reply)
- No fan-out (one-to-one only)

### 9.2 Event Bus (Across Supervision Trees)

**Use when:**
- Multiple subscribers needed
- Loose coupling required
- Fan-out notifications
- Cross-domain coordination
- Asynchronous coordination

**Pattern:**
```rust
// TerminalActor publishes output
event_bus.publish(Event::new(
    EventType::TerminalOutput,
    "terminal.term-123.output",
    json!({"output": "hello\n", "terminal_id": "term-123"}),
    "term-123",
)?, true).await?;

// Multiple subscribers receive:
// - ChatActor (if it owns this terminal)
// - Watcher (for anomaly detection)
// - EventStore (for persistence)
// - Any other interested actor
```

**Pros:**
- Decoupled (subscribers don't know publisher)
- Fan-out (many-to-many)
- Async (fire-and-forget)
- Cross-boundary coordination

**Cons:**
- No ordering guarantees (eventual consistency)
- No built-in request-response pattern
- Need correlation IDs for tracking
- Eventual delivery (not immediate)

### 9.3 Hybrid Approach

**Combine both for best of both worlds:**

```rust
// 1. Send command via RPC (direct, reliable)
let (tx, rx) = oneshot::channel();
terminal.cast(TerminalMsg::RunCommand { 
    cmd: "make test".to_string(), 
    correlation_id: "abc123".to_string(),
    reply: tx,
}).await?;

// 2. Subscribe to events for async updates
event_bus.subscribe("terminal.abc123.*", myself.clone()).await?;

// 3. Receive completion via event bus
async fn handle_event(&mut self, event: Event) {
    if event.correlation_id == Some("abc123".to_string()) {
        match event.event_type {
            EventType::CommandComplete => self.on_test_complete(event.payload),
            EventType::CommandFailed => self.on_test_failed(event.payload),
            _ => {}
        }
    }
}
```

### 9.4 Multi-Agent Coordination Pattern

**Full Multi-Agent Message Flow:**

```
┌─────────────────────────────────────────────────────────────────┐
│  SupervisorAgent (Orchestration)                             │
│  - Coordinates phases, verification, fixes                    │
│  - Load balances across workers, verifiers, fixers          │
└─────────────────────────────────────────────────────────────────┘
         │                            │                            │
         │                            │                            │
         ▼                            ▼                            ▼
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ Worker/Dev Agent │    │ VerifierActor    │    │ FixerActor       │
│ - Writes code    │    │ - Runs E2E tests │    │ - Fixes failures  │
│ - Unit tests     │    │ - Reports results │    │ - Investigates    │
└──────────────────┘    └──────────────────┘    └──────────────────┘
         │                            │                            │
         │ emits                    emits                   emits
         ▼                            ▼                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│  EventBusActor (Pub/Sub)                                   │
│  Topics:                                                        │
│  - "phase.*.complete" (phase completion)                       │
│  - "verify.phase.*" (verification status)                       │
│  - "fix.*.started" (fix started)                              │
│  - "fix.*.complete" (fix completed)                             │
│  - "research.*" (research results)                               │
│  - "docs.*.updated" (doc updates)                              │
└─────────────────────────────────────────────────────────────────────┘
         ▲                            ▲                            ▲
         │                            │                            │
         │ subscribes               subscribes              subscribes
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ ResearcherActor  │    │ DocsUpdaterActor  │    │ WatcherAgent     │
│ - Web search     │    │ - Index system   │    │ - Detect issues  │
│ - LLM inference │    │ - Answer queries │    │ - Suggest fixes  │
└──────────────────┘    └──────────────────┘    └──────────────────┘
```

**Agent Message Protocols Summary:**

| Agent | Sends | Receives | Key Coordination |
|--------|---------|------------|-------------------|
| **Worker/Dev** | PhaseComplete, QueryStatus | ContinuePhase, PhaseBroken | Supervisor |
| **Verifier** | VerificationComplete | (none) | EventBus (publishes) |
| **Fixer** | FixStarted, FixComplete, ResearchRequest | ResearchResults | Researcher |
| **Researcher** | ResearchComplete | (none) | EventBus (publishes) |
| **DocsUpdater** | DocsUpdated, DocsQueryResults | (none) | EventBus (subscribes) |
| **Watcher** | WatcherSignal, SuggestFix | (none) | EventBus (publishes) |
| **Supervisor** | All of above (coordinates) | All events | Central orchestrator |

**Reconciliation Flow (Hotfix Strategy):**

```
1. Worker completes Phase 1 (unit tests pass)
   → Emit: PhaseComplete

2. Supervisor commits Phase 1, starts Verifier (E2E tests)

3. Verifier runs E2E tests, emits: VerificationComplete (FAIL)

4. Supervisor analyzes failure:
   → Spawns FixerAgent (if not already fixing)
   → Alerts Worker: Continue Phase 2, but aware of Phase 1 issue

5. FixerAgent:
   → Researcher: Check for similar failures
   → Implement fix
   → Verify fix (E2E tests)
   → Commit: phase-1-hotfix

6. Fixer emits: FixComplete
   → Supervisor: Phase 1 fixed
   → DocsUpdater: Update phase status
   → Worker: Safe to merge hotfix if needed

7. Worker continues Phase 2 (now Phase 1 is stable)
```

**Key Design Principle:**
- **Supervisor is the central brain** - coordinates all agents
- **EventBus is the nervous system** - broadcasts all signals
- **Each agent is autonomous** - specialized role, publishes events
- **Non-blocking coordination** - agents fire events, don't wait for replies
- **Observer pattern** - agents subscribe to what they need, ignore the rest

---

## 10. Implementation Roadmap

### Phase 1: Supervision Tree Refactoring (Week 1)
- [ ] Create `SupervisorWatchers`
- [ ] Create `SupervisorServices`
- [ ] Move ChatActor spawning to `SupervisorChat`
- [ ] Ensure all TerminalActors under `SupervisorTerminal`
- [ ] Test failure isolation between domains

### Phase 2: Event Bus Integration (Week 1-2)
- [ ] Subscribe ChatActors to terminal output events
- [ ] Implement correlation IDs for request tracking
- [ ] Add event filtering in watchers
- [ ] Implement event replay for missed events

### Phase 3: Service Actors (Week 2)
- [ ] Implement `ResearcherActor` with LLM integration
- [ ] Implement `DocsUpdaterActor` with in-memory index
- [ ] Add request deduplication in ResearcherActor
- [ ] Add index update optimization in DocsUpdaterActor

### Phase 4: Watcher Implementation (Week 2-3)
- [ ] Implement `TestFailureWatcher`
- [ ] Implement `FlounderingWatcher`
- [ ] Implement `DocsStalenessWatcher`
- [ ] Add watcher signal → supervisor logic

### Phase 5: Cross-Tree Coordination (Week 3)
- [ ] Implement worker → supervisor → researcher flow
- [ ] Implement watcher → supervisor → service flow
- [ ] Add load balancing for multiple researchers
- [ ] Test concurrent request scenarios

---

## 11. Open Questions & Design Decisions

### 11.1 Per-Supervisor Event Buses

**Question:** Should each supervisor have its own event bus, or one global bus?

**Options:**
1. **Single global event bus** (simple, but bottleneck)
2. **Per-supervisor event bus** (fault isolation, but need bridging)
3. **Hierarchical event buses** (local + global forwarding)

**Recommendation:** **Option 3 - Hierarchical**
- Local events stay within supervisor (fast)
- Cross-domain events forwarded to global bus
- Combines benefits of both

### 11.2 State Reconstruction vs In-Memory

**Question:** Should service actor state (docs index, research cache) be event-sourced or in-memory?

**Options:**
1. **Full event sourcing** (replay events to rebuild state)
2. **In-memory + event persistence** (current state persisted separately)
3. **Hybrid** (critical data event-sourced, cache in-memory)

**Recommendation:** **Option 3 - Hybrid**
- EventStore has all events (for reconstruction)
- DocsUpdater has in-memory index (for fast queries)
- On restart: rebuild index from events (bootstrap phase)

### 11.3 Monitor vs Link for Cross-Tree Observation

**Question:** Should supervisors use ractor monitors to observe actors in other trees?

**Current:** Links only (direct supervisor-child)
**Consider:** Monitors (one-way observation, no lifecycle coupling)

**Recommendation:** **Evaluate monitors for debugging/diagnostic actors only**
- Keep links for production supervision
- Use monitors for system health dashboards
- Enable `monitors` feature flag in Cargo.toml

### 11.4 Service Discovery: Registry vs Hierarchical

**Question:** How do actors discover service actors across supervision trees?

**Current:** Centralized ActorManager (DashMap)
**Better:** Hierarchical lookup + global registry

**Recommendation:**
- Global services (Researcher, Docs) → register in ApplicationSupervisor
- Domain services → hierarchical lookup (Session → Chat/Terminal)
- Remove centralized ActorManager

### 11.5 Reconciliation Strategy: Sequential vs Hotfix

**Question:** When unit tests pass but E2E fails, how do we reconcile?

**Options:**
1. **Sequential (stop Phase 2, fix Phase 1)** - Lose momentum
2. **Deprecation guards (continue Phase 2)** - Risky, complex
3. **Branch isolation (separate branches)** - Git overhead
4. **Hotfix + Continue (automated FixerAgent)** - ⭐ RECOMMENDED

**Recommendation: **Strategy 4 - Hotfix + Continue**
- Spawn FixerAgent to handle E2E failure (automated)
- Worker/Dev continues Phase 2 development (momentum preserved)
- FixerActor consults ResearcherAgent for similar issues
- When fix verified, merge hotfix if needed
- **Key benefit:** 24/7 development continues uninterrupted

### 11.6 Test Type Separation: Dev vs Verification

**Question:** What's the difference between tests run by Worker vs Verifier?

**Clear Distinction:**

| Aspect | Worker/Dev Tests | Verifier Tests |
|---------|-------------------|-----------------|
| **Purpose** | Fast feedback during coding | Integration gate (phase complete) |
| **Type** | Unit + Integration | **E2E** (end-to-end) |
| **Duration** | Seconds to minutes | Minutes to hours |
| **Environment** | Dev environment (with artifacts) | **Isolated sandbox** (clean system) |
| **Trigger** | Every code change | Only after phase commit |
| **Frequency** | High (hundreds per day) | Low (5-10 per day) |
| **Runner** | Worker/Developer Agent | VerifierAgent |
| **Blocking?** | Yes (blocks next edit) | No (runs in parallel) |

**Why Verifier Needs Sandbox:**
- E2E tests require clean system integration
- No dev artifacts (uncommitted code, test DBs, running processes)
- Multiple parallel verifications need isolated environments
- Prevents "works on my machine" bugs

**Design Impact:**
- **Developer experience unchanged** - fast unit tests still in dev env
- **Verification as gate** - slow E2E tests run after phase complete
- **Pipelining works** - Phase N+1 development happens while Phase N E2E runs

---

## 12. Research Sources

- **ractor Documentation:** https://docs.rs/ractor
- **Erlang/OTP Design Principles:** https://erlang.org/doc/design_principles/
- **Akka Typed Documentation:** https://doc.akka.io/docs/akka/current/typed/
- **ChoirOS Codebase:** `sandbox/src/supervisor/*.rs`, `sandbox/src/actors/event_bus.rs`

---

## 13. Verification & Pipelining Automation

### 13.1 Goal: 24/7 Inference Through Pipelined Development

**Problem:** Sequential development where each phase blocks on testing
- Complete Phase 3 → Wait for tests → Only then start Phase 4
- Developer idle while tests run
- Single verification bottleneck limits throughput

**Solution:** Pipelined development with VerifierAgent running in isolated sandbox
- Developer completes Phase 3 → Immediately starts Phase 4
- VerifierAgent runs E2E tests on Phase 3 in parallel
- E2E tests PASS → Continue Phase 4 development
- E2E tests FAIL → Automated reconciliation (see Section 13.6)

**Benefits:**
- **3-4x speedup** with 3-4 verification sandboxes in parallel
- **Developer never blocks** on slow E2E tests
- **Failed phase = preserved work** (stashed, not lost)
- **>24/7 inference** possible (continuous development + parallel verification)

### 13.2 Test Type Distinction: Unit/Integration vs E2E

**Critical Separation of Concerns:**

| Test Type | Purpose | Duration | Environment | Who Runs? |
|------------|---------|----------|-------------|-------------|
| **Unit Tests** | Fast feedback during coding | **Seconds** | Dev environment | Worker/Developer Agent |
| **Integration Tests** | Component integration | **Minutes** | Dev environment | Worker/Developer Agent |
| **E2E Tests** | Full system integration | **Minutes-Hours** | **Isolated sandbox** | VerifierAgent |

**Developer Loop (Fast):**
```
Write code → Run unit test → Fix → Run integration test → Fix → ...
(seconds)       ✓                ✗              ✓               ✗

Phase 1 complete (unit + integration pass) → COMMIT

→ START Phase 2 IMMEDIATELY (continue fast dev loop)
```

**Verification Loop (Slow):**
```
Clone Phase 1 commit → Run E2E tests → Report results
(minutes-hours)                                    ↓
                                            PASS? → Continue Phase 2
                                            FAIL? → Automated reconciliation
```

**Key Insight:**
- **Developer's fast feedback loop is UNTOUCHED**
- Write code → Unit test (seconds) → Repeat
- No waiting for slow E2E tests during development
- **E2E tests run as verification GATE** (only after phase complete)

**Example Workflow:**

```
Time →
─────────────────────────────────────────────────────────
Phase 1 Dev:
  0:00 → 0:30 (write code + unit/integration tests)
  0:30: COMMIT (tag: phase-1)
  0:30 → START Phase 2 dev loop

Phase 1 Verification (parallel):
  0:30 → 0:45 (E2E tests in sandbox)

Phase 2 Dev:
  0:30 → 1:00 (writing Phase 2 + unit tests)
  0:45: Phase 1 E2E result arrives
  1:00 → Continue Phase 2 development
```

**Why VerifierAgent Needs Sandbox:**
- E2E tests require **clean system integration** (no dev artifacts)
- Multiple parallel verifications need **isolated environments**
- Developer's env has: uncommitted code, test databases, running processes
- Sandbox has: Only the committed code, fresh dependencies

### 13.2 VerifierAgent Design

**Key Insight:** VerifierAgent needs its own sandbox
- Separate git working tree
- No interference with ongoing development
- Can run on committed state while developer modifies code

**Supervisor Tree with VerifierAgent:**

```
┌─────────────────────────────────────────────────────────────┐
│  ApplicationSupervisor (Root)                             │
└─────────────────────────────────────────────────────────────┘
                    │
         ┌──────────┼──────────┐
         │          │          │
         ▼          ▼          ▼
┌───────────┐ ┌──────────┐ ┌───────────┐
│SuperDev   │ │SuperSess │ │SuperServ  │
│(Orchest)  │ │           │ │           │
└───────────┘ └──────────┘ └───────────┘
     │
     ├─→ VerifierAgent (per verification sandbox)
     │   - VerifierAgent-1 (tests Phase 1)
     │   - VerifierAgent-2 (tests Phase 2)
     │   - VerifierAgent-3 (tests Phase 3)
```

**Message Protocol:**

```rust
pub struct VerifierAgent;

pub enum VerifierMsg {
    // Agentic handler: Test a specific phase
    VerifyPhase {
        phase: usize,
        commit_ref: String,  // Git ref or tag (e.g., "phase-3")
        reply: Option<ActorRef<VerifierMsg>>,
    },

    // Deterministic handler: Report results
    VerificationComplete {
        phase: usize,
        success: bool,
        output: TestOutput,
        duration: Duration,
    },

    // Deterministic handler: Get status
    GetStatus {
        phase: usize,
        reply: RpcReplyPort<Option<VerificationStatus>>,
    },
}

pub struct VerificationStatus {
    pub phase: usize,
    pub commit_ref: String,
    pub status: VerificationState,  // Running, Passed, Failed
    pub started_at: DateTime<Utc>,
    pub output: Option<TestOutput>,
}

pub enum VerificationState {
    Pending,
    Running,
    Passed,
    Failed,
}
```

**VerifierActor Implementation:**

```rust
impl VerifierActor {
    type Msg = VerifierMsg;
    type State = VerifierState;
    type Arguments = VerifierArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(
            verifier_id = %myself.get_id(),
            sandbox_dir = %args.sandbox_dir,
            "VerifierActor starting"
        );

        Ok(VerifierState {
            sandbox_dir: args.sandbox_dir,
            event_store: args.event_store,
            active_verification: None,
            verification_history: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            VerifierMsg::VerifyPhase { phase, commit_ref, reply } => {
                // Check if already verifying this phase
                if let Some(active) = &state.active_verification {
                    if active.phase == phase {
                        warn!("Phase {} already being verified", phase);
                        if let Some(r) = reply {
                            let _ = r.send(VerifierMsg::VerificationStatus {
                                status: VerificationState::Running,
                            });
                        }
                        return Ok(());
                    }
                }

                // Start verification (non-blocking to caller!)
                let myself_clone = myself.clone();
                let sandbox_dir = state.sandbox_dir.clone();
                let event_store = state.event_store.clone();
                let phase_clone = phase;
                let commit_ref_clone = commit_ref.clone();

                tokio::spawn(async move {
                    Self::run_verification(
                        &myself_clone,
                        &sandbox_dir,
                        &event_store,
                        phase_clone,
                        commit_ref_clone,
                    ).await;
                });

                // Immediately reply (non-blocking!)
                state.active_verification = Some(ActiveVerification {
                    phase,
                    commit_ref,
                    started_at: Utc::now(),
                });

                if let Some(r) = reply {
                    let _ = r.send(VerifierMsg::VerificationStatus {
                        status: VerificationState::Running,
                    });
                }
            }

            VerifierMsg::VerificationComplete { phase, success, output, duration } => {
                // Clear active verification
                state.active_verification = None;

                // Store result in history
                state.verification_history.insert(phase, VerificationResult {
                    phase,
                    commit_ref: output.commit_ref.clone(),
                    success,
                    output: output.clone(),
                    duration,
                    completed_at: Utc::now(),
                });

                // Persist to EventStore
                let _ = event_store.persist(Event::new(
                    EventType::VerificationComplete,
                    &format!("verify.phase-{}", phase),
                    json!({
                        "phase": phase,
                        "success": success,
                        "test_count": output.test_count,
                        "failures": output.failures,
                        "duration_secs": duration.as_secs(),
                    }),
                    "verifier-agent",
                )?).await;

                // Emit event for all subscribers
                event_bus.publish(Event::new(
                    EventType::VerificationComplete,
                    &format!("verify.phase-{}", phase),
                    json!(output),
                    "verifier-agent",
                )?, true).await;
            }

            VerifierMsg::GetStatus { phase, reply } => {
                let status = state.verification_history.get(&phase).map(|r| VerificationStatus {
                    phase: r.phase,
                    commit_ref: r.commit_ref.clone(),
                    status: if r.success { VerificationState::Passed } else { VerificationState::Failed },
                    started_at: r.completed_at - r.duration,
                    output: Some(r.output.clone()),
                });
                let _ = reply.send(status);
            }
        }
        Ok(())
    }
}

impl VerifierAgent {
    async fn run_verification(
        myself: &ActorRef<VerifierMsg>,
        sandbox_dir: &str,
        event_store: &ActorRef<EventStoreMsg>,
        phase: usize,
        commit_ref: String,
    ) {
        let start = Instant::now();

        info!("Verifying Phase {} from commit {}", phase, commit_ref);

        // 1. Clone commit to isolated sandbox directory
        let phase_dir = format!("{}/phase-{}", sandbox_dir, phase);
        let _ = std::fs::create_dir_all(&phase_dir);

        // Git clone of specific commit
        let _ = Command::new("git")
            .args(["clone", &commit_ref, &phase_dir])
            .output()
            .await;

        // 2. Run full test suite
        let output = Self::run_tests(&phase_dir).await;

        let duration = start.elapsed();

        // 3. Emit completion event
        let _ = myself.cast(VerifierMsg::VerificationComplete {
            phase,
            success: output.success,
            output,
            duration,
        }).await;

        info!("Phase {} verification complete: {} (took {:.2}s)",
                phase,
                if output.success { "PASSED" } else { "FAILED" },
                duration.as_secs_f64());
    }

    async fn run_tests(phase_dir: &str) -> TestOutput {
        // Run: cargo test --workspace
        let output = Command::new("cargo")
            .args(["test", "--workspace"])
            .current_dir(phase_dir)
            .output()
            .await;

        // Parse test output
        TestOutput {
            commit_ref: String::new(),  // Extract from git
            success: output.status.success(),
            test_count: Self::count_tests(&output.stdout),
            failures: Self::count_failures(&output.stdout),
            output: output.stdout.clone(),
        }
    }
}
```

### 13.3 SupervisorAgent Orchestrates Pipelining

**SupervisorAgent Message Protocol:**

```rust
pub enum SupervisorAgentMsg {
    // Developer (or automated agent) completes a phase
    PhaseComplete {
        phase: usize,
        changes: Vec<FileChange>,
        commit_message: String,
    },

    // VerifierActor reports test results
    VerificationResult {
        phase: usize,
        success: bool,
        output: TestOutput,
    },

    // Proceed to next phase (verification passed)
    ProceedToPhase {
        phase: usize,
    },

    // Stash work (verification failed)
    StashWork {
        phase: usize,
        reason: String,
    },

    // Retrieve stashed work
    GetStashedWork {
        phase: usize,
        reply: RpcReplyPort<Option<Vec<FileChange>>>,
    },
}
```

**Pipelining Implementation:**

```rust
impl SupervisorAgent {
    async fn handle_phase_complete(
        &mut self,
        phase: usize,
        changes: Vec<FileChange>,
        commit_message: String,
        state: &mut SupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        info!("Phase {} complete. Committing and starting verification...", phase);

        // 1. Commit phase changes with tag
        let tag = format!("phase-{}", phase);
        let commit_ref = self.git_commit_tag(&tag, &commit_message).await?;

        // 2. Tell VerifierActor to test (NON-BLOCKING!)
        verifier_agent.cast(VerifierMsg::VerifyPhase {
            phase,
            commit_ref: commit_ref.clone(),
            reply: None,  // Don't block - fire and forget
        }).await?;

        // 3. Immediately mark as verifying in state
        state.phase_status.insert(phase, PhaseStatus::Verifying {
            commit_ref: commit_ref.clone(),
            started_at: Utc::now(),
        });

        // 4. START NEXT PHASE IMMEDIATELY (pipelining!)
        info!("Starting Phase {} in parallel with verification...", phase + 1);
        self.start_phase(phase + 1, state).await?;

        // 5. Stash work for next phase (in case verification fails)
        state.stashed_work.insert(phase + 1, StashedWork {
            changes: vec![],  // Will fill in as we develop Phase +1
            committed_at: Utc::now(),
            reason: None,
        });

        // 6. Emit event for observability
        event_bus.publish(Event::new(
            EventType::PhaseComplete,
            &format!("phase.{}", phase),
            json!({
                "phase": phase,
                "next_phase": phase + 1,
                "commit_ref": commit_ref,
                "pipelined": true,
            }),
            "supervisor-agent",
        )?, true).await;

        Ok(())
    }

    async fn handle_verification_result(
        &mut self,
        phase: usize,
        success: bool,
        output: TestOutput,
        state: &mut SupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        if success {
            // Verification PASSED
            info!("✓ Phase {} verification PASSED! Continuing development...", phase);

            // Mark phase as complete
            state.phase_status.insert(phase, PhaseStatus::Complete);

            // Emit success event
            event_bus.publish(Event::new(
                EventType::VerificationPassed,
                &format!("verify.phase-{}.passed", phase),
                json!({
                    "phase": phase,
                    "test_count": output.test_count,
                }),
                "supervisor-agent",
            )?, true).await;

            // Work on Phase +1 continues in parallel (already started!)
            // Nothing to do - development continues

        } else {
            // Verification FAILED
            error!("✗ Phase {} verification FAILED! Stashing work...", phase);

            // Mark phase as failed
            state.phase_status.insert(phase, PhaseStatus::Failed {
                reason: format!("{} tests failed", output.failures),
                output: output.clone(),
            });

            // Alert developer (or automated fix agent)
            event_bus.publish(Event::new(
                EventType::VerificationFailed,
                &format!("verify.phase-{}.failed", phase),
                json!({
                    "phase": phase,
                    "failures": output.failures,
                    "output": &output.output[0..500],  // First 500 chars
                }),
                "supervisor-agent",
            )?, true).await;

            // Stash Phase +1 work (stop developing it)
            state.stashed_work.entry(phase + 1).and_modify(|work| {
                work.reason = Some(format!(
                    "Phase {} failed: {}", phase, work.reason.as_deref().unwrap_or("unknown")
                ));
            });

            // Alert: developer needs to fix Phase N before continuing N+1
            // TODO: Spawn FixerAgent (another agentic type!)
        }

        Ok(())
    }
}
```

### 13.4 24/7 Inference Timeline

```
Time →
─────────────────────────────────────────────────────────────────
Supervisor: │ Phase 1 │ Phase 2 │ Phase 3 │ Phase 4 │ Phase 5 │
            │    ✓     │    ✓     │    ✓     │   ✓?     │   ?      │
            │           │           │           │           │          │
Verifier 1: │    [test]  │           │           │          │
            │  Phase 1  │           │           │          │
            │  PASS ✓    │           │           │          │

Verifier 2: │           │   [test]   │           │          │
            │           │  Phase 2    │           │          │
            │           │  PASS ✓     │           │          │

Verifier 3: │           │           │   [test]   │          │
            │           │           │  Phase 3    │          │
            │           │           │  FAIL ✗     │          │
            │           │           │  STASH!      │          │
            │           │           │  ALERT        │          │
                         │           │           │          │
Result:   │   Continue  │   Continue  │   STOP,    │          │
            │           │           │  FIX       │          │
            │           │           │  Phase 3   │          │

Developer: │ Working... │ Working... │ Working... │ STOPPED   │          │
            │ on Phase 2 │ on Phase 3 │ on Phase 4 │ (fix 3)  │          │
```

**Key Benefits:**
1. **Developer never waits** for tests to complete
2. **Tests run in isolation** (sandbox per verifier)
3. **Parallel verification** (Phase 1, 2, 3 tested simultaneously)
4. **Failed phase = preserved work** (stashed, not lost)
5. **3-4x speedup** with 3-4 verification sandboxes in parallel

### 13.5 Multi-Verifier Strategy

**Why Multiple Verifiers?**
- Phase 1 tests → VerifierAgent-1
- Phase 2 tests → VerifierActor-2
- Phase 3 tests → VerifierAgent-3
- All run in parallel!

**SupervisorServices spawns verifier pool:**

```rust
impl SupervisorServices {
    async fn pre_start(&mut self, args: Args, state: &mut State) {
        // Spawn multiple verifiers
        for i in 0..MAX_VERIFIERS {
            let (verifier, _) = Actor::spawn_linked(
                Some(format!("verifier-{}", i)),
                VerifierAgent,
                VerifierArgs {
                    sandbox_dir: format!("/tmp/verify-sandbox-{}", i),
                    event_store: args.event_store.clone(),
                },
                myself.get_cell(),
            ).await?;

            state.verifiers.push(verifier);
        }

        info!("Spawned {} VerifierAgents for parallel testing", MAX_VERIFIERS);
    }
}
```

**Load Balancing:**

```rust
impl SupervisorAgent {
    fn select_verifier(&self) -> &ActorRef<VerifierMsg> {
        // Round-robin
        let idx = self.current_verifier % self.verifiers.len();
        self.current_verifier += 1;
        &self.verifiers[idx]
    }
}
```

### 13.6 Stash and Resume Pattern

**When Verification Fails:**

```rust
impl SupervisorAgent {
    async fn stash_phase_work(&mut self, phase: usize, state: &mut State) {
        if let Some(stashed) = state.stashed_work.get_mut(&phase) {
            // Collect all uncommitted changes for this phase
            let uncommitted = self.git_diff_since_last_commit().await?;

            stashed.changes = uncommitted;
            stashed.reason = Some("Previous phase verification failed".to_string());

            info!("Stashed {} changes for Phase {}", 
                    stashed.changes.len(), phase);

            // Emit stashed event
            event_bus.publish(Event::new(
                EventType::WorkStashed,
                &format!("phase.{}.stashed", phase),
                json!({
                    "phase": phase,
                    "change_count": stashed.changes.len(),
                    "reason": stashed.reason,
                }),
                "supervisor-agent",
            )?, true).await;
        }
    }

    async fn resume_phase_work(&mut self, phase: usize, state: &mut State) -> Result<(), SupervisorProcessingErr> {
        if let Some(stashed) = state.stashed_work.remove(&phase) {
            info!("Resuming stashed work for Phase {}", phase);

            // Apply stashed changes
            for change in stashed.changes {
                self.apply_change(change).await?;
            }

            // Clear stash
            state.stashed_work.remove(&phase);

            // Continue development
            self.start_phase(phase, state).await?;
        }

        Ok(())
    }
}
```

### 13.7 Integration with Existing Supervision Plan

**Add to supervision-implementation-plan.md as Phase 6:**

```markdown
### Phase 6: Verification & Pipelining - Weeks 8-9
**Goal:** Implement VerifierActor with isolated sandbox for parallel testing
**Risk:** Medium

#### Tasks

1. **Implement VerifierActor** (`src/actors/verifier_agent.rs`)
   - Clone commits to isolated sandbox directories
   - Run full test suite (`cargo test --workspace`)
   - Emit VerificationComplete events with detailed output
   - Handle multiple parallel verifications

2. **Implement SupervisorAgent** (`src/actors/supervisor_agent.rs`)
   - Orchestrate phase completion → verification → next phase flow
   - Stash work on verification failure
   - Load balance across multiple VerifierAgents

3. **Integrate with Supervision Tree**
   - Add SupervisorOrchestration to ApplicationSupervisor
   - Spawn SupervisorAgent as child
   - Spawn multiple VerifierActors via SupervisorServices

4. **Add Git Integration**
   - Implement commit/tag operations
   - Implement diff operations (for stashing)
   - Clone specific commits to verification sandboxes

5. **Update Development Workflow**
   - Replace manual "commit → wait → next phase" with pipelined flow
   - Add dashboard showing phase status + verification results
   - Add stash/resume commands

#### Success Criteria
- [ ] VerifierActor runs tests in isolated sandbox
- [ ] Multiple verifiers run in parallel
- [ ] Development never blocks on verification
- [ ] Failed phases trigger work stashing
- [ ] Stashed work can be resumed
- [ ] 3-4x speedup achieved over sequential testing
```

### 13.6 Reconciliation Strategies: Unit Passes, E2E Fails

**Critical Scenario:**
```
Phase 1:
  - Developer: Writes code + unit/integration tests (all pass) ✓
  - Commit: tag "phase-1"
  - Start Phase 2 dev loop (pipelining)

Meanwhile:
  - Verifier: Runs E2E tests on Phase 1
  - E2E: FAIL ✗ (integration issue, config, API contract)

Phase 2:
  - Developer: 5-15 min into Phase 2 (depends on Phase 1 APIs!)
  - Problem: Phase 2 code assumes Phase 1 is correct
  - Conflict: Can't continue Phase 2 if Phase 1 is broken
```

**Four Reconciliation Strategies:**

#### Strategy 1: Stop Phase 2, Fix Phase 1 (Sequential)

**Flow:**
```
E2E FAIL → Stop Phase 2 development → Fix Phase 1 → Commit → Resume Phase 2
```

**Pros:**
- Clean, sequential fix
- No assumptions about Phase 1 state

**Cons:**
- **Lose Phase 2 momentum** (5-15 min wasted)
- Developer context switch cost
- Slower overall throughput

#### Strategy 2: Continue Phase 2 with Deprecation Flag

**Flow:**
```
E2E FAIL → Mark Phase 1 as "broken" → Phase 2 adds deprecation guards
Phase 1 fixed → Phase 2 removes deprecation guards
```

**Pros:**
- Keep Phase 2 momentum
- Minimal interruption

**Cons:**
- **Phase 2 may be based on broken assumption**
- Deprecation guards add complexity
- Phase 2 might need rework after Phase 1 fix

#### Strategy 3: Branch Strategy (Git Isolation)

**Flow:**
```
E2E FAIL → Phase 2 continues on separate branch
Phase 1 fix on main branch
Phase 1 stable → Merge Phase 2 branch
```

**Pros:**
- Clean isolation (no state pollution)
- Clear git history

**Cons:**
- **Git management overhead** (merge conflicts)
- Phase 2 might be based on broken API
- Merge may be painful if API changes significantly

#### Strategy 4: Hotfix + Continue (RECOMMENDED) ⭐

**Flow:**
```
E2E FAIL → Spawn FixerAgent to handle Phase 1
            → DeveloperAgent continues Phase 2 (with notification)
            → FixerAgent: investigate, fix, commit, verify
            → Phase 1 fixed → Signal "safe to proceed"
            → DeveloperAgent merges hotfix if needed
```

**Pros:**
- **Maximum momentum preservation** (Phase 2 continues)
- **Automated fix** (FixerAgent handles it)
- **Minimal interruption** (developer notified, continues)
- **Can work on multiple failures simultaneously**

**Cons:**
- Phase 2 might waste time if Phase 1 API changes
- Requires FixerAgent implementation (agentic capabilities)

**Full Timeline with Hotfix Strategy:**

```
Time →
────────────────────────────────────────────────────────────────
Phase 1:       │  ✓ (unit + integration pass)
                  │  COMMIT (tag: phase-1)
                  │
Phase 2:       │  START dev loop (0:00)
                  │  0:05 (writing code)
                  │
Verifier:        │  [E2E on Phase 1] (0:05 → 0:15)
                  │  FAIL ✗

Supervisor:      │  Analyze failure (0:15)
                  │  → Alert DevAgent (continue Phase 2)
                  │  → Spawn FixerAgent (fix Phase 1)

Phase 2:       │  CONTINUE dev loop (0:15 → 0:30)
                  │  Developer aware of Phase 1 issue

Fixer:          │  Investigate Phase 1 (0:15 → 0:25)
                  │  → Implement fix
                  │  → Verify E2E
                  │  ✓ PASS
                  │  COMMIT (tag: phase-1-hotfix)

Supervisor:      │  Phase 1 fixed! (0:30)
                  │  → Signal DevAgent (safe to merge hotfix)
                  │  → Update DocsUpdaterActor

Phase 2:       │  Continue dev loop (0:30 → ...)
                  │  Phase 1 now stable, can merge if needed
```

**Comparison Summary:**

| Strategy | Momentum | Automation | Complexity | Recommended |
|----------|-----------|-------------|-------------|-------------|
| Sequential (stop Phase 2) | ❌ Lost | ❌ Manual | ❌ No |
| Deprecation guards | ✅ Kept | ❌ Manual | ⚠️  Maybe |
| Branch isolation | ✅ Kept | ❌ Manual | ⚠️  Maybe |
| **Hotfix + Continue** | ✅ **Kept** | ✅ **Automated** | ✅ **YES** |

### 13.7 Communication Patterns: Multi-Agent Coordination

**Full Multi-Agent Message Flow:**

```
┌─────────────────────────────────────────────────────────────────┐
│  SupervisorAgent (Orchestration)                             │
│  - Coordinates phases, verification, fixes                    │
│  - Load balances across workers, verifiers, fixers          │
└─────────────────────────────────────────────────────────────────┘
         │                            │                            │
         │                            │                            │
         ▼                            ▼                            ▼
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ Worker/Dev Agent │    │ VerifierAgent    │    │ FixerAgent       │
│ - Writes code    │    │ - Runs E2E tests │    │ - Fixes failures  │
│ - Unit tests     │    │ - Reports results │    │ - Investigates    │
└──────────────────┘    └──────────────────┘    └──────────────────┘
         │                            │                            │
         │ emits                    emits                   emits
         ▼                            ▼                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│  EventBusActor (Pub/Sub)                                   │
│  Topics:                                                        │
│  - "phase.*.complete" (phase completion)                       │
│  - "verify.phase.*" (verification status)                       │
│  - "fix.*.started" (fix started)                              │
│  - "fix.*.complete" (fix completed)                             │
│  - "research.*" (research results)                               │
│  - "docs.*.updated" (doc updates)                              │
└─────────────────────────────────────────────────────────────────────┘
         ▲                            ▲                            ▲
         │                            │                            │
         │ subscribes               subscribes              subscribes
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ ResearcherActor  │    │ DocsUpdaterActor  │    │ WatcherAgent     │
│ - Web search     │    │ - Index system   │    │ - Detect issues  │
│ - LLM inference │    │ - Answer queries │    │ - Suggest fixes  │
└──────────────────┘    └──────────────────┘    └──────────────────┘
```

**Message Examples:**

```rust
// Worker/Dev → Supervisor
pub enum WorkerMsg {
    PhaseComplete {
        phase: usize,
        changes: Vec<FileChange>,
        commit_message: String,
    },

    QueryStatus {
        phase: usize,
        reply: RpcReplyPort<Option<PhaseStatus>>,
    },
}

// Supervisor → Worker
pub enum SupervisorMsg {
    ContinuePhase {
        phase: usize,
        dependencies: Vec<Dependency>,  // If Phase 1 broken, list
    },

    PhaseBroken {
        phase: usize,
        reason: String,
        fixer_assigned: Option<ActorId>,
    },
}

// Verifier → Supervisor
pub enum VerifierMsg {
    VerificationComplete {
        phase: usize,
        commit_ref: String,
        success: bool,
        output: E2ETestOutput,
    },
}

// Supervisor → Fixer
pub enum FixerMsg {
    FixE2EFailure {
        phase: usize,
        commit_ref: String,
        failure_output: E2ETestOutput,
        reply: Option<ActorRef<FixerMsg>>,
    },
}

// Fixer → Researcher
pub enum FixerResearchMsg {
    InvestigateFailure {
        failure_context: FailureContext,
        reply: ActorRef<ResearcherMsg>,
    },

    // Researcher → Fixer
    pub enum ResearcherToFixerMsg {
        SimilarFailuresFound {
            failure_patterns: Vec<FailurePattern>,
            suggested_fixes: Vec<FixSuggestion>,
        },
    }
}

// All actors → DocsUpdater
pub enum DocsUpdateMsg {
    UpdatePhaseStatus {
        phase: usize,
        status: PhaseStatusEnum,  // Complete, Broken, Fixed
        commit_ref: String,
    },

    IndexFailure {
        phase: usize,
        failure_type: String,  // "E2E integration failure"
        description: String,
    },
}

// DocsUpdater → All actors (queries)
pub enum DocsQueryMsg {
    GetPhaseStatus {
        phase: usize,
        reply: RpcReplyPort<Option<PhaseStatus>>,
    },

    ListBrokenPhases {
        reply: RpcReplyPort<Vec<usize>>,
    },
}
```

**FixerAgent Coordination Flow:**

```rust
impl FixerAgent {
    async fn handle_fix_request(
        &mut self,
        phase: usize,
        failure_output: E2ETestOutput,
        state: &mut FixerState,
    ) -> Result<(), ActorProcessingErr> {
        // 1. Consult ResearcherActor (check for similar failures)
        let research_request = ResearcherMsg::Research {
            correlation_id: ULID::new().to_string(),
            query: format!("E2E failure: {}", failure_output.error_message),
            context: ResearchContext {
                phase,
                failure_type: "E2E",
                error_logs: failure_output.logs.clone(),
            },
            reply: myself.clone().into(),
        };

        researcher_actor.send(research_request).await?;

        // Wait for research result (or timeout)
        let research_result = tokio::time::timeout(
            Duration::from_secs(30),
            self.wait_for_research_result(),
        ).await;

        // 2. Use research to guide fix (if available)
        if let Ok(Ok(Some(research))) = research_result {
            info!("Research found similar failures: {}", research.suggestions.len());
            self.apply_suggested_fixes(&research.suggestions).await?;
        }

        // 3. Reproduce issue in verification sandbox
        let repro_result = self.reproduce_failure(&phase, &failure_output).await?;

        if !repro_result.reproducible {
            warn!("Could not reproduce E2E failure. Flaky test?");
            return Ok(());
        }

        // 4. Implement fix
        let fix_attempt = self.implement_fix(&phase, &failure_output, &research).await?;

        // 5. Verify fix (run E2E in sandbox)
        let verify_result = self.verify_fix(&phase, &fix_attempt.commit_ref).await?;

        if verify_result.success {
            // Fix worked! Commit hotfix
            self.commit_hotfix(&phase, &fix_attempt).await?;

            // Emit fix complete event
            event_bus.publish(Event::new(
                EventType::FixComplete,
                &format!("fix.phase-{}", phase),
                json!({
                    "phase": phase,
                    "fix_type": fix_attempt.fix_type,
                    "commit_ref": fix_attempt.commit_ref,
                }),
                "fixer-agent",
            )?, true).await;

            // Notify supervisor: Phase is fixed
            supervisor_agent.cast(SupervisorMsg::PhaseFixed {
                phase,
                fix_commit_ref: fix_attempt.commit_ref,
            }).await?;

        } else {
            // Fix didn't work. Log and escalate
            error!("Fix attempt failed for Phase {}. Escalating.", phase);

            event_bus.publish(Event::new(
                EventType::FixFailed,
                &format!("fix.phase-{}.failed", phase),
                json!({
                    "phase": phase,
                    "attempts": state.failed_attempts + 1,
                    "last_error": verify_result.error,
                }),
                "fixer-agent",
            )?, true).await;

            // Maybe escalate to human if multiple failures
            if state.failed_attempts > 3 {
                supervisor_agent.cast(SupervisorMsg::EscalateToHuman {
                    phase,
                    reason: "Automated fix failed 3 times",
                }).await?;
            }
        }

        Ok(())
    }
}
```

**WatcherAgent Coordination:**

```rust
impl WatcherActor {
    async fn observe_worker_floundering(
        &mut self,
        event: Event,
        state: &mut WatcherState,
    ) -> Result<(), ActorProcessingErr> {
        // Detect: Worker stuck on same task for > 30 min
        if self.is_worker_stuck(&event, state) {
            // Check if FixerAgent already working on this
            if state.active_fixes.contains(&event.actor_id) {
                info!("Fix already in progress for worker {}", event.actor_id);
                return Ok(());
            }

            // Suggest fix to supervisor
            supervisor_agent.cast(SupervisorMsg::SuggestFix {
                worker_id: event.actor_id.clone(),
                issue_type: "worker_stuck",
                suggestion: format!(
                    "Worker stuck on task: {}",
                    event.payload["task"]
                ),
            }).await?;

            // Track this fix
            state.active_fixes.insert(event.actor_id.clone(), Utc::now());
        }

        Ok(())
    }

    async fn observe_e2e_regression(
        &mut self,
        event: Event,
        state: &mut State,
    ) -> Result<(), ActorProcessingErr> {
        // Detect: E2E test suite takes 2x longer than baseline
        if let Some(phase) = self.extract_phase(&event) {
            if self.is_e2e_regression(&event, state) {
                // Suggest investigation
                supervisor_agent.cast(SupervisorMsg::SuggestInvestigation {
                    phase,
                    metric: "e2e_duration",
                    baseline: state.baseline_e2e_duration,
                    current: event.payload["duration"],
                    severity: "regression",
                }).await?;
            }
        }

        Ok(())
    }
}
```

### 13.8 Benefits Summary

| Metric | Sequential Development | Pipelined with Verifier |
|---------|---------------------|-------------------------|
| **Developer wait time** | ~5-15 min per phase (tests) | 0 min (parallel) |
| **Total cycle time (5 phases)** | ~2.5 hours | ~45 min |
| **Speedup** | 1x (baseline) | **3-4x** |
| **Failed phase recovery** | Manual (re-do work) | **Automated** (FixerAgent) |
| **Parallel verification** | No | Yes (3-4 sandboxes) |
| **24/7 inference** | No (blocked) | **Yes** |

---

## 14. Open Questions & Design Decisions

### 14.1 Per-Supervisor Event Buses

**Question:** Should each supervisor have its own event bus, or one global bus?

**Options:**
1. **Single global event bus** (simple, but bottleneck)
2. **Per-supervisor event bus** (fault isolation, but need bridging)
3. **Hierarchical event buses** (local + global forwarding)

**Recommendation:** **Option 3 - Hierarchical**
- Local events stay within supervisor (fast)
- Cross-domain events forwarded to global bus
- Combines benefits of both

### 14.2 State Reconstruction vs In-Memory

**Question:** Should service actor state (docs index, research cache) be event-sourced or in-memory?

**Options:**
1. **Full event sourcing** (replay events to rebuild state)
2. **In-memory + event persistence** (current state persisted separately)
3. **Hybrid** (critical data event-sourced, cache in-memory)

**Recommendation:** **Option 3 - Hybrid**
- EventStore has all events (for reconstruction)
- DocsUpdater has in-memory index (for fast queries)
- On restart: rebuild index from events (bootstrap phase)

### 14.3 Monitor vs Link for Cross-Tree Observation

**Question:** Should supervisors use ractor monitors to observe actors in other trees?

**Current:** Links only (direct supervisor-child)
**Consider:** Monitors (one-way observation, no lifecycle coupling)

**Recommendation:** **Evaluate monitors for debugging/diagnostic actors only**
- Keep links for production supervision
- Use monitors for system health dashboards
- Enable `monitors` feature flag in Cargo.toml

### 14.4 Service Discovery: Registry vs Hierarchical

**Question:** How do actors discover service actors across supervision trees?

**Current:** Centralized ActorManager (DashMap)
**Better:** Hierarchical lookup + global registry

**Recommendation:**
- Global services (Researcher, Docs) → register in ApplicationSupervisor
- Domain services → hierarchical lookup (Session → Chat/Terminal)
- Remove centralized ActorManager

---

## 15. Research Sources

- **ractor Documentation:** https://docs.rs/ractor
- **Erlang/OTP Design Principles:** https://erlang.org/doc/design_principles/
- **Akka Typed Documentation:** https://doc.akka.io/docs/akka/current/typed/
- **ChoirOS Codebase:** `sandbox/src/supervisor/*.rs`, `sandbox/src/actors/event_bus.rs`

---

*Last updated: 2026-02-06*  
*Status: Draft - Ready for review and implementation*
