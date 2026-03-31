# VM Lifecycle Testing & Fixes — Node B

**Date:** 2026-03-08
**Status:** Active

## Narrative Summary (1-minute read)

Node B (staging) was experiencing intermittent 502 errors. Through comprehensive Playwright testing we identified and fixed two bugs: (1) `ensure_running` didn't handle Stopped/Failed sandbox status, and (2) the proxy had no retry on TCP connect race conditions. We also discovered that all users share a single sandbox process on port 8080 — there's no per-user isolation yet.

We then added proper hibernate/restore support: a new `Hibernated` status distinguishes snapshot-preserved VMs from hard-stopped ones, the idle watchdog now sets `Hibernated` instead of `Stopped`, and a new `/hibernate` admin endpoint enables explicit snapshot creation. Snapshot restore runs in ~4.5s vs ~12s cold boot (2.68x speedup).

## What Changed

### Bug Fixes (commit `3165fce`)
- `ensure_running()` now handles `Stopped`/`Failed` status by cleaning up residual handles before re-spawning (previously only handled `Running`)
- `proxy_http()` retries TCP connect once after 500ms to handle the race between ensure_running completing and the port being ready

### Hibernate/Restore Support (commit `85f7f0d`)
- New `SandboxStatus::Hibernated` — distinguishes snapshot-preserved state from hard-stopped
- Idle watchdog sets `Hibernated` (not `Stopped`) after hibernate
- New `POST /admin/sandboxes/:user_id/:role/hibernate` endpoint
- `ensure_running` handles `Hibernated` by calling `runtime_ctl ensure`, which detects the snapshot and restores instead of cold booting

### Test Suites Added
- `vm-lifecycle-report.spec.ts` — cold boot, health reliability, concurrency, multi-account (8 tests)
- `vm-lifecycle-stress.spec.ts` — port discovery, rapid registration, WASM render, sandbox recovery (6 tests)
- `vm-snapshot-restore.spec.ts` — hibernate vs stop timing comparison (1 test)

## Performance Numbers (Node B)

| Metric | Value |
|--------|-------|
| Registration (WebAuthn) | ~1000ms |
| Cold boot (stop → ensure) | ~12s |
| Snapshot restore (hibernate → ensure) | ~4.5s avg |
| Restore speedup | 2.68x |
| Health endpoint avg latency | 59ms |
| Health endpoint reliability | 10/10 |
| 5 concurrent API calls | 248ms wall, 0 failures |
| 10 concurrent health checks | 395ms wall, 10/10 OK |
| 3 concurrent registrations | 1261ms wall |
| 4 concurrent page renders | 568ms wall, 4/4 OK |
| WASM render time | 360ms |
| WebSocket upgrade | Connected, working |

## Architecture Discovery: Shared Sandbox

All users on Node B share a single `sandbox-live` systemd service on port 8080. The hypervisor registry tracks entries per-user, but they all point to the same process. This means:
- No user data isolation
- Stopping one user's sandbox kills all users
- Idle watchdog hibernate affects all users simultaneously

This is the current state that ADR-0014 (per-user VM lifecycle) is designed to fix.

## Node A Status

Still DOWN after failed `nixos-rebuild switch` OOM'd during first unified flake build. Not pingable. Needs OVH KVM console or rescue mode.

## What To Do Next

1. **Recover Node A** — KVM console or rescue, check boot logs
2. **ADR-0014 Phase 1-3** — Per-user VM isolation (the real fix for shared-sandbox architecture)
3. **Optimize restore time** — Currently 4.5s; the `wait_for_vm_health` poll interval is 3s which adds latency. Could reduce to 1s.
4. **CI pre-build gate** — `nix build .#nixosConfigurations.<name>.config.system.build.toplevel` before `nixos-rebuild switch` to prevent OOM
