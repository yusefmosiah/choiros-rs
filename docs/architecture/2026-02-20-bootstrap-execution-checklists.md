# ChoirOS Bootstrap Execution Checklists

Date: 2026-02-20  
Status: Active checklist  
Owner: ChoirOS runtime and deployment

## Narrative Summary (1-minute read)

These checklists translate the deployment runbook into gate-based execution items.
Each phase can be run as a GitHub issue. Do not advance phases without gate evidence.

## What Changed

1. Added phase-by-phase issue-ready checklists.
2. Made bootstrap objective explicit in execution criteria.
3. Added evidence and rollback requirements to every phase.

## What To Do Next

1. Open one GitHub issue per phase using these checklists.
2. Execute in order; attach evidence before closing each issue.
3. Stop phase progression if gate criteria are not met.

## Global Definition of Done (Bootstrap)

- [ ] One user can run `live` and `dev` sandboxes concurrently behind hypervisor.
- [ ] Hypervisor can route/swap between live/dev for rollback.
- [ ] Prior known-good config/image can be restored in bounded time.

## Phase 1 Issue: Local Podman Smoke

### Outcome

Hypervisor manages sandbox containers locally with the same contract intended for AWS.

### Gate

- [ ] Hypervisor starts/stops/restarts sandbox container locally.
- [ ] Auth + proxy path works through hypervisor.
- [ ] WebSocket task flow works end-to-end.

### Tasks

- [ ] Add/verify sandbox OCI image build path.
- [ ] Wire hypervisor local container runtime command path.
- [ ] Validate env, volume mounts, and ports for live/dev roles.
- [ ] Capture one successful end-to-end user task.

### Evidence

- [ ] `podman ps` lifecycle output attached.
- [ ] Hypervisor logs for spawn/stop transitions attached.
- [ ] E2E smoke result attached.

### Rollback

- [ ] Document and verify fallback to direct-process sandbox mode.

## Phase 2 Issue: CI + FlakeHub Cache

### Outcome

Flake builds are repeatable and accelerated by managed binary cache.

### Gate

- [ ] CI builds `sandbox`, `desktop`, and `hypervisor` flakes successfully.
- [ ] Repeat build shows improved duration with cache hits.

### Tasks

- [ ] Enable FlakeHub cache in workflow with OIDC permissions.
- [ ] Keep workflow non-blocking initially; then promote to required checks.
- [ ] Capture baseline and post-cache build times.

### Evidence

- [ ] Workflow run links attached.
- [ ] Cache hit/miss summary attached.
- [ ] Build duration comparison attached.

### Rollback

- [ ] Document fallback path if cache integration fails.

## Phase 3 Issue: Hypervisor NixOS Module

### Outcome

Host runtime is encoded declaratively and builds reproducibly.

### Gate

- [ ] NixOS system closure builds for target host config.
- [ ] Hypervisor service starts reliably under systemd.

### Tasks

- [ ] Implement `nixosModules.default` in `hypervisor/flake.nix`.
- [ ] Define hypervisor systemd service.
- [ ] Define container runtime/service integration for sandbox lifecycle.
- [ ] Externalize env and secrets file references.

### Evidence

- [ ] `nix build` toplevel result attached.
- [ ] Service status/log output attached.

### Rollback

- [ ] Document previous generation fallback path.

## Phase 4 Issue: EC2 NixOS Provisioning

### Outcome

AWS host is provisioned and converged from NixOS flake config.

### Gate

- [ ] EC2 host converges from declarative config.
- [ ] Hypervisor remains healthy across reboot.

### Tasks

- [ ] Launch initial target host (`t3a.large`, `us-east-1`, `gp3` volume).
- [ ] Configure network policy, DNS, and TLS routing.
- [ ] Deploy NixOS host config via CLI.

### Evidence

- [ ] Deployment command transcript attached.
- [ ] Post-reboot health output attached.

### Rollback

- [ ] Verify generation rollback and service recovery.

## Phase 5 Issue: Production Validation

### Outcome

MVP traffic path is usable and recoverable in production.

### Gate

- [ ] Auth/register/login/recovery flow passes.
- [ ] live/dev sandbox lifecycle works and route swap succeeds.
- [ ] Rollback rehearsal passes.

### Tasks

- [ ] Run smoke suite against deployed host.
- [ ] Validate logs, restart behavior, and disk/memory headroom.
- [ ] Record first operational playbook.

### Evidence

- [ ] Smoke suite results attached.
- [ ] Runtime metrics snapshot attached.
- [ ] Rollback rehearsal output attached.

### Rollback

- [ ] Confirm rollback from current prod generation to previous known-good.

## Execution Rules

1. One active phase at a time.
2. No gate bypasses.
3. Evidence required for close.
4. If gate fails, open defect issue and stop forward progression.
