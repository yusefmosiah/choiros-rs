# OVH US-East Bootstrap Runbook: Secrets + Compute Lifecycle

Date: 2026-03-03
Status: Active
Owner: Platform / Runtime / Infra

## Narrative Summary (1-minute read)

This runbook is the operator path for bootstrapping ChoirOS on two OVH US-East `SYS-1` hosts with:

1. OVH service-account OAuth2 API auth.
2. OVH Secret Manager as the secret value store.
3. OVH KMS for cryptographic key operations.
4. Host-rendered runtime credential files consumed by `LoadCredential`.
5. Low-complexity two-node compute lifecycle with explicit failover.

## What Changed

1. Replaced “server not procured” assumptions with concrete host inventory.
2. Added a direct bootstrap flow for OVH secrets and host lifecycle API usage.
3. Added operator checklist for current `ensure|stop` runtime lifecycle and phased expansion.

## What To Do Next

1. Complete Section 1 (identity + policies) and Section 2 (secret seeding).
2. Implement Section 3 host secret-sync units.
3. Execute Section 5 failover drill.
4. Close Section 6 gaps for snapshot lifecycle.

## 0) Host Inventory (Authoritative)

1. `ns1004307.ip-51-81-93.us` (`51.81.93.94`) - `SYS-1 | Intel Xeon-E 2136` - `us-east-vin`
2. `ns106285.ip-147-135-70.us` (`147.135.70.196`) - `SYS-1 | Intel Xeon-E 2136` - `us-east-vin`

Bootstrap role plan:

1. Node A (`51.81.93.94`) = primary ingress + control plane + runtime.
2. Node B (`147.135.70.196`) = runtime + warm standby for control plane.

## 1) OVH API Identity and Policy

### 1.1 Create service account

Create one dedicated service account for Choir control plane automation.

Use OAuth2 token flow for API calls and avoid new legacy app-key integrations.

### 1.2 Minimum permissions

Grant only required actions for bootstrap:

1. Dedicated server read/status/reboot/task operations.
2. Secret Manager read/list/version access.
3. KMS key operations needed for token signing/encryption.

Examples visible in OVH docs:

1. `dedicatedServer:apiovh:get`
2. `dedicatedServer:apiovh:reboot`
3. `secretmanager:apiovh:secret/get`
4. `secretmanager:apiovh:secret/access`

## 2) Seed Secrets in OVH Secret Manager

Create one Secret Manager resource for platform secrets.

Seed these values:

1. `zai_api_key`
2. `kimi_api_key`
3. `openai_api_key`
4. `inception_api_key`
5. `tavily_api_key`
6. `brave_api_key`
7. `exa_api_key`
8. `provider_gateway_token`

Store with versioning enabled; use rotation by creating new versions.

## 3) Render Secrets on Hosts (`/run/...`)

### 3.1 Bootstrap file (host-local)

Install one root-only bootstrap file on each host (`0600`) containing only service-account
bootstrap credential material required to obtain short-lived OAuth2 tokens.

### 3.2 Secret sync unit

Install:

1. `choiros-secrets-sync.service` (one-shot)
2. `choiros-secrets-sync.timer` (periodic refresh)

Required behavior:

1. Fetch short-lived OVH API token.
2. Read Secret Manager values.
3. Write atomically to `/run/choiros/credentials/platform/<secret_name>`.
4. Apply `root:root` + `0400`.
5. Preserve last known good file on transient API failures.

### 3.3 Wire hypervisor credentials

Configure `LoadCredential=` entries (including `provider_gateway_token`) and consume through
`$CREDENTIALS_DIRECTORY`.

No `EnvironmentFile` secret values in production units.

## 4) Bring-Up Sequence (Two Nodes)

1. Install NixOS baseline on both nodes.
2. Bootstrap API identity and secret resources (Sections 1-2).
3. Install secret-sync units and verify files under `/run/choiros/credentials/platform/`.
4. Deploy Choir binaries and `nixos-rebuild switch`.
5. Verify:
   1. `hypervisor` active
   2. `container@sandbox-live` active
   3. `container@sandbox-dev` active
6. Run health checks:
   1. `curl -fsS http://127.0.0.1:9090/login`
   2. `curl -fsS http://127.0.0.1:8080/health`
   3. `curl -fsS http://127.0.0.1:8081/health`

## 5) Compute Lifecycle Operations (Bootstrap)

### 5.1 What exists now

Current runtime lifecycle is `ensure|stop` through hypervisor admin routes and vfkit control path.

### 5.2 Operator lifecycle checklist

1. Start runtime:
   1. `POST /admin/sandboxes/{user_id}/{role}/start`
2. Stop runtime:
   1. `POST /admin/sandboxes/{user_id}/{role}/stop`
3. Branch runtime:
   1. `POST /admin/sandboxes/{user_id}/branches/{branch}/start`
   2. `POST /admin/sandboxes/{user_id}/branches/{branch}/stop`
4. Snapshot status visibility:
   1. `GET /admin/sandboxes`
   2. (current output is runtime status snapshot, not VM memory snapshot)

### 5.3 Failover drill

1. Mark Node A drained (no new starts).
2. Route new starts to Node B.
3. Validate login -> desktop -> prompt loop on Node B ingress.
4. Restore Node A and reverse the drain.
5. Record timings and errors.

## 6) Immediate Gaps To Close

1. Add `provider_gateway_token` to credential-file loading path end-to-end.
2. Add backend adapter for OVH/Linux `cloud-hypervisor`.
3. Add full lifecycle APIs from ADR-0010:
   1. `create/start/stop/snapshot/restore/delete/get/list`
4. Add snapshot retention/GC and restore throttling.

## 7) Capacity Envelope for This Purchase

Using ADR-0010 assumptions and `2 vCPU / 3 GiB` baseline:

1. Per node: about `5` SLO-safe active sessions (`7-8` stretch), `~71-107` parked snapshots.
2. Two nodes: about `10` SLO-safe active sessions (`14-16` stretch), `~142-214` parked snapshots.

This is aligned with the stated bootstrap experiment target.

## 8) References

1. `docs/architecture/adr-0008-ovh-selfhosted-secrets-architecture.md`
2. `docs/architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md`
3. `docs/architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md`
4. `nix/modules/choiros-platform-secrets.nix`
5. `hypervisor/src/provider_gateway.rs`
