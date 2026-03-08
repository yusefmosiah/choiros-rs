# Implementing ADR-0014: Per-User VM Lifecycle and Storage

Date: 2026-03-06
Kind: Guide
Status: Active
Priority: 2
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

## Phase 2: Per-user virtio-blk on btrfs (Gate 2)

**Goal:** Each user's `data.img` lives on a per-user btrfs subvolume.

**Key insight:** virtiofs CANNOT survive VM snapshot/restore (cloud-hypervisor
issue #6931 — FUSE file handles not captured in snapshots). All mutable data
must be on virtio-blk, which IS captured atomically by snapshots.

**Files to modify:**
- `scripts/ops/ovh-runtime-ctl.sh` — create per-user btrfs subvolume,
  symlink `data.img` from btrfs to VM state dir.
- `nix/ch/sandbox-vm.nix` — remove virtiofs `/workspace` mount, add
  `CHOIR_WORKSPACE_DIR` pointing within virtio-blk mount.

**Key change in `ovh-runtime-ctl.sh`:**
```bash
# Create per-user btrfs subvolume
btrfs subvolume create /data/users/${USER_ID}

# Symlink data.img into VM state dir (microvm expects it there)
ln -sf /data/users/${USER_ID}/data.img ${VM_DIR}/data.img
```

**NO per-user virtiofsd needed.** The existing virtiofsd handles only
read-only shares (nix-store, credentials). User data goes through
virtio-blk which is already configured in sandbox-vm.nix.

**Run Gate 2 tests.** data.img must live on btrfs subvolume.

## Phase 3: Persistence across restart (Gate 3)

**Goal:** Stop VM, start VM, data is still there. Hibernate, restore, data
is still there.

This works automatically because:
1. virtio-blk `data.img` persists on disk (it's just a file on btrfs)
2. `ovh-runtime-ctl.sh stop` kills cloud-hypervisor but data.img persists
3. `ovh-runtime-ctl.sh ensure` boots a new VM using the same data.img
4. Hibernate/restore also works: cloud-hypervisor snapshots capture
   virtio-blk state, and `vm.restore` resumes with data intact

**Run Gate 3 tests.** Write → stop → start → read AND write → hibernate →
restore → read must both succeed.

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
Phase 1 (host btrfs)              ← DONE on Node B
Phase 2 (per-user virtio-blk)     ← scripts + nix config (symlink data.img to btrfs)
Phase 3 (persistence)             ← should pass automatically after Phase 2
Phase 4 (lifecycle API)           ← Rust code in hypervisor
Phase 5 (watchdog → hibernate)    ← DONE (hibernate + heartbeat)
Phase 6 (migration)               ← operator tooling
```

Phases 1-3 are the critical path to per-user isolation. Phase 4 is the
API layer. Phase 5 is done. Phase 6 is operational tooling.

## What NOT to Do

- Don't build desktop sync yet (Mutagen is Phase 2 of the ADR, not this guide)
- Don't build multi-node placement or autoscaling
- Don't add quota enforcement until multitenancy requires it
- Don't optimize — get the gates passing first, then benchmark (Gate 6)
