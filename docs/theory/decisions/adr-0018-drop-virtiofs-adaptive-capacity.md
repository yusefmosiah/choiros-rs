# ADR-0018: Drop Virtiofs, Enable KSM, Adaptive VM Capacity Management

Date: 2026-03-09
Kind: Decision
Status: Draft
Priority: 1
Requires: [ADR-0014, ADR-0016]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

Per-user VM isolation (ADR-0014 Phase 4) works but hits a hard ceiling at
~42 concurrent VMs on a 32 GB node. The bottleneck is memory: each VM
costs ~514 MB (338 MB cloud-hypervisor + 176 MB virtiofsd). KSM cannot
help because virtiofs requires `shared=on` (MAP_SHARED), and KSM only
deduplicates MAP_PRIVATE pages. This is a fundamental Linux kernel
limitation confirmed by cloud-hypervisor maintainers (issue #5873).

This ADR eliminates virtiofs entirely, replacing it with a shared
read-only squashfs virtio-blk image for `/nix/store`. This enables
`shared=off,mergeable=on`, unlocking KSM page deduplication. Combined
with eliminating virtiofsd overhead (176 MB/VM), projected per-VM cost
drops from ~514 MB to ~170 MB — a 3x capacity increase to ~170 VMs.

Additionally, this ADR introduces adaptive capacity management: the idle
watchdog becomes memory-pressure-aware (hibernating VMs sooner under
load), a capacity gate rejects new VMs when at limit (HTTP 503 with
retry guidance), and 16 GB swap provides a safety net for pressure spikes.

## What Changed

- 2026-03-09: Initial draft based on heterogeneous load test findings.

## What To Do Next

See companion implementation guide: `docs/theory/guides/adr-0018-implementation.md`

---

## Decision 1: Replace Virtiofs with Virtio-blk for Nix Store

### Context

Each VM currently uses 2 virtiofs shares:
- `/nix/store` (read-only, ~500 MB closure) — sandbox binary + NixOS runtime
- `/run/choiros/credentials/sandbox` (read-only) — one env file with gateway token

Each virtiofs share requires a virtiofsd daemon process. With 4 virtiofsd
instances per VM at ~42 MB each, this costs 176 MB RSS per VM — more than
half the per-VM overhead. virtiofs also forces `shared=on` in
cloud-hypervisor memory config, which blocks KSM.

### Decision

1. Build the sandbox NixOS closure into a **shared read-only squashfs image**
   mounted as a second virtio-blk device in each VM.
2. Drop the credentials virtiofs share — inject the gateway token via
   kernel cmdline (`choir.gateway_token=<TOKEN>`). A guest systemd
   oneshot extracts it from `/proc/cmdline` and writes an env file.
3. Set cloud-hypervisor memory to `shared=off,mergeable=on` to enable KSM.

### Consequences

**Positive:**
- Eliminates all virtiofsd processes (176 MB/VM savings)
- Enables KSM (estimated 50% dedup on identical NixOS guest pages)
- Fixes VM snapshot/restore (virtiofs FUSE handles don't survive snapshots)
- Faster VM boot (no virtiofsd socket handshake)

**Negative:**
- Must rebuild squashfs image on every NixOS config or sandbox binary change
- Adds a build step to the deploy pipeline (~30s)
- Squashfs image is ~500 MB-1 GB on disk (shared across all VMs, one copy)

### Why Squashfs

- Read-only by design (no accidental writes)
- Compressed (LZ4: ~40% of uncompressed size, fast decompression)
- Multiple VMs can open the same image file with `readonly=on`
- Linux kernel has native squashfs support (no FUSE)
- `mksquashfs` from `squashfs-tools` is in nixpkgs

### Why Not Ext4 Read-Only Image

Ext4 images work but are uncompressed (~1.5 GB vs ~600 MB squashfs).
Squashfs is purpose-built for read-only compressed filesystems.

---

## Decision 2: Adaptive Idle Watchdog

### Context

The current idle watchdog uses a fixed 30-minute timeout. When 42 VMs
accumulated (32 stale from previous tests + 10 new), the node ran out of
memory and became unresponsive. The watchdog had not yet timed out the
stale VMs.

### Decision

The idle watchdog reads system available memory from `/proc/meminfo` and
adjusts its hibernation aggressiveness based on memory pressure:

| Available Memory | Pressure Level | Idle Timeout |
|-----------------|----------------|-------------|
| > 60% of total | Normal | Configured (30 min) |
| 30-60% | Warning | 5 minutes |
| 15-30% | High | 30 seconds |
| < 15% | Critical | Immediate (hibernate least-recent) |

At Critical, the watchdog hibernates the N least-recently-active VMs
needed to bring available memory above the High threshold, regardless of
their idle duration.

### Consequences

- VMs hibernate faster under load, preserving system stability
- Active users are unaffected (their last_activity is recent)
- Cold users get hibernated sooner but restore from snapshot in ~4.5s

---

## Decision 3: Capacity Gate

### Context

Currently `ensure_running()` will always attempt to spawn a new VM, even
if the system is out of memory. This leads to OOM rather than a graceful
error.

### Decision

Before spawning a new VM, `ensure_running()` checks:
1. Running VM count against a configurable max (default: 50)
2. Available system memory > 1 GB (enough for one VM + headroom)

If either check fails, return HTTP 503 Service Unavailable with a
`Retry-After: 30` header and a user-friendly message: "Server at
capacity. Your workspace will be available shortly — please try again
in 30 seconds."

The middleware already maps `ensure_running` errors to 503. The capacity
check just adds a clear pre-flight check with a specific error message.

### Consequences

- Users get a clear error instead of a hung request or OOM crash
- The 503 + Retry-After pattern is standard and works with browsers
- Frontend can show a waiting room / retry UI

---

## Decision 4: Swap (16 GB)

### Context

Both nodes have zero swap configured. With spiky VM workloads, there is
no safety net between "memory is getting tight" and "OOM killer fires."

### Decision

Add a 16 GB swapfile on each node. 16 GB on a 32 GB machine provides
ample headroom for pressure spikes. The NVMe storage has 444 GB free;
16 GB is negligible.

Swap is not for steady-state use — it's a safety net. Hibernated VMs
that get swapped out are not latency-sensitive (they restore from
snapshot, not from swap). The adaptive watchdog should keep the system
out of swap under normal operation.

### Consequences

- OOM is much harder to trigger
- Hibernated VM memory pages naturally migrate to swap, freeing RAM
- Negligible disk cost (16 GB / 444 GB free)

---

## Projected Capacity

| Metric | Current (virtiofs) | After ADR-0018 |
|--------|-------------------|----------------|
| cloud-hypervisor RSS/VM | 338 MB | ~170 MB (KSM) |
| virtiofsd RSS/VM | 176 MB | 0 MB |
| Total per-VM | 514 MB | ~170 MB |
| Max VMs (32 GB, no swap) | ~58 | ~170 |
| Max VMs (32 GB + 16 GB swap) | ~58 | ~200+ |
| Snapshot/restore | Broken (virtiofs) | Clean |
| VM boot time | ~12s | ~8-10s (no virtiofsd) |

KSM deduplication estimate assumes ~50% page sharing across identical
NixOS guests. Actual savings depend on workload divergence — idle VMs
will share more, heavy VMs less. The load test report showed 338 MB RSS
at 1024 MB configured; with KSM on identical guests, ~170 MB is
conservative.

---

## Sources

- [KSM merges no pages with shared=on — cloud-hypervisor#5873](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/5873)
- [KSM and virtiofs incompatibility — kata-containers/runtime#2798](https://github.com/kata-containers/runtime/issues/2798)
- [Linux KSM documentation](https://www.kernel.org/doc/html/latest/admin-guide/mm/ksm.html)
- [virtiofs requires MAP_SHARED — virtio-fs/qemu#16](https://gitlab.com/virtio-fs/qemu/-/issues/16)
- [cloud-hypervisor memory docs](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/memory.md)
- [ChoirOS heterogeneous load test report (2026-03-09)](../state/reports/2026-03-09-heterogeneous-load-test.md)
- [ChoirOS per-user VM load test report (2026-03-09)](../state/reports/2026-03-09-per-user-vm-load-test.md)
