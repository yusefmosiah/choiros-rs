# Implementing ADR-0014: Per-User VM Lifecycle and Storage

Date: 2026-03-06
Kind: Guide
Status: Active
Priority: 2
Requires: [ADR-0014]
Updated: 2026-03-09

## Narrative Summary (1-minute read)

Per-user VM isolation requires each authenticated user to get their own
cloud-hypervisor microVM with its own virtio-blk data volume on a per-user
btrfs subvolume. The hypervisor dynamically allocates ports and VM IPs,
routes proxy traffic to the right VM based on session auth, and manages
idle hibernation per-user.

Phases 1-3 and 5 are done. Phase 4 (per-user routing + dynamic VM
allocation) is the remaining critical work. Phase 6 (migration) is deferred.

## What Changed

- 2026-03-09: Phases 1-3, 5 complete. Guide rewritten for Phase 4 focus.
- ADR-0017 (systemd lifecycle) deployed — `SystemdLifecycle` in Rust handles
  btrfs subvolumes, data.img creation, and systemd unit management.
- Hardcoded `live`/`dev` instance names and fixed ports are the remaining
  blocker. Phase 4 replaces them with per-user dynamic allocation.

## Phase Status

```
Phase 1 (host btrfs)              ✓ DONE — both nodes, @data subvolume
Phase 2 (per-user virtio-blk)     ✓ DONE — SystemdLifecycle creates subvol + data.img + symlink
Phase 3 (persistence)             ✓ DONE — data.img survives stop/start (virtio-blk)
Phase 4 (per-user routing)        ← THIS IS THE WORK
Phase 5 (idle watchdog)           ✓ DONE — hibernate + heartbeat
Phase 6 (cross-node migration)    deferred — operator tooling
```

## Phase 4: Per-User VM Routing

### Current Architecture (shared VM)

```
All users → proxy → 127.0.0.1:8080 → socat → 10.0.0.10:8080 → single VM
```

- `ensure_running()` returns fixed `live_port` (8080) for all users
- One systemd instance: `socat-sandbox@live`, `cloud-hypervisor@live`
- One VM IP: `10.0.0.10`, one data.img

### Target Architecture (per-user VM)

```
User A → proxy → 127.0.0.1:12000 → socat → 10.0.0.102:8080 → VM-A
User B → proxy → 127.0.0.1:12001 → socat → 10.0.0.103:8080 → VM-B
```

- `ensure_running()` allocates a dynamic port per user from the port range
- Per-user systemd instance: `socat-sandbox@u-{short_id}`
- Per-user VM IP: `10.0.0.{102+N}`, per-user data.img on btrfs subvol
- Sandbox inside VM always listens on `:8080` (unchanged)

### Implementation Steps

#### Step 1: Dynamic port allocation for roles

**File:** `hypervisor/src/sandbox/mod.rs`

Change `ensure_running()` to allocate ports dynamically instead of using
fixed `live_port`/`dev_port`. Reuse the existing `allocate_branch_port()`
logic (which already works for branch sandboxes).

```rust
// Before: let port = self.port_for(role);  // always 8080
// After:  let port = self.allocate_port(&entries)?;  // dynamic from range
```

Keep `live_port` as a special case only for the "default" bootstrap user
(unauthenticated requests before login). Authenticated users get dynamic ports.

#### Step 2: Per-user systemd instance naming

**File:** `hypervisor/src/sandbox/systemd.rs`

The `ensure()` method takes `instance: &str`. Currently always called with
`"live"`. Change to generate per-user instance names:

```rust
// Instance name: "u-{first 8 chars of user_id}"
// e.g., user_id "ab121631-c30f-4ff2-860f-7c3d230f3a30" → instance "u-ab121631"
```

Short IDs are safe because:
- systemd unit names have length limits
- Collision probability is negligible at our scale (<1000 users)
- The registry maps user_id → instance, so collisions are detectable

#### Step 3: Per-user VM IP allocation

**File:** `hypervisor/src/sandbox/systemd.rs`

Each VM needs a unique IP on `br-choiros` (10.0.0.0/24). Derive from port:

```rust
// Port 8080 → IP 10.0.0.100 (default live), 8081 → 10.0.0.101 (default dev)
// Port 12000 → IP 10.0.0.102, Port 12001 → IP 10.0.0.103, etc.
// All IPs in dnsmasq DHCP range (10.0.0.100-254), ~153 concurrent VMs per node
```

Write the VM IP to a config file in the state dir so systemd units can read it.

#### Step 4: Parameterize systemd units

**File:** `nix/hosts/ovh-node.nix`

The socat and cloud-hypervisor units currently hardcode port/IP per instance
name (`live*` → 8080, `dev*` → 8081). Change to read from config files:

```bash
# In socat-start script:
STATE_DIR="/opt/choiros/vms/state/${INSTANCE}"
VM_IP=$(cat "${STATE_DIR}/vm-ip")
PORT=$(cat "${STATE_DIR}/host-port")
```

The Rust `SystemdLifecycle.ensure()` writes these files before starting units.

#### Step 5: Dynamic guest NixOS config

**File:** `flake.nix`, `nix/ch/sandbox-vm.nix`

Currently one guest config (`sandbox-live`) with hardcoded MAC/IP. For
per-user VMs, the guest networking must match the allocated IP. Options:

**Option A (simple):** DHCP inside the VM. The host bridge runs a DHCP
server (systemd-networkd can do this) that assigns IPs based on MAC.
The Rust code generates a unique MAC per instance.

**Option B (current pattern):** Static IP in guest config. This requires
building a new NixOS guest config per unique IP, which is expensive.

**Recommended: Option A.** The sandbox binary doesn't care what IP it has.
The socat forwarding happens on the host side. Guest DHCP is simpler than
per-user NixOS rebuilds.

#### Step 6: TAP device per user

**File:** `nix/hosts/ovh-node.nix` (tap-setup@ unit)

The `tap-setup@` unit already creates `tap-{instance}`. With per-user
instance names (`u-ab121631`), it creates `tap-u-ab121631`. This works
as-is — just verify TAP name length limits (max 15 chars for Linux
interface names, `tap-u-ab121631` = 14 chars, fits).

#### Step 7: microvm-run per user

The `cloud-hypervisor@` unit uses `${vmRunnerLive}/bin/microvm-run` which
embeds the guest NixOS config (including MAC and static IP). For per-user
VMs with DHCP, we need a single guest config that uses DHCP instead of
static IP. Build one guest config, share it across all instances.

### Files to Modify (Summary)

| File | Change |
|------|--------|
| `hypervisor/src/sandbox/mod.rs` | Dynamic port allocation for roles |
| `hypervisor/src/sandbox/systemd.rs` | Per-user instance names, VM IP allocation, write config files |
| `nix/hosts/ovh-node.nix` | Parameterize socat/CH units to read port/IP from config files |
| `nix/ch/sandbox-vm.nix` | Switch from static IP to DHCP |
| `flake.nix` | Single DHCP guest config instead of per-role configs |

### Verification

1. Register two test users on draft.choir-ip.com
2. Log in as user A → gets VM on port 12000, IP 10.0.0.100
3. Log in as user B → gets VM on port 12001, IP 10.0.0.101
4. Verify isolation: user A cannot see user B's data
5. Idle timeout: user A's VM hibernates independently of user B
6. Stop user A → user B unaffected

### What NOT to Do

- Don't build desktop sync yet
- Don't build multi-node placement or autoscaling
- Don't add quota enforcement until multitenancy requires it
- Don't build a full VM fleet API — keep using ensure_running() pattern
- Don't per-user NixOS guest builds — use DHCP
