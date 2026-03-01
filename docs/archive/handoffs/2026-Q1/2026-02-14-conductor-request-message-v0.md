# Conductor Request Message v0

Date: 2026-02-14  
Status: Proposed and implementation-authoritative once accepted  
Scope: Replace "escalation contract" with a minimal conductor request message

## Narrative Summary (1-minute read)

"Escalation" is too heavy as a first abstraction.
What the system actually needs is simpler: workers and app agents must be able to send Conductor
a typed request for attention or decision without blocking.

This doc defines that minimal primitive as `request`.
No separate escalation subsystem, no dedicated escalation actor, no large policy surface.

Conductor remains event-driven and non-blocking:
children push `progress`, `result`, `failed`, or `request` messages;
conductor wakes, decides, delegates, and yields.

## What Changed

1. Reframed "escalation" as a basic `request` message kind.
2. Removed assumption that request handling needs its own orchestration subsystem.
3. Reduced required fields to a minimal v0 envelope.
4. Kept advanced controls (`deadline`, `dedupe_key`, `ttl`) as optional add-ons only when needed.

## What To Do Next

1. Implement typed `request` message in shared actor protocol.
2. Route worker/app-agent control asks through this one primitive.
3. Add tests that verify non-blocking progression under concurrent requests.
4. Defer advanced request controls until concrete production need appears.

---

## Design Principle

If a concept can be represented as a message kind, prefer that over creating a new subsystem.

For now:
1. Keep one message envelope.
2. Keep a small kind set.
3. Keep conductor policy model-led on deterministic rails.

## Message Kinds (v0)

1. `progress` - in-flight update.
2. `result` - completed output.
3. `failed` - terminal failure.
4. `request` - explicit ask for conductor attention/decision.

## `request` Envelope (v0)

Required fields:
1. `run_id`, `session_id`, `thread_id`, `correlation_id`.
2. `from_agent`, `to_agent` (`conductor`).
3. `request_kind` (`blocked|approval|conflict|reprioritize`).
4. `summary` (short natural-language context).

Optional fields (defer unless needed):
1. `deadline_ms`.
2. `dedupe_key`.
3. `hop_count` / `ttl`.
4. `requested_action`.

## Why This Is Enough

1. It preserves non-blocking orchestration semantics.
2. It supports urgent/decision-required cases without over-modeling.
3. It keeps room for hardening later without locking into premature abstractions.

## Runtime Behavior

1. Worker/app sends `request` message.
2. Runtime persists event metadata.
3. Conductor wakes with bounded `agent_tree_snapshot` context.
4. Conductor decides next delegation/replan.
5. Turn ends; no polling loop.

## Naming Guidance

Use `request` as the primary term in current docs and contracts.
`escalation` may appear only as historical wording in archived docs.

## Acceptance Criteria

1. Conductor request handling works without any dedicated escalation subsystem.
2. All request traffic uses the typed v0 envelope.
3. Non-blocking/no-polling conductor invariants remain intact.
4. Optional controls remain optional until justified by observed failures.
