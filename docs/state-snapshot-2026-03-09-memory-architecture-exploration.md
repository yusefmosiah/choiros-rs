# Memory Architecture Exploration
Date: 2026-03-09
Kind: Snapshot
Status: Active
Requires: []

## Narrative Summary (1-minute read)

The active Rust memory implementation is [`sandbox/src/actors/memory.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/memory.rs). It is a small, symbolic retrieval actor backed by SQLite tables and lexical scoring, not the richer vector/episodic system described in older design docs.

Today, the runtime wires memory into only one live consumer: Conductor start-of-run context injection. There are no live runtime producers calling `Ingest`, and the default application path spawns memory on `:memory:` storage, so the service is usually both empty and non-durable. In practice, memory currently exists as optional retrieval infrastructure, not as a meaningful runtime pillar.

The right current-path architecture is to keep memory in that role: an optional retrieval service that improves context quality when populated, but never defines correctness. The minimum useful next step is not vector search. It is to make the existing symbolic service coherent, observable, and non-empty through a small number of explicit producers owned primarily by Writer and later Terminal and Researcher.

## What Changed

1. Confirmed the active implementation boundary in code instead of relying on archived design intent.
2. Confirmed the live runtime path from `ApplicationSupervisor` to `SessionSupervisor` to `MemoryActor` to Conductor.
3. Confirmed there are no live runtime `Ingest` producers outside tests.
4. Confirmed Writer, Terminal, and Researcher do not currently query memory.
5. Verified the focused memory test binary passes on the current tree.

## What To Do Next

1. Keep `MemoryActor` as optional retrieval infrastructure, not mandatory orchestration state.
2. Add observability for query attempts, empty hits, disabled memory, and ingest outcomes.
3. Add a small set of explicit producers:
   Writer canonical outputs and user objective summaries first, Terminal completion and verification summaries second, Researcher evidence summaries third.
4. Add explicit query surfaces for Writer and Terminal after producers exist.
5. Defer vector and embedding work until traces show the symbolic path is materially insufficient.

## 1. Source of Truth: What Is Active

### Active module

The active module is [`sandbox/src/actors/memory.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/memory.rs).

Why this is authoritative:

- [`sandbox/src/actors/mod.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/mod.rs) exports `pub mod memory;`, which resolves to `memory.rs`, not the `memory/` directory.
- `SessionSupervisor`, `ConductorSupervisor`, and `ConductorActor` all import `crate::actors::memory::{...}` from that module.
- The dedicated integration suite [`sandbox/tests/memory_actor_test.rs`](/Users/wiz/choiros-rs/sandbox/tests/memory_actor_test.rs) exercises `MemoryActor`, `MemoryMsg`, `IngestRequest`, `CollectionKind`, and `VecStore` from `memory.rs`.

### `memory/actor.rs` classification

[`sandbox/src/actors/memory/actor.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/memory/actor.rs) is stale alternate code, not part of the live module graph.

Reasons:

- The `memory/` directory has no `mod.rs`.
- No reachable Rust module declares `mod actor;` under `memory`.
- The file imports `super::protocol::{...}`, but there is no sibling `protocol` module in that directory.
- Its API is a different design entirely: `MemoryAgent`, embedding generation, `sqlite-vec`, MCP calls, pattern storage, and semantic retrieval.

Conclusion: treat it as historical or experimental residue, not an active alternate runtime path.

### Live callsites

Current live callsites touching memory are:

- [`sandbox/src/supervisor/mod.rs`](/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs): ApplicationSupervisor creates `SessionSupervisorArgs` with `vec_db_path: None`.
- [`sandbox/src/supervisor/session.rs`](/Users/wiz/choiros-rs/sandbox/src/supervisor/session.rs): `SessionSupervisor` always spawns `MemoryActor` and passes it only to `ConductorSupervisor`.
- [`sandbox/src/supervisor/conductor.rs`](/Users/wiz/choiros-rs/sandbox/src/supervisor/conductor.rs): `ConductorSupervisor` passes the optional memory actor into `ConductorArguments`.
- [`sandbox/src/actors/conductor/runtime/start_run.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/runtime/start_run.rs): Conductor does a best-effort `GetContextSnapshot` call at run start.
- [`sandbox/src/actors/conductor/runtime/conductor_adapter.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/runtime/conductor_adapter.rs): retrieved memory is rendered into conductor system context.

Current non-callsites:

- Writer: no memory query or ingest path found.
- Terminal: no memory query or ingest path found.
- Researcher: no memory query or ingest path found.
- Runtime ingestion: no non-test `MemoryMsg::Ingest` callsites found.

### Tests

The live test coverage is concentrated in [`sandbox/tests/memory_actor_test.rs`](/Users/wiz/choiros-rs/sandbox/tests/memory_actor_test.rs):

- store creation and hash determinism
- deduplicating ingest
- lexical search ranking
- cross-collection `GetContextSnapshot`
- `ArtifactSearch`
- `ArtifactExpand`
- `ArtifactContextPack`
- selective re-ingest behavior via content-hash dedup

Conductor test helpers also explicitly validate a no-memory code path by spawning the actor with `memory_actor: None` in [`sandbox/src/actors/conductor/tests/support.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/tests/support.rs).

## 2. Current Runtime Path End to End

The current runtime path is:

`ApplicationSupervisor -> SessionSupervisor -> MemoryActor -> ConductorSupervisor -> ConductorActor`

Important details:

1. [`sandbox/src/supervisor/mod.rs`](/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs) passes `vec_db_path: None`, so the default runtime does not configure a durable SQLite file.
2. [`sandbox/src/supervisor/session.rs`](/Users/wiz/choiros-rs/sandbox/src/supervisor/session.rs) resolves that to `:memory:` and always spawns `MemoryActor`.
3. The same supervisor passes `Some(memory_actor)` only into `ConductorSupervisor`, not into Writer, Terminal, or Researcher supervisors.
4. [`sandbox/src/actors/conductor/runtime/start_run.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/runtime/start_run.rs) makes a 500 ms best-effort `GetContextSnapshot` call with `max_items: 4`. If the call times out, fails, or returns zero items, the run continues without memory.
5. [`sandbox/src/actors/conductor/runtime/conductor_adapter.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/runtime/conductor_adapter.rs) appends the returned lines to the conductor prompt under `Retrieved memory context (relevance-ranked):`.

Practical behavior today:

- Memory is usually empty because there are no live runtime producers.
- Memory is non-durable by default because the app path uses `:memory:`.
- Conductor already degrades gracefully when memory is absent, slow, or empty.
- The rest of the actor system is not yet integrated with memory at all.

This means the current system is best described as "memory actor exists, but is mostly an empty optional retrieval stub."

## 3. Current Contract Audit

The current contract surface in [`sandbox/src/actors/memory.rs`](/Users/wiz/choiros-rs/sandbox/src/actors/memory.rs) is small and viable.

### `Ingest`

Purpose:

- insert one text artifact into one of four collections
- deduplicate by `chunk_hash`

Current behavior:

- returns `true` on insert
- returns `false` on duplicate
- also returns `false` on insert error, which means duplicate and error are currently conflated

Assessment:

- good enough for initial producers
- should remain explicit and caller-driven
- eventual observability should distinguish duplicate-skip from actual failure

### `ArtifactSearch`

Purpose:

- lexical search within one collection

Current behavior:

- uses token overlap plus exact-substring boost
- returns ranked `ContextItem` values

Assessment:

- appropriate as the minimum retrieval model for current product needs
- this is a symbolic retrieval service, despite leftover "vector" wording in comments and argument names

### `ArtifactExpand`

Purpose:

- multi-hop symbolic expansion from seed item IDs into related artifacts across collections

Current behavior:

- loads seed contents from the source collection
- uses each seed content as a lexical query across all four collections
- merges and deduplicates neighbors

Assessment:

- worth keeping because it provides a bounded "search then widen" operation without semantic infrastructure
- likely more useful for Writer and Terminal than for Conductor

### `ArtifactContextPack`

Purpose:

- pack retrieved artifacts into a token-budget-sized `ContextSnapshot`

Current behavior:

- performs fresh per-collection searches against the objective
- greedily adds results until `token_budget * 4` characters are used

Important contract drift:

- the doc comment says it runs `GetContextSnapshot` internally, but it does not
- the doc comment mentions a rationale field, but `ContextItem` has no rationale field

Assessment:

- keep the operation
- tighten the comment and contract later so implementation and docs match

### `GetContextSnapshot`

Purpose:

- retrieve a merged relevance-ranked cross-collection snapshot

Current behavior:

- searches all four collections
- truncates to `max_items`
- returns a `ContextSnapshot` with empty provenance

Assessment:

- this remains the right shared retrieval contract for now
- it is already the best-effort boundary Conductor consumes

### `ContextSnapshot`

The shared types in [`shared-types/src/lib.rs`](/Users/wiz/choiros-rs/shared-types/src/lib.rs) are still good enough for the current lane:

- `ContextItem`
- `CitationRef`
- `ContextSnapshot`

Assessment:

- keep `ContextSnapshot` as the cross-actor retrieval bundle
- do not replace it unless a concrete integration requirement forces that change
- allow its consumers to treat empty `items` as a normal best-effort outcome

## 4. Compare Current Code to Prior Architecture Intent

An earlier archived MemoryAgent design described a much richer system:

- local plus global memory
- vector search and HNSW indexes
- SONA learning
- episodic pattern memory
- pattern outcomes and reward updates
- future graph and GNN refinement

The live code does not implement that design. The active implementation is:

- local only
- SQLite table backed
- lexical only
- explicit ingest only
- zero runtime producers by default
- one live runtime consumer by default

Pieces still worth keeping from the older intent:

- memory is additive to filesystem truth, not a replacement for it
- memory should be optional infrastructure, not correctness-defining state
- retrieval should center on durable artifacts and evidence, not hidden chat state
- shared retrieval bundles should be typed and bounded

Pieces that should stay deferred:

- embeddings and vector indexes
- adaptive learning and reward shaping
- global shared knowledge layers
- hidden conversational-state accumulation
- large automatic ingestion pipelines

## 5. Architecture Decisions

### Decision 1: role of memory

Memory should be an optional retrieval service, not a mandatory runtime pillar.

Implications:

- the system must keep working when memory is disabled, unavailable, or empty
- memory may improve context quality, but must not determine correctness
- canonical state stays with Writer documents, durable artifacts, and normal actor state

### Decision 2: retrieval model

Continue with symbolic lexical retrieval for now.

Reason:

- it is already implemented
- it has passing focused tests
- current integration gaps are about producers and query surfaces, not retrieval sophistication
- there is no evidence yet that embeddings are the current bottleneck

### Decision 3: ownership and producers

Recommended producer ownership:

- Writer owns canonical run-scoped artifacts worth remembering
- Researcher contributes source-backed evidence summaries
- Terminal contributes summarized execution trajectories and verification outcomes
- Conductor mostly consumes memory and should not be the primary ingestion owner

### Decision 4: initial query surfaces

Recommended query surface order:

- Conductor: keep the existing best-effort start-of-run injection and later bounded wake-time retrieval
- Writer: add explicit retrieval for planning, delegation, and long-lived run continuity
- Terminal: add explicit query access for long coding loops and verification work
- Researcher: optional later consumer, not required for first useful integration

### Decision 5: first-class ingest units

Ingest these, not raw tool chatter:

- user prompt and objective summaries
- Writer version snapshots or canonical section summaries
- Terminal completion and verification summaries
- Researcher evidence summaries with source references

Avoid ingesting:

- raw high-volume tool traces
- entire unfiltered logs
- hidden conversational scratch state

## 6. Coherent Current-Path Implementation Plan

### Phase 1: make current memory architecture coherent

- document `memory.rs` as the active implementation
- explicitly treat `memory/actor.rs` as stale or quarantined code
- keep memory behind a clean optional service boundary
- add observability for query attempts, empty results, and ingest outcomes

Important nuance:

The current codebase already supports optional consumption at the Conductor boundary because `memory_actor` is an `Option`. But the main runtime still always spawns `MemoryActor`, so "memory off" is only partially true today. The first runtime hardening step is to make the service itself optional at boot, not just optional at the consumer API.

### Phase 2: make memory non-empty

Add a small number of explicit producers:

1. Writer canonical outputs and user objective summaries
2. Terminal completion and verification summaries
3. Researcher evidence summaries

Do not add broad automatic ingestion before there is evidence the summaries are insufficient.

### Phase 3: make memory useful to actors

- keep Conductor's existing `ContextSnapshot` injection
- add explicit Writer query points
- add Terminal query access for long-running loops
- verify graceful degradation when memory is absent or empty

### Phase 4: revisit richer retrieval only after use is visible

Only reconsider embeddings or vector retrieval after:

- memory is actually populated in real runs
- traces show query behavior and empty-hit patterns
- symbolic retrieval is demonstrably limiting Writer or Terminal outcomes

## 7. Verification Performed

Code exploration confirmed:

- no live runtime `Ingest` producers outside tests
- no Writer, Terminal, or Researcher memory callsites
- Conductor is the only active runtime consumer
- default application wiring uses `:memory:` storage

Focused test run:

```bash
cargo test -p sandbox --test memory_actor_test -- --nocapture
```

Result:

- 11 tests passed
- covered ingest, dedup, search, expand, context pack, snapshot merge, and selective re-ingest behavior

## 8. Bottom Line

The smallest useful architecture for ChoirOS right now is not a full episodic memory system. It is the existing lexical `MemoryActor`, treated honestly:

- optional
- best-effort
- explicit-producer-driven
- retrieval-only
- subordinate to canonical Writer state and durable artifacts

That is the right base to harden first. If that base becomes visibly useful and visibly insufficient, richer retrieval can be justified later from traces instead of speculation.
