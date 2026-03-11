# ADR-0014: Per-User VM Lifecycle, Storage, and Desktop Sync

Date: 2026-03-06
Kind: Decision
Status: Draft
Priority: 2
Requires: [ADR-0007, ADR-0012]
Authors: wiz + Claude
Updated: 2026-03-11

## Narrative Summary (1-minute read)

Each ChoirOS user gets two lightweight VMs: dev and prod. Dev is the
interactive workspace where coding agents run. Prod serves the user's deployed
app and only accepts promoted builds that pass verification (tests, docs,
review). Heavy jobs like Playwright E2E tests with Chrome and video recording
run on a shared job queue, not inside user VMs.

This ADR covers the full per-user VM lifecycle, the dev/prod promotion model,
workload-based resource allocation, storage provisioning, and desktop sync.

The storage decision is virtio-blk + btrfs host. Per-user data.img files live
on per-user btrfs subvolumes. Instant CoW snapshots, forks, and incremental
migration via btrfs send/receive. This is deployed and proven.

## What Changed

- 2026-03-11: Added dev/prod dual-VM model, heavy job queue, workload resource
  profiles, and promotion-as-invariant. Restructured ADR around workload
  classes instead of single-VM lifecycle.
- 2026-03-08: CRITICAL CORRECTION — virtiofs cannot survive VM snapshot/restore.
  Changed to virtio-blk + btrfs host for all mutable data.
- Merged ADR-0010 (fleet lifecycle API + capacity) into this ADR.

## What To Do Next

1. Implement dev/prod VM pair creation on user registration.
2. Implement promotion API (dev -> prod) with verification gate.
3. Implement job queue for heavy workloads (Playwright, full builds).
4. Define resource profiles per workload class.
5. Wire btrfs snapshot into promotion (snapshot dev, apply to prod).
6. (Later) Evaluate Mutagen for desktop sync prototype.

---

## 1) Workload Classes and Resource Profiles

Not all work is equal. Resource allocation must match the workload.

### 1.1 User sandbox (prod VM)

The user's persistent environment. Serves their app, runs their agents,
holds their workspace. This is the only long-lived VM per user.

- Long-lived, always available (or wakes on request from hibernation)
- ~256-430 MB RAM, 1 vCPU
- Idle hibernation with heartbeat watchdog
- Instant restore from snapshot on return
- Updated only via verified promotion from the build pool

### 1.2 Build/dev jobs (shared pool)

All development work — compilation, testing, coding agents, verification —
runs on a shared pool of beefy VMs. Users do not compile in their own VM.

- Short-to-medium lived, on-demand
- 1-4 GB RAM, 2-4 vCPU depending on job type
- Shared across all users via job queue
- Results stream back to user's EventStore
- Job VMs are ephemeral or pooled — do work, return results, recycle

### 1.3 Heavy jobs (also on shared pool)

Playwright E2E with Chrome + video recording, large test suites, batch AI
processing. Same pool as build/dev, just bigger resource profiles.

- 2-4 GB RAM, 2-4 vCPU
- Strict time limits enforced by hypervisor
- Read-only workspace snapshot, no write access to user data

### 1.4 Why a shared pool instead of per-user dev VMs

Compilation is a heavy workload. Rust cargo build needs gigs of RAM and
minutes of CPU. Even Go needs hundreds of MB. A 430 MB user VM cannot build
itself.

Per-user dev VMs sized for compilation (2-4 GB) waste memory. Most users
aren't building at the same time. A shared pool of 4-8 beefy VMs serving
100 users is radically more efficient. The pool stays busy through queuing,
not per-user allocation.

A 32 GB node: ~70 user sandboxes at 430 MB each, plus 4 pool workers at
2-4 GB each. Versus ~16 users if every user had a 2 GB dev VM.

---

## 2) Prod VM + Build Pool Model

### 2.1 Architecture

Each user has one persistent lightweight VM (prod). All development work
runs on a shared build pool.

```
User's prod VM:  serves app, runs agents, holds workspace (~256-430 MB)
Build pool:      shared beefy VMs that compile, test, verify, promote
```

### 2.2 The background processing model

Users don't need to be online while work happens. The pool processes jobs
asynchronously. User sandboxes can hibernate while their work is being done.

```
User submits work → sandbox hibernates → pool picks up job
Pool: build → test → verify → docs update
Pool done → promote result into user's prod snapshot
User returns → sandbox wakes → update applied
```

This enables 24/7 background processing for all users. Coding agents work
overnight. Verification runs while the user sleeps. Results accumulate and
are applied on return.

### 2.3 Promotion as invariant enforcement

Promotion from build pool to prod requires verification. This is enforced by
the infrastructure. The promotion API refuses to proceed unless:

- Tests pass (or test results are explicitly acknowledged)
- The harness reports completion (code + tests + docs atomic)
- The user approves (or auto-approve is configured)

Prod never drifts because it only changes through verified promotion.

### 2.4 Promotion mechanics

1. Build pool job completes with artifacts
2. Snapshot user's prod data.img (btrfs, <1s) as rollback point
3. Apply build artifacts to user's workspace
4. Verify: tests pass, health check, docs updated
5. Stop or hibernate user's prod VM
6. Swap prod data.img with promoted version (reflink copy)
7. Start prod VM
8. Health check → promotion complete or rollback

Rollback: pre-promotion snapshot always retained. Swap back and restart.

### 2.5 Implication for sandbox language choice

If the sandbox binary is Go, compilation takes seconds and needs hundreds of
MB. The build pool turns around updates fast and cheaply. Rust compilation
takes minutes and needs gigs. This is a concrete reason the sandbox runtime
should move to Go — it makes the build pool dramatically more efficient.

### 2.6 Inter-agent communication model

Agents inside a sandbox never talk to other VMs directly. All inter-agent
communication is mediated by the hypervisor control plane.

```
Within a VM:     actors talk directly (in-process messaging)
VM to hypervisor: HTTP API (provider gateway, job queue, publish/retrieve)
Between users:   mediated by hypervisor (published docs, shared index)
```

The sandbox only knows the hypervisor's API endpoint. No cross-VM networking.
Each sandbox is a hermit.

**Job dispatch:** Terminal agent calls `POST /jobs/v1/run` to the hypervisor.
Hypervisor snapshots workspace, creates a pool job, streams events back to
the requesting sandbox's EventStore.

**Writer-to-writer communication:** User A's writer publishes a living
document to the hypervisor. User B's writer retrieves it via the hypervisor
API. Writers don't talk to each other directly — they publish and retrieve
through a shared authority on the control plane. This is the foundation for
collaborative living documents across users.

### 2.7 Relationship to Node B legacy topology

Node B runs `sandbox-live` (8080) and `sandbox-dev` (8081) as separate
systemd services. The dev instance prefigures the build pool concept. The
target replaces the global dev instance with a shared build pool serving all
users.

---

## 3) Heavy Job Queue

### 3.1 Design

Heavy jobs are dispatched from the user's dev VM (or from the conductor) and
run on shared infrastructure. The job queue manages:

- Job submission (user or agent requests a heavy job)
- Capacity allocation (find or create a job VM with the right profile)
- Execution (run the job in an isolated ephemeral VM)
- Result streaming (JSONL events back to the user's EventStore)
- Cleanup (destroy job VM after completion)

### 3.2 Job types

| Job type | Profile | Typical duration |
|----------|---------|-----------------|
| Playwright E2E (Chrome + video) | 2 GB RAM, 2 vCPU | 2-10 min |
| Full cargo build | 2-4 GB RAM, 4 vCPU | 5-30 min |
| Large test suite | 1-2 GB RAM, 2 vCPU | 1-15 min |
| AI batch processing | 1-2 GB RAM, 1 vCPU | variable |

### 3.3 Scheduling

Simple FIFO with priority for now. No preemption. Capacity limit per node.
If all job slots are full, jobs queue. Users see queue position and ETA via
events.

Future: priority queuing, job affinity, cross-node dispatch.

### 3.4 Security

Job VMs are ephemeral and isolated. They receive a read-only snapshot of the
user's workspace (or specific files) and return results. They do not have
write access to the user's dev or prod data.img.

---

## 4) Storage Decision

### 4.1 Virtio-blk + Btrfs Host

This is deployed and proven. The full rationale is preserved here for reference.

1. **Snapshot-safe.** cloud-hypervisor VM snapshots capture virtio-blk state
   atomically. After vm.restore, the block device is exactly as it was.
2. **Already in use.** ChoirOS uses virtio-blk data.img for sandbox mutable
   state since ADR-0016.
3. **Industry-proven.** Fly.io uses NVMe thin LVM → virtio-blk. Replit uses
   btrfs + NBD to GCS. Both are block-device-first.
4. **Btrfs host backing.** Per-user data.img files live on host btrfs
   subvolumes → instant CoW snapshots, forks, incremental migration.
5. **Near-native performance.** virtio-blk is the fastest guest I/O path.
6. **Per-user quotas.** btrfs qgroup limit — kernel-enforced on host.
7. **Crash safety.** btrfs CoW + ext4 journal inside data.img.

### 4.2 Why NOT virtiofs for mutable data

virtiofs FUSE file handles are NOT captured in cloud-hypervisor VM snapshots
(issue #6931). After restore, stale handles → I/O errors and data loss.

**UPDATE (ADR-0018):** virtiofs for read-only shares also removed. shared=on
blocks KSM page deduplication, and virtiofsd costs 176 MB/VM. ADR-0018
replaces nix-store virtiofs with shared read-only erofs/squashfs virtio-blk.

### 4.3 Storage layout

```
Host (btrfs at /data):
  /data/users/{user_id}/
    data.img                user's prod VM virtio-blk volume (ext4, 2-10 GB)
  /data/snapshots/{user_id}/
    {timestamp}/            btrfs CoW snapshots (pre-promotion rollback, etc.)
  /data/pool/
    jobs/{job_id}/          ephemeral job workspace (snapshot of user data)

VM state dir (/opt/choiros/vms/state/{instance}/):
  data.img -> /data/users/{user_id}/data.img
  vm.pid, vm.log, *.sock

Guest VM (user prod):
  /opt/choiros/data/sandbox/
    ├── runtime/            events.db, conductor runs, writer state
    └── workspace/          user files, projects
  /nix/store               (erofs/squashfs virtio-blk, shared read-only)
  /tmp                     (tmpfs, ephemeral)

Pool job VM (ephemeral):
  /workspace/              read-only snapshot of user's workspace
  /scratch/                writable scratch for build artifacts
  /nix/store               (shared read-only, same as user VMs)
```

### 4.4 Btrfs gotchas

- CoW fragmentation on random-write files (SQLite, VM images). Mitigate with
  chattr +C on specific directories.
- Reflink copy slow on highly fragmented files. Defragment first if needed.
- Quotas (qgroups) add overhead. Enable only when multitenancy requires it.

---

## 5) VM Lifecycle API

### 5.1 Endpoints

```
POST   /v1/vms                         create (provisions dev + prod pair)
POST   /v1/vms/{vm_id}/start
POST   /v1/vms/{vm_id}/stop
POST   /v1/vms/{vm_id}/snapshot
POST   /v1/vms/{vm_id}/restore
POST   /v1/vms/{vm_id}/promote         dev → prod promotion with verification
DELETE /v1/vms/{vm_id}
GET    /v1/vms/{vm_id}
GET    /v1/vms?owner_id=...

POST   /v1/jobs                        submit heavy job
GET    /v1/jobs/{job_id}               job status + results
DELETE /v1/jobs/{job_id}               cancel job
```

### 5.2 Lifecycle state machine

```
creating -> stopped -> running -> stopping -> stopped
                                   |
running -> pausing -> paused -> snapshotting -> snapshotted -> restoring -> running
                                   |
                                deleted / failed

Promotion (dev → prod):
  dev running → snapshot dev → verify → stop prod → apply → start prod → health check
```

### 5.3 Sizing

Default user sandbox: 1 vCPU / 256-430 MB RAM.
Pool workers: 2-4 vCPU / 2-4 GB RAM per job.

| Profile | Usable RAM | User sandboxes | Pool workers | Parked snapshots |
|---------|-----------|---------------|-------------|-----------------|
| 32 GB node | ~26 GB | ~60 active | 2-4 | 200+ |
| 64 GB node | ~52 GB | ~120 active | 4-8 | 400+ |
| 256 GB node | ~224 GB | ~500 active | 8-16 | 1500+ |

Assumptions: 1.0 RAM overcommit, 20% host reserve, hibernated sandboxes
consume only snapshot storage. Pool workers are shared across all users.
Most users are hibernated at any time — active count is concurrent, not total.

---

## 6) Desktop Sync (Future)

### 6.1 Phased approach

Phase 1: Mutagen for VM↔Desktop sync. Three-way merge, rsync-efficient delta,
filesystem watching. Run between host btrfs subvolume and desktop client folder.

Phase 2: SQLite metadata index + smart sync. Sync metadata eagerly, content
lazily. Content-addressed blob transfer.

Phase 3: OS-native placeholder files (macOS File Provider, Windows Cloud Files
API, Linux FUSE).

Phase 4: (Optional) cr-sqlite for multi-device CRDT sync.

### 6.2 Desktop sync is not blocking

Desktop sync is a desirable feature but not required for the core platform.
Users interact through the web UI first. Desktop sync enables local editor
integration (VS Code, Cursor, etc.) for users who want it.

---

## 7) Validation

### Implemented gates (DONE)

- Host btrfs partition on both nodes
- Per-user btrfs subvolumes on VM create
- Per-user virtio-blk data.img on btrfs subvolume
- Persistence across VM stop/start and hibernate/restore
- Per-user routing with dynamic ports, DHCP, per-user VMs on Node B
- Idle hibernation with heartbeat watchdog

### Remaining gates

- Dev/prod VM pair creation on user registration
- Promotion API with verification gate
- Promotion snapshot → apply → health check cycle
- Job queue submission and execution
- Job VM ephemeral lifecycle (create → run → stream results → destroy)
- Resource profile enforcement per workload class
- Cross-node migration via btrfs send/receive (deferred)

---

## Sources

### Storage
- [Replit: Storage The Next Generation](https://blog.replit.com/replit-storage-the-next-generation)
- [AgentFS FUSE (Turso)](https://turso.tech/blog/agentfs-fuse)
- [FUSE Performance (ACM)](https://dl.acm.org/doi/fullHtml/10.1145/3310148)
- [virtiofs Design](https://virtio-fs.gitlab.io/design.html)
- [btrfs Subvolumes](https://btrfs.readthedocs.io/en/latest/Subvolumes.html)
- [btrfs Send/Receive](https://btrfs.readthedocs.io/en/latest/Send-receive.html)

### Sync
- [Dropbox: Rewriting the Heart of Our Sync Engine](https://dropbox.tech/infrastructure/rewriting-the-heart-of-our-sync-engine)
- [Mutagen File Synchronization](https://mutagen.io/documentation/synchronization/)
- [cr-sqlite (vlcn.io)](https://github.com/vlcn-io/cr-sqlite)
- [Nextcloud Desktop Architecture](https://docs.nextcloud.com/desktop/3.3/architecture.html)
- [Windows Cloud Files API](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine)

### Industry references
- [Fly.io: NVMe volume slices → virtio-blk](https://fly.io/docs)
- [Docker Desktop uses Mutagen](https://mutagen.io)
- [Replit: btrfs + NBD to GCS](https://blog.replit.com)
