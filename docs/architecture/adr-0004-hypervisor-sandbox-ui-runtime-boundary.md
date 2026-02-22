# ADR-0004: Hypervisor-Sandbox UI Runtime Boundary

Date: 2026-02-22  
Status: Draft  
Owner: ChoirOS runtime and deployment

## Narrative Summary (1-minute read)

ChoirOS will move authenticated desktop runtime ownership entirely into sandbox role
instances (`live` and `dev`). Hypervisor remains auth/control-plane/router and will no
longer be an authenticated runtime static asset origin.

This eliminates mixed-owner runtime drift and makes sandbox runtime rewriteability
the default behavior, aligned with live/dev promotion goals.

## What Changed

1. Selected strict sandbox ownership for authenticated UI/runtime paths.
2. Rejected split host-static + sandbox-runtime ownership as unstable.
3. Defined promotion as routing swap over complete role-owned runtime.
4. Added explicit re-evaluation triggers for future multi-tenant or CDN concerns.

## What To Do Next

1. Implement route ownership cutover in hypervisor.
2. Add integration tests asserting single-owner runtime behavior for HTTP + WS.
3. Add telemetry for runtime mismatch and reconnect storm detection.
4. Validate live/dev swap under full desktop runtime load.

## Context

Current architecture mixes path ownership:

1. Hypervisor serves some desktop static assets from host build outputs.
2. Sandbox serves desktop API and websocket runtime.

This creates runtime contract mismatch potential and weakens the live/dev sandbox model
where agents should own and validate the full runtime inside sandbox boundaries.

## Decision

### Boundary Decision

1. Hypervisor is control plane:
   - auth/session pages and `/auth/*`
   - admin sandbox lifecycle
   - provider gateway
   - role-aware proxying
2. Sandbox is authenticated runtime plane:
   - desktop shell/root runtime
   - runtime assets and app APIs
   - app websocket endpoints

### Promotion Semantics

1. `live` and `dev` each own complete runtime surfaces.
2. promotion is route-role pointer swap, not host static swap.

## Alternatives Considered

### 1) Split Ownership (Rejected)

- Hypervisor serves static runtime assets while sandbox serves APIs/WS.
- Rejected due to recurrent runtime drift and reconnect loop risk.

### 2) Hybrid Transitional Host Fallback (Rejected as Target)

- Keep host runtime fallback for authenticated app paths during migration.
- Rejected as target state because it preserves ambiguity at ownership boundary.

### 3) Versioned App Prefixes (Deferred)

- Move runtime to explicit prefixes (for example `/app`, `/dev/app`).
- Deferred to a future migration if root-path contracts become a scaling constraint.

## Consequences

### Positive

1. Runtime coherency improves: one owner per role for bundle + API + WS.
2. Sandbox rewriteability model becomes real for live/dev flows.
3. Promotion safety improves: fewer hidden cross-boundary dependencies.

### Negative

1. Requires careful route cutover and regression testing.
2. Increases pressure on sandbox packaging/runtime-serving discipline.
3. Requires additional observability around proxy/role/runtime mismatches.

## Compatibility Constraints

1. Unauthenticated auth flow must remain stable (`/login`, `/register`, `/recovery`).
2. Admin and provider-gateway behavior must remain stable.
3. Existing live/dev role model and swap API semantics remain intact.

## Re-evaluation Triggers

Revisit this ADR when any of the following becomes true:

1. Multi-tenant routing requires tenant-prefixed runtime segmentation.
2. Runtime CDN offload becomes a hard performance requirement.
3. Security boundary changes require stronger isolation primitives.
4. Product requires independent version negotiation between control-plane and runtime-plane.

## References

- `docs/architecture/2026-02-22-sandbox-owned-ui-runtime-spec.md`
- `docs/architecture/adr-0003-hypervisor-sandbox-secrets-boundary.md`
- `hypervisor/src/main.rs`
- `hypervisor/src/middleware.rs`
- `tests/playwright/proxy-integration.spec.ts`
