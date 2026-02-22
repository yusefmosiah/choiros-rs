# Handoff: Mac Playwright Debug Plan for Sandbox Not Loading (os.choir.chat)

Date: 2026-02-22  
Owner: runtime/e2e  
Status: ready for Mac Codex execution

## Narrative Summary (1-minute read)

Auth is now working on `https://os.choir.chat` (account creation + passkey registration/login confirmed from Mac). The remaining issue is post-auth desktop/sandbox load failure.

Current browser evidence indicates frontend startup requests are failing with access-control behavior on:
- `GET/POST /desktop/default-desktop/apps`

This does not look like a raw "sandbox process down" issue by itself. We need deterministic Playwright repro + per-hop evidence (browser, hypervisor, caddy, sandbox containers).

## What Changed Before This Handoff

1. Grind host was updated to secure origin + matching WebAuthn values:
   - `WEBAUTHN_RP_ID=os.choir.chat`
   - `WEBAUTHN_RP_ORIGIN=https://os.choir.chat`
2. Caddy now serves `os.choir.chat` and TLS is live.
3. UI was updated to gate initial desktop-state fetch on auth and improve HTML-response error messaging.
4. Repo rename completed from `sandbox-ui` to `dioxus-desktop` across code/docs/scripts.

## Problem Statement

After successful auth at `https://os.choir.chat`, Dioxus loads but desktop runtime still fails to become usable.

Observed console snippet:
- `Fetch API cannot load https://os.choir.chat/desktop/default-desktop/apps due to access control checks.`
- websocket connect attempt logs: `wss://os.choir.chat/ws`

Hard reload after login does not fix.

## Objective

Use Mac-side Playwright as the canonical reproducer and isolate the first broken hop among:
1. Browser request behavior (redirect/CORS/fetch mode)
2. Caddy edge
3. Hypervisor auth/proxy middleware
4. Sandbox live/dev upstream APIs

## Execution Environment

- Public target: `https://os.choir.chat`
- Repo root on Mac: local checkout of this repo
- Grind SSH: root access is available from Mac for host-level checks/fixes

## Step-by-Step Plan (Mac Codex)

1. Baseline Playwright setup and run targeted debug spec(s).
2. Add temporary network instrumentation in Playwright to log every `/desktop` and `/ws` request/response.
3. Reproduce login -> post-auth load flow deterministically.
4. Correlate with grind logs in parallel over SSH.
5. Classify failing hop and implement smallest safe fix.
6. Re-run tests and report pass/fail + evidence.

## Playwright Commands (Mac)

From repo root:

```bash
cd tests/playwright
npm ci
npx playwright install
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://os.choir.chat \
  npx playwright test --config=playwright.config.ts --project=hypervisor \
  bios-auth.spec.ts proxy-integration.spec.ts --workers=1 --retries=0
```

If failure reproduces, run single test with trace:

```bash
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://os.choir.chat \
  npx playwright test --config=playwright.config.ts --project=hypervisor \
  proxy-integration.spec.ts --workers=1 --retries=0 --trace on
```

## Required Temporary Instrumentation (Playwright)

In the failing spec, add logging around:
- `page.on('request')` for URLs containing `/desktop`, `/ws`, `/auth/me`
- `page.on('response')` same filter, logging status + headers (`location`, `content-type`, `access-control-*`)
- `page.on('requestfailed')` capturing `failure().errorText`
- Collect browser console messages and JS errors

Persist debug output to test artifacts.

## Grind Correlation Commands (run over SSH from Mac)

Use aligned timestamps (UTC preferred):

```bash
journalctl -u caddy -f
journalctl -u hypervisor -f
journalctl -u container@sandbox-live -f
journalctl -u container@sandbox-dev -f
```

Quick hop probes:

```bash
curl -i http://127.0.0.1:9090/login
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8081/health
curl -i http://127.0.0.1:8080/desktop/default-desktop/apps
curl -i http://127.0.0.1:8080/desktop/default-desktop/apps \
  -H 'x-choiros-proxy-authenticated: true' \
  -H 'x-choiros-user-id: user-1' \
  -H 'x-choiros-sandbox-role: live'
```

## Suspected Failure Modes to Prove/Disprove

1. **Pre-auth startup side effects fire too early and never recover**
   - `register_core_apps_once` marks completion even when requests fail pre-auth.
2. **Redirect handling turns same-origin auth redirect into fetch access-control failure in WASM client path**
3. **Websocket boot timing race vs auth session establishment**
4. **Sandbox upstream behavior differs when called via hypervisor proxy headers**

## Candidate Fixes (apply only after evidence)

1. Gate core-app registration effect on `AuthState::Authenticated(_)`.
2. Change app-registration effect to set "registered" only after at least one successful pass, or retry post-auth.
3. Gate websocket bootstrap on authenticated state.
4. Gate theme preference fetch on authenticated state.
5. If proxy/header mismatch found, patch hypervisor middleware/proxy tagging with tests.

## Acceptance Criteria

1. Playwright `hypervisor` project passes targeted auth+proxy specs against `https://os.choir.chat`.
2. No browser console/network errors for `/desktop/default-desktop/apps` after auth.
3. Desktop loads reliably on first post-auth render (without manual refresh loops).
4. Evidence bundle includes:
   - test names + status
   - first failing line before fix
   - changed files
   - post-fix rerun output
   - relevant log excerpts from grind

## Deliverables Back to Main Branch

1. Code fix(es) + tests.
2. Updated handoff doc with final root cause and exact fix.
3. Commit(s) pushed to `main`.

