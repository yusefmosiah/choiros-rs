# ADR-0008: OVH Self-Hosted Secrets Architecture (No Repo Secrets, No Sandbox Secrets)

Date: 2026-03-01
Kind: Decision
Status: Accepted
Requires: []
Owner: Platform / Runtime / Infra
Supersedes: ADR-0003 implementation approach for platform secret storage

## Narrative Summary (1-minute read)

ChoirOS will move to a strict control-plane secrets model for OVH self-hosting:

1. No secret material in git (plaintext or encrypted).
2. No provider or user secret values in sandbox runtimes, images, or env.
3. Control plane is the only secret authority.
4. NixOS remains declarative for wiring, but secret values are delivered at runtime via
   self-hosted secret infrastructure and systemd credentials.

The runtime plane (per-user VM + per-branch containers) is keyless. It can request capabilities,
not secret values. Provider access and user-secret resolution happen in control-plane services
(provider gateway + secrets broker).

## Implementation Reality (2026-03-16)

Current repo state is only a partial match for this target policy:

1. Managed sandboxes already block direct provider/search key envs and require gateway routing.
2. OVH hosts still materialize credentials from persistent host files under
   `/opt/choiros/secrets/platform/*`, not from a runtime-fetched secret backend.
3. OVH hypervisor wiring still mixes `LoadCredential` with an `EnvironmentFile` compatibility
   path.
4. The shared provider-gateway token is still relayed into managed runtimes through VM state,
   kernel cmdline, and guest env reconstruction.
5. User-secret broker storage and APIs remain unimplemented.

## What Changed

1. Replaced the "committed encrypted secrets" deployment pattern with "runtime-fetched secrets".
2. Defined Nix/systemd credential delivery as the production secret injection mechanism.
3. Added explicit per-user secret broker contract and storage model.
4. Added migration path from current partial ADR-0003 implementation.

## What To Do Next

1. Implement `secrets-broker` in hypervisor/control plane.
2. Replace persistent `/opt/choiros/secrets/platform/*` host materialization with a
   runtime-fetched secret backend.
3. Remove hypervisor `EnvironmentFile` compatibility loading and keep secrets on the
   `LoadCredential` + `$CREDENTIALS_DIRECTORY` path only.
4. Replace guest-visible provider-gateway token relay with short-lived runtime credentials.
5. Add integration tests that prove keyless sandbox and scoped broker/gateway behavior.

## Context

Current repo state has four contradictions with target policy:

1. OVH hosts still materialize platform credentials from persistent files under
   `/opt/choiros/secrets/platform/*`.
2. OVH host config still maintains `/run/choiros/credentials/sandbox` even though managed
   runtimes should be keyless.
3. Hypervisor still carries an `EnvironmentFile` compatibility path alongside
   `LoadCredential`.
4. Managed sandboxes are keyless for provider/search keys, but the shared provider-gateway
   token is still passed into the guest runtime.

This violates the intended 3-tier boundary where the control plane owns secret authority and
runtime plane is untrusted/keyless compute.

## Decision

### 1) Trust Boundaries (Authoritative)

1. Control plane (hypervisor + gateway + broker): trusted secret authority.
2. Runtime plane (user VM + branch containers): untrusted/keyless.
3. Client plane (web/desktop/mobile): never receives raw platform secrets.

### 2) Secret Classes

1. Platform secrets:
   - Provider API credentials and infrastructure tokens.
   - Scope: control-plane services only.
2. User secrets:
   - Per-user integration secrets (for example personal API tokens).
   - Scope: `(user_id, secret_name)` with policy-gated capability mapping.

### 3) Hard Rules

1. No secret files committed to repo (including encrypted blobs).
2. No provider/user secret values in sandbox env, filesystems, or logs.
3. No secret values in EventStore/EventBus/WebSocket traces.
4. Capability requests from runtime must be authenticated, authorized, audited, and short-lived.

## OVH Target Architecture

## Control Plane

- `identity`: authentication/session service.
- `runtime-orchestrator` (hypervisor): routes to per-user runtime.
- `provider-gateway`: all upstream provider calls, signs/auths with platform keys.
- `secrets-broker`: per-user secret CRUD + resolve-by-capability.
- `secret-store`: self-hosted OpenBao/Vault cluster (Raft), private network only.

## Runtime Plane

- Per-user VM.
- Per-branch containers.
- Sandboxes run without provider/user secret values.

## Client Plane

- Requests go through control plane.
- Clients never receive platform credentials.

## Nix Pattern (Production)

Use NixOS for service topology and hardening, not for storing secret values.

### Required pattern

1. Deploy control-plane services with NixOS modules.
2. Run self-hosted OpenBao/Vault as a NixOS-managed service.
3. Run agent sidecar/service to fetch+renew runtime credentials and render files into `/run/...`.
4. Pass credentials into services with `systemd` `LoadCredential=`.
5. Service code reads secret files from `$CREDENTIALS_DIRECTORY`.

### Forbidden pattern

1. `Environment=`/`EnvironmentFile=` as the long-term production secret path.
2. Committed encrypted secrets as production source of truth.
3. Passing provider keys into runtime containers.

Current OVH wiring still contains an `EnvironmentFile` compatibility path for hypervisor boot;
that is implementation debt, not the desired steady state.

### Example unit wiring pattern (shape)

```nix
systemd.services.hypervisor.serviceConfig = {
  LoadCredential = [
    "aws_bedrock:/run/choiros/credentials/platform/aws_bedrock"
    "zai_api_key:/run/choiros/credentials/platform/zai_api_key"
    "kimi_api_key:/run/choiros/credentials/platform/kimi_api_key"
    "openai_api_key:/run/choiros/credentials/platform/openai_api_key"
    "tavily_api_key:/run/choiros/credentials/platform/tavily_api_key"
    "brave_api_key:/run/choiros/credentials/platform/brave_api_key"
    "exa_api_key:/run/choiros/credentials/platform/exa_api_key"
  ];
};
```

Rust service code then resolves credentials via `$CREDENTIALS_DIRECTORY/*` rather than env.

## Runtime-to-Control Authentication

Replace static long-lived gateway token model with short-lived runtime tokens.

1. Runtime requests signed with short TTL credential (`exp` in minutes).
2. Claims include `user_id`, `runtime_id`, `scope`, `nonce`.
3. Provider-gateway and secrets-broker verify signature + scope + expiry.
4. Replays prevented with nonce/cache window.

## Per-User Secrets Broker Contract

Minimum API surface:

1. `PUT /me/secrets/:name`
2. `DELETE /me/secrets/:name`
3. Internal `POST /internal/secrets/resolve` with capability envelope

Policy behavior:

1. Input: `(user_id, runtime_id, capability, secret_name?)`.
2. Policy map determines allowed secret(s).
3. Return only requested value and only for authorized capability.
4. Emit audit metadata only (allow/deny, principal, capability, runtime, latency).

Storage behavior:

1. Values stored in secret-store KV.
2. Metadata/index in hypervisor DB (`user_secrets` table).
3. Optional envelope encryption split (Transit/KMS style) for app-layer ciphertext storage.

## Concrete Gaps to Close in Current Code

1. Add `user_secrets` schema and APIs (currently absent).
2. Remove the OVH sandbox credential materialization path and `/run/choiros/credentials/sandbox`.
3. Replace guest-visible provider-gateway token delivery with short-lived runtime auth.
4. Stop loading hypervisor provider keys from `EnvironmentFile`; keep the OVH path on
   `LoadCredential` only.
5. Replace persistent host secret materialization with runtime-fetched secrets.
6. Ensure managed runtimes propagate only non-secret routing metadata.

## Migration Plan

### Phase A: Boundary Enforcement

1. Keep sandbox keyless mode enforced for provider/search key env names.
2. Keep gateway mandatory for managed mode researcher/model routes.
3. Remove the remaining guest-visible provider-gateway token relay.
4. Add failing integration tests for secret presence in sandbox env/cmdline.

### Phase B: Control-Plane Secret Delivery

1. Introduce self-hosted secret-store and agent on control-plane hosts.
2. Replace `/opt/choiros/secrets/platform/*` materialization with runtime-fetched secrets.
3. Migrate hypervisor/provider-gateway key reads fully onto credential files.
4. Remove the OVH `EnvironmentFile` compatibility path.

### Phase C: User Secret Broker

1. Add DB migration (`user_secrets` metadata/index).
2. Implement CRUD and resolve APIs with audit events.
3. Add capability policy mapping and deny-by-default behavior.

### Phase D: Token Hardening

1. Replace static runtime gateway token with short-lived signed runtime credentials.
2. Add replay protection and scope checks.

## Verification Criteria

1. `git grep` finds no committed secret values or encrypted secret artifacts used by production path.
2. Managed sandbox startup fails if any provider key env is present.
3. Research/model calls in managed mode succeed only via provider gateway.
4. User secret resolution succeeds only for authorized capability and user scope.
5. Audit logs contain metadata but never secret values.
6. Restart/redeploy leaves control plane functional with runtime credential renewal.

## Consequences

### Positive

1. Matches 3-tier trust model.
2. Removes secret sprawl from repo and runtime plane.
3. Improves revocation and rotation workflows.

### Tradeoffs

1. Adds control-plane dependency on self-hosted secret infrastructure.
2. Requires broker/gateway contract hardening and additional tests.
3. Increases initial operations complexity (agent + policy bootstrap).

## Alternatives Considered

1. Keep sops-nix with encrypted secrets in git:
   - Rejected for this policy target (no secret material in repo at all).
2. Pass secrets via env files only:
   - Rejected due to systemd security guidance and leak surface.
3. Put user secrets in runtime VM/container:
   - Rejected; breaks control-plane authority and keyless runtime principle.

## Source Notes (External Research)

Primary references used for this ADR:

1. systemd credential model and security properties (`LoadCredential`,
   `$CREDENTIALS_DIRECTORY`, encrypted credentials, credstore paths):
   - https://systemd.io/CREDENTIALS/
   - https://www.freedesktop.org/software/systemd/man/systemd.exec.html
   - https://man7.org/linux/man-pages/man5/systemd.exec.5.html
2. systemd guidance against env vars for secrets:
   - https://www.freedesktop.org/software/systemd/man/systemd.exec.html
3. sops-nix behavior and implications:
   - https://github.com/Mic92/sops-nix
4. Vault/OpenBao agent and template patterns:
   - https://developer.hashicorp.com/vault/docs/agent-and-proxy/agent/template
   - https://developer.hashicorp.com/vault/docs/agent-and-proxy/autoauth/methods/approle
   - https://openbao.org/docs/agent-and-proxy/agent/
   - https://openbao.org/docs/agent-and-proxy/agent/template/
5. Vault secret engines and broker-friendly primitives:
   - KV v2: https://developer.hashicorp.com/vault/docs/secrets/kv/kv-v2
   - Transit: https://developer.hashicorp.com/vault/docs/secrets/transit
6. Vault audit requirements and behavior:
   - https://developer.hashicorp.com/vault/docs/audit
   - https://developer.hashicorp.com/vault/docs/audit/best-practices

## Repo References

1. Existing ADR boundary and missing user-secret implementation:
   - `docs/theory/decisions/adr-0003-hypervisor-sandbox-secrets-boundary.md`
2. Legacy committed encrypted secrets path (removed from repo):
   - `infra/secrets/choiros-platform.secrets.sops.yaml`
3. Control-plane credential wiring module (LoadCredential-based):
   - `nix/modules/choiros-platform-secrets.nix`
4. Current sandbox keyless enforcement and gaps:
   - `sandbox/src/main.rs`
   - `sandbox/src/actors/researcher/providers.rs`
5. Hypervisor provider-key reads (env fallback + credential files):
   - `hypervisor/src/provider_gateway.rs`
