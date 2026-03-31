# ADR-0018: Drop Virtiofs, Enable KSM, Adaptive VM Capacity Management

Date: 2026-03-09
Kind: Decision
Status: Accepted (Phases 1-6 deployed), Phase 7 planned
Priority: 1
Requires: [ADR-0014, ADR-0016]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

Per-user VM isolation (ADR-0014 Phase 4) hit a ceiling at ~42 concurrent
VMs on a 32 GB node. The bottleneck was memory: each VM cost ~514 MB
(338 MB cloud-hypervisor + 176 MB virtiofsd). KSM could not help because
virtiofs requires `shared=on` (MAP_SHARED), and KSM only deduplicates
MAP_PRIVATE pages.

This ADR eliminated virtiofs entirely. With `shares=[]`, the microvm.nix
module auto-generates an erofs disk for `/nix/store`. Combined with
`shared=off,mergeable=on`, KSM is now active. Gateway token is injected
via kernel cmdline instead of virtiofs share.

**Measured results (Node B, 2026-03-10):** Per-VM cost dropped from
~514 MB to ~443 MB (36% reduction). KSM saves 1.7 GB at 58 concurrent
VMs. Max concurrent VMs increased from ~42 to 58+ with adaptive watchdog.
16-user heterogeneous load test passed at 100%.

The ~443 MB per-VM (vs projected ~170 MB) is due to **double caching**:
the erofs nix store is cached in both the host page cache AND the guest
page cache (~100 MB overhead). Phase 7 addresses this with virtio-pmem,
which enables FSDAX (zero-copy, no guest page cache) for a projected
~330 MB per-VM.

## What Changed

- 2026-03-09: Initial draft based on heterogeneous load test findings.
- 2026-03-10: Phases 1-6 deployed and validated on Node B. Updated with
  measured results. Original squashfs plan replaced by microvm.nix erofs.
  Added Phase 7 (virtio-pmem) based on double-caching analysis.
- 2026-03-10: Promoted to Node A (both nodes now on ADR-0018).

## What To Do Next

Phase 7: Replace virtio-blk erofs with virtio-pmem erofs. This eliminates
the guest page cache overhead (~100 MB/VM) by using FSDAX direct access.
See implementation guide: `docs/adr-0018-implementation.md`

---

## Decision 1: Replace Virtiofs with Erofs Store Disk

### Context

Each VM originally used 2 virtiofs shares:
- `/nix/store` (read-only, ~500 MB closure) — sandbox binary + NixOS runtime
- `/run/choiros/credentials/sandbox` (read-only) — one env file with gateway token

Each virtiofs share required a virtiofsd daemon process. With 4 virtiofsd
instances per VM at ~42 MB each, this cost 176 MB RSS per VM — more than
half the per-VM overhead. virtiofs also forced `shared=on` in
cloud-hypervisor memory config, which blocked KSM.

### Decision

1. Set `shares = []` in the microvm guest config. The microvm.nix module
   **automatically generates an erofs disk** containing the nix store
   closure. This is a single file in `/nix/store/` on the host, shared
   by all VMs as a read-only virtio-blk device.
2. Drop the credentials virtiofs share — inject the gateway token via
   kernel cmdline (`choir.gateway_token=<TOKEN>`). A guest systemd
   oneshot extracts it from `/proc/cmdline` and writes an env file.
3. Set cloud-hypervisor memory to `shared=off,mergeable=on` to enable KSM.

### Why Erofs (Not Squashfs)

The original plan was a custom squashfs image. In practice, the microvm.nix
module already builds an erofs store disk when `shares=[]`. Using the
module's built-in mechanism is simpler and avoids conflicts with the
module's initrd closure finder. erofs also supports FSDAX (direct access)
which squashfs does not — this is critical for Phase 7 (virtio-pmem).

### Consequences

**Positive:**
- Eliminates all virtiofsd processes (176 MB/VM savings)
- Enables KSM (measured 6.6x dedup ratio at 58 VMs, 1.7 GB saved)
- Fixes VM snapshot/restore (virtiofs FUSE handles don't survive snapshots)
- Faster VM boot (no virtiofsd socket handshake): 8-14s vs 10-15s
- No custom build step — microvm.nix handles erofs image generation

**Negative:**
- Guest page cache duplicates ~100 MB of nix store data per VM (double
  caching problem). This is addressable with virtio-pmem (Phase 7).

### Measured Results (2026-03-10)

| Metric | Before (virtiofs) | After (erofs+KSM) | Improvement |
|--------|-------------------|---------------------|-------------|
| virtiofsd RSS/VM | ~176 MB | 0 | eliminated |
| Per-VM total | ~514 MB | ~443 MB | 14% reduction |
| KSM dedup | 0 (shared=on) | 1.7 GB @ 58 VMs | enabled |
| VM boot time | 10-15s | 8-14s | ~15% faster |
| Max concurrent (before OOM) | ~42 | 58+ (with watchdog) | 38%+ more |
| Load test (16 users) | — | 100% pass | validated |

### The Double-Caching Problem

With virtio-blk + erofs, nix store data exists in two places:

1. **Host page cache** — cloud-hypervisor reads erofs image blocks via
   io_uring from the host filesystem
2. **Guest page cache** — guest kernel caches erofs filesystem data inside
   guest RAM (the VM's 1024 MB allocation)

This ~100 MB overhead explains why per-VM RSS is ~443 MB instead of the
projected ~170 MB. With virtiofs, nix store data lived only in the host
page cache (one copy). The erofs approach trades one daemon (176 MB) for
guest page cache overhead (~100 MB) — still a net win, but not as large
as projected.

**Phase 7 (virtio-pmem + FSDAX)** eliminates this by mapping the erofs
image directly into the guest's address space via DAX, bypassing the
guest page cache entirely.

---

## Decision 2: Adaptive Idle Watchdog

### Context

The original idle watchdog used a fixed 30-minute timeout. When 42 VMs
accumulated (32 stale from previous tests + 10 new), the node ran out of
memory and became unresponsive.

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

### Measured Results

At 58 concurrent VMs, the adaptive watchdog correctly hibernated VMs
from 58 → 29 under memory pressure, stabilizing the system.

---

## Decision 3: Capacity Gate

Before spawning a new VM, `ensure_running()` checks:
1. Running VM count against a configurable max (default: 50)
2. Available system memory > 1 GB (enough for one VM + headroom)

If either check fails, return HTTP 503 Service Unavailable with a
`Retry-After: 30` header.

---

## Decision 4: Swap (16 GB)

16 GB swapfile on each node. Safety net for pressure spikes, not
steady-state use. Hibernated VM memory pages naturally migrate to swap.

---

## Decision 5: THP Must Be Disabled for KSM

### Context (discovered during deployment)

Even with `shared=off,mergeable=on`, KSM found zero shared pages after
52 full scans. Investigation revealed cloud-hypervisor calls
`MADV_HUGEPAGE` on VM memory. With THP=madvise (default), this created
2 MB hugepages. KSM can only merge 4 KB base pages.

### Decision

Set THP to `never` system-wide via systemd tmpfiles:

```nix
"/sys/kernel/mm/transparent_hugepage/enabled".w = { argument = "never"; };
```

This is a system-wide setting. VMs started after this change get 4 KB
pages that KSM can merge. VMs started before must be restarted.

---

## Decision 6 (Planned): Virtio-PMEM for Nix Store

### Context

The erofs-on-virtio-blk approach works but causes double caching (~100 MB
per VM). The virtio ecosystem offers a better primitive: **virtio-pmem**.

Virtio-pmem exposes a host file as persistent memory in the guest's
physical address space (PCI BAR). With erofs FSDAX (filesystem direct
access), the guest reads nix store files by directly accessing the host's
mmap'd pages via EPT translation — **no guest page cache, no data copies**.

### How It Works

```
Guest reads /nix/store/...-bash/bin/bash
    ↓
erofs translates file offset → pmem offset
    ↓
FSDAX maps pmem page directly (no page cache allocation)
    ↓
EPT translates guest-physical → host-virtual
    ↓
Host page cache for erofs image file (shared across all VMs)
```

All VMs using the same erofs image share the same host page cache pages.
This is natural deduplication without KSM — the host kernel manages it.

### Projected Impact

| Approach | Host cache | Guest cache | Daemon | Per-VM | Snapshot? |
|----------|-----------|-------------|--------|--------|-----------|
| virtiofs (original) | Yes | No | 176 MB | ~514 MB | No |
| virtio-blk + erofs (current) | Yes | Yes (+100 MB) | 0 | ~443 MB | Yes |
| **virtio-pmem + erofs FSDAX** | Yes (shared) | **No** (DAX) | 0 | **~330 MB?** | Yes* |

*Snapshot/restore with virtio-pmem needs verification on Node B.

### Cloud-Hypervisor Support

```bash
cloud-hypervisor --pmem file=/path/to/erofs-image,discard_writes=on
```

- `file=<path>`: backing file (must be raw, directly mmappable)
- `discard_writes=on`: read-only semantics (writes succeed but are not persisted)
- Supported since cloud-hypervisor v0.7.0 (early 2020)

### Prerequisites

- Guest kernel must have erofs + FSDAX support (standard in recent kernels)
- erofs image must be **uncompressed** for FSDAX (compressed erofs cannot DAX)
- microvm.nix may need configuration to use `--pmem` instead of `--disk`
  for the store disk (investigation needed)

### Risk

- Uncompressed erofs images are larger on disk (but the host page cache
  footprint is the same — only accessed pages are resident)
- FSDAX on erofs is relatively new (kernel 5.15+) — needs testing
- microvm.nix may not support virtio-pmem out of the box — may need
  manual `--pmem` flag injection in the cloud-hypervisor@ service

### Plan

Experiment on Node B (now that Node A is promoted and stable):
1. Check if microvm.nix supports virtio-pmem natively
2. If not, manually add `--pmem` to cloud-hypervisor@ ExecStart
3. Verify guest mounts erofs with FSDAX (`mount -o dax`)
4. Measure per-VM RSS reduction
5. Test snapshot/restore with virtio-pmem
6. If validated, update microvm config and deploy

---

## Capacity Projections (Updated with Measured Data)

| Metric | virtiofs (baseline) | erofs+KSM (current) | pmem+FSDAX (Phase 7) |
|--------|--------------------|-----------------------|----------------------|
| Per-VM memory | ~514 MB | ~443 MB | ~330 MB (projected) |
| KSM savings @ 58 VMs | 0 | 1.7 GB | TBD (less needed) |
| Max VMs (32 GB) | ~42 | ~58+ | ~80+ (projected) |
| Nix store copies in RAM | 1 (host cache) | 2 (host + guest) | 1 (host, shared) |

---

## Sources

- [KSM merges no pages with shared=on — cloud-hypervisor#5873](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/5873)
- [KSM and virtiofs incompatibility — kata-containers/runtime#2798](https://github.com/kata-containers/runtime/issues/2798)
- [Linux KSM documentation](https://www.kernel.org/doc/html/latest/admin-guide/mm/ksm.html)
- [virtiofs requires MAP_SHARED — virtio-fs/qemu#16](https://gitlab.com/virtio-fs/qemu/-/issues/16)
- [cloud-hypervisor memory docs](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/memory.md)
- [cloud-hypervisor device_model.md](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/device_model.md)
- [cloud-hypervisor virtiofs restore issue #6931](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6931)
- [cloud-hypervisor double memory with shared=off #4805](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/4805)
- [Linux EROFS documentation](https://docs.kernel.org/filesystems/erofs.html)
- [Linux DAX documentation](https://docs.kernel.org/filesystems/dax.html)
- [Virtio PMEM — LWN](https://lwn.net/Articles/776292/)
- [ChoirOS load test report (2026-03-10)](../state/reports/2026-03-10-adr-0018-load-test.md)
- [ChoirOS virtio ecosystem research](../virtio-ecosystem.md)
