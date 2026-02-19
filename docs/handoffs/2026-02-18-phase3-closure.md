# Handoff: Phase 3 Complete → Phase 4 (RLM Harness)

Date: 2026-02-18
Commit: 58a965c
Status: Phase 3 fully closed. All gate tests green. Ready to start Phase 4.

## Narrative Summary (1-minute read)

Phase 3 is done. The full citation lifecycle is wired end-to-end:
researcher proposes citations during web_search tool execution, writer confirms or
rejects them after delegation completes, user inputs are recorded at both surfaces,
confirmed external URLs are published as global_external_content events, and a
`.qwy`-style citation registry is emitted on every writer loop version save. All 5
Phase 3 Playwright gate tests pass. 169/169 unit tests pass.

The next phase is Phase 4 (RLM Harness): ActorHarnessActor implementation, NextAction
enum expansion, Conductor RLM harness turn, run state durability, and ContextSnapshot.
The gate types for all Phase 4 work (ActorHarnessMsg, HarnessProfile, ActorHarnessResult)
were already defined in Phase 2 — Phase 4 is implementation only, no new types.

## What Changed (this session)

### Phase 3 implementation (all in commit 58a965c)

**3.1 — researcher citation emit**
- File: `sandbox/src/actors/researcher/mod.rs`
- `emit_citation_proposed_events` called at end of `run_with_harness`
- Harvests all `web_search` tool outputs from the harness `AgentResult`
- Emits one `citation.proposed` event per URL with:
  - `citation_id` (new ULID), `citing_run_id`, `citing_actor: "researcher"`
  - `cited_kind: "external_url"`, `cited_id` (URL), `confidence`, `excerpt`
  - `status: "proposed"`, `proposed_by: "researcher"`
- Returns `Vec<ProposedCitationStub>` (new type in shared-types) upstream

**3.2 — writer citation confirmation**
- File: `sandbox/src/actors/writer/mod.rs`
- `emit_citation_confirmation_events` called in `handle_delegation_worker_completed`
- On worker success: emits `citation.confirmed` per stub with `confirmed_by: "writer"`, `confirmed_at`
- On worker failure: emits `citation.rejected` per stub with `rejected_by: "writer"`, `rejected_at`

**3.3 — UserInput records**
- `sandbox/src/api/conductor.rs`: emits `user_input` at `POST /conductor/execute`
  - payload includes `surface: "conductor.execute"`, `record: UserInputRecord`
- `sandbox/src/api/writer.rs`: emits `user_input` at `POST /writer/prompt`
  - payload includes `surface: "writer.prompt_document"`, `run_id`, `record: UserInputRecord`
- `sandbox/src/actors/writer/mod.rs`: emits `user_input` at `WriterMsg::SubmitUserPrompt`
  - payload includes `surface: "writer.submit_user_prompt"`, `run_id`, `record: UserInputRecord`

**3.4 — external content publish**
- File: `sandbox/src/actors/writer/mod.rs`
- `emit_global_external_content_upsert` called on successful delegation completion
- For each confirmed external URL stub, emits `global_external_content.upsert`:
  - `cited_kind`, `cited_id`, `content_hash` (sha256 of cited_id), `citing_run_id`, `action: "upsert"`

**3.5 — qwy.citation_registry**
- File: `sandbox/src/actors/writer/mod.rs`, `set_section_content_internal`
- `confirmed_citations_by_run_id: HashMap<String, Vec<ProposedCitationStub>>` in `WriterState`
- Stubs accumulated in `handle_delegation_worker_completed` (after confirmed)
- On writer version save (when `is_writer_source`), emits `qwy.citation_registry`:
  - `run_id`, `version_id`, `citation_registry: [{citation_id, cited_kind, cited_id}, ...]`

**Playwright gate tests**
- File: `tests/playwright/phase3-citations.spec.ts`
- 5 tests, all API-only (no browser), all passing in ~7s:
  - `3.3a`: conductor user_input record has `record.input_id`, `record.surface: "conductor"`
  - `3.3b`: writer user_input record has `record.input_id`, `record.surface: "writer"`
  - `3.1+3.2+3.4`: citation lifecycle (checks existing DB events first, triggers fresh run only if none)
  - `3.5`: qwy.citation_registry (checks existing DB events first, triggers fresh run only if none)
  - smoke: all six citation topic constants are queryable via `/logs/events`

### Fixes made to the test spec during this session

- `3.3b` versions endpoint: use `path=conductor/runs/{run_id}/draft.md` not `run_id=`
- `3.3b` prompt endpoint: use `/writer/prompt` not `/writer/prompt_document`
- `3.3b` PatchOp format: `{ "op": "insert", "pos": 0, "text": "..." }` (tagged enum, snake_case)
- `3.5` strategy: check for ANY existing `qwy.citation_registry` event in DB first;
  only trigger a fresh conductor run if the DB has none. This avoids a 3-minute wait
  when prior confirmed-citation runs predate the registry fix.

## Current State

### What is working

- Full citation event chain: `citation.proposed` → `citation.confirmed` → `global_external_content.upsert` → `qwy.citation_registry`
- UserInput records on conductor and writer HTTP surfaces
- All 5 Phase 3 Playwright gate tests passing
- All 5 Phase 1 Playwright gate tests still passing
- 169/169 unit tests passing
- Sandbox binary compiles clean with `SQLX_OFFLINE=true`

### Key constants / topics

```rust
// shared-types/src/lib.rs
EVENT_TOPIC_CITATION_PROPOSED         = "citation.proposed"
EVENT_TOPIC_CITATION_CONFIRMED        = "citation.confirmed"
EVENT_TOPIC_CITATION_REJECTED         = "citation.rejected"
EVENT_TOPIC_USER_INPUT                = "user_input"
EVENT_TOPIC_GLOBAL_EXTERNAL_CONTENT   = "global_external_content"
EVENT_TOPIC_QWY_CITATION_REGISTRY     = "qwy.citation_registry"
```

### Key files changed in Phase 3

```
sandbox/src/actors/researcher/mod.rs   emit_citation_proposed_events, ProposedCitationStub return
sandbox/src/actors/writer/mod.rs       emit_citation_confirmation_events, emit_global_external_content_upsert,
                                       qwy.citation_registry in set_section_content_internal,
                                       confirmed_citations_by_run_id in WriterState
sandbox/src/api/conductor.rs           UserInputRecord at POST /conductor/execute
sandbox/src/api/writer.rs              UserInputRecord at POST /writer/prompt
shared-types/src/lib.rs                ProposedCitationStub struct; all Phase 2 types present
tests/playwright/phase3-citations.spec.ts  5 gate tests (new file)
```

### Pre-existing state (not changed)

- 87 pre-existing clippy errors (none from our changes; suppressed on `mod baml_client`)
- `conductor-writer.e2e.spec.ts` requires frontend (`dx serve`) — not a CI gate
- Backend: port 8080, `DATABASE_URL=sqlite:./data/events.db`, `SQLX_OFFLINE=true`
- Frontend: port 3000 (`dx serve` in `dioxus-desktop/`)

## What To Do Next (Phase 4)

Spec: `docs/architecture/2026-02-17-codesign-runbook.md`, Phase 4 section (line 697+)

### 4.1 — ActorHarnessActor implementation

The types are already defined (Phase 2):
- `ActorHarnessMsg::Execute` in `sandbox/src/actors/conductor/protocol.rs`
- `ConductorMsg::SubharnessComplete`, `ConductorMsg::SubharnessFailed` (stub handlers)
- `ActorHarnessResult` struct

What needs to be built:
1. Full `ActorHarnessActor` — ractor actor under `ConductorSupervisor`
   - Accepts `ActorHarnessMsg::Execute { objective, context, conductor_ref, correlation_id }`
   - Runs `AgentHarness` with `HarnessProfile::Subharness`
   - On finish: sends `ConductorMsg::SubharnessComplete(ActorHarnessResult)` or `SubharnessFailed`
   - Stops itself after sending result (ephemeral)
2. Wire into `ConductorSupervisor`: spawn on `SpawnActorHarness` conductor decision
3. Stub handlers in `ConductorActor` for `SubharnessComplete` / `SubharnessFailed`
4. Gate test: conductor spawns subharness, subharness runs a trivial harness turn,
   conductor receives typed completion message

### 4.2 — NextAction enum expansion

Current `NextAction` in `sandbox/src/actors/conductor/` (BAML + Rust):
- Check what variants exist today in `baml_src/` and `ConductorDecision` in protocol.rs
- Add `SpawnActorHarness { objective: String, context: String }` variant
- Add `Delegate { target: String, objective: String }` variant (for routing to Writer)
- Update BAML `conductor_plan` function to return expanded set
- Gate: conductor can choose `SpawnActorHarness` in a test scenario

### 4.3 — Conductor RLM harness turn

- Wire `HarnessProfile::Conductor` into conductor wake
- Context input: bounded agent-tree snapshot (already partially implemented per AGENTS.md)
  + recent run state digest
- Step budget: `HarnessProfile::Conductor` should enforce a much lower `max_steps` than Worker
- Gate: conductor harness turn completes faster than a worker turn (measurable)

### 4.4 — Run state durability

- `ConductorRunState` (existing struct or new) should be projectable from event store
- On actor restart, conductor rehydrates active run list from `EventStore::GetRecentEvents`
  querying `conductor.run.*` events
- Gate: restart conductor with an in-progress run, verify run state is recovered

### 4.5 — ContextSnapshot type

- Already partially present as a concept; needs a concrete Rust type
- Fields: `objective`, `items: Vec<ContextItem>`, each item has `content`, `provenance`
- `stub_memory_actor` returns empty `ContextSnapshot` (Phase 5 fills it with real retrieval)
- Gate: ContextSnapshot type compiles; MemoryActor stub returns it; conductor receives it

### Phase 4 Gate (from runbook)

- ActorHarnessActor spawns, runs, returns typed completion to conductor
- Conductor wake-context reconstruction from event store works
- `HarnessProfile::Conductor` enforces step budget
- All existing capability dispatch still works (no regressions)

### Playwright gate for Phase 4

Write `tests/playwright/phase4-rlm-harness.spec.ts`:
- Trigger a conductor run that requires subharness delegation
- Verify `conductor.subharness.spawned` event appears
- Verify `conductor.subharness.completed` event appears with `ActorHarnessResult` payload
- Verify existing citation/marginalia events still fire (regression)

## Quick commands

```bash
# Build
SQLX_OFFLINE=true cargo build -p sandbox

# Unit tests
SQLX_OFFLINE=true cargo test -p sandbox --lib

# Phase 3 gate (fast, ~7s)
cd tests/playwright && npx playwright test phase3-citations.spec.ts --reporter=list

# Phase 1 gate
cd tests/playwright && npx playwright test phase1-marginalia.spec.ts --reporter=list

# Start server (after kill)
SQLX_OFFLINE=true DATABASE_URL=sqlite:./data/events.db ./target/debug/sandbox --port 8080 &

# Check server
curl -s http://localhost:8080/health
```

## Discovered API facts (for Phase 4 test authors)

- `/writer/ensure` — POST `{ path: "conductor/runs/{run_id}/draft.md", desktop_id, objective }`
- `/writer/versions` — GET `?path=conductor/runs/{run_id}/draft.md` (not `?run_id=`)
- `/writer/prompt` — POST, PatchOp format: `{ "op": "insert", "pos": 0, "text": "..." }`
- `/logs/events` — GET `?event_type_prefix=...&limit=N`
- `/conductor/execute` — POST `{ objective, desktop_id, output_mode: "markdown_report_to_writer" }`
