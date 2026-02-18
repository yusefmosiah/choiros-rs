# ChoirOS Codesign Runbook

Date: 2026-02-17
Status: Living document — update as phases complete
Supersedes: `2026-02-17-codesign-sketch-and-questions.md` (Gate 0 questions resolved)

## Narrative Summary (1-minute read)

This runbook captures the full co-design of three tightly coupled spines — RLM control
flow, RuVector memory substrate, and Marginalia observation UX — alongside the `.qwy`
document format, the citation infrastructure, and the NixOS deployment architecture.

It is organized as a phased execution plan. Each phase has a gate: a set of verifiable
conditions that must be true before the next phase starts. The phases are sequenced by
dependency, not by size. Types before behavior. Local memory before global. Deployment
before publishing. Refactoring before everything.

The document is intentionally comprehensive. It is the single authoritative source for
what we are building, in what order, and why.

## What Changed (from the sketch)

1. All Gate 0 questions from `2026-02-17-codesign-sketch-and-questions.md` are now resolved.
2. Citation model fully defined: researcher proposes, writer confirms, incentive structure explicit.
3. `.qwy` document format researched and specified.
4. Four local embedding collections defined; global store unified.
5. Nine codebase seams identified and assigned to Phase 0.
6. Eight execution phases defined with gates.
7. Marginalia pulled forward to Phase 1 (safe UI, no infrastructure dependency).
8. WriterActor ephemeral model decided; WriterSupervisor registry pattern defined.
9. SubharnessActor pattern defined for conductor multistep work.

## What To Do Next

Start Phase 0. Fix the nine seams. Do not begin Phase 1 until Phase 0 gate passes.

## Handoff Status (2026-02-18)

This checkpoint records practical seam-closure progress and evidence captured in the current branch.

### What Landed

1. Writer delegation contract hardening
   - Writer delegation planning/execution now enforces tool contract at harness boundary.
   - Writer can only use `message_writer` and `finished` in delegation mode.
   - Writer synthesis can only use `finished`.
   - Invalid tools are rejected as contract violations (no silent fallback behavior).

2. Delegation lifecycle correctness
   - Delegated worker inflight state is cleared on worker completion signal, not on dispatch.
   - This prevents losing worker lifecycle ownership mid-run.

3. End-to-end verification
   - Playwright E2E run confirms prompt -> conductor -> writer -> researcher chain with live doc updates.
   - Run evidence captured with trace/video/screenshot artifacts under ignored Playwright artifact paths.

### Phase 0 Seam Review (Current)

- 0.2 WriterActor ephemeral + WriterSupervisor: in progress; writer-per-run behavior is active in current flow, final concurrency gate coverage still needed.
- 0.3 ResearcherActor concurrent dispatch: in progress; writer-owned delegation path works, dedicated concurrent stress gate still pending.
- 0.7 Worker dispatch fire-and-forget: materially improved in writer delegation path; completion arrives asynchronously by actor message.
- 0.8 EventType::UserInput coverage: active on conductor/writer entry paths in current flow; retain as explicit regression test target.

### What To Do Next (Phase 0 Closure)

1. Add deterministic tests for n concurrent writer runs (run/window isolation + no shared mutable state).
2. Add regression tests that assert writer delegation rejects non-contract tools.
3. Add ordered websocket/event assertions for async worker completion -> writer wake -> revision application.

---

## Fixed Commitments (Contract-Authoritative)

1. Filesystem artifacts are canonical truth for build/runtime outcomes.
2. All artifacts are indexed in RuVector for retrieval — local first, global on publish.
3. RLM chooses topology dynamically; deterministic rails remain for safety/operability only.
4. Context is composed per turn from callable retrieval APIs, not from long chat append.
5. Marginalia consumes semantic changes and provenance — not raw chat stream.
6. Types are immutable records. No update semantics. Correlations are learned by the
   vector index over time, not baked into the schema.
7. Citation is the ground truth quality signal. Citation count over confirmed entries is
   the retrieval weight for the vector index.
8. External content is public by default on confirmed citation. Content hash is the
   deduplication key. Hash drift = new record, not an update.
9. Global RuVector runs in the hypervisor. Local RuVector runs in the sandbox.
   Publishing is opt-in per version. Global store is enabled after auth and API proxying.
10. WriterActor is ephemeral, spawned per document run. WriterSupervisor is the registry.
11. SubharnessActor is a conductor-scoped lambda actor: spawned on demand, runs to
    completion, sends a typed message back to conductor, then stops.
12. Conductor remains semi-single-shot: brief RLM harness turn, memory-managed context,
    fast decisions. Subharnesses carry the duration of multistep work.
13. Conductor delegates only to app agents (Writer, future HarnessedActor). Worker
    delegation (Researcher/Terminal) is exclusively Writer-owned.

---

## Three-Spine Architecture

### Spine 1: RLM Control Flow

The RLM (Recursive Language Model) is the default execution mode. Linear tool-looping
is a degenerate case of `NextAction::ToolCalls`. The model composes its own context
each turn via retrieval APIs. Topology (linear / parallel / recursive) is model-chosen,
bounded by deterministic safety rails.

**Conductor** gets the same RLM harness as workers, configured for brevity. Its primary
use of RLM is memory management: freeing context from old runs, composing a lean wake
context for current work. It does not re-plan mid-turn; it spawns subharnesses for
multistep conductor work.

Conductor does not route worker capabilities directly. It routes app agents. Writer is
the execution manager for worker delegation and decides when to call Researcher/Terminal.

**SubharnessActor** is the mechanism for arbitrary multistep conductor work. It is a
proper ractor actor under ConductorSupervisor, not a `tokio::spawn`. Panics become
supervision signals. Completion is a typed message back to conductor's mailbox.

**NextAction** variants (to be implemented in Phase 3):
```
ToolCalls       — linear, call one or more tools this turn
SpawnSubharness — spawn a SubharnessActor for multistep work
Delegate        — route to a worker or app agent
Complete        — objective achieved
Block           — cannot proceed, needs human or escalation
```

### Spine 2: RuVector Memory Substrate

Four local embedding collections:

```
user_inputs
  unit:     human directive text (objective or prompt diff Insert/Replace text)
  trigger:  EventType::UserInput on any surface
  value:    cross-surface intent correlation; personal history as weak prior
  note:     all surfaces in one collection — cross-app correlations are the point

version_snapshots
  unit:     whole document content at VersionSource::Writer boundary
  trigger:  harness loop completion (one per AgentHarness::run() call)
  value:    "what has been produced on similar objectives"
  note:     intermediate versions are NOT embedded — loop completion only

run_trajectories
  unit:     summary of one harness run (objective → tools → outcome)
  trigger:  AgentResult returned from harness.run()
  value:    "what approaches worked on similar tasks"

doc_trajectories
  unit:     rolled-up summary across all runs touching a document
  trigger:  updated each time a new version_snapshot is added for that path
  value:    strategic pattern recall — document arc over time
```

External content: citation graph traversal locally (no vector search). Public in global
store on confirmed citation. See External Content section below.

Global store: unified semantic search across `external_content` and published `.qwy`
snapshots. Same collection, `record_kind` field for filtering. Citation count is the
shared quality signal.

### Spine 3: Marginalia Observation UX

Marginalia is a read-only observer. It has no write authority. It consumes:
- `WriterRunPatchPayload` events from the patch stream
- `DocumentVersion` and `Overlay` types for version navigation
- Semantic changeset summaries derived from patch ops

**v1** (Phase 1): built against existing patch stream and `section_id`. Anchors are
approximate (section-level). Safe to build now.

**v2** (Phase 8): migrate anchors to `.qwy` block UUIDs. Full annotation stability
across non-trivial edits. Unblocked by `.qwy` format landing in Phase 2.

---

## The `.qwy` Document Format

### Design Principles

- Block tree with ULID node IDs — stable forever, never reassigned
- Append-only patch log — no in-place mutation
- Provenance vocabulary from W3C PROV-O — `wasGeneratedBy`, `wasAttributedTo`,
  `wasRevisionOf`, `wasQuotedFrom`, `hadPrimarySource`
- CSL-JSON citation registry — standard bibliographic format
- `chunk_hash` per block — SHA-256 of rendered text, drives selective re-embedding
- Dual encoding: canonical CBOR internal, canonical JSON human-readable projection,
  Markdown as derived render
- ChoirOS-specific provenance predicates: `qwy:conductorRunId`, `qwy:loopId`,
  `qwy:workerTurnId`

### Structure

```
document
├── header
│   ├── document_id         ULID  (stable forever)
│   ├── schema_version      u32   (additive only — never remove or reorder fields)
│   ├── created_at          DateTime
│   ├── created_by          agent_id
│   └── conductor_run_id    ULID? (nullable)
│
├── block_tree
│   └── block
│       ├── block_id        ULID  (stable, never reassigned)
│       ├── block_type      enum  (paragraph | heading | code | embed | citation_anchor)
│       ├── parent_id       ULID? (null = root)
│       ├── children        [ULID] (ordered)
│       ├── content         string (plain text — atjson style)
│       ├── chunk_hash      SHA-256 (of rendered content — embedding cache key)
│       ├── provenance
│       │   ├── wasGeneratedBy    activity_id
│       │   ├── wasAttributedTo   agent_id
│       │   ├── wasRevisionOf     block_id?
│       │   ├── hadPrimarySource  source_ref?
│       │   └── conductor_run_id  ULID?
│       └── annotations     [{type, start, end, attrs}]
│           (citation anchors, highlights, comments — atjson style)
│
├── patch_log               (append-only)
│   └── patch_entry
│       ├── patch_id        ULID
│       ├── tx_id           ULID
│       ├── timestamp       DateTime
│       ├── author          agent_id
│       ├── run_id          ULID?
│       ├── loop_id         ULID?
│       └── ops             [{action, path: [block_id, ...], value}]
│
├── citation_registry
│   └── citation_id -> CitationRecord
│       ├── citation_id     ULID
│       ├── cite_kind       enum (internal_block | external_url | published_qwy)
│       ├── target          block_id | url | document_id
│       ├── url             string?   (external + published qwy)
│       ├── content_hash    SHA-256?  (external content at fetch time)
│       ├── accessed_at     DateTime? (external content)
│       ├── snapshot_ref    string?   (local disk path — private, not in .qwy file)
│       └── csl_metadata    CSL-JSON? (if bibliographic metadata extractable)
│
└── version_index
    └── [{snapshot_hash, tx_id, timestamp, author}]
```

### URL as First-Class

Three distinct URL roles:

1. **Source URL** — `prov:hadPrimarySource` on a block. Where content came from.
   Carried by the researcher at ingestion time.

2. **Citation URL** — annotation on a block with `type: citation_anchor`. Points to
   a `CitationRecord` in the registry. The record carries the URL and CSL metadata.

3. **Embed URL** — `block_type: embed` with a `url` field. External content rendered
   inline. Carries `content_hash` at embed time for staleness detection.

### Staleness Rule

A URL whose content changes (hash drift) is a new `CitationRecord`, not an update to
the existing one. Old citations point to old hashes. The citation is to the content as
it existed at confirmation time, not to the URL as a mutable pointer.

---

## Citation Infrastructure

### Model

Researcher proposes. Writer confirms. This is a game with opposing incentives:

- **Researcher optimizes up** — maximize citation proposals. Reward exploration breadth.
  A researcher that only proposes certain citations is playing too safe.
- **Writer optimizes down** — be selective. Confirm only what earned its place.
  A writer that confirms everything is not exercising editorial judgment.

The healthy equilibrium: high proposal volume, moderate confirmation rate. The confirmed
citations represent genuine editorial selection from a genuinely exploratory retrieval pass.

The researcher's training signal is all its proposals plus their outcomes. The writer's
training signal is all confirmed + rejected decisions. Both are captured in the schema
without additional event types.

### BAML Types

```
class Citation {
  cited_id       string    // artifact path, version_id, URL, block_id, or input_id
  cite_kind      CitationKind
  confidence     float
  excerpt        string?   // specific span that triggered this
  rationale      string    // why this was relevant
}

enum CitationKind {
  RetrievedContext    // researcher pulled it into context
  InlineReference     // appears as a link/reference in document text
  BuildsOn            // this run extends or revises the cited artifact
  Contradicts         // explicitly disputes prior artifact
  Reissues            // restates a prior objective or directive
}
```

`ResearchResult` returns `citations: Vec<Citation>`. `WriterRunPatchPayload` carries
`citations: Vec<Citation>` as an optional field.

### Citation Record Schema

```
CitationRecord
  citation_id     ULID
  cited_id        string         (artifact path | version_id | input_id | URL | block_id)
  cited_kind      string         (version_snapshot | user_input | external_content |
                                  qwy_block | external_url)
  citing_run_id   ULID
  citing_loop_id  ULID
  citing_actor    string         (researcher | writer | terminal | user)
  cite_kind       CitationKind
  confidence      float
  excerpt         string?
  rationale       string
  status          enum           (proposed | confirmed | rejected | superseded)
  proposed_by     string         (researcher | writer | user)
  confirmed_by    string?        (writer | user | null)
  confirmed_at    DateTime?
  created_at      DateTime
```

### External Content

```
ExternalContent (local — private)
  content_id      ULID
  url             string
  content_hash    SHA-256
  fetched_at      DateTime
  fetched_by      loop_id
  run_id          ULID?
  title           string?
  content_text    string         (embeddable text — extracted and cleaned)
  chunk_strategy  enum           (full | sections | paragraphs)
  snapshot_ref    string?        (local disk path)
  domain          string?
  csl_metadata    CSL-JSON?

GlobalExternalContent (public — in hypervisor global store)
  content_id      content_hash   (natural deduplication key)
  url             string
  title           string?
  content_text    string
  chunk_strategy  enum
  csl_metadata    CSL-JSON?
  first_cited_at  DateTime
  citation_count  u32
  domain          string?
  record_kind     "external_content"
  [no fetched_by, run_id, snapshot_ref — stripped at publish]
```

Publish trigger: confirmed citation on external content → global record created or
citation_count incremented if content_hash already exists.

---

## Actor Topology (Target State)

### Supervision Tree

```
ApplicationSupervisor
└── SessionSupervisor (per session)
    ├── ConductorSupervisor
    │   ├── ConductorActor          (one per session)
    │   └── SubharnessActor         (ephemeral, spawned per conductor request)
    ├── WriterSupervisor            (NEW — registry for ephemeral writers)
    │   └── WriterActor             (ephemeral, one per open document run)
    ├── ResearcherSupervisor        (NEW — registry for concurrent research)
    │   └── ResearcherActor         (ephemeral per task, or concurrent dispatch)
    ├── TerminalSupervisor
    │   └── TerminalActor
    └── DesktopSupervisor
        └── DesktopActor
```

### Key Changes from Current State

**WriterActor: singleton → ephemeral**
- Spawned per document run (identified by `run_id`)
- WriterSupervisor is the registry: `run_id → ActorRef<WriterMsg>`
- Conductor resolves writer ref via WriterSupervisor, not a stored singleton
- Closes when the run completes or window closes

**ConductorActor: singleton actor refs → registry lookups**
- Remove `researcher_actor`, `terminal_actor`, `writer_actor` from `ConductorState`
- Remove `SyncDependencies` message (workaround for the singleton problem)
- Conductor resolves worker refs from supervisors at dispatch time

**Conductor document proxy messages: removed**
- `ListWriterDocumentVersions`, `GetWriterDocumentVersion`, `ListWriterDocumentOverlays`,
  `CreateWriterDocumentVersion` removed from `ConductorMsg`
- These route directly to WriterSupervisor or the specific WriterActor by `run_id`

**Worker dispatch: `tokio::spawn` + `ractor::call!` → supervised actors**
- `spawn_capability_call` in `decision.rs` currently uses `tokio::spawn` + blocking RPC
- Replace with supervised actor spawn under the appropriate supervisor
- Completion arrives as a typed message in conductor's mailbox
- Panics become supervision signals, not silent hangs

**SubharnessActor: new**
```
SubharnessActor
  spawned by:   ConductorActor (on demand)
  lifetime:     single objective — spawns, runs, sends completion, stops
  messages in:  SubharnessMsg::Execute {
                  objective, context, correlation_id, reply_to: ActorRef<ConductorMsg>
                }
  messages out: ConductorMsg::SubharnessComplete { correlation_id, result, citations }
                ConductorMsg::SubharnessFailed  { correlation_id, reason }
  supervision:  under ConductorSupervisor
  profile:      HarnessProfile::Subharness (medium steps, scoped context)
```

**CapabilityWorkerOutput: open for extension**
```rust
pub enum CapabilityWorkerOutput {
    Researcher(ResearcherResult),
    Terminal(TerminalAgentResult),
    Writer(WriterCompletionResult),    // new
    Subharness(SubharnessResult),      // new
}
```

**EventType::UserInput: emit at all entry points**
- `POST /conductor/execute` — emit on `ExecuteTask` receipt
- `POST /writer/prompt` — emit on `WriterSource::User` envelope enqueue
- Any future user-facing input surface
- Single ingestion subscriber for `user_inputs` collection

**Run state durability**
- `ConductorRunState` currently lives in-memory only (`HashMap<run_id, ConductorRunState>`)
- Needs a durable projection path from the event store for conductor wake context
- Required before RLM harness can free and rehydrate run memories

### HarnessProfile

```rust
pub enum HarnessProfile {
    Conductor,    // max_steps: low, context: memory-managed, output: typed action
    Worker,       // max_steps: high, context: full, output: result + findings + citations
    Subharness,   // max_steps: medium, context: scoped to objective, output: typed completion
}
```

---

## Memory Planes

```
Working plane (RAM)
  Turn-local and branch-local memory.
  Discardable. Not authoritative.
  Current: implicit in actor state (ConductorState, AgentHarness conversation vec).
  Target:  explicit ContextSnapshot type composed per turn via retrieval APIs.

Episodic plane (RuVector — local sandbox)
  Run/session trajectories, decisions, outcomes, quality.
  Retrieval substrate for planning and strategy recall.
  Collections: user_inputs, version_snapshots, run_trajectories, doc_trajectories.

Artifact plane (files + RuVector mirrors)
  Files are source of truth (.qwy documents, code, reports).
  RuVector stores searchable representations plus metadata.
  Retrieved artifacts resolve back to file refs and content hashes.

Global plane (RuVector — hypervisor)
  Published .qwy version snapshots + external content.
  Unified semantic search. Citation count as quality signal.
  Enabled after auth and API proxying.
```

---

## Phased Execution Plan

### Phase 0 — Refactor (keep all features, fix seams)

Goal: eliminate the nine identified codebase seams without adding new behavior.
All existing features must continue to pass their tests after each change.

Seams to fix (in dependency order):

**0.1 Worker supervision**
- Replace `tokio::spawn` in `conductor/runtime/decision.rs:177` with supervised
  ractor actor spawning under ConductorSupervisor
- Panics must arrive as supervision signals, not silent timeouts
- Gate: capability call failures produce a `ConductorMsg` within bounded time

**0.2 WriterActor ephemeral + WriterSupervisor**
- WriterSupervisor spawned under SessionSupervisor
- WriterActor spawned per `run_id` on first use, registered in supervisor
- WriterActor stops on run completion or window close
- Gate: n concurrent writer runs produce n independent WriterActor instances
  with no shared mutable state

**0.3 ResearcherActor concurrent dispatch**
- ResearcherSupervisor spawned under SessionSupervisor
- ResearcherActor ephemeral per task, or concurrent dispatch supported
- Gate: two concurrent research tasks do not contend on a single actor

**0.4 Conductor singleton refs → registry lookups**
- Remove `researcher_actor`, `terminal_actor`, `writer_actor` from `ConductorState`
  (`conductor/actor.rs:43-44`)
- Remove `SyncDependencies` from `ConductorMsg` (`protocol.rs:28-32`)
- Conductor resolves refs from supervisors at dispatch time
- Gate: conductor dispatches correctly without pre-injected actor refs

**0.5 Conductor document proxy messages removed**
- Remove `ListWriterDocumentVersions`, `GetWriterDocumentVersion`,
  `ListWriterDocumentOverlays`, `CreateWriterDocumentVersion` from `ConductorMsg`
- Route these directly to WriterSupervisor
- Update API handlers accordingly
- Gate: writer document API calls succeed without routing through conductor

**0.6 CapabilityWorkerOutput open for extension**
- Add `Writer` and `Subharness` variants (stubs, not yet implemented)
- Gate: enum compiles; existing Researcher and Terminal paths unaffected

**0.7 Worker dispatch: fire-and-forget with typed completion**
- Replace `ractor::call!` blocking RPC in `workers.rs` with send + await completion
  message in conductor mailbox
- `CapabilityCallFinished` is already the right message; the dispatch path needs
  to not block on the call
- Gate: conductor turn returns immediately after dispatch; completion arrives
  asynchronously as a message

**0.8 EventType::UserInput emitted at all entry points**
- Emit `EventType::UserInput` at `POST /conductor/execute`
- Emit `EventType::UserInput` at writer prompt enqueue (`WriterSource::User`)
- Gate: event store contains `user_input` events for both surfaces under test

**0.9 libsql → sqlx migration (URGENT — unblocks Phase 6 Nix/cross-compilation)**
- Replace `libsql` dependency with `sqlx` (already in workspace) in `sandbox/Cargo.toml`
- Remove manual `run_migrations()` with `PRAGMA table_info` introspection in
  `actors/event_store.rs`; replace with `sqlx::migrate!()` macro
- Add proper migration files for `session_id` and `thread_id` columns (currently only
  added via in-code workarounds, not tracked in `migrations/`)
- Enable `RETURNING` clause in `handle_append` (currently commented out due to libsql
  limitation)
- Enable sqlx compile-time query checking (`SQLX_OFFLINE` mode for CI)
- Gate: `cargo test -p sandbox --test '*'` passes; no `libsql` dependency remains;
  `sqlx migrate run` succeeds against a fresh database

**Phase 0 Gate:**
- All existing integration tests pass
- `cargo clippy --workspace -- -D warnings` passes
- `cargo fmt --check` passes
- n concurrent writer runs work correctly
- Worker panics produce supervision signals within 5 seconds

---

### Phase 1 — Marginalia v1 (safe UI, read-only)

Goal: semantic changeset observation UX against existing patch stream.
No new backend types required. No write authority.

**1.1 Semantic changeset summarization**
- BAML function: given a `Vec<PatchOp>` and before/after content, produce a
  human-readable summary of what changed and why it matters
- Emit as a new event type: `writer.run.changeset`
- Fields: `patch_id`, `loop_id`, `summary`, `impact` (low/medium/high), `op_taxonomy`

**1.2 Version navigation UI**
- List versions for a document (API already exists)
- Navigate between versions, see diff rendered
- Show `VersionSource` (Writer / UserSave / System) as provenance indicator

**1.3 Annotation display**
- Display `Overlay` records (proposals, comments, worker completions) alongside
  the document
- Show author (`OverlayAuthor`) and status (`OverlayStatus`)
- Read-only in v1 — no new annotation creation UI yet

**1.4 Patch stream live view**
- Real-time display of `writer.run.patch` events as they arrive
- Show op taxonomy (insert / delete / replace) and source

**Phase 1 Gate:**
- Semantic changeset events appear in the event store for writer runs
- Version navigation works across at least 3 versions of a document
- Overlay display renders without layout regression
- No new backend mutation paths introduced

---

### Phase 2 — Types

Goal: define all shared type contracts before any new behavior is built on them.
No implementation beyond the type definitions and their serialization.

**2.1 `.qwy` core types**
- `BlockId` newtype over ULID
- `BlockNode` struct (block_id, block_type, parent_id, children, content, chunk_hash,
  provenance, annotations)
- `BlockType` enum (Paragraph | Heading | Code | Embed | CitationAnchor)
- `ProvenanceEnvelope` struct (wasGeneratedBy, wasAttributedTo, wasRevisionOf,
  hadPrimarySource, conductor_run_id, loop_id)
- `PatchEntry` struct (patch_id, tx_id, timestamp, author, run_id, loop_id, ops)
- `QwyPatchOp` enum ({action, path: Vec<BlockId>, value})
- `ChunkHash` newtype over [u8; 32]
- `QwyDocument` struct (header, blocks, patch_log, citation_registry, version_index)

**2.2 Citation types**
- `CitationKind` enum (RetrievedContext | InlineReference | BuildsOn | Contradicts |
  Reissues)
- `CitationStatus` enum (Proposed | Confirmed | Rejected | Superseded)
- `CitationRecord` struct (full schema from above)
- BAML `Citation` class and `CitationKind` enum in `researcher.baml` and `writer.baml`

**2.3 Embedding collection record types**
- `UserInputRecord` (input_id, content, surface, desktop_id, session_id, thread_id,
  run_id?, document_path?, base_version_id?, created_at)
- `VersionSnapshotRecord` (version_id, document_path, content, objective, loop_id,
  run_id, chunk_hash, created_at)
- `RunTrajectoryRecord` (loop_id, run_id, worker_type, objective, summary,
  steps_taken, success, created_at)
- `DocTrajectoryRecord` (document_path, version_count, run_count, last_loop_id,
  cumulative_summary, last_updated_at)
- `ExternalContentRecord` (local and global variants — schema above)

**2.4 SubharnessActor message types**
- `SubharnessMsg::Execute` (objective, context, correlation_id, reply_to)
- `ConductorMsg::SubharnessComplete` (correlation_id, result, citations)
- `ConductorMsg::SubharnessFailed` (correlation_id, reason)
- `SubharnessResult` struct

**2.5 HarnessProfile enum**
- `HarnessProfile` (Conductor | Worker | Subharness) with associated config
- Wire into `HarnessConfig`

**2.6 WriterSupervisor message types**
- `WriterSupervisorMsg::Resolve { run_id, reply }` → `ActorRef<WriterMsg>`
- `WriterSupervisorMsg::Register { run_id, actor_ref }`
- `WriterSupervisorMsg::Deregister { run_id }`

**Phase 2 Gate:**
- All new types compile in `shared-types`
- BAML types generate without error
- No runtime behavior added — types only
- `cargo test --lib` passes across workspace

---

### Phase 3 — Citations

Goal: first behavioral layer. Researcher proposes citations. Writer confirms or rejects.
Citation events flow into the event store.

**3.1 Researcher citation extraction**
- BAML `Citation` objects returned in `ResearcherResult`
- Researcher emits `CitationRecord` with `status: Proposed` into event store
  on each `citation_attach` lifecycle event
- `citing_actor: "researcher"`, `confirmed_by: null`

**3.2 Writer confirmation path**
- On overlay acceptance (user accepts a writer proposal), emit `CitationRecord`
  updates: `status: Confirmed`, `confirmed_by: "writer"`, `confirmed_at`
- On overlay rejection, emit `status: Rejected`
- Gate: confirmed citations queryable from event store by `run_id`

**3.3 UserInput ingestion**
- `EventType::UserInput` subscriber creates `UserInputRecord`
- Content extraction for writer prompts: concatenate `text` fields from
  `Insert` and `Replace` ops in `prompt_diff`; drop `Delete` and `Retain`
- Gate: `user_inputs` records created for both conductor and writer surfaces

**3.4 External content citation publish trigger**
- On confirmed citation where `cited_kind: external_content`:
  - Check global store for existing record by `content_hash`
  - If absent: create new `GlobalExternalContent` record
  - If present: increment `citation_count`
- Gate: confirmed external citation creates or increments global record

**3.5 Citation registry in `.qwy` documents**
- `CitationRecord` entries written to `citation_registry` section of `.qwy` on
  writer loop completion
- Gate: `.qwy` files contain citation registry entries for runs that produced citations

**Phase 3 Gate:**
- Researcher → writer citation flow produces confirmed records end-to-end
- External content publish trigger fires on confirmed external citation
- `user_inputs` records exist for test runs on both surfaces
- Citation events queryable from event store with correct status lifecycle

---

### Phase 4 — RLM Harness

Goal: model-managed context composition per turn. SubharnessActor implementation.
Conductor gets RLM harness with memory management.

**4.1 SubharnessActor implementation**
- Full ractor actor under ConductorSupervisor
- Runs `AgentHarness` with `HarnessProfile::Subharness`
- Sends typed `SubharnessComplete` or `SubharnessFailed` to conductor on finish
- Gate: conductor spawns subharness, receives completion message, run continues

**4.2 NextAction enum expansion**
- Add `SpawnSubharness`, `Delegate`, variants
- Conductor BAML functions updated to return expanded NextAction
- Gate: conductor can choose SpawnSubharness and correctly spawn + await

**4.3 Conductor RLM harness turn**
- Conductor runs brief `AgentHarness` with `HarnessProfile::Conductor`
- Context: bounded agent-tree snapshot + recent run state
- Memory management: conductor can mark old run states for eviction
- Gate: conductor turn is measurably faster than worker turns (step budget enforced)

**4.4 Run state durability**
- `ConductorRunState` projected from event store on actor restart
- Conductor wake rehydrates from projection, not only in-memory state
- Gate: conductor restart with active runs does not lose run state

**4.5 ContextSnapshot type**
- Per-turn context composition: retrieved from MemoryActor (stub in this phase),
  selected documents, working memory
- Provenance fields on every context item — no opaque items allowed
- Gate: ContextSnapshot carries provenance for all items; stub MemoryActor compiles

**Phase 4 Gate:**
- SubharnessActor spawns, runs, returns typed completion to conductor
- Conductor wake-context reconstruction from event store works
- HarnessProfile::Conductor enforces step budget
- All existing capability dispatch still works

---

### Phase 5 — Local Vector Memory

Goal: local vector memory operational. Retrieval APIs available to RLM.

Vector backend decision: **sqlite-vec** (not RuVector/rvf-runtime).
Rationale: sqlite-vec runs in-process alongside the existing SQLite DB, is maintained by
the sqlite team, is stable and proven, and covers all Phase 5 needs (four collections,
HNSW-style ANN search, chunk_hash dedup). RuVector (`rvf-runtime`/`rvf-index`) was
evaluated and deferred — it was published 4 days before this decision with 57 total
downloads, and its differentiating feature (SONA learning) is already explicitly deferred
to Phase 5+. MemoryActor is the abstraction boundary; the backend is swappable without
changing the RLM-facing API.

**5.1 Dependencies**
- Add `sqlite-vec` (rusqlite, bundled), `fastembed` (wraps `ort`, AllMiniLML6V2),
  `sha2`, `hex`, `zerocopy` to `sandbox/Cargo.toml`
- Decision: `fastembed` preferred over bare `ort` — handles tokenizer, session, and
  model download in one crate; `ort` is its own dep so the plan intent is satisfied.
  Offline/test mode: `CHOIROS_MEMORY_STUB=1` activates hash-based stub vectors.
- Gate: crates compile; `Embedder::init()` succeeds (real or stub); sqlite-vec
  extension loads and `vec0` virtual tables are created at runtime

**5.2 MemoryActor**
- Spawned under SessionSupervisor
- Manages local sqlite-vec virtual tables via four collections:
  episodic, artifacts, trajectories, citations
- Ingestion: subscribes to `EventType::UserInput`, `VersionSource::Writer` events,
  `AgentResult` completions
- `chunk_hash` check: skip re-embedding if hash matches cached value
- Gate: ingestion pipeline creates embedding records for test runs

**5.3 Retrieval APIs**
- `artifact_search(query, filters)` → top-k candidates
- `artifact_expand(hit_ids, expansion_mode)` → neighbor artifacts, related episodes,
  semantic-change neighbors, dependency/provenance edges
- `artifact_context_pack(objective, token_budget)` → structured ContextSnapshot
  with rationale and confidence; deterministic for same inputs
- Gate: retrieval APIs return results for seeded test corpus

**5.4 RLM context composition wired**
- `ContextSnapshot` populated from MemoryActor on each conductor and worker turn
- Citation count used as retrieval weight
- Gate: model receives context with provenance fields populated

**5.5 Selective re-embedding**
- `chunk_hash` comparison before embedding call
- Only changed blocks re-embedded on document update
- Gate: second version of a document with one changed block produces one
  embedding call, not a full re-embed

**Phase 5 Gate:**
- End-to-end: user objective → conductor turn → worker with retrieved context →
  writer confirms citation → citation count increments in local index
- Retrieval precision measured against a fixed task corpus (baseline established)
- Token cost per turn measured and within acceptable range

---

### Phase 6 — NixOS + Deployment

Goal: reproducible builds, three-flake structure, deployed on EC2 NixOS.

**6.1 Home manager on Mac**
- `home.nix` consuming sandbox dev shell
- Reproducible dev environment: cargo, sqlx-cli, just, dx
- Gate: `nix develop` produces working dev shell on Mac

**6.2 `sandbox/flake.nix`**
- Rust workspace as a Nix package
- Dev shell with all dependencies pinned
- `nix build` produces sandbox binary
- Gate: `nix build .#sandbox` succeeds; binary runs

**6.3 `frontend/flake.nix`**
- Dioxus build as a Nix package
- `nix build` produces frontend static assets
- Gate: `nix build .#frontend` succeeds; assets serve correctly

**6.4 `hypervisor/flake.nix`**
- NixOS host configuration
- Imports sandbox as a container/service
- Declares container spec as a NixOS module
- Global RuVector service stub (not yet active)
- Gate: `nix build .#hypervisor` succeeds; sandbox runs as a container under hypervisor

**6.5 Auth + API proxying**
- Auth layer in hypervisor (not in sandbox)
- API proxy routes authenticated requests to sandbox container
- Gate: authenticated requests reach sandbox; unauthenticated requests are rejected

**6.6 EC2 NixOS deployment**
- Deploy hypervisor flake to EC2 NixOS instance
- Sandbox container running under hypervisor
- Gate: `choir-ip.com` serves from NixOS EC2; existing features work

**6.7 MicroVM preparation (deferred to later)**
- MicroVMs come after the container boundary is stable
- Declared as explicit future milestone, not part of Phase 6 gate

**Phase 6 Gate:**
- Three flakes build cleanly
- EC2 deployment serves production traffic
- Auth layer rejects unauthenticated requests
- Sandbox container lifecycle managed by hypervisor

---

### Phase 7 — Global RuVector + Publishing

Goal: global semantic search operational. User publishing enabled.

**7.1 Global RuVector service in hypervisor**
- RuVector instance in hypervisor flake
- `GlobalExternalContent` and published `.qwy` snapshot collections
- Citation count as shared quality signal across users
- Gate: global store accepts records; semantic search returns results

**7.2 User opt-in publishing**
- Per-version publish UI: user explicitly marks a `.qwy` version for publishing
- Published version snapshot → global store record
- Strip private fields (`fetched_by`, `run_id`, `snapshot_ref`) at publish boundary
- Gate: published document appears in global semantic search for other users

**7.3 External content automatic publish**
- Confirmed citation on external content → global record (deduped by `content_hash`)
- Citation count incremented on duplicate
- Gate: two users citing the same URL produce one global record with `citation_count: 2`

**7.4 Unified global search**
- Single search surface: `external_content` + published `.qwy` snapshots
- `record_kind` field for optional filtering
- Citation count as ranking signal
- Gate: global search returns mixed results ranked by relevance + citation count

**Phase 7 Gate:**
- User can publish a `.qwy` version and it appears in global search
- External content auto-publishes on confirmed citation
- Cross-user citation count increments correctly
- Privacy boundary enforced: unpublished content never appears in global search

---

### Phase 8 — Marginalia v2

Goal: migrate annotation anchors from `section_id`/byte offsets to `.qwy` block UUIDs.
Full annotation stability across non-trivial edits.

**8.1 Block-UUID annotation anchors**
- Marginalia annotations reference `block_id` (ULID), not byte offset or section string
- Annotation follows the block through reorder, edit, reparent
- Gate: annotation on a block survives 3 non-trivial document edits

**8.2 Semantic changeset schema v2**
- `op_taxonomy` + `impact_summary` + `verification_evidence` on changeset events
- Block-level granularity (which blocks changed, what kind of change)
- Gate: changeset events contain block_id references for all structural changes

**8.3 Annotation creation UI**
- Users can create annotations anchored to blocks
- Annotations stored as `Overlay` records with `block_id` anchor
- Gate: annotation survives document save and reload

**8.4 Version graph navigation**
- Visual navigation of version history with branch/merge support
- `parent_version_id` chain rendered as a graph
- Gate: user can navigate to any prior version and see its content

**Phase 8 Gate:**
- Annotation survives non-trivial document edits with correct block tracking
- Version graph renders for documents with 5+ versions
- Semantic changeset events reference block UUIDs

---

## Current Codebase Seams (Phase 0 Reference)

For each seam: file and line number of the problem, target state.

| # | Seam | Location | Target |
|---|------|----------|--------|
| 1 | Worker supervision via tokio::spawn | `conductor/runtime/decision.rs:177` | Supervised ractor actor |
| 2 | WriterActor singleton | `conductor/actor.rs:44` | Ephemeral + WriterSupervisor |
| 3 | ResearcherActor singleton | `conductor/actor.rs:43` | Per-task or concurrent |
| 4 | Conductor singleton refs | `conductor/actor.rs:42-45` | Registry lookups |
| 5 | Conductor doc proxy messages | `conductor/protocol.rs:61-86` | Remove, route direct |
| 6 | CapabilityWorkerOutput closed | `conductor/protocol.rs:100-103` | Open for extension |
| 7 | Blocking ractor::call! in workers | `conductor/workers.rs:27,56,71,91` | Fire-and-forget |
| 8 | EventType::UserInput never emitted | `actors/event_bus.rs:131` | Emit at all entry points |
| 9 | libsql bundled C fork (no RETURNING, no proper migrations, blocks cross-compilation) | `sandbox/Cargo.toml:25`, `actors/event_store.rs` | sqlx + `sqlx::migrate!()` |

---

## Gate 0 Questions — Now Resolved

From `2026-02-17-codesign-sketch-and-questions.md`. All resolved in this document.

| # | Question | Resolution |
|---|----------|------------|
| 1 | Canonical artifact unit in vector memory v1? | Whole doc at `VersionSource::Writer` (per harness loop completion) |
| 2 | Minimum metadata per artifact record? | `ProvenanceEnvelope` + `chunk_hash` + `loop_id` + `run_id` + `objective` |
| 3 | Mandatory expansion edges? | `wasRevisionOf`, `hadPrimarySource`, `citation_id` refs |
| 4 | Staleness policy on hash/version drift? | Hash drift = new record. Old citations point to old hashes. Immutable. |
| 5 | Hard token budget policies per model class? | `HarnessProfile` config — Conductor: low, Worker: high, Subharness: medium |
| 6 | `artifact_context_pack` guarantees? | Deterministic for same inputs, carries rationale + confidence, structured output |
| 7 | Which episodic records influence NextAction? | `run_trajectories`, `doc_trajectories`, `user_inputs` — via ContextSnapshot |
| 8 | Minimum semantic changeset shape for Marginalia? | `patch_id`, `loop_id`, `op_taxonomy`, `impact_summary`, `block_id` refs |
| 9 | Annotation anchor storage across edits? | `.qwy` block UUIDs (Phase 2+). Section-level in Marginalia v1 (Phase 1). |
| 10 | Acceptance threshold for replacing append-only chat? | Baseline established in Phase 5; promotion on statistically meaningful uplift |

---

## Deferred (Explicit)

- **MicroVMs** — after container boundary is stable (Phase 6 tail or post-Phase 8)
- **SONA learning** — after local vector memory is operational (Phase 5+); backend TBD (RuVector/rvf deferred pending production maturity)
- **Self-prompting** (model queries memory to construct its own prompts) — after
  retrieval APIs exist (Phase 5+)
- **Global vector search on external content locally** — external content is
  citation-graph-only locally; global search enabled in Phase 7
- **Marginalia annotation creation** — Phase 8 (v1 is read-only display)
- **PDF app** — remains deferred per existing roadmap

---

## References

- `docs/architecture/2026-02-17-codesign-sketch-and-questions.md` — original sketch, superseded
- `docs/architecture/2026-02-17-rlm-actor-network-concept.md` — RLM concept
- `docs/architecture/2026-02-16-memory-agent-architecture.md` — memory agent design
- `docs/architecture/2026-02-14-conductor-non-blocking-subagent-pillar.md`
- `docs/architecture/2026-02-14-agent-tree-snapshot-contract.md`
- `docs/architecture/roadmap-dependency-tree.md`
- `AGENTS.md` — model-led control flow hard rules
