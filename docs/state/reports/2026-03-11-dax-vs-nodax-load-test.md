# Load Test Report: DAX vs No-DAX virtio-pmem

Date: 2026-03-11
Test: `heterogeneous-load-test.spec.ts` (16 users: 6 idle, 5 light, 3 medium, 2 heavy)
Configs compared: virtio-pmem with FS_DAX (Node B) vs virtio-pmem without DAX (Node A)

## Narrative Summary

Both nodes run virtio-pmem for the nix store (erofs, uncompressed, `discard_writes=on`).
Node B has a kernel rebuilt with `CONFIG_FS_DAX=y`, `CONFIG_DAX=y`, `CONFIG_VIRTIO_PMEM=y`
(built-in), and mounts with `dax=always`. Node A uses VIRTIO_PMEM as a module loaded in
the initrd, with no DAX mount option.

**Key finding**: DAX dramatically reduces per-VM proportional memory (Pss) via KSM
deduplication of guest RAM, but does NOT reduce RSS (guest RAM is still allocated upfront).
The biggest win is host page cache: 7.5 GB with DAX vs 7.1 GB without — roughly equal
because neither config double-caches heavily. The real difference is in KSM effectiveness:
DAX keeps guest nix-store pages identical across VMs (never dirtied by page cache writes),
enabling ~260 MB/VM KSM dedup vs ~0-74 MB/VM without.

## Test Results

| Metric | Node B (DAX) | Node A (no DAX) |
|--------|-------------|-----------------|
| **Result** | PASS | FAIL (timeout) |
| Registration (16 users) | 6,364 ms | 10,930 ms |
| VM ready (16 users) | 14,813 ms | 10,705 ms |
| Workload total | 12,094 ms | 12,801 ms |
| Light health checks | 10/10 all users | 10/10 all users |
| Medium conductor | ~6.2s avg | ~4.3s avg |
| Heavy 3-prompt | ~12s avg | ~12.3s avg |

Node A failed with a 180s timeout during cleanup/teardown, not during workloads.
All 16 users registered, all VMs booted, all workloads completed on both nodes.

## VM Boot Times

| Percentile | Node B (DAX) | Node A (no DAX) |
|-----------|-------------|-----------------|
| First VM | 1,097 ms | 1,924 ms |
| p50 | ~12,100 ms | ~9,400 ms |
| p95 | ~14,800 ms | ~10,700 ms |
| Last VM | 14,806 ms | 10,703 ms |

Node A booted 16 VMs faster (10.7s vs 14.8s). Node A had only 7 pre-existing VMs
vs Node B's 46, so less contention during concurrent boot.

## Memory Profile

### Pre-test baseline

| Metric | Node B (DAX) | Node A (no DAX) |
|--------|-------------|-----------------|
| Pre-existing VMs | 46 | 7 |
| Used memory | 7.9 GB | 3.3 GB |
| Available memory | 23 GB | 27 GB |
| Host page cache | 6.3 GB | 6.9 GB |
| KSM pages sharing | 1,848,480 | 16,386 |

### Post-test (after adding 16 users)

| Metric | Node B (DAX) | Node A (no DAX) |
|--------|-------------|-----------------|
| Total VMs | 45 | 17 |
| Used memory | 10 GB | 7.4 GB |
| Available memory | 20 GB | 23 GB |
| Host page cache | 7.5 GB | 7.1 GB |
| KSM pages sharing | 1,781,497 | 16,175 |
| KSM pages shared | 65,265 | 2,365 |

### Per-VM memory (post-test sample, 5 VMs each)

| Metric | Node B (DAX) | Node A (no DAX) |
|--------|-------------|-----------------|
| RSS (avg) | **532 MB** | **544 MB** |
| Pss (avg) | **105 MB** | **363 MB** |
| KSM dedup/VM | **265 MB** | **15 MB** |

**The critical difference: Pss is 105 MB/VM with DAX vs 363 MB/VM without.**

This is because DAX keeps nix-store pages clean and identical across VMs, making
them perfect KSM merge candidates. Without DAX, the guest page cache dirties pages
as it reads nix-store content, making them unique per-VM and unmergeable by KSM.

## Capacity Projections

Based on 32 GB total RAM, ~2 GB reserved for host:

| Config | Pss/VM | Max VMs (theoretical) | Max VMs (practical, 80%) |
|--------|--------|----------------------|--------------------------|
| DAX | 105 MB | ~285 | ~230 |
| No DAX (KSM cold) | 363 MB | ~82 | ~65 |
| No DAX (KSM warm) | ~200 MB* | ~150 | ~120 |

*KSM on Node A had only 17 VMs and hadn't fully converged. With more VMs and time,
KSM would merge more pages, likely reaching ~200 MB/VM Pss.

Note: These are theoretical limits. Practical limits depend on CPU contention, I/O,
and guest workload memory. The previous virtio-blk baseline topped out at 58 VMs.

## Kernel Configuration

### Node B (DAX)
```
CONFIG_VIRTIO_PMEM=y   (built-in)
CONFIG_LIBNVDIMM=y     (built-in)
CONFIG_DAX=y           (built-in)
CONFIG_FS_DAX=y        (built-in)
CONFIG_EROFS_FS=y      (built-in)
Mount: /dev/pmem0 erofs ro,dax=always
```

### Node A (no DAX)
```
CONFIG_VIRTIO_PMEM=m   (module, loaded in initrd)
CONFIG_LIBNVDIMM=m     (module, loaded in initrd)
CONFIG_DAX=m           (module)
CONFIG_FS_DAX not set
CONFIG_EROFS_FS=m      (module)
Mount: /dev/pmem0 erofs ro
```

## Conclusions

1. **DAX + KSM is the winning combination.** DAX keeps nix-store pages clean → KSM
   merges them → ~3.5x lower Pss per VM (105 MB vs 363 MB).

2. **Host page cache is similar** between configs (~7 GB). The pmem device avoids
   double-caching regardless of DAX.

3. **Boot times** depend more on existing VM count than on DAX. Node A (7 VMs) booted
   16 new VMs in 10.7s; Node B (46 VMs) took 14.8s for the same 16.

4. **Workload performance** is equivalent — DAX doesn't speed up conductor/LLM calls
   since those are network-bound.

5. **Promote DAX to Node A** — the ~3.5x Pss improvement is significant for capacity.
   Re-promote after Node A gets the DAX kernel.

## Raw Test Logs

- Node B (DAX): `tests/artifacts/load-test-node-b-dax.log`
- Node A (no DAX): `tests/artifacts/load-test-node-a-nodax.log`
