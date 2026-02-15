# Writer Versioned Document UX: Implementation Runbook

**Date:** 2026-02-15  
**Status:** Execution-ready  
**Owner:** Next coding session  
**Scope:** Writer UX and run-writer data model for versioned revisions + overlay comments/proposals

## Narrative Summary (1-minute read)

The current writer UX mixes live patches and direct text input in one mutable buffer. This causes typing interference, proposal clutter, and poor readability in edit mode. The fix is architectural: treat the document as an immutable vector of versions, and treat user/worker updates as overlays attached to a specific base version.

In this model:
- Writer creates new versions (`v0..vn`) instead of rewriting in place.
- User edits and worker proposals are diff overlays on one base version.
- Overlays are gray and ephemeral; once a new revision is produced, prior overlays are hidden by default.
- `Prompt` uses user-authored diff intent (including deletions) against the current base version.
- `<` and `>` navigate versions deterministically.

This runbook executes that cutover in safe slices without deterministic fallback hacks.

## What Changed

- Reframed writer from mutable single-buffer document to immutable version history + overlays.
- Replaced proposal-block-in-markdown behavior with typed overlay state.
- Defined prompt semantics as diff intent against a base version (not ad-hoc side prompt text box).
- Defined concurrency behavior: writer rewrites immediately, can dispatch workers concurrently, and rewrites again when outcomes arrive.

## What To Do Next

1. Land Slice A (data model and serialization): versions + overlays in `RunWriter`.
2. Land Slice B (message/API contract): prompt/overlay/version operations via typed actor/API messages.
3. Land Slice C (writer actor flow): new-version synthesis + async delegation completion loop.
4. Land Slice D (UI): version nav `< >`, overlay rendering, edit-buffer isolation from live patches.
5. Land Slice E (tests and migration): deterministic replay and acceptance gates.

---

## Product Contract (Authoritative)

### Core entities

1. `DocumentVersion`
- `version_id: u64` (monotonic per run)
- `created_at: DateTime<Utc>`
- `source: writer|user_save|system`
- `content: String` (full canonical markdown body)
- `parent_version_id: Option<u64>`

2. `Overlay`
- `overlay_id: String` (ULID)
- `base_version_id: u64`
- `author: user|researcher|terminal|writer`
- `kind: comment|proposal|worker_completion`
- `diff_ops: Vec<PatchOp>` (insert/delete/replace; deletions allowed)
- `status: pending|superseded|applied|discarded`
- `created_at: DateTime<Utc>`

3. `RunDocument`
- `objective: String`
- `versions: Vec<DocumentVersion>`
- `overlays: Vec<Overlay>`
- `head_version_id: u64`

### UX semantics

- Writer opens on `head_version_id` by default.
- `<` moves to previous version; `>` moves to next version.
- Gray overlays render on the selected version when `overlay.base_version_id == selected_version_id` and status is `pending`.
- Deletions render as gray strikethrough.
- Prompt on edited content generates overlay diff from `base_version_id` to current edit buffer.
- Writer synthesis creates a new canonical `DocumentVersion` and advances head.
- After new version creation, older pending overlays are marked `superseded` unless explicitly reapplied.

## Non-Negotiable Runtime Rules

1. No control-flow via EventStore. Actor messages only.
2. EventStore still captures all state transitions for tracing/observability.
3. Writer actor remains event-driven; no long blocking loops.
4. Worker delegations are async from writer (dispatch now, completion message later).
5. Conductor remains orchestration-only; no app-level writer UX logic leaks into conductor runtime.

## Slice A: RunWriter Versioned State

### Goal

Introduce immutable version history + overlays in run-writer state and persistence.

### Files

- `sandbox/src/actors/run_writer/state.rs`
- `sandbox/src/actors/run_writer/messages.rs`
- `sandbox/src/actors/run_writer/mod.rs`

### Changes

1. Replace section-first canonical content mutation with:
- `versions` vector
- `head_version_id`
- `overlays` list

2. Add messages:
- `GetVersion { version_id }`
- `GetHeadVersion`
- `ListVersions`
- `CreateVersion { parent_version_id, content, source }`
- `CreateOverlay { base_version_id, author, kind, diff_ops }`
- `ResolveOverlay { overlay_id, status }`

3. Keep existing patch event emission but include:
- `base_version_id`
- `target_version_id` (if new version)
- `overlay_id` (if overlay event)

4. Persist document snapshots atomically as before.

### Acceptance gates

- Head revision monotonic and deterministic.
- Able to fetch any version by id.
- Overlay lifecycle transitions are persisted and queryable.

## Slice B: API + Conductor Route for Prompt/Version Ops

### Goal

Expose typed endpoints for prompt submission and version navigation without bypassing actor contracts.

### Files

- `sandbox/src/api/writer.rs`
- `sandbox/src/api/mod.rs`
- `sandbox/src/actors/conductor/protocol.rs`
- `sandbox/src/actors/conductor/actor.rs`

### Changes

1. Keep prompt route:
- `POST /writer/prompt` with `{ path, prompt_diff, base_version_id }`

2. Add version routes:
- `GET /writer/versions?path=...`
- `GET /writer/version?path=...&version_id=...`
- optional: `POST /writer/save-version` for explicit user save snapshots

3. Extend conductor message contract for user prompt forwarding to writer inbox with typed payload metadata (`base_version_id`, `overlay_id`).

### Acceptance gates

- Prompt request rejected if `base_version_id` is stale/invalid.
- Version list and version fetch return stable ordered results.

## Slice C: Writer Actor Revision Loop (Event-Driven)

### Goal

Writer synthesizes new versions and can concurrently dispatch workers.

### Files

- `sandbox/src/actors/writer/mod.rs`

### Changes

1. Writer inbox items include:
- source
- base version id
- overlay id
- content/diff intent

2. For user prompt:
- create user overlay immediately (gray)
- synthesize new canonical version immediately from current context
- optionally dispatch async worker tasks if planner says more evidence is needed
- on worker completion message, add worker overlay and synthesize another new version

3. No blocking on worker completion during first rewrite.

4. Replace section-canon rewrite calls with `CreateVersion` calls in run-writer.

### Acceptance gates

- First rewrite starts without waiting for worker completion.
- Worker completion causes a second rewrite as a new version.
- No append-only proposal blocks in canonical markdown output.

## Slice D: Writer UI (Version Navigation + Overlay Rendering)

### Goal

Stable edit UX with version cursor and overlay rendering, no typing interference from live updates.

### Files

- `dioxus-desktop/src/components/writer.rs`
- `dioxus-desktop/src/api.rs`
- `dioxus-desktop/src/desktop/state.rs`

### Changes

1. Add version controls in title bar:
- `<` previous version
- `>` next version
- label: `v{current} of {total}`

2. Decouple editor buffer from live patches:
- when user is typing, do not mutate editor text from websocket patch stream
- show `New version available` banner instead
- user can jump/apply explicitly

3. Render overlays (gray) on selected base version only.

4. Prompt button semantics:
- derive diff between edit buffer and selected base version
- submit typed `prompt_diff` (with deletions)
- do not require separate prompt text input

5. Save semantics:
- Save creates a new user-sourced canonical version from current editor state

### Acceptance gates

- No cursor jumps or text clobber while typing.
- Overlays are visible only on matching base version.
- `/simplify to 3 bullet points` style prompt yields rewrite in a new version with large deletions allowed.

## Slice E: Migration + Tests

### Goal

Backfill old runs and lock behavior with integration tests.

### Files

- `sandbox/tests/run_writer_contract_test.rs`
- `sandbox/tests/conductor_api_test.rs`
- `sandbox/tests/e2e_conductor_scenarios.rs`
- `dioxus-desktop` component tests where available

### Changes

1. Migration behavior for legacy flat docs:
- old content imported as `v1` canonical
- old proposal blocks imported as `pending overlays` on `v1`

2. Tests:
- version creation monotonicity
- overlay status transitions
- prompt diff includes delete operations
- async delegate then follow-up rewrite sequence
- UI version cursor behavior and non-interference while typing

### Acceptance gates

- Deterministic replay of version/overlay sequence from persisted state.
- End-to-end prompt -> overlay -> rewrite -> worker completion -> rewrite flow is green.

## Suggested Execution Commands

```bash
cargo fmt --all
cargo check -p sandbox
cargo check --manifest-path dioxus-desktop/Cargo.toml
./scripts/sandbox-test.sh --lib run_writer
./scripts/sandbox-test.sh --lib writer
./scripts/sandbox-test.sh --test conductor_api_test
```

## Rollout and Risk Control

1. Feature flag first:
- `WRITER_VERSION_VECTOR_ENABLED=1`
- keep fallback read compatibility for legacy docs only during migration window

2. Rollout order:
- backend slices A/B/C first
- UI slice D second
- migration/test slice E last

3. Regression tripwires:
- writer status stuck `Running` after completed worker
- live patch stream mutates active typing buffer
- overlay leakage across versions

## Done Criteria

1. Writer document is a version vector with deterministic navigation.
2. User edits are prompt diffs anchored to a base version, including deletions.
3. Worker/user proposals are gray overlays, not canonical markdown clutter.
4. Writer rewrites are new versions, not in-place append behavior.
5. EventStore remains trace-only for control flow.
