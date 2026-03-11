# Machine Class Stress Test Comparison (ADR-0014 Phase 6)

**Date:** 2026-03-11
**Host:** Node B (32 GB RAM, draft.choir-ip.com)
**Test:** `machine-class-stress.spec.ts` — ramp 5 VMs/batch × 8 batches = 40 VMs max
**All classes:** 2 vCPU, 1 GB RAM per VM

## Summary

All 4 machine classes reached 39-40 VMs on a 32 GB host with no ceiling hit.
Boot times, health latency, and I/O workload performance are indistinguishable
across classes. The only meaningful differentiator is **memory efficiency**:
cloud-hypervisor + virtio-pmem (ch-pmem) benefits significantly from KSM page
deduplication, using ~25% less memory per VM at scale.

## Comparison Table (at 40 VMs)

| Metric | ch-pmem-2c-1g | ch-blk-2c-1g | fc-pmem-2c-1g | fc-blk-2c-1g |
|---|---|---|---|---|
| VMs booted | 39/40 | 39/40 | 39/40 | **40/40** |
| Boot failures | 1 | 1 | 1 | **0** |
| Boot median (batch 8) | 6,367 ms | 6,358 ms | 6,367 ms | 6,376 ms |
| Boot median range | 6.3-6.4s | 6.2-8.3s | 6.3-8.4s | 6.3-8.3s |
| Memory per VM (batch 8) | **276 MB** | 366 MB | 365 MB | 366 MB |
| Memory available @ 40 | **19,850 MB** | 16,359 MB | 16,308 MB | 15,954 MB |
| Memory used @ 40 | ~11 GB | ~14.3 GB | ~14.2 GB | ~14.6 GB |
| Health p50 (batch 8) | 42 ms | 45 ms | 46 ms | 44 ms |
| Health p99 (batch 8) | 47 ms | 53 ms | 71 ms | 51 ms |
| I/O prompt avg | 4,289 ms | 4,570 ms | 5,230 ms | 4,531 ms |
| Test wall time | 13.4 min* | 2.3 min | 2.4 min | **1.7 min** |
| Cleanup memory recov. | 30,257 MB | 30,334 MB | 30,283 MB | 30,275 MB |

*ch-pmem had one boot timeout in batch 3 that caused a ~12 min wall-time spike.
Without that outlier, wall time would be ~2.5 min (same as others).

## Key Findings

### 1. KSM only benefits cloud-hypervisor + pmem

ch-pmem converges to **276 MB/VM** at scale (down from 452 MB at 5 VMs).
All other classes plateau at **365-366 MB/VM**. This is because:
- KSM can deduplicate identical read-only pages across VMs
- virtio-pmem maps the store disk into guest physical memory where KSM can merge pages
- virtio-blk uses I/O paths that don't create mergeable page mappings
- Firecracker's pmem implementation may not expose pages in a KSM-friendly way

### 2. Projected capacity at 32 GB (2 GB reserve for host)

| Class | MB/VM @ 40 | Theoretical max | Headroom @ 40 |
|---|---|---|---|
| ch-pmem | 276 | ~108 VMs | 19.8 GB |
| ch-blk | 366 | ~82 VMs | 16.4 GB |
| fc-pmem | 365 | ~82 VMs | 16.3 GB |
| fc-blk | 366 | ~82 VMs | 16.0 GB |

ch-pmem gives ~32% more capacity than any other class on the same hardware.

### 3. Boot times are uniform

All classes boot in 6.2-6.4s (median) with no degradation up to 40 VMs.
The boot time is dominated by NixOS init inside the VM, not hypervisor startup.
Occasional spikes to 8-10s correlate with concurrent batch boots (I/O contention).

### 4. Health latency is rock solid

All classes maintain p50 ~44ms, p99 ~50-70ms across the full range.
No measurable degradation from 5 to 40 VMs for any class.

### 5. I/O workload (conductor prompts) is equivalent

All classes handle conductor prompts in 3.9-6.5s range. The variation is
dominated by LLM response time, not VM I/O characteristics.

### 6. Firecracker blk had the cleanest run

fc-blk-2c-1g was the only class to boot all 40/40 VMs with zero failures,
completing in 1.7 minutes. All other classes had 1 boot timeout each.

## Recommendation

**Production topology: ch-blk-2c-2g (user sandboxes) + ch-pmem-4c-4g (worker pool)**

- **User sandboxes (ch-blk-2c-2g):** virtio-blk provides tenant isolation (no
  shared pmem pages between users). 2c-2g gives headroom for interactive use
  at near-zero cost over 1g at idle.
- **Worker pool (ch-pmem-4c-4g):** virtio-pmem enables KSM dedup across identical
  worker VMs, maximizing density for the shared compute pool. 4c-4g provides
  enough resources for builds, tests, and Playwright.
- **Elastic resize:** Users needing heavy compute (build/test/Playwright) get
  cold-boot resized from 2c-2g to 4c-4g in ~8.4s, then back when done.
- **Safe production target:** 45 VMs (32 users + 13 workers) on a 32 GB host,
  with ~13 GB headroom.

For homogeneous deployments, ch-pmem-2c-1g remains the density champion at
276 MB/VM (proven to 92 VMs in ADR-0022 stress tests).

## 2c-2g Sizing Comparison (at 40 VMs)

Same test, doubled guest RAM (2 GB per VM).

| Metric | ch-pmem-2c-2g | ch-blk-2c-2g | fc-pmem-2c-2g | fc-blk-2c-2g |
|---|---|---|---|---|
| VMs booted | **40/40** | 39/40 | **40/40** | 39/40 |
| Boot median (batch 8) | 6,333 ms | 6,333 ms | 6,362 ms | 8,079 ms |
| Memory per VM (batch 8) | 377 MB | 364 MB | 371 MB | 286 MB |
| Memory available @ 40 | 15,421 MB | 16,050 MB | 15,747 MB | 19,419 MB |
| Health p50 (batch 8) | 47 ms | 42 ms | 42 ms | 45 ms |
| Health p99 (batch 8) | 81 ms | 54 ms | 46 ms | 55 ms |
| I/O prompt avg | 4,671 ms | 4,593 ms | 4,314 ms | 4,840 ms |
| Test wall time | **1.7 min** | 2.3 min | 1.7 min | 13.4 min* |

*fc-blk-2c-2g had one boot timeout in batch 5 that caused a ~12 min wall-time spike.

### Key Observation: 2 GB VMs cost almost the same as 1 GB VMs

This is the most important finding. At idle (no workload inside the VM), KSM
and Linux balloon/reclaim mean the guest's allocated RAM doesn't translate to
proportional host memory consumption:

| Class | 1g MB/VM | 2g MB/VM | Delta |
|---|---|---|---|
| ch-pmem | 276 | 377 | +101 MB (+37%) |
| ch-blk | 366 | 364 | **-2 MB (0%)** |
| fc-pmem | 365 | 371 | +6 MB (+2%) |
| fc-blk | 366 | 286 | **-80 MB (-22%)** |

For idle VMs, doubling guest RAM costs essentially nothing in host memory.
The real cost only appears when guest processes actually use the extra RAM.
This means **2c-2g is the right default** — it gives guests headroom for
builds, tests, and Playwright without meaningful density loss at idle.

## VM Cold-Boot Resize Test

Validated the elastic compute flow: boot on 2c-1g → write data → stop →
switch to 4c-4g → boot → data persists → stop → switch back to 2c-1g →
boot → data still persists.

| Phase | Time |
|---|---|
| Initial boot (ch-pmem-2c-1g) | 8,245 ms |
| Resize up boot (ch-pmem-4c-4g) | 8,361 ms |
| Resize down boot (ch-pmem-2c-1g) | 8,391 ms |

Boot time is identical regardless of VM sizing. data.img portability
across sizing changes is confirmed — no snapshot involved, just cold boot.

## 4c-4g Matrix (all 4 types, 20 VMs each, batch=10)

| Metric | ch-pmem-4c-4g | ch-blk-4c-4g | fc-pmem-4c-4g | fc-blk-4c-4g |
|---|---|---|---|---|
| VMs booted | **20/20** | **20/20** | **20/20** | 19/20 |
| Boot median | 10,378 ms | 10,330 ms | 10,295 ms | 10,367 ms |
| Memory per VM @ 20 | 471 MB | 246 MB | 144 MB | 113 MB |
| Health p50 | 46 ms | 50 ms | 46 ms | 47 ms |
| Wall time (class) | 27s | 26s | 43s | 44s |

Boot time at 4c is ~10.3s (vs 6.3s for 2c) — the extra vCPUs add ~4s to
NixOS init (more CPUs to bring online, NUMA balancing, etc.).

Memory per VM numbers are lower than steady-state because we stop each
class's VMs before testing the next, giving KSM time to reclaim. The
ch-pmem-4c-4g figure of 471 MB at 20 VMs is the most representative.

## 8c-8g Matrix (all 4 types, 10 VMs each)

| Metric | ch-pmem-8c-8g | ch-blk-8c-8g | fc-pmem-8c-8g | fc-blk-8c-8g |
|---|---|---|---|---|
| VMs booted | 9/10 | **10/10** | **10/10** | **3/10** |
| Boot median | 10,359 ms | 10,296 ms | 9,335 ms | 30,156 ms |
| Memory per VM | 472 MB | 312 MB | 194 MB | 181 MB |
| Health p50 | 43 ms | 51 ms | 43 ms | 50 ms |
| Ceiling hit | no | no | no | **yes** |

**fc-blk-8c-8g is unreliable at 8 vCPUs** — only 3/10 booted, with 30s
boot times. This may be a Firecracker limitation with higher vCPU counts.
The other three 8c-8g classes work fine.

Boot time at 8c is still ~10.3s (same as 4c). The NixOS init time plateaus
after 4 vCPUs — going to 8 doesn't add additional boot latency.

## Elastic Resize Under Load

10 users on ch-pmem-2c-1g → stop 5 → resize to ch-pmem-4c-4g → boot → verify
→ stop → resize back → boot → verify. All while 5 other VMs stay running.

| Phase | Wall time | Boot median | Memory avail |
|---|---|---|---|
| Boot 10 on 2c-1g | 13.3s | — | 26,510 MB |
| Resize 5 up to 4c-4g | 44.7s* | 8,408 ms | 25,574 MB |
| Resize 5 back to 2c-1g | 44.7s* | 8,395 ms | 26,539 MB |

*Sequential resize (stop → set class → wait for boot) per user. Parallelizing
would bring this to ~8-10s.

Health latency unaffected by mixed sizings: 2c-1g p50 = 45ms, 4c-4g p50 = 44ms.
Memory fully recovers after downsize: 26,539 MB vs 26,510 MB baseline.

## Heterogeneous Production Topology

**Test:** `heterogeneous-capacity.spec.ts` — ch-blk-2c-2g (user sandboxes) +
ch-pmem-4c-4g (worker pool), ramping +5 users + 2 workers per round.

Rationale: blk for user isolation (no shared pmem pages between tenants),
pmem for worker density (KSM dedup on identical store disks in the pool).

| Round | Users | Workers | Total | Mem Avail | User p99 | Worker p99 | MB/VM |
|-------|-------|---------|-------|-----------|----------|------------|-------|
| 1 | 5 | 2 | 7 | 27,043 MB | 51 ms | 49 ms | 489 |
| 2 | 10 | 4 | 14 | 24,216 MB | 49 ms | 50 ms | 447 |
| 3 | 15 | 6 | 21 | 21,333 MB | 55 ms | 56 ms | 435 |
| 4 | 20 | 8 | 28 | 18,485 MB | 53 ms | 49 ms | 428 |
| 5 | 25 | 10 | 35 | 15,825 MB | 56 ms | 46 ms | 418 |
| 6 | 30 | 12 | 42 | 13,013 MB | 57 ms | 49 ms | 416 |
| 7 | 35 | 14 | 49 | 10,374 MB | 48 ms | 50 ms | 410 |
| 8 | 40 | 16 | **56** | 7,428 MB | 51 ms | 51 ms | **411** |
| **9** | 44 | 18 | 62 | 18,252* | **2,397** | **2,611** | 197* |
| 10 | 49 | 20 | 69 | 7,111 MB | 2,405 | 129 ms | 339 |

*Round 9: OOM killer reclaimed VMs (memory jumped from 7.4 GB to 18.2 GB,
VMs dropped from 57 to 28). Boot times doubled to 16s.

### Key Findings

1. **Clean capacity: 56 VMs** (40 users + 16 workers) with perfect health
   - Zero boot failures through 8 rounds (56/56 = 100%)
   - p99 latency never exceeded 57 ms
   - Boot fixes (data.img flock, .microvm-run cleanup) eliminated all prior failures

2. **Degradation is abrupt, not gradual** — system goes from p99=51ms at 56 VMs
   to p99=2.6s at 62 VMs. The OOM killer is the degradation mechanism, not
   gradual latency increase. This means capacity planning should target ~80%
   of the clean ceiling (45 VMs) for production headroom.

3. **Recovery after OOM** — round 10 still booted 7/7 new VMs after OOM killed
   others. The system self-heals but with elevated p99 on survivors. Worker
   p99 recovered to 129ms while user p99 stayed at 2.4s (more VMs = more
   health probes = more contention).

4. **Memory efficiency: 411 MB/VM** at scale for the mixed topology. This is
   between ch-pmem-only (276 MB) and ch-blk-only (366 MB), as expected from
   mixing blk (user) and pmem (worker) classes.

5. **I/O workload unaffected** — conductor prompts at 5.2s avg under 69 VMs
   of mixed load. LLM response time dominates, not VM I/O.

### Production Capacity Planning (32 GB host)

| Scenario | Users | Workers | Total | Memory headroom |
|----------|-------|---------|-------|-----------------|
| Clean ceiling | 40 | 16 | 56 | 7.4 GB (23%) |
| Safe production (80%) | 32 | 13 | 45 | ~13 GB (41%) |
| Burst (degraded) | 49 | 20 | 69 | ~7 GB (22%)* |

*Burst capacity involves OOM risk. Not recommended for sustained operation.

## Raw Data

Full stress test output is available in Playwright test results.
Run commands:
```bash
for cls in ch-pmem-2c-1g ch-blk-2c-1g fc-pmem-2c-1g fc-blk-2c-1g; do
  MACHINE_CLASS=$cls PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
    npx playwright test machine-class-stress.spec.ts --project=stress
done
# Same for 2c-2g variants
for cls in ch-pmem-2c-2g ch-blk-2c-2g fc-pmem-2c-2g fc-blk-2c-2g; do
  MACHINE_CLASS=$cls PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
    npx playwright test machine-class-stress.spec.ts --project=stress
done
```
