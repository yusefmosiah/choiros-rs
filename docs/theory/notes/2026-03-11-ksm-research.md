# KSM (Kernel Same-page Merging) Research for MicroVM Workloads

Date: 2026-03-11
Kind: Research Note
Status: Complete
Authors: wiz + Claude

## Narrative Summary (1-minute read)

KSM is a Linux kernel feature that deduplicates identical memory pages across
processes. A kernel thread (ksmd) scans registered anonymous pages, builds a
red-black tree of content hashes, and replaces duplicates with a single
copy-on-write (CoW) page. For our microVM workload (cloud-hypervisor with
virtio-pmem store disks), KSM is actively merging pages and saving real memory.

On Node B right now (1 live VM, kernel 6.18.16): KSM reports 44,357 shared
pages and 16,813 sharing pages, with a general profit of ~60 MB. The live
sandbox VM process shows 61,099 merging pages and ~239 MB of KSM-deduplicated
memory (from `smaps_rollup`). ksmd is consuming ~1.6% CPU averaged over 5.6
hours of uptime.

The key architectural insight: cloud-hypervisor maps guest RAM as MAP_PRIVATE
anonymous pages with `mergeable=on` (MADV_MERGEABLE). When the guest boots
NixOS, it reads the erofs store disk into guest page cache. Since every VM
boots the same NixOS closure, these guest page cache pages are identical
across VMs and KSM merges them. With virtio-pmem (our current config), the
store disk is mapped directly into guest physical address space as pmem pages
-- these are *also* anonymous MAP_PRIVATE pages on the host side (because
cloud-hypervisor uses `discard_writes=on`), and KSM can merge them.

The real savings scale with VM count. At 50+ VMs sharing the same NixOS store
closure (~500 MB), the theoretical deduplication ceiling is substantial. Our
ADR-0018 load tests measured 1.7 GB saved at 58 concurrent VMs.

## How KSM Works (Technical Detail)

### Kernel Architecture

KSM is implemented in `mm/ksm.c`. It maintains two red-black trees:

- **Stable tree**: Contains pages that have been successfully merged. These are
  write-protected CoW pages shared by two or more processes. Lookups are by
  page content (memcmp). Pages in this tree are considered "stable" because
  their content cannot change without triggering a CoW fault.

- **Unstable tree**: Contains candidate pages that have been scanned but not yet
  matched. These pages could change at any time (they are not write-protected),
  so the tree is periodically invalidated and rebuilt. Each full scan cycle
  rebuilds the unstable tree from scratch.

### Scanning Process

1. ksmd wakes every `sleep_millisecs` (our config: 200ms).
2. It scans `pages_to_scan` pages per wake cycle (our config: 1000).
3. For each scanned page, it computes a checksum and searches the stable tree.
4. If found in stable tree: merge (increment sharing count, replace PTE with
   CoW mapping to the stable page, free the duplicate physical page).
5. If not in stable tree: search the unstable tree.
6. If found in unstable tree: both pages match, so merge them into a new
   stable tree entry. Both processes get CoW PTEs to the new shared page.
7. If not found anywhere: insert into unstable tree as a candidate.

### Copy-on-Write Behavior

When a process writes to a KSM-merged page, the hardware triggers a page
fault (write to read-only page). The kernel's CoW handler allocates a new
physical page, copies the content, and updates the faulting process's PTE
to point to the new private copy. The shared page's reference count
decrements. This is tracked in `/proc/vmstat` as `cow_ksm` (our node:
28,087 CoW events since boot).

### Memory Accounting

- `pages_shared`: Number of unique page contents in the stable tree (44,357).
  Each represents one physical page backing potentially many virtual mappings.
- `pages_sharing`: Number of virtual pages that are mapped to shared pages
  beyond the first reference (16,813). This is the actual savings count.
- `pages_unshared`: Pages scanned but no match found (4). Low means most
  scanned pages find matches -- excellent for our workload.
- `pages_volatile`: Pages that changed before they could be merged (25,452).
  These are pages that were scanned, added to the unstable tree, but modified
  before the next scan found a match.
- `full_scans`: Number of complete passes over all registered pages (1,352).

**Memory saved** = `pages_sharing` * page_size = 16,813 * 4096 = ~65 MB.
But `general_profit` reports ~60 MB, which accounts for KSM's own metadata
overhead (rmap_items at 64 bytes each on x86_64).

The per-process `ksm_merging_pages` for the live VM (61,099 pages = ~239 MB)
is higher than the system-wide `pages_sharing` because `pages_sharing` only
counts the *extra* references beyond the first. With only one VM running,
most of those 61,099 pages are merged with themselves (same page appearing
at multiple virtual addresses within the same VM) or are in the stable tree
waiting for future VMs to share against.

### MADV_MERGEABLE Requirement

KSM only scans pages that have been explicitly registered via
`madvise(addr, len, MADV_MERGEABLE)`. This is an opt-in mechanism.
Applications (or VMMs) must mark memory regions as candidates. Without this
call, ksmd ignores the pages entirely.

There is also `prctl(PR_SET_MEMORY_MERGE, 1)` which marks *all* anonymous
memory of a process as mergeable. Cloud-hypervisor does not use this; it uses
the targeted `MADV_MERGEABLE` approach on guest RAM regions.

## Our Current Configuration (Node B, 2026-03-11)

### KSM Tunables

| Tunable | Value | Default | Notes |
|---------|-------|---------|-------|
| `run` | 1 | 0 | Active (1=run, 0=stop, 2=unmerge-all) |
| `sleep_millisecs` | 200 | 20 | Wake interval. 10x less aggressive than default |
| `pages_to_scan` | 1000 | 100 | 10x more pages per wake than default |
| `merge_across_nodes` | 1 | 1 | NUMA cross-node merging (single-socket, irrelevant) |
| `max_page_sharing` | 256 | 256 | Max VMs sharing one stable page |
| `stable_node_chains_prune_millisecs` | 2000 | 2000 | Stale chain cleanup interval |
| `use_zero_pages` | 0 | 0 | Not merging zero pages with kernel zero page |
| `smart_scan` | 1 | 1 | Skips pages that failed to merge previously |
| `advisor_mode` | none | none | No automatic tuning (we set static values) |

### How These Are Set

NixOS host config (`nix/hosts/ovh-node.nix`) uses systemd-tmpfiles:

    systemd.tmpfiles.settings."10-ksm" = {
      "/sys/kernel/mm/ksm/run".w = { argument = "1"; };
      "/sys/kernel/mm/ksm/sleep_millisecs".w = { argument = "200"; };
      "/sys/kernel/mm/ksm/pages_to_scan".w = { argument = "1000"; };
    };

### THP (Transparent Huge Pages)

    $ cat /sys/kernel/mm/transparent_hugepage/enabled
    always madvise [never]

THP is disabled (`never`). This is set in the NixOS config because
cloud-hypervisor defaults to calling `MADV_HUGEPAGE` on guest memory, which
would cause the kernel to back guest RAM with 2MB huge pages. KSM operates
on 4KB base pages only -- it cannot scan or merge huge pages. Disabling THP
ensures all guest memory stays as 4KB pages eligible for KSM merging.

### Live Metrics (1 VM running)

| Metric | Value | Meaning |
|--------|-------|---------|
| `pages_shared` | 44,357 | Unique deduplicated page contents |
| `pages_sharing` | 16,813 | Extra references (savings) |
| `pages_unshared` | 4 | Scanned but no match |
| `pages_volatile` | 25,452 | Changed before merge |
| `full_scans` | 1,352 | Complete scan passes |
| `general_profit` | ~60 MB | Net memory savings |
| `cow_ksm` (vmstat) | 28,087 | CoW faults on merged pages |
| VM `ksm_merging_pages` | 61,099 | Pages merged for live VM |
| VM KSM in smaps_rollup | ~239 MB | Per-process KSM memory |
| ksmd CPU | ~1.6% | Averaged over 5.6h uptime |

### cloud-hypervisor Memory Flags

From the generated `.microvm-run` for the live VM:

    --memory 'mergeable=on,shared=off,size=1024M'
    --pmem 'file=/nix/store/...-store-disk-pmem-aligned,discard_writes=on'

- `mergeable=on`: cloud-hypervisor calls `madvise(MADV_MERGEABLE)` on guest
  RAM regions. This is implemented in `vmm/src/memory_manager.rs` in the
  `create_userspace_mapping()` method.
- `shared=off`: Guest RAM is mmap'd with MAP_PRIVATE (not MAP_SHARED). This
  is required for KSM -- the kernel only merges MAP_PRIVATE anonymous pages.
- `discard_writes=on`: The pmem backing file is mapped MAP_PRIVATE, meaning
  guest writes create private CoW copies rather than modifying the host file.
  This is what makes pmem pages eligible for KSM.

### What MADV_MERGEABLE Covers

Based on cloud-hypervisor source analysis (`vmm/src/memory_manager.rs`):

- Guest RAM regions: YES. `create_userspace_mapping()` applies
  `MADV_MERGEABLE` when `mergeable=on` is set.
- Pmem regions: NOT DIRECTLY. The pmem file is mmap'd separately by the
  device manager, not through `create_userspace_mapping()`. However, pmem
  with `discard_writes=on` uses MAP_PRIVATE, creating anonymous (CoW) pages
  on write. These anonymous pages *may* be picked up by KSM if the guest
  touches them and they become anonymous.

The practical implication: guest RAM pages that cache the erofs store content
(read into guest page cache from the pmem device) ARE anonymous pages within
the guest RAM region and ARE marked MADV_MERGEABLE. This is the primary
deduplication path.

## Why virtio-pmem Benefits from KSM

### The Memory Path

With virtio-pmem + `discard_writes=on`:

1. The host mmap's the erofs file as MAP_PRIVATE into the guest's physical
   address space (the pmem BAR region).
2. The guest kernel sees a `/dev/pmem0` device and mounts the erofs filesystem
   with DAX (direct access). DAX means the guest does NOT use its page cache
   for reads from pmem -- it reads directly from the mapped pages.
3. These mapped pages are backed by the same host file across all VMs. Since
   the mapping is MAP_PRIVATE, each VM gets its own set of PTEs, but the
   underlying physical pages are initially shared (same file offset = same
   host page cache page).
4. If a guest writes to a pmem page (with discard_writes=on), the host
   creates a private anonymous copy (CoW). This anonymous page is then
   eligible for KSM if marked.

### Comparison with virtio-blk (Previous Config)

With virtio-blk, the guest *must* use its page cache to read from the block
device. This creates ~100 MB of duplicate page cache inside each guest (the
NixOS store content cached in guest RAM). These guest page cache pages live
in the guest's anonymous RAM region (which IS marked MADV_MERGEABLE), so
KSM can merge identical pages across VMs. But you pay the 100 MB per-VM
upfront before KSM can claw it back.

With virtio-pmem + DAX, the guest skips its page cache entirely. The store
content is accessed directly through the pmem mapping. This eliminates the
100 MB guest page cache overhead. KSM still works on the guest RAM region
for other content (runtime heap, tmpfs, etc.).

### Net Effect

virtio-pmem reduces per-VM memory by ~100 MB (no guest page cache) while
KSM continues to deduplicate the remaining guest RAM. The store content
itself, accessed through DAX, is inherently shared at the host page cache
level (all VMs mmap the same file) without needing KSM at all.

This is the ideal combination: structural sharing (pmem DAX) for the store
disk plus KSM deduplication for guest RAM.

## Tuning Recommendations

### Current Scan Rate Analysis

With our config (pages_to_scan=1000, sleep_millisecs=200):

- Scan rate: 1000 pages / 200ms = 5,000 pages/second = ~20 MB/s
- At 50 VMs with 1024 MB RAM each: ~50 GB of scannable memory
- But only MADV_MERGEABLE regions are scanned, so effective scan area is
  the guest RAM regions (~50 * 1024 MB = ~50 GB worst case)
- Full scan time: 50 GB / 20 MB/s = ~2,500 seconds (~42 minutes)
- This means newly booted VMs take up to 42 minutes to fully merge

### Recommendations for 50-100 VM Workload

**1. Increase `pages_to_scan` to 4000-8000**

At 50+ VMs, the scan area grows proportionally. To maintain a reasonable
convergence time (under 10 minutes for a new VM's pages to be fully scanned):

    # Target: full scan in ~10 minutes at 50 VMs
    # 50 VMs * 262,144 pages/VM (1 GB) = 13,107,200 scannable pages
    # 13,107,200 pages / 600 seconds / (1000ms/200ms) = ~4,369 pages_to_scan
    echo 4000 > /sys/kernel/mm/ksm/pages_to_scan

For 100 VMs, consider 8000. Monitor ksmd CPU -- at 4000 pages and 200ms
sleep, expect ~3-5% CPU on a modern Xeon.

**2. Keep `sleep_millisecs` at 200**

The default (20ms) is too aggressive for a production host. 200ms is a good
balance. Lowering to 100ms would double scan throughput but also double CPU
cost. Prefer increasing `pages_to_scan` over decreasing `sleep_millisecs`
because larger batches per wake are more cache-friendly.

**3. Enable `use_zero_pages`**

    echo 1 > /sys/kernel/mm/ksm/use_zero_pages

Guest VMs have large zero-filled regions (unallocated heap, zeroed BSS).
With `use_zero_pages=1`, KSM maps these to the kernel's shared zero page
instead of allocating a separate stable tree entry. This saves both memory
and stable tree traversal time. There is no downside for our workload.

**4. Consider `advisor_mode=scan-time`**

The kernel's built-in KSM advisor (available since ~6.7) automatically
adjusts `pages_to_scan` based on scan duration and CPU targets:

    echo scan-time > /sys/kernel/mm/ksm/advisor_mode
    echo 70 > /sys/kernel/mm/ksm/advisor_max_cpu
    echo 200 > /sys/kernel/mm/ksm/advisor_target_scan_time
    echo 500 > /sys/kernel/mm/ksm/advisor_min_pages_to_scan
    echo 30000 > /sys/kernel/mm/ksm/advisor_max_pages_to_scan

This auto-tunes as VM count changes. Worth evaluating, but our static config
works well enough for now.

**5. `max_page_sharing` at 256 is fine**

This limits how many PTEs can reference a single stable page. At 256, we
can support 256 VMs sharing one page before a chain split. Our target is
50-100 VMs, well within this limit. Higher values slow down certain kernel
operations (compaction, NUMA balancing, swapping) because the rmap chain is
longer.

**6. `stable_node_chains_prune_millisecs` at 2000 is fine**

This controls how often the kernel checks for stale entries in chained
stable nodes. The default of 2000ms is appropriate. Only relevant if
`max_page_sharing` is hit and chains form.

**7. `merge_across_nodes` does not matter**

Our hosts are single-socket (no NUMA). This tunable only matters on
multi-socket systems where cross-NUMA merging would introduce memory
access latency. Leave at 1.

### Proposed NixOS Config Change

```nix
systemd.tmpfiles.settings."10-ksm" = {
  "/sys/kernel/mm/ksm/run".w = { argument = "1"; };
  "/sys/kernel/mm/ksm/sleep_millisecs".w = { argument = "200"; };
  "/sys/kernel/mm/ksm/pages_to_scan".w = { argument = "4000"; };
  "/sys/kernel/mm/ksm/use_zero_pages".w = { argument = "1"; };
};
```

## Risks and Gotchas

### 1. CoW Amplification Under Write-Heavy Workloads

When a guest writes to a KSM-merged page, the kernel must allocate a new
page, copy 4KB, and update PTEs. If many VMs simultaneously write to
previously-merged pages (e.g., during a `nixos-rebuild` or heavy compilation
inside the VM), CoW faults spike. Each fault takes ~2-4 microseconds, but
thousands of concurrent faults can cause measurable latency.

Current `cow_ksm` count: 28,087 over 5.6 hours with 1 VM. This is low and
healthy. Monitor this under load.

### 2. ksmd CPU Scaling

ksmd is single-threaded. At very high VM counts (100+), the scan area grows
and ksmd may struggle to keep up without high `pages_to_scan` values. CPU
cost scales linearly with scan area. At 100 VMs with pages_to_scan=8000,
expect 5-10% of one CPU core.

If ksmd CPU becomes problematic, the advisor mode can cap it automatically.

### 3. Memory Pressure Interaction

When the host is under memory pressure, the kernel may need to break KSM
merges to swap pages or reclaim memory. Unmerging a page costs a page
allocation + copy, which is the opposite of what you want under pressure.
However, KSM net reduces memory usage, so it generally *prevents* pressure.

### 4. MADV_MERGEABLE Is NOT Applied to pmem Regions

Cloud-hypervisor only calls MADV_MERGEABLE on guest RAM (via
`create_userspace_mapping()`), not on the pmem mmap. The pmem region is
mmap'd by the device manager separately. This means:

- The pmem mapping itself is not scanned by KSM.
- But with DAX, the guest accesses pmem content directly without copying it
  into guest RAM, so there is nothing to deduplicate there anyway.
- Guest RAM (heap, tmpfs, non-DAX page cache) IS covered by KSM.

This is actually the correct behavior. The pmem file is already structurally
shared (one host file, many MAP_PRIVATE mappings). KSM is not needed for
the read path. CoW pages from guest writes to pmem are private and transient
(discarded on VM shutdown).

### 5. THP=never Is Correct and Required

Cloud-hypervisor calls `MADV_HUGEPAGE` (via the `thp=on` default in its
memory config) to request transparent huge page backing for guest RAM. If
THP were enabled (`always` or `madvise`), the kernel would back guest RAM
with 2MB huge pages. KSM cannot scan or merge huge pages -- it only works
on base 4KB pages.

Setting THP=`never` at the host level overrides cloud-hypervisor's
MADV_HUGEPAGE hint. This ensures all guest RAM remains as 4KB pages
eligible for KSM.

The tradeoff: THP reduces TLB misses for the guest (one TLB entry covers
2MB instead of 4KB). Disabling THP slightly increases TLB pressure. For our
workload (many small VMs with identical content), the memory savings from KSM
far outweigh the TLB performance cost. At 50 VMs, KSM can save gigabytes;
THP would save microseconds per memory access.

### 6. Firecracker Differences

Firecracker does NOT have a `mergeable=on` option. To use KSM with
Firecracker, you would need to either:

- Use `prctl(PR_SET_MEMORY_MERGE, 1)` on the Firecracker process (requires
  patching Firecracker or using a wrapper).
- Use `madvise(MADV_MERGEABLE)` on the guest memory region (requires
  Firecracker source modification).

Firecracker's memory model uses a single mmap for guest RAM. Without explicit
MADV_MERGEABLE, ksmd will not scan Firecracker VM pages. This is a
significant difference from cloud-hypervisor, which exposes `mergeable=on`
as a first-class configuration option.

If we move to Firecracker (ADR-0023 context), KSM support would need to be
added upstream or worked around.

### 7. Security Consideration

KSM merging can theoretically leak information across VMs via timing
side-channels. A malicious guest could detect whether a specific page content
exists in another VM by measuring write latency (CoW vs no-CoW). This is a
known theoretical attack vector. In practice:

- Our VMs run trusted code (our own sandbox binary).
- The merged content is NixOS store paths (public, not secret).
- KSM timing attacks require precise measurement and are impractical at scale.

For multi-tenant hostile environments, KSM should be disabled. For our
single-tenant microVM platform, it is safe.

### 8. Kernel Version Matters

Our kernel (6.18.16) has all modern KSM features: smart_scan, advisor_mode,
per-process ksm_stat, general_profit tracking. Older kernels (<5.x) lack
some of these. No concerns here.

## What To Do Next

1. **Increase `pages_to_scan` to 4000** in `nix/hosts/ovh-node.nix`. This
   reduces merge convergence time from ~42 minutes to ~10 minutes at 50 VMs.
   Monitor ksmd CPU after deploying.

2. **Enable `use_zero_pages`**. No downside, saves memory on zero-filled
   guest pages.

3. **Run a load test at 50 VMs** and capture:
   - `pages_shared`, `pages_sharing`, `general_profit` (savings)
   - ksmd CPU usage (via `top` or `pidstat`)
   - `cow_ksm` delta during test (write amplification)
   - Per-VM `ksm_merging_pages` from `/proc/<pid>/ksm_stat`

4. **Evaluate `advisor_mode=scan-time`** as an alternative to static tuning.
   This would auto-adapt as VM count fluctuates.

5. **If pursuing Firecracker (ADR-0023)**: investigate upstream MADV_MERGEABLE
   support or `prctl(PR_SET_MEMORY_MERGE)` wrapper. Without this, Firecracker
   VMs will not benefit from KSM at all.

6. **Document the pmem+KSM interaction** in ADR-0018. The current ADR mentions
   KSM but does not explain the nuance that pmem DAX provides structural
   sharing while KSM handles guest RAM deduplication. These are complementary,
   not redundant.
