# RLM Integration with ChoirOS: Unified Technical Specification

**Date:** 2026-02-08
**Status:** Technical Specification
**Version:** 1.0

---

## Executive Summary

This document unifies the Recursive Language Model (RLM) runtime contract, Jido/OTP architectural patterns, and ChoirOS implementation specifics into a coherent technical specification. It provides evidence-backed findings from the current codebase, a correspondence table mapping concepts across frameworks, a unified architecture design, and a phased implementation plan.

---

## 1. RLM Summary

### 1.1 Runtime Contract

The RLM (Recursive Language Model) runtime defines a structured execution environment for LLM-based agents with the following core principles:

| Aspect | Definition |
|--------|------------|
| **Call-Stack / Frame Model** | Recursive subcalls have isolated context frames, each representing a unit of work with specific goals, inputs/outputs, budget constraints, and parent/child relationships |
| **ContextPack** | Bounded context slices assembled on-demand for LLM calls, containing: always-on brief context, stack-top frame breadcrumbs, recent conversation slice (k items, token-bounded), and retrieved evidence via handles |
| **External Memory** | Full state lives in EventStore (append-only SQLite log), not in actor memory. Actors are restartable and stateless |
| **Frame Lifecycle** | Frames transition through: `Active` -> `Waiting` -> `Completed`/`Failed`/`Cancelled` |

### 1.2 Unbounded Context Mechanism

The fundamental innovation of RLM is replacing unbounded context accumulation with **bounded, assembled-on-demand context packs**:

```rust
// PROBLEMATIC: Current ChatAgentState (unbounded)
pub struct ChatAgentState {
    args: ChatAgentArguments,
    messages: Vec<BamlMessage>,  // Grows forever!
    current_model: String,
    model_registry: ModelRegistry,
}
```

**Location:** `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` (lines 35-40)

The `messages` vector:
1. **Unbounded** - grows with every user/assistant exchange
2. **Redundant** - all data is already in EventStore
3. **Not restart-safe** - actor state is lost on crash
4. **Token-inefficient** - entire history passed to LLM calls

### 1.3 Gaps (Failure Handling)

| Gap | Current State | Target State |
|-----|---------------|--------------|
| **Actor Restart** | ChatAgent reloads all messages from EventStore on start | Resume from Frame stack, detect pending work |
| **Token Budgeting** | No explicit limits; unbounded messages passed to LLM | Strict ContextPack budgets per frame |
| **Subcall Isolation** | Direct tool delegation without frame tracking | Frame-per-subcall with isolated budgets |
| **Failure Recovery** | Lost in-flight work on crash | Resume from top-of-stack with pending work detection |
| **Context Truncation** | Implicit (eventually OOM) | Explicit token-bounded assembly |

---

## 2. Jido Framework Analysis

### 2.1 OTP Patterns Relevant to ChoirOS

While Jido framework documentation is not explicitly present in the codebase, the design patterns from the RLM specification align with core OTP (Open Telecom Platform) principles:

| OTP Pattern | RLM Equivalent | ChoirOS Implementation |
|-------------|----------------|------------------------|
| **Supervision Trees** | Conductor hierarchy | `ApplicationSupervisor` -> `SessionSupervisor` -> `ChatSupervisor` |
| **GenServer State** | Frame-based state | `ChatAgentState` with `messages: Vec<BamlMessage>` (to be replaced) |
| **Event Sourcing** | External Memory | `EventStoreActor` with SQLite persistence |
| **Process Isolation** | Frame isolation | Actor-per-conversation model |
| **Restart Strategies** | Resume-from-restart | `one_for_one` supervision in ractor |

### 2.2 Supervision Pattern

**Current Implementation:** `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs`

```rust
//! Application Supervisor - Root of the supervision tree
//!
//! ## Architecture
//!
//! ApplicationSupervisor (one_for_one strategy)
//! └── SessionSupervisor (one_for_one strategy)
//!     ├── DesktopSupervisor
//!     ├── ChatSupervisor
//!     └── TerminalSupervisor
```

### 2.3 Pure Functions / Signal Contract

**Current Implementation:** `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-worker-live-update-event-model.md`

Workers emit typed turn reports rather than ad-hoc signals:

```rust
pub struct WorkerTurnReport {
    pub turn_id: String,
    pub worker_id: String,
    pub task_id: String,
    pub status: WorkerTurnStatus,  // Running, Completed, Failed, Blocked
    pub findings: Vec<WorkerFinding>,
    pub learnings: Vec<WorkerLearning>,
    pub escalations: Vec<WorkerEscalation>,
    pub artifacts: Vec<WorkerArtifact>,
}
```

**Location:** `/Users/wiz/choiros-rs/shared-types/src/lib.rs` (lines 419-432)

### 2.4 Signals (Control vs Observability)

Two distinct planes:

**Control Plane (requires action):**
- `blocker`: Cannot continue without missing dependency/input
- `help`: Worker can continue but would benefit from guidance
- `approval`: Risky action requires explicit authorization
- `conflict`: Contraduous evidence/options need arbitration

**Observability Plane (informational):**
- `finding`: Grounded fact with evidence
- `learning`: Synthesis that changes strategy/understanding
- `progress`: Step/lifecycle status
- `artifact`: References to generated outputs

---

## 3. ChoirOS Current State

### 3.1 Evidence-Backed Findings

#### 3.1.1 Context Accumulation Locations

| File | Line | Issue |
|------|------|-------|
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 37 | `messages: Vec<BamlMessage>` - unbounded growth |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 979-982 | Messages pushed on every user input |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 1157-1160 | Messages pushed on every assistant response |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 1650-1683 | History loaded from EventStore on actor start |

#### 3.1.2 EventStore Integration

**Current Schema:** `/Users/wiz/choiros-rs/sandbox/src/actors/event_store.rs` (lines 121-191)

```sql
CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,
    thread_id TEXT
);
```

**Key Capabilities:**
- Append-only event log
- Session/thread scoping via `session_id`/`thread_id` columns
- Indexed by `actor_id`, `event_type`, and `session_id/thread_id`
- JSON payload for flexible event data

#### 3.1.3 Worker Event Payload Types

Historical note:
- Legacy structs below are retained for reference.
- Current runtime semantics use the worker live-update event model (`progress/result/failed/request`).

**Location:** `/Users/wiz/choiros-rs/shared-types/src/lib.rs` (lines 351-463)

```rust
pub struct WorkerFinding {
    pub finding_id: String,
    pub claim: String,
    pub confidence: f64,
    pub evidence_refs: Vec<String>,
    pub novel: Option<bool>,
}

pub struct WorkerLearning {
    pub learning_id: String,
    pub insight: String,
    pub confidence: f64,
    pub supports: Vec<String>,
    pub changes_plan: Option<bool>,
}

pub struct WorkerEscalation {
    pub escalation_id: String,
    pub kind: WorkerEscalationKind,  // Blocker, Help, Approval, Conflict
    pub reason: String,
    pub urgency: WorkerEscalationUrgency,  // Low, Medium, High
    pub options: Vec<String>,
    pub recommended_option: Option<String>,
    pub requires_human: Option<bool>,
}
```

### 3.2 Existing Gaps

| Gap | Evidence | Impact |
|-----|----------|--------|
| No Frame abstraction | No `Frame` or `FrameId` types in codebase | Cannot track call stack |
| No ContextPack | No bounded context assembly | Unbounded token usage |
| No StateIndex | No actor for context management | No resume capability |
| No token budgeting | No `max_context_tokens` anywhere | OOM risk |
| No frame depth limits | No `max_subframe_depth` | Stack overflow risk |

---

## 4. Correspondence Table

| RLM Concept | Jido/OTP Pattern | ChoirOS Current | ChoirOS Target |
|-------------|------------------|-----------------|----------------|
| **Frame** | Process/GenServer state | `ChatAgentState.messages: Vec<BamlMessage>` | `Frame` struct with `frame_id`, `parent_frame_id`, `goal`, `budgets` |
| **Frame Stack** | Supervision tree | Actor hierarchy only | Frame stack per conversation scope |
| **ContextPack** | Bounded message passing | Full `messages` vector passed to LLM | Assembled on-demand with token budgets |
| **External Memory** | Event sourcing | `EventStoreActor` with SQLite | Same, plus frame projections |
| **StateIndex** | Process registry | `ractor::registry` for actors | `StateIndexActor` for frame stacks |
| **Resume** | Supervision restart | Reload all messages | `FindTopOfStack` + `RebuildFrameStack` |
| **Budgets** | Process limits | None | `FrameBudgets { max_context_tokens, max_tool_calls, max_subframe_depth, timeout_ms }` |
| **Handles** | References/pids | Direct event references | `ContextHandle` with type-safe retrieval |
| **Conductor** | Supervisor | `ApplicationSupervisor` | Enhanced with frame-aware coordination |

---

## 5. Unified Architecture

### 5.1 Combined StateIndex + Frame Stack Design

#### 5.1.1 Core Types

```rust
// ============================================================================
// Frame Types
// ============================================================================

/// Unique identifier for a frame in the call stack
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

/// A frame represents a unit of work in the call stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    /// Unique frame identifier
    pub frame_id: FrameId,

    /// Parent frame (None for root frames)
    pub parent_frame_id: Option<FrameId>,

    /// Which actor owns this frame
    pub actor_id: String,

    /// Session/thread scope for isolation
    pub session_id: Option<String>,
    pub thread_id: Option<String>,

    /// Frame goal/intent
    pub goal: String,

    /// Input parameters to this frame
    pub inputs: serde_json::Value,

    /// Context handles - references to evidence/events
    pub context_handles: Vec<ContextHandle>,

    /// Budget constraints
    pub budgets: FrameBudgets,

    /// Current status
    pub status: FrameStatus,

    /// References to results produced by this frame
    pub result_refs: Vec<ResultRef>,

    /// When the frame was created
    pub created_at: DateTime<Utc>,

    /// When the frame was completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
}

/// Handle to a piece of context that can be retrieved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHandle {
    /// Handle identifier (e.g., event_seq, artifact_id)
    pub handle_id: String,

    /// Type of handle (determines how to retrieve)
    pub handle_type: HandleType,

    /// Brief description
    pub description: String,

    /// Estimated token count if retrieved
    pub estimated_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HandleType {
    EventRef,
    ToolOutput,
    WorkerFinding,
    WorkerLearning,
    Artifact,
    Document,
    Url,
}

/// Budget constraints for a frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameBudgets {
    /// Maximum tokens for LLM context from this frame
    pub max_context_tokens: usize,

    /// Maximum tool calls allowed in this frame
    pub max_tool_calls: usize,

    /// Maximum depth of sub-frames
    pub max_subframe_depth: usize,

    /// Maximum time allowed for this frame (milliseconds)
    pub timeout_ms: u64,
}

impl Default for FrameBudgets {
    fn default() -> Self {
        Self {
            max_context_tokens: 4000,
            max_tool_calls: 10,
            max_subframe_depth: 3,
            timeout_ms: 120_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrameStatus {
    Active,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}
```

#### 5.1.2 ContextPack Assembly

```rust
/// A bounded context pack assembled for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    /// Metadata about this pack
    pub metadata: ContextPackMetadata,

    /// Always-on brief context (~500 tokens)
    pub brief_context: BriefContext,

    /// Stack-top frame breadcrumbs
    pub frame_breadcrumbs: Vec<FrameBreadcrumb>,

    /// Recent conversation slice (token-bounded)
    pub conversation_slice: ConversationSlice,

    /// Retrieved evidence via handles
    pub evidence: Vec<RetrievedEvidence>,

    /// Token usage summary
    pub token_summary: TokenSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefContext {
    /// System prompt/instructions
    pub system_prompt: String,

    /// Key invariants
    pub invariants: Vec<String>,

    /// Current working context
    pub working_context: serde_json::Value,

    /// Active tool descriptions
    pub available_tools: Vec<ToolDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSlice {
    /// Messages in chronological order
    pub messages: Vec<BamlMessage>,

    /// How many messages were included
    pub message_count: usize,

    /// How many messages were truncated
    pub truncated_count: usize,

    /// Estimated token count
    pub estimated_tokens: usize,
}
```

#### 5.1.3 StateIndex Actor

```rust
/// StateIndexActor - RLM-style context management for ChoirOS
pub struct StateIndexActor;

/// Arguments for spawning StateIndexActor
#[derive(Debug, Clone)]
pub struct StateIndexArguments {
    /// Reference to the EventStore for persistence
    pub event_store: ActorRef<EventStoreMsg>,

    /// Database path for frame projections
    pub database_path: String,
}

/// State for StateIndexActor
pub struct StateIndexState {
    args: StateIndexArguments,
    conn: libsql::Connection,

    /// In-memory cache of active frame stacks (bounded, LRU eviction)
    active_stacks: LruCache<String, FrameStack>,
}

/// A stack of frames for a given scope
#[derive(Debug, Clone)]
pub struct FrameStack {
    pub scope: Scope,
    pub frames: Vec<Frame>,
    pub last_accessed: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub actor_id: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

/// Messages handled by StateIndexActor
#[derive(Debug)]
pub enum StateIndexMsg {
    /// Frame lifecycle operations
    PushFrame {
        frame: Frame,
        reply: RpcReplyPort<Result<FrameId, StateIndexError>>,
    },

    UpdateFrame {
        frame_id: FrameId,
        updates: FrameUpdates,
        reply: RpcReplyPort<Result<(), StateIndexError>>,
    },

    PopFrame {
        frame_id: FrameId,
        final_status: FrameStatus,
        result_summary: String,
        reply: RpcReplyPort<Result<Frame, StateIndexError>>,
    },

    AddHandle {
        frame_id: FrameId,
        handle: ContextHandle,
        reply: RpcReplyPort<Result<(), StateIndexError>>,
    },

    /// Context pack assembly
    GetContextPack {
        request: GetContextPackRequest,
        reply: RpcReplyPort<GetContextPackResult>,
    },

    /// Resume operations
    ResumeActor {
        actor_id: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        reply: RpcReplyPort<Result<ActorResumeState, StateIndexError>>,
    },

    /// Rebuild projections from EventStore
    RebuildProjections {
        since_seq: i64,
        reply: RpcReplyPort<Result<usize, StateIndexError>>,
    },
}

/// Resume state for a restarting actor
#[derive(Debug, Clone)]
pub struct ActorResumeState {
    /// The current active frame (top of stack)
    pub current_frame: Option<Frame>,

    /// Full frame stack (root to current)
    pub frame_stack: Vec<Frame>,

    /// Unfinished work detected
    pub pending_work: Vec<PendingWork>,
}

#[derive(Debug, Clone)]
pub struct PendingWork {
    pub frame_id: FrameId,
    pub work_type: PendingWorkType,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingWorkType {
    WaitingForSubcall,
    WaitingForUser,
    ToolInProgress,
    UnhandledError,
}
```

### 5.2 Database Schema Additions

```sql
-- ============================================================================
-- Frame Storage
-- ============================================================================

CREATE TABLE IF NOT EXISTS frames (
    frame_id TEXT PRIMARY KEY,
    parent_frame_id TEXT,
    actor_id TEXT NOT NULL,
    session_id TEXT,
    thread_id TEXT,
    goal TEXT NOT NULL,
    inputs TEXT NOT NULL,  -- JSON
    budgets TEXT NOT NULL, -- JSON
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    FOREIGN KEY (parent_frame_id) REFERENCES frames(frame_id)
);

CREATE TABLE IF NOT EXISTS frame_handles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id TEXT NOT NULL,
    handle_id TEXT NOT NULL,
    handle_type TEXT NOT NULL,
    description TEXT,
    estimated_tokens INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (frame_id) REFERENCES frames(frame_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frame_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id TEXT NOT NULL,
    result_type TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (frame_id) REFERENCES frames(frame_id) ON DELETE CASCADE
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_frames_actor ON frames(actor_id);
CREATE INDEX IF NOT EXISTS idx_frames_session_thread ON frames(session_id, thread_id);
CREATE INDEX IF NOT EXISTS idx_frames_status ON frames(status);
CREATE INDEX IF NOT EXISTS idx_frames_parent ON frames(parent_frame_id);
CREATE INDEX IF NOT EXISTS idx_handles_frame ON frame_handles(frame_id);
CREATE INDEX IF NOT EXISTS idx_results_frame ON frame_results(frame_id);

-- View for active frame stacks per actor
CREATE VIEW IF NOT EXISTS active_frame_stacks AS
WITH RECURSIVE frame_tree AS (
    -- Root frames (no parent)
    SELECT
        frame_id,
        parent_frame_id,
        actor_id,
        session_id,
        thread_id,
        goal,
        status,
        created_at,
        0 as depth,
        frame_id as root_frame_id
    FROM frames
    WHERE parent_frame_id IS NULL AND status = 'active'

    UNION ALL

    -- Child frames
    SELECT
        f.frame_id,
        f.parent_frame_id,
        f.actor_id,
        f.session_id,
        f.thread_id,
        f.goal,
        f.status,
        f.created_at,
        ft.depth + 1,
        ft.root_frame_id
    FROM frames f
    JOIN frame_tree ft ON f.parent_frame_id = ft.frame_id
    WHERE f.status = 'active'
)
SELECT * FROM frame_tree;
```

### 5.3 Integration with Existing Components

#### 5.3.1 ChatAgent Integration

```rust
/// Updated ChatAgentState (without unbounded messages)
pub struct ChatAgentState {
    args: ChatAgentArguments,
    current_frame: Option<Frame>,  // Current active frame
    current_model: String,
    model_registry: ModelRegistry,
    // REMOVED: messages: Vec<BamlMessage>
}

/// Updated ChatAgentArguments
#[derive(Debug, Clone)]
pub struct ChatAgentArguments {
    pub actor_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub state_index: Option<ActorRef<StateIndexMsg>>,  // NEW
    pub preload_session_id: Option<String>,
    pub preload_thread_id: Option<String>,
    pub application_supervisor: Option<ActorRef<ApplicationSupervisorMsg>>,
}
```

#### 5.3.2 Resume Logic

```rust
#[async_trait]
impl Actor for ChatAgent {
    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Get StateIndex reference
        let state_index = args.state_index.clone()
            .ok_or_else(|| ActorProcessingErr::from("StateIndex not provided"))?;

        // Attempt to resume from previous state
        let resume_state = ractor::call!(state_index, |reply| {
            StateIndexMsg::ResumeActor {
                actor_id: args.actor_id.clone(),
                session_id: args.preload_session_id.clone(),
                thread_id: args.preload_thread_id.clone(),
                reply,
            }
        }).map_err(|e| ActorProcessingErr::from(e.to_string()))?;

        match resume_state {
            Ok(resume) if !resume.frame_stack.is_empty() => {
                tracing::info!(
                    actor_id = %args.actor_id,
                    frame_count = resume.frame_stack.len(),
                    pending_work = resume.pending_work.len(),
                    "Resuming from previous state"
                );
            }
            _ => {
                tracing::info!(actor_id = %args.actor_id, "Starting fresh");
            }
        }

        Ok(ChatAgentState {
            args,
            current_frame: resume_state.ok().and_then(|r| r.current_frame),
            model_registry: ModelRegistry::new(),
            current_model: std::env::var("CHOIR_CHAT_MODEL")
                .ok()
                .or_else(|| load_model_policy().chat_default_model)
                .unwrap_or_else(|| "ClaudeBedrockSonnet45".to_string()),
        })
    }
}
```

---

## 6. Implementation Plan

### Phase 1: Schema and Types (Week 1)

| Task | File | Description |
|------|------|-------------|
| 1.1 | `shared-types/src/frame.rs` | Create `Frame`, `FrameId`, `ContextHandle`, `FrameBudgets`, `FrameStatus` types |
| 1.2 | `shared-types/src/context_pack.rs` | Create `ContextPack`, `BriefContext`, `ConversationSlice` types |
| 1.3 | `shared-types/src/lib.rs` | Re-export new types |
| 1.4 | `sandbox/migrations/003_frames.sql` | Create frame tables and indexes |
| 1.5 | Tests | Unit tests for type serialization/deserialization |

### Phase 2: StateIndex Actor (Week 2)

| Task | File | Description |
|------|------|-------------|
| 2.1 | `sandbox/src/actors/state_index.rs` | Create `StateIndexActor` with frame operations |
| 2.2 | `sandbox/src/actors/state_index.rs` | Implement `PushFrame`, `PopFrame`, `UpdateFrame` |
| 2.3 | `sandbox/src/actors/state_index.rs` | Implement `GetContextPack` with token budgeting |
| 2.4 | `sandbox/src/actors/state_index.rs` | Implement `ResumeActor` with pending work detection |
| 2.5 | `sandbox/src/actors/mod.rs` | Export `StateIndexActor` |
| 2.6 | Tests | Integration tests for frame lifecycle |

### Phase 3: ChatAgent Migration (Week 3)

| Task | File | Description |
|------|------|-------------|
| 3.1 | `sandbox/src/actors/chat_agent.rs` | Add `state_index` to `ChatAgentArguments` |
| 3.2 | `sandbox/src/actors/chat_agent.rs` | Remove `messages` from `ChatAgentState` |
| 3.3 | `sandbox/src/actors/chat_agent.rs` | Update `pre_start` to use `ResumeActor` |
| 3.4 | `sandbox/src/actors/chat_agent.rs` | Update `handle_process_message` to use `GetContextPack` |
| 3.5 | `sandbox/src/actors/chat_agent.rs` | Push/pop frames around tool delegation |
| 3.6 | Tests | Verify restart/resume behavior |

### Phase 4: Event Integration (Week 4)

| Task | File | Description |
|------|------|-------------|
| 4.1 | `shared-types/src/lib.rs` | Add frame event constants |
| 4.2 | `sandbox/src/actors/state_index.rs` | Emit frame events to EventStore |
| 4.3 | `sandbox/src/actors/state_index.rs` | Implement `RebuildProjections` for recovery |
| 4.4 | `sandbox/src/actors/researcher.rs` | Add handles to parent frame on completion |
| 4.5 | Tests | Event replay and projection tests |

### Phase 5: Supervision Integration (Week 5)

| Task | File | Description |
|------|------|-------------|
| 5.1 | `sandbox/src/supervisor/session.rs` | Spawn `StateIndexActor` in supervision tree |
| 5.2 | `sandbox/src/supervisor/chat.rs` | Pass `StateIndex` reference to `ChatAgent` |
| 5.3 | `sandbox/src/main.rs` | Wire StateIndex into application startup |
| 5.4 | Tests | End-to-end supervision tests |

### Phase 6: Token Budgeting & Optimization (Week 6)

| Task | File | Description |
|------|------|-------------|
| 6.1 | `sandbox/src/actors/state_index.rs` | Implement token estimation |
| 6.2 | `sandbox/src/actors/state_index.rs` | Implement conversation truncation |
| 6.3 | `sandbox/src/actors/state_index.rs` | Add budget enforcement |
| 6.4 | Config | Add `CHOIR_DEFAULT_CONTEXT_TOKENS` env var |
| 6.5 | Tests | Token budget enforcement tests |

### Test Strategy

| Phase | Test Type | Coverage |
|-------|-----------|----------|
| 1 | Unit | Type serialization, schema validation |
| 2 | Integration | Frame CRUD, context pack assembly |
| 3 | Integration | ChatAgent resume, message processing |
| 4 | Integration | Event emission, projection rebuild |
| 5 | E2E | Full supervision tree restart |
| 6 | Load | Token budget enforcement, memory usage |

---

## 7. Open Questions / Risks

### 7.1 Distribution

| Question | Risk Level | Mitigation |
|----------|------------|------------|
| How to distribute StateIndex across nodes? | High | Start with single-node; design for future clustering via libsql replication |
| Frame consistency across distributed actors? | Medium | Use EventStore as single source of truth; frames are projections |
| Cross-node frame references? | Low | Use ULID-based FrameIds that are globally unique |

### 7.2 Idempotency

| Question | Risk Level | Mitigation |
|----------|------------|------------|
| Duplicate frame push on retry? | Medium | FrameId is client-generated (ULID); use INSERT OR IGNORE |
| Duplicate context pack requests? | Low | ContextPacks are read-only projections; safe to regenerate |
| Idempotent resume? | Medium | Resume is based on persistent frame state; deterministic |

### 7.3 Caching

| Question | Risk Level | Mitigation |
|----------|------------|------------|
| LRU cache size for active stacks? | Medium | Default 1000 stacks; configurable via env var |
| Cache invalidation on frame update? | Low | Update in-place or evict from LRU |
| ContextPack memoization? | Low | Cache packs by (frame_id, budget) with TTL |

### 7.4 Termination

| Question | Risk Level | Mitigation |
|----------|------------|------------|
| Orphaned frames on actor crash? | Medium | Watcher scans for stale frames; mark as `Failed` after timeout |
| Frame leak on infinite recursion? | Low | Enforce `max_subframe_depth` on push |
| Long-running frame detection? | Medium | Watcher flags frames exceeding `timeout_ms` |
| Graceful shutdown with pending frames? | Medium | On SIGTERM, wait for active frames to complete or mark as `Cancelled` |

### 7.5 Compatibility

| Question | Risk Level | Mitigation |
|----------|------------|------------|
| Migration from current message-based state? | High | Dual-write during Phase 3; fallback to messages if StateIndex unavailable |
| EventStore schema compatibility? | Medium | New tables are additive; existing events unchanged |
| ractor version compatibility? | Low | Using stable ractor 0.14+ APIs only |

---

## Appendix A: File References

### Current Implementation Files

| File | Purpose |
|------|---------|
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | ChatAgent with unbounded messages |
| `/Users/wiz/choiros-rs/sandbox/src/actors/event_store.rs` | EventStoreActor with SQLite |
| `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` | ResearcherActor with worker reports |
| `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs` | Supervision tree root |
| `/Users/wiz/choiros-rs/sandbox/src/supervisor/session.rs` | SessionSupervisor |
| `/Users/wiz/choiros-rs/shared-types/src/lib.rs` | Shared types including WorkerTurnReport |

### Design Documents

| File | Purpose |
|------|---------|
| `/Users/wiz/choiros-rs/docs/architecture/state_index_rlm_design.md` | Original StateIndex design |
| `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-worker-live-update-event-model.md` | Worker live-update event model |
| `/Users/wiz/choiros-rs/docs/design/2026-02-08-capability-actor-architecture.md` | Capability actor design |

---

## Appendix B: Restart/Resume Semantics

### Normal Flow

```
1. ChatAgent receives ProcessMessage
2. StateIndex.PushFrame (new frame for this request)
3. StateIndex.GetContextPack (bounded context for LLM)
4. LLM processing...
5. If tool calls: PushFrame for each subcall
6. On completion: PopFrame with status Completed
```

### Restart Flow

```
1. ChatAgent crashes (OOM, panic, etc.)
2. Supervisor restarts ChatAgent
3. pre_start calls StateIndex.ResumeActor
4. StateIndex queries frames table for active frames
5. StateIndex rebuilds frame stack from leaf to root
6. StateIndex detects pending work (Waiting, incomplete tools)
7. ChatAgent receives ActorResumeState
8. ChatAgent continues from current_frame or starts fresh
```

### Token Budget Enforcement

```
1. GetContextPack request includes budget_tokens (e.g., 8000)
2. Reserve ~500 tokens for brief_context
3. Reserve ~50 tokens per breadcrumb
4. Allocate 60% of remaining to conversation_slice
5. Allocate remainder to evidence retrieval
6. If evidence exceeds budget, prioritize by handle relevance
7. Return pack with token_summary showing usage
```

---

*End of Document*
