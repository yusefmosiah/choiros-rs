# ADR-0003: Hypervisor-Sandbox Secrets Boundary

Date: 2026-02-20
Kind: Decision
Status: Draft
Priority: 4
Requires: [ADR-0008, ADR-0014]
Owner: ChoirOS runtime and deployment

## Implementation Verification

**Last Verified**: 2026-03-16
**Verification Status**: ⚠️ PARTIALLY IMPLEMENTED WITH ACTIVE BOUNDARY GAPS

### Verified Components (Platform Secrets)
- ✅ Provider Gateway in hypervisor (`hypervisor/src/provider_gateway.rs`)
- ✅ Provider/search API keys remain hypervisor-side in managed runtime mode
- ✅ Keyless sandbox policy blocks provider/search key envs in managed sandboxes
- ✅ Sandbox auto-detects managed mode, skips `.env` loading
- ✅ Managed model/search calls route through provider gateway with token auth

### Implementation Gaps (Current)
- ❌ No `user_secrets` table in database
- ❌ No `/me/secrets` API endpoints
- ❌ No user-level secret broker for GitHub tokens, personal API keys
- ❌ Shared provider-gateway auth token is still injected into managed runtimes
- ❌ OVH host wiring still mixes `LoadCredential` with `EnvironmentFile` compatibility
- ❌ Standalone/dev mode still supports direct search-provider env keys outside managed mode

### To Complete
1. Remove guest-visible provider-gateway token delivery from env/kernel cmdline paths
2. Eliminate remaining `EnvironmentFile` compatibility wiring for control-plane secrets
3. Add `user_secrets` table and `/me/secrets` API
4. Implement secret broker for user-level secrets with audit metadata only

## Narrative Summary (1-minute read)

For MVP, ChoirOS treats the hypervisor as the trusted control plane and each sandbox as
untrusted compute. Platform secrets stay in hypervisor scope only. Sandboxes can access
user-level secrets only through a policy-checked broker API, never by direct store access.

This keeps the short-term architecture simple enough for EC2 + container rollout while
preserving a clean upgrade path to stronger isolation later.

## What Changed

1. Defined explicit trust boundary: platform secrets are never visible to sandboxes.
2. Defined user-level secrets as scoped resources brokered by hypervisor policy.
3. Added MVP-compatible API and storage requirements for future hardening.
4. Added non-goals to avoid overbuilding before deployment value is reached.

## What To Do Next

1. Remove guest-visible provider-gateway token delivery from the OVH runtime path.
2. Add minimal hypervisor user-secret storage and policy-checked broker APIs.
3. Add audit metadata for secret access without recording secret values.
4. Keep direct provider env reads limited to local/dev paths until the broker exists.

## Context

ChoirOS is moving to a hypervisor + sandbox container model for Phase 6 deployment work.
The team is cost constrained and prioritizing MVP velocity. We still need a secrets model
that avoids accidental platform key exposure and remains compatible with stronger
isolation later.

## Decision

### Boundary

1. Hypervisor is the only platform-secret authority.
2. Target boundary: sandboxes do not receive platform secrets through env, files, APIs,
   logs, or events.
3. Current exception: the shared provider-gateway auth token is still injected into managed
   runtimes and must be removed to satisfy this ADR fully.
4. Sandboxes may request user-level secrets only through hypervisor broker endpoints.

### Secret classes

1. Platform secrets (global/infra):
   - Examples: provider API keys, signing keys, deployment credentials.
   - Scope: hypervisor only.

2. User-level secrets (tenant scoped):
   - Examples: user GitHub token, user API keys for personal integrations.
   - Scope: `(user_id, secret_name)` with optional workspace scope later.

### Access model

1. Sandbox requests a capability, not raw storage path.
2. Hypervisor policy maps capability -> allowed secret(s) for authenticated user.
3. Hypervisor returns only the value needed for the task/session.
4. Secret values are never written to EventStore payloads or logs.
5. Managed runtime search/model routes already require gateway routing; direct env-key reads
   remain a standalone/dev-mode fallback only.

## MVP Requirements

1. Hypervisor-managed encrypted-at-rest storage for user-level secrets.
2. Minimal API surface:
   - `PUT /me/secrets/:name`
   - `DELETE /me/secrets/:name`
   - internal broker resolution endpoint for sandbox capability calls
3. Audit metadata events for access:
   - `user_id`, `sandbox_id`, `capability`, timestamp, allow/deny result
   - no raw secret values
4. Short-lived secret usage in sandbox context where possible.

## Security and Compatibility Constraints

1. No platform secret passthrough into sandbox startup environment.
2. No secret value material in websocket streams, tracing spans, or artifacts.
3. Secret IDs are stable; values are rotatable without API contract changes.
4. Design must remain compatible with future stronger isolation (microVM, VM-per-user,
   external KMS/Vault).

## Non-Goals (MVP)

1. Full enterprise secret manager integration in first pass.
2. Complex secret-sharing models across users/teams.
3. UI-complete secret management console before deployment baseline works.
4. Perfect containment against container escape in MVP phase.

## Consequences

### Positive

- Clear trust boundary from day one.
- Faster MVP path with lower redesign risk later.
- Better auditability for secret access decisions.

### Negative

- Adds broker/policy code before feature-rich UI exists.
- Requires discipline to keep logs/events secret-free.
- Some short-term friction for integrations expecting raw env injection.

## Re-evaluation Triggers

Revisit this ADR when any of the following becomes true:

1. Compliance requires external KMS/Vault-backed key management.
2. Multi-tenant enterprise controls require richer RBAC.
3. Sandbox hardening moves beyond container baseline.
4. Secret access volume requires dedicated policy service separation.

## References

- `docs/adr-0002-rust-nix-build-and-cache-strategy.md`
- `AGENTS.md`
