# ChoirOS Architecture Reconciliation Review

**Date:** 2026-02-08
**Scope:** Full architecture/code/docs reconciliation
**Status:** Critical drift identified - immediate action required

---

## 1. Narrative Summary (1-minute read)

ChoirOS has **significant architectural drift** between documented intent and code reality. The four critical findings:

1. **Capability Leak (CRITICAL):** `ChatAgent` can execute tools directly via local `ToolRegistry`, bypassing `TerminalActor` entirely. This violates the core security model.

2. **Naming Drift:** 11 naming inconsistencies between "Actor" and "Agent" terminology, plus the completely unimplemented "Automatic*" prefix pattern proposed in docs.

3. **Dual Contract Violations:** `TerminalActor` accepts both typed commands (`SendInput`) AND natural language objectives (`RunAgenticTask`), breaking the intended interface boundary.

4. **Observability Gaps:** Supervision events not persisted, EventRelay cursor issues, and hardcoded event type strings causing watcher/log panels to appear empty.

**Execution Direction (2026-02-09):** Per AGENTS.md, the primary orchestration path is **Prompt Bar -> Conductor**. Chat is a compatibility surface that escalates multi-step planning to Conductor. The findings in this document describe the **legacy** chat-first architecture; fixes must align with conductor-first, not reinforce chat-centric workflows.

**NO ADHOC WORKFLOW Policy:** All workflow state transitions must use typed protocol fields (BAML/shared-types), not natural-language string matching.

**Recommendation:** Pick messaging model **Option B** (separate contracts for uActor→Actor vs AppActor→ToolActor) with immediate fixes for capability boundaries.

---

## 2. What Changed (Current Reality Snapshot)

### 2.1 Runtime Architecture

```
Current Implementation (as of 2026-02-08):
=============================================

ApplicationSupervisor (one_for_one)
└── SessionSupervisor (one_for_one)
    ├── DesktopSupervisor ──► DesktopActor(s)
    ├── ChatSupervisor ─────► ChatActor(s) ──► ChatAgent(s) [COMPATIBILITY LAYER]
    │                                                    ↓ (escalates multi-step planning)
    │                                             ConductorActor [PRIMARY ORCHESTRATION]
    └── TerminalSupervisor ─► TerminalActor(s)

Event Flow:
  ChatAgent/Workers ──► EventStore ──► EventRelay ──► EventBus ──► WebSocket/UI
  SupervisionEvents ──► EventBus ONLY (not persisted)

**Note (2026-02-09):** ChatAgent is a compatibility surface per AGENTS.md Execution Direction. Multi-step planning/orchestration belongs in Conductor.
```

### 2.2 Key Components Inventory

| Component | Type | Lines | Status |
|-----------|------|-------|--------|
| `ChatAgent` | AppActor | ~1300 | **VIOLATION: Direct tool execution** |
| `TerminalActor` | ToolActor | ~900 | **Dual contract: typed + natural language** |
| `ApplicationSupervisor` | uActor | ~600 | Supervision events not persisted |
| `EventStoreActor` | Infrastructure | ~400 | Source of truth per ADR-0001 |
| `EventRelayActor` | Infrastructure | ~200 | 120ms poll, cursor-based |
| `WatcherActor` | Infrastructure | ~400 | Scans EventStore every 500ms |
| `ToolRegistry` | Capability | ~100 | **Instantiated locally in ChatAgent** |

### 2.3 Documentation vs Reality

| Document Claims | Code Reality | Drift Level |
|-----------------|--------------|-------------|
| "bash execution delegated through TerminalActor" | ChatAgent has direct ToolRegistry access | **CRITICAL** |
| "Automatic* prefix for LLM-driven processes" | No "Automatic*" types exist | HIGH |
| "EventStore is source of truth" | Supervision events go only to EventBus | MEDIUM |
| "uActor → Actor secure envelopes" | No capability tokens implemented | HIGH |
| "Typed tool contracts" | Natural language objectives in TerminalMsg | MEDIUM |

---

## 3. Conflict Matrix (Doc intent vs Code reality vs Risk)

### 3.1 Naming Conflicts

| Location | Document Intent | Code Reality | Risk | Fix Priority |
|----------|-----------------|--------------|------|--------------|
| `terminal.rs:371-380` | `TerminalResult` | `AgentResult` | Medium - misleads about actor type | P2 |
| `terminal.rs:382-391` | `TerminalProgress` | `AgentProgress` | Medium - misleads about actor type | P2 |
| `chat_agent.rs:76-84` | `ActorResponse` | `AgentResponse` | High - struct in "Agent" is for Chat"Agent" | P1 |
| `AGENTS.md:155` | "Actors:" directory | "agents:" mentioned | Low - documentation inconsistency | P3 |
| `logging-watcher-architecture-design.md` | `Automatic*` prefix | **Not implemented anywhere** | High - architectural pattern missing | P1 |
| `event_schema_design_report.md` | `actor.spawned` events | Mixed `worker.task.*` naming | Medium - event taxonomy inconsistent | P2 |

### 3.2 Messaging Model Conflicts

| Contract Type | Document Intent | Code Reality | Risk |
|---------------|-----------------|--------------|------|
| **uActor → Actor** | Secure prompt envelopes with capability tokens | Plain RPC with `Option<ActorRef>` | **CRITICAL** - no capability enforcement |
| **AppActor → ToolActor** | Typed tool contracts only | Natural language objectives accepted | HIGH - bypasses type safety |
| **Event taxonomy** | `uactor.*` and `appactor.*` namespaces | `worker.task.*`, `chat.*`, mixed | MEDIUM - inconsistent naming |
| **Supervision events** | Persisted to EventStore | Published to EventBus only | MEDIUM - observability gap |

### 3.3 Capability Boundary Conflicts

| Actor | Can Execute Bash (Intended) | Can Execute Bash (Actual) | Risk |
|-------|----------------------------|---------------------------|------|
| `ChatActor` | NO | NO | - |
| `ChatAgent` | NO (must delegate) | **YES (via ToolRegistry)** | **CRITICAL** |
| `TerminalActor` | YES | YES | - |
| `ApplicationSupervisor` | NO | NO | - |

### 3.4 Observability Conflicts

| Pipeline Stage | Document Intent | Code Reality | Risk |
|----------------|-----------------|--------------|------|
| **Event emission** | All events to EventStore | Supervision events to EventBus only | MEDIUM |
| **Event relay** | At-least-once delivery | Cursor-based, no dedup on consumer | MEDIUM |
| **WebSocket streaming** | `since_seq` for reconnect | No server-side session storage | LOW |
| **Watcher alerts** | Real-time detection | 500ms poll, may miss rapid events | MEDIUM |

---

## 4. Recommended Target Model

### 4.1 Decision: Messaging Model Option B

**Selected:** Option B - Separate contracts for uActor→Actor vs AppActor→ToolActor

**Justification:**

| Criterion | Option A (Universal) | Option B (Separate) | Winner |
|-----------|---------------------|---------------------|--------|
| **Implementation complexity** | Lower (single contract) | Higher (two contracts) | A |
| **Safety/isolation** | Weaker (blurred boundaries) | **Stronger (clear boundaries)** | **B** |
| **Observability** | Harder to distinguish flows | **Explicit interface_kind field** | **B** |
| **Model/tool reliability** | More ambiguity | **Clearer semantics** | **B** |
| **Migration cost** | Higher (rewrite all) | **Lower (incremental)** | **B** |

**Rationale:** While Option A is simpler to implement initially, Option B provides the safety boundaries ChoirOS needs for multi-agent operation. The explicit `interface_kind` discriminator (already partially implemented) enables proper observability and debugging. Migration cost is lower because we can fix boundaries incrementally rather than rewriting everything.

### 4.2 Target Architecture

```
Target Architecture (Post-Reconciliation):
===========================================

uActor Layer (Meta-Coordination):
  ApplicationSupervisor
  └── SessionSupervisor
      ├── DesktopSupervisor
      ├── ChatSupervisor (NO tool registry!)
      └── TerminalSupervisor

AppActor Layer (Typed Tool Contracts):
  ChatAgent ──delegates──► TerminalActor (bash)
         │
         └──delegates──► FileActor (read/write)
         │
         └──delegates──► SearchActor (search)

Messaging Contracts:
  uActor→Actor: Secure delegation envelopes with capability tokens
  AppActor→ToolActor: Typed tool calls with explicit schemas

Observability:
  ALL events → EventStore (source of truth)
  EventRelay → EventBus (delivery only)
  Watcher scans EventStore
```

### 4.3 Naming Standardization

| Pattern | Usage | Example |
|---------|-------|---------|
| `*Actor` | Infrastructure/system actors | `TerminalActor`, `EventStoreActor` |
| `*Agent` | LLM-driven app actors | `ChatAgent` |
| `*Supervisor` | Supervision tree nodes | `ApplicationSupervisor` |
| `Automatic*` | [DEPRECATED] Never implemented, do not use | N/A |

---

## 5. Minimal Migration Plan (phased, low-risk, test-first)

### Phase 1: Critical Security Fix (Week 1) - **BLOCKING**

**Goal:** Remove direct tool execution capability from ChatAgent

1. **Remove ToolRegistry from ChatAgentState** (`chat_agent.rs:36`)
   - Delete `tool_registry: Arc<ToolRegistry>` field
   - Update constructor (line ~1168)

2. **Remove ExecuteTool message variant** (`chat_agent.rs:68-72`)
   - Delete `ExecuteTool` from `ChatAgentMsg` enum
   - Remove `handle_execute_tool` handler

3. **Remove execute_tool_impl function** (`chat_agent.rs:1247-1258`)
   - This function is the critical bypass

4. **Update handle_process_message** (`chat_agent.rs:718-736`)
   - Remove `else` branch for direct tool execution
   - ALL tools must go through delegation

5. **Add tests** (`tests/capability_boundary_test.rs`)
   ```rust
   #[tokio::test]
   async fn test_chat_agent_cannot_execute_tools_directly() {
       // Spawn ChatAgent without ToolRegistry
       // Attempt to execute tool
       // Verify it fails or delegates
   }
   ```

**Acceptance Criteria:**
- [ ] `cargo test capability_boundary_test` passes
- [ ] ChatAgent cannot execute any tool directly
- [ ] All tool calls flow through TerminalActor or new ToolActors

---

### Phase 2: Fix Dual Contract Violations (Week 2)

**Goal:** Remove natural language objectives from TerminalActor

1. **Deprecate RunAgenticTask** (`terminal.rs:374-408`)
   - Mark as `#[deprecated]`
   - Log warning when used

2. **Add typed command envelope** (`terminal.rs`)
   ```rust
   pub struct TypedCommand {
       pub command: String,
       pub cwd: String,
       pub timeout_ms: u64,
       pub capability_token: String, // NEW
   }
   ```

3. **Update delegation path** (`supervisor/mod.rs:730-1151`)
   - Use TypedCommand instead of natural language

4. **Add capability token validation**
   - Tokens issued by ApplicationSupervisor
   - Validated by TerminalActor before execution

**Acceptance Criteria:**
- [ ] TerminalActor rejects natural language objectives
- [ ] All bash commands use TypedCommand with valid capability token
- [ ] Existing tests updated, new tests for token validation

---

### Phase 3: Observability Fixes (Week 3)

**Goal:** Fix supervision event persistence and watcher gaps

1. **Persist supervision events** (`supervisor/mod.rs:261-287`)
   - Change `persist: false` to `persist: true`
   - OR remove EventBus publish, use EventStore only

2. **Unify event type constants** (`websocket_chat.rs:616-646`)
   - Replace hardcoded strings with `shared_types` constants

3. **Add EventStore write confirmation** (`chat_agent.rs:600-626`)
   - Propagate EventStore errors to caller
   - Don't swallow failures

4. **Add WebSocket reconnection handling** (`websocket_logs.rs`)
   - Server-side session storage for `last_seq`

**Acceptance Criteria:**
- [ ] Supervision events appear in EventStore
- [ ] Watcher detects all worker events
- [ ] WebSocket reconnect resumes from correct position

---

### Phase 4: Naming Cleanup (Week 4)

**Goal:** Fix naming inconsistencies (lower risk, can be deferred)

1. **Rename TerminalActor types** (`terminal.rs:371-391`)
   - `AgentResult` → `TerminalResult`
   - `AgentProgress` → `TerminalProgress`

2. **Update documentation**
   - Remove "Automatic*" references from docs
   - Standardize on Actor/Agent/Supervisor terminology

3. **Add naming lint** (optional)
   - CI check for naming consistency

**Acceptance Criteria:**
- [ ] No "Agent*" types in TerminalActor
- [ ] Documentation matches code
- [ ] All tests pass

---

## 6. Acceptance Criteria (explicit pass/fail checks)

### Critical (Must Pass Before Merge)

| # | Criteria | Test Command |
|---|----------|--------------|
| C1 | ChatAgent cannot execute tools directly | `cargo test chat_agent_no_direct_tools` |
| C2 | All bash commands go through TerminalActor | `cargo test bash_delegation_only` |
| C3 | Capability tokens validated on tool execution | `cargo test capability_token_required` |
| C4 | No "Automatic*" prefix in codebase | `grep -r "Automatic" src/ || true` |

### High Priority

| # | Criteria | Test Command |
|---|----------|--------------|
| H1 | Supervision events persisted to EventStore | `cargo test supervision_events_persisted` |
| H2 | Watcher detects all worker lifecycle events | `cargo test watcher_detects_workers` |
| H3 | Event type constants unified | `cargo test event_type_consistency` |
| H4 | TerminalActor dual contract removed | `cargo test terminal_typed_only` |

### Medium Priority

| # | Criteria | Test Command |
|---|----------|--------------|
| M1 | Naming inconsistencies resolved | `cargo test naming_consistency` |
| M2 | WebSocket reconnects correctly | `cargo test websocket_reconnect` |
| M3 | All documentation updated | `mdbook build docs/` |

---

## 7. What To Do Next (ordered checklist)

### Immediate (Today)

- [ ] **1. Create tracking issues** for each phase of migration plan
- [ ] **2. Assign owners** for Phase 1 critical fixes
- [ ] **3. Block non-critical PRs** until Phase 1 complete

### Week 1: Critical Fixes

- [ ] **4. Remove ToolRegistry from ChatAgent** (`chat_agent.rs:36`)
- [ ] **5. Remove ExecuteTool message variant** (`chat_agent.rs:68-72`)
- [ ] **6. Remove execute_tool_impl function** (`chat_agent.rs:1247-1258`)
- [ ] **7. Update delegation paths** to use TerminalActor
- [ ] **8. Write capability boundary tests**
- [ ] **9. Run full test suite:** `just test`

### Week 2: Dual Contract Fixes

- [ ] **10. Deprecate RunAgenticTask** in TerminalActor
- [ ] **11. Implement TypedCommand** with capability tokens
- [ ] **12. Update supervisor delegation** to use TypedCommand
- [ ] **13. Add token validation** in TerminalActor
- [ ] **14. Write contract compliance tests**

### Week 3: Observability Fixes

- [ ] **15. Fix supervision event persistence**
- [ ] **16. Unify event type constants**
- [ ] **17. Add EventStore write confirmation**
- [ ] **18. Fix WebSocket reconnection**
- [ ] **19. Write observability integration tests**

### Week 4: Cleanup

- [ ] **20. Fix naming inconsistencies**
- [ ] **21. Update all documentation**
- [ ] **22. Run E2E tests:** `agent-browser` validation
- [ ] **23. Update AGENTS.md** with new patterns
- [ ] **24. Archive this review document**

---

## Appendix A: Detailed Code References

### Critical Capability Violation

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs`
**Lines:** 36, 1247-1258

```rust
// LINE 36: ChatAgent has local ToolRegistry
pub struct ChatAgentState {
    args: ChatAgentArguments,
    messages: Vec<BamlMessage>,
    tool_registry: Arc<ToolRegistry>,  // VIOLATION
    current_model: String,
    model_registry: ModelRegistry,
}

// LINES 1247-1258: Direct tool execution bypass
async fn execute_tool_impl(
    registry: Arc<ToolRegistry>,
    tool_name: String,
    tool_args: String,
) -> Result<ToolOutput, ToolError> {
    // This allows ANY tool execution without delegation
}
```

### Dual Contract Violation

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
**Lines:** 100-115

```rust
// Typed contract (correct)
SendInput { input: String, reply: RpcReplyPort<...> }

// Natural language objective (violation)
RunAgenticTask { objective: String, reply: RpcReplyPort<...> }
```

### Supervision Event Gap

**File:** `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs`
**Lines:** 261-287

```rust
// Supervision events published to EventBus but NOT EventStore
if let Err(e) = ractor::cast!(
    event_bus,
    EventBusMsg::Publish {
        event: supervision_event,
        persist: false,  // GAP: never persisted
    }
)
```

---

## Appendix B: Research Sources

This review synthesized findings from:

1. **Subagent: Naming Analysis** - 11 naming inconsistencies identified
2. **Subagent: Messaging Inventory** - 14 message enums, ~75 variants, dual-contract violations
3. **Subagent: Capability Audit** - Critical violation: ChatAgent direct tool execution
4. **Subagent: Observability Pipeline** - Event flow gaps, supervision event loss

All findings cross-referenced against:
- `/Users/wiz/choiros-rs/AGENTS.md`
- `/Users/wiz/choiros-rs/docs/architecture/adr-0001-eventstore-eventbus-reconciliation.md`
- `/Users/wiz/choiros-rs/docs/architecture/logging-watcher-architecture-design.md`
- `/Users/wiz/choiros-rs/docs/architecture/ractor-supervision-best-practices.md`
- Source files in `/Users/wiz/choiros-rs/sandbox/src/`

---

**End of Review**
