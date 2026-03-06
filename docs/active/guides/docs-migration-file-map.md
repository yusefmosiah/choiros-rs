# Docs Migration File Map

Date: 2026-03-06
Kind: Guide
Status: Active
Requires: [ADR-0015]

## Current Tree (non-archive, non-research)

```
docs/
  ARCHITECTURE_SPECIFICATION.md
  AUTOMATIC_COMPUTER_ARCHITECTURE.md
  axum-migration-plan.md
  BUGFIXES_AND_FEATURES.md
  CHOIR_MULTI_AGENT_VISION.md
  content-viewer-research.md
  DESKTOP_ARCHITECTURE_DESIGN.md
  DOCUMENTATION_UPGRADE_PLAN.md
  multiresearchraw_sandbox_infra.md
  research-dioxus-architecture.md
  terminal-ui.md
  TESTING_STRATEGY.md
  theme-system-research.md
  websocket-implementation-review.md
  window-management-research.md
  active/
    decisions/
      adr-0015-docs-kanban-architecture.md
    guides/
      docs-migration-guide.md
    notes/
      2026-03-06-docs-kanban-model.md
  architecture/
    2026-02-28-3-tier-gap-closure-plan.md
    2026-02-28-cutover-stocktake-and-pending-work.md
    2026-02-28-local-cutover-status-and-next-steps.md
    2026-02-28-local-vfkit-architecture-review.md
    2026-02-28-voice-layer-gemini-hackathon-plan.md
    2026-02-28-wave-plan-local-to-ovh-bootstrap.md
    2026-03-06-docs-v2-problem-framing.md
    actor-network-orientation.md
    adr-0001-eventstore-eventbus-reconciliation.md
    adr-0002-rust-nix-build-and-cache-strategy.md
    adr-0003-hypervisor-sandbox-secrets-boundary.md
    adr-0004-hypervisor-sandbox-ui-runtime-boundary.md
    adr-0005-alm-harness-integration.md
    adr-0006-prompt-centralization-baml.md
    adr-0007-3-tier-control-runtime-client-architecture.md
    adr-0008-ovh-selfhosted-secrets-architecture.md
    adr-0009-terminal-renderer-strategy-xterm-vs-libghostty.md
    adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md
    adr-0011-bootstrap-into-publishing-state-compute-decoupling.md
    adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md
    adr-0013-fleet-ctl-change-lifecycle-and-promotion.md
    adr-0014-per-user-storage-and-desktop-sync.md
    backend-authoritative-ui-state-pattern.md
    directives-execution-checklist.md
    event-schema-design-report.md
    files-api-contract.md
    logging-watcher-architecture-design.md
    mind-map-ui-research.md
    model-provider-agnostic-runbook.md
    NARRATIVE_INDEX.md
    pdf-app-implementation-guide.md
    pi_agent_rust_integration_analysis.md
    ractor-supervision-best-practices.md
    researcher-search-dual-interface-runbook.md
    roadmap-critical-analysis.md
    roadmap-dependency-tree.md
    simplified-agent-harness.md
    supervision-cutover-handoff.md
    TRACING_UX_UPGRADE.md
    unified-agentic-loop-harness.md
    writer-api-contract.md
  checkpoints/
    2026-03-06-writer-tracing-bootstrap-checkpoint.md
  design/
    event_bus_ractor_design.md
    PDF_APP_FULL_IMPLEMENTATION.md
    PDF_APP_HALF_DAY_IMPLEMENTATION.md
    watcher-actor-architecture.md
  dev-blog/
    2026-02-01-why-agents-need-actors.md
    from-slop-to-signal-verified.md
  handoffs/
    2026-02-28-local-nixos-builder-vm-setup.md
    2026-03-04-231800-ovh-bootstrap-current-status.md
    README.md
  prompts/
    refactor-session-2026-02-10.md
  reports/
    2026-02-26-022407-provider-matrix-kimi.md
    2026-02-26-022532-provider-matrix-all-models.md
    2026-02-26-local-cutover-step1.md
    conductor-intelligence-2026-02-10.md
  runbooks/
    2026-02-28-local-vfkit-nixos-miniguide.md
    2026-03-05-deployment-checkpoint-and-next-steps.md
    dev-chatgpt-codex-auth-bridge.md
    local-provider-matrix-validation.md
    nix-setup.md
    ovh-config-and-deployment-entrypoint.md
    ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md
    platform-secrets-sops-nix.md
    vfkit-local-proof.md
  security/
    choiros-logging-security-report.md
```

## Proposed Tree

```
docs/
  canon/
    decisions/
      adr-0001-eventstore-eventbus-reconciliation.md        # Accepted
      adr-0007-3-tier-control-runtime-client-architecture.md # In Progress (implemented)
      adr-0008-ovh-selfhosted-secrets-architecture.md        # Accepted
      adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md # Accepted
    guides/
      local-vfkit-nixos-miniguide.md                         # how to run locally (exists)
      ovh-config-and-deployment-entrypoint.md                # how to deploy (exists)
      platform-secrets-sops-nix.md                           # how to manage secrets (exists)
      local-provider-matrix-validation.md                    # how to validate providers (exists)
      nix-setup.md                                           # how to set up nix (exists)
      vfkit-local-proof.md                                   # how to run vfkit proof (exists)
      actor-network-orientation.md                           # system reference: actor model
      model-provider-agnostic-runbook.md                     # system reference: provider gateway
      files-api-contract.md                                  # system reference: files API
      writer-api-contract.md                                 # system reference: writer API
      ractor-supervision-best-practices.md                   # system reference: supervision
      simplified-agent-harness.md                            # system reference: harness
      unified-agentic-loop-harness.md                        # system reference: harness v2
    reports/
      (empty — promote here only if durably useful)
  active/
    decisions/
      adr-0002-rust-nix-build-and-cache-strategy.md          # Draft
      adr-0003-hypervisor-sandbox-secrets-boundary.md        # Draft
      adr-0004-hypervisor-sandbox-ui-runtime-boundary.md     # Draft
      adr-0005-alm-harness-integration.md                    # Draft
      adr-0006-prompt-centralization-baml.md                 # Draft
      adr-0009-terminal-renderer-strategy-xterm-vs-libghostty.md # Proposed
      adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md # Proposed
      adr-0011-bootstrap-into-publishing-state-compute-decoupling.md # Proposed
      adr-0013-fleet-ctl-change-lifecycle-and-promotion.md   # Draft
      adr-0014-per-user-storage-and-desktop-sync.md          # Draft
      adr-0015-docs-kanban-architecture.md                   # Draft (already here)
    guides/
      deployment-checkpoint-and-next-steps.md                # active build checklist
      ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md # in-progress bootstrap
      docs-migration-guide.md                                # this migration (already here)
      docs-migration-file-map.md                             # this file (already here)
    notes/
      2026-03-06-docs-kanban-model.md                        # already here
      2026-03-06-docs-v2-problem-framing.md                  # problem framing (from architecture/)
      roadmap-critical-analysis.md                           # analysis/thinking
      roadmap-dependency-tree.md                             # dependency thinking
      mind-map-ui-research.md                                # UI research
      pi_agent_rust_integration_analysis.md                  # integration analysis
      backend-authoritative-ui-state-pattern.md              # pattern exploration
  state/
    snapshots/
      2026-03-06-writer-tracing-bootstrap-checkpoint.md      # current checkpoint
      2026-02-28-local-nixos-builder-vm-setup.md             # handoff
      2026-03-04-ovh-bootstrap-current-status.md             # handoff
    reports/
      2026-02-26-provider-matrix-kimi.md                     # test results
      2026-02-26-provider-matrix-all-models.md               # test results
      2026-02-26-local-cutover-step1.md                      # cutover report
      conductor-intelligence-2026-02-10.md                   # analysis report
      choiros-logging-security-report.md                     # security report
  archive/
    (existing archive/ stays as-is, plus new arrivals:)
    supervision-cutover-handoff.md                           # cutover completed
    2026-02-28-voice-layer-gemini-hackathon-plan.md          # one-off event, done
    directives-execution-checklist.md                        # completed checklist
    event-schema-design-report.md                            # completed design
    logging-watcher-architecture-design.md                   # superseded naming
    2026-02-28-3-tier-gap-closure-plan.md                    # completed plan
    2026-02-28-cutover-stocktake-and-pending-work.md         # completed stocktake
    2026-02-28-local-cutover-status-and-next-steps.md        # completed cutover
    2026-02-28-local-vfkit-architecture-review.md            # completed review
    2026-02-28-wave-plan-local-to-ovh-bootstrap.md           # completed wave plan
    pdf-app-implementation-guide.md                          # completed/dormant
    TRACING_UX_UPGRADE.md                                    # completed/dormant
    NARRATIVE_INDEX.md                                       # replaced by new index
    ARCHITECTURE_SPECIFICATION.md                            # superseded by ADRs
    AUTOMATIC_COMPUTER_ARCHITECTURE.md                       # vision doc, archive
    CHOIR_MULTI_AGENT_VISION.md                              # vision doc, archive
    DESKTOP_ARCHITECTURE_DESIGN.md                           # completed/dormant
    DOCUMENTATION_UPGRADE_PLAN.md                            # superseded by ADR-0015
    TESTING_STRATEGY.md                                      # stale
    BUGFIXES_AND_FEATURES.md                                 # stale tracking doc
    axum-migration-plan.md                                   # completed migration
    content-viewer-research.md                               # completed research
    research-dioxus-architecture.md                          # completed research
    terminal-ui.md                                           # completed/dormant
    theme-system-research.md                                 # completed research
    websocket-implementation-review.md                       # completed review
    window-management-research.md                            # completed research
    multiresearchraw_sandbox_infra.md                        # raw research dump
    dev-chatgpt-codex-auth-bridge.md                         # experimental, dormant
    design/event_bus_ractor_design.md                        # completed design
    design/PDF_APP_FULL_IMPLEMENTATION.md                    # completed/dormant
    design/PDF_APP_HALF_DAY_IMPLEMENTATION.md                # completed/dormant
    design/watcher-actor-architecture.md                     # completed design
    dev-blog/2026-02-01-why-agents-need-actors.md            # published, archive
    dev-blog/from-slop-to-signal-verified.md                 # published, archive
    prompts/refactor-session-2026-02-10.md                   # session artifact
    handoffs/README.md                                       # superseded
```

## Docs That May Need Content Updates Before/During Migration

| Doc | Issue | Action |
|-----|-------|--------|
| `adr-0007` (3-tier) | Status "In Progress" — is it still? Or effectively Accepted? | Review, possibly promote status |
| `actor-network-orientation.md` | May be stale vs current actor topology | Spot-check before placing in canon |
| `files-api-contract.md` | May not reflect current API | Spot-check |
| `writer-api-contract.md` | May not reflect current writer | Spot-check |
| `simplified-agent-harness.md` / `unified-agentic-loop-harness.md` | Two harness docs — are both current? | Determine which is canon, archive the other |
| `model-provider-agnostic-runbook.md` | May predate provider gateway rewrite | Spot-check |
| `nix-setup.md` | May predate OVH deployment | Spot-check |

## Migration Order

1. **Canon first** — move accepted ADRs + operational guides to `canon/`
2. **Active second** — move draft ADRs + in-progress guides + notes to `active/`
3. **State third** — move snapshots + reports to `state/`
4. **Archive last** — move everything else to `archive/`
5. **Update references** — fix cross-doc links, replace NARRATIVE_INDEX
6. **Add frontmatter** — Kind, Status, Priority, Requires on every doc
