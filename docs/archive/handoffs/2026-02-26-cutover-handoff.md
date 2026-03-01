# Cutover Handoff - 2026-02-26

## Snapshot
- Writer actor lifecycle fixed: Conductor no longer stops WriterActor at run finalize.
  - File: `sandbox/src/actors/conductor/runtime/finalize.rs`
- Writer open timing improved: prompt bar now opens Writer as soon as active non-immediate calls exist.
  - File: `dioxus-desktop/src/desktop/components/prompt_bar.rs`
- Added tests for the writer-open predicate against active calls.
  - File: `dioxus-desktop/src/desktop/components/prompt_bar.rs`
- Local dev command set and auth/client plumbing updates are included in this commit scope.
  - Files: `Justfile`, `dioxus-desktop/src/api.rs`, `dioxus-desktop/src/desktop/shell.rs`

## Validated
- `cargo check -p sandbox`
- `cargo test --manifest-path dioxus-desktop/Cargo.toml run_state_requires_writer -- --nocapture`
- Full local hypervisor stack Playwright (auth + proxy):
  - `PLAYWRIGHT_HYPERVISOR_BASE_URL=http://localhost:9090`
  - `npx playwright test --config=playwright.config.ts --project=hypervisor bios-auth.spec.ts proxy-integration.spec.ts --workers=1`
  - Result: 12 passed

## Important Operational Note
- WebAuthn tests require `localhost` origin alignment. Using `127.0.0.1` fails passkey flows because RP origin defaults to localhost in hypervisor config.

## Remaining Cutover Work
1. Stabilize sandbox project Playwright against full deployment mode (not `dx serve` rebuild state), ideally with deterministic auth/session bootstrap.
2. Add an end-to-end assertion that Writer window opens before first delegated worker completion event (concurrency UX contract).
3. Continue control-plane split/cutover tasks after this checkpoint.

## Quick Start Commands
```bash
just stop
just local-build-ui
cargo build -p sandbox
just local-hypervisor

cd tests/playwright
PLAYWRIGHT_HYPERVISOR_BASE_URL=http://localhost:9090 \
npx playwright test --config=playwright.config.ts \
  --project=hypervisor bios-auth.spec.ts proxy-integration.spec.ts --workers=1
```
