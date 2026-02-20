# Agent Trajectory Viewer - Execution Log

**Date:** February 20, 2026  
**Source Runbook:** `docs/design/2026-02-19-agent-trajectory-viewer.md`

## Narrative Summary (1-minute read)

Implemented trace-viewer phase work across UI + backend tests + Playwright phase/eval harnesses.
Phase E2E suites for phase 1-4 are passing against a clean restarted backend/UI.
Eval harness exists and runs, but currently fails in this environment because run-scoped terminal/timeline events are not emitted reliably for eval-triggered runs.

## What Changed

- UI implementation (`dioxus-desktop/src/components/trace.rs`):
  - Added parsing/state for:
    - `conductor.worker.call/result`
    - `conductor.capability.completed/failed/blocked`
    - `conductor.run.started`
    - `conductor.task.progress/completed/failed`
    - `worker.task.started/progress/completed/failed/finding/learning`
  - Added run summary metrics:
    - run status, worker counts/failures, worker calls, capability failures
    - total duration and total tokens
  - Added run-row sparkline and status badge rendering.
  - Added delegation timeline UI and delegation-colored graph edges.
  - Added worker graph nodes and lifecycle chip strip in loop details.
  - Added trajectory grid with Status/Duration/Tokens modes and cell selection/highlighting.
  - Added duration bars and token stacked bars in trace cards.
  - Added pure-function unit tests for trajectory cell build + bucketing.

- Backend/API/test changes:
  - Added `sandbox/tests/trace_viewer_test.rs` with 8 integration tests:
    - delegation queryability
    - run status terminal derivation
    - capability call_id correlation
    - worker lifecycle round-trip
    - finding/learning round-trip
    - timeline worker objective coverage
    - duration round-trip
    - token usage round-trip
  - Added route alias:
    - `GET /conductor/runs/{run_id}/timeline` -> `run_observability::get_run_timeline`
    - existing `/api/runs/{run_id}/timeline` remains.

- Playwright:
  - Added:
    - `tests/playwright/trace-viewer.helpers.ts`
    - `tests/playwright/trace-viewer-phase1.spec.ts`
    - `tests/playwright/trace-viewer-phase2.spec.ts`
    - `tests/playwright/trace-viewer-phase3.spec.ts`
    - `tests/playwright/trace-viewer-phase4.spec.ts`
    - `tests/playwright/trace-viewer-eval.spec.ts`
  - Updated `tests/playwright/playwright.config.ts`:
    - included phase specs in `sandbox` project
    - added `trace-eval` project.

## Verification

### Backend + UI Rust tests

- `./scripts/sandbox-test.sh --test trace_viewer_test` -> **PASS (8/8)**
- `cd dioxus-desktop && cargo test --lib` -> **PASS**

### E2E Playwright (phase suites)

- `cd tests/playwright && npx playwright test --project=sandbox --grep "trace-viewer-phase"` -> **PASS (12/12)**

### Eval Playwright

- `cd tests/playwright && npx playwright test --project=trace-eval trace-viewer-eval.spec.ts --grep "file-listing" --max-failures=1 --reporter=list` -> **FAIL**
  - Failure mode: timeout waiting for `conductor.task.completed/failed` and no run-scoped events returned for the run id.
  - Current result indicates eval harness is wired correctly but runtime signal completeness for eval runs is not stable in this environment.

## What To Do Next

1. Diagnose eval run lifecycle emission:
   - verify `conductor.execute` run id persistence and corresponding `conductor.task.*` emission path for eval runs.
   - inspect run filter semantics for `/logs/events?run_id=...` under high load.
2. Re-run full eval suite after lifecycle signal fix:
   - `cd tests/playwright && npx playwright test --project=trace-eval trace-viewer-eval.spec.ts --max-failures=1 --reporter=list`
3. Optional hardening:
   - expose deterministic test-mode run completion signal for eval harness stability.
