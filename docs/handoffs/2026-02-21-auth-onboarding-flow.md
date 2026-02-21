# Handoff: Auth Onboarding Flow Regression

Date: 2026-02-21  
Owner: auth/runtime  
Status: open

## Narrative Summary (1-minute read)

Runtime deployment is now live on AWS with NixOS native containers, hypervisor,
public domain, and TLS. Public HTTPS smoke tests pass for core auth endpoints and
proxy integration, but product behavior now shows a UX mismatch: the register
entry path presents login behavior.

This handoff scopes the next workstream to isolate and fix onboarding UX/state
handling without changing current deployment topology.

## What Changed

1. Production host is now served at `https://os.choir-ip.com` with Caddy -> hypervisor.
2. Apex `https://choir-ip.com` redirects to `https://os.choir-ip.com`.
3. WebAuthn RP config is set to:
   - `WEBAUTHN_RP_ID=os.choir-ip.com`
   - `WEBAUTHN_RP_ORIGIN=https://os.choir-ip.com`
4. Public Playwright smoke passed on HTTPS domain for:
   - `tests/playwright/bios-auth.spec.ts`
   - `tests/playwright/proxy-integration.spec.ts`

## What To Do Next

1. Reproduce the onboarding mismatch manually and in Playwright with explicit
   expectations for register-specific copy/state transitions.
2. Trace frontend route/modal state logic for `/register` vs `/login` and confirm
   API intent boundaries.
3. Confirm server-side mode detection and response payload parity for register vs login.
4. Implement minimal fix and add regression coverage.
5. Re-run domain-mode Playwright suite against `https://os.choir-ip.com` and attach evidence.

## Repro Notes

- User report: register path shows login behavior.
- Baseline URL: `https://os.choir-ip.com/register`
- Secondary URL: `https://os.choir-ip.com/login`
- Important: keep tests on production hostname mode; avoid localhost-only assumptions.

## Guardrails for Fix

- Keep RP origin/domain unchanged unless root-cause proves WebAuthn config defect.
- Do not introduce deterministic orchestration fallbacks in conductor/runtime paths.
- Preserve current NixOS container substrate and hypervisor reverse-proxy topology.
