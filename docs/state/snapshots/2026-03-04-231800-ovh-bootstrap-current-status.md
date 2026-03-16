# Handoff: OVH Bootstrap and Local Startup Documentation Status
Date: 2026-03-04
Kind: Snapshot
Status: Active
Requires: []

## Session Metadata
- Created: 2026-03-04 23:18:00
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~1 hour

### Recent Commits (for context)
  - c927637 Document OVH secrets lifecycle
  - 3dde966 Document OVH secrets bootstrap plan
  - 2475bde Update NARRATIVE_INDEX and ADR statuses post-review
  - 243d5af Add ADR-0007: 3-Tier Control/Runtime/Client Architecture
  - 055015c Add ADR-0005, ADR-0006, and voice layer plan

## Handoff Chain

- **Continues from**: [2026-02-28-local-nixos-builder-vm-setup.md](./2026-02-28-local-nixos-builder-vm-setup.md)
  - Previous title: Local NixOS Builder VM Setup Handoff - 2026-02-28
- **Supersedes**: None

## Current State Summary

Current work focused on documentation stabilization for OVH bootstrap and local bring-up after
recent command/runbook churn. A new comprehensive OVH entrypoint doc now exists and the top-level
architecture index points to it. Local startup is now documented prominently as `just local-build-ui`
then `just dev` and `just dev-status` on canonical ingress `http://127.0.0.1:9090/login`. Work is
left in a docs-ready state with four uncommitted docs changes (no code/runtime behavior changes).

## Codebase Understanding

## Architecture Overview

Current execution lane is local-first reliability -> OVH US-East two-node bootstrap -> publishing.
Canonical local topology is hypervisor ingress on `9090`; `3000` is direct sandbox/dev loop and not
the cutover path. OVH control-plane direction is service-account OAuth2 + Secret Manager + KMS.
Runtime lifecycle today is still `ensure|stop`, with ADR-defined target expansion to minimal 80/20
VM lifecycle (`create/start/stop/snapshot/restore/delete/get/list`).

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| docs/practice/guides/ovh-config-and-deployment-entrypoint.md | Single OVH bootstrap/deployment entrypoint | New comprehensive big-picture doc for re-onboarding |
| docs/ATLAS.md | Human-first docs index | Current successor to the historical `docs/archive/NARRATIVE_INDEX.md` |
| README.md | Repo landing doc | Updated to show canonical local startup path (`just dev`) |
| docs/archive/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md | Operator procedure for OVH secrets + lifecycle | Historical execution detail referenced by newer practice docs |
| docs/practice/decisions/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md | Accepted OVH bootstrap ADR | Decision authority for secrets and two-node lifecycle |
| Justfile | Current command source of truth | Confirms `just dev`/`dev-status`/`stop` are canonical now |

## Key Patterns Discovered

1. `docs/ATLAS.md` is now the top human-first index; this snapshot originally updated the predecessor `docs/archive/NARRATIVE_INDEX.md`.
2. Major architecture/roadmap docs require:
   - `Narrative Summary (1-minute read)`
   - `What Changed`
   - `What To Do Next`
3. Canonical local runtime is vfkit + hypervisor on `9090`; avoid anchoring new docs to legacy
   `dev-sandbox`/`dev-ui` patterns.
4. OVH account bootstrap scripting is intentionally local-only and gitignored by policy.
5. `docs/archive/*` is reference history, not active execution authority.

## Work Completed

## Tasks Finished

- [x] Created comprehensive OVH config/deployment entrypoint doc.
- [x] Linked the new entrypoint in the human-first index (`docs/archive/NARRATIVE_INDEX.md` at the time; now `docs/ATLAS.md`).
- [x] Added prominent canonical local startup section in `README.md`.
- [x] Added prominent canonical local startup section in the human-first index (`docs/archive/NARRATIVE_INDEX.md` at the time; now `docs/ATLAS.md`).
- [x] Verified local start commands against current `Justfile`.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| README.md | Replaced outdated sandbox-only quick start with canonical `just dev` startup on `9090` | Reduce operator confusion from command churn |
| docs/archive/NARRATIVE_INDEX.md | Added `Start Local Now (Canonical)` and inserted OVH entrypoint into runbook order | Historical predecessor to the current `docs/ATLAS.md` index |
| docs/practice/guides/ovh-config-and-deployment-entrypoint.md | New comprehensive OVH bootstrap/deployment entrypoint with map/checklist | Provide single big-picture re-onboarding doc |
| docs/state/snapshots/2026-03-04-231800-ovh-bootstrap-current-status.md | New handoff summarizing current status and next steps | Preserve session context and actionable continuation |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Add a dedicated OVH entrypoint doc | Keep info split across ADRs/runbooks only vs create a single entrypoint | Fast re-onboarding and lower operator overhead |
| Promote local startup commands to top-level docs | Leave startup details only in runbooks vs surface in README/index | Recent Justfile churn made top-level clarity necessary |
| Keep OVH account setup automation local-only | Commit account setup scripts/config vs keep gitignored | Avoid committing account-specific operational artifacts/secrets risk |

## Pending Work

## Immediate Next Steps

1. ~~Create NixOS host configuration for x86_64-linux bare metal.~~ Done.
2. ~~Run `nixos-anywhere` to convert Ubuntu -> NixOS on both nodes.~~ Done.
3. ~~Verify NixOS boots and SSH works post-conversion.~~ Done.
4. ~~Deploy ChoirOS binaries and verify health checks.~~ Done.
5. Build and deploy UI assets (WASM frontend via `dx build`).
6. Wire provider gateway credentials (API keys for LLM providers).
7. Set per-node hostnames (`choiros-a`, `choiros-b`).
8. Add Caddy reverse proxy for TLS termination and public access.
9. Run OVH bootstrap runbook Sections 1-3 (service-account, secrets, sync units).

## Blockers/Open Questions

- [ ] Confirm final LB + vRack choice for US-East deployment (cost/complexity target).
- [ ] Decide when to implement full lifecycle API beyond `ensure|stop` for snapshot/restore operations.
- [ ] Check whether OVH US has Secret Manager available (may be EU-only).

## Deferred Items

- Full fleet orchestration and horizontal scaling automation deferred until two-node bootstrap is stable.
- Publishing mode and read-only concurrency architecture deferred pending core deployment hardening.

## Context for Resuming Agent

## Important Context

**2026-03-05 progress (session 2 — NixOS install + deploy):**
- Both nodes converted from Ubuntu 24.04 to NixOS via `nixos-anywhere`.
- NixOS host config: `nix/hosts/ovh-node.nix` + `nix/hosts/ovh-node-disk-config.nix` (disko RAID1).
- Flake updated with `disko` input and `nixosConfigurations.choiros-ovh-node`.
- Sandbox and hypervisor built natively on both nodes (x86_64-linux flake outputs via crane).
- Systemd services added: `hypervisor.service` (:9090), `sandbox-live.service` (:8080), `sandbox-dev.service` (:8081).
- `scripts/ops/ovh-runtime-ctl.sh` added as bare metal stub (sandboxes are systemd-managed, not vfkit).
- All health checks passing on both nodes.

**SSH access (NixOS, root):**
```bash
ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94   # Node A
ssh -i ~/.ssh/id_ed25519_ovh root@147.135.70.196 # Node B
```

**Deploy workflow (current):**
```bash
# On each node:
cd /opt/choiros/workspace && git pull
nix build ./sandbox#sandbox --no-link --print-out-paths
nix build ./hypervisor#hypervisor --no-link --print-out-paths
install -m 0755 /nix/store/<hash>-sandbox-0.1.0/bin/sandbox /opt/choiros/bin/sandbox
install -m 0755 /nix/store/<hash>-hypervisor-0.1.0/bin/hypervisor /opt/choiros/bin/hypervisor
nixos-rebuild switch --flake .#choiros-ovh-node
```

**Key files created this session:**
- `nix/hosts/ovh-node.nix` — NixOS host config with systemd services
- `nix/hosts/ovh-node-disk-config.nix` — disko RAID1 disk layout
- `scripts/ops/ovh-runtime-ctl.sh` — bare metal runtime-ctl stub

## Assumptions Made

- AWS deployment paths remain intentionally out of scope (ADR-0008 direction).
- User wants low-complexity two-node OVH bootstrap first, before advanced fleet features.
- Local vfkit/hypervisor path is the baseline for validating deployment-shape behavior.

## Potential Gotchas

- Many legacy docs in archive/reference still mention `dev-sandbox`/`dev-ui`; do not treat those as canonical.
- `scripts/ops/ovh-account-setup.sh` is present locally but gitignored; do not assume it is committed.
- The new OVH entrypoint doc was originally created during this session and now lives at `docs/practice/guides/ovh-config-and-deployment-entrypoint.md`.
- The historical `docs/archive/NARRATIVE_INDEX.md` captured this session's index updates before `docs/ATLAS.md` became the active entrypoint.

## Environment State

## Tools/Services Used

- `just` for command source-of-truth verification.
- `rg`, `sed`, `ls`, `git status` for repo/doc inspection.
- `apply_patch` for doc edits.
- `skills/session-handoff/scripts/create_handoff.py` and `validate_handoff.py` for handoff workflow.

## Active Processes

- No long-running local dev processes were started in this session.

## Environment Variables

- `OVH_APPLICATION_KEY`
- `OVH_APPLICATION_SECRET`
- `OVH_CONSUMER_KEY`
- `OVH_OAUTH_ACCESS_TOKEN`
- `DEPLOY_HOST`
- `DEPLOY_USER`
- `DEPLOY_PORT`
- `SSH_KEY_PATH`

## Related Resources

- `docs/ATLAS.md`
- `docs/practice/guides/ovh-config-and-deployment-entrypoint.md`
- `docs/archive/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
- `docs/practice/decisions/adr-0008-ovh-selfhosted-secrets-architecture.md`
- `docs/archive/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md`
- `docs/theory/decisions/adr-0011-bootstrap-into-publishing-state-compute-decoupling.md`
- `docs/practice/decisions/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
- `docs/practice/guides/local-vfkit-nixos-miniguide.md`

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
