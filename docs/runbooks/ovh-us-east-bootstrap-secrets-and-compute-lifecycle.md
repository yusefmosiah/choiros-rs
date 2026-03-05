# OVH US-East Bootstrap Runbook: Secrets + Compute Lifecycle

Date: 2026-03-03
Updated: 2026-03-05
Status: Active (executing)
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

1. Verify Ubuntu 24.04 install completes on both nodes and SSH works.
2. Convert both nodes from Ubuntu to NixOS via `nixos-anywhere`.
3. Complete Section 1 (identity + policies) and Section 2 (secret seeding).
4. Implement Section 3 host secret-sync units.
5. Execute Section 5 failover drill.
6. Close Section 6 gaps for snapshot lifecycle.

## Execution Progress Log

### 2026-03-05: Host OS Bootstrap

**Completed:**
- [x] Created OVH v1 API application (`choiros-bootstrap`).
- [x] Minted Consumer Key with full API access (unlimited validity).
- [x] Verified API connectivity: `whoami`, `list-servers` both return expected data.
- [x] Uploaded SSH public key to OVH account.
- [x] Confirmed both servers visible: `os: none_64`, `state: ok`, `datacenter: vin1`.
- [x] Kicked off Ubuntu 24.04 install on Node A and Node B.
- [x] Install uses `/1.0/dedicated/server/{name}/reinstall` endpoint (US API; not `/install/start`).
- [x] SSH key injected via `customizations.sshKey` field in reinstall payload.
- [x] First install completed but SSH failed — original `id_ed25519` had unknown passphrase.
- [x] Generated dedicated OVH key (`~/.ssh/id_ed25519_ovh`, no passphrase).
- [x] Replaced OVH account SSH key (`wiz-ovh`) and reinstalled both nodes (tasks 26974571, 26974572).

**API notes (for future reference):**
- US OVH API base: `https://api.us.ovhcloud.com`
- US API uses `/reinstall` endpoint, not `/install/start` (EU API difference).
- v1 signed auth works; existing `ovh-account-setup.sh` only supports OAuth2 bearer for
  `whoami`/`list-servers` — v1 signed calls done inline for now.
- Credentials stored in `.env` (gitignored): `OVH_APPLICATION_KEY`, `OVH_APPLICATION_SECRET`,
  `OVH_CONSUMER_KEY`.
- OVH Ubuntu 24.04 installs create user `ubuntu` (not `root`) with SSH key auth.
- IPMI KVM HTML5 console available via API if SSH fails.

**SSH access (after install completes):**
```bash
ssh -i ~/.ssh/id_ed25519_ovh ubuntu@51.81.93.94   # Node A
ssh -i ~/.ssh/id_ed25519_ovh ubuntu@147.135.70.196 # Node B
```

- [x] Ubuntu 24.04 installed and SSH verified on both nodes.
  - User: `ubuntu` (passwordless sudo), not `root`.
  - Key: `~/.ssh/id_ed25519_ovh` (no passphrase, dedicated OVH key).
  - Hardware confirmed: 2x 954GB NVMe (RAID1), 32GB RAM, 12 threads, x86_64.

**SSH access (verified working):**
```bash
ssh -i ~/.ssh/id_ed25519_ovh ubuntu@51.81.93.94   # Node A (choiros-a)
ssh -i ~/.ssh/id_ed25519_ovh ubuntu@147.135.70.196 # Node B (choiros-b)
```

- [x] Created NixOS host configuration for x86_64-linux bare metal.
  - `nix/hosts/ovh-node.nix` (host config: UEFI GRUB, SSH, firewall, packages).
  - `nix/hosts/ovh-node-disk-config.nix` (disko: 2x NVMe RAID1 with ESP).
  - `flake.nix` updated with `nixosConfigurations.choiros-ovh-node` and `disko` input.
- [x] Enabled root SSH on both nodes (replaced OVH forced-command authorized_keys).
- [x] Ran `nixos-anywhere` on Node A from macOS (kexec -> disko -> install -> reboot).
- [x] Ran `nixos-anywhere` on Node B from macOS (same process, system closure cached).
- [x] NixOS boots and SSH works on both nodes post-conversion.

**SSH access (NixOS, verified working):**
```bash
ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94   # Node A
ssh -i ~/.ssh/id_ed25519_ovh root@147.135.70.196 # Node B
```

**Pending:**
- [ ] Set hostname per-node (currently both are `nixos`).
- [ ] Bootstrap secrets infrastructure (Sections 1-3).
- [ ] Deploy ChoirOS and verify health checks (Section 4).

## Fast Path Commands

Repository helper:

1. `./scripts/ops/ovh-account-setup.sh --help`
2. Script is local-only and gitignored by policy.

Suggested sequence:

1. Create OAuth2 client (service account):
   1. `OVH_APPLICATION_KEY=... OVH_APPLICATION_SECRET=... OVH_CONSUMER_KEY=...`
   2. `./scripts/ops/ovh-account-setup.sh create-client --name choiros-control-plane --description "ChoirOS control plane"`
2. Mint client-credentials token:
   1. `./scripts/ops/ovh-account-setup.sh mint-token --client-id <client_id> --client-secret <client_secret>`
3. Verify token + discover inventory:
   1. `OVH_OAUTH_ACCESS_TOKEN=... ./scripts/ops/ovh-account-setup.sh whoami`
   2. `OVH_OAUTH_ACCESS_TOKEN=... ./scripts/ops/ovh-account-setup.sh list-servers`
   3. `OVH_OAUTH_ACCESS_TOKEN=... ./scripts/ops/ovh-account-setup.sh list-resources --resource-name ns1004307.ip-51-81-93.us --resource-name ns106285.ip-147-135-70.us`
   4. `OVH_OAUTH_ACCESS_TOKEN=... ./scripts/ops/ovh-account-setup.sh list-actions --match 'dedicatedServer|secretmanager|kms'`
4. Create policy from a local JSON file (not committed):
   1. Create and edit a local file, for example `/tmp/iam-policy-bootstrap.json`, with discovered identity/URN/action values.
   2. `OVH_OAUTH_ACCESS_TOKEN=... ./scripts/ops/ovh-account-setup.sh create-policy --policy-file /tmp/iam-policy-bootstrap.json`

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

1. Install Ubuntu 24.04 via OVH `/reinstall` API with SSH key injection.
2. Verify SSH access to both nodes.
3. Convert Ubuntu to NixOS via `nixos-anywhere` with a bare-metal host config.
4. Bootstrap API identity and secret resources (Sections 1-2).
5. Install secret-sync units and verify files under `/run/choiros/credentials/platform/`.
6. Deploy Choir binaries and `nixos-rebuild switch`.
7. Verify:
   1. `hypervisor` active
   2. `container@sandbox-live` active
   3. `container@sandbox-dev` active
8. Run health checks:
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
