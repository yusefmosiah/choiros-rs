# ChoirOS Roadmap Dependency Tree (Active)

Date: 2026-02-26
Status: Authoritative order
Owner: platform/runtime + app runtime

## Narrative Summary (1-minute read)

This is the single active dependency map.
Execution order is strict and blocking. We do not run AWS/grind deployment work.
We stabilize local runtime first, then carry the same contract to OVH.
Here, bootstrap means: Choir is used as the coding agent to build Choir, but only
after the local 3-tier architecture is stable.

## What Changed

1. Replaced mixed historical lanes with one active lane.
2. Removed AWS/grind release flow from active execution order.
3. Made provider/gateway reliability an explicit pre-infra gate.

## What To Do Next

1. Complete Gate 1 and Gate 2 (local reliability + secrets contract).
2. Bring up OVH single-node and pass end-to-end gate.
3. Expand to OVH two-node failover and rollback gate.

## Dependency Tree (Strict)

1. Gate 1: Local 3-tier architecture stabilization
   - Depends on: none
   - Blocks: all downstream infra work
2. Gate 2: Git + CI/CD safety
   - Depends on: Gate 1
   - Blocks: bootstrap and platform work
3. Gate 3: Choir-on-Choir bootstrap loop
   - Depends on: Gate 2
   - Blocks: memory and OVH bring-up
4. Gate 4: Local provider/runtime reliability
   - Depends on: Gate 3
   - Blocks: secrets and OVH bring-up
5. Gate 5: Secrets and gateway contract hardening
   - Depends on: Gate 4
   - Blocks: OVH bring-up
6. Gate 6: OVH single-node dev environment
   - Depends on: Gate 4, Gate 5
   - Blocks: two-node rollout
7. Gate 7: OVH two-node platform hardening
   - Depends on: Gate 6
   - Blocks: external launch and sustained product velocity
8. Gate 8: Product velocity on stable platform
   - Depends on: Gate 7

## Gate Checklist

## Gate 1: Local 3-tier architecture stabilization

- [ ] Hypervisor control plane routes reliably.
- [ ] Runtime boundary is enforced (`live`/`dev` in isolated user runtime).
- [ ] Login -> desktop -> prompt loop works with stable behavior.

## Gate 2: Git + CI/CD safety

- [ ] Commit/rollback workflow validated while runtime is live.
- [ ] Protected branch and required checks are active.
- [ ] Release artifacts map cleanly to commit SHA.
- [ ] Branch-per-sandbox policy is operational.

## Gate 3: Choir-on-Choir bootstrap loop

- [ ] Choir can perform `request -> code change -> test -> report` on this repo.
- [ ] At least one successful and one failed run are captured with usable traces.
- [ ] No hidden fallback masks orchestration errors.

## Gate 4: Local provider/runtime reliability

- [ ] `docs/runbooks/local-provider-matrix-validation.md` passes (`failures=0`).
- [ ] Kimi auth/headers/model config validated for conductor + writer paths.
- [ ] Gateway search passes for Tavily, Brave, Exa.
- [ ] Login -> desktop -> prompt loop works locally.

## Gate 5: Secrets and gateway contract hardening

- [ ] `.env -> sops -> rendered secrets -> runtime env` is documented and verified.
- [ ] Required key names are consistent in code + docs.
- [ ] Gateway token rotation flow documented and tested once.
- [ ] Missing-key errors fail fast and are observable.

## Gate 6: OVH single-node

- [ ] Host converges from repo config only.
- [ ] Hypervisor + sandbox services healthy.
- [ ] Public domain login and prompt execution validated.
- [ ] Playwright smoke from Mac passes.

## Gate 7: OVH two-node

- [ ] Identical module graph on both nodes.
- [ ] Active/passive handoff tested.
- [ ] Rollback to previous generation tested.
- [ ] Incident checklist and runbook verified.

## Gate 8: Product velocity

- [ ] Runtime model defaults documented and set.
- [ ] Runtime model override path documented.
- [ ] Marginalia writer UX backlog translated into execution checklist.
- [ ] Branch-per-sandbox workflow is active with TTL cleanup.

## Active Documents Only

1. `docs/architecture/2026-02-26-comprehensive-cutover-plan.md`
2. `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`
3. `docs/architecture/2026-02-20-bootstrap-execution-checklists.md`
4. `docs/runbooks/local-provider-matrix-validation.md`
5. `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
6. `docs/runbooks/platform-secrets-sops-nix.md`

## Historical References (Not Active)

- `docs/runbooks/deployment-current-and-cutover.md`
- `docs/runbooks/grind-to-prod-release-flow.md`
- `docs/runbooks/mac-ssh-release-flow.md`
- older architecture/handoff docs not listed above
