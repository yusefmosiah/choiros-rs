# Deployment Checkpoint and Next Steps

Date: 2026-03-05
Status: Active
Owner: Platform / Runtime / Infra

## Narrative Summary (1-minute read)

ChoirOS is running on two OVH bare metal nodes with a temporary topology: sandboxes run
as plain systemd services on the host (no isolation, no per-user VMs). This works for
proving the stack end-to-end but is not the target architecture.

The target (per ADR-0010, ADR-0011, ADR-0012) is: one cloud-hypervisor microVM per user
session (2 vCPU / 3 GiB), with NixOS containers inside each VM for branch isolation.
Secrets come from OVH Secret Manager, materialized to `/run/` via a sync service.

This document is the ops checklist to get from current state to target state.

## What Changed (This Session, 2026-03-05)

1. Provider gateway unified: all LLM providers (including Bedrock) route through the
   hypervisor gateway as plain HTTP. Sandbox has zero AWS/provider-specific code.
2. Gateway rewrites Anthropic Messages API to Bedrock InvokeModel format transparently.
3. Model defaults switched to ClaudeBedrockHaiku45 across all callsites.
4. CLAUDE.md and AGENTS.md gitignored (contain ops details).
5. Stale scripts removed: Ubuntu-era provisioning, container-topology deploy scripts,
   legacy tmux workflow, archived runbook stubs.
6. Justfile cleaned of references to removed scripts.

## Current State (What Works)

### Infrastructure
- [x] Two NixOS nodes deployed (Node A: 51.81.93.94, Node B: 147.135.70.196)
- [x] Caddy TLS on Node A for choir-ip.com
- [x] OVH IP Load Balancer configured (147.135.24.51 → both nodes)
- [x] SSH via `~/.ssh/id_ed25519_ovh`

### Services (Node A — fully deployed)
- [x] hypervisor.service (9090) — control plane + provider gateway
- [x] sandbox-live.service (8080) — production sandbox (choiros user)
- [x] sandbox-dev.service (8081) — dev sandbox (choiros user)
- [x] caddy.service (80/443) — TLS termination

### Provider Gateway
- [x] All LLM calls route through gateway (sandbox has no API keys)
- [x] Bedrock via bearer token auth (gateway rewrites /v1/messages → /model/{id}/invoke)
- [x] Z.ai, Kimi, OpenAI, Inception, Tavily, Brave, Exa all proxied
- [x] Gateway token auth between sandbox and hypervisor

### Application
- [x] Login/signup works (WebAuthn on choir-ip.com)
- [x] Desktop loads, prompt bar works
- [x] Conductor starts runs, writer creates documents
- [x] LLM calls succeed through gateway (Haiku 4.5 default)

### Known Functional Issues (deferred to after infra)
- [ ] Writer draft.md race condition (frontend opens before writer creates file)
- [ ] Writer shows raw search results, not rewritten document
- [ ] "Connecting" websocket indicator in bottom-right

## Current State (What's Temporary / Wrong)

### Secrets (manual, non-persistent)
- Secrets delivered via SCP to `/run/choiros/credentials/`
- `/run/` is tmpfs — **secrets do not survive reboot**
- No OVH Secret Manager integration yet
- No `choiros-secrets-sync` service yet
- Sandbox uses `EnvironmentFile` for gateway token (ADR-0012 says credential file)
- `choiros-platform-secrets.nix` module exists but is not imported by host config

### Compute (no isolation)
- Sandboxes run as bare systemd services on host (no VMs, no containers)
- `ovh-runtime-ctl.sh` is a no-op stub
- No cloud-hypervisor backend
- No per-user microVMs
- No branch containers inside VMs
- No snapshot/restore lifecycle

### Node B
- Services deployed and healthy, but not receiving traffic
- No load balancer health check integration
- Same manual secret delivery as Node A

## Deployment Procedure (Current — Temporary)

```bash
# 1. Local: commit and push
git push origin main

# 2. SSH to server, pull, build, deploy
ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94 '
  cd /opt/choiros/workspace &&
  git pull --ff-only origin main &&
  nix build ./sandbox#sandbox -o result-sandbox &&
  nix build ./hypervisor#hypervisor -o result-hypervisor &&
  cp -f result-sandbox/bin/sandbox /opt/choiros/bin/sandbox &&
  cp -f result-hypervisor/bin/hypervisor /opt/choiros/bin/hypervisor &&
  systemctl restart hypervisor sandbox-live sandbox-dev
'

# 3. Verify
ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94 '
  systemctl is-active caddy hypervisor sandbox-live sandbox-dev &&
  curl -fsS http://127.0.0.1:9090/login | head -1 &&
  curl -fsS http://127.0.0.1:8080/health &&
  curl -fsS http://127.0.0.1:8081/health
'
```

For NixOS config changes: add `nixos-rebuild switch` after git pull.

## Path to Target Architecture

### Gate 1: Persistent Secrets (reboot-safe)

**Goal:** Secrets survive reboot without manual SCP.

**Immediate option (simple, no OVH API dependency):**
1. Store secrets persistently at `/opt/choiros/secrets/` (root:root 0700, on NVMe)
2. Add a systemd oneshot `choiros-secrets-materialize.service` that copies from
   persistent storage to `/run/choiros/credentials/` on boot
3. Wire as `Before=hypervisor.service sandbox-live.service sandbox-dev.service`
4. Import and configure `choiros-platform-secrets.nix` in `ovh-node.nix`
5. Move sandbox gateway token from `EnvironmentFile` to credential file

**Target option (ADR-0012 compliant):**
1. Set up OVH service-account OAuth2 (Section 1 of bootstrap runbook)
2. Seed secrets in OVH Secret Manager (Section 2)
3. Implement `choiros-secrets-sync` timer + oneshot that fetches from Secret Manager
   and renders to `/run/choiros/credentials/platform/`
4. Only bootstrap credential lives on-disk permanently

**Decision needed:** Ship the simple option now and migrate to Secret Manager later?
Or go straight to Secret Manager? The simple option is a 1-hour task. Secret Manager
integration is a multi-session project.

### Gate 2: Cloud-Hypervisor Backend

**Goal:** Sandboxes run in cloud-hypervisor microVMs instead of bare systemd services.

**Prerequisites:**
- Gate 1 complete (secrets survive reboot)

**Steps:**
1. Install cloud-hypervisor on NixOS hosts (add to system packages or overlay)
2. Create a minimal NixOS guest image for sandbox microVMs
   - Based on current `nix/vfkit/user-vm.nix` pattern but for cloud-hypervisor
   - Contains sandbox binary, minimal userspace
3. Implement cloud-hypervisor backend in `ovh-runtime-ctl.sh` (or Rust binary)
   - `ensure`: create VM if not running, boot guest image, start sandbox inside
   - `stop`: graceful shutdown of VM
4. Update `hypervisor/src/sandbox/mod.rs` to use cloud-hypervisor backend
5. Remove bare systemd sandbox-live/sandbox-dev services from `ovh-node.nix`
6. Test: login → desktop → prompt → LLM response through microVM

**Architecture (per ADR-0010/0011):**
```
Host (NixOS)
├── hypervisor.service (control plane, port 9090)
├── caddy.service (TLS, port 443)
└── cloud-hypervisor VMs (one per user session)
    └── NixOS guest (2 vCPU / 3 GiB)
        ├── sandbox process (port 8080)
        └── containers (one per branch)
```

### Gate 3: Lifecycle API (Phase 1)

**Goal:** `start/stop/get/list` with node placement.

**Prerequisites:**
- Gate 2 complete (cloud-hypervisor running)

**Steps:**
1. Implement VM lifecycle state machine in hypervisor
   (`creating → stopped → running → stopping → stopped`)
2. Add REST API endpoints per ADR-0010:
   - `POST /v1/vms` (create)
   - `POST /v1/vms/{vm_id}/start`
   - `POST /v1/vms/{vm_id}/stop`
   - `GET /v1/vms/{vm_id}`
   - `GET /v1/vms?owner_id=...`
3. Add node placement metadata for two-node routing
4. Add host-drain mode

### Gate 4: Snapshot Lifecycle (Phase 2)

**Goal:** `snapshot/restore/delete` for session parking.

**Prerequisites:**
- Gate 3 complete

**Steps:**
1. Implement cloud-hypervisor snapshot/restore
2. Add `POST /v1/vms/{vm_id}/snapshot`
3. Add `POST /v1/vms/{vm_id}/restore`
4. Add `DELETE /v1/vms/{vm_id}`
5. Add snapshot quota, retention policy, restore throttle
6. Idle timeout → auto-snapshot parking

### Gate 5: Two-Node Operations

**Goal:** Failover drill, load distribution.

**Prerequisites:**
- Gates 1-3 complete

**Steps:**
1. Deploy identical stack to Node B
2. Configure load balancer health checks
3. Run failover drill (drain A → route to B → restore A)
4. Document operator runbook for failover
5. Capture timing and error evidence

## Files Reference

### Active (keep, update as needed)
- `nix/hosts/ovh-node.nix` — shared host config
- `nix/hosts/ovh-node-a.nix`, `ovh-node-b.nix` — per-node config
- `nix/hosts/ovh-node-disk-config.nix` — disko partitioning
- `nix/modules/choiros-platform-secrets.nix` — secrets wiring module (needs expansion)
- `scripts/ops/ovh-runtime-ctl.sh` — runtime lifecycle dispatch (needs cloud-hypervisor backend)
- `scripts/ops/ovh-account-setup.sh` — OVH API identity setup
- `scripts/ops/check-flakehub-cache.sh` — build cache validation
- `scripts/ops/validate-local-provider-matrix.sh` — provider pre-deploy check
- `hypervisor/src/provider_gateway.rs` — gateway with Bedrock rewrite
- `sandbox/src/actors/model_config.rs` — uniform provider registration

### Stale (removed this session)
- `scripts/deploy/` — used nixos-container topology
- `scripts/ops/apply-release-manifest.sh` — container references
- `scripts/ops/build-release-manifest.sh` — container references
- `scripts/ops/host-state-snapshot.sh` — container references
- `scripts/ops/promote-grind-to-prod.sh` — container references
- `scripts/provision-server.sh` — Ubuntu/EC2 era
- `scripts/setup-ec2-env.sh` — EC2 era
- `scripts/dev-workflow.sh` — pre-hypervisor tmux

### ADR Authority Chain
- ADR-0008: Secrets architecture (source of truth = OVH Secret Manager)
- ADR-0010: VM lifecycle API (80/20 minimal surface)
- ADR-0011: State/compute decoupling + runtime modes
- ADR-0012: Two-node bootstrap secrets + compute lifecycle (master plan)
