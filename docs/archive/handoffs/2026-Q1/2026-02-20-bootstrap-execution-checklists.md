# Bootstrap Execution Checklists (Choir Builds Choir)

Date: 2026-02-26
Status: active checklist
Owner: platform/runtime

## Narrative Summary (1-minute read)

For ChoirOS, `bootstrap` means using Choir itself as the coding agent to build Choir.
Bootstrap is only activated after the local 3-tier architecture is stable.
Sequence is strict:

1. Build and validate local 3-tier runtime.
2. Lock git workflow + CI/CD gates for safe live changes and rollback.
3. Activate and prove the Choir-on-Choir dev loop.
4. Build memory system to improve the agentic build loop.
5. Deploy to OVH using the same contract.

## What Changed

1. Replaced AWS-era bootstrap assumptions with local-first bootstrap order.
2. Added explicit memory-system phase before OVH rollout.
3. Added git/CI/CD phase before infrastructure promotion.

## What To Do Next

1. Complete Phase 1 and Phase 2 locally.
2. Do not start bootstrap until Phase 2 is complete.
3. Do not start OVH rollout until Phase 4 is complete.
4. Use this checklist with `docs/architecture/roadmap-dependency-tree.md`.
5. Keep execution aligned with `docs/architecture/2026-02-26-comprehensive-cutover-plan.md`.

## Global Definition of Done

- [ ] Local 3-tier runtime is stable and reproducible.
- [ ] Memory system works end-to-end with observable correctness.
- [ ] Git and CI/CD enforce clean, deterministic release behavior.
- [ ] OVH deployment reproduces local architecture with rollback safety.

## Phase 1: Local 3-Tier Runtime Stabilization

### Outcome

Local architecture is stable enough to serve as the substrate for Choir-on-Choir bootstrap.

### Gate

- [ ] Hypervisor control plane starts and routes reliably.
- [ ] User runtime boundary is enforced (VM/container isolation contract).
- [ ] `live` and `dev` sandbox surfaces run concurrently.
- [ ] login -> desktop -> prompt execution loop passes.

### Evidence

- [ ] Service/process health snapshots.
- [ ] E2E run artifact (Playwright or equivalent).
- [ ] Event trace confirming scoped routing and worker lifecycle.

## Phase 2: Git + CI/CD Safety Bootstrap

### Outcome

Code changes during live runtime are safe, traceable, and reversible.

### Gate

- [ ] Protected mainline path with required checks.
- [ ] Clean-tree enforcement for release builds.
- [ ] Provider matrix validation is a required gate.
- [ ] Artifact/version traceability is captured per release.
- [ ] Branching policy is defined and enforced (`branch-per-sandbox` default).
- [ ] Commit/rollback workflow is validated while runtime is live.

### Evidence

- [ ] CI config links and passing run IDs.
- [ ] Release checklist with commit SHA + artifact mapping.
- [ ] Rollback instruction validated once.
- [ ] Branch lifecycle spec documented (create, lock, merge, prune).
- [ ] One successful hot-change + rollback drill report.

### Branching Policy (Recommended)

1. Default unit is one branch per sandbox container, not per user.
2. A single user can own multiple sandbox branches for parallel experiments.
3. Each sandbox branch has metadata:
   - owner user id
   - sandbox id
   - base commit
   - TTL/expiry
4. Merge path is sandbox branch -> reviewed integration branch -> main.

### Local Orchestration Milestone (Before Phase 3)

1. Add `just` entrypoints for split local topology:
   - `dev-control-plane`
   - `dev-runtime-plane`
   - `dev-all`
   - `stop-all`
2. Add tmux script for service windows and log capture.
3. Make these commands the daily bootstrap loop.

## Phase 3: Choir-on-Choir Loop Bootstrap

### Outcome

Choir can modify, test, and validate Choir locally with traceable runs.

### Gate

- [ ] Core coding-agent loop works end-to-end (`request -> code change -> test -> report`).
- [ ] Failures are visible and actionable (no silent fallback path).
- [ ] Run traces clearly show plan/delegation/tool/result flow.

### Evidence

- [ ] At least one successful real feature/fix built by Choir on this repo.
- [ ] At least one failed run with useful debugging trace.
- [ ] Report artifact captured in `docs/reports/`.

## Phase 4: Local Memory System Bootstrap

### Outcome

Memory layer is functional and safe on top of local runtime.

### Gate

- [ ] Ingestion and retrieval work for expected task paths.
- [ ] Memory writes/reads are scoped correctly by user/session.
- [ ] Memory failures are observable and fail loudly (no silent fallback).

### Evidence

- [ ] Integration tests for memory ingestion/retrieval.
- [ ] Trace logs showing memory calls in real runs.
- [ ] Documented known limits and deferred items.

## Phase 5: OVH Deployment Bootstrap

### Outcome

OVH runtime matches validated local architecture.

### Gate

- [ ] Single OVH node passes full user flow.
- [ ] Two-node active/passive handoff passes.
- [ ] Rollback to prior generation passes.

### Evidence

- [ ] Host convergence logs.
- [ ] Public domain E2E artifacts.
- [ ] Failover and rollback drill results.

## Execution Rules

1. One active phase at a time.
2. No gate bypass.
3. Every gate requires evidence.
4. Failures become explicit defects before forward movement.
