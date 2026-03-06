# Implementing ADR-0014: Per-User VM Lifecycle and Storage

Date: 2026-03-06
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0014]

## What This Guide Is

Sequenced implementation steps for ADR-0014. The ADR defines *what* and *why*.
This guide defines *how*, *where*, and *in what order*. Each phase maps to a
test gate in the ADR — implement until the gate passes, then move to the next.

## Prerequisites

- SSH access to Node A (`ssh -i ~/.ssh/id_ed25519_ovh root@51.81.93.94`)
- Current state: single shared sandbox VM, shared `/opt/choiros/data/sandbox`
- Existing code: `hypervisor/src/sandbox/mod.rs` (SandboxRegistry, idle watchdog)

## Phase 1: Host btrfs storage (Gate 1)

**Goal:** `/data` exists as btrfs, subvolumes can be created/snapshotted.

**On Node A:**
```bash
# Check current disk layout
lsblk -f
# Identify unused partition or create one
# Format as btrfs (DESTRUCTIVE — confirm correct device)
mkfs.btrfs /dev/sdX
mkdir -p /data
mount /dev/sdX /data
# Add to /etc/fstab for persistence

# Create directory structure
mkdir -p /data/users /data/snapshots
```

**Run Gate 1 tests from ADR-0014.** All 4 must pass before proceeding.

**NixOS integration:** Add the mount to `nix/hosts/ovh-node-a.nix`:
```nix
fileSystems."/data" = {
  device = "/dev/disk/by-uuid/...";
  fsType = "btrfs";
};
```

## Phase 2: Per-user virtiofsd (Gate 2)

**Goal:** Each VM gets its own virtiofsd sharing its own subvolume.

**Files to modify:**
- `scripts/ops/ovh-runtime-ctl.sh` — currently starts one virtiofsd for
  `/opt/choiros/data/sandbox`. Change to start per-user virtiofsd for
  `/data/users/{user_id}/`.
- `nix/ch/sandbox-vm.nix` — update virtiofs mount from shared sandbox dir
  to per-user `/workspace`.

**Key change in `ovh-runtime-ctl.sh`:**
```bash
# Before: single shared virtiofsd
virtiofsd --socket-path=/tmp/virtiofs-sandbox.sock \
  --shared-dir=/opt/choiros/data/sandbox

# After: per-user virtiofsd
virtiofsd --socket-path=/tmp/virtiofs-${USER_ID}.sock \
  --shared-dir=/data/users/${USER_ID}
```

**On VM create**, create the btrfs subvolume first:
```bash
btrfs subvolume create /data/users/${USER_ID}
```

**Run Gate 2 tests.** File written in VM must be visible on host at
`/data/users/{user_id}/`.

## Phase 3: Persistence across restart (Gate 3)

**Goal:** Stop VM, start VM, data is still there.

This should work automatically once Phase 2 is correct — the subvolume
persists on the host, virtiofsd re-shares it on next start. The fix is
that storage lives on the host filesystem, not inside the VM.

**What to verify:**
1. `ovh-runtime-ctl.sh stop` kills cloud-hypervisor but NOT virtiofsd's
   shared dir (the btrfs subvolume persists regardless)
2. `ovh-runtime-ctl.sh ensure` re-creates virtiofsd + cloud-hypervisor
   pointing at the same subvolume
3. Inside VM, `/workspace` mounts the same data

**Run Gate 3 tests.** Write → stop → start → read must succeed.

If this gate passes, the P0 fatal bug is fixed.

## Phase 4: Lifecycle API (Gate 4)

**Goal:** Hypervisor exposes REST endpoints for VM lifecycle.

**Files to modify:**
- `hypervisor/src/sandbox/mod.rs` — `SandboxRegistry` currently has
  `ensure_running()` and `stop()`. Extend with:
  - `create(owner_id, flavor)` — creates btrfs subvolume + VM config
  - `start(vm_id)` — starts virtiofsd + cloud-hypervisor
  - `stop(vm_id)` — stops cloud-hypervisor (subvolume persists)
  - `snapshot(vm_id)` — btrfs snapshot + optional VM state save
  - `restore(vm_id)` — restore from snapshot
  - `delete(vm_id)` — cleanup subvolume + snapshot + VM config
- `hypervisor/src/api/mod.rs` — add REST routes:
  - `POST /v1/vms` → `create`
  - `POST /v1/vms/:id/start` → `start`
  - `POST /v1/vms/:id/stop` → `stop`
  - `POST /v1/vms/:id/snapshot` → `snapshot`
  - `POST /v1/vms/:id/restore` → `restore`
  - `DELETE /v1/vms/:id` → `delete`
  - `GET /v1/vms/:id` → status
  - `GET /v1/vms?owner_id=...` → list
- `hypervisor/migrations/` — new migration for VM state (or extend
  existing `0002_runtime_registry.sql` schema)

**State machine:** Encode the lifecycle state machine from ADR-0014 in
the registry. Invalid transitions return 409 Conflict.

**Run Gate 4 tests.** All API operations must work end-to-end.

## Phase 5: Idle watchdog fix

**Goal:** Watchdog snapshots VM instead of destroying it.

**File:** `hypervisor/src/sandbox/mod.rs` (idle watchdog section)

**Current behavior:** Watchdog calls `stop()` after idle timeout → VM
state lost.

**New behavior:** Watchdog calls `snapshot()` → VM parked with data
preserved. Next request calls `restore()` → VM resumes.

**Also fix `last_activity`:** Currently only updated on proxy requests.
Must also update on WebSocket frames and health checks.

## Phase 6: Cross-node migration (Gate 5)

**Goal:** Move a user's data from Node A to Node B.

```bash
btrfs send /data/snapshots/{user_id} | ssh node-b btrfs receive /data/users/
```

This is an operator-level operation for now, not API-exposed.

## Order of Operations

```
Phase 1 (host btrfs)     ← can do right now, no code changes
Phase 2 (per-user virtiofsd)  ← scripts + nix config
Phase 3 (persistence)    ← should pass automatically after Phase 2
Phase 4 (lifecycle API)  ← Rust code in hypervisor
Phase 5 (watchdog fix)   ← Rust code in hypervisor
Phase 6 (migration)      ← operator tooling
```

Phases 1-3 are the critical path to fixing the P0 bug. Phase 4-5 are
the API layer. Phase 6 is operational tooling.

## What NOT to Do

- Don't build desktop sync yet (Mutagen is Phase 2 of the ADR, not this guide)
- Don't build multi-node placement or autoscaling
- Don't add quota enforcement until multitenancy requires it
- Don't optimize — get the gates passing first, then benchmark (Gate 6)
