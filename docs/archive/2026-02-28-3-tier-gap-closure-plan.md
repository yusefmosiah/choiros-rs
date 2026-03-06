# 3-Tier Gap Closure Plan: Per-User microVMs + Per-Branch Containers (Mac-first)

Date: 2026-02-28
Status: In Progress
Owner: platform/runtime

## Narrative Summary (1-minute read)

Target architecture is:

1. Global control plane (identity, secrets broker, runtime orchestrator, provider gateway).
2. Per-user runtime plane (one user VM each; inside it, branch containers: `main`, `dev`, `branch-*`).
3. Client plane (web/mobile/API).

Current code is still `live/dev` role-based with shared ports and no branch-aware runtime lifecycle.
The fastest path to deployment parity from macOS is `microvm.nix + vfkit` for local user VMs,
then Linux/OVH `cloud-hypervisor` backend under the same control-plane contracts.

## What Changed

1. Confirmed hard gaps between current runtime and target 3-tier architecture.
2. Locked local parity strategy: macOS uses vfkit-hosted NixOS VMs; cloud-hypervisor parity is Linux/OVH.
3. Defined phased implementation plan from contract hardening to branch-container lifecycle.
4. Implemented vfkit-only hypervisor runtime path; removed process-runtime compatibility fallback.
5. Added repo-owned vfkit host/guest control scripts and a NixOS microVM configuration output.
6. Added Playwright vfkit terminal proof spec (video-recorded) for NixOS guest identity validation.

## What To Do Next

1. Add runtime registry schema and branch-aware routing contracts.
2. Split identity and secrets broker out of hypervisor.
3. Implement Mac vfkit user-VM + in-VM container orchestration path.
4. Reuse same contracts for OVH/Linux cloud-hypervisor backend.
5. Remove fixed `live/dev` runtime assumptions from deploy/ops tooling.

## Current Gaps (Code-Verified)

1. Global role ports (`8080/8081`) and listener adoption can collapse user isolation.
2. Role-only DB/runtime state (`sandbox_{role}.db`) is not user/branch isolated.
3. Routing supports only `live`/`dev` semantics; no branch pointers (`main/dev/exp-*`).
4. Runtime backend abstraction is missing (`Process` only).
5. File root is repo-level, not per-user/per-branch root.
6. Deploy/ops scripts assume exactly two fixed containers.
7. Top-level flake does not define runtime `nixosConfigurations` graph for this architecture.
8. Hypervisor still owns auth/session/provider gateway; control-plane split incomplete.

## Target Runtime Contract

- Control plane:
  - Identity service
  - Secrets broker
  - Runtime orchestrator
  - Provider gateway
- Runtime plane:
  - `UserVmRef`
  - `BranchRuntimeRef`
  - `RoutePointer`
  - `RuntimeHealth`
- Routing:
  - Pointers: `main`, `dev`, `exp-*`
  - Branch direct addressing: `branch:<name>`
  - `/dev` retained as temporary compatibility alias

## Implementation Plan

### Phase A: Runtime Contract Hardening

- Add DB tables: `user_vms`, `branch_runtimes`, `route_pointers`, `runtime_events`.
- Add typed runtime registry in control plane.
- Keep process backend only as compatibility adapter.

### Phase B: Public Runtime API

- Add:
  - `POST /runtime/v1/users/{user}/branches/{branch}/ensure`
  - `DELETE /runtime/v1/users/{user}/branches/{branch}`
  - `POST /runtime/v1/users/{user}/pointers/{pointer}/set`
  - `GET /runtime/v1/users/{user}/topology`
- Keep `/dev` mapped to pointer `dev` during migration.

### Phase C: Control-Plane Split

- Extract auth/session to identity service.
- Extract user secret resolution to secrets broker.
- Keep hypervisor as routing + lifecycle + policy + observability only.

### Phase D: macOS Local Parity (vfkit)

- Add repo-managed NixOS configs for:
  - control-plane VM
  - user-VM template
- In each user VM, run NixOS containers per branch (`main`, `dev`, `branch-*`).

### Phase E: Guest Runtime Manager

- Add in-VM guest agent API for branch container CRUD and health.
- Control plane manages guest agents, not direct host shell container commands.

### Phase F: Branch-Per-Container Semantics

- Naming: `sandbox-{branch-slug}`.
- Isolated branch DB/workspace under user VM storage.
- Atomic pointer switching.

### Phase G: OVH/Linux Backend Parity

- Implement Linux backend adapter with `cloud-hypervisor`.
- Reuse same API and runtime contracts from macOS path.

### Phase H: Legacy Path Removal

- Remove role-only assumptions from runtime registry and scripts.
- Replace fixed `sandbox-live/sandbox-dev` ops with dynamic inventory.

## Public API / Type Changes

1. Replace routing authority `SandboxRole` with branch/pointer target type.
2. Expand runtime backend enum to support VM guest backends.
3. Add propagation headers:
   - `x-choiros-vm-id`
   - `x-choiros-branch`
   - `x-choiros-runtime-id`

## Test Cases and Acceptance Criteria

1. Two users resolve to different VM/runtime IDs and isolated state.
2. One user running `main` + `feature-x` has isolated branch runtime state.
3. Pointer swaps are atomic.
4. Cross-user runtime access is denied.
5. Guest container crashes auto-recover without pointer drift.
6. WS/event streams remain properly scoped (`session_id`, `thread_id`, `user_id`, `runtime_id`).
7. Deploy scripts pass with dynamic runtime inventory.
8. Mac vfkit parity suite passes; Linux cloud-hypervisor parity suite passes.

## Assumptions and Defaults

1. macOS parity backend is vfkit, not nested cloud-hypervisor.
2. Full 3-tier scope is in plan (not runtime-only).
3. `/dev` remains temporary compatibility alias.
4. One branch maps to one container inside one user VM.
5. Platform/provider keys remain control-plane-only.

## References

- `docs/architecture/2026-02-26-comprehensive-cutover-plan.md`
- `docs/architecture/roadmap-dependency-tree.md`
- `docs/handoffs/2026-02-22-platform-project-checklist-ovh-microvm.md`
- `docs/runbooks/platform-secrets-sops-nix.md`
- `docs/runbooks/nix-setup.md`
