# Local NixOS + VFKit Miniguide (Manual Run)

Date: 2026-02-28  
Status: Active  
Owner: platform/runtime

## Narrative Summary (1-minute read)

Use `9090` when you want deployed-shape behavior (hypervisor ingress + per-user runtime routing).  
Use `3000` only for direct sandbox/dev loops.

If you are validating cutover, run through `9090` and the hypervisor Playwright project.

## What Changed

1. Local vfkit runtime control is Rust-hosted (`vfkit-runtime-ctl`).
2. Hypervisor serves SPA bootstrap and proxies runtime HTTP/WS.
3. Guest runtime control defaults to `if-missing` sandbox rebuild mode for faster and more stable startup.

## What To Do Next

1. Run this guide once end-to-end on your machine.
2. Keep `9090` as canonical for cutover testing.
3. Use `3000` only when intentionally bypassing hypervisor.

## 1) One-Time Prerequisites

1. Build UI assets:
```bash
just local-build-ui
```
2. Ensure Linux builder is wired:
```bash
just cutover-status
just cutover-status --probe-builder
```
3. If builder is missing:
```bash
just builder-bootstrap-utm <utm-vm-name>
```

## 2) Start Local Stack

```bash
just dev
just dev-status
```

Expected:
1. Hypervisor healthy at `http://127.0.0.1:9090/login`

## 3) Manual Product Check (Canonical Path)

1. Open `http://127.0.0.1:9090`.
2. Authenticate.
3. Use prompt bar at bottom.
4. Open `Writer`.
5. Open `Terminal`.
6. Open `Trace`.
7. In Terminal, run:
```bash
cat /etc/os-release
```
Expected includes `NixOS`.

## 4) Optional Guest Workload View (btop)

```bash
just btop
```

This SSHes to the guest VM and opens `btop`.

## 5) Run Hypervisor E2E with Video

```bash
cd tests/playwright
npx playwright test --config=playwright.config.ts --project=hypervisor desktop-app-suite-hypervisor.spec.ts --workers=1
```

Artifacts:
1. `tests/artifacts/playwright/test-results/**/video.webm`
2. `tests/artifacts/playwright/test-results/**/trace.zip`
3. `tests/artifacts/playwright/html-report/index.html`

Open report:
```bash
cd tests/playwright
npx playwright show-report ../artifacts/playwright/html-report
```

## 6) Troubleshooting Fast Map

1. White/blank desktop in `9090`:
   1. Check hypervisor up: `just dev-status`
   2. Check runtime start errors in hypervisor logs.
2. Stuck at `Connecting...`:
   1. Wait for runtime warmup (terminal can take ~10s+ on cold start).
   2. Verify runtime port is running via `/admin/sandboxes` API.
3. Need fresh guest sandbox binary build:
```bash
./scripts/ops/vfkit-guest-ssh.sh -- 'CHOIR_VFKIT_GUEST_BUILD_SANDBOX_MODE=always /workspace/scripts/ops/vfkit-guest-runtime-ctl.sh ensure --runtime live --port 8080 --role live'
```
4. Reset local vfkit processes/tunnels:
```bash
just vfkit-reset
```
