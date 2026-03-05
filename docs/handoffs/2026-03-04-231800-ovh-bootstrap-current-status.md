# Handoff: OVH Bootstrap and Local Startup Documentation Status

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
| docs/runbooks/ovh-config-and-deployment-entrypoint.md | Single OVH bootstrap/deployment entrypoint | New comprehensive big-picture doc for re-onboarding |
| docs/architecture/NARRATIVE_INDEX.md | Human-first architecture/docs index | Updated with prominent local startup commands and new OVH entrypoint link |
| README.md | Repo landing doc | Updated to show canonical local startup path (`just dev`) |
| docs/runbooks/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md | Operator procedure for OVH secrets + lifecycle | Still authoritative for execution details |
| docs/architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md | Accepted OVH bootstrap ADR | Decision authority for secrets and two-node lifecycle |
| Justfile | Current command source of truth | Confirms `just dev`/`dev-status`/`stop` are canonical now |

## Key Patterns Discovered

1. `NARRATIVE_INDEX.md` is the top human-first index and should include fast operator actions.
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
- [x] Linked the new entrypoint in `docs/architecture/NARRATIVE_INDEX.md`.
- [x] Added prominent canonical local startup section in `README.md`.
- [x] Added prominent canonical local startup section in `docs/architecture/NARRATIVE_INDEX.md`.
- [x] Verified local start commands against current `Justfile`.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| README.md | Replaced outdated sandbox-only quick start with canonical `just dev` startup on `9090` | Reduce operator confusion from command churn |
| docs/architecture/NARRATIVE_INDEX.md | Added `Start Local Now (Canonical)` and inserted OVH entrypoint into runbook order | Make current startup and OVH docs discoverable from one index |
| docs/runbooks/ovh-config-and-deployment-entrypoint.md | New comprehensive OVH bootstrap/deployment entrypoint with map/checklist | Provide single big-picture re-onboarding doc |
| docs/handoffs/2026-03-04-231800-ovh-bootstrap-current-status.md | New handoff summarizing current status and next steps | Preserve session context and actionable continuation |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Add a dedicated OVH entrypoint doc | Keep info split across ADRs/runbooks only vs create a single entrypoint | Fast re-onboarding and lower operator overhead |
| Promote local startup commands to top-level docs | Leave startup details only in runbooks vs surface in README/index | Recent Justfile churn made top-level clarity necessary |
| Keep OVH account setup automation local-only | Commit account setup scripts/config vs keep gitignored | Avoid committing account-specific operational artifacts/secrets risk |

## Pending Work

## Immediate Next Steps

1. Commit current docs changes (`README.md`, `NARRATIVE_INDEX.md`, OVH entrypoint, this handoff).
2. Run OVH bootstrap runbook Sections 1-3 (service-account identity, policy scope, secret seeding, host sync units).
3. Install/converge NixOS on both OVH SYS-1 nodes and complete first failover drill evidence.

## Blockers/Open Questions

- [ ] Confirm final LB + vRack choice for US-East deployment (cost/complexity target).
- [ ] Decide when to implement full lifecycle API beyond `ensure|stop` for snapshot/restore operations.
- [ ] Clean up remaining stale docs that still reference legacy `dev-sandbox` / `dev-ui` commands.

## Deferred Items

- Full fleet orchestration and horizontal scaling automation deferred until two-node bootstrap is stable.
- Publishing mode and read-only concurrency architecture deferred pending core deployment hardening.

## Context for Resuming Agent

## Important Context

There are currently four uncommitted docs changes only:
1. `README.md` (canonical local startup now documented).
2. `docs/architecture/NARRATIVE_INDEX.md` (new local startup block + OVH entrypoint link).
3. `docs/runbooks/ovh-config-and-deployment-entrypoint.md` (new comprehensive OVH entrypoint).
4. `docs/handoffs/2026-03-04-231800-ovh-bootstrap-current-status.md` (this handoff).

Current canonical local startup is:
1. `just local-build-ui`
2. `just dev`
3. `just dev-status`
4. Open `http://127.0.0.1:9090/login`

OVH bootstrap authority stack to follow:
1. `docs/architecture/adr-0008-ovh-selfhosted-secrets-architecture.md`
2. `docs/architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
3. `docs/runbooks/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
4. `docs/runbooks/ovh-config-and-deployment-entrypoint.md`

Known target hosts (from session context):
1. `ns1004307.ip-51-81-93.us` (`51.81.93.94`)
2. `ns106285.ip-147-135-70.us` (`147.135.70.196`)

## Assumptions Made

- AWS deployment paths remain intentionally out of scope (ADR-0008 direction).
- User wants low-complexity two-node OVH bootstrap first, before advanced fleet features.
- Local vfkit/hypervisor path is the baseline for validating deployment-shape behavior.

## Potential Gotchas

- Many legacy docs in archive/reference still mention `dev-sandbox`/`dev-ui`; do not treat those as canonical.
- `scripts/ops/ovh-account-setup.sh` is present locally but gitignored; do not assume it is committed.
- The new OVH entrypoint doc is currently untracked until committed.
- `NARRATIVE_INDEX.md` date header still says `2026-03-03`; content is newer but date was not bumped in this session.

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

- `docs/architecture/NARRATIVE_INDEX.md`
- `docs/runbooks/ovh-config-and-deployment-entrypoint.md`
- `docs/runbooks/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
- `docs/architecture/adr-0008-ovh-selfhosted-secrets-architecture.md`
- `docs/architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md`
- `docs/architecture/adr-0011-bootstrap-into-publishing-state-compute-decoupling.md`
- `docs/architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
- `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
