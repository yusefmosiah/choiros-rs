# Implementing ADR-0019: Per-User Memory Curation and Retrieval

Date: 2026-03-15
Kind: Guide
Status: Active
Priority: 4
Requires: [ADR-0019]

## Narrative Summary (1-minute read)

The live ChoirOS memory path is much smaller than the ADR destination. Today
the active code is a lexical SQLite-backed `MemoryActor` in
`sandbox/src/actors/memory.rs`, spawned by `SessionSupervisor`, queried only
by Conductor at run start, and usually backed by `:memory:` storage. There is
no live `MemoryCurator`, no durable default producer path, no Writer or
Terminal retrieval integration, and no graph projection.

This guide turns ADR-0019 into a sequence that matches the current code. The
order matters:

1. Make the existing memory path durable, optional, and observable.
2. Route high-signal runtime outputs through a canonical producer boundary.
3. Add `MemoryCurator` as the event-driven artifact curator.
4. Add explicit retrieval surfaces for Writer and Terminal.
5. Add the temporal graph only after curated artifacts exist and traces prove
   the schema is worth carrying.

Do not start with graph migrations, embeddings, or the stale vector-memory
code in `sandbox/src/actors/memory/actor.rs`. None of those are on the live
path today.

## What Changed

- 2026-03-15: Initial implementation guide grounded in the active Rust memory
  path and current event/report plumbing.

## What To Do Next

Execute Phases 1 through 4 before implementing Section 11 of the ADR. The
temporal graph is the last step here, not the first one.

---

## Source Of Truth

These files define the current implementation boundary:

| File | Why it matters |
|------|----------------|
| `sandbox/src/actors/memory.rs` | Active `MemoryActor`, SQLite schema, lexical retrieval, ingest/query contracts |
| `sandbox/src/supervisor/session.rs` | Spawns `MemoryActor`; currently defaults to `:memory:` |
| `sandbox/src/supervisor/conductor.rs` | Wires memory only into Conductor |
| `sandbox/src/actors/conductor/runtime/start_run.rs` | Sole live best-effort query surface (`GetContextSnapshot`) |
| `sandbox/src/supervisor/mod.rs` | Defines `ApplicationSupervisorMsg::IngestWorkerTurnReport` canonical signal boundary |
| `sandbox/src/actors/researcher/adapter.rs` | Emits worker reports as raw events today |
| `sandbox/src/actors/terminal.rs` | Emits worker reports as raw events today |
| `sandbox/src/actors/writer/adapter.rs` | Emits writer-local delegation report events today |
| `sandbox/src/actors/event_store.rs` | Canonical append-only event log for replay and backfill |
| `sandbox/src/actors/event_bus.rs` | Delivery plane for live subscriptions |
| `sandbox/src/actors/event_relay.rs` | Bridges committed EventStore rows into EventBus topics |
| `sandbox/tests/memory_actor_test.rs` | Current contract coverage for memory ingest/search/packing |
| `sandbox/tests/worker_signal_contract_test.rs` | Current contract coverage for canonical worker report ingestion |

## Current State

1. `MemoryActor` is already the right passive boundary. Keep it.
2. The default application path is non-durable because `SessionSupervisor`
   resolves `vec_db_path` to `:memory:`.
3. Conductor is the only live retrieval consumer.
4. Runtime producers do not call `MemoryMsg::Ingest`.
5. Worker/report signals already exist, but their canonical ingestion path is
   not wired into memory.
6. The temporal graph and `MemoryCurator` described by ADR-0019 do not exist.

## Phase Status

```text
Phase 1  (coherence + durability)        NOT STARTED
Phase 2  (canonical producer ingress)    NOT STARTED
Phase 3  (MemoryCurator actor)           NOT STARTED
Phase 4  (Writer/Terminal retrieval)     NOT STARTED
Phase 5  (temporal graph projection)     DEFERRED
```

## Phase 1: Make The Current Memory Path Coherent

Goal: keep the current lexical retrieval design, but make it durable,
optional, and observable enough to support later curation work.

### Scope

- Keep `MemoryActor` passive: ingest plus retrieval only.
- Preserve best-effort degradation semantics.
- Make production boot use a real per-user SQLite path instead of `:memory:`.
- Add observability around empty retrieval, duplicate ingest, and ingest
  failure.

### Files To Modify

| File | Change |
|------|--------|
| `sandbox/src/supervisor/session.rs` | Stop assuming `:memory:` for normal app runs; plumb a per-user durable path |
| `sandbox/src/supervisor/mod.rs` | Decide whether memory is enabled and what per-user path to pass into session creation |
| `sandbox/src/actors/memory.rs` | Split duplicate-skip from real insert failure; emit structured observability events |
| `sandbox/src/actors/conductor/runtime/start_run.rs` | Emit retrieval-attempt/hit/empty/failure metrics or events around the existing query |
| `sandbox/tests/memory_actor_test.rs` | Extend tests for duplicate-vs-error behavior and file-backed durability |

### Implementation Notes

1. Add an explicit boot-time memory mode:
   - disabled
   - enabled with durable SQLite path
   - test-only `:memory:`
2. Treat "memory disabled" and "memory empty" as normal operating modes.
3. Change ingest return semantics so callers can distinguish:
   - inserted
   - duplicate skipped
   - failed
4. Emit structured events or logs for:
   - retrieval attempted
   - retrieval empty
   - retrieval failed or timed out
   - ingest inserted
   - ingest duplicate skipped
   - ingest failed

### Exit Criteria

- A normal application run can use durable per-user memory storage.
- Existing Conductor behavior still degrades cleanly when memory is off, slow,
  or empty.
- Operators can tell whether memory is empty, disabled, or broken without
  guessing from logs.

## Phase 2: Canonical Producer Ingress

Goal: define one high-signal way for runtime work to become memory input
without dumping raw transcripts into retrieval.

### Scope

- Reuse `ApplicationSupervisorMsg::IngestWorkerTurnReport` as the canonical
  worker-report boundary.
- Normalize Worker, Researcher, and Terminal summaries before memory ingest.
- Keep Writer document state canonical; only derived memory artifacts get
  stored.

### Files To Modify

| File | Change |
|------|--------|
| `sandbox/src/supervisor/mod.rs` | Extend canonical report ingestion so accepted signals can feed curated memory inputs |
| `sandbox/src/actors/agent_harness/mod.rs` | Route report completion through the canonical supervisor ingress instead of raw event-only emission |
| `sandbox/src/actors/researcher/adapter.rs` | Reuse canonical report ingestion path |
| `sandbox/src/actors/terminal.rs` | Reuse canonical report ingestion path |
| `sandbox/src/actors/writer/adapter.rs` | Keep writer-local report events scoped; only promote canonical summaries worth remembering |
| `shared-types/src/lib.rs` | Add typed result/status fields only if the existing report schema cannot express ingestion outcomes cleanly |

### Implementation Notes

1. Do not let producers call `MemoryMsg::Ingest` ad hoc from multiple actors.
   That will recreate the hidden-conversation-bucket failure mode.
2. Canonical producer categories for v1:
   - `user_inputs`
   - `version_snapshots`
   - `run_trajectories`
   - `doc_trajectories`
3. Start with content already present in the runtime:
   - user objective summaries
   - accepted worker findings and learnings
   - terminal completion or failure summaries
   - writer canonical output summaries
4. Preserve provenance fields from the start:
   - `user_id`
   - `session_id`
   - `thread_id`
   - `run_id`
   - `source_ref`
   - producing actor
   - timestamps

### Exit Criteria

- Worker-style outputs enter memory through one canonical boundary.
- Producers store bounded summaries, not raw transcripts or tool chatter.
- Tests prove accepted reports produce canonical signals with enough
  provenance to support later retrieval.

## Phase 3: Add MemoryCurator

Goal: move curation responsibility out of foreground actors and into an
always-running background actor without changing `MemoryActor`'s passive role.

### Scope

- Spawn `MemoryCurator` alongside `MemoryActor` in `SessionSupervisor`.
- Subscribe to committed runtime events through EventBus/EventRelay.
- Project high-signal events into bounded memory artifacts.
- Keep replay and dedup idempotent.

### Files To Modify

| File | Change |
|------|--------|
| `sandbox/src/actors/` | Add a new `memory_curator.rs` actor module |
| `sandbox/src/actors/mod.rs` | Export the new actor |
| `sandbox/src/supervisor/session.rs` | Spawn and supervise `MemoryCurator` next to `MemoryActor` |
| `sandbox/src/actors/event_bus.rs` | Use existing subscribe/query surfaces for curator subscriptions |
| `sandbox/src/actors/event_relay.rs` | Verify committed events reach EventBus topics the curator will watch |
| `sandbox/tests/` | Add integration coverage for curator subscription, replay, and idempotent ingest |

### Implementation Notes

1. `MemoryCurator` should subscribe to a narrow set of topics first:
   - `worker.report.received`
   - `worker.finding.created`
   - `worker.learning.created`
   - `research.finding.created`
   - `research.learning.created`
   - `writer.run.status`
   - `writer.run.changeset`
2. The curator should keep its own replay cursor so it can recover after a
   crash and backfill from EventStore.
3. The curator writes only derived artifacts. It does not mutate documents and
   it does not become a second source of truth.
4. If event volume becomes noisy, tighten admission rules before adding more
   topics.

### Exit Criteria

- `MemoryCurator` can restart and resume from the last committed event cursor.
- Background curation produces non-empty memory without requiring foreground
  actors to know memory internals.
- `MemoryActor` remains a passive query/store boundary.

## Phase 4: Add The Right Query Surfaces

Goal: use memory where continuity matters instead of widening Conductor's
implicit prompt stuffing.

### Scope

- Keep Conductor start-of-run retrieval bounded and best-effort.
- Add explicit Writer retrieval points for document continuity.
- Add explicit Terminal retrieval points for long-running coding and
  verification loops.

### Files To Modify

| File | Change |
|------|--------|
| `sandbox/src/supervisor/session.rs` | Pass `memory_actor` into Writer and Terminal supervisors |
| `sandbox/src/supervisor/writer.rs` | Accept optional memory actor handle |
| `sandbox/src/supervisor/terminal.rs` | Accept optional memory actor handle |
| `sandbox/src/actors/writer/*` | Add explicit retrieval calls at document continuity boundaries |
| `sandbox/src/actors/terminal.rs` | Add explicit retrieval calls for long coding or verification loops |
| `sandbox/tests/` | Add no-memory and empty-memory integration tests for Writer and Terminal |

### Implementation Notes

1. Prefer explicit retrieval calls over hidden automatic context injection.
2. Use `ArtifactContextPack` or a similarly budgeted retrieval surface when
   prompt size matters.
3. Add query points only where the actor already has a clear objective and
   enough provenance to scope the request.
4. Empty retrieval must remain a normal result.

### Exit Criteria

- Writer and Terminal can retrieve relevant prior context without relying on
  Conductor to overstuff prompts.
- Retrieval remains optional, bounded, and provenance-preserving.

## Phase 5: Temporal Graph Projection

Goal: implement ADR-0019 Sections 10 through 14 only after curated artifacts
exist and the graph has real inputs.

### Scope

- Add the additive `nodes`, `edges`, and `events` tables from the ADR.
- Project from curated artifacts and canonical documents, not raw event spam.
- Start with software-document structure only, then generalize carefully.

### Files To Modify

| File | Change |
|------|--------|
| `sandbox/src/actors/memory.rs` or a new graph store module | Add migrations for temporal graph tables |
| `sandbox/src/actors/memory_curator.rs` | Project graph nodes and edges from curated events |
| `docs/theory/` consumers | Add the first graph-backed view such as `ATLAS.md` rendering |
| `sandbox/tests/` | Add migration, point-in-time query, and graph projection coverage |

### Implementation Notes

1. Do not build the graph from raw transcripts.
2. Start with a narrow node set:
   - document
   - section
   - claim
   - finding
   - concept
3. Start with a narrow edge set:
   - cites
   - depends_on
   - derived_from
   - relates_to
4. Only widen into travel/media/conversation domains after the projection and
   query model is stable for the current document-centered work.

### Exit Criteria

- Graph tables are additive to the existing memory store.
- At least one useful graph-backed view exists.
- Point-in-time queries work on real projected data, not synthetic examples.

## Recommended PR Sequence

1. PR 1: durable memory boot path plus observability
2. PR 2: canonical worker-report ingress for memory-worthy signals
3. PR 3: `MemoryCurator` actor with event replay cursor
4. PR 4: Writer and Terminal retrieval integration
5. PR 5: temporal graph migration plus one graph-backed view

## Explicit Non-Goals For This Guide

- Reviving `sandbox/src/actors/memory/actor.rs`
- Adding embeddings or vector search in v1
- Storing raw transcripts or tool logs in memory
- Making memory required for correctness
- Mixing per-user memory with the future global knowledge base
