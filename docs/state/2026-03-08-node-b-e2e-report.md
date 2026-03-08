# Node B E2E Test Report — 2026-03-08

## Narrative Summary (1-minute read)

After deploying ADR-0014 corrections (virtio-blk for mutable data, btrfs-progs PATH fix)
and the BIOS boot screen fix for pre-authenticated sessions, we ran the full Playwright
hypervisor E2E suite against `draft.choir-ip.com` (Node B). **31 of 41 tests passed (76%)**,
with 4 failures (all known/expected), 3 skipped, and 3 that did not run. Auth, VM lifecycle,
concurrency, and core desktop flows are solid. Tests run sequentially (--workers=1) to
prevent interference from destructive VM lifecycle tests.

## What Changed (this deploy)

- `btrfs-progs` added to runtime-ctl nix PATH (was causing exit code 127)
- BIOS boot screen `__biosComplete()` fix confirmed deployed in WASM binary
- Auth E2E test selectors updated for new BIOS-style modal
- Concurrency load test rewritten for shared-sandbox architecture:
  - Removed per-user sandbox start calls (caused orphan process explosion)
  - Removed hibernate-under-load test (destructive to shared sandbox)
  - Added sandbox proxy readiness wait for conductor/writer tests
  - Added correct `ConductorExecuteRequest` schema (desktop_id, output_mode)

## Test Results

**Target:** `https://draft.choir-ip.com` (Node B)
**Suite:** `hypervisor` project (Playwright)
**Duration:** 10.9 minutes (sequential, --workers=1)
**Overall:** 31 passed / 4 failed / 3 skipped / 3 did not run

### Passed (31)

| Spec | Tests | Notes |
|------|-------|-------|
| bios-auth.spec.ts | 8/8 | All auth flows solid |
| concurrency-load-test.spec.ts | 7/7 | All concurrency tests pass |
| proxy-integration.spec.ts | 1/2 | Auto-spawn works |
| desktop-app-suite-hypervisor.spec.ts | 1/1 | Desktop renders |
| vm-lifecycle-report.spec.ts | 8/8 | Cold boot, health, concurrent requests, multi-account |
| vm-lifecycle-stress.spec.ts | 5/6 | Port discovery, registrations, WASM render, concurrent loads |
| writer-concurrency-hypervisor.spec.ts | 1/1 | Writer opens before run completes |

### Failed (4) — All Known/Expected

| Test | Cause | Severity |
|------|-------|----------|
| branch-proxy: branch runtime startable | Branch runtime not deployed on Node B | Expected |
| proxy-integration: /api/events | Sandbox events endpoint routing issue | Low |
| vm-snapshot-restore: hibernate→restore | virtiofsd reconnection after restore | Known (ch#6931) |
| writer-bugfix: prompt creates doc | Writer pipeline enters error state | Known bug |

### Skipped (3) + Did Not Run (3)

- vfkit cutover/terminal proofs: require local vfkit topology
- branch-proxy dependent tests: blocked by first branch test failure
- writer-concurrency: blocked by localhost:9090 path requirement

## Concurrency & Load Metrics

```
╔══════════════════════════════════════════════════════════════════════════╗
║              CONCURRENCY & LOAD TEST REPORT                            ║
╠══════════════════════════════════════════════════════════════════════════╣
║  concurrent-auth        5/5 users registered            1833 ms total  ║
║  concurrent-auth        avg per user                          367 ms   ║
║  concurrent-api         3/3 auth checks concurrent            196 ms   ║
║  concurrent-api         logout isolation                         yes   ║
║  auth-capacity          10/10 sequential registrations               ║
║  auth-capacity          avg time                              999 ms   ║
║  auth-capacity          p50 time                              996 ms   ║
║  auth-capacity          min/max                        976 / 1027 ms   ║
║  conductor-prompt       sandbox proxy readiness           ~90s wait    ║
║  mixed-workload         auth + heartbeat + reg concurrent     1130 ms  ║
╚══════════════════════════════════════════════════════════════════════════╝
```

Key observations:
- **Auth is fast and consistent**: ~1s per registration, no degradation at 10 users
- **Concurrent auth scales**: 5 users in 1.8s (367ms avg)
- **Sandbox proxy cold start**: ~90s for first proxied request after registration
- **Auth isolation**: Logout correctly scopes to single session

## Known Issues

1. **Sandbox proxy cold start delay**: First proxied request after registration takes ~90s
   (triggers runtime-ctl ensure + socat setup). Subsequent requests are fast.
2. **Orphan process accumulation**: Each ensure call spawns supervisord/virtiofsd.
   Failed ensures or test interruptions leave orphan processes. Runtime-ctl should
   clean stale PIDs on startup.
3. **VM snapshot/restore**: virtiofsd sockets don't reconnect after cloud-hypervisor
   restore (known upstream issue ch#6931). Cold boot works reliably.
4. **Writer pipeline**: Conductor → writer flow enters error state (known bug).

## Recommendations

1. Add stale PID/socket cleanup to runtime-ctl `ensure` action
2. Fix writer pipeline (conductor → writer error state)
3. Add content hash to WASM filename for cache busting
4. Profile sandbox proxy cold start delay (90s is too slow for UX)
5. Consider running destructive VM tests (snapshot, stop) in isolation
