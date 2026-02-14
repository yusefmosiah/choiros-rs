# Agent Tree Snapshot Contract (Conductor Wake Context)

Date: 2026-02-14  
Status: Proposed and implementation-authoritative once accepted  
Scope: Typed `agent_tree_snapshot` envelope for non-blocking conductor wakes

## Narrative Summary (1-minute read)

Conductor turns are non-blocking and event-driven.
To make model planning reliable on each wake, conductor needs a bounded, typed view of current system topology.

`agent_tree_snapshot` is that view.
It is a compact, deterministic digest of logical subagents (workers/app agents), their status, leases,
recent signals, and active correlation handles.

This snapshot is not built by polling child agents.
It is produced from event/state projections and attached to each conductor wake message.

The contract is intentionally bounded and truncation-aware so prompt context remains stable and scalable.

## What Changed

1. Defined a first-class typed `agent_tree_snapshot` envelope for conductor wake context.
2. Defined canonical node fields for status, lease, signal, request, and correlation metadata.
3. Defined hard size bounds, deterministic truncation order, and freshness semantics.
4. Defined producer/consumer responsibilities without introducing polling loops.
5. Defined acceptance tests for non-blocking progression and snapshot determinism.

## What To Do Next

1. Add shared-types definitions for `AgentTreeSnapshot` and `AgentTreeNodeDigest`.
2. Add snapshot builder projection fed by actor/event updates (no direct child polling).
3. Attach snapshot to every conductor wake message.
4. Add truncation/freshness telemetry fields and stream them for observability.
5. Add integration tests for ordering, truncation determinism, and human-interrupt preemption.

---

## Design Goals

1. Give conductor enough live context to orchestrate subagents safely.
2. Keep prompt payload bounded and deterministic.
3. Preserve non-blocking runtime behavior.
4. Maintain auditability through typed metadata, not prose parsing.

## Non-Goals

1. Full event replay in wake context.
2. Embedding raw tool outputs or long natural-language transcripts.
3. Polling child actors for state at wake time.

## Envelope Shape (Conceptual)

Required top-level fields:
1. `snapshot_id` (ULID/UUID).
2. `generated_at` (UTC).
3. `as_of_event_seq` (latest committed event sequence observed).
4. `root_agent_id` (conductor for this run/session scope).
5. `scope` (`run_id`, `session_id`, `thread_id`).
6. `nodes` (`AgentTreeNodeDigest[]`, bounded).
7. `summary` (counts + health hints, bounded).
8. `truncated` (bool) + `truncation_meta`.
9. `stale` (bool) + `snapshot_age_ms`.

## Node Digest Shape (Conceptual)

Each `AgentTreeNodeDigest` should contain:
1. Identity:
   - `agent_id`
   - `role` (`conductor|worker|app_agent|ui_agent|system_actor|human_interface`)
   - `parent_agent_id | null`
2. Lifecycle:
   - `status` (`idle|running|blocked|failed|completed|unknown`)
   - `status_updated_at`
3. Lease:
   - `lease_owner | null`
   - `lease_expires_at | null`
   - `lease_remaining_ms | null`
4. Work focus:
   - `active_run_id | null`
   - `active_task_id | null`
   - `capability | null`
5. Recent signal:
   - `last_signal_kind | null` (`progress|result|failed|request|heartbeat|input`)
   - `last_signal_at | null`
   - `last_correlation_id | null`
6. Request context:
   - `open_request_count`
   - `last_request_kind | null`
   - `recent_request_dedupe_keys` (bounded small list)

## Summary Shape (Conceptual)

`summary` should include:
1. `node_count_total`.
2. `node_count_included`.
3. `counts_by_status`.
4. `blocked_count`, `failed_count`, `overdue_lease_count`.
5. `active_correlation_handles` (bounded list).
6. `open_request_count` (count).

## Bounds and Budget (Required)

Suggested default limits:
1. `max_nodes = 64`.
2. `max_recent_request_dedupe_keys_per_node = 3`.
3. `max_active_correlation_handles = 20`.
4. `max_total_snapshot_bytes = 24_000` (before prompt wrapping).
5. `max_string_length = 160` per free text field (prefer IDs/enums).

If limits are exceeded, truncation must be deterministic and visible.

## Deterministic Truncation Policy (Required)

Inclusion order:
1. Always include root conductor node.
2. Include nodes directly referenced by current wake message correlation/run.
3. Include `blocked` and `failed` nodes first.
4. Include nodes with active leases nearing expiry.
5. Include remaining nodes by newest `last_signal_at`, tie-break by `agent_id`.

When truncating:
1. Set `truncated = true`.
2. Fill `truncation_meta` with:
   - `omitted_count_total`
   - `omitted_by_status`
   - `byte_budget`
   - `policy_version`

## Freshness Semantics (Required)

1. Snapshot generation is projection-driven from committed/in-memory actor events.
2. No synchronous child polling is allowed to "refresh" snapshot.
3. Mark `stale = true` when `snapshot_age_ms` exceeds configured threshold.
4. Conductor should still run with stale snapshots, but decision policy should treat stale context conservatively.

## Producer/Consumer Responsibilities

Producer (runtime projection):
1. Build snapshot incrementally from actor/event updates.
2. Enforce bounds/truncation deterministically.
3. Emit snapshot telemetry (`generated`, `truncated`, `stale`, generation latency).

Consumer (conductor turn):
1. Read snapshot as bounded state digest.
2. Delegate/replan without polling child actors.
3. Preserve typed safety rails (routing/auth/budgets/cancel/idempotency/loop prevention).

## Security and Redaction

1. Snapshot carries IDs, enums, counts, and handles.
2. Do not include raw secret-bearing payloads or full tool outputs.
3. Redact sensitive fields before snapshot materialization.
4. Keep references (`event_id`, `artifact_id`, `correlation_id`) for drill-down.

## Observability Events

Emit at least:
1. `conductor.snapshot.generated`
2. `conductor.snapshot.truncated`
3. `conductor.snapshot.stale`
4. `conductor.wake.received` (with snapshot metadata only)

These events support debugging without turning snapshot into an unbounded log dump.

## Acceptance Criteria

1. Every conductor wake includes a typed `agent_tree_snapshot`.
2. Snapshot generation does not poll child actors.
3. Snapshot truncation is deterministic for identical inputs.
4. Conductor accepts concurrent human input while child work is in-flight.
5. Snapshot metadata is queryable in logs/events for post-run audit.
