# Conductor Model-Led Control Plane: Direction and Next Steps

Date: 2026-02-14  
Status: Active direction update (authoritative for current work)  
Scope: Conductor orchestration authority, watcher de-scope, request-message control path, tracing rollout order

## Narrative Summary (1-minute read)

ChoirOS architecture is centered on one control-plane core:
`Conductor` receives and sends actor messages across humans, workers, and app agents.

These messages can carry natural-language objectives, but orchestration authority is
model-led, not deterministic workflow code.

The runtime should stop encoding brittle, step-by-step orchestration for normal multi-step work.
Deterministic logic remains only for safety and operability rails (identity, routing, auth,
budgets, timeouts, cancellation, idempotency, loop prevention, and trace persistence).

Watcher/Wake are removed from normal run progression.
Requests should be sent directly by worker/app agents to Conductor through typed actor messages.
Conductor remains orchestration-only and does not execute tools directly.
Tool schemas are defined once in shared contracts and granted to agents/workers by capability policy.

Near-term execution pattern for app automation is explicit:
1. Build good human UX first.
2. Expose stable headless API.
3. Make that API agentic through an app-agent harness.

Tracing follows this exact sequence next.

## What Changed

1. Replaced ambiguous "NO ADHOC WORKFLOW" wording with explicit model-led control-flow policy.
2. Declared "no deterministic workflow authority" as the target for multi-step orchestration.
3. Removed Watcher/Wake from core request path in normal runs.
4. Set direct request path to `Worker/App Agent -> Conductor` via typed actor messages.
5. Clarified Watcher role as optional recurring-event detection, not orchestration authority.
6. Locked tracing rollout order: human UX -> headless API -> app-agent harness.
7. Added conductor non-blocking subagent pillar and wake-context requirement.
8. Added typed `agent_tree_snapshot` contract for bounded conductor wake context.
9. Clarified capability ownership: shared tool schemas, granted usage, and Writer canonical mutation authority.

## What To Do Next

1. Finalize typed request-message contract for all worker/app->conductor asks.
2. Remove remaining deterministic orchestration paths that bypass model-led planning.
3. Add loop-safety and operability rails where request fan-out is observed.
4. Complete Writer app-agent harness contract and verify it uses only app API surfaces.
5. Enforce capability ownership boundary: remove active Conductor direct tool execution paths.
6. Deliver Tracing Phase 1 (human UX), then Tracing Phase 2 (headless API), then Phase 3 harness.
7. Add integration tests for ordered scoped streams that include request and completion signals.
8. Enforce non-blocking/no-polling conductor turns with agent-tree wake snapshots.
9. Implement `agent_tree_snapshot` bounds/truncation/freshness policy in runtime projection.

---

## Control Authority Split (Authoritative)

Model-managed by default:
1. Task decomposition.
2. Delegation ordering.
3. Adaptive replanning.
4. Request handling timing and narrative context.

Deterministic rails only:
1. Identity and routing correctness.
2. Capability boundaries and authorization.
3. Budgets, deadlines, cancellation, and backpressure.
4. Idempotency, dedupe, cooldowns, and loop prevention.
5. Event/trace persistence and auditability.

## Capability Ownership Boundary

1. Conductor has no direct tool execution path.
2. Tool schemas are defined once and reused by all granted agents/workers.
3. Terminal and Researcher include `file_read`, `file_write`, and `file_edit` as baseline worker tools.
4. Writer app agent is canonical for living-document/revision mutation authority.
5. Workers emit outputs/requests; app agents own interactive continuity and artifact shaping.

## Subagent Model (Non-Blocking)

Conductor treats workers and app agents as logical subagents.
This hierarchy is message-driven, not blocking parent-child execution.

Hard constraints:
1. Conductor never polls child agents for completion.
2. Conductor never blocks waiting for child work.
3. Conductor wakes on pushed actor events and returns finite turns.
4. Every wake includes a bounded agent-tree state digest.

See: `docs/architecture/2026-02-14-conductor-non-blocking-subagent-pillar.md`.
Contract: `docs/architecture/2026-02-14-agent-tree-snapshot-contract.md`.

## Conductor Request Message Direction (v0)

Every control ask from workers or app agents to Conductor should use one typed `request` message,
with natural-language rationale as payload context.

Minimum v0 contract:
1. `run_id`, `session_id`, `thread_id`, `correlation_id`.
2. `from_agent`, `to_agent` (`conductor`).
3. `request_kind` (`blocked|approval|conflict|reprioritize`).
4. `summary` (short natural-language context).

Optional (only when needed):
1. `deadline_ms`.
2. `dedupe_key`.
3. `hop_count`/`ttl`.
4. `requested_action`.

This keeps model planning flexible while avoiding premature control-plane abstractions.

See: `docs/architecture/2026-02-14-conductor-request-message-v0.md`.

## Watcher/Wake Position

Current position:
1. Watcher is not part of normal run progression authority.
2. Watcher can return later as a recurring-event detector actor.
3. If present, Watcher emits signals; Conductor remains orchestration authority.

This avoids hidden control loops while preserving a place for future anomaly detection.

## App Agent Rollout Pattern

Use this order for each app domain:
1. Human-first UX that is clear, debuggable, and valuable by itself.
2. Headless API that exposes the same capabilities with typed contracts.
3. App-agent harness that consumes only that API (no private bypass path).

Immediate application:
1. Finish Tracing for humans.
2. Expose tracing headless interfaces.
3. Agentify tracing after API is stable and tested.

## Implementation Risks To Avoid

1. Reintroducing deterministic per-task orchestration branches in Conductor.
2. Letting free-text parsing become control authority instead of typed metadata.
3. Creating app-agent harnesses that bypass app APIs and fork behavior.
4. Over-designing request controls before evidence demands them.

## Acceptance Signals For This Direction

1. Multi-step runs are model-directed without deterministic fallback orchestration branches.
2. Conductor requests appear as typed actor messages with auditable metadata.
3. Watcher is absent from normal run-step authority.
4. Tracing ships in the staged sequence: human UX, then API, then harness.
