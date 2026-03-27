# ADR-0025: Go Admin Dashboard

Date: 2026-03-11
Kind: Decision
Status: Proposed
Priority: 1
Requires: [ADR-0024]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

The first Go service in ChoirOS is an admin dashboard. It reads the
hypervisor's existing SQLite database (read-only) and live system stats from
/proc and statfs, then serves a server-rendered HTML page. Zero coupling to
the Rust hypervisor — no shared libraries, no IPC, no feature flags. Just an
additive Go binary running alongside the existing stack.

This is the smallest possible Go service that exercises the full deploy
pipeline: Go source → nix build → systemd unit → Caddy route. It proves
ADR-0024's incremental Go introduction path with zero risk to existing
functionality.

## What Changed

- 2026-03-11: Initial ADR.

## What To Do Next

1. Scaffold the Go module and binary.
2. Implement SQLite queries and /proc readers.
3. Serve HTML dashboard on a local port.
4. Add nix build, systemd unit, Caddy route.
5. Deploy to Node B via CI.

---

## Decision 1: Standalone Go Binary, Read-Only

The admin dashboard is a separate process. It does not modify, extend, or
replace any part of the Rust hypervisor.

It reads:
- Hypervisor SQLite DB (read-only, `SQLITE_OPEN_READONLY`)
- `/proc/meminfo`, `/proc/loadavg` (Linux sysfs)
- `statfs` syscall for disk usage

It serves:
- `GET /admin/` — server-rendered HTML dashboard
- `GET /admin/api/stats` — JSON for programmatic use

It requires:
- Admin authentication (initially: shared secret or IP allowlist, later:
  hypervisor session cookie validation)

## Decision 2: No JS Framework

Server-rendered HTML with Go `html/template`. Auto-refresh via
`<meta http-equiv="refresh" content="30">`. No React, no Dioxus, no
WebSocket, no build step for frontend assets.

For time-series visualization later, embed a single-file charting library
(uPlot, 35KB) via script tag. No npm, no bundler.

## Decision 3: Dashboard Content

### Live system stats (from /proc and syscalls)

- Memory: total, available, used percentage
- CPU: load average (1m, 5m, 15m)
- Disk: total, free, used percentage (for /data btrfs partition)

### From existing SQLite tables

**Users:**
- Total registered users
- Recent registrations (last 7d)

**Sessions (audit_log):**
- Active sessions (last 24h)
- Login count by day (last 7d)
- Recent login events

**VMs (user_vms):**
- Active VMs by state (running, stopped, failed)
- VMs per user
- Recent lifecycle events (runtime_events)

**Branch runtimes (branch_runtimes):**
- Active runtimes by state
- Port allocation map

**Route pointers:**
- Current routing state per user

### Not in scope (yet)

- Provider gateway stats (no table yet — add later)
- Job queue stats (ADR-0014, not built yet)
- Historical time-series (no storage, render live only)

## Decision 4: Deploy Shape

```
choiros-rs/
  admin-dashboard/          ← new Go module
    main.go
    handlers.go
    queries.go
    system.go
    templates/
      dashboard.html
    go.mod
    go.sum
```

Nix build:
```nix
admin-dashboard = pkgs.buildGoModule {
  pname = "choir-admin";
  version = "0.1.0";
  src = ./admin-dashboard;
  vendorHash = "...";
};
```

Systemd unit:
```ini
[Service]
ExecStart=/opt/choiros/bin/admin-dashboard \
  --db /opt/choiros/data/hypervisor.db \
  --listen 127.0.0.1:9091
ReadOnlyPaths=/opt/choiros/data/hypervisor.db
```

Caddy route:
```
handle /admin/* {
    reverse_proxy 127.0.0.1:9091
}
```

## Decision 5: Auth

Phase 1: IP allowlist (localhost + admin IPs). Simple, sufficient for
single-operator use.

Phase 2: Read the hypervisor's session cookie, validate against the
tower_sessions table in SQLite, check if user has admin role. Requires
adding an `is_admin` column to the users table (hypervisor migration).

## Consequences

### Positive

- First Go service deployed through the full pipeline
- Immediately useful for operations
- Zero risk to existing functionality (additive, read-only)
- Proves nix buildGoModule + systemd + Caddy for Go services
- Foundation for future Go services (ADR-0024)

### Negative

- One more process to manage (mitigated: systemd, minimal resource usage)
- SQLite read-only connection adds a reader to the DB (mitigated: SQLite
  handles concurrent readers well, WAL mode)
- Admin auth is initially weak (mitigated: IP allowlist, internal port)

## Sources

- [uPlot](https://github.com/leeoniya/uPlot) — lightweight charting
- [mattn/go-sqlite3](https://github.com/mattn/go-sqlite3) — SQLite driver
- Go `html/template` — server-rendered templates
