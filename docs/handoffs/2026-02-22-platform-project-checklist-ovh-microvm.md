# Platform Project Checklist: OVH + microvm.nix Hard Cutover

Date: 2026-02-22  
Owner: platform/runtime  
Status: approved direction, execution starting

## Narrative Summary (1-minute read)

We are executing a hard platform cutover from AWS/NixOS-container runtime to OVH bare metal with
`microvm.nix`, using `cloud-hypervisor` as the only VM backend. The hypervisor process remains
small and stable: lifecycle, routing, policy enforcement, and observability only. Stateful and
high-change services (auth, secrets broker, global memory) move out of the hypervisor boundary
into separate service VMs or machines. Each user receives one kernel-isolated microVM and that VM
runs multiple containers (`live`, `dev`, and optional sidecars). Host sharing is a first-class
optimization because user VMs share large amounts of identical code and Nix closures.

## What Changed

1. Hypervisor backend decision is locked:
   - `cloud-hypervisor` only.
   - No compatibility backends (`process`, `podman`, or dual hypervisor support).
2. Deployment topology decision is locked:
   - Two identical OVH servers instead of separate prod/dev classes.
   - Initial mode: active/passive failover; evolve to active/active after lease/routing hardening.
3. Isolation and packaging model is locked:
   - One user -> one microVM kernel boundary.
   - Multiple containers inside user VM (`live`, `dev`, optional jobs/memory adapters).
4. Platform boundary decision is locked:
   - Hypervisor stays small and stable.
   - Auth, secrets management, and global memory are split out of hypervisor runtime.
5. Performance plan is now explicit:
   - Add quantitative targets (SLO hypotheses) and benchmark gates.
   - Compare estimates against real OVH load tests and revise.

## What To Do Next

## Phase 0 - Lock Architecture Contract (No Fallback Paths)

- [ ] Publish a single runtime contract: `cloud-hypervisor` is mandatory.
- [ ] Remove runtime compatibility abstractions from roadmap text and implementation backlog.
- [ ] Define hard ownership boundaries:
  - [ ] Hypervisor: routing, VM/container lifecycle orchestration, policy checks, trace metadata.
  - [ ] Auth service: identity, WebAuthn, sessions.
  - [ ] Secrets service: provider-token mediation, audit, policy.
  - [ ] Memory service: global memory APIs, indexing, retrieval, compaction.
- [ ] Define canonical route model:
  - [ ] `/` -> user VM -> `sandbox-live`
  - [ ] `/dev/*` -> same user VM -> `sandbox-dev`

## Phase 1 - Infra Foundation on Two Identical Nodes

- [ ] Choose two identical OVH nodes for symmetry (capacity, failover, simpler ops).
- [ ] Bootstrap both hosts with reproducible NixOS + `microvm.nix`.
- [ ] Configure host networking for VM ingress and inter-node control traffic.
- [ ] Configure immutable host role labels and generation-based rollback.
- [ ] Prove node replacement workflow from clean host in bounded time.

## Phase 2 - Hypervisor Minimization and Service Split

- [ ] Refactor hypervisor into a thin control-plane binary:
  - [ ] auth/session delegation to auth service
  - [ ] secrets delegation to secrets service
  - [ ] memory delegation to memory service
- [ ] Remove any hidden business/state logic from hypervisor process.
- [ ] Add typed service-client contracts with strict timeout/retry budgets.
- [ ] Add circuit-breaker and explicit error mapping per downstream service.

## Phase 3 - User VM + In-VM Container Runtime

- [ ] Build user microVM base image (minimal NixOS profile).
- [ ] Add in-VM container runtime + per-VM ingress.
- [ ] Start `sandbox-live` and `sandbox-dev` containers in each user VM.
- [ ] Enforce container cgroup limits inside VM to prevent sibling starvation.
- [ ] Implement per-user VM lifecycle operations:
  - [ ] create
  - [ ] start
  - [ ] stop
  - [ ] recycle
  - [ ] health-check

## Phase 4 - Host Sharing and Artifact Strategy

- [ ] Define shared-path policy (read-only where possible) for common code/Nix closures.
- [ ] Measure cold-start and build-time deltas with sharing enabled vs disabled.
- [ ] Document allowed writable paths and persistence layout.
- [ ] Enforce drift controls so shared artifacts remain reproducible and auditable.

## Phase 5 - Security Hardening for Keyless Sandboxes

- [ ] Remove raw provider API keys from sandbox/container runtime env.
- [ ] Route provider access through secrets broker/gateway only.
- [ ] Enforce egress policy:
  - [ ] allow gateway and essential infra only
  - [ ] block direct provider egress from sandbox containers
- [ ] Add startup guard: sandbox fails fast if forbidden provider keys are present.
- [ ] Add provider mediation audit events (metadata, latency, outcome, policy decision).

## Phase 6 - Comprehensive Testing and Failure Drills

- [ ] Run contract and integration tests for route isolation (`session_id`, `thread_id`).
- [ ] Add ordered websocket integration tests for multi-instance streams.
- [ ] Add NixOS/microVM lifecycle integration tests under concurrent load.
- [ ] Run chaos drills:
  - [ ] VM crash
  - [ ] host restart
  - [ ] auth service outage
  - [ ] secrets service outage
  - [ ] memory service outage
- [ ] Run failover drill from node A to node B with measured RTO/RPO.

## Phase 7 - Hard Cutover and Deletion of Legacy Paths

- [ ] Execute maintenance-window cutover to OVH microVM runtime.
- [ ] Validate live traffic with on-call checklist and SLO dashboards.
- [ ] Remove legacy AWS compute/runtime paths from active stack.
- [ ] Remove old runtime codepaths from repository and deployment configs.
- [ ] Keep only required AWS usage (if any) for model endpoints.

---

## Target Runtime Architecture

```text
User Browser
  -> Hypervisor (small control plane)
      -> User MicroVM (kernel boundary)
          -> Guest ingress router
              -> sandbox-live container
              -> sandbox-dev container
              -> optional job sidecars

Auth, Secrets, and Global Memory run outside Hypervisor
(separate microVMs or separate machines).
```

## KS-2 vs KS-4 Planning Decision

As of 2026-02-22, our workload is concurrent multi-tenant sandbox execution. That favors thread
count over peak single-core speed.

Decision:

- Primary choice: core-dense node class (KS-2 profile) for both nodes.
- Avoid mixed-node topology unless benchmarks show a clear reason to split.

Rationale:

1. Per-user VM + multi-container workload competes on concurrent CPU slices.
2. Single-core build speed matters, but platform bottleneck is aggregate concurrency.
3. Two identical nodes simplify failover math, scheduling behavior, and capacity forecasting.

Note:

- OVH SKU naming/specs can vary by region and offer generation. Reconfirm exact CPU/RAM/network
  details at order time and update this document with the purchased SKU IDs.

## Performance Hypotheses (Initial SLO Targets)

These are starting estimates to test and refine, not guaranteed outcomes.

| Metric | Target p50 | Target p95 | Notes |
|---|---:|---:|---|
| User VM boot to healthy | < 8s | < 20s | measured from `create/start` request to health pass |
| Container start inside warm VM | < 1s | < 3s | includes ingress route registration |
| Route switch live/dev in same VM | < 100ms | < 250ms | control-plane only, no VM restart |
| Prompt -> first `actor_call` websocket chunk | < 1.5s | < 4s | normal load, no cold VM |
| Hypervisor control-plane CPU (steady) | < 40% | < 60% | reserve headroom for spikes/failover |
| Node-level active user capacity (initial) | 8 users | 12 users | medium workload estimate, validate by load test |

Failure budgets:

- [ ] Any p95 regression > 20% blocks rollout.
- [ ] Websocket stream error/drop rate > 0.5% blocks rollout.
- [ ] Cross-session event bleed incidence > 0 blocks rollout.

## Benchmark and Validation Program

## A. Microbenchmarks

- [ ] Host vs guest compile time (`cargo check`, targeted tests).
- [ ] Shared artifacts on/off impact (startup, disk, IO wait).
- [ ] Per-container memory pressure and CPU throttling behavior.

## B. Integration Benchmarks

- [ ] Prompt bar -> conductor -> terminal delegation latency under concurrency.
- [ ] Writer and websocket chunk ordering correctness with parallel user sessions.
- [ ] VM lifecycle thrash test (rapid start/stop/recycle).

## C. Soak and Chaos

- [ ] 24h soak with periodic VM/container restarts.
- [ ] Host reboot drill with workload recovery measurement.
- [ ] Node failover drill (active node loss) with recovery timing evidence.

## D. Security Verification

- [ ] Negative test: sandbox runs with no raw provider keys.
- [ ] Negative test: direct provider egress blocked from sandbox containers.
- [ ] Header and credential leak checks in proxy paths.

---

## Implementation Workstreams

## Workstream 1 - Hypervisor Runtime Refactor

- [ ] Remove `process` and `podman` runtime code from hypervisor.
- [ ] Implement VM lifecycle backend for `cloud-hypervisor` only.
- [ ] Replace static live/dev host ports with user VM route resolution.

## Workstream 2 - Contract and Event Schema

- [ ] Extend runtime telemetry with `vm_id` and `container_name`.
- [ ] Preserve canonical event flow: EventStore commit first, relay after commit.
- [ ] Add typed lifecycle events for VM/container and route transitions.

## Workstream 3 - Auth/Secrets/Memory Service Split

- [ ] Define API contracts for auth/secrets/memory.
- [ ] Move stateful logic out of hypervisor into dedicated services.
- [ ] Add health, retries, and explicit failure semantics for each dependency.

## Workstream 4 - Nix Image and Deployment Pipeline

- [ ] Build and version guest images declaratively.
- [ ] Add reproducible release artifacts and rollback pointers.
- [ ] Verify node bootstrap from documented commands only.

## Workstream 5 - Testing Harness Upgrade

- [ ] Expand Rust integration tests for scoped websocket streams.
- [ ] Add NixOS test harness for VM/container orchestration paths.
- [ ] Keep Playwright as canonical E2E for user flows.

---

## Ops Checklist (Runbook Summary)

## Pre-Cutover

- [ ] Two-node environment online and independently reproducible.
- [ ] Service split deployed and health-checked (auth/secrets/memory).
- [ ] Performance SLO baselines captured from staging load tests.
- [ ] Rollback command path tested on both nodes.

## Cutover Window

- [ ] Freeze non-essential deploys.
- [ ] Shift traffic to OVH microVM runtime.
- [ ] Validate auth, prompt-bar, writer, terminal, websocket streaming.
- [ ] Verify policy checks and keyless sandbox invariants.

## Post-Cutover (First 72h)

- [ ] Watch latency/error/failover dashboards continuously.
- [ ] Run one controlled failover drill.
- [ ] Close all severity-1/2 regressions before expanding load.
- [ ] Remove legacy runtime toggles and stale route logic.

---

## Acceptance Criteria (Project Complete)

- [ ] `cloud-hypervisor` is the only runtime backend in code and ops docs.
- [ ] Two identical OVH nodes run the production platform with tested failover.
- [ ] Hypervisor process is small (lifecycle/routing/policy/telemetry only).
- [ ] Auth, secrets, and global memory are outside hypervisor runtime boundary.
- [ ] Per-user microVM + in-VM multi-container model is live and stable.
- [ ] Sandboxes operate without raw provider API keys.
- [ ] Websocket and event isolation tests show zero cross-instance bleed.
- [ ] Performance targets are measured, tracked, and used as release gates.
