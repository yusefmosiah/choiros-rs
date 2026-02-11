# High-Priority Determinism/Blocked/Observability Handoff (2026-02-11)

## Narrative Summary (1-minute read)
This handoff captures the high-priority runtime hardening pass focused on three risks:
1. deterministic authority in worker/conductor control paths,
2. silent fallback behavior when planner/policy calls fail,
3. missing run-level observability and live verification.

The code now enforces typed authority for terminal and conductor bootstrap, propagates explicit blocked states, adds run-level timeline observability, and validates behavior with live external-LLM E2E.

## What Changed
1. Terminal determinism + fallback removal
- Removed deterministic shell-command bypass path from terminal agentic loop.
- Planner (`PlanAction`) failures now emit blocked progress and return typed `TerminalError::Blocked`.
- No silent direct-execution fallback remains in terminal planning path.

2. Conductor typed bootstrap policy
- Added BAML contract `ConductorBootstrapAgenda` and generated client types.
- Replaced confidence-threshold bootstrap logic with typed policy output (`dispatch_capabilities`, `block_reason`, `rationale`, `confidence`).
- Agenda capability selection is now policy-resolved and normalized against available capabilities.

3. Explicit blocked propagation to run/task state
- Added `ConductorError::WorkerBlocked` and propagated mapping from terminal worker.
- Runtime call-result handling now marks blocked outcomes with `emit_capability_blocked` semantics instead of generic failure.

4. Retry path execution fix
- `DecisionType::Retry` now respawns capability calls directly (instead of only requeueing dispatch intent), preventing retry-loop dead behavior.

5. Wake-event provenance guard
- Conductor `ProcessEvent` now ignores events with mismatched `metadata.run_id` and ignores events for unknown runs.
- Prevents spurious wake/policy execution from out-of-run provenance.

6. Run timeline observability API
- Added `GET /api/runs/{run_id}/timeline`.
- Returns ordered timeline + objective/status + derived artifacts.
- Supports required milestone validation through `required_milestones` query parameter and returns `422` when missing.

7. Live E2E coverage (external LLM path)
- Added/updated `sandbox/tests/run_lifecycle_e2e_test.rs` with live tests:
  - basic run milestones,
  - run-id correlation stability,
  - streaming-before-terminal-state,
  - concurrent run correlation isolation.
- Tests are serialized with an async lock to avoid actor-name collision flake.

## Validation Evidence
Executed on 2026-02-11:
1. `./scripts/sandbox-test.sh --lib conductor`
- Result: passed (46/46).

2. `./scripts/sandbox-test.sh --lib api::run_observability`
- Result: passed (3/3).

3. `./scripts/sandbox-test.sh --test run_lifecycle_e2e_test -- --nocapture`
- Result: passed (4/4) with live external model calls.

## What To Do Next
1. Expand live E2E from 4 scenarios to full 7-scenario matrix in research program (explicit watcher wake + blocked-run + observability/finding/learning milestone assertions).
2. Add focused API integration tests for `/api/runs/{run_id}/timeline` against realistic run event streams.
3. Continue legacy state simplification (`tasks` + `runs`) once compatibility endpoints are fully migrated.
4. Evaluate unified agentic harness extraction after current high-priority stabilization gates remain green for multiple runs.
