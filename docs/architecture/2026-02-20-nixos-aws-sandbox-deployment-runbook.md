# ChoirOS Runbook: NixOS Sandboxes on NixOS on AWS

Date: 2026-02-20  
Status: Living document (MVP deployment path)  
Owner: ChoirOS runtime and deployment

## Narrative Summary (1-minute read)

This runbook defines the shortest safe path from current local flake-based development to
an MVP deployment on AWS where a NixOS host runs the Hypervisor and manages Sandbox
instances as containers.

The plan intentionally prioritizes delivery speed and cost control over hard sandbox
security. We accept that container isolation is not a complete defense against prompt
injection-driven escape for MVP, and we keep the architecture forward-compatible with
future hardening.

The sequence is: local Podman smoke test, CI + FlakeHub cache, NixOS host module,
single-node EC2 rollout, validation, and rollback playbook.

## What Changed

1. Consolidated current Nix work into a single deployment runbook.
2. Added a gate-based path from local flakes to AWS NixOS production.
3. Added explicit secrets boundary requirements for hypervisor/sandbox split.
4. Added rollback, ops, and cost-control procedures for MVP operation.

## What To Do Next

1. Finish local Podman smoke path (Phase 1 gate).
2. Enable draft Nix CI workflow with FlakeHub cache (Phase 2 gate).
3. Implement `hypervisor/flake.nix` NixOS module + EC2 host target (Phase 3/4 gates).
4. Deploy single EC2 NixOS node, then run end-to-end validation (Phase 5 gate).

## Scope and Constraints

- Goal: deploy MVP to AWS with NixOS host, Hypervisor process, Sandbox container runtime.
- Non-goal: strong sandbox hardening in this phase.
- Cost posture: minimal infra footprint, fast iteration.
- Security posture: platform secrets remain hypervisor-only; user secrets are brokered.

## Bootstrap Success Definition

ChoirOS is considered bootstrapped for this phase when:

1. A single authenticated user can run two sandboxes concurrently for coding work:
   - `live` sandbox: stable route target for normal usage
   - `dev` sandbox: feature iteration target
2. Hypervisor can route/swap between these targets for rapid rollback.
3. Prior known-good image/config can be restored in bounded time.

This is the primary MVP value objective. Security hardening beyond container baseline is
explicitly deferred.

## Initial AWS Target (MVP Cost Band)

Chosen default target (for now):

- Region: `us-east-1`
- Instance family: `t3a.large` (x86_64, 2 vCPU, 8 GiB RAM)
- Storage: `gp3` 80-100 GiB

Why:

1. Fits the current `$40-60/month` compute target range for always-on single-node MVP.
2. 8 GiB memory gives enough headroom for hypervisor plus two low-to-moderate sandbox
   containers for single-user operation.
3. x86_64 aligns with current Linux deployment assumptions and reduces early surprises.

Scale-up trigger:

- Move to larger instance class when either sustained CPU credit pressure, memory pressure,
  or concurrent user demand causes instability.

## Current Starting Point

- Nix installed via Determinate installer.
- Home Manager flakes configured.
- Component flakes drafted and running locally:
  - `sandbox/flake.nix`
  - `dioxus-desktop/flake.nix` (desktop)
  - `hypervisor/flake.nix`
- Draft Nix CI workflow added:
  - `.github/workflows/nix-ci-draft.yml`
- Architecture decisions drafted:
  - `docs/architecture/adr-0002-rust-nix-build-and-cache-strategy.md`
  - `docs/architecture/adr-0003-hypervisor-sandbox-secrets-boundary.md`

## Target Runtime Architecture (MVP)

```
Internet
  -> EC2 NixOS host
       -> Hypervisor (systemd service, port 9090)
            -> Auth/session boundary
            -> User secrets broker
            -> Podman-managed sandbox containers (live/dev)
                 -> Sandbox API (8080/8081 internal mapping)
```

## Phase Plan and Gates

### Phase 1: Local container smoke (Podman)

Objective: verify hypervisor can manage sandbox as container locally using the same
runtime contract planned for AWS.

Tasks:

1. Add container image build for sandbox (OCI image from Nix output or Dockerfile bridge).
2. Run sandbox container with explicit env + volume mount for data.
3. Wire hypervisor spawn path to Podman command for local live/dev lifecycle.
4. Confirm reverse proxy + websocket behavior still works.

Gate:

- Hypervisor can start/stop/restart sandbox container locally.
- Login flow and one end-to-end task succeed through hypervisor proxy.

### Phase 2: CI and binary cache

Objective: make Nix builds repeatable and fast in CI before AWS rollout.

Tasks:

1. Activate FlakeHub cache for GitHub Actions (`id-token: write` OIDC path).
2. Keep draft workflow initially non-blocking; then promote to required checks.
3. Build matrix for `sandbox`, `desktop`, `hypervisor` flake package outputs.
4. Record cache hit metrics and wall-clock build times.

Gate:

- CI successfully builds all three components from flakes.
- Repeat build time improves with cache hits.

### Phase 3: Hypervisor NixOS module

Objective: encode runtime as declarative NixOS config.

Tasks:

1. Add `nixosModules.default` in `hypervisor/flake.nix`.
2. Define systemd unit for hypervisor binary.
3. Define Podman service/unit spec for sandbox container lifecycle contract.
4. Add host firewall and service dependencies.
5. Externalize env vars and secret file references.

Gate:

- `nix build .#nixosConfigurations.<host>.config.system.build.toplevel` succeeds.
- Local/VM test boots and hypervisor starts cleanly.

### Phase 4: AWS NixOS host provisioning

Objective: stand up minimal-cost EC2 NixOS host and deploy via flake.

Tasks:

1. Choose instance class (start small; allow scale-up).
2. Provision EBS volume and security group (9090 + SSH admin ingress policy).
3. Apply NixOS host config using remote deploy method.
4. Configure domain, DNS, and TLS termination path.

Gate:

- Host converges with declarative config.
- Hypervisor service healthy after reboot.

### Phase 5: Production validation

Objective: prove MVP is usable and recoverable.

Tasks:

1. Run auth/register/login/recovery flow checks.
2. Run sandbox live/dev start-stop path.
3. Validate websocket proxy and task execution.
4. Validate logs, basic metrics, and crash restart behavior.
5. Execute rollback rehearsal.

Gate:

- Production smoke suite passes.
- Rollback procedure tested once successfully.

## Secrets and Policy Requirements (MVP)

1. Platform secrets only on hypervisor host scope.
2. No platform secrets in sandbox env, files, logs, event payloads.
3. User-level secrets stored and resolved via hypervisor broker policy.
4. Access audit events include metadata only (never secret values).

## Rollback and Recovery

1. Keep previous generation available (`nixos-rebuild` generation rollback).
2. Keep previous container image/tag pinned and runnable.
3. Maintain DB snapshot backup cadence for auth/user-secret store.
4. If rollout fails: revert generation, restart services, validate login + proxy.

## Operational Baseline

1. Health endpoints for hypervisor and sandbox.
2. Structured logs retained on host with rotation.
3. Basic alerts: service down, restart loop, disk pressure.
4. Weekly check: cache health, image bloat, DB size growth.

## Cost Controls (MVP)

1. Start single-node EC2 + single EBS volume.
2. Use FlakeHub cache plan to reduce CI compute waste.
3. Avoid managed orchestrators until demand requires them.
4. Add autoscaling only after usage data justifies complexity.

## Deferred Until Post-MVP

1. Stronger sandbox isolation (microVM/gVisor/firecracker path).
2. Multi-node orchestration and automatic failover.
3. Advanced secret manager integration (KMS/Vault-first model).
4. Tenant-level policy engine and richer RBAC.

## Acceptance Criteria (End State)

1. Flake-based CI builds for all three components are green and cached.
2. EC2 NixOS host is declared and reproducible from flake config.
3. Hypervisor serves authenticated traffic and proxies sandbox containers.
4. Secrets boundary is enforced per ADR-0003.
5. Deployment and rollback can be performed from documented steps only.

## References

- `docs/architecture/2026-02-17-codesign-runbook.md`
- `docs/architecture/adr-0002-rust-nix-build-and-cache-strategy.md`
- `docs/architecture/adr-0003-hypervisor-sandbox-secrets-boundary.md`
- `.github/workflows/nix-ci-draft.yml`
