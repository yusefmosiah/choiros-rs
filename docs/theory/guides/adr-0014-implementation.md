# Implementing ADR-0014: Per-User VM Lifecycle and Storage

Date: 2026-03-06
Kind: Guide
Status: Active
Priority: 2
Requires: [ADR-0014]
Updated: 2026-03-11

## Narrative Summary (1-minute read)

Per-user VM isolation is deployed (Phases 1-5). Each user gets a
cloud-hypervisor microVM with dynamic port, DHCP IP, per-user btrfs
subvolume, and idle hibernation.

Next: implement machine classes so VM type is runtime configuration (not
hardcoded nix), test all 4 VM types via E2E evals, then build the tier-aware
job queue and promotion pipeline.

## What Changed

- 2026-03-11: Added Phase 6 (machine classes). VM type is a named runtime
  config with account tier mapping. Shifted build pool to Phase 7, promotion
  to Phase 8, inter-agent to Phase 9. Job queue is now tier-aware.
- 2026-03-11: Restructured around prod VM + build pool model. Added resource
  profiles and promotion implementation steps.
- 2026-03-09: Phase 4 complete on Node B. Per-user VMs verified with 4
  concurrent users on unique ports, IPs, MACs, and TAP devices.

## Phase Status

```
Phase 1 (host btrfs)              DONE — both nodes, @data subvolume
Phase 2 (per-user virtio-blk)     DONE — SystemdLifecycle creates subvol + data.img
Phase 3 (persistence)             DONE — data.img survives stop/start (virtio-blk)
Phase 4 (per-user routing)        DONE — dynamic ports, DHCP, per-user VMs on Node B
Phase 5 (idle watchdog)           DONE — hibernate + heartbeat
Phase 6 (machine classes)         NOT STARTED
Phase 7 (build pool)              NOT STARTED
Phase 8 (promotion API)           NOT STARTED
Phase 9 (inter-agent comms)       NOT STARTED
Phase 10 (cross-node migration)   DEFERRED
```

## Phase 6: Machine Classes

### What changes

VM type becomes runtime configuration instead of compile-time nix. The
hypervisor reads a machine class config file, and `ensure()` takes a class
name to select the right runner, systemd template, and sizing.

### Config format

Nix generates this from the deployed runners:

```toml
# /opt/choiros/config/machine-classes.toml

[classes.ch-blk-2c-1g]
hypervisor = "cloud-hypervisor"
transport = "blk"
vcpu = 2
memory_mb = 1024
runner = "/nix/store/...-choiros-ch-sandbox-live/bin/microvm-run"
systemd_template = "cloud-hypervisor"

[classes.ch-pmem-2c-1g]
hypervisor = "cloud-hypervisor"
transport = "pmem"
vcpu = 2
memory_mb = 1024
runner = "/nix/store/...-choiros-ch-sandbox-live/bin/microvm-run"
systemd_template = "cloud-hypervisor"

[classes.fc-blk-2c-1g]
hypervisor = "firecracker"
transport = "blk"
vcpu = 2
memory_mb = 1024
runner = "/nix/store/...-choiros-fc-sandbox-live/bin/microvm-run"
systemd_template = "firecracker"

# ... more classes as needed

[tier-defaults]
# Placeholder — real mappings emerge from experimentation
# free = "..."
# pro = "..."

[host]
default_class = "ch-blk-2c-1g"  # fallback until tiers are configured
nix_generation = "/nix/var/nix/profiles/system"  # for snapshot invalidation
```

### Files to modify

| File | Change |
|------|--------|
| `hypervisor/src/config.rs` | Parse machine class config (TOML) |
| `hypervisor/src/sandbox/systemd.rs` | `ensure()` takes class name, reads config, picks runner |
| `hypervisor/src/sandbox/mod.rs` | Pass machine class through ensure_running() |
| `hypervisor/src/db/mod.rs` | Store machine_class on user_vms |
| `hypervisor/migrations/0003_machine_classes.sql` | Add machine_class column to user_vms |
| `nix/hosts/ovh-node.nix` | Generate machine-classes.toml from deployed runners |
| `nix/hosts/ovh-node.nix` | Add firecracker@ systemd template |
| `nix/hosts/ovh-node.nix` | Generation-aware snapshot invalidation |

### Implementation steps

All nix builds happen in CI or on the server (x86_64-linux), never locally
on the dev Mac.

**Step 1: Verify FC runners build (CI)**

The FC nixosConfigurations exist in flake.nix but have never been built.
First step is verifying they evaluate and build:

```bash
# On server or in CI (x86_64-linux only)
nix build .#nixosConfigurations.choiros-fc-sandbox-live-blk.config.microvm.runner.firecracker
nix build .#nixosConfigurations.choiros-fc-sandbox-live.config.microvm.runner.firecracker
```

If this fails, debug the microvm.nix fork's firecracker runner. The guest
config (`sandbox-vm.nix`) already passes `sandboxHypervisor` to
`microvm.hypervisor` — the guest side should be hypervisor-agnostic.

**Step 2: Extract all runner paths (flake.nix)**

Currently only `vmRunnerLive` (CH pmem) is extracted. Need all 4:

```nix
vmRunnerChPmem = self.nixosConfigurations.choiros-ch-sandbox-live
  .config.microvm.runner.cloud-hypervisor;
vmRunnerChBlk = self.nixosConfigurations.choiros-ch-sandbox-live-blk
  .config.microvm.runner.cloud-hypervisor;
vmRunnerFcPmem = self.nixosConfigurations.choiros-fc-sandbox-live
  .config.microvm.runner.firecracker;
vmRunnerFcBlk = self.nixosConfigurations.choiros-fc-sandbox-live-blk
  .config.microvm.runner.firecracker;
```

**Step 3: Generate machine-classes.toml (nix)**

Nix generates the config with resolved store paths. Add to ovh-node.nix:

```nix
environment.etc."choiros/machine-classes.toml".text = ''
  [classes.ch-pmem-2c-1g]
  hypervisor = "cloud-hypervisor"
  transport = "pmem"
  vcpu = 2
  memory_mb = 1024
  runner = "${vmRunnerChPmem}"
  systemd_template = "cloud-hypervisor"

  [classes.ch-blk-2c-1g]
  hypervisor = "cloud-hypervisor"
  transport = "blk"
  vcpu = 2
  memory_mb = 1024
  runner = "${vmRunnerChBlk}"
  systemd_template = "cloud-hypervisor"

  # ... fc classes similarly

  [host]
  default_class = "ch-blk-2c-1g"
  nix_generation = "${config.system.build.toplevel}"
'';
```

**Step 4: Add firecracker@ systemd template (nix)**

Parallel to the existing `cloud-hypervisor@` template in ovh-node.nix.
Firecracker uses a JSON config file instead of CLI flags, so the template
reads the runner's generated config and patches it (similar to how CH
template sed-rewrites the microvm-run script for MAC/TAP/etc).

**Step 5: Runtime vcpu/memory via sed rewriting**

The microvm-run script has `--cpus boot=2` and `--memory size=1024M` baked
in at nix build time. The systemd template already rewrites MAC, TAP, etc.
Add vcpu/memory rewriting:

```bash
# cloud-hypervisor@ template reads from state dir
VCPU=$(cat "${STATE_DIR}/machine-vcpu" 2>/dev/null || echo "2")
MEM_MB=$(cat "${STATE_DIR}/machine-memory-mb" 2>/dev/null || echo "1024")
sed -i "s/boot=[0-9]*/boot=${VCPU}/" "${STATE_DIR}/.microvm-run"
sed -i "s/size=[0-9]*M/size=${MEM_MB}M/" "${STATE_DIR}/.microvm-run"
```

The Rust `ensure()` writes these files based on the machine class config.

**Step 6: SQLite migration**

```sql
ALTER TABLE user_vms ADD COLUMN machine_class TEXT;
```

**Step 7: Rust config parsing and ensure() plumbing**

- Parse machine-classes.toml on hypervisor startup
- `ensure()` accepts class name, looks up config
- Writes vcpu, memory, runner path to state dir for systemd to read
- Starts the right systemd template based on `systemd_template` field
- Falls back to `host.default_class` if no class specified

**Step 8: Generation-aware snapshot invalidation**

Replace the current "wipe all snapshots on hypervisor restart" with:

```bash
CURRENT_GEN=$(readlink /nix/var/nix/profiles/system)
for state_dir in /opt/choiros/vms/state/*/; do
  SAVED_GEN=$(cat "${state_dir}/nix-generation" 2>/dev/null || echo "")
  if [[ "$CURRENT_GEN" != "$SAVED_GEN" ]]; then
    rm -rf "${state_dir}/vm-snapshot"
    echo "$CURRENT_GEN" > "${state_dir}/nix-generation"
  fi
done
```

Same-generation restarts preserve snapshots. New generations invalidate.

**Step 9: Admin API for testing**

```
PUT /admin/users/{user_id}/machine-class
Body: { "machine_class": "fc-blk-2c-1g" }
```

Sets the class for next VM boot. Existing VM must stop first.

**Step 10: E2E eval**

Playwright test: register 4 users → set different class per user via admin
API → trigger sandbox boot for each → health check all 4 → verify all
running concurrently on different hypervisors/transports.

### VM type and sizing exploration (after machine classes work)

With machine classes deployed, experimentation is config + E2E:

1. Add a new class to the TOML (e.g., `ch-pmem-1c-256m`)
2. Push via CI → nix generates updated config → deploy to Node B
3. Set test users to new class via admin API
4. Run stress tests with mixed classes (existing Playwright suite)
5. Measure: boot time, memory overhead, host memory per VM, performance
6. Record results as a report in `docs/state/reports/`

Key experiments to run:
- **CH vs FC overhead**: Same sizing, different hypervisor. Which is lighter?
- **blk vs pmem memory savings**: Confirm ADR-0018's ~80MB/VM savings at scale
- **Minimum viable size**: Sweep down from 1GB — 512, 256. Where does it break?
- **Mixed class contention**: Run 30 small + 10 large VMs. Does packing work?
- **FC snapshot**: Does firecracker support snapshot/restore? Different semantics?

Each experiment produces a report that informs tier→class mapping decisions.

## Phase 7: Build Pool

### What changes

Add a shared pool of worker VMs that execute build, test, and coding agent
jobs on behalf of users. Users do not compile in their own sandbox VM.

### Architecture

```
User sandbox → POST /jobs/v1/run → hypervisor
Hypervisor → snapshot user workspace → create pool job VM → execute
Pool VM → stream events → user's EventStore
Pool VM → complete → artifacts ready for promotion
```

### Files to modify

| File | Change |
|------|--------|
| `hypervisor/src/jobs/mod.rs` | New module: job queue, submission, scheduling |
| `hypervisor/src/jobs/pool.rs` | Pool worker management: create, recycle, destroy |
| `hypervisor/src/jobs/executor.rs` | Job execution: snapshot, mount, run, stream, cleanup |
| `hypervisor/src/api/jobs.rs` | HTTP endpoints: submit, status, cancel |
| `hypervisor/src/sandbox/systemd.rs` | Pool VM lifecycle (ephemeral instances) |

### Implementation steps

1. Add job queue data model (SQLite: jobs table with status, owner, profile,
   command, created_at, started_at, completed_at)
2. Add `POST /jobs/v1/run` endpoint — accepts job type, command, resource
   profile. Returns job_id and queue position.
3. Pool manager: maintain N warm worker VMs (configurable, default 2).
   Workers are generic — they receive a workspace snapshot and a command.
4. Job executor:
   a. btrfs snapshot user's data.img (read-only)
   b. Mount snapshot into pool worker VM as /workspace (read-only)
   c. Mount scratch volume for output (writable)
   d. Execute command inside pool VM
   e. Stream stdout/stderr as events to user's EventStore
   f. Collect artifacts from scratch volume
   g. Update job status
   h. Release worker back to pool
5. Add `GET /jobs/v1/{id}` for status and `DELETE /jobs/v1/{id}` for cancel
6. Capacity management: priority queue by tier, FIFO within tier
7. Tier budget enforcement:
   - Read user's tier from DB
   - Check concurrent job limit, daily job count, max duration for tier
   - Reject with 429 if budget exceeded
   - Machine class for job VM also tier-dependent (config-driven)

### Resource profiles

| Profile | RAM | vCPU | Max duration | Use case |
|---------|-----|------|-------------|----------|
| `light` | 1 GB | 1 | 30 min | Light tests, linting |
| `standard` | 2 GB | 2 | 30 min | Cargo build, Playwright |
| `heavy` | 4 GB | 4 | 60 min | Large builds, parallel tests |

### Security

- Pool VMs get read-only workspace snapshot, not write access to user data
- Pool VMs isolated from each other and from user VMs
- Time limits enforced by hypervisor (kill after max duration)
- Artifacts returned via EventStore, not direct filesystem access

## Phase 8: Promotion API

### Endpoint

```
POST /v1/vms/{vm_id}/promote
  Body: {
    "job_id": "...",           // completed build pool job
    "verification": {
      "tests_passed": bool,
      "docs_updated": bool
    }
  }
  Response: { "promotion_id": "...", "status": "promoting" }
```

### Implementation steps

1. Add `POST /v1/vms/{vm_id}/promote` endpoint
2. Verify the referenced job completed successfully
3. Check verification gate:
   - If verification metadata missing or incomplete, reject with 422
   - In bootstrap phase, allow manual override with explicit flag
4. Snapshot user's current data.img (btrfs, <1s) as rollback point
5. Apply job artifacts to user's workspace
6. Stop or hibernate user's sandbox VM
7. Swap data.img with promoted version (reflink copy)
8. Start sandbox VM
9. Health check (curl /health within 30s)
10. On success: emit promotion event, return 200
11. On failure: restore from rollback snapshot, restart, return 500

### Rollback

Pre-promotion snapshot always retained. `POST /v1/vms/{vm_id}/rollback`
swaps back and restarts.

### Verification gate phases

Phase 1 (bootstrap): manual. User clicks "promote" and confirms.
Phase 2 (harness integration): automatic. Promotion API checks that the
build pool job completed with code+tests+docs passing.
Phase 3 (full enforcement): promotion blocked unless harness reports success.

## Phase 9: Inter-Agent Communication via Hypervisor

### Design

All cross-sandbox communication goes through the hypervisor. No VM-to-VM
networking.

### Endpoints

```
POST /v1/publish                    publish a document/artifact
GET  /v1/published?query=...        discover published content
GET  /v1/published/{doc_id}         retrieve published document
```

### Implementation steps

1. Add published documents table (SQLite: doc_id, owner_id, title, content,
   metadata, published_at, updated_at)
2. Add publish endpoint — sandbox writer calls this to publish living docs
3. Add discovery endpoint — other sandboxes can search published content
4. Add retrieval endpoint — fetch full document by ID
5. Add citation tracking — when User B cites User A's doc, record the link
6. Access control: public by default, private/unlisted options later

This is the foundation for writer-to-writer communication across users.
Writers publish through the hypervisor and retrieve through the hypervisor.
No direct connections between sandboxes.

## Current Architecture Reference (Phases 1-5, deployed)

### Per-user routing (Phase 4, DONE)

```
User A → proxy → 127.0.0.1:12000 → socat → 10.0.0.102:8080 → VM-A
User B → proxy → 127.0.0.1:12001 → socat → 10.0.0.103:8080 → VM-B
```

- Dynamic port allocation from range (12000+)
- Per-user systemd instances: `cloud-hypervisor@u-{short_id}`, `socat-sandbox@u-{short_id}`
- Per-user TAP devices, DHCP IPs, MAC addresses
- Sandbox inside VM always listens on :8080 (unchanged)

### Key files

| File | Role |
|------|------|
| `hypervisor/src/sandbox/mod.rs` | ensure_running(), port allocation, registry |
| `hypervisor/src/sandbox/systemd.rs` | SystemdLifecycle: btrfs, data.img, systemd units |
| `nix/hosts/ovh-node.nix` | Systemd unit templates, networking, DHCP |
| `nix/ch/sandbox-vm.nix` | Guest NixOS config |

### Verified on Node B

- 4 concurrent users on unique ports (12000-12004), IPs (10.0.0.102-106)
- Per-user btrfs subvolumes, data.img, systemd units
- Idle hibernation per-user with heartbeat watchdog
- Data persistence across stop/start and hibernate/restore

## What NOT to Do

- Don't build desktop sync yet (Phase 9+, deferred)
- Don't build multi-node placement or autoscaling
- Don't add quota enforcement until multitenancy requires it
- Don't build per-user NixOS guest configs — use DHCP
- Don't give job VMs write access to user data
