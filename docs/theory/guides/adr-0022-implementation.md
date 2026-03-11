# Implementing ADR-0022: Hypervisor Concurrency and Dynamic Capacity

Date: 2026-03-11
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0022]

## Narrative Summary (1-minute read)

Three concurrency bottlenecks in the hypervisor cap practical VM capacity
at ~60-70 active users despite having 300+ VM memory headroom with DAX.
This guide covers implementing DashMap for the registry and rate limiter,
connection pooling for the HTTP proxy, dynamic capacity gating, and system
metrics collection. Implementation order is designed so each step can be
tested independently.

## What Changed

- 2026-03-11: Initial implementation guide.

## What To Do Next

Start with Phase 1 (rate limiter DashMap — smallest, safest), then Phase 2
(registry DashMap — biggest impact), then Phase 3 (connection pooling),
Phase 4 (dynamic cap), Phase 5 (metrics endpoint + stress test).

---

## Phase 1: Rate Limiter DashMap (30 min)

Smallest change, lowest risk. Validates the DashMap dependency before
tackling the registry.

### 1a. Add DashMap dependency

**File:** `hypervisor/Cargo.toml`

```toml
dashmap = "6"
```

### 1b. Change rate limit state type

**File:** `hypervisor/src/state.rs` (line 18)

```rust
// Before
pub rate_limit_state: Arc<Mutex<HashMap<String, Vec<Instant>>>>,

// After
pub rate_limit_state: Arc<DashMap<String, Vec<Instant>>>,
```

Add `use dashmap::DashMap;` to imports.

### 1c. Update initialization

**File:** `hypervisor/src/main.rs` (around line 100)

```rust
// Before
rate_limit_state: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),

// After
rate_limit_state: Arc::new(DashMap::new()),
```

### 1d. Update rate limit enforcement

**File:** `hypervisor/src/provider_gateway.rs` (lines 377-407)

```rust
// Before
pub async fn enforce_per_sandbox_rate_limit(
    state: &ProviderGatewayState,
    sandbox_id: &str,
) -> Result<(), Response> {
    let mut guard = state.rate_limit_state.lock().await;
    let bucket = guard.entry(sandbox_id.to_string()).or_default();
    // ...retain, len check, push...
}

// After
pub async fn enforce_per_sandbox_rate_limit(
    state: &ProviderGatewayState,
    sandbox_id: &str,
) -> Result<(), Response> {
    let mut bucket = state.rate_limit_state.entry(sandbox_id.to_string()).or_default();
    // ...retain, len check, push — same logic, just operating on DashMap RefMut...
}
```

The `retain`, `len()`, and `push` calls work identically on the `RefMut`
returned by `DashMap::entry().or_default()`.

### 1e. Update test

**File:** `hypervisor/src/provider_gateway.rs` (test around line 643-665)

Change `Mutex::new(HashMap::new())` to `DashMap::new()`.

### Verify

```bash
cargo test -p hypervisor --lib -- rate_limit
cargo clippy -p hypervisor
```

---

## Phase 2: Sandbox Registry DashMap (2-3 hours)

Biggest change, highest impact. The global `Mutex<HashMap>` becomes a
`DashMap` with per-shard locking. Every method on `SandboxRegistry` that
calls `self.entries.lock().await` must be converted.

### 2a. Change struct definition

**File:** `hypervisor/src/sandbox/mod.rs` (line 149)

```rust
// Before
entries: Mutex<HashMap<String, UserSandboxes>>,

// After
entries: DashMap<String, UserSandboxes>,
```

Update `new()` accordingly.

### 2b. Convert simple accessors

These methods do a single lock → lookup → return. Convert to
`self.entries.get(&user_id)` or `self.entries.get_mut(&user_id)`:

| Method | Line | Pattern |
|--------|------|---------|
| `touch_activity` | ~583 | `get_mut` → update `last_activity` |
| `port_of` | ~595 | `get` → return port |
| `branch_port_of` | ~607 | `get` → return port |
| `snapshot` | ~619 | `iter()` → collect snapshot |
| `stop` | ~511 | `get_mut` → stop handle |
| `hibernate` | ~529 | `get_mut` → hibernate handle |
| `stop_branch` | ~547 | `get_mut` → stop handle |
| `swap_roles` | ~562 | `get_mut` → swap entries |

Pattern for each:

```rust
// Before
let mut entries = self.entries.lock().await;
if let Some(user_map) = entries.get_mut(user_id) {
    // ...
}

// After
if let Some(mut user_map) = self.entries.get_mut(user_id) {
    // ...
}
```

### 2c. Convert ensure_running (critical path)

**File:** `hypervisor/src/sandbox/mod.rs` (lines 249-419)

This is the hot path — called on every proxied request. Currently
acquires the mutex 4-5 times. Strategy: minimize time holding DashMap
guards, never hold them across async operations.

```
Phase A: Check existing entry (brief get_mut, extract status/port, drop guard)
Phase B: If running, return port (no lock needed)
Phase C: If needs restart, do async stop_handle (no lock held)
Phase D: Capacity gate — count_running_vms via iter() (brief read locks)
Phase E: Allocate port via iter() (brief read locks)
Phase F: Spawn instance (no lock held — this takes seconds)
Phase G: Insert entry (brief get_mut or entry().or_default())
```

Key rule: **never hold a DashMap guard across an `.await` point.**
DashMap guards are `!Send` in the default configuration, so the compiler
will enforce this — holding a guard across `.await` is a compile error.

### 2d. Convert ensure_branch_running

Same pattern as `ensure_running`. Currently holds the mutex across
`spawn_instance` (the worst offender — 45+ seconds). Must restructure
to drop the guard before spawning.

### 2e. Restructure idle watchdog (critical fix)

**File:** `hypervisor/src/sandbox/mod.rs` (lines 660-769)

Currently holds the mutex for the entire hibernation sweep, including
async `hibernate_handle` calls. Must become collect-then-execute:

```rust
// Phase 1: Collect candidates (brief per-shard read locks)
let mut to_hibernate: Vec<(String, SandboxRole, ...)> = Vec::new();
for entry in self.entries.iter() {
    let user_id = entry.key().clone();
    let user_map = entry.value();
    for (role, sandbox) in &user_map.roles {
        if sandbox.status == Running && sandbox.last_activity.elapsed() >= timeout {
            to_hibernate.push((user_id.clone(), *role, ...));
        }
    }
}
// All DashMap guards dropped here

// Phase 2: Execute hibernations (no locks held)
for (user_id, role, ...) in to_hibernate {
    // Re-acquire briefly to check status hasn't changed
    if let Some(mut user_map) = self.entries.get_mut(&user_id) {
        if let Some(entry) = user_map.roles.get_mut(&role) {
            if entry.status == Running {
                entry.status = Hibernated;
                let handle = entry.handle.take();
                drop(user_map); // Release guard before async work
                if let Some(h) = handle {
                    self.hibernate_handle(&user_id, None, h).await;
                }
            }
        }
    }
}
```

### 2f. Convert count_running_vms and allocate_port

Both need to scan all entries. Use `self.entries.iter()`:

```rust
fn count_running_vms(entries: &DashMap<String, UserSandboxes>) -> usize {
    entries.iter().map(|e| /* count running in e.value() */).sum()
}

async fn allocate_port(&self) -> anyhow::Result<u16> {
    let mut used = HashSet::new();
    for entry in self.entries.iter() {
        for e in entry.value().roles.values() {
            used.insert(e.port);
        }
        for e in entry.value().branches.values() {
            used.insert(e.port);
        }
    }
    // ... scan for free port ...
}
```

Note: `allocate_port` currently takes `&HashMap` as parameter. Change
signature to take `&self` and read from `self.entries` directly.

### Verify

```bash
cargo test -p hypervisor
cargo clippy -p hypervisor
# Then run heterogeneous load test against local or staging
```

---

## Phase 3: HTTP Proxy Connection Pooling (1-2 hours)

### 3a. Create pooled client

**File:** `hypervisor/src/state.rs`

```rust
use hyper_util::client::legacy::Client as PooledClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

pub struct AppState {
    // ... existing fields ...
    pub proxy_client: PooledClient<HttpConnector, axum::body::Body>,
}
```

**File:** `hypervisor/src/main.rs`

```rust
let proxy_client = PooledClient::builder(TokioExecutor::new())
    .pool_idle_timeout(std::time::Duration::from_secs(30))
    .pool_max_idle_per_host(10)
    .build_http();
```

Note: Check the exact body type compatibility between axum and hyper-util.
The proxy currently uses `axum::body::Body` which implements `hyper::body::
Body`. May need a body adapter depending on versions.

### 3b. Refactor proxy_http

**File:** `hypervisor/src/proxy/mod.rs`

```rust
// Before (lines 37-69): manual TcpStream + handshake + spawn(conn)
let stream = TcpStream::connect(...).await?;
let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
tokio::spawn(conn);
let resp = sender.send_request(proxy_req).await?;

// After: pooled client
let resp = client.request(proxy_req).await?;
```

Keep the retry logic for initial connection failures. The pooled client
handles reconnection for evicted connections automatically, but a fresh
sandbox may not be listening yet.

### 3c. Thread through middleware

**File:** `hypervisor/src/middleware.rs`

Pass `state.proxy_client` to `proxy_http`. The middleware already has
access to `State(state)`.

### 3d. Leave WebSocket proxy unchanged

`proxy_ws` and `proxy_ws_raw` use `tokio_tungstenite` for long-lived
connections. No connection pooling benefit.

### Verify

```bash
cargo test -p hypervisor
# Test that proxied requests work: register user, health check, conductor prompt
```

---

## Phase 4: Dynamic VM Cap (30 min)

### 4a. Add config

**File:** `hypervisor/src/config.rs`

```rust
pub max_concurrent_vms: usize,  // from CHOIR_MAX_VMS, default 200
```

### 4b. Add to SandboxRegistry

**File:** `hypervisor/src/sandbox/mod.rs`

```rust
pub struct SandboxRegistry {
    // ... existing fields ...
    max_concurrent_vms: usize,
}

impl SandboxRegistry {
    fn effective_max_vms(&self) -> usize {
        let hard_cap = self.max_concurrent_vms;
        match read_memory_percent_available() {
            Some(pct) if pct > 60 => hard_cap,
            Some(pct) if pct > 30 => hard_cap * 3 / 4,
            Some(pct) if pct > 15 => hard_cap / 2,
            Some(_) => 0,
            None => hard_cap, // macOS fallback
        }
    }
}
```

### 4c. Add memory percent reader

**File:** `hypervisor/src/sandbox/mod.rs` (near existing `read_available_memory_mb`)

```rust
fn read_memory_percent_available() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = None;
    let mut available = None;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = line.split_whitespace().nth(1)?.parse::<u64>().ok();
        } else if line.starts_with("MemAvailable:") {
            available = line.split_whitespace().nth(1)?.parse::<u64>().ok();
        }
    }
    Some(available? * 100 / total?)
}
```

### 4d. Replace constant in capacity gate

```rust
// Before (line 311)
if running >= MAX_CONCURRENT_VMS {

// After
let effective_max = self.effective_max_vms();
if running >= effective_max {
    return Err(anyhow::anyhow!(
        "Server at capacity ({running}/{effective_max} VMs). \
         Please try again shortly."
    ));
}
```

### 4e. Nix config

**File:** `nix/hosts/ovh-node.nix`

Add to hypervisor service environment:
```nix
"CHOIR_MAX_VMS=200"
```

### Verify

```bash
cargo test -p hypervisor
# Check that effective_max_vms returns sensible values
```

---

## Phase 5: Metrics Endpoint + Stress Test (1-2 hours)

### 5a. Add /admin/metrics endpoint

**File:** `hypervisor/src/api/mod.rs`

```rust
/// GET /admin/metrics — system resource snapshot
pub async fn system_metrics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let snapshot = state.sandbox_registry.snapshot().await;
    let mut running = 0u32;
    let mut hibernated = 0u32;
    for user in snapshot.values() {
        for entry in user.roles.values() {
            match entry.status.as_deref() {
                Some("Running") => running += 1,
                Some("Hibernated") => hibernated += 1,
                _ => {}
            }
        }
    }

    let mem_available_mb = read_available_memory_mb();
    let mem_pct = read_memory_percent_available();
    let load_avg = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|v| v.to_string()));
    let ksm = read_ksm_stats();

    Json(serde_json::json!({
        "running_vms": running,
        "hibernated_vms": hibernated,
        "mem_available_mb": mem_available_mb,
        "mem_percent_available": mem_pct,
        "load_avg_1m": load_avg,
        "ksm_pages_sharing": ksm.map(|k| k.sharing),
        "ksm_pages_shared": ksm.map(|k| k.shared),
    }))
}
```

Route: `.route("/admin/metrics", get(api::system_metrics))`

### 5b. Add KSM reader

**File:** `hypervisor/src/sandbox/mod.rs` (or a new `metrics.rs`)

```rust
struct KsmStats { sharing: u64, shared: u64 }

fn read_ksm_stats() -> Option<KsmStats> {
    let sharing = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_sharing")
        .ok()?.trim().parse().ok()?;
    let shared = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_shared")
        .ok()?.trim().parse().ok()?;
    Some(KsmStats { sharing, shared })
}
```

### 5c. Update stress test

**File:** `tests/playwright/capacity-stress-test.spec.ts`

Add a `collectMetrics` helper that fetches `/admin/metrics` and returns
the JSON. Call at start and end of each wave. Add columns to wave result:

```typescript
interface SystemMetrics {
  running_vms: number;
  hibernated_vms: number;
  mem_available_mb: number | null;
  mem_percent_available: number | null;
  load_avg_1m: string | null;
  ksm_pages_sharing: number | null;
}

async function collectMetrics(page: Page): Promise<SystemMetrics | null> {
  try {
    const res = await page.request.get("/admin/metrics", { timeout: 5_000 });
    if (res.ok()) return await res.json();
  } catch { /* non-critical */ }
  return null;
}
```

Add to wave output:

```
Wave | +New | Total | Boot p50 | Health | MemAvail | Load | KSM | Status
```

### Verify

```bash
cargo test -p hypervisor
# Deploy to Node B, run stress test, verify metrics in output
```

---

## Files Summary

| Phase | File | Change |
|-------|------|--------|
| 1 | hypervisor/Cargo.toml | Add `dashmap = "6"` |
| 1 | hypervisor/src/state.rs | Rate limit type → DashMap |
| 1 | hypervisor/src/main.rs | DashMap init |
| 1 | hypervisor/src/provider_gateway.rs | DashMap entry API |
| 2 | hypervisor/src/sandbox/mod.rs | Registry Mutex → DashMap, idle watchdog restructure |
| 3 | hypervisor/src/state.rs | Add proxy_client to AppState |
| 3 | hypervisor/src/main.rs | Create pooled client |
| 3 | hypervisor/src/proxy/mod.rs | Use pooled client |
| 3 | hypervisor/src/middleware.rs | Thread client through |
| 4 | hypervisor/src/config.rs | CHOIR_MAX_VMS config |
| 4 | hypervisor/src/sandbox/mod.rs | Dynamic cap logic |
| 4 | nix/hosts/ovh-node.nix | CHOIR_MAX_VMS env var |
| 5 | hypervisor/src/api/mod.rs | /admin/metrics endpoint |
| 5 | hypervisor/src/sandbox/mod.rs | KSM + memory readers |
| 5 | tests/playwright/capacity-stress-test.spec.ts | Metrics collection |

---

## Verification Checklist

After all phases:

- [ ] `cargo test -p hypervisor` passes
- [ ] `cargo clippy -p hypervisor` clean
- [ ] Heterogeneous load test (16 users) passes on staging
- [ ] Capacity stress test v2 runs without registry contention spikes
- [ ] `/admin/metrics` returns valid JSON with resource data
- [ ] Stress test report includes per-wave resource columns
- [ ] Dynamic cap scales down when memory drops below thresholds
