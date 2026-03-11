# ADR-0007: 3-Tier Control/Runtime/Client Architecture

Date: 2026-02-28
Kind: Decision
Status: Draft
Priority: 2
Requires: []
Owner: Platform/Runtime

---

## Narrative Summary (1-minute read)

ChoirOS implements a 3-tier architecture separating concerns across three planes:

1. **Control Plane** (Hypervisor): Identity, routing, secrets broker, provider gateway
2. **Runtime Plane** (Per-User): MicroVMs per user with branch containers inside
3. **Client Plane**: Web/desktop frontends that authenticate via control plane

This replaces the previous role-based model (live/dev) with a branch-aware runtime model where users work on `main`, `dev`, or feature branches in isolated containers.

---

## What Changed

### From Role-Based to Branch-Aware

**Previous model**:
- Fixed roles: `live` (port 8080) and `dev` (port 8081)
- Shared runtime state
- No branch isolation

**Current model**:
- Route pointers: `main`, `dev`, `exp-*` that resolve to branches or roles
- Per-branch containers with isolated SQLite databases
- Dynamic port allocation (12000-12999 for branches)

### Key Architectural Decisions

1. **Pointer-based routing**: `/dev` is a compatibility alias to pointer "dev", not hardcoded
2. **VM-first runtime**: vfkit is the only supported local backend (process backend removed)
3. **Guest binary reuse**: `if-missing` mode avoids rebuilding on every ensure
4. **Ownership enforcement**: Never adopt pre-existing listeners on shared ports
5. **Writer is a living-document runtime**: run documents are represented by
   `draft.md` plus `draft.writer-state.json`; `.writer_revisions` is
   transitional compatibility state, not the target model

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CONTROL PLANE (Hypervisor)                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │   Identity   │  │    Route     │  │   Secrets    │  │     Provider    │ │
│  │   Service    │  │   Registry   │  │    Broker    │  │     Gateway     │ │
│  │  (WebAuthn)  │  │  (pointers)  │  │  (API keys)  │  │ (rate-limited)  │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └─────────────────┘ │
└─────────┼─────────────────┼─────────────────┼──────────────────────────────┘
          │                 │                 │
          │    ┌────────────┴─────────────────┴────────────┐
          │    │         RUNTIME PLANE (Per-User)          │
          │    │  ┌─────────────────────────────────────┐  │
          │    │  │         User MicroVM (vfkit)        │  │
          │    │  │   ┌─────────┐ ┌─────────┐ ┌──────┐  │  │
          │    │  │   │  main   │ │   dev   │ │feat-*│  │  │
          │    │  │   │container│ │container│ │  ... │  │  │
          │    │  │   └────┬────┘ └────┬────┘ └───┬──┘  │  │
          │    │  └────────┼───────────┼──────────┼──────┘  │
          │    └───────────┼───────────┼──────────┼─────────┘
          │                │           │          │
┌─────────┴────────────────┴───────────┴──────────┴──────────────────────────┐
│                              CLIENT PLANE                                   │
│                     (Web/Desktop/Mobile Frontends)                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Implementation Status

### ✅ Complete (Phase A)

| Component | File | Description |
|-----------|------|-------------|
| Runtime Registry | `hypervisor/src/runtime_registry.rs` | Pointer resolution, branch tracking |
| DB Schema | `hypervisor/migrations/0002_runtime_registry.sql` | user_vms, branch_runtimes, route_pointers, runtime_events |
| Sandbox Registry | `hypervisor/src/sandbox/mod.rs` | VM lifecycle, port allocation, idle watchdog |
| Route Middleware | `hypervisor/src/middleware.rs` | Pointer resolution, request sanitization, proxying |
| Admin APIs | `hypervisor/src/api/mod.rs` | Runtime control, pointer management |
| Vfkit Control | `hypervisor/src/bin/vfkit-runtime-ctl.rs` | MicroVM lifecycle management |
| VM Config | `nix/vfkit/user-vm.nix` | NixOS microVM configuration |
| Guest Control | `scripts/ops/vfkit-guest-runtime-ctl.sh` | Container management inside VM |
| E2E Tests | `tests/playwright/vfkit-cutover-proof.spec.ts` | Full flow with video artifacts |

### 🔄 In Progress (Phase B-C)

| Component | Status | Blocker |
|-----------|--------|---------|
| Control plane / runtime separation | Partial | Auth/session still in hypervisor |
| User VM lifecycle tracking | Stubbed | Table exists but not actively used |
| Guest agent API | Partial | Uses SSH instead of guest agent |

### ⏳ Not Started (Phase D-G)

| Component | Phase | Description |
|-----------|-------|-------------|
| Public Runtime API | D | User-facing API (not admin) |
| Identity service extraction | D | Standalone auth service |
| Secrets broker extraction | D | Standalone secrets service |
| Cloud-hypervisor backend | E | OVH/Linux backend |
| OVH deployment automation | F | Infrastructure as code |
| Control-plane-only VM | G | Separate control plane deployment |

---

## Core Concepts

### Writer Runtime Note

Within the runtime plane, Writer should be understood as a living-document
system rather than a generic file editor.

Current direction:

- the first user prompt is a real version,
- versions tell the story of the run,
- users, Writer, and future collaborators may author revisions,
- one commit path records accepted versions in order,
- worker updates usually enter as evidence, progress, artifacts, or proposals,
  not direct canonical diffs,
- run-scoped Writer persistence is `draft.md` plus `draft.writer-state.json`.

The active contract for this is maintained in
`docs/practice/guides/writer-api-contract.md`.

### Route Pointers

Named pointers that resolve to runtime targets:

```rust
pub enum PointerTarget {
    Role(SandboxRole),    // Legacy: live, dev
    Branch(String),       // New: main, dev, feature-x
}
```

Default pointers created on user registration:
- `main` -> `SandboxRole::Live` (port 8080)
- `dev` -> `SandboxRole::Dev` (port 8081)

### Branch Containers

Inside each user VM, NixOS containers provide isolation:

```
User VM (vfkit)
├── nixos-container@main (port 12000)
├── nixos-container@dev (port 12001)
└── nixos-container@feature-x (port 12002)
```

Each container has:
- Isolated filesystem
- Separate SQLite database
- Independent sandbox binary
- 30-minute idle timeout

### Middleware Routing

Request path determines target:

| Path | Pointer | Target |
|------|---------|--------|
| `/*` | `main` | Resolves to pointer target |
| `/dev/*` | `dev` | Resolves to pointer target |
| `/branch/<name>/*` | N/A | Direct branch access |

Headers added to proxied requests:
- `x-choiros-user-id`: Authenticated user
- `x-choiros-route-pointer`: Pointer name used
- `x-choiros-runtime`: Resolved backend (role or branch)

---

## Database Schema

```sql
-- VM lifecycle tracking
CREATE TABLE user_vms (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    backend TEXT NOT NULL,  -- vfkit | cloud-hypervisor
    state TEXT NOT NULL,    -- creating | ready | stopped | error
    host_ip TEXT,
    ssh_port INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Branch runtime instances
CREATE TABLE branch_runtimes (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    vm_id TEXT,
    branch_name TEXT NOT NULL,
    http_port INTEGER NOT NULL,
    state TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_activity_at DATETIME
);

-- Named pointers (main, dev, exp-*)
CREATE TABLE route_pointers (
    user_id TEXT NOT NULL,
    pointer_name TEXT NOT NULL,
    target_kind TEXT NOT NULL,  -- role | branch
    target_value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, pointer_name)
);

-- Audit log
CREATE TABLE runtime_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    user_id TEXT NOT NULL,
    detail_json TEXT NOT NULL,
    correlation_id TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

## API Surface

### Admin APIs (Current)

```
GET    /admin/sandboxes
POST   /admin/sandboxes/:user_id/:role/start
POST   /admin/sandboxes/:user_id/:role/stop
POST   /admin/sandboxes/:user_id/swap
POST   /admin/sandboxes/:user_id/branches/:branch/start
POST   /admin/sandboxes/:user_id/branches/:branch/stop
GET    /admin/sandboxes/:user_id/pointers
POST   /admin/sandboxes/:user_id/pointers/set
```

### Public Runtime API (Planned)

```
POST   /runtime/v1/users/{user}/branches/{branch}/ensure
DELETE /runtime/v1/users/{user}/branches/{branch}
POST   /runtime/v1/users/{user}/pointers/{pointer}/set
GET    /runtime/v1/users/{user}/topology
```

---

## Configuration

### Hypervisor Config

```yaml
# hypervisor/config.yaml
runtime:
  backend: vfkit  # or cloud-hypervisor
  vfkit:
    guest_address: 192.168.64.2
    ssh_port: 2222
    cores: 4
    memory: 8192
  idle_timeout_minutes: 30

routing:
  default_pointers:
    main: { role: live }
    dev: { role: dev }

ports:
  role_live: 8080
  role_dev: 8081
  branch_range_start: 12000
  branch_range_end: 12999
```

---

## Testing

### E2E Test Coverage

| Test | File | Coverage |
|------|------|----------|
| Vfkit Cutover Proof | `tests/playwright/vfkit-cutover-proof.spec.ts` | Auth, branch start, terminal proof |
| Branch Proxy | `tests/playwright/branch-proxy-integration.spec.ts` | Routing, pointer swaps |
| Desktop Suite | `tests/playwright/desktop-app-suite-hypervisor.spec.ts` | Full user flow |

### Manual Verification

```bash
# Start dev branch
just dev

# Verify NixOS guest
curl http://localhost:9090/dev/api/health

# Check container identity
# In terminal: cat /etc/os-release

# Swap pointers
curl -X POST http://localhost:9090/admin/sandboxes/{user}/swap
```

---

## Rollout Phases

### Phase A: Local Vfkit (✅ Complete)
- Vfkit microVM support
- Branch container lifecycle
- Route pointer resolution
- Middleware routing
- E2E test coverage

### Phase B: Public Runtime API (Next)
- User-facing API (not admin)
- API key authentication
- Rate limiting
- Documentation

### Phase C: Control Plane Split
- Extract identity service
- Extract secrets broker
- Hypervisor becomes pure routing

### Phase D: Guest Agent API
- In-VM agent for container CRUD
- Control plane manages agents
- Replace SSH with agent API

### Phase E: Cloud-Hypervisor Backend
- Linux/OVH backend adapter
- Same contracts, different hypervisor

### Phase F: OVH Deployment
- Infrastructure as code
- Automated host bring-up

### Phase G: Control-Plane-Only VM
- Separate control plane deployment
- Multi-region support

---

## Files and Locations

### Core Implementation

| Component | Path |
|-----------|------|
| Runtime Registry | `hypervisor/src/runtime_registry.rs` |
| Sandbox Registry | `hypervisor/src/sandbox/mod.rs` |
| Route Middleware | `hypervisor/src/middleware.rs` |
| Admin APIs | `hypervisor/src/api/mod.rs` |
| Vfkit Control | `hypervisor/src/bin/vfkit-runtime-ctl.rs` |
| VM Nix Config | `nix/vfkit/user-vm.nix` |
| Guest Scripts | `scripts/ops/vfkit-guest-runtime-ctl.sh` |
| DB Migrations | `hypervisor/migrations/0002_runtime_registry.sql` |

### Configuration

| Config | Path |
|--------|------|
| Hypervisor | `hypervisor/src/config.rs` |
| Main | `hypervisor/src/main.rs` |

### Tests

| Test | Path |
|------|------|
| Vfkit Proof | `tests/playwright/vfkit-cutover-proof.spec.ts` |
| Branch Proxy | `tests/playwright/branch-proxy-integration.spec.ts` |
| Desktop Suite | `tests/playwright/desktop-app-suite-hypervisor.spec.ts` |

---

## Related Documents

- `2026-02-28-3-tier-gap-closure-plan.md` - Gap analysis and phased implementation
- `2026-02-28-local-vfkit-architecture-review.md` - Vfkit-specific details
- `2026-02-28-wave-plan-local-to-ovh-bootstrap.md` - Deployment sequence
- `adr-0004-hypervisor-sandbox-ui-runtime-boundary.md` - UI runtime boundary

---

## Acceptance Criteria

1. ✅ User can start/stop branch runtimes via API
2. ✅ Route pointers resolve correctly to branches or roles
3. ✅ Middleware routes requests to correct backend
4. ✅ Idle timeout stops unused runtimes
5. ✅ E2E tests pass with video artifacts
6. ⏳ Public runtime API available
7. ⏳ Control plane split complete
8. ⏳ Cloud-hypervisor backend works
9. ⏳ OVH deployment automated

---

## Update Log

| Date | Change | Author |
|------|--------|--------|
| 2026-02-28 | Initial ADR creation | Platform team |

---

*This is a living document. Update as implementation progresses.*
