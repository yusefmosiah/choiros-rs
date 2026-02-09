# Unified Agentic Loop Harness (Chat, Terminal, Researcher)

Date: 2026-02-08
Status: Draft architecture for implementation

## Narrative Summary (1-minute read)

Chat, Terminal, and Researcher are currently close but not identical runtime loops. The next step is to converge them into one shared harness that supports the same lifecycle semantics for all capability actors: receive message, plan, call tools/delegations, emit typed events, optionally yield non-blocking, then resume on completion signals. This keeps behavior predictable across actors and makes observability, retries, and policy enforcement consistent.

## What Changed

- Locked the direction: one shared loop abstraction, multiple actor-specific capability policies.
- Clarified role boundaries:
  - Capability actors: execute work through a shared harness.
  - Conductor: orchestration and wake-up routing.
  - Watcher: deterministic alerting/signals from EventStore.
- Preserved dual contract:
  - `uactor -> actor` delegation envelope.
  - `appactor -> toolactor` typed tool calls.
- Added explicit non-goal: no raw provider/tool dump should be injected into chat answer context as final user output.

## What To Do Next

1. Extract common loop state machine from `ChatAgent` follow-up/tool loop into `agentic_harness` module.
2. Port `ResearcherActor` to the same loop contract (tool iterations + typed report emission each turn).
3. Port `TerminalActor` objective mode to the same loop contract while preserving typed app-tool dispatch path.
4. Route completion wake-ups through Conductor (or direct actor message for same-scope continuations), not ad-hoc per-actor spawn logic.
5. Add loop-level ordered integration tests for: `deferred -> completion signal -> final answer`.

---

## Problem

Current behavior is fragmented:

- Chat has a tool loop plus async follow-up continuation.
- Researcher has provider loop + deterministic summarization/reporting.
- Terminal has typed command execution and separate objective handling.

This fragmentation creates inconsistent behavior under load, inconsistent event semantics, and harder tuning for non-blocking UX.

## Target Model

All capability actors use one harness with actor-specific adapters.

Harness responsibilities:

- Prompt assembly (timestamped).
- Plan step execution (`PlanAction`).
- Tool/delegation dispatch.
- Observation injection for replanning.
- Step caps and timeout budget.
- Deferred/async yield behavior.
- Typed turn report emission (`finding`, `learning`, `escalation`, `artifact`).
- Final synthesis (`SynthesizeResponse`) only when a stable answer is ready.

Actor adapter responsibilities:

- Allowed tool surface.
- Model policy role (`chat`, `terminal`, `researcher`).
- Domain-specific result normalization.
- Domain-specific validation gates.

## Shared Loop State Machine

```text
RECEIVE_MESSAGE
  -> PLAN_STEP
    -> (no tools) SYNTHESIZE_FINAL
    -> (tools) EXECUTE_TOOL_CALLS
      -> TOOL_RESULT_OBSERVE
        -> (budget remaining) PLAN_STEP
        -> (needs background work) DEFER_ACK + YIELD

ON_COMPLETION_SIGNAL
  -> RESUME_WITH_COMPLETION_OBSERVATION
    -> PLAN_STEP
      -> SYNTHESIZE_FINAL
```

## Event Contract (Loop-Level)

Required events from all harness users:

- `worker.task.started`
- `worker.task.progress`
- `worker.task.completed` / `worker.task.failed`
- `worker.report.received`
- domain events (`research.*`, `terminal.*`, `chat.*`)

Required metadata:

- `trace_id`, `span_id`, `correlation_id`, `task_id`
- `interface_kind`
- `model_requested`, `model_used`
- scope (`session_id`, `thread_id`)

## Non-Blocking UX Rule

When a delegated task exceeds soft-wait:

- emit immediate short ack to caller,
- continue background work,
- on completion, resume loop and emit final answer,
- do not emit stale “still running” text after completion.

## Messaging/Wake-Up Rule

- Worker completion emits deterministic signal event.
- Conductor is woken by signal for cross-actor orchestration decisions.
- Same-actor continuation may be triggered by direct message receive when safe and scope-local.
- Avoid polling loops where signal/message wake-up is available.

## Policy and Safety

- Capability scoping remains at adapter level (tool allowlists, model allowlists, domain constraints).
- Prompt Bar/Conductor can route to actors but should not bypass actor capability policy.
- All actions remain reproducible via EventStore-backed run logs.

## Acceptance Criteria

1. Chat, Terminal, Researcher share one harness module for loop orchestration.
2. Each actor keeps only adapter-level logic and capability policy.
3. Integration tests show ordered non-blocking flow across all three actors.
4. Run markdown exports show consistent step structure across actors.
5. No raw provider dumps as final user-facing assistant messages.
