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

## Decision 1: Replace Global Mutex with DashMap

Replace `entries: Mutex<HashMap<String, UserSandboxes>>` with
`entries: DashMap<String, UserSandboxes>`.

DashMap is a concurrent hash map with per-shard locking. Operations on
different users' entries proceed in parallel. Only operations on the
same user's entry contend.

The idle watchdog must be restructured: collect hibernation candidates
under brief read locks, then execute async hibernate operations without
holding any lock.

### Consequences

- `ensure_running` for user A no longer blocks user B's health checks
- Idle watchdog no longer blocks all registry access during hibernation
- `count_running_vms` and `allocate_port` use `DashMap::iter()` which
  takes per-shard read locks briefly (acceptable for rare operations)
- Port allocation has a theoretical race (two concurrent allocations
  could pick the same port), mitigated by the existing TCP probe

---

## Decision 2: Add Connection Pooling to HTTP Proxy

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

### Consequences

- Subsequent requests to the same sandbox reuse existing TCP connections
- Connection setup overhead eliminated for active sandboxes
- Failed pooled connections are automatically evicted on sandbox restart

---

## Decision 3: Replace Rate Limiter Mutex with DashMap

Replace `rate_limit_state: Arc<Mutex<HashMap<String, Vec<Instant>>>>`
with `Arc<DashMap<String, Vec<Instant>>>`.

Each sandbox's rate limit bucket is independent. DashMap's `entry()` API
is a near drop-in replacement for the existing `HashMap::entry()` usage.

### Consequences

- LLM proxy calls for different sandboxes no longer serialize
- Same-sandbox rate limit checks are still serialized (correct behavior)

---

## Decision 4: Dynamic VM Cap Based on Resource Usage

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
CPU is overloaded.

### Consequences

- DAX nodes can run more VMs when memory is abundant
- Non-DAX nodes naturally hit lower caps as memory fills
- No manual tuning needed when moving between hardware configurations
- Users see "Server at capacity, please try again shortly" when the
  dynamic cap is reached

---

## Decision 5: System Metrics in Stress Tests

Add an `/admin/metrics` endpoint to the hypervisor that returns:
- `mem_total_mb`, `mem_available_mb`, `mem_percent_available`
- `running_vms`, `hibernated_vms`
- `load_avg_1m`, `load_avg_5m`
- `ksm_pages_sharing`, `ksm_pages_shared` (when available)

The stress test polls this endpoint at the start and end of each wave,
including the data in the per-wave report table. This enables
correlation between resource usage and performance degradation, and
produces graphs for capacity reports.

### Consequences

- Stress test reports include system resource data alongside latency
- Capacity decisions can reference concrete resource thresholds
- The endpoint is on `/admin/` (will require admin auth per ADR-0020)

---

## Existing Positive Design Decisions

- Tokio multi-threaded runtime (uses all CPU cores by default)
- `hyper-util` already in dependencies (connection pool available)
- Memory availability check already in `ensure_running` (extend, not replace)
- Idle watchdog already hibernates inactive VMs (fix locking, keep behavior)

---

## Sources

- Capacity stress test report: `docs/state/reports/2026-03-11-capacity-stress-test.md`
- DAX vs no-DAX load test: `docs/state/reports/2026-03-11-dax-vs-nodax-load-test.md`
- DashMap: https://docs.rs/dashmap
- hyper-util connection pool: https://docs.rs/hyper-util/latest/hyper_util/client/legacy/struct.Client.html
