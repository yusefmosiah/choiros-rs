# Comprehensive Cutover Plan: Local 3-Tier to OVH

Date: 2026-02-26
Status: Active
Owner: platform/runtime

## Narrative Summary (1-minute read)

This is the execution anchor for the hard cutover.

End-state architecture:

1. Global control-plane services (shared): identity, secrets broker, control API, provider gateway, global memory.
2. Per-user runtime plane: one microVM per user, multiple sandbox containers inside (`live`, `dev`, branch/experiment containers).
3. Client plane: Dioxus web, mobile, API consumers.

Immediate strategy:

1. Stabilize local 3-tier runtime first.
2. Make local orchestration reproducible via `just` + tmux.
3. Split auth/secrets from hypervisor.
4. Prove the same shape on OVH.

## What Changed

1. Declared the hard target service topology and ownership boundaries.
2. Added a single execution sequence to prevent track drift.
3. Added local-first service orchestration milestones before OVH migration work.

## What To Do Next

1. Complete Phase 1 and Phase 2 this week (local runtime + local orchestration).
2. Start Phase 3 auth split only after local orchestration is stable.
3. Do not begin OVH cutover until Phase 5 is green.

## Current Checkpoint (2026-02-26)

Active phase: Phase 1 (Local 3-Tier Runtime Stability)

Observed state:

1. Conductor no longer terminates WriterActor at run finalize (writer lifecycle fixed).
2. Writer window now opens earlier for active non-immediate runs (better concurrent UX).
3. Full local hypervisor auth/proxy Playwright suite is green on `localhost`.
4. `dx build --release` logs a non-fatal `wasm-opt` crash in local env; assets still produced and command exits `0`.

Gate status:

- [x] Hypervisor + sandbox come up reliably with shared runtime contract.
- [~] Frontend bootstrap asset contract is verified automatically.
- [x] Playwright hypervisor/auth/proxy suite is green.

Interpretation:

1. Runtime correctness is improving, but startup/orchestration is still spread across multiple commands.
2. We should complete prod-like local startup unification before moving to Phase 2 service choreography.

## Hard Ownership Boundaries

## Hypervisor (Control Plane Component)

- Owns routing, per-user runtime lifecycle, policy enforcement, observability.
- Does not own Dioxus assets.
- Does not own auth ceremonies.
- Does not own provider secret material.

## Sandbox Containers (Per-User Runtime)

- Serve runtime APIs and user-facing app behavior.
- Run as isolated containers inside each user microVM.
- Never hold raw provider credentials.

## Dioxus Client (Client Plane)

- Separate client artifact/process.
- Talks to identity + control/runtime APIs.
- Not bundled into hypervisor authority.

## Phase Plan (Strict Order)

## Phase 1: Local 3-Tier Runtime Stability

Goal: login -> desktop -> prompt flow stable on localhost with prod-like contract.

Exit criteria:

- [x] Hypervisor + sandbox come up reliably with shared runtime contract.
- [~] Frontend bootstrap asset contract is verified automatically.
- [x] Playwright hypervisor/auth/proxy suite is green.

Immediate execution checklist:

1. Define canonical prod-like local startup command path (`just` recipes only). [done]
2. Verify prompt flow on full local deployment path (`localhost:9090`) with auth.
3. Add/enable one Playwright spec that asserts Writer opens during active delegated run (not after completion).
4. Record pass/fail outputs in a dated report under `docs/reports/`. [done]

## Phase 2: Local Service Orchestration (`just` + tmux)

Goal: run the future distributed shape locally with explicit commands.

Exit criteria:

- [x] `just dev-control-plane` starts all control-plane services locally.
- [x] `just dev-runtime-plane` starts per-user runtime services.
- [x] `just dev-all` and `just stop-all` are deterministic.
- [x] tmux layout scripts capture logs by service.

## Phase 3: Auth Split (Identity Service)

Goal: move authentication out of hypervisor cleanly.

Exit criteria:

- [ ] identity service owns register/login/recovery/session issuance.
- [ ] hypervisor only validates identity claims.
- [ ] old hypervisor auth routes are proxy-shims or removed.

## Phase 4: Secrets Split (Secrets Broker)

Goal: enforce keyless sandbox runtime.

Exit criteria:

- [ ] sandbox has zero raw provider keys in env.
- [ ] all provider access mediated through gateway + broker.
- [ ] broker emits audit events for policy/latency/outcome.

## Phase 5: Multi-Container Per-User Runtime

Goal: user runtime supports more than `live`/`dev` safely.

Exit criteria:

- [ ] per-user microVM runs N containers (`live`, `dev`, `branch-*`).
- [ ] stable pointer routing exists (`main`, `dev`, `exp-*`).
- [ ] branch-per-sandbox workflow linked to runtime containers.

## Phase 6: OVH Single Node

Goal: prove local shape runs on OVH node unchanged in semantics.

Exit criteria:

- [ ] control-plane + user-runtime layout converges declaratively.
- [ ] public flow and prompt execution pass.
- [ ] rollback drill passes once.

## Phase 7: OVH Two Node

Goal: active/passive failover with reproducibility.

Exit criteria:

- [ ] handoff and rollback tested between nodes.
- [ ] incident checklist complete and rehearsed.

## Weekly Cadence (Stay On Task)

1. Monday: set 1 active phase objective + acceptance tests.
2. Daily: run required local gate checks before coding new features.
3. Friday: publish short status report with pass/fail per gate and blockers.

## Daily Gate Command Set (Local)

1. `just local-build-ui`
2. `cargo build -p sandbox`
3. `just local-hypervisor`
4. `PLAYWRIGHT_HYPERVISOR_BASE_URL=http://localhost:9090 npx playwright test --config=playwright.config.ts --project=hypervisor bios-auth.spec.ts proxy-integration.spec.ts --workers=1`
5. provider matrix validation (when gateway token + provider env are present)
6. record outcomes in `docs/reports/`

Stop command:

1. `just stop`

## Next 72 Hours (Strict)

1. Finish Phase 1 remaining gates:
   - canonical startup recipe set
   - writer-concurrency regression spec
2. Begin Phase 2 only after Phase 1 gates are green:
   - `dev-control-plane`, `dev-runtime-plane`, `dev-all`, `stop-all`
   - tmux-backed logs by service
3. Do not start Phase 3 auth split implementation work until Phase 2 command reliability is demonstrated.

## Blockers and Stop Rules

- If a lower phase gate is red, no work on higher phases.
- No “temporary fallback” that hides routing/auth/contract errors.
- Fail loudly, capture trace, fix root cause.

## References

1. `docs/architecture/roadmap-dependency-tree.md`
2. `docs/architecture/2026-02-20-bootstrap-execution-checklists.md`
3. `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`
4. `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
5. `docs/runbooks/local-provider-matrix-validation.md`
