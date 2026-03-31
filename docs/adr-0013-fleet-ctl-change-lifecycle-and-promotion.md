# ADR-0013: Fleet-Ctl, Change Lifecycle, and User-to-Global Promotion

Date: 2026-03-06
Kind: Decision
Status: Draft
Priority: 3
Requires: []
Supersedes: Extends ADR-0010 (VM fleet lifecycle), integrates ADR-0011 (state/compute decoupling)
Authors: wiz + Claude

## Narrative Summary (1-minute read)

ChoirOS needs a unified model for how changes flow through the system — from a user editing
a prompt inside their sandbox, to that change being tested, approved, and promoted to the
global platform. This ADR defines the change lifecycle, the fleet-ctl architecture that
enables it, and the protocol for promoting user-scoped changes to global defaults.

The key insight: **every change follows the same cycle regardless of what layer it touches.**
A prompt tweak, a model swap, a Rust code fix, and a NixOS config change all go through:
make → test safely → human approve → promote → rollback if needed. What varies is the blast
radius and the time to apply, not the protocol.

## What Changed

- Renamed "hypervisor" to "control-plane" (the current binary is an app server, not a hypervisor)
- Proposed fleet-ctl as the actual VM/service lifecycle manager on bare metal
- Identified that platform services (auth, provider gateway) don't need microVMs — systemd
  services with blue/green binary deployment are sufficient
- MicroVMs reserved for user sandboxes (kernel isolation, snapshotting, per-user lifecycle)
- Defined change hierarchy: what should be runtime (Level 0) vs config reload (Level 1) vs
  binary swap (Level 2) vs VM rebuild (Level 3)
- Defined promotion protocol: user-scoped changes → testing → approval → global promotion

## What To Do Next

1. Write fleet-ctl MVP (evolve ovh-runtime-ctl.sh)
2. Make model selection and prompts runtime-configurable (Level 0)
3. Implement safe testing environment inside user VMs
4. Design and implement the promotion protocol

---

## 1. Change Hierarchy

Every behavioral change in ChoirOS sits at one of four levels, determined by what artifact
must change and how it's applied:

### Level 0: Runtime parameter (instant, no restart)

Changes that flow through the system as data. The binary doesn't change, config files don't
change, nothing restarts. The change takes effect on the next request.

What belongs here:
- Model selection per request/callsite
- Prompt text (system prompts, tool descriptions)
- Tool grants per agent (which tools an agent can use)
- Feature flags
- Callsite defaults (which model for which role)

What's currently wrong: Model catalog is a TOML file loaded at startup. System prompts are
string literals in adapter.rs. Changing a word in a prompt requires cargo build + deploy.
These should all be database/API-backed and changeable per-request.

### Level 1: Config reload (seconds, no rebuild)

Changes to operational parameters that require the process to re-read configuration but
don't require recompilation.

What belongs here:
- Provider definitions (URLs, auth endpoints, rate limits)
- Quota and budget limits
- Timeout budgets
- Harness configuration (max_steps, etc.)

Mechanism: SIGHUP handler or filesystem watcher reloads config. No binary change needed.

### Level 2: Binary swap (minutes, service restart)

Rust code changes that require recompilation. The binary changes, the service restarts,
but the VM/host doesn't change.

What belongs here:
- New tool implementations
- New actor types
- Protocol changes
- Bug fixes in application logic

Mechanism: cargo build → copy binary to staging path → health check → promote (swap binary,
restart service). Blue/green deployment with rollback.

### Level 3: Infrastructure change (slow, VM/host rebuild)

Changes to the operating system, kernel, system packages, or network configuration.

What belongs here:
- NixOS configuration changes
- Kernel updates
- System dependency changes
- Network topology changes

Mechanism: nix build → new VM image or nixos-rebuild → restart VM.

### Current state vs target

| Change type | Current level | Target level |
|---|---|---|
| Model selection | 2 (TOML in binary) | 0 (API parameter) |
| System prompts | 2 (hardcoded strings) | 0 (prompt registry) |
| Tool grants | 2 (hardcoded in adapter) | 0 (config per agent) |
| Callsite defaults | 2 (TOML in binary) | 0 (DB-backed, API-mutable) |
| Provider config | 2 (TOML in binary) | 1 (config file + reload) |
| Harness config | 2 (constants in code) | 1 (config file + reload) |
| Tool implementations | 2 | 2 (correct) |
| NixOS config | 3 | 3 (correct) |

Pushing changes down the hierarchy (from Level 2 to Level 0) is the single biggest
unlock for iteration speed.

---

## 2. Architecture: Fleet-Ctl and Service Topology

### Naming correction

The current "hypervisor" binary is an application server (auth, routing, provider gateway,
sandbox management). It's not a hypervisor. Rename:

| Current name | New name | Role |
|---|---|---|
| hypervisor (binary) | control-plane | Auth, API gateway, routing |
| (embedded in hypervisor) | provider-gateway | LLM proxy, upstream auth |
| ovh-runtime-ctl.sh | fleet-ctl | VM and service lifecycle manager |
| sandbox (binary) | sandbox (unchanged) | User workspace runtime |

### Service topology

```
Bare metal (managed by fleet-ctl):
  fleet-ctl           VM/service lifecycle, Caddy config management
  Caddy               TLS termination, health-based routing

Platform services (systemd, blue/green deploy):
  control-plane       Auth (WebAuthn), API gateway, admin endpoints
  provider-gateway    LLM provider proxy, key management, Bedrock rewrite

User workloads (microVMs, per-user lifecycle):
  sandbox-{user_id}   Per-user sandbox with kernel isolation
```

### Why this split

Platform services are trusted code written by the team. They don't need kernel isolation —
they need independent deployment and rollback. Systemd services with blue/green binary
paths give that at ~50-100MB RSS each, not 3GB.

User sandboxes run agent-generated code and need kernel isolation, per-user snapshotting,
and independent lifecycle. MicroVMs are correct here.

### Resource budget (64GB RAM OVH node)

```
Caddy + fleet-ctl:          ~100MB
control-plane (x2 b/g):     ~100MB
provider-gateway (x2 b/g):   ~60MB
Platform overhead:           ~260MB

User VMs (3GB each):
  20 active users:          ~60GB
  Remaining:                parked as snapshots on disk (4-6GB each)
```

---

## 3. Fleet-Ctl: Lifecycle Manager

Fleet-ctl is the only privileged process on bare metal besides Caddy. It manages two kinds
of workloads with different lifecycle operations:

### Platform services (systemd)

```
fleet-ctl service list
fleet-ctl service deploy <name> <binary-path>     # stage new version
fleet-ctl service promote <name>                   # swap stage → live
fleet-ctl service rollback <name>                  # swap back
fleet-ctl service status <name>
```

Implementation: blue/green systemd units. Each service has `-live` and `-stage` units on
different ports. Promote swaps the Caddy upstream and restarts.

### User VMs (cloud-hypervisor microVMs)

```
fleet-ctl vm create   --user <id> --role <role> --config <nix-config>
fleet-ctl vm start    --vm <id>
fleet-ctl vm stop     --vm <id>
fleet-ctl vm snapshot --vm <id>              # park to disk
fleet-ctl vm restore  --vm <id>              # wake from snapshot
fleet-ctl vm delete   --vm <id>
fleet-ctl vm list     [--user <id>]
fleet-ctl vm status   --vm <id>
```

VM state machine:
```
creating → stopped → running → stopping → stopped
                  ↘ pausing → snapshotted → restoring ↗
                                          → deleted
```

### Caddy integration

Fleet-ctl updates Caddy's upstream configuration when services promote/rollback or VMs
start/stop. Caddy's admin API allows runtime upstream changes without restart.

### Idle watchdog

**Current state (broken):** The hypervisor's `SandboxRegistry::run_idle_watchdog` kills VMs
after 30 minutes of inactivity. `last_activity` is only updated by proxy requests — not by
reading documents, browsing, or WebSocket keepalive. When the VM is killed, **all state is
lost** (no snapshotting). Next request triggers a full cold boot (~2 min) and a 502 error.

**Target state:** Fleet-ctl monitors user VM activity with multiple thresholds:
1. First threshold (e.g. 15 min): stop VM (fast restart from persistent state)
2. Second threshold (e.g. 2 hours): snapshot to disk (slow restore, saves RAM)
3. Third threshold (e.g. 7 days): delete snapshot (user must re-create)

**Critical prerequisites:**
- Sandbox state MUST be persistent (virtiofs shared dir or disk image) so VM stop doesn't lose data
- `last_activity` must be updated by frontend heartbeat, not just proxy requests
- Wake-on-request must return "loading" instead of 502 while VM boots
- WebSocket keepalive (ping/pong) to prevent silent connection death

Wake-on-request: user HTTP request → fleet-ctl detects no running VM → restore/start →
route when healthy.

---

## 4. The Inner Development Loop (Changes Inside ChoirOS)

When a user (or agent) makes a change inside their ChoirOS sandbox, the change needs safe
testing before it takes effect. "Inner changes" are NOT instant — they follow the same
test/approve/promote cycle, just within the user's scope.

### What "inner changes" means

From inside a running sandbox, a user or agent can change:
- Prompts (Level 0 when runtime-configurable)
- Model selection (Level 0)
- Tool grants (Level 0)
- Rust code (Level 2 — requires build + restart)

### Safe testing inside a user VM

For Level 0 changes (prompts, model config):
- Apply change to a staging config scope (not the live scope)
- Run the user's test suite against the staging config
- Show diff of behavior (e.g., same prompt, different model — compare outputs)
- User approves → promote staging config to live config

For Level 2 changes (code):
- Build new binary inside the VM (cargo build) or on host
- Start a second sandbox process on a staging port inside the same VM
- Run E2E tests against the staging port
- User approves → fleet-ctl swaps the binary, restarts the live sandbox process

The lightest test environment for code changes is a second process inside the same VM —
no nested VM or container needed. The sandbox binary is stateless enough that two copies
can coexist on different ports sharing the same filesystem.

For heavier isolation (testing NixOS changes, kernel-level differences), a nested
NixOS container (systemd-nspawn) inside the VM provides filesystem/network isolation
without the overhead of a nested microVM. But this is a rare case.

### Test environment hierarchy (lightest to heaviest)

| Test type | Environment | Overhead |
|---|---|---|
| Prompt/model change | Same process, staging config scope | ~0 |
| Code change | Second process, staging port | ~50MB RSS |
| System change | systemd-nspawn container in VM | ~200MB RSS |
| Kernel change | Nested VM (avoid if possible) | ~3GB RSS |

Default to the lightest environment that provides sufficient isolation for the change type.

---

## 5. The Outer Development Loop (User Changes → Global Promotion)

This is the hardest unsolved problem: how do changes made inside one user's sandbox become
global platform defaults?

### The problem

User A improves a system prompt for the writer agent. It works well in their sandbox.
They want to share it. Meanwhile:
- User B has their own prompt customizations
- The platform has a global default prompt
- Next week, the platform ships a new writer feature that changes the default prompt

This is a distributed version control problem for runtime configuration.

### Promotion protocol (proposed)

```
1. User makes change in their sandbox (scoped to user_id)
2. User marks change as "proposed for global" (creates a proposal)
3. Proposal includes:
   - What changed (diff of config/code/prompt)
   - Test results (automated E2E results from user's staging)
   - User's rationale
4. Platform operator reviews proposal
   - Can run proposal in a platform staging environment
   - Can A/B test: route N% of traffic to proposed change
5. Operator approves → change becomes new global default
6. Existing user customizations are NOT overwritten
   - Users who haven't customized this setting get the new default
   - Users who have customized it keep their version
   - Users can opt-in to "sync with global" for specific settings
```

### The fork/merge model

Each user's sandbox is conceptually a **fork** of the global platform configuration:

```
Global defaults (platform-owned):
  writer_prompt = "You are the ChoirOS Writer..."
  writer_model = "ClaudeBedrockSonnet46"
  max_steps = 5

User A's overrides (user-owned):
  writer_prompt = "You are a concise technical writer..."  (customized)
  writer_model = (inherits global)                         (not customized)
  max_steps = (inherits global)                            (not customized)

User B's overrides:
  writer_model = "ZaiGLM5"                                 (customized)
```

When global defaults change, only non-customized settings propagate to users.
This is the same model as CSS inheritance or git merge with conflict resolution.

### Intent reconciliation (from ADR-0011)

When a user's sandbox wakes from a snapshot, it may be behind the global platform state.
Reconciliation:
1. Compare user's last-seen global version with current global version
2. For each changed setting:
   - If user hasn't customized it → apply global update
   - If user has customized it → keep user's version, flag as divergent
3. User can review divergent settings and choose to sync or keep

This is the "inbound intent reconciliation" concept from ADR-0011, applied to config
inheritance rather than document content.

### What this requires (not yet built)

1. **Config registry with scoping**: global → user override layers
2. **Proposal/review workflow**: user proposes, operator reviews, system merges
3. **Version tracking**: which global version each user has seen
4. **Divergence detection**: which user settings conflict with global updates
5. **A/B testing infrastructure**: route percentage of traffic to proposed changes

### Pragmatic starting point

For bootstrapping (1-2 users who are also the operators), the promotion protocol
simplifies dramatically:

1. User makes a change in their sandbox
2. User tests it (staging process + E2E)
3. User approves it for their own sandbox (instant)
4. User pushes to git (becomes candidate for global)
5. CI stages it on the platform
6. User (who is also the operator) promotes it globally

The full proposal/review workflow is needed when users != operators. For now,
git + CI + fleet-ctl promote IS the promotion protocol.

---

## 6. CI/CD Integration

### Change detection

CI detects what changed and routes to the right deploy mode:

```
paths changed → deploy mode:
  docs/**, *.md                        → skip (paths-ignore, already done)
  sandbox/config/**, sandbox/baml_src/ → config deploy (scp + reload signal)
  sandbox/**, shared-types/**          → binary deploy (cargo build + stage + promote)
  hypervisor/**, → control-plane/**    → binary deploy (cargo build + stage + promote)
  nix/**, flake.nix, flake.lock        → infra deploy (nix build + VM rebuild)
  .github/**                           → CI self-test only
```

### Deploy modes

**Config deploy** (Level 0-1, seconds):
```
scp config files to server → signal reload → verify
```

**Binary deploy** (Level 2, minutes):
```
cargo build --release on server →
  fleet-ctl service deploy control-plane ./target/release/control-plane →
  run E2E against stage →
  fleet-ctl service promote control-plane
```

**Infra deploy** (Level 3, slow):
```
nix build on server →
  fleet-ctl vm create --config new-image →
  health check →
  fleet-ctl promote
```

### Rollback

Every deploy mode supports rollback:
- Config: revert file, signal reload
- Binary: fleet-ctl service rollback (swap back to previous binary)
- Infra: fleet-ctl vm restore (previous snapshot)

---

## 7. Build Sequence

### Wave 1: Push changes to runtime (Level 0)
- Model registry backed by DB/API instead of static TOML
- Prompt registry (files or DB) read per-request, not compiled in
- Tool grants configurable per-agent
- API endpoints to read/write these at runtime

### Wave 2: fleet-ctl MVP
- Evolve ovh-runtime-ctl.sh into fleet-ctl
- Add service deploy/promote/rollback for platform services
- Add VM snapshot/restore
- Caddy upstream management

### Wave 3: Rename and split
- Current hypervisor binary → control-plane
- Extract provider-gateway if justified
- Update CI, systemd units, Caddy config
- Blue/green systemd units for platform services

### Wave 4: Per-user VM allocation
- Dynamic VM creation with IP/port pool
- User login → ensure VM → route
- Idle watchdog → stop → snapshot

### Wave 5: Safe testing inside VMs
- Staging process/port for code changes inside user VMs
- E2E test runner against staging port
- Approve/promote UI in ChoirOS

### Wave 6: Promotion protocol
- Config scoping (global → user overrides)
- Proposal workflow (user → operator review)
- Divergence detection on wake
- A/B testing (percentage-based routing to proposed changes)

---

## 8. Existing Foundation (what's already built)

Before building, recognize what exists and build on it rather than reinventing:

### Sandbox registry (hypervisor/src/sandbox/mod.rs)
- Per-user scoping: `HashMap<String, UserSandboxes>` keyed by user_id
- Role sandboxes (live/dev) and branch sandboxes per user
- `swap_roles()` — swaps live↔dev assignments (primitive promote)
- `ensure_branch_running()` — starts a named branch sandbox on a dynamic port
- Idle tracking with `idle_secs` per sandbox

### Route pointers (hypervisor/src/runtime_registry.rs)
- `route_pointers` table: `(user_id, pointer_name, target_kind, target_value)`
- Default pointers: "main" → live, "dev" → dev
- Custom pointers: arbitrary string → role or branch target
- Foundation for the fork/merge model — user pointers already isolate routing

### Database schema (hypervisor/migrations/)
- `user_vms` table exists (unused): `(id, user_id, backend, state, host, metadata_json)`
- `branch_runtimes` table exists: `(id, user_id, vm_id, branch_name, role, port, state)`
- `runtime_events` table exists: audit trail for lifecycle events
- These tables are the persistence layer for fleet-ctl — wire them, don't recreate

### Viewer readonly support (sandbox/src/api/viewer.rs)
- `ViewerContentResponse` has `readonly: bool` field
- Foundation for runtime mode enforcement (RW_OWNER vs RO_PUBLISHED)

### ADR-0011 intent reconciliation (designed, not built)
ADR-0011 defines the full publishing model that this ADR's promotion protocol should
build toward. Key concepts to preserve:

- **Pointer semantics:** Each published target has `stable` (serving) and `candidate`
  (pending) pointers. Promotion = pointer flip. Rollback = flip back.
- **Inbound intents:** Reader prompts don't write directly to `stable`. They enqueue
  `inbound_intent` records with idempotency keys, scoped per-document.
- **Headless reconciler:** Wakes on schedule (hourly default), applies batched intents
  to `candidate`, runs validation policy, auto-promotes or queues for approval.
- **Runtime modes:** `RW_OWNER`, `RO_PUBLISHED`, `RO_PUBLISHED_WITH_PROMPT`, `FORKED_RW`

The promotion protocol in Section 5 of this ADR is the bootstrapping-phase simplification
of ADR-0011's full model. As the system matures:
- "User proposes change" → becomes `inbound_intent` enqueue
- "Operator reviews" → becomes reconciler + validation policy
- "Global promotion" → becomes `candidate → stable` pointer flip
- "User overrides persist" → becomes `FORKED_RW` mode

### What's missing (the actual gaps)
1. **VM state lost on restart (FATAL)** — idle watchdog kills VM, all data gone, no persistence
2. **`last_activity` broken** — only updated by proxy, not by browsing/reading/WS keepalive
3. **No WebSocket keepalive** — connections die silently, users see "Sandbox disconnected"
4. **502 on cold boot** — `ensure_running()` returns before VM is healthy, proxy fails
5. fleet-ctl doesn't exist — ovh-runtime-ctl.sh has only ensure/stop
6. No snapshot/restore (cloud-hypervisor supports it, not wired)
7. No dynamic VM creation (static two-VM topology)
8. No stable/candidate pointer distinction in route_pointers
9. No intent queue or reconciler
10. No runtime mode enforcement
11. Model catalog and prompts are static (compiled in or loaded once at startup)

## 9. Open Questions

1. **fleet-ctl: script or binary?** Shell script is simpler to start. Rust binary gives
   type safety and can expose an HTTP API. Start as script, graduate to binary when the
   API surface stabilizes?

2. **Prompt storage: files or DB?** Files are simpler (git-tracked, editor-friendly).
   DB enables per-user overrides and version history. Could start with files, add DB layer
   for user overrides.

3. **Config scoping granularity:** Per-user? Per-session? Per-run? More granularity =
   more complexity. Start with per-user overrides on global defaults.

4. **Cargo build location:** Build on host and share binary via virtiofs? Or build inside
   VM? Host build is faster (shared nix cache), VM build is more isolated.

5. **How does the operator UI for promotion work?** CLI? Web UI in ChoirOS? GitHub PR
   review? For bootstrapping, git + CLI is fine. For multi-user, needs a UI.

## 10. Verification Criteria

- [ ] Model selection changeable via API without restart
- [ ] System prompt changeable without rebuild
- [ ] fleet-ctl can deploy/promote/rollback a platform service
- [ ] fleet-ctl can snapshot/restore a user VM
- [ ] E2E tests run against staged version before promotion
- [ ] Rollback restores previous version within 30 seconds
- [ ] User config overrides persist across VM snapshot/restore
- [ ] Global config update propagates to users who haven't customized
