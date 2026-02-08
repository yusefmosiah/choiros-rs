# ADR-0001: EventStore/EventBus Reconciliation

Date: 2026-02-08  
Status: Accepted (phase-1 rollout in progress)  
Owner: ChoirOS core architecture

## Context

ChoirOS currently has two event mechanisms:

1. `EventStoreActor` (libSQL): durable append-only log, replayable, queryable.
2. `EventBusActor`: pub/sub delivery plane for low-latency fanout.

This creates a conceptual conflict if both are treated as authoritative.  
We require deterministic replay, scoped isolation, and security/audit guarantees for multi-agent operation.

## Decision

`EventStore` is the **single source of truth**.  
`EventBus` is a **delivery/notification plane only**.

### Rules

1. Single-write rule:
   - Domain actors write events to `EventStore` only.
   - No business-critical dual-write path (`EventStore` + `EventBus`) from producers.

2. Relay rule:
   - After durable commit, committed events may be relayed to `EventBus`.
   - Relay payload must include durable identity (`seq`, `event_id`) for dedup and recovery.

3. Read correctness rule:
   - Watchers, audits, and replay always anchor on `EventStore` cursors.
   - `EventBus` may optimize live UX but cannot define correctness.

4. Recovery rule:
   - Consumers resume from last processed `seq` in `EventStore`.
   - `EventBus` drops/outages cannot cause permanent data loss.

## Why

1. Deterministic replay/debug:
   - Only durable ordered log can satisfy this.

2. Security/audit traceability:
   - Policy, model, and tool decisions must be queryable from one canonical ledger.

3. Simpler failure semantics:
   - Avoid split-brain between volatile and durable paths.

4. Preserves low-latency UX:
   - Relay enables real-time fanout while maintaining correctness.

## Event Contract (Committed Relay Envelope)

Minimum relay envelope for EventBus publication:

```json
{
  "seq": 12345,
  "event_id": "01K...",
  "event_type": "worker.task.completed",
  "topic": "worker.task.completed",
  "timestamp": "2026-02-08T06:00:00Z",
  "actor_id": "application_supervisor",
  "user_id": "system",
  "trace_id": "optional",
  "correlation_id": "optional",
  "payload": {}
}
```

Constraints:
- `seq` must come from committed `EventStore` row.
- Consumers dedup by `event_id` (or `seq` per stream).

## Taxonomy Decision

Canonical namespaces:

- `worker.task.*`
- `chat.*`
- `watcher.finding.*`
- `watcher.learning.*`
- `watcher.signal.*`
- `researcher.*`

Compatibility aliases may exist temporarily but are not canonical.

## Failure Semantics

1. EventStore write fails:
   - Operation fails; no relay attempted.

2. EventStore write succeeds, relay fails:
   - Durable record exists.
   - Live stream may miss event.
   - Consumers recover via cursored EventStore polling/replay.

3. EventBus unavailable:
   - System remains correct (degraded live updates only).

4. Relay duplicates:
   - Allowed; consumers must dedup by `event_id` / `seq`.

## Consequences

### Positive
- Single correctness model.
- Strong audit/replay guarantees.
- Clear watcher architecture baseline.

### Negative
- Slightly higher latency than pure in-memory bus for some UI updates.
- Requires relay/cursor discipline across consumers.

## Non-Goals

- Replacing EventBus entirely.
- Designing final high-scale distributed log architecture.
- Full SIEM pipeline in this phase.

## Rollout Plan

1. Freeze conceptual drift:
   - Do not add new producer dual-write logic.

2. Normalize taxonomy:
   - Migrate mixed names to canonical namespaces.

3. Implement committed relay adapter:
   - Relay from committed EventStore events to EventBus envelope.

4. Consumer migration:
   - Watcher and logs UI use EventStore cursor as primary, EventBus as optional acceleration.

5. Add invariants/tests:
   - "If EventStore contains event, consumer can eventually process it without EventBus."

## Rollout Progress (Phase 2 in progress)

Implemented:
- `ApplicationSupervisor` now persists request/worker events directly to `EventStore` first.
- Direct EventBus fanout from supervisor helper path removed to avoid duplicate delivery.
- EventBus default persistence setting changed to disabled (`default_persist: false`).
- WebSocket actor-call streaming accepts canonical `worker.task.*` names in addition to legacy names.
- Added `EventRelayActor` (`EventStore -> EventBus`) using committed rows:
  - polls `EventStore` via cursor (`since_seq`)
  - publishes to EventBus with `persist: false`
  - enriches relay payload with `committed_event` metadata (`seq`, `event_id`, etc.)
- `ApplicationSupervisor` now supervises `EventRelayActor`.

Remaining:
- Remove remaining legacy naming paths after migration window.

## Acceptance Criteria

1. Every critical workflow is replayable from EventStore alone.
2. EventBus outage does not violate correctness.
3. Watcher alerts remain reproducible from stored events.
4. Canonical event namespaces enforced for new events.
