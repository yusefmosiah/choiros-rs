# ChoirOS Narrative Index (Read This First)

Date: 2026-02-26
Purpose: single entry point for active architecture and execution docs.

## 60-Second Story

ChoirOS is in a platform reset phase.
AWS/grind deployment lanes are no longer active.
The active plan is local-first reliability, then OVH microVM bring-up with the same runtime contract.

Use the docs below in order. If a doc is not listed in `Active Read Order`, treat it as reference history.

## Active Read Order

1. `docs/architecture/2026-02-27-writer-diffusion-architecture.md`
2. `docs/architecture/2026-02-27-agent-contract-hard-cutover.md`
2. `docs/architecture/2026-02-26-comprehensive-cutover-plan.md`
3. `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`
4. `docs/architecture/roadmap-dependency-tree.md`
5. `docs/architecture/2026-02-20-bootstrap-execution-checklists.md`
6. `docs/runbooks/local-provider-matrix-validation.md`
7. `docs/runbooks/platform-secrets-sops-nix.md`
8. `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
9. `docs/architecture/2026-02-17-codesign-runbook.md`

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
