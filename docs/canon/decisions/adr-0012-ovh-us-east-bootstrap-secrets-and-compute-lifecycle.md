# ADR-0012: OVH US-East Bootstrap Secrets and Two-Node Compute Lifecycle

Date: 2026-03-03
Kind: Decision
Status: Accepted
Requires: []
Owner: Platform / Runtime / Infra
Extends: ADR-0008, ADR-0010, ADR-0011

## Narrative Summary (1-minute read)

ChoirOS now has two OVH US-East `SYS-1` servers and needs an exact bootstrap operating model.

This ADR locks that model:

1. Secrets source of truth is OVH Secret Manager (KV2/REST), with OVH KMS for cryptographic
   key operations.
2. OVH API auth is service-account OAuth2 first; new work does not use legacy app-key signing.
3. Host nodes keep only minimal bootstrap credentials and materialize runtime secret files under
   `/run/choiros/credentials/platform/*`.
4. Hypervisor/provider gateway consume secrets via `systemd` `LoadCredential` +
   `$CREDENTIALS_DIRECTORY`, not `EnvironmentFile`.
5. Compute lifecycle is split into:
   1. OVH host lifecycle (power/reboot/install/task/vRack) via OVH API.
   2. Choir session lifecycle (`create/start/stop/snapshot/restore/delete/get/list`) via the
      control-plane API from ADR-0010.
6. Bootstrap topology is two nodes in US-East-VIN with low-complexity failover and explicit
   progression from current `ensure|stop` behavior to full snapshot lifecycle.

## What Changed

1. Added authoritative two-node host inventory and role model for bootstrap.
2. Defined exact secret bootstrap, sync, and credential-delivery flow for OVH.
3. Defined control-plane API split between OVH host operations and Choir microVM/session
   operations.
4. Added realistic capacity envelope for the exact purchased `SYS-1` profile.
5. Defined concrete implementation gates to move from current code to target lifecycle surface.

## What To Do Next

1. Stand up OVH service account + policy set and validate OAuth2 token retrieval.
2. Create Secret Manager resource and seed platform keys.
3. Add host-level `choiros-secrets-sync` service/timer to render
   `/run/choiros/credentials/platform/*`.
4. Wire remaining secret consumers (especially provider-gateway shared token) to
   `$CREDENTIALS_DIRECTORY`.
5. Ship lifecycle API phases:
   1. `start/stop/get/list` with node placement awareness.
   2. `snapshot/restore/delete`.

## Context

### Procured Hosts (Authoritative)

1. `ns1004307.ip-51-81-93.us` (`51.81.93.94`) - `SYS-1 | Intel Xeon-E 2136` - `us-east-vin`
2. `ns106285.ip-147-135-70.us` (`147.135.70.196`) - `SYS-1 | Intel Xeon-E 2136` - `us-east-vin`

### Current Code Reality

1. Hypervisor runtime lifecycle is currently `ensure|stop` (vfkit control path), not full
   snapshot lifecycle:
   - `hypervisor/src/bin/vfkit-runtime-ctl.rs`
   - `hypervisor/src/sandbox/mod.rs`
2. `LoadCredential` wiring exists for provider/search keys:
   - `nix/modules/choiros-platform-secrets.nix`
3. Provider gateway already supports reading secrets from `$CREDENTIALS_DIRECTORY`:
   - `hypervisor/src/provider_gateway.rs`
4. Cloud-hypervisor backend target for OVH is documented but not implemented yet in runtime code.

## Decision

### 1) OVH API Version + Auth Policy

1. Use OVH API `v2` by default for new integration work.
2. Use `v1` endpoints when the required operation is not available in `v2`.
3. Use service-account OAuth2 for API access in automation and control-plane services.
4. Legacy API key/secret/consumer-key signing remains compatibility-only, not the default path.

### 2) Secret System of Record

Use:

1. OVH Secret Manager for provider/runtime secret values.
2. OVH KMS for cryptographic keys and signing/encryption operations.

Do not use:

1. Repo-committed secret artifacts.
2. Sandbox-level secret injection.
3. `Environment=`/`EnvironmentFile=` secret values for production.

### 3) Bootstrap Secret Flow (Exact)

#### 3.1 Bootstrap identity on each host

Each host keeps a root-only bootstrap credential file (`0600`) for OVH service-account OAuth2
exchange. This is the only long-lived host-resident bootstrap credential.

#### 3.2 Runtime secret materialization

Each host runs a `choiros-secrets-sync` unit (timer + one-shot):

1. Exchange service-account credential for short-lived access token.
2. Read required secrets from Secret Manager APIs.
3. Render files atomically to `/run/choiros/credentials/platform/<name>`.
4. Enforce `root:root` and `0400`.
5. Keep last known good file until successful refresh.

#### 3.3 Service consumption

`systemd` units load secrets with `LoadCredential=` and services read from
`$CREDENTIALS_DIRECTORY`. Values are never logged or passed into sandbox env.

### 4) Platform Secret Inventory (Bootstrap)

Required platform secrets:

1. `zai_api_key`
2. `kimi_api_key`
3. `openai_api_key`
4. `inception_api_key`
5. `tavily_api_key`
6. `brave_api_key`
7. `exa_api_key`
8. `provider_gateway_token` (shared runtime-to-gateway token; currently env-backed and must be
   moved to credential file path)

Optional:

1. `aws_bedrock` only if Bedrock path is still intentionally enabled.
2. `flakehub_auth_token` only on hosts where `determinate-nixd` login is enabled.

### 5) Two-Node Bootstrap Topology

Low-complexity initial assignment:

1. Node A (`51.81.93.94`):
   1. Primary ingress target.
   2. Control-plane services (hypervisor, provider gateway, auth/session storage).
   3. Runtime workloads.
2. Node B (`147.135.70.196`):
   1. Runtime workloads.
   2. Warm standby control-plane config (disabled or passive until failover drill).

Failover mode in bootstrap is explicit and operator-driven (no complex orchestrator required).

### 6) Compute Lifecycle Split

#### 6.1 Host lifecycle (OVH API)

Use OVH API for server-level operations only:

1. Inventory and status.
2. Reboot/power operations.
3. Reinstall/rescue tracking.
4. Task/status tracking.
5. Optional vRack attachment operations.

Representative path family:

1. `/1.0/dedicated/server/{serviceName}`
2. `/1.0/dedicated/server/{serviceName}/reboot`
3. `/1.0/dedicated/server/{serviceName}/install/status`
4. `/1.0/dedicated/server/{serviceName}/task`
5. `/1.0/dedicated/server/{serviceName}/vrack`

#### 6.2 Session/microVM lifecycle (Choir control plane)

Use Choir APIs for user/session runtime lifecycle:

1. `create`
2. `start`
3. `stop`
4. `snapshot`
5. `restore`
6. `delete`
7. `get`
8. `list`

This remains backend-agnostic (`vfkit` locally, `cloud-hypervisor` target on OVH).

### 7) Delivery Phases for Lifecycle

#### Phase 0 (now)

1. `ensure|stop` only (implemented).
2. Role/branch runtime tracking exists but no snapshot semantics.

#### Phase 1

1. Add explicit `start/stop/get/list` API shape aligned with ADR-0010.
2. Add node placement metadata for two-node routing decisions.
3. Add host-drain mode (no new starts on draining node).

#### Phase 2

1. Add `snapshot/restore/delete`.
2. Add snapshot quota and retention policy.
3. Add restore-throttle controls to avoid boot storms.

### 8) Capacity Envelope for Purchased Profile

Assumptions:

1. Host: `12` threads, `32 GiB` RAM.
2. Reserve: `20%` RAM for host/control-plane.
3. VM baseline: `2 vCPU / 3 GiB`.
4. CPU overcommit: `2.0`.
5. Snapshot size: `4-6 GiB`.
6. Usable snapshot disk per node: `~430 GiB` (2x512 NVMe soft RAID with reserve).

Per-node envelope:

1. Theoretical active: `min((12*2)/2, 25.6/3) = min(12, 8) = 8`.
2. SLO-safe active (`70%`): `~5`.
3. Stretch active: `7-8`.
4. Parked snapshots: `~71-107`.

Two-node envelope:

1. SLO-safe active: `~10`.
2. Stretch active: `~14-16`.
3. Parked snapshots: `~142-214`.

Inference:

This matches the stated experiment goal (`5-10` active users + `100-200` snapshots) with margin
if lifecycle parking and quotas are enforced.

## Operational Rails (Mandatory)

1. Sandbox/runtime plane remains keyless for provider/user secrets.
2. Provider/search calls in managed mode must route through provider gateway.
3. Secret access is audited with metadata only, no value logging.
4. Node drain + failover drills are required before external user ramp.
5. API operations are idempotent and state-machine validated.

## Verification Criteria

1. Secrets on hosts exist only under `/run/choiros/credentials/platform/*` and are not exported
   in process env.
2. Hypervisor/provider gateway boot succeeds with credential files only.
3. Managed sandbox fails if direct provider keys are injected.
4. OVH API automation can:
   1. List target servers.
   2. Fetch status.
   3. Trigger reboot in controlled test.
   4. Poll task completion.
5. Two-node failover drill succeeds with documented operator steps.

## Consequences

### Positive

1. Clean control-plane secret boundary aligned with ADR-0008.
2. Practical 80/20 two-node operating model with low complexity.
3. Clear split between host infra control and session runtime lifecycle.

### Tradeoffs

1. Requires one bootstrap credential on each host.
2. Adds a secret-sync service that must be monitored.
3. Full snapshot lifecycle remains phased, not immediate.

## Source Notes (External Research)

1. OVH API v2 principles and branch guidance:
   https://support.us.ovhcloud.com/hc/en-us/articles/30667077334291-OVHcloud-API-v2-Principles-of-Operation
2. OVH service-account OAuth2 usage:
   https://support.us.ovhcloud.com/hc/en-us/articles/43234764826771-Using-service-accounts-to-connect-to-OVHcloud-APIs
3. Managing service accounts by API:
   https://support.us.ovhcloud.com/hc/en-us/articles/30633737688603-Managing-OVHcloud-service-accounts-via-API
4. Secret Manager REST API patterns:
   https://help.ovhcloud.com/csm/de-secret-manager-rest-api?id=kb_article_view&sysparm_article=KB0072849
5. Secret Manager KV2 API patterns:
   https://help.ovhcloud.com/csm/en-ie-secret-manager-kv2-api?id=kb_article_view&sysparm_article=KB0072841
6. OVH KMS overview and endpoint model:
   https://support.us.ovhcloud.com/hc/en-us/articles/34887531180435-Using-OVHcloud-Key-Management-Service-KMS
7. OVH dedicated server API schema (host lifecycle operations):
   https://api.us.ovhcloud.com/1.0/dedicated/server.json
8. OKMS CLI reference:
   https://github.com/ovh/okms-cli

## Repo References

1. `docs/architecture/adr-0008-ovh-selfhosted-secrets-architecture.md`
2. `docs/architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md`
3. `docs/architecture/adr-0011-bootstrap-into-publishing-state-compute-decoupling.md`
4. `nix/modules/choiros-platform-secrets.nix`
5. `hypervisor/src/provider_gateway.rs`
6. `hypervisor/src/config.rs`
7. `hypervisor/src/sandbox/mod.rs`
8. `hypervisor/src/bin/vfkit-runtime-ctl.rs`
