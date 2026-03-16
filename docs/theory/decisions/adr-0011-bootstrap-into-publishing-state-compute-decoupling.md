# ADR-0011: Bootstrap Into Publishing (State/Compute Decoupling + Runtime Modes)

Date: 2026-03-02
Kind: Decision
Status: Proposed
Priority: 3
Requires: [ADR-0014, ADR-0027]
Owner: Platform / Runtime / Product

## Narrative Summary (1-minute read)

**Note:** The publishing model is further refined by ADR-0027, which reframes
publishing as a graph operation (promote subgraph from per-user KB to global
KB), not a file export. The state/compute decoupling described here still
holds; ADR-0027 adds the knowledge graph dimension.

ChoirOS bootstrap target is publishing, not always-on per-user compute.

Hard invariants:

1. User code must never disrupt platform services.
2. Users must never violate multitenant bounds.

Decision direction:

1. Decouple persistent user state from elastic compute.
2. Use ADR-0014 machine classes plus a shared worker pool instead of fixed always-on per-user compute.
3. Make publishing first-class:
   1. Immutable published revisions/projections for read concurrency.
   2. `stable` and `candidate` refs with instant rollback.
   3. Prompt-enabled read mode that writes to publisher-owned candidate state via queued intents.
4. Reconcile inbound intent queues on scheduled headless publisher wakes (hourly default).

This gives lower baseline cost, better isolation, and faster path to real user validation.

## What Changed

- 2026-03-15: Clarified scope after ADR-0027 and ADR-0014.
  1. ADR-0027 owns the knowledge-graph interpretation of publishing
     (subgraph promotion from private KB to global KB).
  2. ADR-0011 remains the runtime/control-plane decision for decoupling state
     from compute, runtime modes, reconcile flow, and rollback semantics.
  3. Replaced superseded compute-tier terminology with ADR-0014 machine
     classes and shared worker pool language.
- 2026-03-11: Compute tier terminology (`lite`, `standard`, `heavy`) superseded
  by ADR-0014 machine classes. Machine classes are runtime configuration with
  account tier mapping. Publishing concepts remain valid and are the next
  major milestone after bootstrap.
- 2026-03-02: Initial ADR.
  1. Declared "bootstrap into publishing" as the next product/infra milestone after OVH bootstrap.
  2. Defined state/compute decoupling model for runtime architecture.
  3. Defined publish runtime modes and inbound intent reconciliation flow.
  4. Defined an 80/20 API and control-plane rails for safe multitenant operation.

## What To Do Next

1. Implement publish contract surface for graph promotion (`publish` + refs +
   fork + intent queue), with ADR-0027 owning the promoted-subgraph schema.
2. Map publish/reconcile workloads onto ADR-0014 machine classes and shared
   worker-pool policy.
3. Add headless publisher reconcile worker with hourly default cadence.
4. Add audit, privacy review, and observability for intent ingestion,
   reconcile decisions, promotions, and rollbacks.

## Context

Current state:

1. Local path is vfkit-first and production parity targets OVH/Linux backend parity under shared
   contracts.
2. Runtime lifecycle control is still minimal in implementation (`ensure|stop` path currently).
3. Prior wave plan puts publishing in post-bootstrap product expansion, after memory lane work.
4. ADR-0027 now defines publishing as subgraph promotion from private KB to
   global KB, so this ADR should focus on runtime and scheduling semantics,
   not file-export mechanics.

Observed planning issue:

1. Snapshot parking controls idle cost but does not provide background compute.
2. Always-on per-user VM allocation is expensive and mismatched to bursty workload classes.
3. Reader concurrency should not require mutable authoring compute.

## Decision

### 1) Product/Infra Milestone Order

After OVH single-node bring-up and bootstrap loop stabilization, next target is publishing
bootstrap:

1. Publish immutable revisions/projections from private authoring state.
2. Serve read concurrency cheaply.
3. Introduce controlled writeback to publisher state.

Memory/multimedia/live-audio lanes follow publishing bootstrap.

### 2) Authoritative Isolation Objective

All runtime and scheduling choices must preserve both:

1. Platform containment: user workloads cannot impact control-plane reliability.
2. Tenant isolation: no cross-tenant read/write/compute boundary violation.

If a cost optimization weakens either, reject it.

### 3) State/Compute Decoupling

Separate:

1. State plane (long-lived): per-user KB state, published revisions, history,
   refs, permissions, and audit records.
2. Compute plane (ephemeral): leased workers/microVMs selected by machine
   class and workload policy.

No permanent compute reservation per user is required.

### 4) Runtime Modes

1. `RW_OWNER`: mutable authoring mode.
2. `RO_PUBLISHED`: read-only published mode.
3. `RO_PUBLISHED_WITH_PROMPT`: reader prompts allowed with constrained write path.
4. `FORKED_RW`: private writable fork from published state.

### 5) Pointer Model

Each publish target maintains serving refs over its published history:

1. `stable` ref: currently serving version.
2. `candidate` ref: pending/promoted version.

Rollback is a ref flip from `stable` to a previous published revision. Under
ADR-0027, these refs layer on top of the global KB commit graph rather than a
standalone artifact store.

### 6) Inbound Prompt Writeback Model

Reader prompts in published mode do not write directly to `stable`.
They produce `inbound_intent` records scoped to
`(publisher_id, publish_id, target_node_id)`.

Reconcile flow:

1. Queue intents with idempotency keys.
2. Wake headless publisher compute on schedule (hourly default).
3. Apply intents into publisher-owned `candidate` state.
4. Run bounded validation policy.
5. Auto-promote or request publisher approval based on policy.
6. Notify publisher and emit audit trail.

### 7) Compute Allocation Policy

Baseline runtime shape:

1. Authoring state is long-lived but can park cold aggressively.
2. Publish/reconcile work runs on leased workers from the shared pool.
3. Worker selection uses ADR-0014 machine classes and queue policy.

Policy:

1. Start minimal.
2. Escalate to larger machine classes only on workload signals.
3. Return workers to the pool after completion/idle timeout.
4. Park cold state aggressively.

### 8) QA Strategy (80/20)

For publishable changes:

1. Fast preview path (`candidate` visible quickly).
2. Verified path (bounded smoke checks + optional browser tests).
3. Manual approval happy path by publisher.
4. Guaranteed revert path via ref rollback.

## Minimal API Surface (80/20)

Publisher and publish revision:

1. `POST /v1/publishes` (create publish projection from source subgraph/revision)
2. `POST /v1/publishes/{id}/promote` (candidate -> stable)
3. `POST /v1/publishes/{id}/rollback` (stable -> previous)
4. `POST /v1/publishes/{id}/fork` (fork published state to private RW)

Reader prompt writeback:

1. `POST /v1/publishes/{id}/intents` (enqueue inbound intent)
2. `GET /v1/publishes/{id}/intents` (publisher/reconciler view)
3. `POST /internal/publishers/{publisher_id}/reconcile` (apply queue to candidate)

Serving:

1. `GET /p/{publish_id}` (read-only runtime route)
2. `GET /p/{publish_id}/status` (stable/candidate/ref metadata)

## Control-Plane Rails

1. Capability tokens per worker lease and intent operation.
2. Deny-by-default network egress for worker classes.
3. No provider/user raw secrets in runtime workers.
4. Full audit trail for intent ingestion, reconciliation, promotion, rollback.
5. Quotas per tenant/publisher for queue depth and reconcile budget.

## Operational Topology (Bootstrap)

1. Two runtime nodes are sufficient for first experiment cohort.
2. Managed LB is acceptable for low-complexity ingress.
3. Control-plane service split to dedicated node(s) is staged, but logical service boundaries are
   required now.

## Validation Targets (Initial)

1. Warm new-user publish workspace available in seconds.
2. Candidate preview latency under one minute for small edits on default
   machine classes.
3. Reconcile loop drains queued intents within one hourly cycle under normal load.
4. Rollback to prior stable ref is immediate and user-visible.

## Risks

1. Intent queue abuse or spam without quota and moderation rails.
2. Reconcile conflicts for high-churn collaborative writes.
3. Over-allocation to larger machine classes can erase expected cost gains.
4. Weak audit coverage can hide policy violations.

## Consequences

### Positive

1. Better cost shape by default-minimal compute.
2. Better reader concurrency via immutable published revisions/projections.
3. Better reliability through ref-based promotion and rollback.

### Tradeoffs

1. Added product complexity in publish/intent/reconcile UX.
2. Need stronger policy + scheduler observability.
3. Requires explicit change-management semantics for collaborative writes.

## Repo References

1. `docs/archive/2026-02-28-wave-plan-local-to-ovh-bootstrap.md`
2. `docs/practice/decisions/adr-0007-3-tier-control-runtime-client-architecture.md`
3. `docs/practice/decisions/adr-0008-ovh-selfhosted-secrets-architecture.md`
4. `docs/archive/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md`
5. `hypervisor/src/sandbox/mod.rs`
6. `hypervisor/src/bin/vfkit-runtime-ctl.rs`
