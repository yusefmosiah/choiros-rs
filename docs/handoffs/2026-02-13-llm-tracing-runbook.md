# LLM Tracing Runbook (Choir Trace App)

**Date:** 2026-02-13  
**Status:** Execution-ready (prerequisite before Slice C/D/E)  
**Source Contracts:**  
- `docs/handoffs/2026-02-13-simplified-multiagent-comms-architecture.md`  
- `docs/handoffs/2026-02-13-simplified-multiagent-comms-implementation-runbook.md`

## Narrative Summary (1-minute read)

Slice A/B cleanup is complete. The next blocker is traceability of model behavior.

Current logs are actor/event centric, but not call centric. We cannot reliably answer, per LLM call, which model was used, what system context/input was sent, what output/error came back, and how long it took for a specific `run_id`/`task_id` scope.

This runbook defines the tracing contract and the minimum Trace app needed to inspect model calls cleanly before continuing Slice C/D/E.

## What Changed

- Converted tracing from a loose proposal into an implementation-first runbook with concrete file ownership.
- Locked the event contract to `llm.call.started|completed|failed` with scope identifiers and truncation markers.
- Added explicit instrumentation targets for conductor policy, harness decide loop, and watcher review/mitigation calls.
- Added a dedicated Trace app scope in desktop UI using existing logs APIs.
- Added verification gates and smoke commands that block Slice C start until green.

## What To Do Next

1. Add shared LLM trace helper and event constants.
2. Instrument conductor, harness, and watcher call sites.
3. Ship Trace app window in Choir desktop.
4. Run tracing validation gates; continue Slice C only after they pass.

---

## 1) Objective and Success Criteria

### 1.1 Objective

Make every runtime LLM call inspectable in one place, with enough context to debug routing/prompting errors without adding deterministic guard hacks.

### 1.2 Success criteria

- Every LLM call emits a `started` event and exactly one terminal event (`completed` or `failed`) with shared `trace_id`.
- Trace events contain role, function, model/provider, scope ids, duration, and bounded input/output context.
- Trace app can filter and inspect these calls live via websocket.
- A conductor run shows traces for policy and worker decision calls end to end.

## 2) Event Contract (Authoritative)

### 2.1 Event types

- `llm.call.started`
- `llm.call.completed`
- `llm.call.failed`

### 2.2 Common payload fields (all three types)

- `trace_id` (ULID)
- `role` (`conductor`, `researcher`, `terminal`, `watcher`, etc.)
- `function_name` (for example `ConductorDecide`, `Decide`, `WatcherReviewLogWindow`)
- `model_used`
- `provider` (if known)
- `actor_id`
- `started_at` (RFC3339 UTC)
- `run_id` (optional)
- `task_id` (optional)
- `call_id` (optional)
- `scope.session_id` (optional)
- `scope.thread_id` (optional)

### 2.3 Started payload extras

- `system_context` (bounded text)
- `input` (bounded JSON value)
- `input_summary` (short readable summary)

### 2.4 Completed payload extras

- `ended_at` (RFC3339 UTC)
- `duration_ms`
- `output` (bounded JSON/string)
- `output_summary`

### 2.5 Failed payload extras

- `ended_at` (RFC3339 UTC)
- `duration_ms`
- `error_code` (optional)
- `error_message`
- `failure_kind` (optional)

### 2.6 Bounded payload policy

- `system_context`: max 4 KB
- `input`: max 16 KB serialized
- `output`: max 16 KB serialized
- Add truncation metadata (`truncated`, `original_size`) whenever clipping happens.
- Redact sensitive keys before persist (`authorization`, `api_key`, `token`, `password`).

## 3) Backend Implementation

### 3.1 New helper and constants

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/observability/llm_trace.rs` (new)
- `sandbox/src/observability/mod.rs` (new/updated)

Actions:
1. Add constants for `llm.call.started|completed|failed`.
2. Add helper APIs that emit consistent payloads and return `trace_id` + `started_at`.
3. Centralize truncation/redaction in helper to avoid drift between actors.

### 3.2 Instrumentation targets (MVP)

1. Conductor policy:
- `sandbox/src/actors/conductor/policy.rs`
- Functions:
  - `bootstrap_agenda` (`ConductorBootstrapAgenda`)
  - `decide_next_action` (`ConductorDecide`)
  - `refine_objective_for_capability` (`ConductorRefineObjective`)

2. Worker harness decide loop:
- `sandbox/src/actors/agent_harness/mod.rs`
- Function:
  - `decide` (`Decide`)

3. Watcher reviews:
- `sandbox/src/actors/watcher.rs`
- Functions:
  - `llm_review_window` (`WatcherReviewLogWindow`)
  - `recommend_mitigation` (`WatcherRecommendMitigation`)

Instrumentation rule:
- Wrap each LLM call in `started` then `completed|failed`.
- Reuse one `trace_id` per call.
- On error paths, always emit `llm.call.failed` before returning.

### 3.3 Non-goals (MVP)

- Token-level stream traces.
- New trace-specific database table.
- Full prompt-template source export.

## 4) Trace App (Choir Desktop)

### 4.1 Scope

Ship a dedicated `Trace` app focused on LLM calls, not mixed lifecycle logs.

### 4.2 Files

- `dioxus-desktop/src/components/trace.rs` (new)
- `dioxus-desktop/src/components.rs`
- `dioxus-desktop/src/desktop/apps.rs`
- `dioxus-desktop/src/desktop_window.rs`
- `dioxus-desktop/src/api.rs` (only if helper wrappers are added)

### 4.3 Data source

- `GET /logs/events` (Trace app filters prompt/llm/tool events client-side)
- `WS /ws/logs/events` (live feed, client-side filtering)

### 4.4 View contract

Run graph surface:
- Root: user prompt (`trace.prompt.received` / `conductor.task.started`)
- Branch: conductor LLM calls
- Branches: researcher/terminal/watcher LLM calls
- Branch: worker tool call/result counts (including failures)

LLM detail surface:
- Existing per-trace list/detail remains for payload inspection (`system_context`, `input`, `output/error`, scope ids).

Grouping rule:
- Prefer `trace_id`.
- Secondary grouping labels: `run_id` then `task_id` when present.

## 5) Validation Gates

### 5.1 Backend tests

- Trace helper unit tests:
  - payload shape
  - truncation markers
  - redaction coverage
- Conductor policy tests assert `llm.call.*` emitted for 3 policy calls.
- Harness tests assert `Decide` emits `started` then terminal event.
- Watcher tests assert both review and mitigation calls emit traces.

### 5.2 Desktop tests

- Trace app renders trace rows from `llm.call.*` events.
- Failed traces show error details.
- Live websocket updates append in event sequence order.

### 5.3 Smoke commands

```bash
cargo test -p sandbox --lib
cargo test -p sandbox --test conductor_api_test -- --nocapture
cargo test -p sandbox --test capability_boundary_test -- --nocapture
```

Manual smoke:
1. Start `just dev-sandbox` and `just dev-ui`.
2. Run one conductor task from Prompt Bar that exercises researcher or terminal.
3. Open Trace app and verify visible `ConductorDecide` plus worker `Decide` traces.

## 6) Rollout Order

1. Add helper/constants.
2. Instrument conductor policy.
3. Instrument harness decide.
4. Instrument watcher calls.
5. Add Trace app list/detail UI.
6. Run validation gates.
7. Resume Slice C once tracing is verified.

## 7) Coverage Snapshot (2026-02-14)

### 7.1 Runtime LLM call inventory

As of February 14, 2026, runtime LLM calls are BAML-based and currently occur in:

- Conductor policy:
  - `sandbox/src/actors/conductor/policy.rs` (`ConductorBootstrapAgenda`, `ConductorDecide`, `ConductorRefineObjective`)
- Agent harness planner:
  - `sandbox/src/actors/agent_harness/mod.rs` (`Decide`)
- Watcher:
  - `sandbox/src/actors/watcher.rs` (`WatcherReviewLogWindow`, `WatcherRecommendMitigation`)

No additional direct model HTTP provider call path was identified in runtime actor modules for these flows.

### 7.2 Current tracing coverage

- `Conductor` policy calls: traced (`llm.call.started|completed|failed`) with mandatory emitter wiring.
- `Researcher` planner calls: traced via harness `Decide` with mandatory emitter wiring.
- `Terminal` planner calls: traced via harness `Decide` with mandatory emitter wiring.
- `Watcher` review/mitigation calls: traced via dedicated watcher emitter.
- User prompt root: traced via `trace.prompt.received`.
- Worker tool call/output: traced via `worker.tool.call` and `worker.tool.result`.

### 7.3 Known caveats (not missing call-sites, but coverage fragility)

- `RunBashTool` remains a non-LLM path; it now emits `worker.tool.*` for command/output visibility.
- Worker LLM traces depend on run scope being provided by dispatchers; conductor paths provide this now.

## 8) Hardening Plan (Where We Are Going)

1. Make trace wiring non-optional for runtime constructors:
   - Conductor default policy now always receives event store in production actor wiring.
   - Harness wrappers in runtime actors now always attach `LlmTraceEmitter`.
2. Add regression tests for wiring, not just helper payload shape:
   - Assert conductor policy emits `llm.call.*` in actor integration path.
   - Assert researcher/terminal harness paths emit `llm.call.*` for `Decide`.
3. Add explicit “trace coverage contract” doc check:
   - Any new runtime `B.*.call(...)` must include `start_call` + terminal event.
4. Expand graph fidelity:
   - Add call-id edge rendering from conductor capability calls to worker/tool subgraphs.
   - Add watcher escalation edge overlay (`watcher.escalation.*`) on the run graph.
