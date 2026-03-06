# Local Cutover Step 1 Report

Date: 2026-02-26  
Scope: Phase 1 completion work + Phase 2 command surface bootstrap

## Narrative Summary (1-minute read)

Local cutover command surface is now live and deterministic via `just` + tmux orchestration:

1. `just dev-control-plane`
2. `just dev-runtime-plane`
3. `just dev-all`
4. `just stop-all` (and `just stop` alias)

Full local deployment was started through the new path, health checks passed, and the hypervisor Playwright auth/proxy suite passed (`12/12`).

## What Changed

1. Added `scripts/dev-cutover.sh` as the canonical local cutover orchestrator.
2. Added `Justfile` recipes:
   - `dev-control-plane`
   - `dev-runtime-plane`
   - `dev-all`
   - `dev-all-foreground`
   - `dev-status`
   - `dev-attach`
   - `stop-all`
3. Redirected `just stop` to `just stop-all`.

## What To Do Next

1. Add/enable Writer concurrency Playwright regression spec for delegated runs.
2. Validate tmux log capture expectations against the phase gates.
3. Continue with Phase 2 service decomposition while preserving this command contract.

## Commands Run

1. `just stop-all`
2. `just local-build-ui`
3. `just dev-all`
4. `just dev-status`
5. `cd tests/playwright && PLAYWRIGHT_HYPERVISOR_BASE_URL=http://localhost:9090 npx playwright test --config=playwright.config.ts --project=hypervisor bios-auth.spec.ts proxy-integration.spec.ts --workers=1`
6. `just dev-all-foreground` (manual verification of foreground log stream and Ctrl+C shutdown)

## Results

1. `just local-build-ui` completed successfully; local non-fatal `wasm-opt` SIGABRT still appears, assets emitted.
2. `just dev-all` started tmux session `choiros-cutover` with windows:
   - `shell`
   - `sandbox`
   - `hypervisor`
3. `just dev-status` reported:
   - sandbox healthy on `http://127.0.0.1:8080/health`
   - hypervisor healthy on `http://127.0.0.1:9090/login`
4. Playwright suite result: `12 passed (30.7s)`.
5. Individual plane commands verified:
   - `just dev-control-plane` -> hypervisor healthy on `:9090/login`
   - `just dev-runtime-plane` -> sandbox healthy on `:8080/health`
6. Added hypervisor-path concurrency regression skeleton:
   - `tests/playwright/writer-concurrency-hypervisor.spec.ts`
   - Included in hypervisor Playwright project match list
   - Currently marked `fixme` pending a stable active-run UI assertion surface
7. Hypervisor Playwright suite after cutover command changes:
   - `12 passed, 1 skipped`

## Notes

1. Browser auth tests must target `http://localhost:9090` (not `127.0.0.1`) for RP/origin consistency.
2. New plane processes take a few seconds to warm up; early `dev-status` checks can show `down` before compile/start completes.
3. Concurrency assertion via Trace run state is currently flaky due UI selection/state ambiguity with existing windows; next step is to expose a deterministic active-run signal for E2E.
