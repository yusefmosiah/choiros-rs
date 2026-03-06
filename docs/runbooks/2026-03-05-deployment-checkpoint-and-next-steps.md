# Deployment Checkpoint and Next Steps

Date: 2026-03-05 (updated)
Status: Active
Owner: Platform / Runtime / Infra

## Narrative Summary (1-minute read)

ChoirOS is running on two OVH bare metal nodes. Gate 1 (persistent secrets) is complete.
Gate 2 (cloud-hypervisor microVMs) is partially complete — the VM boots NixOS, networking
works (TAP + bridge), but the sandbox service inside the VM isn't confirmed reachable yet
(firewall fix committed but untested). CI/CD pipeline is live: push to main → fmt + test →
deploy to Node A.

The next priority is bootstrapping: developing ChoirOS inside ChoirOS. This means fixing
bugs that block the self-hosting loop, then evolving the user sandbox VM to include what's
needed for development workflows.

## What Changed (Latest)

1. **CI/CD pipeline live** — GitHub Actions: fmt check → cargo test → deploy to Node A
2. **Gate 1 complete** — Persistent secrets at `/opt/choiros/secrets/`, materialized to
   `/run/choiros/credentials/` on boot via oneshot service
3. **Gate 2 in progress** — cloud-hypervisor VM boots NixOS, ping works from host,
   sandbox binary dependencies available via virtiofs /nix/store share
4. **Deploy key** — Dedicated `github-actions-deploy@choiros` SSH key for CI (not personal key)
5. **Stale EC2/AWS secrets removed** from GitHub

## Current State

### Working
- [x] Two NixOS nodes (A: 51.81.93.94, B: 147.135.70.196)
- [x] Caddy TLS on Node A for choir-ip.com
- [x] Persistent secrets (reboot-safe)
- [x] CI/CD: push → test → deploy to Node A
- [x] Login/signup (WebAuthn), Desktop loads, Conductor runs
- [x] LLM calls through provider gateway (Haiku 4.5 default)
- [x] All providers proxied (Bedrock, Z.ai, Kimi, OpenAI, Inception, Tavily, Brave, Exa)

### Known Bugs (blocking bootstrap)
- [x] ~~Writer "Failed to canonicalize sandbox root"~~ — Fixed (85651cc, 237ca75)
- [x] ~~Writer draft.md race condition~~ — Fixed (frontend retries with backoff)
- [x] ~~Writer reprompting deadlock~~ — Fixed (90e5e66, tokio::spawn background task)
- [x] ~~Model defaults all Haiku~~ — Writer/conductor upgraded to Sonnet 4.6
- [ ] **VM state lost on restart (FATAL)** — idle watchdog kills VM, all in-VM data gone, no snapshotting
- [ ] **`last_activity` not updated by browsing** — only proxy requests reset idle timer, reading docs doesn't
- [ ] **502 on cold boot** — VM killed by watchdog, next request gets 502 during ~2min cold boot
- [ ] **WebSocket disconnect on idle** — no ping keepalive, connections die silently
- [ ] Writer circular revisions — reprompt creates multiple versions that overwrite each other
- [ ] Writer citation markers broken — `[^s1]` in doc body doesn't link to sidebar sources
- [ ] Writer reprompt runs invisible in trace — events not recognized by trace parser

### Incomplete
- [ ] Gate 2: sandbox inside cloud-hypervisor VM not confirmed reachable (firewall fix untested)
- [ ] Gate 2: dev VM runner not built yet (only live VM tested)
- [ ] Node B deployed but not receiving traffic (no LB health checks)
- [ ] Clippy has 30+ pre-existing warnings (non-blocking in CI for now)

## CI/CD Pipeline

```
push to main → Format check → Cargo test → Deploy to Node A
                                              (pull, nix build on server, restart)
```

No nix builds in CI — if tests pass, we build on the server. Build errors caught there.

GitHub secrets: `FLAKEHUB_AUTH_TOKEN`, `OVH_DEPLOY_SSH_KEY`, `OVH_NODE_A_HOST`, `OVH_NODE_B_HOST`

## Path to Bootstrap (Developing ChoirOS Inside ChoirOS)

### Priority 1: Fix Writer Bugs
The Writer is the primary development surface. Remaining bugs:
1. ~~Fix "Failed to canonicalize sandbox root" error~~ DONE
2. ~~Fix draft.md race condition~~ DONE
3. ~~Fix reprompting deadlock~~ DONE
4. Fix circular revisions (version-aware writer context)
5. Fix citation markers (system prompt + citation index contract)
6. Fix reprompt trace visibility (parser recognition)
See: `docs/checkpoints/2026-03-06-writer-tracing-bootstrap-checkpoint.md`

### Priority 2: Complete Gate 2 (Cloud-Hypervisor)
1. Test firewall fix (rebuild VM on server)
2. Verify sandbox reachable through socat forwarder
3. Build dev VM runner
4. End-to-end: login → desktop → prompt → LLM response through microVM

### Priority 3: User Sandbox VM for Development
What does the sandbox VM need for self-hosting development workflows?
- Git (clone/pull/push)
- Cargo / Rust toolchain (or build via nix)
- File editing (Writer mutations)
- Branch isolation (diverging user branches vs global updates)

### Priority 4: Branch Isolation Design
Open question: what does it mean to have diverging user branches and global sandbox updates?
- Per-user git branches inside VMs?
- NixOS containers per branch inside each VM?
- How do global platform updates propagate to user-customized branches?
- Snapshot/restore for branch switching?

### Later Gates (unchanged)
- Gate 3: Lifecycle API (start/stop/get/list with node placement)
- Gate 4: Snapshot Lifecycle (snapshot/restore/delete for session parking)
- Gate 5: Two-Node Operations (failover drill, load distribution)

## Deploy Procedure

```bash
# Automatic (CI/CD on push to main):
# fmt check → test → SSH to Node A → pull → nix build → copy bins → restart

# Manual (when needed):
ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94
cd /opt/choiros/workspace
git pull --ff-only origin main
nix build ./sandbox#sandbox -o result-sandbox
nix build ./hypervisor#hypervisor -o result-hypervisor
cp -f result-sandbox/bin/sandbox /opt/choiros/bin/sandbox
cp -f result-hypervisor/bin/hypervisor /opt/choiros/bin/hypervisor
systemctl restart hypervisor

# For NixOS config changes:
nixos-rebuild switch --flake .#choiros-a

# For VM image changes:
nix build ".#nixosConfigurations.choiros-ch-sandbox-live.config.microvm.runner.cloud-hypervisor" -o result-vm-live
```

## Files Reference

### Active
- `nix/hosts/ovh-node.nix` — shared host config (secrets materialization, bridge, NAT)
- `nix/hosts/ovh-node-a.nix`, `ovh-node-b.nix` — per-node config
- `nix/ch/sandbox-vm.nix` — cloud-hypervisor guest NixOS config
- `nix/modules/choiros-platform-secrets.nix` — secrets wiring module
- `scripts/ops/ovh-runtime-ctl.sh` — cloud-hypervisor VM lifecycle manager
- `.github/workflows/ci.yml` — CI/CD pipeline
- `hypervisor/src/provider_gateway.rs` — gateway with Bedrock rewrite
- `sandbox/src/actors/model_config.rs` — uniform provider registration
- `sandbox/src/actors/writer/mod.rs` — Writer actor (has bugs)
