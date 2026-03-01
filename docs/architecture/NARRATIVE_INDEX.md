# ChoirOS Narrative Index (Read This First)

Date: 2026-02-28
Purpose: single entry point for active architecture and execution docs.

## 60-Second Story

ChoirOS is in a platform reset phase.
AWS/grind deployment lanes are no longer active.
The active plan is local-first reliability, then OVH microVM bring-up with the same runtime contract.

Use the docs below in order. If a doc is not listed in `Active Read Order`, treat it as reference history.

## Active Read Order

1. `docs/architecture/2026-02-28-local-vfkit-architecture-review.md`
2. `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`
3. `docs/architecture/2026-02-28-cutover-stocktake-and-pending-work.md`
4. `docs/architecture/2026-02-28-local-cutover-status-and-next-steps.md`
5. `docs/architecture/2026-02-28-3-tier-gap-closure-plan.md`
6. `docs/architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md`
7. `docs/runbooks/vfkit-local-proof.md`
8. `docs/architecture/roadmap-dependency-tree.md`
9. `docs/runbooks/local-provider-matrix-validation.md`
10. `docs/runbooks/platform-secrets-sops-nix.md`
11. `docs/handoffs/2026-02-28-local-nixos-builder-vm-setup.md`

## Current Decisions (Explicit)

- One active execution lane at a time.
- Model-led orchestration remains the control-flow rule.
- Conductor does not execute tools directly.
- Provider gateway is the security boundary for provider/search keys.
- Local validation gates must pass before OVH infra expansion.

## Deprecated as Active Operator Docs

These files remain for context, not current execution authority:

- `docs/runbooks/deployment-current-and-cutover.md`
- `docs/runbooks/grind-to-prod-release-flow.md`
- `docs/runbooks/mac-ssh-release-flow.md`

## Doc Readability Rule

Major docs must include:

- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`
