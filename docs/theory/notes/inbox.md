# Inbox

Date: 2026-03-06
Kind: Note
Status: Active
Priority: 3
Requires: []

Off-topic TODOs noticed during other work. Triage regularly:
promote to own doc, do it, or delete it.

## Items

- [ ] Review [CI Boundaries and the Bootstrap Thesis](/Users/wiz/choiros-rs/docs/theory/notes/2026-03-11-ci-boundaries-and-bootstrap-thesis.md) and decide which parts should graduate into ADR-0014 / ADR-0024 language
- [ ] CLAUDE.md is gitignored — consider whether it should be committed
- [ ] Spot-check practice guide candidates (actor-network-orientation, API contracts)
- [ ] Automatic machine class migration on mismatch — see [deferred items](2026-03-11-deferred-machine-class-items.md)
- [ ] Generation-aware snapshot invalidation — see [deferred items](2026-03-11-deferred-machine-class-items.md)
- [ ] data.img isolation security review — see [deferred items](2026-03-11-deferred-machine-class-items.md)
- [ ] KSM side-channel security review — KSM enables timing-based page dedup detection (flip feng shui, row hammer variants). Current mitigation: KSM only benefits ch-pmem worker pool (same-tenant), not cross-tenant user sandboxes (ch-blk). Needs formal threat model. See [KSM research](2026-03-11-ksm-research.md)
- [ ] Running demo of heterogeneous topology — ch-blk-2c-2g users + ch-pmem-4c-4g workers as a persistent demo environment, not just stress test
- [ ] Playwright + Chromium in worker-class guest image — current microVM guests are minimal (coreutils, curl, procps). Worker pool VMs need Playwright/Chromium to test real build→test→E2E flows. Will significantly increase store disk size and memory footprint. Need worker-specific NixOS image variant.

## Resolved

- [x] Fix 30+ clippy warnings — done 2026-03-11, all warnings cleared
- [x] Orchestration layer between conductor and app agents — captured in
  `docs/theory/notes/2026-03-11-agent-architecture-session-notes.md`. Decision:
  defer until after BAML removal and writer contract fix. ALM harness, cagent,
  and harness-level orchestration are all candidates. See session notes.
- [x] Review simplified-agent-harness.md DECIDE→EXECUTE pattern — it's a BAML
  artifact. Standard tool-use protocol loop replaces it when BAML is removed.
  See session notes section 1.
