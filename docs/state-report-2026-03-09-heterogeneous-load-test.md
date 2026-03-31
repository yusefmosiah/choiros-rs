# Heterogeneous Per-User VM Load Test Report

Date: 2026-03-09
Node: B (staging) — 147.135.70.196 (draft.choir-ip.com)
Hardware: 12 cores, 32GB RAM, 2×512GB NVMe (btrfs RAID1)

## Narrative Summary

This test ran 10 simultaneous users with 4 different workload profiles
against per-user VMs (ADR-0014 Phase 4) to measure behavior under
realistic heterogeneous traffic. VM memory was reduced from 3072 MB to
1024 MB for this test. KSM was enabled but did not activate (see below).

## Test Configuration

| Parameter | Value |
|-----------|-------|
| Configured VM memory | 1024 MB (reduced from 3072) |
| Idle users | 3 (register, sit idle) |
| Light users | 3 (health checks, heartbeats, auth) |
| Medium users | 2 (single conductor prompt) |
| Heavy users | 2 (3 conductor prompts + burst API) |
| Total new users | 10 |
| Pre-existing VMs | 32 |
| Peak VMs during test | 42 |

## Results: 100% Pass Rate

All 10 users registered, got VMs, and completed their workloads without errors.

### Phase 1: Registration (3.5s)

10 concurrent WebAuthn registrations completed in 3,506ms. No failures.

### Phase 2: VM Boot (20.5s)

All 10 VMs became healthy in ~20s (consistent with cold boot from
previous tests: ~12s boot + ~8s health poll interval). VMs boot in
parallel — the 20s is wall time for the slowest VM, not cumulative.

### Phase 3: Workload Execution (13.7s)

| Profile | Users | Duration | Details |
|---------|-------|----------|---------|
| Idle | 3 | 5.0s | No requests after boot (5s wait) |
| Light | 3 | 1.2–1.5s | 10 health (100%), 5 heartbeat (100%), 5 auth (100%) |
| Medium | 2 | 5.3–5.7s | 1 conductor prompt each (HTTP 202, ~5.5s) |
| Heavy | 2 | 12.8–13.7s | 3 conductor prompts + API burst each |

Heavy users ran 3 sequential conductor prompts (5.3s + 3.5s + 4.0s avg)
with a concurrent API burst between prompts 2 and 3. All returned
HTTP 202 (accepted). The burst (health + heartbeat + auth + runs check)
completed in 57-65ms, confirming the VM remains responsive during
conductor execution.

### Conductor Latency

| Metric | Value |
|--------|-------|
| Medium prompt (single) | 5.3–5.7s |
| Heavy prompt 1 (primes) | 5.3–5.9s |
| Heavy prompt 2 (creative) | 3.4–3.5s |
| Heavy prompt 3 (analysis) | 4.0–4.1s |
| All prompts status | HTTP 202 |

Conductor response times are dominated by LLM latency (routed through
the provider gateway to Bedrock). Sub-6s for simple prompts, sub-4s for
follow-up prompts in the same session.

## Memory Analysis

### With 1024 MB Configured (down from 3072)

| Metric | Value |
|--------|-------|
| VMs running | 42 |
| cloud-hypervisor RSS per VM | 330–350 MB (avg 338 MB) |
| virtiofsd instances | 172 (4 per VM) |
| virtiofsd RSS per VM | ~176 MB (42–181 MB per instance) |
| Total per-VM footprint | ~514 MB |
| System memory used | 17.0 GB |
| System memory available | 14.9 GB |

Reducing configured memory from 3072 to 1024 MB dropped per-VM RSS from
~408 MB to ~338 MB (17% reduction). The guest was already only touching
~380 MB, so the reduction mainly affects the sparse page table overhead.
Virtiofsd RSS decreased from ~229 MB to ~176 MB per VM.

### Capacity Projection (1024 MB config)

| Metric | 32GB Node |
|--------|-----------|
| Available for VMs | ~30 GB |
| Per-VM footprint | ~514 MB |
| Max concurrent VMs | ~58 |
| With idle hibernation (10% active) | ~580 users |

This is a 26% improvement over the 3072 MB config (was ~46 max VMs).

## KSM Status: NOT WORKING

KSM was enabled (`/sys/kernel/mm/ksm/run = 1`) with aggressive scan
settings (5000 pages per 200ms cycle). After 30+ minutes with 42 VMs
running, KSM reports:

- `pages_scanned: 0`
- `pages_shared: 0`
- `pages_sharing: 0`

Despite `--memory mergeable=on` in the cloud-hypervisor command line,
KSM finds zero eligible pages. This suggests cloud-hypervisor is not
calling `madvise(MADV_MERGEABLE)` on VM memory regions — possibly a
bug or limitation in the version deployed (from nixpkgs). Without KSM,
the theoretical 2× memory savings from page deduplication is not
available.

**Recommendation:** File upstream issue or test with explicit
`MADV_MERGEABLE` patching. Alternatively, investigate QEMU/KVM KSM
integration as cloud-hypervisor may handle this differently.

## Comparison with Previous Test (3072 MB config)

| Metric | 3072 MB | 1024 MB | Change |
|--------|---------|---------|--------|
| VM RSS | 408 MB | 338 MB | -17% |
| Virtiofsd RSS | 229 MB | 176 MB | -23% |
| Total per-VM | 637 MB | 514 MB | -19% |
| Max VMs (memory) | ~46 | ~58 | +26% |
| Health check latency | 1-4 ms | same | — |
| Conductor latency | N/A | 3.4-5.9s | — |

## Issues Found

1. **KSM not functional** — pages_scanned stays at 0 despite correct
   configuration. cloud-hypervisor `mergeable=on` may not work as
   expected with the current build.

2. **Admin registry shows 0 entries in roles** — The `/admin/sandboxes`
   endpoint returned 19 users but 0 role entries. The snapshot format
   may have changed or entries are stored differently than expected.

3. **VM boot time ~20s** — Cold boot with 32 existing VMs adds overhead
   vs the 12s baseline. Snapshot restore (~4.5s) should be used for
   returning users.

## Recommendations

1. **Keep 1024 MB config** — Guest only needs ~338 MB. The 1024 MB cap
   is sufficient headroom and allows 26% more VMs.

2. **Investigate KSM** — Either patch cloud-hypervisor to call
   MADV_MERGEABLE, or test a newer version. KSM could double capacity.

3. **Prioritize snapshot restore** — 20s cold boot vs 4.5s restore is a
   significant UX difference. Ensure hibernated VMs restore instead of
   cold booting.

4. **Add swap** — 4-8 GB swap provides safety margin for memory spikes.

5. **Monitor conductor latency** — 5.5s per prompt is the main user-facing
   latency. This is LLM-bound, not VM-bound.
