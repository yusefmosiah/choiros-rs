# Phase 0 Closure — Handoff

Date: 2026-02-18
Branch: main
Status: Phase 0 gate passed — ready for Phase 1

## Narrative Summary (1-minute read)

Phase 0 is closed. All nine codebase seams from the codesign runbook have been
addressed. Seams 0.1–0.8 were already closed in the prior branch (bb111fa).
This session closed seam 0.9 (libsql → sqlx migration, the urgent blocker for
Phase 6 Nix cross-compilation) and added the three remaining test gates.

The codebase now builds fully with `SQLX_OFFLINE=true`, runs `sqlx migrate run`
cleanly against a fresh database, uses `RETURNING` in `handle_append`, and has a
checked offline query cache committed under `sandbox/.sqlx/`. All Phase 0 gate
tests pass. No pre-existing test failures were introduced.

## What Changed

### Seam 0.9 — libsql → sqlx migration

- `sandbox/Cargo.toml`: `libsql = "0.9"` removed; `sqlx = { workspace = true }` added
  (sqlx 0.8 with runtime-tokio, sqlite, chrono, json, migrate, uuid features — already
  declared in workspace Cargo.toml)
- `sandbox/migrations/20260131000000_events_scope_columns.sql`: new migration file
  tracking `session_id` and `thread_id` columns that were previously added via a
  manual `PRAGMA table_info` introspection loop in `run_migrations()`
- `sandbox/src/actors/event_store.rs`: full rewrite
  - `libsql::Connection` → `sqlx::SqlitePool`
  - `run_migrations()` hand-rolled function → `sqlx::migrate!("./migrations")`
  - `INSERT` + separate `SELECT` workaround → single `INSERT ... RETURNING`
  - All query sites use `sqlx::query_as!` with `seq as "seq!"` override (sqlx
    infers `seq` as `Option<i64>` from SQLite schema; the `!` suffix asserts non-null)
  - `From<libsql::Error>` impl replaced with `From<sqlx::Error>`
  - `EventStoreState { conn: Connection }` → `EventStoreState { pool: SqlitePool }`
  - WAL journal mode enabled on `SqliteConnectOptions`
- `sandbox/.sqlx/`: offline query cache generated via `cargo sqlx prepare`
  against `sqlite:/tmp/choiros-prepare.db` — committed for `SQLX_OFFLINE=true` CI

### Phase 0 test gates

New file: `sandbox/tests/phase0_gate_test.rs`

Three tests:

1. `test_concurrent_writer_run_isolation` — spawns 4 distinct `WriterActor`
   instances via `WriterSupervisor` (one per run_id), verifies each has a unique
   actor ID, independent `EnsureRunDocument` state, and independent dedup tables
   (same `message_id` sent to actor A is NOT treated as duplicate by actor B)

2. `test_writer_tool_contract_allow_lists_match_spec` — structural regression
   test using `include_str!` to embed `adapter.rs` source at compile time and
   assert the exact allow-list strings:
   - `WriterDelegationAdapter`: `["message_writer", "finished"]`
   - `WriterSynthesisAdapter`: `["finished"]`
   - Also asserts no worker tools (`bash`, `web_search`, `file_read`, etc.)
     appear in the delegation adapter's `allowed_tool_names` body

3. `test_writer_inbox_events_causal_ordering` — enqueues a Conductor-source
   inbound message (no LLM call), waits 250ms for async inbox processing,
   then queries the event store and asserts:
   - `writer.actor.inbox.enqueued` event exists with the correct `message_id`
   - `writer.actor.apply_text` (the initial proposal write) appears at or before
     the enqueued event seq — reflecting the actual design: the content write
     is synchronous within `enqueue_inbound`, the telemetry event follows

## What To Do Next

Phase 0 is closed. All gates pass. Begin Phase 1.

### Phase 1 — Marginalia v1 (safe UI, read-only)

Goal: semantic changeset observation UX against the existing patch stream.
No new backend mutation paths. No write authority.

Four sub-tasks (from runbook `2026-02-17-codesign-runbook.md`, Phase 1):

**1.1 Semantic changeset summarization**
- Add a BAML function: given a `Vec<PatchOp>` and before/after content, produce
  a human-readable summary + impact level
- Emit as `writer.run.changeset` event with fields:
  `patch_id`, `loop_id`, `summary`, `impact` (low/medium/high), `op_taxonomy`
- Wire into the writer inbox processing path after `synthesize_with_llm` applies
  a document revision

**1.2 Version navigation UI**
- API already exists (`ListWriterDocumentVersions`, `GetWriterDocumentVersion`)
- Frontend: list versions for a document, navigate between them, render diff
- Show `VersionSource` (Writer / UserSave / System) as provenance indicator

**1.3 Annotation display**
- Display `Overlay` records alongside the document
  (`ListWriterDocumentOverlays` API exists)
- Show `OverlayAuthor` and `OverlayStatus`
- Read-only in v1 — no annotation creation

**1.4 Patch stream live view**
- Real-time display of `writer.run.patch` events as they arrive over websocket
- Show op taxonomy (insert / delete / replace) and source

**Phase 1 gate:**
- `writer.run.changeset` events appear in event store for writer runs
- Version navigation works across ≥ 3 versions of a document
- Overlay display renders without layout regression
- No new backend mutation paths introduced

### Standing notes for next session

- `SQLX_OFFLINE=true` must be set for CI and local builds until a `DATABASE_URL`
  is available at build time. The `.sqlx/` cache in `sandbox/.sqlx/` covers all
  current queries.
- If new sqlx queries are added, regenerate the cache:
  ```bash
  DATABASE_URL="sqlite:/tmp/choiros-prepare.db" sqlx database create
  DATABASE_URL="sqlite:/tmp/choiros-prepare.db" sqlx migrate run
  DATABASE_URL="sqlite:/tmp/choiros-prepare.db" cargo sqlx prepare -- -p sandbox
  ```
- The three pre-existing `--lib` test failures (terminal actor live tests,
  conductor missing-workers test) are unchanged from before this session.
  They require live infrastructure and are not regressions.

## References

- `docs/architecture/2026-02-17-codesign-runbook.md` — Phase 0 seam table and
  Phase 1 spec (authoritative)
- `docs/architecture/roadmap-dependency-tree.md` — Phase 0 gate checklist
- `docs/architecture/NARRATIVE_INDEX.md` — read order for architecture docs
- `sandbox/tests/phase0_gate_test.rs` — gate tests added this session
