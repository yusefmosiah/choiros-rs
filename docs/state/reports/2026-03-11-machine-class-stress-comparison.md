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

**Default class: ch-pmem-2c-1g** — the 25% memory savings compound at scale,
and the previous ADR-0022 stress tests proved it clean to 92 VMs. The KSM
deduplication is a significant advantage for multi-tenant density.

For users who need Firecracker (e.g., for snapshot/restore speed or security
profile), fc-blk-2c-1g is the most reliable alternative.

## Raw Data

Full stress test output is available in Playwright test results.
Run commands:
```bash
for cls in ch-pmem-2c-1g ch-blk-2c-1g fc-pmem-2c-1g fc-blk-2c-1g; do
  MACHINE_CLASS=$cls PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
    npx playwright test machine-class-stress.spec.ts --project=stress
done
```
