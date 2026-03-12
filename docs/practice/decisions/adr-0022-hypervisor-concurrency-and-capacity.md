# ADR-0022: Hypervisor Concurrency and Dynamic Capacity

Date: 2026-03-11
Kind: Decision
Status: Accepted
Priority: 1
Requires: [ADR-0014, ADR-0018]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

Capacity stress testing revealed that the practical VM ceiling with active
users is ~60-70 VMs — far below the 300+ idle-VM ceiling enabled by DAX.
The bottleneck is not memory or CPU but three concurrency chokepoints in
the hypervisor: a global mutex on the sandbox registry, per-request TCP
connection establishment in the HTTP proxy, and a global mutex on the
provider gateway rate limiter.

This ADR defines fixes for each bottleneck and replaces the hardcoded
`MAX_CONCURRENT_VMS = 50` cap with dynamic capacity based on resource
usage. It also adds system metrics collection to stress tests so future
capacity changes are data-driven.

## What Changed

- 2026-03-11: Initial ADR from stress test findings.
- 2026-03-11: Incorporated external code review (Codex). Added P1 findings:
  admin auth prerequisite, boot coalescing, port allocation TOCTOU, double
  TCP connect on hot path. Added security considerations for snapshots and
  stale-port routing.

## What To Do Next

Implement optimizations in order. See implementation guide:
`docs/theory/guides/adr-0022-implementation.md`

---

## Context: Stress Test Findings

Two rounds of capacity stress testing (report:
`docs/state/reports/2026-03-11-capacity-stress-test.md`):

| Test | Node A (no DAX) | Node B (DAX) |
|------|-----------------|--------------|
| v1 (idle existing VMs) | Crashed at 60 | 300 VMs, no crash |
| v2 (active background users) | Degraded at 50 | Degraded at 60-70 |

With active users (conductor prompts, health polling, bursty traffic),
both nodes degrade at roughly the same point regardless of DAX. The
memory ceiling is not the constraint — Node B had 14 GB available at
80 VMs. The hypervisor process itself is the constraint.

### Bottleneck 1: Global Mutex on Sandbox Registry

`SandboxRegistry.entries: Mutex<HashMap<String, UserSandboxes>>`

Every proxied request calls `ensure_running()` which acquires this lock
4-5 times. The idle watchdog holds it during async hibernate operations.
At 60+ concurrent users, all registry operations serialize on this one
lock regardless of which user's VM is being accessed.

**Impact:** Health check latency jumps from 45ms to 2,550ms at 70 VMs.
Background polling success drops from 100% to 35%.

### Bottleneck 2: No Connection Pooling in HTTP Proxy

Every HTTP request to a sandbox creates a fresh TCP connection and
HTTP/1.1 handshake via `TcpStream::connect` + `hyper::client::conn::
http1::handshake`. At 60 VMs with 10 requests/second each, this means
600 new connections/second with ~10-50ms overhead per connection.

**Impact:** Cumulative latency from connection setup across all
concurrent requests.

### Bottleneck 3: Global Mutex on Rate Limiter

`rate_limit_state: Arc<Mutex<HashMap<String, Vec<Instant>>>>`

Every provider gateway request (LLM calls from any sandbox) contends
on this lock. The lock is held during an O(n) `retain` operation on
each sandbox's request history vector.

**Impact:** Serializes all LLM proxy calls across all VMs.

### Bottleneck 4: Static VM Cap

`const MAX_CONCURRENT_VMS: usize = 50` is hardcoded. With DAX and the
concurrency fixes above, the node can handle significantly more VMs.
The cap should be driven by actual resource availability.

---

## Prerequisite: Admin Authorization (ADR-0020 C1)

The `/admin/*` endpoints have no authorization — any authenticated user
can enumerate VM state and start/stop/hibernate other users' sandboxes.
The proposed `/admin/metrics` endpoint inherits the same problem.

**This must be fixed before deploying the metrics endpoint.** The fix is
defined in ADR-0020 (admin allowlist by user ID). Implementation is
independent of the concurrency work below but is a hard prerequisite
for Phase 5 (metrics endpoint).

---

## Decision 1: Coalesce Concurrent Cold Starts

`ensure_running()` has a race: a second request arriving while the first
spawn is in progress sees the `Stopped` placeholder and calls
`spawn_instance()` again for the same user/role/port. This turns bursty
traffic into duplicate boot work.

Replace the status-check-then-spawn pattern with per-(user, role) start
leases. When the first request begins a spawn, it inserts a
`Starting(tokio::sync::watch::Receiver)` status. Subsequent requests
for the same user/role join the existing watch channel instead of
re-entering lifecycle code.

```rust
enum SandboxStatus {
    Running,
    Hibernated,
    Starting(watch::Receiver<Result<u16, String>>),
    Stopped,
    Failed,
}
```

The spawning task sends the result (port or error) on the watch channel.
All waiters receive it simultaneously.

### Consequences

- Bursty login traffic produces exactly one boot per VM, not N
- Stress test results become more representative (no wasted boot work)
- The `Starting` state also provides natural admission control: requests
  that arrive during boot wait instead of spawning duplicates

---

## Decision 2: Port Allocation with Reserved Leases

The current port allocator scans all entries for used ports, then probes
candidates with a TCP connect. With DashMap's non-atomic iter-then-insert,
two concurrent allocators can pick the same port (TOCTOU bug). The current
`Mutex<HashMap>` prevents this only because the lock is held across both
the scan and the placeholder insert.

Replace scan-based allocation with a dedicated `PortAllocator` that owns
port reservations independently of the registry:

```rust
struct PortAllocator {
    reserved: DashSet<u16>,
    range_start: u16,
    range_end: u16,
}

impl PortAllocator {
    fn reserve(&self) -> Option<u16> {
        for port in self.range_start..=self.range_end {
            if self.reserved.insert(port) {
                return Some(port);
            }
        }
        None
    }
    fn release(&self, port: u16) {
        self.reserved.remove(&port);
    }
}
```

`DashSet::insert` returns `false` if already present — this is an atomic
test-and-set that eliminates the TOCTOU. Ports are released when a VM
stops or fails.

### Consequences

- Port allocation is O(1) amortized (no full scan)
- No TOCTOU: `DashSet::insert` is atomic
- Port lifecycle is explicit: reserve on allocate, release on stop/fail
- Works correctly with DashMap registry (no ordering dependency)

---

## Decision 3: Remove Readiness Probe from Hot Path

Every proxied request pays two local TCP connects: `is_port_ready()`
inside `ensure_running()` (line 262) and then `TcpStream::connect` in
the proxy (line 37). The readiness probe is only useful for detecting a
crashed sandbox — but the proxy connect already does that.

Remove the `is_port_ready()` call from the `Running` status branch.
Instead, trust the `Running` status and let the proxy handle connection
failures:

```rust
SandboxStatus::Running => {
    entry.last_activity = Instant::now();
    return Ok(entry.port);
}
```

If the proxy connect fails, it returns 502 Bad Gateway. The next request
can then detect the failure and trigger a respawn. This halves the TCP
connect overhead on every successful request.

### Consequences

- Hot path goes from 2 TCP connects to 1
- Crashed sandbox detection moves from proactive to reactive (first
  request after crash gets 502, second request triggers respawn)
- With connection pooling (Decision 5), the hot path may have zero
  new TCP connects for active sandboxes

---

## Decision 4: Replace Global Mutex with DashMap

Replace `entries: Mutex<HashMap<String, UserSandboxes>>` with
`entries: DashMap<String, UserSandboxes>`.

DashMap is a concurrent hash map with per-shard locking. Operations on
different users' entries proceed in parallel. Only operations on the
same user's entry contend.

The idle watchdog must be restructured: collect hibernation candidates
under brief read locks, then execute async hibernate operations without
holding any lock.

Port allocation race is resolved by Decision 2 (dedicated `PortAllocator`
with atomic `DashSet::insert`).

### Consequences

- `ensure_running` for user A no longer blocks user B's health checks
- Idle watchdog no longer blocks all registry access during hibernation
- `count_running_vms` uses `DashMap::iter()` which takes per-shard read
  locks briefly (acceptable for rare operations)

---

## Decision 5: Add Connection Pooling to HTTP Proxy

Replace per-request `TcpStream::connect` + `handshake` with
`hyper_util::client::legacy::Client` which provides automatic HTTP/1.1
connection pooling. `hyper-util` with `features = ["full"]` is already
in `hypervisor/Cargo.toml`.

Store the pooled client in `AppState` and pass through the proxy
middleware. Configure:
- `pool_idle_timeout(30s)` — release unused connections after 30s
- `pool_max_idle_per_host(10)` — up to 10 idle connections per sandbox

WebSocket proxying is unchanged (long-lived connections, no pooling
benefit).

**Security note:** Pool entries must be keyed by `(port, generation)`
or invalidated on sandbox restart. A stale pooled connection to a
recycled port could route traffic to the wrong user's VM. The proxy
should verify the sandbox identity on connection reuse or evict all
connections for a port when its owning sandbox changes.

### Consequences

- Subsequent requests to the same sandbox reuse existing TCP connections
- Connection setup overhead eliminated for active sandboxes
- Failed pooled connections are automatically evicted on sandbox restart

---

## Decision 6: Replace Rate Limiter Mutex with DashMap

Replace `rate_limit_state: Arc<Mutex<HashMap<String, Vec<Instant>>>>`
with `Arc<DashMap<String, Vec<Instant>>>`.

Each sandbox's rate limit bucket is independent. DashMap's `entry()` API
is a near drop-in replacement for the existing `HashMap::entry()` usage.

### Consequences

- LLM proxy calls for different sandboxes no longer serialize
- Same-sandbox rate limit checks are still serialized (correct behavior)

---

## Decision 7: Dynamic VM Cap Based on Resource Usage

Replace `const MAX_CONCURRENT_VMS: usize = 50` with a configurable
hard ceiling and a dynamic effective cap based on system resources.

The hard ceiling comes from `CHOIR_MAX_VMS` environment variable
(default 200). The effective cap scales down based on available memory:

| Available Memory | Effective Cap |
|-----------------|---------------|
| > 60% | 100% of hard ceiling |
| 30-60% | 75% of hard ceiling |
| 15-30% | 50% of hard ceiling |
| < 15% | No new VMs |

Optionally, 1-minute load average can further scale down the cap if
CPU is overloaded. For more granular control, consider PSI (Pressure
Stall Information) / cgroups and per-tenant budgets.

### Consequences

- DAX nodes can run more VMs when memory is abundant
- Non-DAX nodes naturally hit lower caps as memory fills
- No manual tuning needed when moving between hardware configurations
- Users see "Server at capacity, please try again shortly" when the
  dynamic cap is reached

---

## Decision 8: System Metrics in Stress Tests

Add an `/admin/metrics` endpoint to the hypervisor that returns:
- `mem_total_mb`, `mem_available_mb`, `mem_percent_available`
- `running_vms`, `hibernated_vms`
- `load_avg_1m`, `load_avg_5m`
- `ksm_pages_sharing`, `ksm_pages_shared` (when available)

The stress test polls this endpoint at the start and end of each wave,
including the data in the per-wave report table. This enables
correlation between resource usage and performance degradation, and
produces graphs for capacity reports.

**Prerequisite:** Admin authorization (ADR-0020 C1) must be deployed
before this endpoint goes to production.

### Consequences

- Stress test reports include system resource data alongside latency
- Capacity decisions can reference concrete resource thresholds
- The endpoint is on `/admin/` (requires admin auth per ADR-0020)

---

## Security Considerations

### Snapshot and Hibernation Paths

VM snapshots are stored under `/opt/choiros/vms/state/.../vm-snapshot`
and data snapshots under `/data/snapshots/`. These contain full guest
RAM, which may include sensitive material (user data, in-flight LLM
responses). The gateway bearer token is also written into each instance
state directory.

Requirements:
- Snapshot directories must have strict permissions (0700, hypervisor user)
- Explicit retention policy: delete snapshots after configurable TTL
- Consider encryption at rest for snapshot files
- Gateway token files already addressed by ADR-0020 H5

### Stale-Port Cross-Tenant Risk

If the proxy caches routing or pools connections by `127.0.0.1:port`,
and a port is recycled to a different user's VM, stale cache/pool
entries could route traffic to the wrong tenant. Connection pool entries
must be bound to a VM generation or unit identity, not just host:port.

### DAX/KSM as Trust-Tier Feature

DAX on a shared `/nix/store` is a cross-VM observation channel (timing
side-channel on shared page accesses). KSM is the same class: it
deduplicates identical anonymous guest pages across VMs. Both are safe
when all tenants are trusted (same operator, same trust level) but
should be a trust-tier feature, not a global default, if hostile
co-tenants are in scope. This aligns with the planned free-tier (shared
pmem+DAX, public data) vs paid-tier (isolated virtio-blk, private data)
distinction.

---

## Future Optimizations (Not In Scope)

These are architecturally sound but not needed at current scale:

- **Data/control plane split:** Separate the HTTP proxy into its own
  process with dedicated connection pools. The hypervisor becomes a
  control plane only (auth, lifecycle, provider gateway).
- **WS/SSE for liveness:** Replace health/heartbeat polling with
  server-push. The stress test shows how much traffic comes from
  `/health` and `/heartbeat` — pushing liveness to WebSocket or SSE
  with adaptive backoff would reduce per-VM request volume.
- **Cloud Hypervisor VMM optimizations:** Huge pages (THP), vhost-user-net
  improvements for the network path. Relevant if the bottleneck moves
  below the hypervisor application layer.

---

## Existing Positive Design Decisions

- Tokio multi-threaded runtime (uses all CPU cores by default)
- `hyper-util` already in dependencies (connection pool available)
- Memory availability check already in `ensure_running` (extend, not replace)
- Idle watchdog already hibernates inactive VMs (fix locking, keep behavior)
- Placeholder port reservation pattern (extend to proper allocator)

---

## Sources

- Capacity stress test report: `docs/state/reports/2026-03-11-capacity-stress-test.md`
- DAX vs no-DAX load test: `docs/state/reports/2026-03-11-dax-vs-nodax-load-test.md`
- External code review: Codex (2026-03-11)
- DashMap: https://docs.rs/dashmap
- hyper-util connection pool: https://docs.rs/hyper-util/latest/hyper_util/client/legacy/struct.Client.html
- Linux KSM docs: https://www.kernel.org/doc/html/latest/admin-guide/mm/ksm.html
- Cloud Hypervisor performance: https://github.com/cloud-hypervisor/cloud-hypervisor/releases
