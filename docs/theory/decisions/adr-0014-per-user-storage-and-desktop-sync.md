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

**Storage: virtiofs + btrfs host** — the Replit-proven pattern. Per-user btrfs subvolumes
on the host, shared into VMs via virtiofs. Native performance, instant snapshots/forks,
incremental migration via `btrfs send/receive`.

**Lifecycle API:** Minimal 80/20 fleet API — create, start, stop, snapshot, restore,
delete, get, list. Storage provisioning is a create-time operation, snapshot/restore
operates on both VM state and btrfs subvolume atomically.

**Desktop sync: Mutagen (phase 1) → metadata index + placeholder files (phase 2)**.
Mutagen (used by Docker Desktop) solves VM↔host bidirectional sync. Later, SQLite
metadata index for smart/lazy sync with OS-native placeholder files.

**SQLite FUSE rejected** for primary storage (single-writer bottleneck, FUSE overhead).
Retained for sync metadata index.

## What Changed

- Merged ADR-0010 (fleet lifecycle API + capacity) into this ADR
- Storage and compute lifecycle are one system, not two decisions
- Evaluated 7 storage approaches with performance data
- Evaluated 6 sync approaches including cr-sqlite, Mutagen, Syncthing
- Identified Replit as closest architectural analogue

## What To Do Next

1. Implement lifecycle API endpoints on hypervisor (create/start/stop/snapshot/restore)
2. Format OVH host data partition as btrfs
3. Create per-user subvolumes on VM create (`/data/users/{user_id}/`)
4. Update virtiofsd to share per-user dirs instead of shared `/opt/choiros/data/sandbox`
5. Wire btrfs snapshot into snapshot/restore lifecycle operations
6. Add per-VM telemetry (CPU, RSS, snapshot latency, disk throughput)
7. (Later) Evaluate Mutagen for desktop sync prototype

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

### Why Virtiofs + Btrfs Host

1. **Already in use.** ChoirOS uses virtiofs for `/nix/store` and sandbox data sharing. Incremental change.
2. **Replit-proven.** "All Repls use btrfs as their filesystem of choice." Chosen for quotas + CoW snapshots.
3. **Native performance.** cargo/git/rustc work at full speed. No FUSE daemon, no SQLite bottleneck.
4. **Instant snapshots.** `btrfs subvolume snapshot` — metadata-only, <1s regardless of size.
5. **Instant forks.** Same mechanism, writable snapshot = fork.
6. **Incremental migration.** `btrfs send -p parent new | ssh node-b btrfs receive /data/users/` — only changed blocks.
7. **Per-user quotas.** `btrfs qgroup limit 10G /data/users/alice` — kernel-enforced.
8. **Crash safety.** btrfs CoW + metadata checksumming. VM kill = last consistent state preserved.

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
Host (btrfs):
  /data/users/{user_id}/          btrfs subvolume per user
  /data/snapshots/{user_id}/      btrfs snapshots for rollback
  /data/shared/nix-store/         shared read-only (existing)

Per-user virtiofsd:
  shares /data/users/{user_id}/ into VM as /workspace

Guest VM:
  /workspace     (virtiofs mount, user's persistent data)
  /nix/store     (virtiofs mount, shared read-only, existing)
  /tmp           (tmpfs, ephemeral scratch)
```

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

### Gate 2: VM sees per-user storage

```bash
# T5: virtiofsd serves per-user dir (not shared /opt/choiros/data/sandbox)
# From inside VM:
mount | grep virtiofs | grep -q "/workspace" && echo PASS
touch /workspace/canary-$$ && echo PASS

# T6: file persists on host
# From host:
test -f /data/users/{user_id}/canary-$$ && echo PASS
```

### Gate 3: Persistence across VM restart (the P0 fatal bug)

```bash
# T7: write → stop → start → read
# From inside VM:
echo "persist-test-$$" > /workspace/persist-test.txt

# Stop VM, start VM (via lifecycle API or ovh-runtime-ctl)

# From inside VM after restart:
test "$(cat /workspace/persist-test.txt)" = "persist-test-$$" && echo PASS
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
# T16: fio random read/write IOPS on virtiofs+btrfs (inside VM)
fio --name=randwrite --ioengine=libaio --rw=randwrite --bs=4k \
    --numjobs=4 --size=256M --runtime=30 --directory=/workspace
# Record IOPS, compare to direct btrfs baseline

# T17: cargo build inside VM
cd /workspace/choiros-rs && time cargo build 2>&1
# Record wall time, compare to host build time

# T18: git operations inside VM
cd /workspace/choiros-rs && time git status && time git log --oneline -100
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
