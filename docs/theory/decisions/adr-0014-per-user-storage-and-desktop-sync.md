# ADR-0014: Per-User VM Lifecycle, Storage, and Desktop Sync

Date: 2026-03-06
Kind: Decision
Status: Draft
Priority: 2
Requires: [ADR-0007, ADR-0012]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

ChoirOS needs per-user persistent, isolated, snapshotable, forkable VMs with storage
that can eventually sync to desktop clients. This ADR covers the full per-user VM
lifecycle: how VMs are created, sized, started, stopped, snapshotted, restored, and
how their storage is provisioned.

**Storage: virtio-blk + btrfs host** — per-user virtio-blk volumes (data.img files)
stored on host btrfs subvolumes. virtio-blk survives VM snapshot/restore (cloud-hypervisor
captures block device state atomically). btrfs provides instant CoW snapshots, forks,
and incremental migration via `btrfs send/receive` on the host side.

**Why NOT virtiofs for mutable data:** virtiofs uses FUSE file handles that are NOT
captured in cloud-hypervisor VM snapshots (issue #6931). After restore, the guest kernel
tries to resume FUSE operations on stale handles → I/O errors and data loss. virtiofs is
correct for read-only shares (nix-store, credentials) but WRONG for mutable user data.

**Lifecycle API:** Minimal 80/20 fleet API — create, start, stop, snapshot, restore,
delete, get, list. Storage provisioning is a create-time operation, snapshot/restore
operates on both VM state and virtio-blk volume atomically.

**Desktop sync: Mutagen (phase 1) → metadata index + placeholder files (phase 2)**.
Mutagen (used by Docker Desktop) solves VM↔host bidirectional sync. Later, SQLite
metadata index for smart/lazy sync with OS-native placeholder files.

**SQLite FUSE rejected** for primary storage (single-writer bottleneck, FUSE overhead).
Retained for sync metadata index.

## What Changed

- **2026-03-08: CRITICAL CORRECTION** — virtiofs cannot survive VM snapshot/restore.
  Changed from "virtiofs + btrfs host" to "virtio-blk + btrfs host" for all mutable
  data. virtiofs retained ONLY for read-only shares (nix-store, credentials).
  See "Why NOT virtiofs for Mutable Data" section below.
- Merged ADR-0010 (fleet lifecycle API + capacity) into this ADR
- Storage and compute lifecycle are one system, not two decisions
- Evaluated 7 storage approaches with performance data
- Evaluated 6 sync approaches including cr-sqlite, Mutagen, Syncthing
- Identified Replit as closest architectural analogue
- Identified Fly.io (NVMe thin LVM → virtio-blk) as the correct pattern for
  persistent data that must survive VM snapshots

## What To Do Next

1. Format OVH host data partition as btrfs (DONE on Node B)
2. Create per-user btrfs subvolumes on VM create (`/data/users/{user_id}/`)
3. Store per-user `data.img` on btrfs subvolume, symlink into VM state dir
4. Separate sandbox data layout: `/opt/choiros/data/sandbox/runtime/` vs `workspace/`
5. Wire btrfs snapshot into hibernate/restore lifecycle operations
6. Implement lifecycle API endpoints on hypervisor (create/start/stop/snapshot/restore)
7. Add per-VM telemetry (CPU, RSS, snapshot latency, disk throughput)
8. (Later) Evaluate Mutagen for desktop sync prototype

---

## Storage Decision

### The Seven Approaches Evaluated

| Approach | Guest Perf | Snapshot | Fork | Migrate | Desktop Sync | Complexity |
|----------|-----------|----------|------|---------|-------------|-----------|
| **1. FUSE+SQLite** | Poor (1-4x latency, single-writer) | cp .db (2-30s) | cp .db (2-30s) | scp full file | Best (row-level delta) | Medium |
| **2. OverlayFS** | Native reads, copy-up on write | Depends on host fs | Depends on host fs | rsync | Same as host fs | Low |
| **3. Btrfs subvolumes** | Native | Instant (<1s) | Instant (<1s) | Incremental (send/receive) | Needs external | Low |
| **4. ZFS datasets** | Native | Instant | Instant | Incremental | Needs external | Medium (licensing, RAM) |
| **5. qcow2 virtio-blk** | Best (near-native) | Instant (overlay chain) | Instant | Full image copy | Hard | High |
| **6. Virtiofs + btrfs** | Near-native | Instant | Instant | Incremental | Needs external | **Lowest** |
| **7. 9p** | 2-8x slower than virtiofs | N/A | N/A | N/A | N/A | Don't use |

### Why Virtio-blk + Btrfs Host

1. **Snapshot-safe.** cloud-hypervisor VM snapshots capture virtio-blk state atomically.
   After `vm.restore`, the block device is exactly as it was — no stale handles, no I/O errors.
2. **Already in use.** ChoirOS uses virtio-blk `data.img` for sandbox mutable state since ADR-0016.
3. **Industry-proven.** Fly.io uses NVMe thin LVM → virtio-blk for persistent volumes.
   Replit uses btrfs + NBD (network block device) to GCS. Both are block-device-first.
4. **Btrfs host backing.** Per-user `data.img` files live on host btrfs subvolumes →
   instant CoW snapshots, forks, and incremental migration via `btrfs send/receive`.
5. **Near-native performance.** virtio-blk is the fastest guest I/O path. No FUSE daemon overhead.
6. **Per-user quotas.** `btrfs qgroup limit 10G /data/users/alice` — kernel-enforced on host.
7. **Crash safety.** btrfs CoW + ext4 journal inside data.img = double safety net.

### Why NOT Virtiofs for Mutable Data

virtiofs was the original choice (this ADR, pre-correction). It fails for one critical reason:

**VM snapshot/restore breaks virtiofs.** cloud-hypervisor `vm.snapshot` saves CPU state,
memory, and virtio-blk device state — but NOT virtiofs FUSE file handle state. After
`vm.restore`, the guest kernel resumes with stale FUSE file descriptors. Any in-flight
I/O or cached file handles produce errors. This is cloud-hypervisor issue #6931.

virtiofs remains correct for:
- `/nix/store` (read-only, no state to preserve)
- `/run/choiros/credentials/sandbox` (read-only secrets, re-read on service start)

**UPDATE (ADR-0018):** virtiofs for read-only shares is also being removed.
`shared=on` (required by virtiofs) blocks KSM page deduplication, and
virtiofsd costs 176 MB/VM. ADR-0018 replaces nix-store virtiofs with a
shared read-only squashfs virtio-blk image and drops the creds share
entirely (gateway token already injected via env var).

virtiofs is WRONG for:
- User workspace data (must survive hibernate/restore)
- Runtime state like SQLite DBs (must survive hibernate/restore)
- Anything that the sandbox writes to and expects to persist across VM lifecycle

### Why Not FUSE+SQLite for Primary Storage

The intuition was right — one SQLite file per user is elegantly portable and sync-friendly.
But the performance costs are disqualifying for a dev workspace:

- **Single-writer bottleneck.** SQLite WAL mode allows one writer at a time. `cargo build`
  spawns many `rustc` processes writing `.rlib`/`.rmeta` simultaneously — they'd serialize.
- **FUSE overhead.** 0-83% degradation depending on workload. Metadata-intensive operations
  (stat, readdir, open — exactly what build tools do) suffer most.
- **mmap edge cases.** FUSE mmap works on Linux but has incomplete inotify support. Build
  tools and editors rely on both.

FUSE-over-io_uring (Linux 6.14, March 2025) narrows the gap (~50% latency reduction) but
doesn't close it for metadata-heavy workloads.

**However**: SQLite is the right choice for a **metadata/sync index** alongside the real
filesystem. This is the hybrid the research recommends for desktop sync.

### Why Not ZFS

Technically excellent, but:
- CDDL vs GPL licensing friction (can't ship in mainline kernel)
- Memory-hungry ARC cache (defaults to ~60% of RAM)
- NixOS supports it but adds build/maintenance burden
- btrfs is kernel-native and sufficient for this use case

### Why Not qcow2

Best raw guest performance via virtio-blk, but:
- Management complexity (backing file chains, garbage collection, flattening)
- No incremental migration (must copy full images)
- Can't inspect/modify user files from host without loopback mounting
- Snapshot chain depth degrades read performance

### Architecture

```
Host (btrfs at /data):
  /data/users/{user_id}/              btrfs subvolume per user
  /data/users/{user_id}/data.img      virtio-blk volume (ext4, 2-10 GB)
  /data/snapshots/{user_id}/          btrfs snapshots (captures data.img atomically)

VM state dir (/opt/choiros/vms/state/{vm_name}/):
  data.img -> /data/users/{user_id}/data.img   (symlink to btrfs)
  vm.pid, vm.log, *.sock                       (ephemeral process state)

Guest VM:
  /opt/choiros/data/sandbox/          (virtio-blk mount — ALL mutable data)
    ├── runtime/                      events.db, conductor runs, writer state
    └── workspace/                    user files, projects
  /nix/store                          (virtiofs mount, shared read-only)
  /run/choiros/credentials/sandbox    (virtiofs mount, secrets, read-only)
  /tmp                                (tmpfs, ephemeral scratch)
```

**Data flow:** User writes go to `/opt/choiros/data/sandbox/workspace/` inside the VM,
which is on the virtio-blk volume. The backing `data.img` file lives on a per-user btrfs
subvolume on the host. `btrfs subvolume snapshot` captures the entire data.img atomically
(CoW, metadata-only, <1s). VM `hibernate` saves VM state + btrfs snapshot = complete
point-in-time capture. VM `restore` resumes from snapshot with all data intact.

### Btrfs Gotchas to Watch

- **CoW fragmentation** on long-lived random-write files (SQLite DBs, VM images). Mitigate
  with `chattr +C` (nodatacow) on specific directories.
- **reflink copy** can be slow on highly fragmented files. Defragment first if needed.
- **Quotas** (qgroups) add overhead. Enable only when multitenancy requires enforcement.

---

## VM Lifecycle API (merged from ADR-0010)

### 80/20 Lifecycle Endpoints

```
POST   /v1/vms                    create (provisions btrfs subvolume + VM)
POST   /v1/vms/{vm_id}/start
POST   /v1/vms/{vm_id}/stop
POST   /v1/vms/{vm_id}/snapshot   (VM state + btrfs snapshot, atomic)
POST   /v1/vms/{vm_id}/restore    (from snapshot)
DELETE /v1/vms/{vm_id}
GET    /v1/vms/{vm_id}
GET    /v1/vms?owner_id=...
```

Required rails: idempotency key on mutating requests, strict state-machine
validation, quota checks on create/start/restore, lifecycle events for every
state transition.

### Lifecycle State Machine

```
creating -> stopped -> running -> stopping -> stopped
                                    |
running -> pausing -> paused -> snapshotting -> snapshotted -> restoring -> running
                                    |
                                 deleted / failed
```

No live migration, autoscaling, or multi-node placement in bootstrap scope.

### Bootstrap Sizing

Default session: `2 vCPU / 3 GiB RAM`. Idle timeout + snapshot park for cost control.

| Profile | Usable RAM | SLO-safe Active | Stretch | Parked Snapshots |
|---------|-----------|----------------|---------|-----------------|
| KS-2 (Xeon D-1540, 64 GB) | 52 GiB | 11 | 14-16 | 63-95 |
| EPYC 7351P (256 GB) | 224 GiB | 22 | 28-32 | 71-107 |

Assumptions: CPU overcommit 2.0, RAM overcommit 1.0, 20% host reserve,
4-6 GiB snapshot footprint per parked session.

### Validation Required Before "Accepted"

1. Per-VM metrics: CPU, RSS, snapshot create/restore latency, disk throughput
2. Controlled load tests on both profiles (interactive, burst, park/restore)
3. 7-day canary on one OVH node with observed SLO data

---

## Desktop Sync Decision

### Future desideratum: Dropbox-like desktop mount

Users should be able to mount their cloud workspace locally, with bidirectional sync,
conflict resolution, and optionally placeholder/on-demand files.

### The Sync Approaches Evaluated

| Approach | Bidirectional | Conflict Resolution | Placeholder Files | Maturity |
|----------|--------------|--------------------|--------------------|---------|
| **Mutagen** | Yes (three-way merge) | Multiple modes (safe, alpha-wins) | No | Production (Docker Desktop) |
| **Syncthing** | Yes | File-level rename | No | Production |
| **cr-sqlite** | Yes (CRDT) | Column-level LWW | Natural fit | **Not production-ready** |
| **Nextcloud client** | Yes (csync) | File-level rename | Yes (macOS, Windows) | Production |
| **Litestream** | One-way only | N/A | N/A | Production (backup only) |
| **LiteFS** | Single-writer | N/A | N/A | Production (server clusters) |

### Recommended Phased Approach

#### Phase 1: Mutagen for VM↔Desktop sync

Mutagen was literally built for the "cloud VM filesystem ↔ local desktop" problem. Docker
acquired it for this reason. It provides:

- Three-way merge (remote tree, local tree, synced ancestor — same model as Dropbox)
- rsync-efficient delta transfer
- Filesystem watching (inotify/FSEvents/ReadDirectoryChanges)
- Multiple conflict modes
- Ignore patterns (`.git`, `target/`, `node_modules/`)

Run Mutagen between the host's per-user btrfs subvolume and the desktop client folder.
No custom sync code needed.

#### Phase 2: SQLite metadata index + smart sync

Add a SQLite metadata index alongside the filesystem (maintained by fanotify/inotify):

```sql
CREATE TABLE files (
    inode INTEGER PRIMARY KEY,
    parent_inode INTEGER NOT NULL,
    name TEXT NOT NULL,
    content_hash BLOB,  -- SHA-256
    size INTEGER,
    mtime INTEGER,
    mode INTEGER,
    UNIQUE(parent_inode, name)
);
```

This enables:
- Sync metadata eagerly, content lazily (desktop has full tree, fetches files on access)
- Content-addressed blob transfer (deduplication, efficient deltas)
- Fast tree comparison for sync decisions

#### Phase 3: OS-native placeholder files

- **macOS:** File Provider framework (what iCloud/Nextcloud use)
- **Windows:** Cloud Files API via `wincs` Rust crate
- **Linux:** FUSE mount with lazy hydration

Placeholders mean the desktop always has a complete directory listing but only downloads
file bodies when opened. Large workspaces (10GB+) become practical to sync.

#### Phase 4: (Optional) cr-sqlite for multi-device CRDT sync

If peer-to-peer sync without cloud authority is needed (e.g., laptop ↔ tablet direct),
adopt cr-sqlite for the metadata index. Not needed while cloud VM is the authority.

### Why Not Build a Custom Sync Engine

Dropbox spent years rewriting their sync engine in Rust. The three-tree merge problem is
deceptively complex. Mutagen already solved it for dev environments. Use it.

### Why Not Pure SQLite + cr-sqlite for Everything

cr-sqlite is explicitly **not production-ready** (vlcn.io acknowledges this). It handles
metadata sync well but doesn't solve binary blob sync. And the FUSE+SQLite primary
filesystem has disqualifying performance for build workloads (see storage section).

The hybrid is correct: real filesystem for performance, SQLite metadata index for sync.

---

## Production References

| Platform | Storage | Sync | Notes |
|----------|---------|------|-------|
| **Replit** | btrfs subvolumes + Margarine NBD to GCS | SSH/SSHFS | Closest analogue to ChoirOS |
| **GitHub Codespaces** | Persistent Docker volumes | VS Code Remote (SSH) | No local sync |
| **Gitpod Flex** | Per-user VM + container | SSH | S3 backup on stop |
| **Docker Desktop** | ext4 in VM | **Mutagen** | Exactly our use case |
| **Fly.io** | NVMe volume slices | N/A | One-VM-per-app model |
| **Dropbox** | Custom (Rust sync engine) | Three-tree merge | Gold standard, don't rebuild |

---

## Verification (TDD — write these tests before implementation)

### Gate 1: Host storage (run on Node A)

```bash
# T1: btrfs partition exists
test "$(stat -f -c %T /data)" = "btrfs" && echo PASS

# T2: can create per-user subvolume
btrfs subvolume create /data/users/test-user-$$
test -d /data/users/test-user-$$ && echo PASS

# T3: can snapshot in <1s
time btrfs subvolume snapshot /data/users/test-user-$$ /data/snapshots/test-snap-$$
# assert wall time < 1s

# T4: can delete subvolume + snapshot
btrfs subvolume delete /data/snapshots/test-snap-$$
btrfs subvolume delete /data/users/test-user-$$
```

### Gate 2: Per-user virtio-blk on btrfs

```bash
# T5: data.img lives on per-user btrfs subvolume
test -f /data/users/{user_id}/data.img && echo PASS
# Verify it's a btrfs subvolume
btrfs subvolume show /data/users/{user_id}/ && echo PASS

# T6: VM state dir symlinks to btrfs-backed data.img
readlink /opt/choiros/vms/state/{vm_name}/data.img | grep -q "/data/users/" && echo PASS

# T7: write inside VM persists to virtio-blk
# From inside VM:
echo "canary-$$" > /opt/choiros/data/sandbox/workspace/test.txt && echo PASS
```

### Gate 3: Persistence across VM restart (the P0 fatal bug)

```bash
# T8: write → stop → start → read
# From inside VM:
echo "persist-test-$$" > /opt/choiros/data/sandbox/workspace/persist-test.txt

# Stop VM, start VM (via lifecycle API or ovh-runtime-ctl)

# From inside VM after restart:
test "$(cat /opt/choiros/data/sandbox/workspace/persist-test.txt)" = "persist-test-$$" && echo PASS

# T9: write → hibernate → restore → read (snapshot/restore path)
echo "hibernate-test-$$" > /opt/choiros/data/sandbox/workspace/hibernate-test.txt
# Hibernate VM (ovh-runtime-ctl hibernate), then ensure (restore)
test "$(cat /opt/choiros/data/sandbox/workspace/hibernate-test.txt)" = "hibernate-test-$$" && echo PASS
```

### Gate 4: Lifecycle API

```bash
# T8: create returns VM with storage
curl -sf -X POST http://localhost:9090/v1/vms \
  -d '{"owner_id":"test","flavor":"2vcpu-3gib"}' | jq -e '.vm_id' && echo PASS

# T9: start → running
VM_ID=$(curl -sf ... | jq -r .vm_id)
curl -sf -X POST http://localhost:9090/v1/vms/$VM_ID/start
curl -sf http://localhost:9090/v1/vms/$VM_ID | jq -e '.status == "running"' && echo PASS

# T10: snapshot → snapshotted (includes btrfs snapshot)
curl -sf -X POST http://localhost:9090/v1/vms/$VM_ID/snapshot
curl -sf http://localhost:9090/v1/vms/$VM_ID | jq -e '.status == "snapshotted"' && echo PASS
# Verify btrfs snapshot exists on host:
test -d /data/snapshots/$VM_ID && echo PASS

# T11: restore → running with data intact
curl -sf -X POST http://localhost:9090/v1/vms/$VM_ID/restore
# Verify data written before snapshot is present

# T12: delete cleans up subvolume + snapshot
curl -sf -X DELETE http://localhost:9090/v1/vms/$VM_ID
test ! -d /data/users/$VM_ID && echo PASS
test ! -d /data/snapshots/$VM_ID && echo PASS

# T13: idempotency — duplicate create with same key returns same VM
# T14: quota — create beyond limit returns 429
```

### Gate 5: Cross-node migration

```bash
# T15: btrfs send/receive to Node B
btrfs send /data/snapshots/$VM_ID | ssh node-b btrfs receive /data/users/
ssh node-b test -d /data/users/$VM_ID && echo PASS
```

### Gate 6: Performance baselines (P7, non-blocking)

```bash
# T16: fio random read/write IOPS on virtio-blk+ext4 (inside VM)
fio --name=randwrite --ioengine=libaio --rw=randwrite --bs=4k \
    --numjobs=4 --size=256M --runtime=30 --directory=/opt/choiros/data/sandbox/workspace
# Record IOPS, compare to direct host baseline

# T17: cargo build inside VM
cd /opt/choiros/data/sandbox/workspace/choiros-rs && time cargo build 2>&1
# Record wall time, compare to host build time

# T18: git operations inside VM
cd /opt/choiros/data/sandbox/workspace/choiros-rs && time git status && time git log --oneline -100
# Record latency
```

### Future gates (not blocking promotion to Accepted)

- Mutagen bidirectional sync (desktop → host → VM)
- SQLite metadata index tracks file tree
- Placeholder files on at least one OS

---

## Sources

### Storage
- [Replit: Storage The Next Generation](https://blog.replit.com/replit-storage-the-next-generation)
- [AgentFS FUSE (Turso)](https://turso.tech/blog/agentfs-fuse)
- [FUSE Performance (ACM)](https://dl.acm.org/doi/fullHtml/10.1145/3310148)
- [To FUSE or Not to FUSE (USENIX FAST'17)](https://www.usenix.org/system/files/conference/fast17/fast17-vangoor.pdf)
- [RFUSE (FAST'24)](https://www.usenix.org/system/files/fast24-cho.pdf)
- [FUSE-over-io_uring (Linux 6.14)](https://www.phoronix.com/news/Linux-6.14-FUSE)
- [SQLite Faster Than Filesystem](https://sqlite.org/fasterthanfs.html)
- [virtiofs Design](https://virtio-fs.gitlab.io/design.html)
- [Cloud Hypervisor virtiofs](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/fs.md)
- [btrfs Subvolumes](https://btrfs.readthedocs.io/en/latest/Subvolumes.html)
- [btrfs Send/Receive](https://btrfs.readthedocs.io/en/latest/Send-receive.html)

### Sync
- [Dropbox: Rewriting the Heart of Our Sync Engine](https://dropbox.tech/infrastructure/rewriting-the-heart-of-our-sync-engine)
- [Mutagen File Synchronization](https://mutagen.io/documentation/synchronization/)
- [cr-sqlite (vlcn.io)](https://github.com/vlcn-io/cr-sqlite)
- [LiteFS (Fly.io)](https://fly.io/docs/litefs/)
- [Nextcloud Desktop Architecture](https://docs.nextcloud.com/desktop/3.3/architecture.html)
- [Windows Cloud Files API](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine)
- [wincs Rust crate](https://github.com/ok-nick/wincs)
- [Syncthing Synchronization](https://docs.syncthing.net/users/syncing.html)
