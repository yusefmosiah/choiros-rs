# Handoff: Auth Onboarding Flow Regression

Date: 2026-02-21  
Owner: auth/runtime  
Status: resolved (auth UX fix shipped to production); CI/CD follow-up pending

## Narrative Summary (1-minute read)

Runtime deployment is live on AWS with NixOS native containers, hypervisor,
public domain, and TLS. The auth onboarding mismatch on `/register` has now
been fixed in the Dioxus modal UX and deployed to production assets.

The next workstream is CI/CD hardening and cache-backed build/deploy automation
to remove manual artifact sync steps.

## What Changed

1. Production host is served at `https://os.choir-ip.com` with Caddy -> hypervisor.
2. Apex `https://choir-ip.com` redirects to `https://os.choir-ip.com`.
3. WebAuthn RP config remains:
   - `WEBAUTHN_RP_ID=os.choir-ip.com`
   - `WEBAUTHN_RP_ORIGIN=https://os.choir-ip.com`
4. Auth modal now has explicit login/signup modes with email-first prompt and
   route-aware intent (`/login` vs `/register`), no implicit login->register fallback.
5. Public Playwright smoke passed on HTTPS domain for:
   - `tests/playwright/bios-auth.spec.ts`
   - `tests/playwright/proxy-integration.spec.ts`
   - latest result: 12/12 passing on `https://os.choir-ip.com`

## What To Do Next

1. Wire CI workflow to build `sandbox`, `hypervisor`, and `dioxus-desktop` artifacts
   from cache-backed flake outputs.
2. Add deploy job that updates EC2 runtime artifacts declaratively (prefer host-side
   Nix build or pull-from-cache over ad hoc rsync).
3. Keep Playwright domain-mode auth/proxy smoke as release-gate evidence.
4. Add rollback-safe deploy docs covering generation rollback + artifact rollback.

## Verification Notes

- `/register` now renders signup mode label and email prompt.
- `/login` now renders login mode label and email prompt.
- Login with unknown email now returns inline error and does not auto-create account.
- Keep tests on production hostname mode; avoid localhost-only assumptions.

## Guardrails for Next Phase

- Keep RP origin/domain unchanged unless root-cause proves WebAuthn config defect.
- Do not introduce deterministic orchestration fallbacks in conductor/runtime paths.
- Preserve current NixOS container substrate and hypervisor reverse-proxy topology.
