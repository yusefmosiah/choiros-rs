# ADR-0017: systemd-Native VM Lifecycle Management

Date: 2026-03-09
Kind: Decision
Status: Accepted
Priority: 2
Requires: [ADR-0014, ADR-0016]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

The bash runtime-ctl script (`scripts/ops/ovh-runtime-ctl.sh`) manages VM lifecycle
(virtiofsd, cloud-hypervisor, socat, TAP devices) via raw process spawning with PID files.
This is the root cause of orphan process accumulation, data.img lock contention, and
sandbox unavailability after failed operations. Each `ensure` call spawns new processes
without checking or cleaning stale ones. Test runs and crashes leave zombie process trees.

**Fix: Replace bash process management with systemd unit templates.** Each VM component
becomes a `@.service` unit parameterized by user/role instance ID. systemd handles:
- Process supervision and restart (`Restart=on-failure`)
- Orphan elimination (`KillMode=control-group` — kills entire cgroup on stop)
- Dependency ordering (`After=`, `Requires=`, `BindsTo=`)
- Idempotent start/stop (starting an already-running service is a no-op)
- Log aggregation (journald, no scattered log files)

**Policy logic stays in Rust.** The hypervisor's `SandboxRegistry` calls `systemctl`
instead of shelling out to bash. Decisions like cold-boot vs snapshot-restore, btrfs
snapshot timing, and per-user resource allocation remain in Rust code that can be tested.

**Why not ractor or a custom Rust supervisor?** OS process supervision is a solved problem.
systemd already runs on the host, already manages the hypervisor service, and provides
cgroup isolation that no userspace supervisor can match. ractor is designed for in-process
actor messaging — it has no subprocess management, no cgroup support, and adding it would
mean reimplementing what systemd already does.

## What Changed

Replaces `scripts/ops/ovh-runtime-ctl.sh` (469-line bash script) with:

1. **4 systemd unit templates** in `nix/hosts/ovh-node.nix`:
   - `tap-setup@.service` — TAP device creation/teardown (oneshot)
   - `virtiofsd@.service` — virtiofs daemon per VM instance
   - `cloud-hypervisor@.service` — VM process per instance
   - `socat-sandbox@.service` — port forwarding per instance

2. **Rust lifecycle methods** in `hypervisor/src/sandbox/mod.rs`:
   - `run_runtime_ctl()` replaced with `systemctl_start/stop/is_active` calls
   - `ensure_running()` orchestrates: btrfs setup → systemctl start chain
   - `hibernate()` orchestrates: socat stop → VM pause+snapshot → virtiofsd stop
   - `stop()` orchestrates: systemctl stop chain → btrfs snapshot → cleanup

## Decision

### Problem: Bash runtime-ctl has no supervision

The current flow when `ensure_running()` is called:

```
SandboxRegistry::run_runtime_ctl("ensure", user_id, role)
  → tokio::process::Command("ovh-runtime-ctl.sh", ["ensure", ...])
    → setup_tap (ip link add)
    → start_virtiofsd (background &, write PID file)
    → cold_boot_vm or restore_vm (background &, write PID file)
    → start_socat (background &, write PID file)
    → wait_for_vm_health (curl loop)
```

Failure modes that cause orphans:
- Script exits mid-way → virtiofsd running but no VM, PID file stale
- VM crashes → socat still running on stale port, virtiofsd still serving
- Second `ensure` call → new virtiofsd/VM spawned alongside old ones (no idempotency)
- Test suite interruption → entire process tree orphaned (no cgroup kill)
- data.img flock held by dead cloud-hypervisor → new VM can't start

### Solution: systemd unit templates

Each component becomes a parameterized systemd unit with `%i` instance substitution.
Instance ID format: `{user_id_short}-{role}` (e.g., `abc123-live`).

#### Unit dependency chain

```
tap-setup@%i.service (oneshot, RemainAfterExit=yes)
  ↑ Requires + After
virtiofsd@%i.service (simple, long-running)
  ↑ Requires + After
cloud-hypervisor@%i.service (simple, long-running)
  ↑ Requires + After
socat-sandbox@%i.service (simple, long-running)
```

`BindsTo=` on each child means stopping a parent cascades down.
`KillMode=control-group` on each unit means ALL processes in the cgroup die on stop.

#### Key properties

| Property | Bash (current) | systemd (proposed) |
|----------|---------------|-------------------|
| Orphan cleanup | None (PID files, manual kill) | KillMode=control-group (automatic) |
| Idempotency | None (spawns duplicates) | systemctl start is no-op if running |
| Dependency ordering | Sequential bash calls | After=/Requires=/BindsTo= |
| Restart on crash | None | Restart=on-failure |
| Logging | Scattered files + stdout | journald (filterable by unit) |
| Status query | PID file + kill -0 | systemctl is-active |

#### What stays in Rust

| Concern | Where | Why |
|---------|-------|-----|
| Cold boot vs snapshot restore | SandboxRegistry | Requires checking snapshot existence, btrfs state |
| btrfs subvolume create/snapshot | SandboxRegistry | Policy decision (when to snapshot, naming) |
| data.img provisioning | SandboxRegistry | Size, format, symlink into VM state dir |
| Per-user port allocation | SandboxRegistry | Registry state, collision avoidance |
| Idle watchdog | SandboxRegistry | Activity tracking, timeout policy |
| VM config generation | SandboxRegistry | CPU/RAM allocation, network params |

### Alternatives considered

**ractor (Rust actor framework):** Designed for in-process actor messaging. No subprocess
management, no cgroup isolation, no restart-on-crash for OS processes. Would require
reimplementing process supervision from scratch while losing cgroup guarantees. Rejected.

**Raw Rust process supervision:** Could spawn processes with `tokio::process::Command` and
track them. But: no cgroup kill (orphans escape), no journald integration, must reimplement
dependency ordering and restart logic. All of which systemd already provides. Rejected.

**microvm.nix / systemd-nspawn:** Too heavy for our needs. We already have cloud-hypervisor
working. Adding another abstraction layer increases complexity without solving the core
problem (orphan processes from unmanaged spawning). Deferred for future evaluation.

### Migration path

1. Add systemd unit templates to `nix/hosts/ovh-node.nix`
2. Add Rust `SystemdLifecycle` module to hypervisor
3. Wire `SandboxRegistry` to use `SystemdLifecycle` instead of `run_runtime_ctl`
4. Deploy to Node B, run E2E suite
5. Remove `scripts/ops/ovh-runtime-ctl.sh` after verification
6. Promote to Node A

### Risks

- **Snapshot/restore complexity:** VM pause+snapshot is a multi-step API call to
  cloud-hypervisor's HTTP API, not a simple systemctl stop. The Rust code must handle
  this directly (systemd manages process lifecycle, not VM-internal state).
- **Dynamic unit creation:** Templates exist at deploy time, but instances are created
  at runtime. `systemctl start cloud-hypervisor@abc123-live` creates the instance.
  If the template changes, running instances continue with old config until restarted.
- **TAP device naming:** Instance ID must map deterministically to TAP device name and
  MAC address. Current script derives these from role; per-user needs a stable mapping.

## What To Do Next

1. Implement systemd unit templates in nix config
2. Implement Rust SystemdLifecycle module
3. E2E verification on Node B (happy path + error recovery)
4. Remove bash runtime-ctl
5. Promote to Node A after Node A recovery
