# Sandbox-Owned UI Runtime Spec (MVP Boundary Shift)

Date: 2026-02-22  
Status: Proposed (pre-implementation)  
Owner: runtime/platform

## Narrative Summary (1-minute read)

ChoirOS currently mixes ownership of the authenticated desktop runtime: hypervisor serves
`/wasm` and `/assets` from host disk while authenticated API and websocket paths are
proxied into sandbox instances. This creates runtime contract drift and reconnect loops
when UI bundle/runtime expectations diverge across boundaries.

This spec moves to a single-owner model for authenticated desktop runtime:

1. Hypervisor remains auth/control-plane/router only.
2. Sandbox owns the full authenticated app surface per role (`live` or `dev`):
   `/`, `/wasm/*`, `/assets/*`, app APIs, and app websockets.
3. Live/dev promotion remains a routing concern (role swap), not host static swap.

## What Changed

1. Defined an authoritative path ownership contract with strict sandbox ownership for
   authenticated desktop runtime.
2. Removed architecture-level permission for hypervisor to serve authenticated runtime
   static assets from host build outputs.
3. Defined role-routing and `/dev/*` rewrite behavior as first-class interface contract.
4. Added test and observability gates that prevent mixed-owner regressions.

## What To Do Next

1. Implement hypervisor route cutover:
   - keep `/auth/*`, auth pages, admin, and provider gateway in hypervisor.
   - route authenticated app runtime paths entirely to sandbox role target.
2. Update middleware allowlist to avoid static path split assumptions.
3. Add route-ownership integration tests (HTTP + WS) for live/dev sandboxes.
4. Add regression alarms for runtime mismatch signals (`/_dioxus` 404 loops,
   repeated websocket reconnect storms).

## Context and Problem Statement

Current runtime layering:

- Hypervisor:
  - serves auth flows and some static UI paths (`/wasm`, `/assets`).
  - proxies fallback traffic to sandbox.
- Sandbox:
  - serves desktop API + websocket runtime (`/desktop/*`, `/ws`, etc.).

Observed failure class:

- Browser loads host-served bundle, then opens runtime websocket/API paths expected by
  that bundle against sandbox.
- If bundle/runtime contracts differ, clients enter reconnect/reload loops.

This breaks the intended live/dev sandbox rewriteability model where agents should be
able to mutate and validate the full runtime in the sandbox boundary before promotion.

## Goals

1. Single ownership for authenticated desktop runtime paths.
2. Role-complete runtime coherence (`live` and `dev` each self-contained).
3. Promotion as routing pointer swap only.
4. Preserve existing auth, session, admin lifecycle, and provider-gateway boundaries.

## Non-Goals (MVP)

1. Full multi-tenant redesign.
2. New UI deployment CDN model.
3. Changing auth protocol behavior.
4. Changing provider-gateway semantics.

## Authoritative Path Ownership Contract

| Path Pattern | Auth Required | Owner | Role Resolution | Rewrite |
| --- | --- | --- | --- | --- |
| `/auth/*` | No (endpoint-specific) | Hypervisor | N/A | None |
| `/login`, `/register`, `/recovery` | No | Hypervisor | N/A | None |
| `/admin/sandboxes*` | Yes | Hypervisor | N/A | None |
| `/provider/v1/{provider}/{*rest}` | Policy-token path | Hypervisor | N/A | None |
| `/dev/*` (authenticated app runtime) | Yes | Sandbox | `dev` | Strip `/dev` before proxy |
| `/` (authenticated desktop shell) | Yes | Sandbox | `live` | None |
| `/wasm/*` (authenticated runtime assets) | Yes | Sandbox | `live`/`dev` by route | None |
| `/assets/*` (authenticated runtime assets) | Yes | Sandbox | `live`/`dev` by route | None |
| `/ws`, `/ws/*` | Yes | Sandbox | `live`/`dev` by route | None |
| `/desktop/*`, `/logs/*`, `/writer/*`, `/conductor/*`, app APIs | Yes | Sandbox | `live`/`dev` by route | None |

Notes:

1. Hypervisor should not host authenticated runtime static assets once cutover completes.
2. Unauthenticated access to protected app/runtime paths still redirects to `/login`.

## Runtime Interface Contract

Each sandbox role instance (`live`, `dev`) must expose a complete coherent runtime:

1. UI entry and static assets needed by that UI build.
2. API endpoints consumed by that UI build.
3. Websocket endpoints consumed by that UI build.
4. No hidden dependency on hypervisor-hosted UI assets.

Invariant:

- A single sandbox role endpoint must be sufficient to run the desktop client without
  pulling runtime-bearing assets from hypervisor host filesystem.

## Live/Dev Promotion Contract

Promotion remains a control-plane routing operation:

1. `swap` exchanges role routing (`live` <-> `dev`) for a user.
2. No host static asset swap is required or allowed for authenticated runtime paths.
3. Post-swap traffic must resolve to the promoted role across HTTP and WS consistently.

Required pre-swap health checks:

1. Role sandbox process reachable and healthy.
2. Desktop websocket handshake success.
3. Desktop root load and app bootstrap success.

## Migration Plan (Phased)

### Phase A - Boundary Lock

1. Merge this spec + ADR.
2. Add route ownership assertions to test harness.

### Phase B - Hypervisor Route Cutover

1. Remove hypervisor-owned authenticated runtime static serving.
2. Proxy authenticated runtime paths to sandbox role targets.
3. Keep auth/admin/provider-gateway routes in hypervisor.

### Phase C - Live/Dev Runtime Validation

1. Validate `/dev/*` rewrite and role isolation.
2. Validate role swap changes effective runtime consistently (HTTP + WS).

### Phase D - Cleanup

1. Remove legacy docs/comments assuming host static ownership for authenticated runtime.
2. Keep any remaining host static support only if explicitly for unauthenticated auth-page
   bootstrapping and documented as such.

## Test Plan and Acceptance Scenarios

### Integration/E2E Scenarios

1. Authenticated `/` returns runtime served via sandbox role path, not mixed host static.
2. No repeating `/_dioxus` 404 websocket loop under hypervisor-served app shell.
3. `/ws` desktop websocket remains stable after initial subscribe.
4. `/dev/` runtime serves dev role and strips prefix correctly.
5. `swap` flips runtime owner role without mixed-asset behavior.
6. Unauthenticated protected path still redirects to `/login`.

### Failure-Mode Scenarios

1. If sandbox runtime asset path missing: bounded error response and no infinite reload storm.
2. If websocket upgrade fails: explicit error telemetry with role/path attribution.

## Observability Requirements

Must log with route-role context:

1. `path`, `method`, `role`, `proxy_target`, `status`.
2. websocket upgrade outcomes and failure reason categories.
3. repeated reconnect detector counters per `user_id` + `sandbox_id`.

Contract-violation signals:

1. Any `/_dioxus` 404 in hypervisor->sandbox proxied traffic for authenticated runtime paths.
2. Repeated websocket reconnects over threshold in short window.

## Compatibility and Rollback

Rollback trigger examples:

1. Elevated websocket reconnect loop rates.
2. Authenticated desktop bootstrap failures above baseline.

Rollback action:

1. Re-enable prior route behavior behind feature flag if needed.
2. Preserve auth/admin/provider-gateway routes unchanged to limit blast radius.

## References

- `hypervisor/src/main.rs`
- `hypervisor/src/middleware.rs`
- `hypervisor/src/proxy.rs`
- `hypervisor/src/sandbox/mod.rs`
- `sandbox/src/api/mod.rs`
- `tests/playwright/proxy-integration.spec.ts`
- `docs/architecture/adr-0003-hypervisor-sandbox-secrets-boundary.md`
