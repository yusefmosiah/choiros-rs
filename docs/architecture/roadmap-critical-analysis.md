# Critical Analysis: ChoirOS Roadmap vs. Codebase & Best Practices

**Date:** 2026-02-06 (Updated 2026-02-09)
**Status:** Critical Assessment with Gaps Identified

## Narrative Summary (1-minute read)

This document provides historical gap analysis for the ChoirOS roadmap. The current architecture direction (2026-02-09) establishes Prompt Bar + Conductor as the primary orchestration path, with Chat as a thin compatibility surface. Key gaps identified include incomplete EventBus integration, missing correlation ID propagation, and the need for typed worker contracts. This analysis remains valid for understanding technical debt but does not override the conductor-first execution policy defined in AGENTS.md.

## What Changed

- **2026-02-09**: Added explicit alignment with AGENTS.md Execution Direction.
- **2026-02-08**: Narrowed execution lane to Logging → Watcher → Researcher.
- **2026-02-07**: Documented Phase B implementation progress.

## What To Do Next

1. Complete observability foundations (Logging, Watcher) before expanding behavior.
2. Implement Researcher with full event lifecycle visibility.
3. Defer PromptBar/Conductor orchestration until after Researcher baseline.
4. Apply NO ADHOC WORKFLOW policy: encode control flow in typed protocols, not string matching.

---

## Update Addendum (2026-02-08)

Execution lane is now explicitly narrowed to:

1. Logging
2. Watcher
3. Researcher

Why this reset:
- Multiagent progress needs stronger observability before adding more behavior.
- Research is only valuable if we can inspect and trust the process in real time.
- Existing EventStore/libSQL foundation should be extended first, not bypassed.

Immediate implications:
- Logging architecture moves from design doc to implementation baseline now.
- Watcher is deterministic first (rule-based), agentic policy deferred.
- Researcher implementation must emit rich lifecycle/citation events from day one.
- Other roadmap branches are temporarily deprioritized unless they unblock this lane.

## Update Addendum (2026-02-07)

Progress since this assessment:
- Phase B moved from conceptual to implemented baseline:
  - delegated task contracts added and persisted
  - `ApplicationSupervisor` now supports async terminal delegation
  - worker lifecycle events include correlation IDs and scope metadata
  - chat `bash` tool path delegates through terminal control plane
- Terminal path now includes an internal agentic harness with transparent execution metadata:
  - `reasoning`
  - `executed_commands`
  - `steps` (command, exit_code, output_excerpt)
- Regression coverage added for failure-path semantics:
  - non-zero command exits emit `worker_failed`

Remaining critical gaps vs. target state:
- UI timeline rendering for step-level worker/actor transparency is incomplete.
- Terminal agent model/client parity with ChatAgent selection is incomplete.
- Full scope enforcement across all non-chat domains remains pending.

---

## Executive Summary

The roadmap correctly identifies critical dependencies (B→F→C→D→G→H) but **significantly underestimates implementation gaps**. Key findings:

- **Phase A (Supervision Cutover):** 80% complete - supervision tree exists but EventBus/orphaned, correlation IDs unused
- **Phase B (Multiagent Control Plane):** 10% complete - worker pattern, `run_async`, and budget enforcement are **completely missing**
- **Phase F (Identity/Scope):** 45% complete - user_id exists but session_id/thread_id missing
- **Phase D (Context Broker):** 40% complete - EventStore solid but memory blocks, archival search absent
- **Phase G (SandboxFS):** 5% complete - hypervisor is stub, no overlay layer

**Critical Risk:** The roadmap assumes Phase A is complete, but foundational infrastructure (EventBus integration, correlation ID propagation) is missing. This creates a false sense of readiness for Phase B.

---

## 1. Phase-by-Phase Assessment

### Phase A: Supervision Cutover (Reported as "complete" ✅ Actual: 80% ⚠️)

**Roadmap Claim:** Supervision cutover is complete.

**Reality Check:**

| Component | Status | Evidence |
|-----------|--------|----------|
| Supervision tree | ✅ Complete | `sandbox/src/supervisor/mod.rs:59-340` implements 3-tier hierarchy |
| Actor lifecycle (GetOrCreate) | ✅ Complete | `app_state.rs:54-100` with linked supervision |
| EventStore persistence | ✅ Complete | `event_store.rs` with SQLite/libsql backend |
| EventBus integration | ❌ Orphaned | Event bus exists but never spawned in supervision tree |
| Correlation ID propagation | ❌ Unused | `correlation_id` field defined but never propagated |
| Supervisor health monitoring | ❌ Missing | No metrics, restart counting, or thresholds |

**Critical Gaps:**

1. **EventBus Not Wired to Supervision** (`sandbox/src/actors/event_bus.rs:254`)
   - EventBusActor defined but never spawned via `Actor::spawn_linked()`
   - No EventBus reference in ApplicationSupervisor state
   - Process groups not joined for pub/sub
   - Impact: Worker results cannot be published, no event-based collection

2. **Correlation ID Infrastructure Missing**
   - Field exists in `Event` struct (line 62) but never used
   - No supervisor correlation ID generation
   - No correlation ID injection into worker tasks
   - No correlation ID aggregation across actor boundaries

3. **No Worker Actor Type**
   - Roadmap mentions "specialist agents" but no `WorkerActor` exists
   - No worker pool management in supervisors
   - No worker registry or lifecycle tracking

**Recommendation:** Before declaring Phase A complete, these gaps must be addressed. Otherwise Phase B will fail due to missing infrastructure.

---

### Phase B: Multiagent Control Plane v1 (Reported as "next" ✅ Actual: 10% ❌)

**Roadmap Claim:** Implement supervisor orchestration and specialist agent contracts.

**Reality Check:**

| Deliverable | Status | Evidence |
|-------------|--------|----------|
| Message protocols | ⚠️ Partial | EventBus has event types but no supervisor-worker protocols |
| Correlation IDs | ❌ Missing | Not propagated through supervision tree |
| One end-to-end delegated flow | ❌ Missing | No worker delegation exists |
| Task budget enforcement | ❌ Missing | No 50/200 call limits tracked |
| `run_async` API | ❌ Missing | No non-blocking delegation API |

**Critical Gaps:**

1. **No Supervisor Coordination Messages**
   - SessionSupervisor only routes GetOrCreate requests (`session.rs:34-58`)
   - No `DelegateTask`, `CollectResults`, `WorkerComplete` messages
   - Supervisors do not coordinate workflows

2. **Missing `run_async` Pattern**
   - ChatAgent uses `tokio::spawn` for fire-and-forget (`chat.rs:140-167`)
   - No structured API for parallel worker delegation
   - No result collection channels
   - No timeout handling across workers

3. **No Task Budget Enforcement**
   - AGENTS.md mandates: Supervisor 50 calls, Worker 200 calls
   - No call counting anywhere in codebase
   - No budget tracking or enforcement mechanisms

4. **Worker Lifecycle Management**
   - No "WorkerActor" type exists
   - No worker registry in supervisors
   - No worker pool or task queue management

**Online Research Recommendations (Applied to Phase B):**

- ✅ **Hierarchical supervisor pattern** (OpenAI Agents API) - ChoirOS Ractor foundation aligns well
- ✅ **Correlation IDs mandatory** for tracing workflows - matches roadmap intent but not implemented
- ✅ **Multi-level timeouts** (operation, task, workflow) - roadmap mentions timeout contracts but not implementation
- ✅ **Graceful degradation** when specialists fail - roadmap mentions fallback but no implementation
- ✅ **Separate concerns** - routing, execution, supervision should be different actors

**Recommendation:** Phase B requires substantial implementation before rollout. The 10% completion status means this is effectively starting from scratch.

---

### Phase F: Identity and Scope Enforcement v1 (Reported as "now" ✅ Actual: 45% ⚠️)

**Roadmap Claim:** Prevent cross-user/session data leakage before compounding memory.

**Reality Check:**

| Deliverable | Status | Evidence |
|-------------|--------|----------|
| Required scope keys on requests | ⚠️ Partial | user_id, actor_id exist; session_id, thread_id missing |
| Enforcement checks at API/supervisor boundaries | ⚠️ Partial | Actor isolation exists but no explicit scope validation |
| Isolation tests prove no leakage | ❌ Missing | No isolation tests found |

**Existing Scoping Keys:**
- ✅ `user_id` - tracked in all events and actors
- ✅ `actor_id` - per-instance identity
- ✅ `desktop_id` / `window_id` / `app_id` - desktop hierarchy
- ✅ `terminal_id` - terminal session tracking
- ❌ `session_id` - **MISSING**
- ❌ `thread_id` - **MISSING**

**Critical Gaps:**

1. **No Session-Level Scoping**
   - Roadmap requires `session_id` for conversation sessions
   - Database schema lacks session_id column
   - No session lifecycle management

2. **No Thread-Level Scoping**
   - Roadmap requires `thread_id` for conversation threading
   - Uses actor_id instead, which conflates session with instance
   - No support for conversation branching

3. **No Cross-Scope Prevention**
   - EventStore queries filter by actor_id but not session/workspace
   - No explicit scope checks in EventStoreActor
   - No validation that events belong to same workspace/session

**Online Research Recommendations (Applied to Phase F):**

- ✅ **Hierarchical scoping** (Global → Workspace → Session → Thread → Actor) - roadmap intent correct
- ✅ **Explicit scope columns** in database - need to add workspace_id, session_id, thread_id
- ✅ **Scope-aware queries** - need to enforce in EventStoreActor
- ⚠️ **Scope enforcement mechanisms** - currently missing

**Recommendation:** Phase F must add session_id and thread_id to schema, enforce scope checks, and write isolation tests before Phase D (Context Broker) is production-safe.

---

### Phase D: Context Broker v1 (Reported as "next" ✅ Actual: 40% ⚠️)

**Roadmap Claim:** Layered context retrieval with compounding intelligence and drill-down handles.

**Reality Check:**

| Deliverable | Status | Evidence |
|-------------|--------|----------|
| Canonical events in libsql | ✅ Complete | EventStore using SQLite/libsql |
| Derived memory layers | ❌ Missing | No memory blocks, archival memory, or summarization |
| API: brief_context + expand(handle) | ❌ Missing | No context broker API exists |
| Relevance test | ❌ Missing | No retrieval or relevance testing |

**Existing Foundations:**
- ✅ EventStoreActor provides centralized event storage
- ✅ Event types defined for all domain events (`shared-types/src/lib.rs:306-316`)
- ✅ Actor isolation prevents cross-contamination
- ✅ Historical context retrieval via `since_seq`
- ✅ ChatAgent loads history from events (`chat_agent.rs:474-498`)

**Missing Components:**

1. **No Core Memory Blocks**
   - No memory blocks (persona, human, scratchpad)
   - No always-visible structured memory
   - Letta pattern not implemented

2. **No Working Memory Layer**
   - Recent events exist but no "last N" window management
   - No automatic context reconstruction
   - No sliding window with compaction

3. **No Archival Memory**
   - No vector-based semantic search
   - No agent tools for memory insert/search
   - No tag-based organization

4. **No Compaction/Summarization**
   - No context compaction when exceeding limits
   - No drill-down handles for recent events
   - No cheaper model for summarization

**Online Research Recommendations (Applied to Phase D):**

- ✅ **Three-tier memory hierarchy** (Core, Working, Archival) - matches Letta pattern
- ✅ **Memory blocks pattern** for always-visible context - need to implement
- ✅ **Sliding window compaction** with summary + handles - need to implement
- ✅ **Vector search** for archival memory - need to implement
- ✅ **SQLite/libsql over JSONL** - current choice correct, keep it

**Recommendation:** Phase D requires building three new components:
1. MemoryBrokerActor (core memory blocks)
2. ArchivalMemoryActor (vector search)
3. Compaction service (sliding window summarization)

---

### Phase G: SandboxFS Persistence (Reported as "later" ✅ Actual: 5% ❌)

**Roadmap Claim:** Durable virtual filesystem with snapshot and rehydrate.

**Reality Check:**

| Deliverable | Status | Evidence |
|-------------|--------|----------|
| SandboxFS interface | ❌ Missing | No filesystem abstraction layer |
| SQLite/libsql-backed storage | ❌ Missing | Hypervisor is stub implementation |
| Versioned snapshots | ❌ Missing | No snapshot mechanism |
| Restart/rehydrate test | ❌ Missing | No rehydration exists |

**Current State:**
- ❌ **Hypervisor is stub** (`hypervisor/src/main.rs:1-5`) - just prints placeholder
- ❌ **No SandboxFS layer** - Direct filesystem access via std::fs
- ✅ **Terminal PTY exists** - portable-pty management in TerminalActor
- ✅ **Session handoff skills exist** - in `skills/` directory but not integrated
- ✅ **File tools with path validation** - `sandbox/src/tools/mod.rs:214-349`

**Critical Gaps:**

1. **No OverlayFS Abstraction**
   - No layered filesystem (lowerdir/upperdir/merged)
   - No copy-on-write semantics
   - No virtual merged view

2. **No Snapshot Mechanism**
   - No diff-based overlay tracking
   - No point-in-time checkpoint creation
   - No snapshot storage (local or remote)

3. **No Rehydration Service**
   - No rollback mechanism
   - No actor state restoration
   - No event log position restoration

**Online Research Recommendations (Applied to Phase G):**

- ✅ **OverlayFS approach** (Docker pattern) - need to implement layered filesystem
- ✅ **Multi-tier snapshots** (in-memory, local disk, remote) - roadmap intent correct
- ✅ **Copy-on-write semantics** - need to implement
- ✅ **Incremental rehydration** - need to implement

**Recommendation:** Phase G is essentially starting from scratch. Requires building:
1. OverlayFSActor (layered filesystem)
2. SnapshotManagerActor (snapshot lifecycle)
3. RehydrationService (async restoration)

---

## 2. Architecture Alignment with Industry Best Practices

### 2.1 Supervision & Multiagent (Phase B)

**Roadmap Intent:** Supervisor orchestrates, workers execute.

**Industry Best Practices (OpenAI Agents, LangGraph, AutoGen, Ractor):**
- ✅ Hierarchical supervisor pattern - ChoirOS Ractor foundation aligns well
- ✅ Correlation IDs mandatory for tracing - roadmap mentions but not implemented
- ✅ Multi-level timeouts (operation, task, workflow) - roadmap mentions timeout contracts but not implementation
- ✅ Graceful degradation when specialists fail - roadmap mentions but no implementation
- ✅ Separate concerns: routing, execution, supervision - roadmap intent correct

**Gaps:**
- ❌ No WorkerActor type
- ❌ No `run_async` API
- ❌ No task budget enforcement (50/200 call limits)
- ❌ No circuit breaker for failing specialists
- ❌ No message protocol layer (AgentMessage with correlation IDs)

**Critical Alignment Issue:**
The roadmap correctly identifies the pattern but assumes infrastructure exists. In reality, the EventBus is orphaned and worker patterns are completely missing.

---

### 2.2 Context & Memory (Phase D)

**Roadmap Intent:** Layered context retrieval with compounding intelligence.

**Industry Best Practices (Letta, LangChain, MemGPT):**
- ✅ Three-tier memory hierarchy (Core, Working, Archival) - roadmap intent correct
- ✅ Memory blocks pattern (persona, human, scratchpad) - roadmap mentions "summary + handles" but not memory blocks
- ✅ Sliding window compaction - roadmap mentions but not implemented
- ✅ Vector search for archival memory - roadmap mentions "relevant handles" but not semantic search
- ✅ SQLite/libsql over JSONL - current choice correct

**Gaps:**
- ❌ No MemoryBrokerActor
- ❌ No ArchivalMemoryActor
- ❌ No compaction service
- ❌ No agent tools for memory insert/search
- ❌ No context window management (prioritization, limiting)

**Critical Alignment Issue:**
Roadmap correctly identifies the need for layered memory but underestimates implementation effort. Need three new actors plus database migrations.

---

### 2.3 Identity & Scope (Phase F)

**Roadmap Intent:** Prevent cross-user/session data leakage.

**Industry Best Practices:**
- ✅ Hierarchical scoping (Global → Workspace → Session → Thread → Actor) - roadmap intent correct
- ✅ Explicit scope columns in database - roadmap mentions but not implemented
- ✅ Scope-aware queries - need to enforce in EventStoreActor
- ⚠️ Scope enforcement mechanisms - currently missing

**Gaps:**
- ❌ No session_id column in events table
- ❌ No thread_id column in events table
- ❌ No scope validation in EventStoreActor
- ❌ No isolation tests

**Critical Alignment Issue:**
Roadmap correctly identifies that Phase F must precede Phase D, but doesn't acknowledge that session_id/thread_id are completely missing from schema.

---

### 2.4 SandboxFS (Phase G)

**Roadmap Intent:** Durable virtual filesystem with snapshot and rehydrate.

**Industry Best Practices (Docker OverlayFS, Kubernetes Volumes):**
- ✅ Layered filesystem (lowerdir/upperdir/merged) - roadmap intent correct
- ✅ Copy-on-write semantics - not implemented
- ✅ Multi-tier snapshots - roadmap mentions but not implemented
- ✅ Incremental rehydration - not implemented
- ✅ Snapshot metadata (actor states, event log position) - not implemented

**Gaps:**
- ❌ No OverlayFS abstraction layer
- ❌ No SnapshotManagerActor
- ❌ No RehydrationService
- ❌ No diff-based overlay tracking
- ❌ Hypervisor is stub

**Critical Alignment Issue:**
Roadmap correctly identifies the need for SandboxFS but places it as "later". However, the implementation is 5% complete, meaning this is effectively starting from scratch.

---

## 3. Risk Assessment

### Risk 1: False Readiness for Phase B ⚠️⚠️⚠️

**Severity:** Critical

**Issue:** Roadmap reports Phase A as "complete", but EventBus is orphaned and correlation IDs unused.

**Impact:**
- Phase B implementation will fail due to missing infrastructure
- Workers cannot publish results without EventBus integration
- No tracing across actor boundaries without correlation ID propagation

**Mitigation:**
1. Wire EventBus to supervision tree (spawn EventBusActor in ApplicationSupervisor)
2. Implement correlation ID generation and propagation
3. Add supervisor health monitoring

---

### Risk 2: Context Leakage Before Phase F ⚠️⚠️

**Severity:** High

**Issue:** Phase D (Context Broker) depends on Phase F (Identity/Scope) but session_id/thread_id are missing.

**Impact:**
- Context broker cannot implement safe memory isolation
- Cross-user/session retrieval leakage possible
- Reorg required if Phase D is built first

**Mitigation:**
1. Add session_id and thread_id to events table
2. Implement scope validation in EventStoreActor
3. Write isolation tests proving no leakage

---

### Risk 3: Underestimation of Phase B Effort ⚠️⚠️

**Severity:** High

**Issue:** Roadmap treats Phase B as "next" but implementation is 10% complete.

**Impact:**
- Timeline significantly underestimated
- Three new actors required (WorkerActor, MessageBus, WorkflowSupervisor)
- Complete `run_async` API needed

**Mitigation:**
1. Reassess timeline for Phase B (add 2-3 weeks)
2. Implement worker pattern before any multiagent rollout
3. Add integration tests for delegated workflows

---

### Risk 4: SandboxFS Complexity Underestimated ⚠️

**Severity:** Medium

**Issue:** Roadmap places SandboxFS as "later" but implementation is 5% complete.

**Impact:**
- Requires building OverlayFS, SnapshotManager, RehydrationService
- Hypervisor is stub, not functional
- Significantly more work than anticipated

**Mitigation:**
1. Reassess SandboxFS timeline (add 3-4 weeks)
2. Implement OverlayFSActor first (foundation)
3. Add snapshot/rehydration tests

---

### Risk 5: Compartmentalization Risk (App Expansion Wave 1) ⚠️

**Severity:** Medium

**Issue:** Roadmap allows App Expansion Wave 1 (file explorer, settings, viewers) in parallel with Phase G.

**Impact:**
- Expanding app surface before core control-plane maturity
- Increased maintenance drag
- Potential rework if sandbox changes required

**Mitigation:**
1. Complete Phase G (SandboxFS) before Wave 1 apps
2. Or implement Wave 1 apps without persistent state (read-only viewers)
3. Ensure apps use SandboxFS abstraction layer

---

## 4. Recommended Execution Order Adjustments

### Current Critical Path (from roadmap):
1. B Multiagent Control Plane v1
2. F Identity and Scope Enforcement v1
3. C Chat Delegation Refactor
4. D Context Broker v1
5. G SandboxFS Persistence
6. H Hypervisor Integration

### Adjusted Critical Path (with gap mitigation):

**Phase 0: Complete Phase A Foundation (1-2 weeks)**
- Wire EventBus to supervision tree
- Implement correlation ID generation and propagation
- Add supervisor health monitoring
- Update Phase A status to 100%

**Phase 1: Identity & Scope Enforcement v1 (1-2 weeks)**
- Add session_id and thread_id to events table
- Implement scope validation in EventStoreActor
- Write isolation tests
- Update Phase F status to 100%

**Phase 2: Multiagent Control Plane v1 (3-4 weeks)**
- Implement WorkerActor
- Add `run_async` API
- Implement task budget enforcement (50/200 call limits)
- Wire MessageBus for routing
- Add correlation ID tracking
- Write integration tests for delegated workflows
- Update Phase B status to 80%

**Phase 3: Orchestration Layer Refactor v1 (1-2 weeks)**
- Implement routing policy (selective delegation)
- Add timeout/retry/error contracts
- Integrate ChatAgent as compatibility surface (escalates multi-step planning to Conductor)
- Implement Conductor as primary orchestration surface
- Write tests for graceful degradation
- Apply NO ADHOC WORKFLOW: encode control flow in typed protocols, not string matching
- Update Phase C status to 80%

**Phase 4: Context Broker v1 (3-4 weeks)**
- Implement MemoryBrokerActor (core memory blocks)
- Implement ArchivalMemoryActor (vector search)
- Add compaction service (sliding window)
- Create context broker API (brief_context, expand)
- Write relevance tests
- Update Phase D status to 70%

**Phase 5: SandboxFS Persistence (3-4 weeks)**
- Implement OverlayFSActor (layered filesystem)
- Implement SnapshotManagerActor
- Implement RehydrationService
- Write rehydration tests
- Update Phase G status to 60%

**Phase 6: Hypervisor Integration (2-3 weeks)**
- Implement hypervisor auth and routing
- Integrate SandboxFS with hypervisor
- Add session lifecycle management
- Write multi-session isolation tests
- Update Phase H status to 60%

**Total Adjusted Timeline:** 14-21 weeks (3-5 months) vs. roadmap's implied shorter timeline

---

## 5. Concrete Recommendations

### Immediate (Before Phase B Start):

1. **Complete Phase A Foundation:**
   - Wire EventBus to supervision tree
   - Implement correlation ID propagation
   - Add supervisor health monitoring
   - Remove "complete" status from Phase A until these are done

2. **Add Missing Scope Keys:**
   - Migration: Add session_id, thread_id to events table
   - Update EventStoreActor to enforce scope checks
   - Write isolation tests

3. **Document Phase B Gaps:**
   - Update roadmap to reflect 10% completion status
   - Explicitly list missing components (WorkerActor, run_async, budgets)
   - Reassess timeline

### Short Term (Next 4-6 weeks):

4. **Implement Worker Pattern:**
   - Create WorkerActor type
   - Implement `run_async` API
   - Add task budget enforcement (50/200 call limits)
   - Wire MessageBus for routing

5. **Implement Context Broker v1:**
   - Create MemoryBrokerActor (core memory blocks)
   - Create ArchivalMemoryActor (vector search)
   - Add compaction service
   - Create context broker API

6. **Implement SandboxFS:**
   - Create OverlayFSActor
   - Create SnapshotManagerActor
   - Create RehydrationService

### Long Term (Beyond Critical Path):

7. **App Expansion Wave 1:**
   - Defer until SandboxFS is production-ready
   - Or implement read-only viewers first
   - Ensure apps use SandboxFS abstraction layer

8. **Nix/NixOS Migration:**
   - Keep as cross-cutting, non-blocking work
   - Complete Stage 1 (dev shell) early
   - Defer Stage 4 (host-level operations) until product is stable

---

## 6. Definition of Ready (Revised)

**Original Definition of Ready for Multiagent Rollout:**
- Supervision-first runtime is stable
- Control-plane contracts are implemented and tested for at least one delegated flow
- Scope enforcement v1 is active on all relevant request/event paths
- Context broker API shape is defined (implementation may be partial, but interface is locked)

**Revised Definition of Ready for Multiagent Rollout:**
- ✅ Phase A is 100% complete (EventBus wired, correlation IDs propagated, health monitoring)
- ✅ Phase F is 100% complete (session_id/thread_id added, scope validation enforced, isolation tests passing)
- ✅ Phase B is at least 50% complete (WorkerActor, run_async API, task budgets implemented)
- ✅ One end-to-end delegated flow exists and is tested (e.g., supervisor delegates terminal task to worker, result collected via EventBus)
- ✅ Correlation ID tracing works across supervisor → worker → EventBus boundaries
- ✅ Context broker API is defined and documented (even if partially implemented)

---

## 7. Conclusion

The roadmap correctly identifies critical dependencies (B→F→C→D→G→H) and shows strong intuition about architecture priorities. However, it significantly underestimates implementation gaps:

- **Phase A:** Not 100% complete - EventBus orphaned, correlation IDs unused
- **Phase B:** 10% complete, not "next" ready - worker pattern, run_async, budgets missing
- **Phase F:** 45% complete - session_id/thread_id missing
- **Phase D:** 40% complete - memory blocks, archival memory, compaction missing
- **Phase G:** 5% complete - SandboxFS completely missing

**Critical Recommendation:** Before starting Phase B, complete Phase A foundation (EventBus wiring, correlation ID propagation). Before starting Phase D, complete Phase F (session_id/thread_id, scope validation).

**Estimated Timeline:** 14-21 weeks (3-5 months) for complete implementation vs. roadmap's implied shorter timeline.

**Key Risk:** Starting Phase B without completing Phase A will cause failures due to missing infrastructure (EventBus, correlation IDs). This is the highest-risk dependency in the roadmap.

---

## References

### Codebase Analysis
- `sandbox/src/supervisor/mod.rs` - Supervision tree
- `sandbox/src/actors/event_bus.rs` - EventBus (orphaned)
- `sandbox/src/actors/event_store.rs` - Event persistence
- `sandbox/src/actors/chat_agent.rs` - Chat agent (BAML integration)
- `sandbox/migrations/20260130000000_create_events_table.sql` - Event schema
- `hypervisor/src/main.rs` - Stub hypervisor

### Online Research
- OpenAI Agents API - Hierarchical supervisor pattern
- LangGraph - Graph-based orchestration
- AutoGen - Group chat patterns
- Ractor - Supervision trees, message priority
- Letta (MemGPT) - Memory blocks, archival memory, compaction
- Docker OverlayFS - Layered filesystems
- SQLite/libsql - Event storage best practices
