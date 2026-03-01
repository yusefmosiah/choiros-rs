# Regression Recovery Plan: Writer Open + Trace Run Feed

Date: 2026-02-14  
Status: Handoff for next session  
Owner: Next coding session

## Narrative Summary (1-minute read)

Two regressions appeared during this session:
1. Prompt Bar submits stay in `Opening Writer...` and Writer does not open reliably.
2. Trace app no longer shows new runs as they begin.

These worked before this session. Multiple runtime files were changed in one pass, including both root-cause candidate changes and speculative fixes. The next session should first restore known-good behavior, then re-apply only contract-safe changes one-by-one with verification after each step.

## What Changed In This Session

Runtime-impacting changes:
- `dioxus-desktop/src/components/trace.rs` - removed `task_id` fallback in `parse_prompt_event`.
- `sandbox/src/api/conductor.rs` - changed `conductor.task.started` telemetry timing/payload.
- `dioxus-desktop/src/desktop/components/prompt_bar.rs` - added writer-open timeout/guard logic.
- `sandbox/src/actors/desktop.rs` - changed window-open event append from sync to async best-effort.

Non-runtime/noise changes:
- `.gitignore` - added `sandbox/conductor/runs/`.
- docs and local generated artifacts are also present in working tree.

## Why The Attempted Fixes Did Not Resolve It

1. They were speculative and touched multiple layers at once (UI state flow, desktop actor persistence, API telemetry).
2. The Trace regression likely came from tightening parse assumptions too early (compatibility removed before all event payload paths were proven uniform).
3. The Writer timeout logic only changes visible failure behavior; it does not fix underlying window-open latency or blocked execution paths.
4. Desktop async append reduced one possible bottleneck, but did not prove the real bottleneck was EventStore sync append.

## Recovery Plan (Do This First In New Session)

### 0) Save current diff snapshot

```bash
git diff > /tmp/2026-02-14-writer-trace-regression.patch
```

### 1) Restore known-good runtime files only

```bash
git restore -- dioxus-desktop/src/components/trace.rs
git restore -- dioxus-desktop/src/desktop/components/prompt_bar.rs
git restore -- sandbox/src/actors/desktop.rs
git restore -- sandbox/src/api/conductor.rs
```

Do not touch unrelated user changes.

### 2) Baseline verification (must pass before new fixes)

Run:

```bash
cargo fmt --all
cargo check -p sandbox
cargo check --manifest-path dioxus-desktop/Cargo.toml
./scripts/sandbox-test.sh --lib desktop
./scripts/sandbox-test.sh --test conductor_api_test
```

Manual checks:
1. Submit a prompt from Prompt Bar.
2. Writer window opens immediately.
3. Trace app shows new run row as soon as run starts.
4. Writer document receives live updates during worker execution.

If baseline fails after restore, stop and diagnose pre-existing breakage before reintroducing any change.

## Controlled Re-Application Plan

### Step A - Re-apply only Trace identity tightening safely

Goal: keep canonical `run_id` preference without breaking feed.

Implement parser compatibility in `dioxus-desktop/src/components/trace.rs`:
- Prefer `payload.run_id`.
- Temporarily allow fallback for legacy payload shapes (`payload.task_id` and/or nested payload forms if present).
- Add explicit TODO/comment with removal condition (after all emitters and stored historical events are normalized).

Verify Trace live updates before any other change.

### Step B - Re-apply Conductor event schema fix without timing change

Goal: ensure `conductor.task.started` includes `run_id` on all applicable emit paths, but avoid altering launch timing semantics in `execute_task`.

If a path cannot know `run_id` yet, use a distinct telemetry event name instead of overloading `conductor.task.started` with partial payload.

### Step C - Writer-open issue diagnosis (if still present)

Instrument and measure instead of speculative logic:
1. Time `open_window` request latency at UI boundary in `dioxus-desktop/src/desktop/components/prompt_bar.rs`.
2. Time `open_window` API handler and `DesktopActorMsg::OpenWindow` handling in `sandbox/src/api/desktop.rs` and `sandbox/src/actors/desktop.rs`.
3. Check if the Desktop actor mailbox is delayed by other messages.

Only after evidence, choose a fix (e.g., prioritization, decoupling, or local optimistic window open).

## Acceptance Criteria

1. Prompt Bar run opens Writer immediately (no indefinite `Opening Writer...`).
2. Trace app shows new runs as they start.
3. Live document updates stream during worker execution.
4. Any identity/schema tightening ships with compatibility and tests.
5. No multi-file speculative changes without per-step verification.

## Files To Inspect First Next Session

- `dioxus-desktop/src/components/trace.rs`
- `dioxus-desktop/src/desktop/components/prompt_bar.rs`
- `sandbox/src/api/conductor.rs`
- `sandbox/src/api/desktop.rs`
- `sandbox/src/actors/desktop.rs`
- `sandbox/src/actors/conductor/runtime/start_run.rs`
