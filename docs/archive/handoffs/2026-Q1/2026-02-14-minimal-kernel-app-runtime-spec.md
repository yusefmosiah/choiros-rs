# Minimal Kernel/App Runtime Spec

Date: 2026-02-14  
Status: Proposed and implementation-authoritative once accepted  
Scope: Conductor runtime simplification, app-level orchestration, shared worker execution, writer-driven canon

## Narrative Summary (1-minute read)

The simplification insight is this:
state should track obligations and ownership, not reasoning.

Today the runtime stores too many planning states and repeatedly re-enters model decisions.
This makes the system hard to reason about and easy to loop.

The target architecture is a strict split:
1. Kernel level (`Conductor`) is deterministic execution and state transition authority.
2. App level (`WriterApp`) is semantic authority for document evolution and replanning.
3. Shared workers execute typed work and emit typed patches.
4. The canonical document is produced only by app-level revision creation.

This removes policy/bootstrap/finalize orchestration complexity as first-class runtime layers.
It keeps only minimal state machines and typed interfaces.
Cutover mode is single-path and in-place: no runtime feature flags and no long-lived dual-path execution.

## What Changed

1. Defined a minimal persistent state model with five record types only: `Run`, `WorkItem`, `Patch`, `Revision`, `EventLog`.
2. Defined kernel/app boundary with explicit authority and no natural-language control flow.
3. Replaced conductor replanning loops with app-driven replanning via typed `plan_delta`.
4. Defined a generic app interface, not a writer-specific syscall.
5. Reduced run lifecycle state to minimal liveness/safety transitions.
6. Declared wake/display events as observability transport, not policy triggers.

## What To Do Next

1. Implement typed state types and storage migration.
2. Implement kernel scheduler with leasing and deterministic transitions.
3. Implement app turn contract and writer app loop.
4. Route worker and user outputs into typed patch events.
5. Remove legacy task-state, policy, and conductor decision loop paths.
6. Add invariant-focused integration tests and cut over behind a feature flag.

---

## 1) Problem Statement

Current orchestration is over-abstracted for the actual product goal.
It duplicates control responsibilities across:
1. bootstrap planning
2. policy adaptation
3. repeated conductor decisions
4. finalize stage logic
5. wake/event-triggered decision paths

Result:
1. Excess model turns
2. ambiguous authority boundaries
3. loop-prone behavior
4. poor legibility for operators and implementers

Product goal:
1. multi-agent collaboration
2. shared worker services
3. living document evolution
4. user and worker patch integration
5. robust and debuggable runtime behavior

## 2) Design Insight (Authoritative)

The runtime should persist only facts needed for correctness:
1. work obligations
2. ownership/leases
3. immutable outputs
4. canonical revision pointer

The runtime should not persist model thought-process phases as state authority.

Corollary:
1. planning belongs to app-level agents
2. execution belongs to kernel-level scheduler
3. canon synthesis belongs to app-level revision creators

## 3) Layer Model

### Kernel Level (`Conductor`)

Responsibilities:
1. run lifecycle and state transitions
2. work queue + leasing
3. dispatch to shared workers
4. patch/revision/event persistence
5. invariant enforcement

Not allowed:
1. semantic replanning
2. direct canon synthesis
3. ad hoc routing by natural-language phrase matching

### App Level (`WriterApp` now, other apps later)

Responsibilities:
1. consume run context and new facts
2. decide missing information/work
3. request new work through typed commands
4. create new canonical revisions from patch sets
5. close run when objective is satisfied or blocked

Not allowed:
1. mutate kernel state outside typed actions
2. bypass worker dispatch or lease rules

### Shared Workers

Responsibilities:
1. execute assigned capability work
2. emit typed outputs and patches
3. report terminal status

Not allowed:
1. run orchestration
2. mutate canon directly
3. spawn ad hoc control workflows

## 4) Minimal Persistent State

### `Run`

Fields:
1. `run_id`
2. `app_id`
3. `objective`
4. `status`: `active | closed`
5. `close_reason`: `completed | blocked | failed | canceled | null`
6. `head_revision_id | null`
7. `created_at`
8. `updated_at`

### `WorkItem`

Fields:
1. `work_id`
2. `run_id`
3. `capability`
4. `objective`
5. `depends_on: work_id[]`
6. `status`: `queued | leased | completed | failed | blocked | canceled`
7. `lease_owner | null`
8. `lease_expires_at | null`
9. `retry_count`
10. `created_at`
11. `updated_at`

### `Patch`

Fields:
1. `patch_id`
2. `run_id`
3. `author_kind`: `worker | user | app`
4. `author_id`
5. `source_work_id | null`
6. `base_revision_id | null`
7. `ops` (typed patch operations)
8. `applied_in_revision_id | null`
9. `created_at`

### `Revision`

Fields:
1. `revision_id`
2. `run_id`
3. `parent_revision_id | null`
4. `created_by_app_id`
5. `applied_patch_ids: patch_id[]`
6. `rejected_patch_ids: patch_id[]`
7. `document_ref` (path/hash)
8. `created_at`

### `EventLog`

Fields:
1. `seq`
2. `run_id`
3. `event_type`
4. `payload` (typed JSON)
5. `actor_id`
6. `created_at`

## 5) Minimal State Machines

### Run State Machine

Transitions:
1. `active -> closed`

Notes:
1. There are no intermediate semantic phases (`bootstrap`, `finalize`, etc.) in authoritative state.

### WorkItem State Machine

Transitions:
1. `queued -> leased`
2. `leased -> completed`
3. `leased -> failed`
4. `leased -> blocked`
5. `leased -> queued` on lease timeout with retry
6. `queued -> canceled` when run closes
7. `leased -> canceled` when run closes

### Revision Chain

Rules:
1. immutable append-only
2. exactly one `head_revision_id` per run
3. parent pointer must match current head at commit time

## 6) Generic App Interface (Not Writer-Specific)

Kernel invokes app turns with typed input.
App replies with typed actions.

### `AppTurnInput`

Fields:
1. `run_id`
2. `objective`
3. `head_revision_id | null`
4. `new_events_since_seq`
5. `unapplied_patch_ids`
6. `open_work_summary`

### `AppTurnActions`

Actions:
1. `request_work { capability, objective, depends_on, idempotency_key }`
2. `create_revision { parent_revision_id, applied_patch_ids, rejected_patch_ids, document_ref }`
3. `close_run { reason }`
4. `noop`

Kernel applies action batch atomically and emits events.

## 7) Scheduling and Execution Model

### Deterministic Kernel Scheduler

Algorithm:
1. select `queued` work with satisfied dependencies
2. lease to matching worker
3. enforce lease timeout and retry budget
4. persist worker terminal result
5. persist worker-produced patches
6. trigger app turn on new facts

No semantic decisions occur in scheduler.

### App-Driven Replanning

Replanning is represented only as additional `request_work` actions from app turns.

This enables cross-capability planning without conductor policy loops.

## 8) Document and Patch Authority

Rules:
1. workers and users submit patches, not canon overwrites
2. app creates revisions from patch sets
3. kernel updates `head_revision_id` only on valid revision commit
4. canonical document is always `head_revision.document_ref`

## 9) Invariants (Must Hold)

1. At most one active lease per `work_id`.
2. A patch can be applied in at most one revision.
3. `create_revision.parent_revision_id` must equal run head at commit time.
4. App turns are serialized per run.
5. `run.status=closed` implies no non-terminal work remains, unless `close_reason=failed|canceled` force-close.
6. All writes are run-scoped (`run_id`) and actor-scoped (`actor_id`).

## 10) Failure and Recovery Semantics

1. Worker crash:
   - lease expires
   - work returns to `queued` until retry budget exhausted
2. App crash:
   - no state loss, replay from `EventLog` + current tables
   - next app turn resumes from latest `seq`
3. Kernel crash:
   - reload from persisted state
   - reconcile expired leases
   - resume scheduler

## 11) Observability Contract

Observability is append-only event transport and debug source.
It is not control authority.

Required event families:
1. `run.started | run.closed`
2. `work.requested | work.leased | work.completed | work.failed | work.blocked | work.canceled`
3. `patch.submitted | patch.applied | patch.rejected`
4. `revision.created | revision.head_changed`
5. `app.turn.started | app.turn.completed | app.turn.failed`

Required correlation fields:
1. `run_id`
2. `work_id` when applicable
3. `patch_id` when applicable
4. `revision_id` when applicable
5. `actor_id`

## 12) Non-Goals

1. No writer-specific kernel syscall design.
2. No policy actor indirection for orchestration.
3. No natural-language authority routing in kernel.
4. No duplicate legacy task/run state model.

## 13) Migration Plan (Implementation Slices)

Cutover rule:
1. No feature flags for runtime authority.
2. No parallel control-plane implementations in production path.
3. Land slices in sequence, but each slice edits the primary path directly.

### Slice 1: Data Model Cutover

1. Introduce `Run`, `WorkItem`, `Patch`, `Revision` typed records in `shared-types`.
2. Add persistence and serializers.
3. Remove legacy writes in the same slice; no compatibility write path.

### Slice 2: Kernel Scheduler

1. Implement queue/lease/timeout/retry mechanics.
2. Route worker calls through work leases only.
3. Remove conductor decision-loop dependence for dispatch.

### Slice 3: App Turn Interface

1. Add `AppTurnInput` and `AppTurnActions` contracts.
2. Implement writer app adapter on top of contract.
3. Trigger turns from new-fact events.

### Slice 4: Patch/Revision Authority

1. Convert worker outputs to typed patches.
2. Convert user document edits to typed patches.
3. Gate canon updates to `create_revision` action only.

### Slice 5: Remove Legacy Control Paths

1. Remove legacy `ConductorTaskState` path.
2. Remove `policy` indirection and BAML conductor decision loop.
3. Remove `finalize` orchestration stage.
4. Remove wake-triggered policy decisions.

### Slice 6: Test Gates

1. Invariant tests for leases, patch application uniqueness, revision parent checks.
2. Multi-run isolation tests on event and state scopes.
3. Recovery tests for worker/app/kernel crash scenarios.
4. Loop-regression test: no repeated planner invocation for same unresolved state.

## 14) Acceptance Criteria

1. Kernel can run and close runs using only minimal state tables and deterministic scheduling.
2. App can replan cross-capability work by typed `request_work` actions.
3. Canon updates occur only through revision creation.
4. Worker and user updates converge through unified patch flow.
5. No policy/bootstrap/finalize orchestration layers remain as runtime authorities.
6. Docs and tests can explain runtime behavior without referencing hidden prompt logic.

## 15) Immediate Mapping to Current Code

Likely removal or deep rewrite targets:
1. `sandbox/src/actors/conductor/policy.rs`
2. `sandbox/src/actors/conductor/runtime/decision.rs`
3. `sandbox/src/actors/conductor/runtime/finalize.rs`
4. legacy task path in `sandbox/src/actors/conductor/state.rs`

Likely retention targets with changed contracts:
1. `sandbox/src/actors/run_writer/*` as basis for revision authority
2. worker execution ports (`researcher`, `terminal`) as shared services
3. event store / observability paths with updated event taxonomy

## 16) Implementation Discipline

1. Enforce typed contracts first, then remove legacy logic.
2. Make each slice shippable and test-gated.
3. Reject abstraction that does not reduce authority ambiguity.
4. If behavior cannot be expressed by state transitions + typed actions, it is out of kernel scope.
5. Do not hide runtime behavior changes behind feature flags.
