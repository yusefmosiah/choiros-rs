# Local-First Reliability and OVH Cutover Plan

Date: 2026-02-26
Status: Active plan
Owner: platform/runtime

## Narrative Summary (1-minute read)

AWS EC2 runtime is retired. Current production-like iteration is local-first.
Bootstrap definition for this plan: Choir is the coding agent used to build Choir.
Prerequisite: bootstrap begins only after local 3-tier architecture stability.
The next stable path is:

1. Make provider gateway + model matrix reliable locally.
2. Lock git safety for live runtime changes (commit/rollback + traceability).
3. Stand up one OVH dev node with the same runtime contract.
4. Promote that node architecture to two-node active/passive.
5. Then continue product work (marginalia/writer UX) on that same runtime.

There is no active `grind -> prod` lane and no active AWS SSM lane.

## What Changed

1. Declared one canonical execution path: local validation first, then OVH bring-up.
2. Deprecated AWS/grind release runbooks as active operator guidance.
3. Made provider matrix and gateway/search key wiring explicit pre-cutover gates.

## What To Do Next

1. Pass local provider matrix (models + gateway search) with `failures=0`.
2. Lock secrets flow (`.env -> sops -> rendered env -> hypervisor`) for all required keys.
3. Stand up one OVH node and prove full login -> desktop -> prompt execution loop.
4. Add second OVH node and prove failover + rollback.

## Active Gates

## Gate 1: Local Reliability (blocking)

- [ ] `docs/runbooks/local-provider-matrix-validation.md` passes fully.
- [ ] Kimi lane verified in both conductor bootstrap and delegated writer flow.
- [ ] Search provider keys present and verified through gateway:
  - [ ] `TAVILY_API_KEY`
  - [ ] `BRAVE_API_KEY`
  - [ ] `EXA_API_KEY`
- [ ] No sandbox raw provider key exposure beyond approved boundary.
- [ ] Regression notes captured in `docs/reports/`.

## Gate 2: Secrets and Config Contract (blocking)

- [ ] Canonical secret names documented and consistent across:
  - [ ] local `.env`
  - [ ] sops entries
  - [ ] `/run/secrets/rendered/*` materialization
  - [ ] hypervisor runtime env
- [ ] Hypervisor provider gateway token lifecycle documented (create/rotate/revoke).
- [ ] Startup guard exists for missing required gateway/search keys.

## Gate 3: OVH Single-Node Bring-Up (blocking)

- [ ] NixOS host converges reproducibly from repo.
- [ ] Hypervisor healthy and role routing works.
- [ ] One user flow passes:
  - [ ] login
  - [ ] desktop load
  - [ ] prompt execution with tool/provider call
- [ ] Playwright smoke passes from Mac.

## Gate 4: OVH Two-Node Hardening (blocking before external launch)

- [ ] Node A/B have identical module graph; only host inputs differ.
- [ ] Active/passive traffic handoff tested.
- [ ] Rollback to previous generation tested and timed.
- [ ] Incident checklist written and tested once.

## Gate 5: Product Velocity Lane (after platform gates)

- [ ] Runtime model defaults updated for current roles:
  - [ ] conductor -> Kimi
  - [ ] writer -> Kimi
  - [ ] terminal -> Claude Haiku
  - [ ] researcher -> GLM-4.7
- [ ] Runtime model override UX documented (safe runtime changes without prompt fear).
- [ ] Marginalia writer UX backlog converted into execution checklist.

## Canonical Active Documents

1. `docs/architecture/roadmap-dependency-tree.md`
2. `docs/architecture/2026-02-20-bootstrap-execution-checklists.md`
3. `docs/runbooks/local-provider-matrix-validation.md`
4. `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
5. `docs/runbooks/platform-secrets-sops-nix.md`

## Deprecated as Active Runbooks

These remain historical references only:

- `docs/runbooks/deployment-current-and-cutover.md`
- `docs/runbooks/grind-to-prod-release-flow.md`
- `docs/runbooks/mac-ssh-release-flow.md`
