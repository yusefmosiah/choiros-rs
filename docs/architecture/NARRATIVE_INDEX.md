# ChoirOS Narrative Index (Read This First)

Date: 2026-03-03
Purpose: single entry point for active architecture and execution docs.

## 60-Second Story

ChoirOS is in a platform reset phase.
Legacy cloud deployment lanes are no longer active.
The active plan is local-first reliability, then OVH US-East bootstrap with the same runtime
contract, strict control-plane secrets boundaries, and phased compute lifecycle expansion.

Use the docs below in order. If a doc is not listed in `Active Read Order`, treat it as reference history.

## Start Local Now (Canonical)

If you just need Choir running locally right now:

```bash
just local-build-ui
just dev
just dev-status
```

Open:
- `http://127.0.0.1:9090/login`

Stop:

```bash
just stop
```

Detailed runbook:
- `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`

## Active Read Order

### Architecture Decision Records (ADRs) - Canonical

1. `docs/architecture/adr-0007-3-tier-control-runtime-client-architecture.md` - **3-Tier Architecture (In Progress)**
2. `docs/architecture/adr-0001-eventstore-eventbus-reconciliation.md` - EventStore/EventBus (Accepted)
3. `docs/architecture/adr-0005-alm-harness-integration.md` - ALM Harness (Draft - decision pending)
4. `docs/architecture/adr-0006-prompt-centralization-baml.md` - Prompt Centralization (Draft - decision pending)
5. `docs/architecture/adr-0008-ovh-selfhosted-secrets-architecture.md` - OVH Self-Hosted Secrets (Accepted)
6. `docs/architecture/adr-0009-terminal-renderer-strategy-xterm-vs-libghostty.md` - Terminal Renderer Strategy (Proposed)
7. `docs/architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md` - OVH VM Fleet Bootstrap Capacity + Lifecycle API (Proposed)
8. `docs/architecture/adr-0011-bootstrap-into-publishing-state-compute-decoupling.md` - Bootstrap Into Publishing: State/Compute Decoupling + Runtime Modes (Proposed)
9. `docs/architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md` - OVH US-East Bootstrap Secrets + Two-Node Compute Lifecycle (Accepted)
10. `docs/architecture/adr-0013-fleet-ctl-change-lifecycle-and-promotion.md` - **Fleet-Ctl, Change Lifecycle, User-to-Global Promotion (Draft)**
11. `docs/architecture/adr-0014-per-user-storage-and-desktop-sync.md` - **Per-User Storage Isolation and Desktop Sync (Draft)**

### Execution Plans (Feb 28, 2026)

10. `docs/architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md` - Wave plan
11. `docs/architecture/2026-02-28-local-vfkit-architecture-review.md` - Vfkit review
12. `docs/architecture/2026-02-28-cutover-stocktake-and-pending-work.md` - Cutover status
13. `docs/architecture/2026-02-28-local-cutover-status-and-next-steps.md` - Local next steps
14. `docs/architecture/2026-02-28-3-tier-gap-closure-plan.md` - Gap closure plan
15. `docs/architecture/roadmap-dependency-tree.md` - Dependency tree

### Checkpoints

16. `docs/checkpoints/2026-03-06-writer-tracing-bootstrap-checkpoint.md` - **Latest: writer bugs, tracing gap, deploy status**

### Runbooks

17. `docs/runbooks/ovh-config-and-deployment-entrypoint.md`
18. `docs/runbooks/2026-03-05-deployment-checkpoint-and-next-steps.md` - **Current ops checklist**
18. `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`
19. `docs/runbooks/vfkit-local-proof.md`
20. `docs/runbooks/local-provider-matrix-validation.md`
21. `docs/runbooks/platform-secrets-sops-nix.md`
22. `docs/runbooks/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`

### Handoffs

22. `docs/handoffs/2026-02-28-local-nixos-builder-vm-setup.md`

## Current Decisions (Explicit)

- One active execution lane at a time.
- Model-led orchestration remains the control-flow rule.
- Conductor does not execute tools directly.
- Provider gateway is the security boundary for provider/search keys.
- Local validation gates must pass before OVH infra expansion.
- OVH service-account OAuth2 + Secret Manager/KMS is the default infra auth/secrets path.

## Deprecated as Active Operator Docs

These files were removed (available in git history if needed):

- `docs/runbooks/deployment-current-and-cutover.md` (removed 2026-03-05)
- `docs/runbooks/mac-ssh-release-flow.md` (removed 2026-03-05)

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
| 0008 | OVH Self-Hosted Secrets Architecture | Accepted | Platform / Runtime / Infra |
| 0009 | Terminal Renderer Strategy (xterm.js vs Ghostty/libghostty) | Proposed | Desktop / Runtime |
| 0010 | OVH Bootstrap VM Fleet Capacity and Minimal 80/20 Lifecycle API | Proposed | Platform / Runtime / Infra |
| 0011 | Bootstrap Into Publishing (State/Compute Decoupling + Runtime Modes) | Proposed | Platform / Runtime / Product |
| 0012 | OVH US-East Bootstrap Secrets and Two-Node Compute Lifecycle | Accepted | Platform / Runtime / Infra |
| 0013 | Fleet-Ctl, Change Lifecycle, and User-to-Global Promotion | Draft | Platform / Runtime / Product |
| 0014 | Per-User Storage Isolation and Desktop Sync | Draft | Platform / Runtime / Product |

### ADR Status Definitions

- **Draft**: Proposal under discussion, decision not yet made
- **Proposed**: Ready for review, seeking feedback
- **Accepted**: Decision made, implementation in progress or complete
- **Deprecated**: Decision reversed, feature removed
- **Superseded**: Replaced by a newer ADR
