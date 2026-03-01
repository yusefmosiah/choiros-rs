# Conductor + Watcher BAML Cutover (2026-02-10)

You are reading the architecture correction doc for the Conductor-first lane.

## Narrative Summary (1-minute read)
We accumulated orchestration debt by treating stubs and deterministic workflows as acceptable intermediate architecture. That created "completion theater": runs look complete in logs, but Conductor is still mostly executing preset steps instead of making runtime decisions.

To fix this, Conductor and Watcher need explicit BAML contracts for decision-making and log review, while preserving typed Rust control flow and deterministic replay guarantees. Conductor delegates natural-language objectives to capability agents; typed outputs determine orchestration control. The new rule is simple: no fixed worker order as orchestration authority.

## What Changed
- Conductor runtime types landed (`agenda`, `active_calls`, `decision_log`, `artifacts`).
- Wake/display metadata landed and telemetry wiring improved.
- Deterministic orchestration remained in production path:
  - legacy `ExecuteTask` still drives static planning/execution.
  - default behavior still encodes `Terminal -> Researcher` ordering.
  - `ProcessEvent` and `DispatchReady` are still incomplete as policy engines.
- Watcher emits alerts, but Conductor does not yet use them as typed wake inputs for replanning.

## What To Do Next
1. Add BAML contracts for Conductor decisions and Watcher escalation triage.
2. Integrate those contracts into Conductor wake/dispatch loop.
3. Remove deterministic workflow authority (`build_default_plan` and fixed for-loop execution authority).
4. Make Watcher LLM-driven for event-log review with typed outputs and replay-safe input slices.
5. Enforce model-tier policy: Watcher uses lower-power models than Conductor.
6. Remove deterministic fallbacks entirely and fail fast with typed errors when policy execution fails.
7. Enforce new test gates that verify adaptive replanning behavior.

## Problem Statement
The system currently violates intended Conductor behavior in three ways:
1. Deterministic sequencing is still primary control authority.
2. Worker semantic outputs (`incomplete`, `blocked`, recommended next capability/objective) are underused.
3. Watcher alerts are not first-class runtime wake signals for Conductor policy.

This creates repeated failure patterns:
- weak terminal outcomes are accepted as completed step payloads,
- follow-up capability selection is static rather than context-driven,
- escalation handling is disconnected from orchestration decisions.

## Root Cause
1. Stub acceptance criteria were too loose (`compiles`, `non-panic`, `logs`) and not behavioral.
2. MVP deterministic flows were not explicitly sunset after typed runtime landed.
3. BAML decision surfaces exist for chat, but not for Conductor/Watcher orchestration.
4. Tests validated transitions but not adaptive planning quality or replan behavior.
5. Agent prompts rewarded feature completion over authority migration.

## Non-Negotiable Principles
1. Conductor is orchestration authority for multi-step work.
2. Control flow is typed (`shared-types` + actor messages + BAML outputs).
3. No natural-language matching for workflow state transitions.
4. No fixed worker order as runtime authority.
5. Conductor capability delegation uses natural-language objectives.
6. Watcher performs LLM-driven review over deterministic event-log truth.
7. Watcher model tier is lower-power than Conductor model tier.

## Target Runtime Model
1. Wake event arrives with typed metadata.
2. Conductor updates run state from event payload.
3. Conductor builds typed decision input snapshot.
4. Conductor invokes BAML policy function and produces/refines natural-language objectives for selected capability calls.
5. Conductor applies typed decision to agenda/calls/artifacts.
6. Conductor dispatches ready capabilities.
7. Watcher runs periodic/event-triggered BAML review over scoped event-log windows and emits typed escalation outputs.
8. Conductor repeats until terminal run status.

Display-only telemetry never changes orchestration state.

## BAML Scope

### Conductor BAML (required)
Add new BAML functions dedicated to orchestration policy.

Suggested functions:
1. `ConductorDecideNextAction(input: ConductorDecisionInput) -> ConductorDecisionOutput`
2. `ConductorRefineObjective(input: ConductorObjectiveRefineInput) -> ConductorObjectiveRefineOutput`
3. `ConductorAssessTerminality(input: ConductorTerminalityInput) -> ConductorTerminalityOutput`

Suggested output enum (typed, closed set):
- `dispatch`
- `retry`
- `spawn_followup`
- `continue`
- `complete`
- `block`

### Watcher BAML (required)
Watcher reviews scoped EventStore windows with lower-power LLMs and emits typed escalation outputs.

Suggested functions:
1. `WatcherReviewLogWindow(input: WatcherLogWindowInput) -> WatcherReviewOutput`
2. `WatcherRecommendMitigation(input: WatcherMitigationInput) -> WatcherMitigationOutput`

Watcher BAML must:
- operate only on replayable EventStore slices,
- emit typed, auditable outputs,
- include confidence and rationale fields.

Watcher BAML must not:
- fabricate raw events,
- bypass typed escalation contracts.

## Contract Requirements
All new BAML outputs must map 1:1 to typed Rust enums/structs.

Minimum fields for `ConductorDecisionInput`:
- `run_id`, `task_id`, `objective`
- `run_status`
- `agenda_snapshot`
- `active_calls_snapshot`
- `latest_wake_event`
- `recent_watcher_alerts`
- `budget_snapshot` (attempts/time/retry windows)

Minimum fields for `ConductorDecisionOutput`:
- `decision_type`
- `target_agenda_item_ids`
- `new_agenda_items`
- `retry_policy`
- `completion_reason`
- `confidence`

Minimum fields for `WatcherTriageOutput`:
- `escalation_action`
- `urgency`
- `recommended_capability`
- `recommended_objective`
- `rationale`
- `confidence`

Minimum fields for `WatcherReviewOutput`:
- `window_id`
- `review_status`
- `escalations`
- `risks`
- `confidence`

## Implementation Boundaries
1. Keep API compatibility with `ConductorExecuteRequest`.
2. Treat request `worker_plan` as optional initial seed, not immutable script.
3. Remove deterministic control authority from:
   - `build_default_plan`
   - fixed-loop `execute_worker_plan`
4. Preserve and extend wake/display lanes.
5. Store every BAML decision invocation as observability events with:
   - model/provider,
   - decision type,
   - confidence,
   - correlation IDs.
6. Enforce model policy:
   - Watcher BAML functions resolve to lower-power models than Conductor BAML functions.
7. No deterministic fallback path is allowed after cutover.
8. On BAML/policy failure, emit typed failure events and transition run state explicitly (failed/blocked), never silent fallback.

## Migration Plan

### Phase 1: Contract Introduction
- Add Conductor and Watcher BAML types/functions in `baml_src`.
- Regenerate BAML client.
- Delete deterministic-path authority code at the start of implementation (`build_default_plan` / fixed-loop executor as control authority).

### Phase 2: Cutover Validation
- Wire Conductor decision loop to BAML policy.
- Wire Watcher review loop to BAML policy over scoped log windows.
- Add fail-fast typed error handling for policy/model failures (no deterministic fallback).
- Validate event timelines and terminal-state correctness under policy failures.

### Phase 3: Hardening
- Make BAML policy loop default orchestration path.
- Add CI checks to prevent regressions to fixed worker order.

### Phase 4: Cleanup
- Remove deterministic authority code.
- Keep only typed failure/blocked transitions for hard failures.

## Test Gates (Must Pass)
1. Conductor can replan after `incomplete` worker outcome.
2. Conductor can change capability choice based on watcher escalation.
3. Repeated wake events do not duplicate dispatch.
4. Terminal status is derived from typed run state, not script exhaustion.
5. BAML output enums/fields fully decode to Rust typed contracts.
6. No deterministic fallback path can execute under any tested policy/model failure scenario.
7. Watcher BAML review path processes event windows and emits typed escalation outputs.
8. Model policy enforces Watcher < Conductor capability tier.

## Coding-Agent Guardrails
1. Do not mark orchestration tasks complete when behavior is still stubbed.
2. Do not accept "compiles + tests" as sufficient for runtime-policy milestones.
3. Every orchestration milestone must include one event timeline proving replan behavior.
4. Every policy branch must have a typed test assertion.
5. Deterministic fallback paths are prohibited; any policy failure must surface as typed run failure/block behavior.

## Documentation Synchronization Rule
When modifying BAML contracts:
1. Update `baml_src/*.baml`.
2. Regenerate BAML client code.
3. Keep `sandbox/src/baml_client/baml_source_map.rs` in sync.
4. Update this doc and related prompt docs if control semantics changed.

## Definition of Done
This cutover is done only when:
1. Conductor runtime decisions are agentic and typed.
2. Watcher alerts influence Conductor through typed wake policy.
3. Fixed worker-order orchestration is removed as authority.
4. BAML contracts for Conductor and Watcher are in active runtime use.
5. CI contains regression guards against deterministic workflow reintroduction.
6. Conductor-to-capability delegation objectives are natural language and policy-authored.
