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

Next: extend to dev/prod VM pairs with promotion gates, and add a heavy job
queue for workloads that don't fit in lightweight user VMs.

## What Changed

- 2026-03-11: Restructured around dev/prod model and job queue. Added resource
  profiles and promotion implementation steps. Prior phase detail preserved
  where still relevant.
- 2026-03-09: Phase 4 complete on Node B. Per-user VMs verified with 4
  concurrent users on unique ports, IPs, MACs, and TAP devices.

## Phase Status

```
Phase 1 (host btrfs)              DONE — both nodes, @data subvolume
Phase 2 (per-user virtio-blk)     DONE — SystemdLifecycle creates subvol + data.img
Phase 3 (persistence)             DONE — data.img survives stop/start (virtio-blk)
Phase 4 (per-user routing)        DONE — dynamic ports, DHCP, per-user VMs on Node B
Phase 5 (idle watchdog)           DONE — hibernate + heartbeat
Phase 6 (build pool)              NOT STARTED
Phase 7 (promotion API)           NOT STARTED
Phase 8 (inter-agent comms)       NOT STARTED
Phase 9 (cross-node migration)    DEFERRED
```

## Phase 6: Build Pool

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
6. Capacity management: if all workers busy, queue jobs FIFO

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

## Phase 7: Promotion API

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

## Phase 8: Inter-Agent Communication via Hypervisor

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
