# Subagent Foundation Execution Plan

Date: 2026-02-14  
Status: Active execution setup  
Scope: Next-step implementation packets for subagent execution

## Narrative Summary (1-minute read)

After the simplification cutover, the next phase is foundation hardening through focused subagent packets.

Priority is architecture coherence and runtime contracts:
1. identity contract cleanup,
2. shared tool-schema and capability grants,
3. non-blocking conductor/app/worker messaging,
4. writer harness authority and app-agent boundaries,
5. tracing foundations.

Known UX gap is acknowledged: workers can complete while on-screen document updates may lag.
This is explicitly deferred for now and should not block foundation work.

## What Changed

1. Added subagent-ready execution packets with explicit acceptance gates.
2. Sequenced work to match current architecture decisions and simplification goals.
3. Marked live document screen-sync issue as deferred (known, non-blocking).
4. Added copy/paste task briefs to reduce orchestration overhead.

## What To Do Next

1. Run packets A through E in order unless a blocking dependency appears.
2. Keep each packet small and mergeable, with test evidence per packet.
3. Do not reintroduce watcher/worker-signal legacy abstractions.
4. Track deferred UX sync issue separately until foundations are stable.

---

## Packet A - Identity Contract Hardening

Objective:
- Remove remaining task-id fallback behavior from active runtime paths.

Deliverables:
1. Canonical identity fields used consistently: `run_id`, `call_id`, `correlation_id`, `session_id`, `thread_id`.
2. Runtime/events avoid fallback interpretation paths.
3. Tests assert no fallback path execution.

Acceptance:
1. Grep-based and test-based evidence that fallback paths are removed from primary flow.
2. Integration tests pass for scoped multi-instance streams.

## Packet B - Shared Tool Schema Registry

Objective:
- Ensure tools are defined once and granted, not duplicated.

Deliverables:
1. Single source of truth for tool schemas.
2. Conductor has no tool execution path.
3. Terminal and Researcher include baseline `file_read/file_write/file_edit` grants.

Acceptance:
1. No duplicate tool schema definitions in runtime sources.
2. Capability-grant tests validate baseline worker profile.

## Packet C - Writer Harness Authority

Objective:
- Make Writer app-agent harness canonical for living-document/revision mutation authority.

Deliverables:
1. Writer harness contract finalized and wired through app-agent path.
2. Mutations attributed to Writer authority in trace/events.
3. Conductor remains orchestration-only.

Acceptance:
1. Boundary tests fail on direct conductor mutation attempts.
2. Writer authority path is the canonical mutation route.

## Packet D - Worker Event Model Enforcement

Objective:
- Enforce canonical worker event model: `progress|result|failed|request`.

Deliverables:
1. Runtime emission paths aligned to canonical kinds.
2. Event naming consistency across API, stream, and docs.
3. Request routing validated under concurrency.

Acceptance:
1. Ordered websocket integration assertions pass for canonical kinds.
2. No legacy worker-signal contract references in active runtime docs.

## Packet E - Tracing Foundation Sequence

Objective:
- Advance tracing in strict order: human UX -> headless API -> app-agent harness.

Deliverables:
1. Human tracing UX quality baseline.
2. Typed headless query/stream API.
3. Harness readiness checklist (without jumping ahead to full tracing agent behavior).

Acceptance:
1. Human-first tracing workflows are usable and auditable.
2. API contract is stable enough for harness consumption.

## Deferred UX Note (Non-Blocking)

Known issue:
- Workers can work and complete while document updates on screen may lag.

Decision:
- Defer this until foundation packets above are stable.
- Keep it tracked as a known UX gap; do not let it derail contract simplification.

## Copy/Paste Subagent Briefs

Packet A brief:
"Remove task-id fallback paths from active runtime identity handling. Keep scoped IDs canonical (`run_id/call_id/correlation_id/session_id/thread_id`). Add tests proving fallback paths do not execute in primary flow."

Packet B brief:
"Consolidate tool schemas into one shared registry. Ensure Conductor executes no tools directly. Validate Terminal+Researcher baseline file tool grants and remove schema duplication."

Packet C brief:
"Finalize Writer app-agent harness mutation authority for living-document/revision updates. Ensure mutation attribution is writer-owned and conductor remains orchestration-only."

Packet D brief:
"Enforce canonical worker event kinds (`progress/result/failed/request`) across runtime, websocket, and docs. Remove residual legacy signal naming from active paths."

Packet E brief:
"Advance tracing foundations in order: human UX first, then headless API, then harness readiness. Avoid skipping sequence."
