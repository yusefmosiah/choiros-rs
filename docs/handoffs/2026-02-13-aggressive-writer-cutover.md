# Aggressive Cutover Spec: Writer-First Concurrent Multiagent Runs

## Narrative Summary (1-minute read)

We are doing a hard cutover to a Writer-first runtime.

- Writer opens immediately when a run starts.
- Intermediate worker output streams into the shared run document live.
- Conductor supervises orchestration only; it does not gate user-visible progress.
- EventStore/EventBus remain the observability backbone.
- Concurrent runs are supported from day one.

This is a breaking change by design. We are removing compatibility shims, poll-based UX, and legacy paths that hide bugs.

## What Changed

- **Removed (design)**: completion-gated Writer opening and prompt-bar polling loop.
- **Removed (design)**: Run app as a separate surface.
- **Removed (design)**: `ConductorAction::UpdateDraft` loop behavior.
- **Removed (design)**: dual-prop compatibility (`path` vs `file_path`) and similar fallback behavior.
- **Added**: strict typed run-stream protocol for Writer live updates.
- **Added**: single-writer mutation authority per run (`RunWriterActor`).
- **Added**: explicit concurrency/scoping contract (`desktop_id`, `session_id`, `thread_id`, `run_id`).

## What To Do Next

1. Land Protocol Cut (Section 6.1) as a breaking API change.
2. Delete poll/run-app codepaths (Section 6.2) in the same PR.
3. Land RunWriterActor + live stream pipeline (Section 6.3/6.4).
4. Add ordered multi-run websocket integration tests (Section 7) before merge.

---

## 1) Product Decisions (Final)

1. Writer is the primary user surface for runs. No separate Run app.
2. Users must see progress before run completion.
3. Conductor does not block UI updates while workers run.
4. Multi-run concurrency is required in MVP.
5. Observability is event-sourced and always on.
6. Control flow is typed only (no string matching workflow control).

---

## 2) Hard Rules (No Defensive Debt)

1. No silent fallback keys in contracts.
2. No best-effort workflow transitions.
3. No hidden retries that alter control authority.
4. No compatibility branches for deprecated workflow states.
5. No polling loop as user-progress transport.
6. Fail fast on protocol violation; emit typed error events.

Notes:
- Retries for transport reliability are allowed only inside infrastructure layers and must be observable.
- Workflow decisions must remain deterministic and typed at actor/protocol boundaries.

---

## 3) Runtime Ownership Model

### 3.1 Actor Roles

- `ConductorActor`
  - Owns run planning, worker dispatch, completion/block decisions.
  - Never directly mutates document text.
- `RunWriterActor` (new, one per run)
  - Single mutation authority for the run document.
  - Applies structured patches/proposals.
  - Emits typed document update events with monotonic revision.
- `Worker Actors` (`Researcher`, `Terminal`, future workers)
  - Read run document snapshot.
  - Emit typed proposal/patch messages to `RunWriterActor`.
  - Never write shared run document directly.

### 3.2 Data Authority

- User-visible collaboration state: run document managed by `RunWriterActor`.
- Observability/tracing state: EventStore/EventBus.
- These are complementary, not competing systems.

---

## 4) Document Model

Path:

`conductor/runs/{run_id}/draft.md`

Structure:

```markdown
# {objective}

## Conductor
{canon text only}

## Researcher
<!-- proposal -->
{live worker proposals}

## Terminal
<!-- proposal -->
{live worker proposals}

## User
<!-- proposal -->
{unsent user directives/comments}
```

Semantics:
- Canon text: accepted synthesis.
- Proposal text: intermediate and editable.
- Deleted proposal content: rendered as gray strikethrough.

---

## 5) Protocol Cut (Breaking)

## 5.1 Conductor Execute Response

`POST /conductor/execute` must return these fields for every accepted run:

- `task_id: String`
- `run_id: String`
- `status: Queued | Running | WaitingWorker | Completed | Failed`
- `document_path: String`
- `writer_window_props: { path: String, run_id: String, preview_mode: bool, ... }`
- `correlation_id: String`

No legacy fallback key support.

## 5.2 Writer Live Event Family

Add strict websocket event variants (server -> client):

- `writer.run.started`
- `writer.run.progress`
- `writer.run.patch`
- `writer.run.status`
- `writer.run.failed`

Required fields on every event:

- `desktop_id`
- `session_id`
- `thread_id`
- `run_id`
- `document_path`
- `revision` (monotonic u64)
- `timestamp` (RFC3339)

`writer.run.patch` payload:

- `patch_id: String`
- `source: conductor | researcher | terminal | user`
- `section_id: conductor | researcher | terminal | user`
- `ops: Vec<PatchOp>` where `PatchOp = Insert | Replace | Delete`
- `proposal: bool`

## 5.3 Worker -> RunWriter Command Contract

Workers send typed commands to `RunWriterActor`:

- `ApplyPatch { run_id, source, section_id, ops, proposal }`
- `AppendLogLine { run_id, source, section_id, text, proposal }`
- `MarkSectionState { run_id, section_id, state }`

No freeform "update draft" action.

---

## 6) Implementation Plan (Aggressive)

## 6.1 Phase A: Protocol + Types First (breaking)

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/api/conductor.rs`
- `sandbox/src/api/websocket.rs`
- `dioxus-desktop/src/desktop/ws.rs`

Actions:
1. Add `run_id` and `document_path` as required execute response fields.
2. Add `writer.run.*` websocket variants.
3. Remove deprecated parsing branches for legacy run update events.

## 6.2 Phase B: Delete Legacy UX Paths

Files:
- `dioxus-desktop/src/desktop/components/prompt_bar.rs`
- `dioxus-desktop/src/desktop/apps.rs`
- `dioxus-desktop/src/desktop_window.rs`
- `dioxus-desktop/src/components/run.rs` (delete)

Actions:
1. Open Writer immediately after successful execute response acceptance.
2. Remove `poll_conductor_task_until_complete` from prompt bar.
3. Remove Run app registration/rendering path.
4. Remove fallback normalization (`file_path` -> `path`).

## 6.3 Phase C: Introduce RunWriterActor

Files:
- `sandbox/src/actors/run_writer/` (new module)
- `sandbox/src/actors/mod.rs`
- supervisor wiring files

Actions:
1. Spawn one `RunWriterActor` per run.
2. Serialize all document writes through this actor.
3. Persist with atomic write (`temp + rename`) and monotonic revision increment.
4. Emit `writer.run.patch` and `writer.run.progress` after each accepted mutation.

## 6.4 Phase D: Worker Integration

Files:
- `sandbox/src/actors/researcher/adapter.rs`
- `sandbox/src/actors/conductor/runtime/call_result.rs`
- `sandbox/src/actors/conductor/runtime/decision.rs`

Actions:
1. Remove worker direct shared-doc file writes.
2. Workers send typed patch/proposal commands to `RunWriterActor`.
3. Remove `ConductorAction::UpdateDraft` from decision flow.
4. Replace with explicit typed states:
   - `SpawnWorker`
   - `AwaitWorker`
   - `MergeCanon`
   - `Complete`
   - `Block`

## 6.5 Phase E: Writer Frontend Live Apply

Files:
- `dioxus-desktop/src/components/writer.rs`
- `dioxus-desktop/src/desktop/shell.rs`
- `dioxus-desktop/src/desktop/state.rs`

Actions:
1. Writer subscribes to `writer.run.*` scoped to active run.
2. Apply patch ops in revision order.
3. On revision gap: fetch latest full document and continue.
4. Render proposal text in gray; deleted proposal text gray + strikethrough.

---

## 7) Testing Gates (Required Before Merge)

## 7.1 Backend Integration

1. `execute_task` returns required Writer-start fields for accepted runs.
2. `RunWriterActor` enforces single-writer ordering and monotonic revision.
3. Concurrent runs (N=3+) do not cross-deliver events.
4. Event ordering test: out-of-order websocket delivery does not corrupt revision state.
5. `ConductorAction::UpdateDraft` no longer exists in executable flow.

## 7.2 Frontend Integration

1. Prompt submit opens Writer without waiting for completion.
2. Live worker updates appear in Writer during active run.
3. Poll timeout UI path is absent.
4. Proposal/canon/deleted styling works in dark and light themes.
5. Two concurrent Writer windows for different runs remain isolated.

## 7.3 Observability Assertions

1. Every applied patch has EventStore receipt with run scope metadata.
2. Worker lifecycle and patch stream are both visible (`actor_call` + writer events).
3. Failure events are explicit (`writer.run.failed`) and user-visible.

---

## 8) Deletions (Explicit)

Delete or fully retire:

- `dioxus-desktop/src/components/run.rs`
- Run app registration in `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/apps.rs`
- Run app render branch in `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop_window.rs`
- Prompt bar poll loop and timeout state path
- `ConductorAction::UpdateDraft` in BAML + generated types + decision handler

No partial deprecation period.

---

## 9) Migration Notes

- This cutover is intentionally breaking.
- Branch should merge only with updated frontend/backend in lockstep.
- If rollback is needed, rollback whole branch, not partial files.

---

## 10) Success Criteria

1. Submit objective -> Writer opens in under 2 seconds.
2. Worker intermediate output appears live without waiting for completion.
3. No prompt-bar poll timeout path remains.
4. Concurrent runs stream independently with strict scope isolation.
5. EventStore retains full worker + patch observability timeline.
6. Conductor completion only finalizes canon; it does not gate visibility.
