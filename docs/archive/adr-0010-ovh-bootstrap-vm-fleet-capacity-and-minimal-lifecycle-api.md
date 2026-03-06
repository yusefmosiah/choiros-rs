# ADR-0010: OVH Bootstrap VM Fleet Capacity and Minimal 80/20 Lifecycle API

Date: 2026-03-02
Kind: Decision
Status: Proposed
Priority: 5
Requires: []
Owner: Platform / Runtime / Infra

## Narrative Summary (1-minute read)

For OVH bootstrap, ChoirOS should ship a minimal VM fleet lifecycle API first, then grow features.
The 80/20 API is:

1. `create`
2. `start`
3. `stop`
4. `snapshot`
5. `restore`
6. `delete`
7. `get`
8. `list`

With conservative bootstrap assumptions (CPU overcommit 2.0, RAM overcommit 1.0, 20% host RAM
reserve), default session sizing at `2 vCPU / 3 GiB` yields this envelope:

1. KS-2 (`Xeon D-1540`, `64 GB RAM`): about `11` SLO-safe active sessions (`14-16` stretch),
   plus `~60-95` parked snapshotted sessions.
2. EPYC 7351P (`256 GB RAM`): about `22` SLO-safe active sessions (`28-32` stretch), plus
   `~70-107` parked snapshotted sessions.

At this stage, CPU and RAM are primary constraints; `500 Mbps` public bandwidth is typically not
the first bottleneck for coding-agent traffic.

## What Changed

1. Defined an authoritative minimal VM lifecycle contract for bootstrap.
2. Added explicit capacity formulas and assumptions for the two OVH profiles under discussion.
3. Added scale conversion guidance from single-node capacity to thousand/million-user planning.
4. Added a measurement plan so modeled capacity can be replaced with observed limits.

## What To Do Next

1. Accept or adjust the bootstrap default VM flavor (`2 vCPU / 3 GiB` suggested).
2. Add telemetry needed to validate this model in production-like load tests.
3. Run a 7-day canary on one OVH node and recalculate envelopes from observed p95/p99 behavior.
4. Keep fleet API minimal until SLO data says the next lifecycle features are required.

## Context

Current repo state relevant to fleet lifecycle:

1. Local guest default VM sizing is currently `4 vCPU / 4096 MiB`:
   `nix/vfkit/user-vm.nix`.
2. Runtime control currently supports only `ensure|stop` actions:
   `hypervisor/src/bin/vfkit-runtime-ctl.rs`.
3. Guest runtime control script also supports only `ensure|stop`:
   `scripts/ops/vfkit-guest-runtime-ctl.sh`.
4. Hypervisor sandbox registry exposes runtime status snapshots, not VM memory snapshots:
   `hypervisor/src/sandbox/mod.rs`.

This means bootstrap still lacks first-class snapshot/restore lifecycle control, which is required
for serious user cost modeling.

## Decision

### 1) Adopt an 80/20 Lifecycle API for Bootstrap

Expose the following control-plane operations (backend-agnostic; vfkit local, OVH backend later):

1. `POST /v1/vms` (`create`)
2. `POST /v1/vms/{vm_id}/start`
3. `POST /v1/vms/{vm_id}/stop`
4. `POST /v1/vms/{vm_id}/snapshot`
5. `POST /v1/vms/{vm_id}/restore`
6. `DELETE /v1/vms/{vm_id}`
7. `GET /v1/vms/{vm_id}`
8. `GET /v1/vms?owner_id=...`

Required rails:

1. Idempotency key on every mutating request.
2. Strict state-machine validation.
3. Quota checks on `create`, `start`, and `restore`.
4. Lifecycle events for every state transition.

### 2) Lifecycle State Machine (Minimal)

`creating -> stopped -> running -> stopping -> stopped`

Snapshot lane:
`running -> pausing -> paused -> snapshotting -> snapshotted -> restoring -> running`

Terminal/error:

1. `deleted`
2. `failed`

No additional orchestration features in bootstrap scope (no live migration, no autoscaling policy
engine, no multi-node placement optimizer).

### 3) Bootstrap Sizing Policy

Default profile for coding-agent sessions:

1. `2 vCPU / 3 GiB RAM`
2. Idle timeout + snapshot park as the default cost-control behavior
3. `4 vCPU` sessions stay opt-in for heavy compile/test lanes only

## Capacity Model

### Input Assumptions

1. Host RAM reserve: `20%` for host OS/hypervisor/background services.
2. CPU overcommit (interactive coding target): `2.0`.
3. RAM overcommit for bootstrap: `1.0` (no intentional RAM overcommit).
4. Snapshot footprint per parked session: `4-6 GiB` (memory snapshot + writable disk deltas).
5. Usable disk assumption for parked snapshots:
   1. KS-2: `~380 GiB` (2x450 NVMe soft RAID + reserve).
   2. EPYC profile: `~430 GiB` (2x500 NVMe soft RAID + reserve).
6. Public bandwidth: `500 Mbps` unmetered baseline.

### Formulas

1. `active_limit = min((threads * cpu_overcommit) / vcpu_per_vm, usable_ram_gib / ram_per_vm_gib)`
2. `slo_safe_active = floor(active_limit * 0.70)`
3. `parked_limit = floor(usable_disk_gib / snapshot_size_gib)`
4. `MAU_estimate = active_concurrency / peak_concurrency_ratio`

### Single-Node Envelope (Two OVH Profiles)

| Profile | CPU Threads | Usable RAM | Theoretical Active (`2vCPU/3GiB`) | SLO-safe Active | Stretch Active | Parked Snapshots |
|---|---:|---:|---:|---:|---:|---:|
| KS-2 (Xeon D-1540, 64 GB) | 16 | 52 GiB | 16 | 11 | 14-16 | 63-95 |
| EPYC 7351P (256 GB) | 32 | 224 GiB | 32 | 22 | 28-32 | 71-107 |

Interpretation:

1. KS-2 is viable for solo use and very small team bootstrap.
2. EPYC 256 GB is the first profile that provides meaningful concurrency headroom for external
   users.

### Sensitivity by VM Flavor

| VM Flavor | KS-2 Theoretical Active | EPYC Theoretical Active | Primary Bottleneck |
|---|---:|---:|---|
| `1 vCPU / 2 GiB` | 26 | 64 | KS-2 RAM, EPYC CPU |
| `2 vCPU / 3 GiB` | 16 | 32 | CPU on both |
| `4 vCPU / 8 GiB` | 6 | 16 | KS-2 RAM, EPYC CPU |

This supports reducing default size below `4 vCPU / 4 GiB` for better economics.

## Thousand/Million Planning Conversion

Using SLO-safe default (`2 vCPU / 3 GiB`) capacity:

1. `1,000` concurrent active users needs roughly:
   1. `~91` KS-2 nodes (`1000 / 11`)
   2. `~46` EPYC nodes (`1000 / 22`)
2. `10,000` concurrent active users needs roughly:
   1. `~910` KS-2 nodes
   2. `~455` EPYC nodes
3. `1,000,000` registered users at `1%` peak concurrency (`10,000` active) maps to the same
   `10,000`-active footprint above.

Inference: millions of registered users are a fleet-shape problem, not a single-node problem.
Bootstrap should optimize for per-node economics and lifecycle correctness first.

## Risks and Limits

1. These are modeled envelopes, not measured production numbers.
2. Heavy compile/test bursts can cut active capacity by `30-50%`.
3. Snapshot disk growth can become first bottleneck if idle GC and retention are weak.
4. Restoring large numbers of snapshots simultaneously can create page-fault storms.
5. If RAM overcommit is introduced later, OOM risk rises sharply without strict controls.

## Validation Plan (Required Before “Accepted”)

1. Add per-VM metrics:
   1. CPU usage and runnable queue pressure.
   2. RSS/working set and host memory pressure.
   3. Snapshot create/restore latency.
   4. Disk read/write throughput and queue depth.
2. Run controlled load tests on both profiles for:
   1. Interactive chat/code-edit loops.
   2. Mixed burst workloads (build/test spikes).
   3. Idle park/restore cycles.
3. Promote this ADR from `Proposed` to `Accepted` only with observed SLO and cost evidence.

## Consequences

### Positive

1. Gives a concrete bootstrap control plane that is small enough to ship quickly.
2. Makes cost and scale discussion quantitative instead of anecdotal.
3. Aligns lifecycle API with snapshot-first parking strategy for user-cost control.

### Tradeoffs

1. Defers advanced fleet features during bootstrap.
2. Requires disciplined SLO telemetry to avoid oversubscription mistakes.
3. Forces explicit profile choices instead of one-size-fits-all defaults.

## Source Notes (External Research)

1. Intel Xeon D-1540 official specs:
   https://www.intel.com/content/www/us/en/products/sku/87039/intel-xeon-processor-d1540-12m-cache-2-00-ghz/specifications.html
2. AMD EPYC 7351P official specs:
   https://www.amd.com/en/support/downloads/drivers.html/processors/epyc/epyc-7001-series/amd-epyc-7351p.html
3. Firecracker NSDI paper (boot/overhead density claims):
   https://www.usenix.org/system/files/nsdi20-paper-agache.pdf
4. Firecracker snapshot semantics and limitations:
   https://raw.githubusercontent.com/firecracker-microvm/firecracker/main/docs/snapshotting/snapshot-support.md
5. Firecracker API surface (machine config, vm pause/resume, snapshot create/load):
   https://raw.githubusercontent.com/firecracker-microvm/firecracker/main/src/firecracker/swagger/firecracker.yaml
6. OVHcloud dedicated bandwidth baseline:
   https://us.ovhcloud.com/bare-metal/bandwidth/
7. OpenStack overcommit concepts and cautions:
   https://docs.openstack.org/arch-design/design-compute/design-compute-overcommit.html
   https://docs.openstack.org/nova/zed/configuration/config.html

## Repo References

1. `nix/vfkit/user-vm.nix`
2. `hypervisor/src/bin/vfkit-runtime-ctl.rs`
3. `scripts/ops/vfkit-guest-runtime-ctl.sh`
4. `hypervisor/src/sandbox/mod.rs`
