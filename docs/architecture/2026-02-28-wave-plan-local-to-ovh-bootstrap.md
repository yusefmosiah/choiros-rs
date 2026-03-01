# Wave Plan: Local Cutover to OVH Bootstrap to Post-Bootstrap Product Expansion

Date: 2026-03-01
Status: Active plan
Owner: platform/runtime

## Narrative Summary (1-minute read)

Local cutover readiness is strong enough to enforce a strict pre-OVH gate.
Execution sequence is locked:

1. Complete strict local manual + automated validation on canonical `9090` flow.
2. Prepare OVH deployment with a design + spike rewrite of current deploy surface.
3. Procure and bring up one OVH node, then prove full user flow + rollback.
4. Activate bootstrap (`Choir builds Choir`) on stable runtime.
5. Expand product capabilities (memory, multimedia, publishing, live audio) only after bootstrap and platform gates are green.

DAG/ALM cleanup is intentionally deferred from this lane to avoid destabilizing cutover progress.

## What Changed

1. Locked execution sequence to: OVH-first, then memory/feature expansion.
2. Locked strict pre-OVH go/no-go gate:
   1. `just cutover-status --probe-builder` passes.
   2. Canonical `9090` hypervisor e2e passes with video/trace artifacts.
   3. Provider matrix validation ends with `failures=0`.
3. Locked manual-test evidence policy to checklist + artifacts.
4. Marked OVH deploy transport as unresolved until server procurement constraints are known.
5. Added deployment-surface rewrite review + spike as mandatory pre-procurement work.

## What To Do Next

1. Run Wave 0 strict local gate and capture evidence.
2. Execute Wave 1 deployment rewrite design + spike.
3. Procure OVH host only after Wave 1 outputs are decision-complete.
4. Execute Wave 2 single-node OVH bring-up.

## Operator Checklist (Strict Go/No-Go)

- [ ] `just cutover-status --probe-builder`
- [ ] `just dev`
- [ ] `just dev-status`
- [ ] `desktop-app-suite-hypervisor.spec.ts` passes with video and trace
- [ ] `./scripts/ops/validate-local-provider-matrix.sh` ends with `failures=0`
- [ ] Manual check passes on `http://127.0.0.1:9090`:
  - [ ] login
  - [ ] desktop load
  - [ ] prompt bar interaction
  - [ ] writer app usable
  - [ ] terminal app connected (allow ~10s startup)
  - [ ] trace app visible and updating
  - [ ] `cat /etc/os-release` includes `NixOS`
- [ ] Evidence captured:
  - [ ] timestamp + commit SHA
  - [ ] checklist notes
  - [ ] video path
  - [ ] trace path
  - [ ] provider matrix summary

## Scope

### In Scope

1. Manual regression on canonical `9090` path.
2. Canonical e2e cutover gate hardening.
3. Provider/gateway reliability gate.
4. OVH deployment surface rewrite design + local spike.
5. Single-node OVH bring-up readiness package.

### Out of Scope

1. DAG/ALM removal.
2. Memory/multimedia/audio implementation work.
3. Two-node OVH failover execution.

## Wave 0: Manual Regression + Go/No-Go Baseline (Now)

### Objective

Prove local cutover path is stable enough to move into OVH prep.

### Required Command Set

1. `just cutover-status --probe-builder`
2. `just dev`
3. `just dev-status`
4. `cd tests/playwright && npx playwright test --config=playwright.config.ts --project=hypervisor desktop-app-suite-hypervisor.spec.ts --workers=1`
5. `./scripts/ops/validate-local-provider-matrix.sh`

### Manual Checklist (must be recorded each run)

1. Login succeeds at `http://127.0.0.1:9090`.
2. Desktop loads and prompt bar accepts input.
3. Writer window opens and stays interactive.
4. Terminal connects (allow ~10s warmup) and `cat /etc/os-release` includes `NixOS`.
5. Trace app opens and shows current run activity.

### Required Evidence

1. Manual checklist notes with timestamp + commit SHA.
2. Playwright video + trace artifact paths.
3. Provider matrix summary with `failures=0`.

### Exit Criteria

1. All Wave 0 commands pass.
2. No unresolved P0/P1 regressions from checklist.

## Wave 1: Local Bootstrap Hardening + OVH Deploy Rewrite Design/Spike

### Objective

Remove deployment ambiguity before OVH procurement.

### Track A: Canonical Cutover Gate Consolidation

1. Add one canonical `just` command that runs strict pre-OVH gate end-to-end.
2. Include branch runtime coverage (`branch-proxy-integration.spec.ts`) in canonical hypervisor suite.
3. Keep `9090` as deployment-shape gate; `3000` remains dev-only compatibility lane.

### Track B: Deployment Rewrite Audit

1. Audit AWS-specific transport assumptions in `scripts/deploy/aws-ssm-deploy.sh`.
2. Audit fixed runtime/container assumptions in `scripts/deploy/host-switch.sh`.
3. Write target OVH deploy contract doc:
   1. Inputs and secrets.
   2. Convergence steps.
   3. Health checks.
   4. Rollback semantics.
   5. Runtime inventory model (`live/dev/branch-*`).

### Track C: Deploy Spike (transport not locked)

1. Build a dry-runnable deploy wrapper that does not depend on AWS SSM.
2. Validate host convergence + diagnostics flow against local Linux target or equivalent SSH test target.
3. Keep this as spike/prototype until OVH server constraints are known.

### Track D: Documentation Alignment

1. Reconcile sequencing drift across active docs so one order is authoritative.
2. Keep `docs/architecture/NARRATIVE_INDEX.md` aligned to active lane.
3. Update operator runbooks to reference the strict gate and evidence policy.

### Exit Criteria

1. Strict gate command exists and passes locally.
2. Deploy rewrite design is decision-complete.
3. Deploy spike proves host convergence path independent of AWS SSM transport.

## Wave 2: OVH Single-Node Bring-Up (After Procurement)

### Objective

Prove one OVH node matches local 3-tier semantics.

### Preconditions

1. OVH host procured with SSH + baseline NixOS.
2. Domain and TLS plan defined.
3. Secrets flow validated against `docs/runbooks/platform-secrets-sops-nix.md`.

### Bring-Up Sequence

1. Converge host from repo configuration.
2. Deploy hypervisor + runtime binaries via rewritten deploy interface.
3. Run host and runtime health checks.
4. Run Mac-origin Playwright hypervisor smoke against OVH endpoint.

### Exit Criteria

1. Public login -> desktop -> prompt loop passes.
2. Terminal proof confirms runtime identity/health.
3. One rollback drill succeeds.

## Wave 3: Bootstrap Activation (Immediately After Wave 2)

### Objective

Establish Choir-on-Choir as daily development loop.

### Required Proof

1. One successful run: request -> code change -> test -> report.
2. One intentionally failed run with useful trace diagnostics.
3. Branch-per-container workflow exercised for at least one feature branch runtime.

### Exit Criteria

1. Bootstrap loop repeatable by checklist.
2. Rollback + regression triage workflow proven under live runtime conditions.

## Wave 4: Post-Bootstrap Product Expansion

### Objective

Build feature roadmap on stable platform.

### Execution Order

1. Memory integration with scoped writes/reads and observability.
2. Multimedia app support (including video app and writer embedding flow).
3. Publishing pipeline.
4. Live audio I/O and screenless operation pathway.

### Rule

No feature lane bypasses red platform gates.

## Planned Interface and Surface Changes

1. Command surface:
   1. Add one strict pre-OVH gate command in `Justfile`.
   2. Add one non-AWS deploy abstraction command.
2. Deployment surfaces:
   1. Separate transport layer from host convergence logic.
   2. Replace fixed container assumptions with dynamic runtime inventory.
3. Documentation surfaces:
   1. One authoritative gate order in active docs.
   2. One required manual-test evidence template per go/no-go run.

## Tests and Scenarios

### Unit

1. Deploy argument parsing and validation.
2. Runtime inventory resolution for `live/dev/branch-*`.

### Integration

1. Branch runtime start/stop + pointer set/read APIs.
2. Health-check error envelope and diagnostics behaviors.

### E2E (Canonical)

1. `desktop-app-suite-hypervisor.spec.ts`.
2. `branch-proxy-integration.spec.ts`.
3. `vfkit-cutover-proof.spec.ts` for NixOS/runtime proof artifacts.

### Manual

1. Checklist + artifacts are mandatory per release-candidate run.

## Explicit Assumptions and Defaults

1. Canonical sequence is local strict gate -> OVH single-node -> bootstrap activation -> memory/features.
2. OVH server is not yet procured; transport remains provisional until procurement constraints are known.
3. `9090` remains canonical for cutover/deployment-shape validation.
4. DAG/ALM cleanup is deferred until after platform stabilization.
5. Strict gate is blocking; no OVH execution starts on red gates.

## References

1. `docs/architecture/NARRATIVE_INDEX.md`
2. `docs/architecture/2026-02-28-local-vfkit-architecture-review.md`
3. `docs/runbooks/2026-02-28-local-vfkit-nixos-miniguide.md`
4. `docs/architecture/2026-02-28-cutover-stocktake-and-pending-work.md`
5. `docs/architecture/2026-02-28-local-cutover-status-and-next-steps.md`
6. `docs/architecture/2026-02-28-3-tier-gap-closure-plan.md`
7. `docs/architecture/2026-02-26-comprehensive-cutover-plan.md`
8. `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`
9. `docs/architecture/roadmap-dependency-tree.md`
10. `docs/architecture/2026-02-20-bootstrap-execution-checklists.md`
11. `docs/runbooks/local-provider-matrix-validation.md`
12. `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
13. `docs/runbooks/platform-secrets-sops-nix.md`
