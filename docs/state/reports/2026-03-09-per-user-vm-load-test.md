# Per-User VM Load Test Report

Date: 2026-03-09
Node: B (staging) — 147.135.70.196 (draft.choir-ip.com)
Hardware: 12 cores, 32GB RAM, 2×512GB NVMe (btrfs RAID1)

## Narrative Summary

Per-user VM isolation (ADR-0014 Phase 4) is deployed on Node B.
Each authenticated user gets their own cloud-hypervisor microVM with a
unique port, IP, MAC, and TAP device. This report covers the first
load test with concurrent per-user VMs under real E2E traffic.

## Test Conditions

- 25-36 per-user VMs running concurrently (fluctuated as E2E tests ran)
- Each VM: 2 vCPU, 3072 MB configured memory, NixOS guest with sandbox
- Plus 1 shared "live" VM for default/unauthenticated users
- VM networking: br-choiros bridge, dnsmasq DHCP, per-user TAP devices

## Memory Efficiency

| Metric | Value |
|--------|-------|
| Configured memory per VM | 3072 MB |
| Actual RSS per VM (process) | 379–447 MB (avg 408 MB) |
| Virtiofsd overhead per VM | ~229 MB |
| Total per-VM footprint | ~637 MB |
| Memory efficiency | 13% of configured actually used |
| Cloud-hypervisor `mergeable=on` | Enabled (KSM page dedup) |

The VMs use sparse memory allocation — cloud-hypervisor only commits pages
the guest touches. With `mergeable=on`, identical pages across VMs are
deduplicated by the kernel (KSM). A freshly booted sandbox guest touches
~380 MB of its 3072 MB allocation.

## Capacity Analysis

| Metric | Node B (32GB) |
|--------|---------------|
| Available for VMs | ~30 GB (after 2GB host reserve) |
| Max concurrent VMs (memory) | ~46 |
| IP range capacity | 153 (10.0.0.102–254) |
| Port range capacity | 1000 (12000–12999) |
| **Bottleneck** | **Memory (46 VMs)** |
| Disk per data.img (actual) | 66 MB avg (2 GB sparse) |
| Disk capacity (btrfs) | 444 GB free → ~6700 users |

With idle hibernation (Phase 5), hibernated VMs release memory. The
effective capacity depends on concurrency ratio — if 10% of users are
active simultaneously, the node can serve ~460 registered users.

## Latency Results

### Test 1: Sequential health checks (25 VMs)

| Metric | Value |
|--------|-------|
| Requests | 25 |
| Success rate | 100% |
| Min | 0.9 ms |
| p50 | 1.4 ms |
| p90 | 1.7 ms |
| Max | 1.8 ms |

### Test 2: Parallel burst (25 VMs × 5 rounds = 125 requests)

All 25 VMs hit simultaneously, 5 rounds.

| Metric | Value |
|--------|-------|
| Requests | 125 |
| Success rate | 100% |
| Duration | 1.0 s |
| Min | 0.9 ms |
| p50 | 2.2 ms |
| p90 | 4.3 ms |
| p99 | 7.0 ms |
| Max | 7.2 ms |

### Test 3: Sustained load (25 VMs × 100 requests = 2500 requests)

100 sequential requests per VM, all VMs in parallel.

| Metric | Value |
|--------|-------|
| Requests | 2,500 |
| Success rate | 100% |
| Duration | 3.6 s |
| Throughput | ~694 req/s |
| Min | 1.0 ms |
| p50 | 2.1 ms |
| p90 | 3.7 ms |
| p95 | 4.5 ms |
| p99 | 6.0 ms |
| Max | 8.3 ms |

## CPU Usage

- 12 cores, load average 2.4 during tests
- Per-VM CPU: 3.5–3.8% each (idle sandbox)
- Total VM CPU: ~25 VMs × 3.7% = ~93% of one core
- Headroom: significant (load avg 2.4 / 12 cores = 20% utilization)

## Cold Boot Time

- Not measured in this test (no new VMs were booted during the test window)
- Previous measurements: ~12s cold boot, ~4.5s snapshot restore

## Issues Found

1. **Live VM died during load test** — cloud-hypervisor@live went inactive
   while 36 VMs were running. No OOM detected (16GB available via cache).
   Restarted cleanly. Root cause unclear — may be related to memory pressure
   or a virtiofsd issue.

2. **VM count fluctuation** — Started with 47, dropped to 25-36 as some
   VMs were cleaned up between test rounds. The idle watchdog may have
   hibernated some.

3. **Socat failures** — 12 socat units in "failed" state alongside 25
   running VMs. These are stale entries from earlier boot attempts that
   timed out (before the dnsmasq DHCP fix was deployed).

## Recommendations

1. **Enable KSM** — `echo 1 > /sys/kernel/mm/ksm/run` or add
   `boot.kernel.sysctl."vm.ksm_run" = 1` in NixOS config. KSM is
   currently disabled (`run=0`) even though cloud-hypervisor passes
   `mergeable=on`. All VMs run identical NixOS images — KSM should
   deduplicate significant shared pages, potentially halving per-VM
   memory from 400 MB to ~200 MB (doubling capacity to ~90 VMs).

2. **Set memory limit per VM to 1024 MB** — Guests only use ~400 MB.
   Reducing from 3072 to 1024 would cap worst-case memory and allow
   the scheduler to pack more VMs.

3. **Add swap** — Even 4-8 GB swap would provide a safety net for
   memory pressure spikes and allow more aggressive VM packing.

4. **Monitor KSM after enabling** — Check `/sys/kernel/mm/ksm/pages_shared`
   to quantify actual memory savings.

4. **Investigate live VM death** — May need a watchdog or auto-restart
   mechanism for the shared live VM.

5. **Reduce socat health check timeout** — Currently 90s, could be 30s
   since VMs boot in ~12s.
