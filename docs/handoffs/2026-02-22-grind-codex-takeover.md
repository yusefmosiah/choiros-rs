# Handoff: Grind Codex Takeover (TLS/Auth Fixed, Sandbox Load Still Failing)

Date: 2026-02-22  
Owner: runtime/e2e  
Status: partial complete, ready for next debugging pass

## Narrative Summary (1-minute read)

Grind is now reachable through a secure origin (`https://os.choir.chat`) and passkey auth is confirmed working from Mac: account creation, passkey registration, and login all succeed.

The remaining live issue is not auth: Dioxus frontend loads, but sandbox-backed desktop runtime does not load after auth.

## What Changed

1. Confirmed grind host runtime and routing architecture:
   - Hypervisor is healthy on `:9090`.
   - Caddy is fronting traffic and now serves `:443`.
   - Frontend static assets are served by hypervisor/Caddy independently of sandbox proxy path.

2. Updated grind host config (declarative NixOS path):
   - Edited `/etc/nixos/configuration.nix` to set:
     - `WEBAUTHN_RP_ID=os.choir.chat`
     - `WEBAUTHN_RP_ORIGIN=https://os.choir.chat`
   - Updated Caddy vhost from `http://18.212.170.200` to `os.choir.chat` with reverse proxy to `127.0.0.1:9090`.
   - Applied with `nixos-rebuild switch`.

3. Post-switch verification (on grind host):
   - `hypervisor`, `caddy`, `container@sandbox-live`, `container@sandbox-dev` are active.
   - Hypervisor environment shows new WebAuthn values for `os.choir.chat`.
   - Caddy is listening on both `:80` and `:443`.

4. External user validation (Mac):
   - TLS works at `https://os.choir.chat`.
   - Passkey registration/login works end-to-end.

## Current Problem Statement

After successful auth on `https://os.choir.chat`, the frontend renders, but sandbox-backed desktop content does not load.

This is likely in one of these hops:
- hypervisor fallback proxy -> sandbox live/dev route selection,
- sandbox runtime health/readiness despite service active,
- desktop API path behavior under authenticated proxied requests,
- websocket/data path needed by desktop shell after auth.

## What To Do Next

1. Capture failing post-auth requests in browser network panel and map each to server hop:
   - `/desktop/*`
   - `/ws` and `/ws/*`
   - any 30x/40x/50x responses and bodies.

2. From grind host, test same failing endpoints with an authenticated session cookie:
   - verify whether hypervisor returns `sandbox unavailable: ...` or upstream `502`.

3. Correlate timestamps across logs:
   - `journalctl -u hypervisor -f`
   - `journalctl -u container@sandbox-live -f`
   - `journalctl -u container@sandbox-dev -f`
   - `journalctl -u caddy -f`

4. If needed, add temporary structured proxy diagnostics in hypervisor around:
   - `ensure_running` result,
   - selected sandbox role/port,
   - upstream response status and latency.

## Key Evidence

- Hypervisor routes static assets directly, but non-static app traffic uses fallback proxy to sandbox.
- Hypervisor environment is now:
  - `WEBAUTHN_RP_ID=os.choir.chat`
  - `WEBAUTHN_RP_ORIGIN=https://os.choir.chat`
- User-reported validation from Mac:
  - TLS works
  - passkey auth works
  - sandbox/desktop still does not load after auth.
