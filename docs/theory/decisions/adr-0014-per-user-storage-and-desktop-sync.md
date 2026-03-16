# ADR-0014: Per-User VM Lifecycle, Storage, and Desktop Sync

Date: 2026-03-06
Kind: Decision
Status: Accepted
Priority: 2
Requires: [ADR-0007, ADR-0012]
Authors: wiz + Claude
Updated: 2026-03-16

## Narrative Summary (1-minute read)

Each ChoirOS user gets one lightweight VM (their sandbox). Build, test, and
other heavy development work runs on a shared pool of worker VMs. The user's
persistent sandbox changes through promotion from that pool, with btrfs
snapshot rollback and restart-based health checks already deployed.

VM types are abstracted as **machine classes**: a named configuration of
hypervisor (CH/FC), store transport (blk/pmem), vCPU count, and memory size.
Machine classes are runtime configuration, not compiled into code. Account
tiers map to default machine classes, but the mapping is configurable and
the class set is designed for experimentation — we don't yet know the right
sizes and combinations.

The job queue and machine classes are tier-aware in design. The queue's
persistent records, priority fields, and promotion API are deployed; warm
pool management, hard tier budgets, and the fully automated verification gate
remain follow-on work.

The storage decision is virtio-blk + btrfs host. Per-user `data.img` files
live on per-user btrfs subvolumes. The sandbox binary now lives on `data.img`
too, so promotion can update a user's sandbox without rebuilding the guest
image. Instant CoW snapshots, forks, and incremental migration via btrfs
send/receive are the long-term storage primitive. Desktop sync is still a
future layer on top of this model, not a prerequisite for it.

## What Changed

- 2026-03-16: Reconciled the ADR with Phases 1-8 now deployed on the
  implementation path. Marked the single-sandbox + shared build pool model
  as accepted, clarified that job queue/promotion bootstrap exists today,
  and narrowed remaining work to worker-pool management, automated
  verification, inter-agent communication, and desktop sync.
- 2026-03-11: Added machine classes as runtime configuration. VM type
  (hypervisor, transport, sizing) is a named config, not compiled into code.
  Account tiers map to default classes. Job queue priority is tier-aware.
- 2026-03-11: Simplified from dev/prod dual-VM to single prod VM + shared
  build pool. Added heavy job queue, workload resource profiles, and
  promotion-as-invariant.
- 2026-03-08: CRITICAL CORRECTION — virtiofs cannot survive VM snapshot/restore.
  Changed to virtio-blk + btrfs host for all mutable data.
- Merged ADR-0010 (fleet lifecycle API + capacity) into this ADR.

## What To Do Next

1. Implement Phase 7.5 worker-pool management: warm workers, scheduling loop,
   event streaming, and tier budget enforcement.
2. Upgrade promotion from rollbackable admin flow to an enforced verification
   gate that blocks completion until harness policy passes.
3. Add automatic class-change cold boot and generation-aware snapshot
   invalidation.
4. Finish the CH/FC x blk/pmem evaluation matrix and tune default machine
   classes from measured workloads rather than static guesses.
5. Implement Phase 9 hypervisor-mediated publish/retrieve for cross-user
   agent and document exchange.
6. Prototype desktop sync with Mutagen without coupling core VM lifecycle to
   local desktop presence.

---

## 1) Machine Classes

### 1.1 What a machine class is

A machine class is a named configuration that defines how a VM runs:

```toml
[classes.ch-pmem-2c-1g]
hypervisor = "cloud-hypervisor"
transport = "pmem"
vcpu = 2
memory_mb = 1024
runner = "/nix/store/...-choiros-ch-sandbox-live/bin/microvm-run"
```

Machine classes are **runtime configuration**, not compiled into code. Adding
a new class = adding a config entry. No code changes, no recompilation. The
host nix config generates the config file (so runner paths resolve correctly),
but the class definitions themselves are just data.

### 1.2 VM type axes

The current axes are:

- **Hypervisor**: cloud-hypervisor, firecracker (possibly others later)
- **Store transport**: blk (virtio-blk), pmem (virtio-pmem, requires erofs alignment)
- **vCPU count**: 1, 2, 4
- **Memory**: 256 MB, 512 MB, 1 GB, 2 GB, 4 GB

Not all combinations make sense. The class set is designed for experimentation:
we don't know the right sizes yet. The system should make it easy to define,
deploy, and measure new classes without code changes.

### 1.3 Account tiers and class mapping

Account tiers map to default machine classes. The mapping is runtime
configuration:

```toml
[tier-defaults]
# These are EXAMPLES, not decisions. Real mappings will emerge
# from experimentation with actual workloads.
free = "..."
pro = "..."
```

Users get the class for their tier by default. Per-user overrides are possible
(admin API). Tiers also affect job queue priority and resource budgets.

The tier names, class assignments, and even which axes matter most are all
unknown. Hypotheses to test:

- Free tier might get pmem (cheaper per-VM, shared, fast) while paid gets
  blk (stronger isolation). Or the opposite. Or it might not matter.
- Firecracker might have lower overhead than cloud-hypervisor. Or not.
- 256 MB might be enough for a prod sandbox. Or 512 MB might be the floor.
- The dominant constraint might be something we haven't identified yet.

The machine class system exists to make these experiments cheap and
CI-controlled, not to encode premature decisions.

### 1.4 How machine classes flow through the system

1. Host nix config builds all runners and writes machine class config file
2. Hypervisor reads config on startup
3. `ensure()` takes a class name, looks up config, picks runner + systemd template
4. Per-VM state dir gets a `machine-class` file so the system remembers
5. E2E tests can create users with specific classes via API
6. Stress tests can mix classes to measure real contention

### 1.5 Snapshot portability within a class

VM snapshots are valid only within the same machine class AND same nix
generation. A snapshot taken on `ch-pmem-2c-1g` cannot restore on
`fc-blk-1c-512m`. A snapshot from a previous `nixos-rebuild` cannot restore
because nix store paths (kernel, initrd, erofs) changed.

Within the same class and generation, snapshots survive hypervisor restarts.
If the class or nix generation changes, the right behavior is to invalidate
the snapshot and cold-boot against the same `data.img`. The current snapshot
invalidation (ovh-node.nix) is still over-aggressive — it wipes on every
restart instead of only on generation change.

---

## 2) Workload Classes and Resource Profiles

Not all work is equal. Resource allocation must match the workload.

### 2.1 User sandbox (prod VM)

The user's persistent environment. It serves their app, holds their
workspace, and hosts the user-facing runtime. This is the only long-lived VM
per user.

- Long-lived, always available (or wakes on request from hibernation)
- Machine class determined by account tier
- Idle hibernation with heartbeat watchdog
- Instant restore from snapshot on return (same nix generation)
- Updated only via promotion from the build pool
- Lightweight/local orchestration may happen here, but compile/test/coding
  jobs are dispatched to the shared pool

### 2.2 Build/dev jobs (shared pool)

All development work — compilation, testing, coding agents, verification —
runs on a shared pool of beefy VMs. Users do not compile in their own VM.

- Short-to-medium lived, on-demand
- Machine class optimized for build workloads (more CPU/RAM)
- Shared across all users via job queue
- Results stream back to user's EventStore
- Job VMs are ephemeral or pooled — do work, return results, recycle
- Queue records and admin APIs are deployed; warm pool management and event
  streaming are the next layer
- **Job priority is tier-aware**: full tier budgets and enforcement remain
  follow-on work

### 2.3 Heavy jobs (also on shared pool)

Playwright E2E with Chrome + video recording, large test suites, batch AI
processing. Same pool as build/dev, just bigger resource profiles.

- Larger machine class (more RAM for Chrome, video encoding)
- Strict time limits enforced by hypervisor
- Read-only workspace snapshot, no write access to user data
- Same queueing/policy surface as build jobs, just with larger profiles
- **Time and concurrency budgets per tier** are part of Phase 7.5, not yet
  fully enforced

### 2.4 Why a shared pool instead of per-user dev VMs

Compilation is a heavy workload. Rust cargo build needs gigs of RAM and
minutes of CPU. Even Go needs hundreds of MB. A 512 MB user VM cannot build
itself.

Per-user dev VMs sized for compilation (2-4 GB) waste memory. Most users
aren't building at the same time. A shared pool of 4-8 beefy VMs serving
100 users is radically more efficient. The pool stays busy through queuing,
not per-user allocation.

A 32 GB node: ~60 user sandboxes at varying class sizes, plus 4 pool workers
at 2-4 GB each. Versus ~16 users if every user had a 2 GB dev VM.

---

## 3) Prod VM + Build Pool Model

### 3.1 Architecture

Each user has one persistent lightweight VM (prod). All development work
runs on a shared build pool.

```
User's prod VM:  serves app, holds workspace, resumes quickly (~256-430 MB)
Build pool:      shared beefy VMs that compile, test, run coding jobs, promote
```

### 3.2 The background processing model

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

### 3.3 Promotion as invariant enforcement

Promotion from build pool to prod is the architectural invariant. The
deployed system already routes binary updates through a promotion API with
btrfs snapshot rollback and restart-based health checks. The remaining gap is
that verification is still bootstrap/manual rather than a mandatory policy
gate.

Target enforcement:

- Tests pass (or test results are explicitly acknowledged)
- The harness reports completion (code + tests + docs atomic)
- The user approves (or auto-approve is configured)

Prod should never drift through ad hoc mutation; it should continue to move
through the promotion path only.

### 3.4 Promotion mechanics

1. Build pool job completes with artifacts
2. Snapshot user's prod data.img (btrfs, <1s) as rollback point
3. Apply build artifacts to user's workspace
4. Verify: current deployment accepts optional/manual verification payload and
   always runs post-restart health checks; target is an enforced harness gate
5. Stop or hibernate user's prod VM
6. Swap prod data.img with promoted version (reflink copy)
7. Start prod VM
8. Health check → promotion complete or rollback

Rollback: pre-promotion snapshot always retained. Swap back and restart.

### 3.5 Implication for sandbox language choice

If the sandbox binary is Go, compilation takes seconds and needs hundreds of
MB. The build pool turns around updates fast and cheaply. Rust compilation
takes minutes and needs gigs. This is a concrete reason the sandbox runtime
should move to Go — it makes the build pool dramatically more efficient.

### 3.6 Inter-agent communication model

Agents inside a sandbox never talk to other VMs directly. All inter-agent
communication is mediated by the hypervisor control plane.

```
Within a VM:     actors talk directly (in-process messaging)
VM to hypervisor: HTTP API (provider gateway, job queue, publish/retrieve)
Between users:   mediated by hypervisor (published docs, shared index)
```

The sandbox only knows the hypervisor's API endpoint. No cross-VM networking.
Each sandbox is a hermit.

**Job dispatch:** Terminal agents submit work to the hypervisor's job
interface. The hypervisor snapshots workspace, creates a pool job, and
streams events back to the requesting sandbox's EventStore.

**Writer-to-writer communication:** User A's writer publishes a living
document to the hypervisor. User B's writer retrieves it via the hypervisor
API. Writers don't talk to each other directly — they publish and retrieve
through a shared authority on the control plane. This is the foundation for
collaborative living documents across users.

### 3.7 Relationship to Node B legacy topology

Node B runs `sandbox-live` (8080) and `sandbox-dev` (8081) as separate
systemd services. The dev instance prefigures the build pool concept. The
target replaces the global dev instance with a shared build pool serving all
users.

---

## 4) Job Queue (Bootstrap Deployed, Worker Management Deferred)

### 4.1 Design

Jobs are dispatched from the user's sandbox (via terminal agent) or from the
conductor and run on shared infrastructure. The job queue manages:

- Job submission (user or agent requests a heavy job)
- Capacity allocation (find or create a job VM with the right profile)
- Execution (run the job in an isolated ephemeral VM)
- Result streaming (JSONL events back to the user's EventStore)
- Cleanup (destroy job VM after completion)

The queue's persistent tables, status transitions, machine-class selection,
and admin API are implemented. The scheduler loop, warm worker pool, and
streaming back to the user's EventStore are deferred to the next phase.

### 4.2 Job types

| Job type | Profile | Typical duration |
|----------|---------|-----------------|
| Playwright E2E (Chrome + video) | 2 GB RAM, 2 vCPU | 2-10 min |
| Full cargo build | 2-4 GB RAM, 4 vCPU | 5-30 min |
| Large test suite | 1-2 GB RAM, 2 vCPU | 1-15 min |
| AI batch processing | 1-2 GB RAM, 1 vCPU | variable |

### 4.3 Scheduling and tier budgets

Priority queue by account tier. Within a tier, FIFO. No preemption.
Tier names and specific budgets are runtime configuration — the system is
designed to enforce whatever the config says, without hard-coding specific
plans.

Per-tier budgets (all configurable):
- Max concurrent jobs per user
- Max job duration
- Max jobs per day/week
- Which machine classes are available for jobs
- Queue priority weight

The current implementation stores priority and resource-profile metadata, but
hard budget enforcement and queue-position event delivery are still pending.

Future: job affinity, cross-node dispatch.

### 4.4 Security

Job VMs are ephemeral and isolated. They receive a read-only snapshot of the
user's workspace (or specific files) and return results. They do not have
write access to the user's dev or prod data.img.

---

## 5) Storage Decision

### 5.1 Virtio-blk + Btrfs Host

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

### 5.2 Why NOT virtiofs for mutable data

virtiofs FUSE file handles are NOT captured in cloud-hypervisor VM snapshots
(issue #6931). After restore, stale handles → I/O errors and data loss.

**UPDATE (ADR-0018):** virtiofs for read-only shares also removed. shared=on
blocks KSM page deduplication, and virtiofsd costs 176 MB/VM. ADR-0018
replaces nix-store virtiofs with shared read-only erofs/squashfs virtio-blk.

### 5.3 Storage layout

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
    ├── bin/sandbox         promoted sandbox binary on data.img
    ├── runtime/            events.db, conductor runs, writer state
    └── workspace/          user files, projects
  /nix/store               (erofs/squashfs virtio-blk, shared read-only)
  /tmp                     (tmpfs, ephemeral)

Pool job VM (ephemeral):
  /workspace/              read-only snapshot of user's workspace
  /scratch/                writable scratch for build artifacts
  /nix/store               (shared read-only, same as user VMs)
```

The eventual desktop-facing home-directory/XDG projection can sit on top of
this layout. The source of truth remains the per-user `data.img` and its
host-side btrfs subvolume.

### 5.4 Btrfs gotchas

- CoW fragmentation on random-write files (SQLite, VM images). Mitigate with
  chattr +C on specific directories.
- Reflink copy slow on highly fragmented files. Defragment first if needed.
- Quotas (qgroups) add overhead. Enable only when multitenancy requires it.

---

## 6) Control Plane and Lifecycle State

### 6.1 Current API surface

The deployed surface is still admin/bootstrap-oriented rather than a polished
public VM lifecycle API. The important current control-plane edges are:

```
PUT    /profile/machine-class
GET    /profile/machine-class
POST   /admin/sandboxes/{user_id}/{role}/start
POST   /admin/sandboxes/{user_id}/{role}/stop
POST   /admin/sandboxes/{user_id}/{role}/hibernate
PUT    /admin/sandboxes/{user_id}/machine-class
DELETE /admin/sandboxes/{user_id}/machine-class

POST   /admin/jobs
GET    /admin/jobs
GET    /admin/jobs/{job_id}
DELETE /admin/jobs/{job_id}

POST   /admin/sandboxes/{user_id}/promote
GET    /admin/sandboxes/{user_id}/promotions
GET    /admin/promotions/{promotion_id}
```

A simpler public API can be layered later once the architecture settles.

### 6.2 Lifecycle state machine

```
creating -> stopped -> running -> stopping -> stopped
                                   |
running -> pausing -> paused -> snapshotting -> snapshotted -> restoring -> running
                                   |
                                deleted / failed

Promotion:
  job complete → snapshot user data → apply artifact → restart sandbox → health check
              ↘ on failure retain rollback snapshot and mark promotion failed
```

In the current implementation a class or generation mismatch should break the
snapshot/restore branch and force a cold boot using the same `data.img`.

### 6.3 Sizing

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

## 7) Desktop Sync (Future)

### 7.1 Phased approach

Phase 1: Mutagen for host-subvolume to desktop-folder sync. Three-way merge,
delta transfer, and filesystem watching. Treat the per-user btrfs subvolume
backing `data.img` as the authoritative source.

Phase 2: SQLite metadata index + smart sync. Sync metadata eagerly, content
lazily. Content-addressed blob transfer.

Phase 3: OS-native placeholder files (macOS File Provider, Windows Cloud Files
API, Linux FUSE).

Phase 4: (Optional) cr-sqlite for multi-device CRDT sync.

Desktop sync should cover user files and settings, not platform-owned store
content or the promotion mechanism for the sandbox binary itself.

### 7.2 Desktop sync is not blocking

Desktop sync is a desirable feature but not required for the core platform.
Users interact through the web UI first. Desktop sync enables local editor
integration (VS Code, Cursor, etc.) for users who want it.

---

## 8) Validation

### Implemented gates (DONE)

- Host btrfs partition on both nodes
- Per-user btrfs subvolumes on VM create
- Per-user virtio-blk data.img on btrfs subvolume
- Persistence across VM stop/start and hibernate/restore
- Per-user routing with dynamic ports, DHCP, per-user VMs on Node B
- Idle hibernation with heartbeat watchdog
- Machine class config: 20 classes defined, deployed, stress tested
- Machine class selection via API (`PUT /profile/machine-class`)
- Mixed-class stress test: 62 VMs (ch-blk-2c-2g + w-ch-pmem-4c-4g)
- Compute workloads verified: Go, Rust, Node.js, Playwright, disk I/O
- Sandbox binary seeded from nix store to data.img on first boot
- SQLite-backed job queue with admin API, status tracking, and priority fields
- Promotion API with btrfs snapshot rollback, binary copy to data.img, restart,
  and health-check-based success/failure reporting

### Remaining gates

- All 4 VM types boot and pass health checks (CH/FC × blk/pmem) — CH done, FC untested
- VM sizing experimentation: find minimum viable sizes per workload
- Worker-pool management: warm workers, scheduler loop, and ephemeral job execution
- Event streaming from worker VMs back to the user's EventStore
- Promotion verification gate enforced automatically instead of manual/bootstrap
- Snapshot invalidation: generation-aware, not restart-aware
- Automatic class-mismatch handling: invalidate snapshot and cold-boot with new class
- Cross-node migration via btrfs send/receive (deferred)
- Desktop sync prototype (Mutagen first)

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
