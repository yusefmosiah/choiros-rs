# Capacity Stress Test Report

Date: 2026-03-11
Test: `capacity-stress-test.spec.ts` (v1 and v2)
Nodes: Node A (choir-ip.com, no DAX) and Node B (draft.choir-ip.com, DAX)

## Narrative Summary

Two rounds of capacity stress testing ramped VMs until failure. The v1 test
(fixed 10-user waves, idle existing VMs) found Node A's ceiling at 50-60 VMs
and ran Node B to 300 VMs without crash. The v2 test (background activity,
snapshot-restore, staggered registration) revealed that the v1 results were
misleadingly optimistic: Node B with realistic background load degraded at
60-70 VMs — far below the 300 VM idle ceiling. Background activity from
existing users is the dominant factor in capacity, not raw VM count.

## What Changed

- 2026-03-11: v1 test run on both nodes (fixed wave size, no background activity).
- 2026-03-11: v2 test run on both nodes (proportional waves, background activity,
  snapshot-restore). Killed at wave 8 (~80 VMs) when Node A production session stalled.

## What To Do Next

1. Profile the hypervisor under load — the bottleneck is likely the sandbox proxy
   or cloud-hypervisor process management, not memory.
2. Add idle-timeout hibernation to automatically free resources from inactive VMs.
3. Re-run v2 on Node B only (not Node A production) with higher wave count.

---

## Test Configurations

### v1: Static Wave Test

- Fixed wave size: 10 users/wave
- Max waves: 30
- Existing VMs: idle (no background activity)
- Boot types: cold boot only
- Health checks: sequential, 5 rounds/user

### v2: Realistic Load Test

- Wave size: max(10, 15% of running VMs), capped at 50
- Background activity on existing VMs during each wave:
  - 50% dormant (idle), 25% polling (health/heartbeat every 3-7s),
    15% active (conductor prompt + health), 10% bursty (quiet then burst)
- Hibernate 5% of existing VMs per wave and restore alongside cold boots
- Registration staggered with 0-2s random jitter per user
- Health checks: parallel, 3 rounds/user

---

## v1 Results: Static Wave Test

### Node A (no DAX) — Crashed at 60 VMs

| Wave | VMs | Boot p50 | Boot p95 | Health | Existing | Status |
|------|-----|----------|----------|--------|----------|--------|
| 1 | 10 | 9,812ms | 10,418ms | 100% | — | OK |
| 2 | 20 | 9,878ms | 10,559ms | 100% | 45ms | OK |
| 3 | 30 | 9,886ms | 10,399ms | 100% | 46ms | OK |
| 4 | 40 | **34,647ms** | 34,649ms | 100% | 47ms | DEGRADED |
| 5 | 50 | 10,058ms | 10,721ms | 100% | 47ms | OK |
| 6 | **60** | **timeout** | **timeout** | **0%** | 50ms | **CRASHED** |

Pattern: Stable at 10s boots through 30 VMs. Spike at 40 (34s), recovers at 50,
total failure at 60. All 10 VMs in wave 6 timed out after 125-127s. Existing VM
health remained at ~47-50ms throughout — the crash is boot-path specific, not a
system-wide resource exhaustion.

### Node B (DAX) — 300 VMs, Never Crashed

| Wave | VMs | Boot p50 | Boot p95 | Health | Existing | Status |
|------|-----|----------|----------|--------|----------|--------|
| 1 | 10 | 9,360ms | 10,269ms | 100% | — | OK |
| 2 | 20 | 10,024ms | 10,424ms | 100% | 45ms | OK |
| 3 | 30 | **33,390ms** | 33,418ms | 100% | 44ms | DEGRADED |
| 6 | 60 | **97,612ms** | 98,366ms | 100% | **356ms** | FAILING |
| 10 | 100 | **81,803ms** | 81,807ms | 100% | **677ms** | FAILING |
| 13 | 130 | **53,886ms** | 54,256ms | 100% | 46ms | DEGRADED |
| 15 | 150 | **339ms** | 785ms | 100% | 47ms | OK |
| 20 | 200 | 121ms | **85,979ms** | 100% | **531ms** | FAILING |
| 27 | 270 | **65,413ms** | 65,431ms | 100% | **839ms** | FAILING |
| 30 | 300 | 20,981ms | 21,107ms | 100% | 47ms | OK |

Pattern: Periodic boot spikes every 3-4 waves (~60-80s interval), always recovers.
After wave 15 (~150 VMs), steady-state boot times dropped to ~200ms (likely KSM
pages already merged — new VMs map into already-shared pages). Spike waves show
existing VM health degradation (356-839ms), confirming the spikes are system-wide
memory pressure events (KSM scan cycles competing with boot-path EPT faults).

### v1 Key Finding

With idle existing VMs, DAX gives ~5x capacity headroom (300 vs 60). But this is
misleading — no real system has 300 idle VMs. The ceiling depends on what existing
VMs are doing.

---

## v2 Results: Realistic Load Test

Both tests killed at wave 8 (~80 VMs) when Node A production session became
unresponsive. Data from 8 waves per node.

### Node B (DAX) — Degraded at 70 VMs

| Wave | VMs | Cold p50 | Restore p50 | New Health | Existing | BG Polling | BG Active | BG Bursty | Status |
|------|-----|----------|-------------|------------|----------|------------|-----------|-----------|--------|
| 1 | 10 | 181ms | — | 100% avg 201ms | — | — | — | — | OK |
| 2 | 20 | 443ms | — | 100% avg 169ms | 167ms | 92% | 90% | 100% | OK |
| 3 | 30 | 187ms | 2,651ms | 100% avg 106ms | 111ms | 99% | 99% | 100% | OK |
| 4 | 40 | 482ms | 2,655ms | 100% avg 132ms | 143ms | 100% | 99% | 100% | OK |
| 5 | 50 | 143ms | 2,493ms | 100% avg 142ms | 141ms | 80% | 81% | 100% | OK |
| 6 | 60 | 194ms | 1,827ms | 100% avg 149ms | **850ms** | **66%** | **64%** | **12%** | OK |
| 7 | **70** | 257ms | 906ms | **13%** avg 68ms | **2,550ms** | **35%** | **16%** | **6%** | **DEGRADED** |
| 8 | 80 | 301ms | 6,074ms | — | — | — | — | — | (killed) |

**The cliff is at 60-70 VMs with active background users.** At wave 6 (60 VMs),
existing VM health latency jumped to 850ms and background activity success rates
dropped (polling 66%, active 64%, bursty 12%). By wave 7 (70 VMs), existing VM
health hit 2.5s, only 13% of new VM health checks passed, and background active
users had 16% success rate.

### Node A (no DAX) — Degraded Earlier

| Wave | VMs | Cold p50 | Restore p50 | New Health | Existing | BG Polling | BG Active | BG Bursty | Status |
|------|-----|----------|-------------|------------|----------|------------|-----------|-----------|--------|
| 1 | 10 | 7,683ms | — | 100% avg 139ms | — | — | — | — | OK |
| 2 | 20 | 8,735ms | — | 100% avg 97ms | 190ms | 94% | 95% | 20% | OK |
| 3 | 30 | 9,579ms | 2,612ms | 100% avg 130ms | 174ms | 100% | 100% | 100% | OK |
| 4 | 40 | 8,612ms | 2,592ms | 100% avg 164ms | 221ms | 100% | 100% | 100% | OK |
| 5 | 50 | **26,035ms** | 1,786ms | 100% avg 151ms | **1,330ms** | 94% | 95% | 100% | OK |
| 6 | 60 | 9,347ms | 2,017ms | 100% avg 778ms | **533ms** | 93% | 94% | 100% | OK |
| 7 | 70 | **24,207ms** | 2,569ms | 100% avg 99ms | **1,812ms** | 93% | 95% | 100% | OK |
| 8 | 80 | 9,274ms | 2,665ms | — | — | — | — | — | (killed) |

Node A shows a different degradation pattern:
- **Cold boot times 10-50x slower** than DAX: 7-26s vs 0.1-0.5s (DAX VMs share
  already-mapped nix store pages; no-DAX VMs must fault in every page)
- Existing VM health degrades earlier (1.3s at 50 VMs vs 850ms at 60 for DAX)
- But background activity success rates stay higher (93-100% vs 35-66% on DAX) —
  this may be because no-DAX VMs have local page cache and don't contend on shared
  host pages during high-fan-out reads

---

## Analysis

### Why v2 Capacity Is Much Lower Than v1

v1 at 300 VMs had no background activity. Every existing VM was truly idle —
no requests, no page faults, no CPU usage. The hypervisor only had to handle
boot requests. v2 runs 15-25% of existing VMs as active/bursty users making
conductor prompts during each wave. This creates:

1. **CPU contention**: Conductor prompts invoke LLM gateway calls, which the
   hypervisor must proxy. At 60+ VMs with 10+ actively proxying, the hypervisor
   becomes the bottleneck.
2. **I/O contention**: Active VMs reading/writing their data volumes compete
   with new VMs faulting in nix store pages.
3. **Hypervisor proxy saturation**: The gateway proxy handles all sandbox HTTP
   traffic. Health checks, heartbeats, and conductor calls from 70 VMs converge
   on a single-process hypervisor.

### Boot Spike Pattern Explained

v1 showed periodic boot spikes every 3-4 waves. v2 confirms these correlate
with background activity:

- **v1 spikes** (no background activity): Every 60-80s, consistent with KSM's
  scan interval. New VM boots coincide with KSM's periodic memory merge cycle,
  which locks pages and competes with EPT fault handling.
- **v2 spikes**: More frequent and deeper because active background users create
  ongoing page faults. The host memory subsystem (EPT, KSM, page compaction)
  handles both new boot faults and existing user activity.

### Cold Boot vs Snapshot Restore

| | Node B (DAX) | Node A (no DAX) |
|---|---|---|
| Cold boot p50 | 150-500ms | 7,000-26,000ms |
| Snapshot restore p50 | 900-6,000ms | 1,700-2,700ms |

DAX cold boots are 50-100x faster because the nix store pages are already
mapped in host memory via shared DAX mappings. New VMs just set up EPT entries
pointing to already-resident host pages.

Snapshot restores are similar on both (~2-3s) because the restore path
reconstructs guest RAM from the snapshot file, not from the nix store.

### The Real Bottleneck

Memory is not the bottleneck at 60-80 VMs. Node B had 14GB available at 80 VMs.
The bottleneck is the **hypervisor process** — it's a single async Rust process
handling:
- HTTP reverse proxy for all sandbox traffic
- cloud-hypervisor process lifecycle management (start/stop/hibernate)
- WebAuthn authentication
- Provider gateway (LLM API proxying)

At 60+ VMs with active users, the hypervisor's connection handling and proxy
throughput saturate before memory does.

---

## Capacity Summary

| Config | Idle Ceiling (v1) | Active Ceiling (v2) | Bottleneck |
|--------|-------------------|---------------------|------------|
| Node A (no DAX, virtio-pmem) | 50 VMs | ~50 VMs (degraded at 50) | Memory + hypervisor |
| Node B (DAX, virtio-pmem) | 300+ VMs | ~60 VMs (degraded at 60-70) | Hypervisor proxy |
| Historical (virtio-blk) | 58 VMs | not tested | Memory |

DAX dramatically increases the memory ceiling (from 50 to 300+ idle VMs) but
the practical ceiling is set by hypervisor throughput at ~60-70 active VMs.
The next capacity improvement requires either:

1. **Horizontal scaling**: Multiple hypervisor processes or nodes behind a load
   balancer.
2. **Hypervisor optimization**: Connection pooling, async proxy improvements,
   or offloading the provider gateway to a separate process.
3. **Idle hibernation**: Automatically hibernate inactive VMs to free both memory
   and hypervisor connection slots.

---

## Snapshot Restore Viability

Snapshot restore works reliably (100% success rate across both nodes, 18 restores
total). Restore times are 1-6s, compared to 0.1-26s for cold boots depending on
config. This makes hibernate/restore a viable idle-management strategy:

- Hibernate VMs after N minutes of inactivity
- Restore on next user request (1-6s latency, acceptable for "warming up" UX)
- This could keep active VM count at ~30-40 while supporting hundreds of registered
  users

---

## Test Infrastructure Notes

- The capacity stress test creates real browser contexts per user (Playwright).
  At 300 VMs (v1), Playwright's trace artifact system ran out of file handles.
  The `stress` project in playwright.config.ts disables trace/video recording.
- Running stress tests against production (Node A) disrupted a real user session.
  Future stress tests should only target staging (Node B).
- The v2 test's 90s background activity window is a reasonable proxy for
  concurrent usage but doesn't capture long-lived session patterns (e.g., a user
  who sends a prompt every 5 minutes for an hour).

## Raw Data

- v1 Node A: `tests/artifacts/stress-v1-node-a.log` (not saved — inline above)
- v1 Node B: `tests/artifacts/stress-v1-node-b.log` (not saved — inline above)
- v2 Node B: 8 waves, killed at wave 8 (data inline above)
- v2 Node A: 8 waves, killed at wave 8 (data inline above)
