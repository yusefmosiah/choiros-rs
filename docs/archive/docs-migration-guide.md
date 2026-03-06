# Docs Migration Guide: v1 → Kanban Architecture

Date: 2026-03-06
Kind: Guide
Status: Active

## Overview

Migrate from flat `docs/architecture/` + scattered genre dirs to the kanban
four-directory model (`canon/`, `active/`, `state/`, `archive/`).

## Phase 1: Create Structure

- [ ] Create `docs/canon/decisions/`, `docs/canon/guides/`, `docs/canon/reports/`
- [ ] Create `docs/active/decisions/`, `docs/active/guides/`, `docs/active/notes/`
- [ ] Create `docs/state/snapshots/`, `docs/state/reports/`
- [ ] Keep `docs/archive/` as-is

## Phase 2: Migrate Accepted ADRs to `canon/decisions/`

These have Status: Accepted or In Progress (implemented):

- [ ] `adr-0001-eventstore-eventbus-reconciliation.md` → `canon/decisions/`
- [ ] `adr-0007-3-tier-control-runtime-client-architecture.md` → `canon/decisions/`
- [ ] `adr-0008-ovh-selfhosted-secrets-architecture.md` → `canon/decisions/`
- [ ] `adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md` → `canon/decisions/`

## Phase 3: Migrate Draft/Proposed ADRs to `active/decisions/`

- [ ] `adr-0002-rust-nix-build-and-cache-strategy.md` → `active/decisions/`
- [ ] `adr-0003-hypervisor-sandbox-secrets-boundary.md` → `active/decisions/`
- [ ] `adr-0004-hypervisor-sandbox-ui-runtime-boundary.md` → `active/decisions/`
- [ ] `adr-0005-alm-harness-integration.md` → `active/decisions/`
- [ ] `adr-0006-prompt-centralization-baml.md` → `active/decisions/`
- [ ] `adr-0009-terminal-renderer-strategy-xterm-vs-libghostty.md` → `active/decisions/`
- [ ] `adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md` → `active/decisions/`
- [ ] `adr-0011-bootstrap-into-publishing-state-compute-decoupling.md` → `active/decisions/`
- [ ] `adr-0013-fleet-ctl-change-lifecycle-and-promotion.md` → `active/decisions/`
- [ ] `adr-0014-per-user-storage-and-desktop-sync.md` → `active/decisions/`
- [ ] `adr-0015-docs-kanban-architecture.md` (already in `active/decisions/`)

## Phase 4: Migrate Guides

Operational (reference for existing systems) → `canon/guides/`:
- [ ] `runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`
- [ ] `runbooks/ovh-config-and-deployment-entrypoint.md`
- [ ] `runbooks/platform-secrets-sops-nix.md`
- [ ] `runbooks/local-provider-matrix-validation.md`
- [ ] `runbooks/vfkit-local-proof.md`

Prescriptive (plans/checklists for in-progress work) → `active/guides/`:
- [ ] `runbooks/2026-03-05-deployment-checkpoint-and-next-steps.md`
- [ ] `runbooks/ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
- [ ] `runbooks/dev-chatgpt-codex-auth-bridge.md`

## Phase 5: Migrate Execution Artifacts to `state/`

Snapshots → `state/snapshots/`:
- [ ] `checkpoints/2026-03-06-writer-tracing-bootstrap-checkpoint.md`
- [ ] `handoffs/2026-02-28-local-nixos-builder-vm-setup.md`
- [ ] `handoffs/2026-03-04-231800-ovh-bootstrap-current-status.md`

Reports → `state/reports/`:
- [ ] `reports/2026-02-26-022407-provider-matrix-kimi.md`
- [ ] `reports/2026-02-26-022532-provider-matrix-all-models.md`
- [ ] `reports/2026-02-26-local-cutover-step1.md`
- [ ] `reports/conductor-intelligence-2026-02-10.md`

## Phase 6: Triage Remaining Architecture Docs

These need case-by-case judgment:

To `active/notes/` (explorations, not yet decisions):
- [ ] `architecture/2026-03-06-docs-v2-problem-framing.md`
- [ ] `architecture/mind-map-ui-research.md`
- [ ] `architecture/roadmap-critical-analysis.md`
- [ ] `architecture/roadmap-dependency-tree.md`
- [ ] `architecture/pi_agent_rust_integration_analysis.md`

To `canon/guides/` (reference for existing systems):
- [ ] `architecture/actor-network-orientation.md`
- [ ] `architecture/files-api-contract.md`
- [ ] `architecture/writer-api-contract.md`
- [ ] `architecture/model-provider-agnostic-runbook.md`

To `active/guides/` (in-progress build plans):
- [ ] `architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md`
- [ ] `architecture/2026-02-28-3-tier-gap-closure-plan.md`
- [ ] `architecture/2026-02-28-cutover-stocktake-and-pending-work.md`
- [ ] `architecture/2026-02-28-local-cutover-status-and-next-steps.md`
- [ ] `architecture/2026-02-28-local-vfkit-architecture-review.md`
- [ ] `architecture/directives-execution-checklist.md`

To `archive/` (completed or superseded):
- [ ] `architecture/supervision-cutover-handoff.md` (cutover complete)
- [ ] `architecture/2026-02-28-voice-layer-gemini-hackathon-plan.md` (one-off event)
- [ ] `architecture/logging-watcher-architecture-design.md` (if superseded)
- [ ] `architecture/event-schema-design-report.md` (completed design)

## Phase 7: Triage Root-Level Docs

- [ ] `ARCHITECTURE_SPECIFICATION.md` → `canon/` or `archive/` (check if current)
- [ ] `AUTOMATIC_COMPUTER_ARCHITECTURE.md` → `canon/` or `active/notes/`
- [ ] `CHOIR_MULTI_AGENT_VISION.md` → `canon/` or `active/notes/`
- [ ] `TESTING_STRATEGY.md` → `canon/guides/` if current
- [ ] `DESKTOP_ARCHITECTURE_DESIGN.md` → check currency
- [ ] `DOCUMENTATION_UPGRADE_PLAN.md` → `archive/` (superseded by ADR-0015)
- [ ] `BUGFIXES_AND_FEATURES.md` → `archive/` or `state/`
- [ ] Remaining loose files → case-by-case into `active/notes/` or `archive/`

## Phase 8: Atlas & References

- [ ] Move NARRATIVE_INDEX to `archive/` (superseded by ATLAS.md)
- [ ] Verify `just atlas` generates correct output
- [ ] Update CLAUDE.md to reference `docs/ATLAS.md` instead of NARRATIVE_INDEX
- [ ] Update any cross-references between docs
- [ ] Verify no broken links

## Phase 9: Add Frontmatter

- [ ] Every doc in `canon/` and `active/` gets `Kind:` and `Status:` frontmatter
- [ ] Spot-check: no Draft docs in `canon/`, no Accepted docs in `active/`

## After Migration

This guide moves to `canon/guides/` as reference for future doc filing.
