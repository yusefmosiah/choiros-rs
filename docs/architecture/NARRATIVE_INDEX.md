# ChoirOS Narrative Index (Read This First)

Date: 2026-02-28
Purpose: single entry point for active architecture and execution docs.

## 60-Second Story

ChoirOS is in a platform reset phase.
AWS/grind deployment lanes are no longer active.
The active plan is local-first reliability, then OVH microVM bring-up with the same runtime contract.

Use the docs below in order. If a doc is not listed in `Active Read Order`, treat it as reference history.

## Active Read Order

### Architecture Decision Records (ADRs) - Canonical

1. `docs/architecture/adr-0007-3-tier-control-runtime-client-architecture.md` - **3-Tier Architecture (In Progress)**
2. `docs/architecture/adr-0001-eventstore-eventbus-reconciliation.md` - EventStore/EventBus (Accepted)
3. `docs/architecture/adr-0005-alm-harness-integration.md` - ALM Harness (Draft - decision pending)
4. `docs/architecture/adr-0006-prompt-centralization-baml.md` - Prompt Centralization (Draft - decision pending)

### Execution Plans (Feb 28, 2026)

5. `docs/architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md` - Wave plan
6. `docs/architecture/2026-02-28-local-vfkit-architecture-review.md` - Vfkit review
7. `docs/architecture/2026-02-28-cutover-stocktake-and-pending-work.md` - Cutover status
8. `docs/architecture/2026-02-28-local-cutover-status-and-next-steps.md` - Local next steps
9. `docs/architecture/2026-02-28-3-tier-gap-closure-plan.md` - Gap closure plan
10. `docs/architecture/roadmap-dependency-tree.md` - Dependency tree

### Runbooks

11. `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`
12. `docs/runbooks/vfkit-local-proof.md`
13. `docs/runbooks/local-provider-matrix-validation.md`
14. `docs/runbooks/platform-secrets-sops-nix.md`

### Handoffs

15. `docs/handoffs/2026-02-28-local-nixos-builder-vm-setup.md`

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

## Architecture Decision Records (ADRs)

ADRs are the **canonical** architecture documents. They are living documents that are updated as implementation progresses. Do not archive ADRs unless the feature itself is deprecated.

### ADR Registry

| ADR | Title | Status | Owner |
|-----|-------|--------|-------|
| 0001 | EventStore/EventBus Reconciliation | Accepted | Core |
| 0002 | Rust + Nix Build Strategy | Draft | Platform |
| 0003 | Hypervisor-Sandbox Secrets Boundary | Draft | Security |
| 0004 | Hypervisor-Sandbox UI Runtime Boundary | Draft | Platform |
| 0005 | ALM Harness Integration | Draft | Core |
| 0006 | Prompt Centralization in BAML | Draft | DX |
| 0007 | 3-Tier Control/Runtime/Client Architecture | In Progress | Platform |

### ADR Status Definitions

- **Draft**: Proposal under discussion, decision not yet made
- **Proposed**: Ready for review, seeking feedback
- **Accepted**: Decision made, implementation in progress or complete
- **Deprecated**: Decision reversed, feature removed
- **Superseded**: Replaced by a newer ADR
