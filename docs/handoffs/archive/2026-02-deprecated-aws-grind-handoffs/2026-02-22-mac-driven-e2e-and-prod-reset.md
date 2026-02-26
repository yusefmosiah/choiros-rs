# Handoff: Mac-Driven E2E and Prod Reset Plan

Date: 2026-02-22  
Owner: runtime/deploy  
Status: ready to execute

## Narrative Summary (1-minute read)

Grind is currently healthy as a runtime target and should be treated as the active validation
environment. Browser E2E should run from Mac (or CI runner), not from grind, because Playwright
downloaded Chromium binaries fail on NixOS due to dynamic linker constraints.

Prod can be reset from scratch. That is the right move if we no longer trust runtime drift.
Reset should be done as a declarative rebuild plus release promotion from grind, not by manual
host edits.

## What Changed

1. Confirmed grind runtime status:
   - `hypervisor`, `container@sandbox-live`, `container@sandbox-dev`, and `caddy` are running.
2. Confirmed grind endpoints:
   - `http://18.212.170.200/login` returns `200`.
   - sandbox health works on container IPs (`10.233.1.2:8080`, `10.233.2.2:8080`).
3. Confirmed Playwright-on-grind limitation:
   - `npx playwright test` fails with NixOS `stub-ld` runtime error for downloaded Chromium.
4. Confirmed DNS split:
   - `os.choir-ip.com` currently resolves to prod (`54.211.83.193`), not grind.

## What To Do Next

1. Run hypervisor E2E from Mac against grind public URL.
2. Keep prod out of the loop until grind passes E2E.
3. After green E2E on grind, perform full prod reset and redeploy from deterministic manifest.

## Current Runtime Targets

- Grind public URL: `http://18.212.170.200`
- Grind health/auth checks:
  - `http://18.212.170.200/login`
  - `http://18.212.170.200/auth/me`

Note: The canonical public domain currently points to prod. For grind validation use the grind IP
directly, not `https://os.choir-ip.com`.

## Mac-Driven E2E Workflow

From local Mac repo root:

```bash
cd tests/playwright
npm ci
npx playwright install
PLAYWRIGHT_HYPERVISOR_BASE_URL=http://18.212.170.200 \
  npx playwright test --config=playwright.config.ts --project=hypervisor
```

Optional narrowed run while iterating auth/proxy only:

```bash
PLAYWRIGHT_HYPERVISOR_BASE_URL=http://18.212.170.200 \
  npx playwright test --config=playwright.config.ts --project=hypervisor \
  bios-auth.spec.ts proxy-integration.spec.ts
```

Artifacts are written to:
- `tests/artifacts/playwright/test-results/`
- `tests/artifacts/playwright/html-report/index.html`

## Prompt for Mac OpenCode Session

Use this prompt from Mac:

```text
Use grind as the runtime target for hypervisor E2E.
Target URL: http://18.212.170.200

Tasks:
1) Run Playwright hypervisor project tests from tests/playwright.
2) If tests fail, classify each failure as auth flow, sandbox bootstrap, proxy routing, or test harness issue.
3) For app/runtime failures, identify the exact failing hop and propose the smallest safe fix.
4) Implement the fix in this repo, rerun the same targeted tests, and report pass/fail with evidence.
5) Do not touch AWS security groups or host firewall unless explicitly requested.

Return:
- failing test names and first error line
- root cause
- changed files
- rerun result
```

## Prod Reset (From Scratch) Plan

Yes, prod can be wiped and rebuilt. Recommended sequence:

1. Freeze prod traffic entry (maintenance window or temporary DNS cutover).
2. Capture one final forensic snapshot:
   - `scripts/ops/host-state-snapshot.sh --output /var/log/choiros/pre-reset.env`
   - backup `/opt/choiros/data` if any data may be needed.
3. Reprovision prod host from baseline NixOS config (or rebuild and clear mutable app state).
4. Re-apply secrets declaratively (sops-nix path only).
5. Promote known-good release from grind using:
   - `scripts/ops/promote-grind-to-prod.sh --grind <grind-host> --prod <prod-host>`
6. Run smoke and Playwright hypervisor tests against prod endpoint.
7. Capture post-reset snapshot for audit.

Non-negotiable policy after reset:
- No interactive prod workspace mutations.
- No provider credentials in sandbox runtime env.
- All updates via manifest-based closure promotion.
