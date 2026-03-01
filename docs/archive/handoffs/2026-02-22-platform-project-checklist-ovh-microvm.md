# Platform Project Checklist: OVH + microVM Hard Cutover

Date: 2026-02-26
Owner: platform/runtime
Status: active detailed checklist

## Narrative Summary (1-minute read)

This is the detailed platform checklist for the OVH microVM architecture.
It is downstream of local reliability gates and should be executed only after local provider/gateway validation is stable.

## What Changed

1. Removed AWS-era operational guidance from active checklist.
2. Aligned checklist ordering to local-first -> OVH single-node -> OVH two-node.
3. Kept hard boundaries: thin hypervisor, per-user microVM, keyless sandbox model.

## What To Do Next

1. Finish local gates in `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`.
2. Execute Phase 1 and Phase 2 on one OVH node.
3. Expand to two-node failover only after Phase 2 evidence is complete.

## Phase 0: Lock Runtime Contract

- [ ] `cloud-hypervisor` is the only VM backend.
- [ ] One user = one microVM boundary.
- [ ] `live` and `dev` run inside the same user VM.
- [ ] Hypervisor responsibilities are limited to routing, lifecycle, policy checks, observability.

## Phase 1: OVH Single-Node Bring-Up

- [ ] Provision OVH node with reproducible NixOS host config.
- [ ] Start hypervisor + required sandbox services.
- [ ] Validate login -> desktop -> prompt loop on public domain.
- [ ] Run Mac-side Playwright smoke and attach artifacts.

## Phase 2: Secrets and Keyless Sandbox Hardening

- [ ] Provider/search keys are mediated through gateway path.
- [ ] Sandbox startup guard fails if forbidden raw keys are present.
- [ ] Egress policy blocks direct provider calls from sandbox.
- [ ] Gateway audit events capture provider/search mediation outcomes.

## Phase 3: Two-Node Platform Expansion

- [ ] Add second identical OVH node (same module graph).
- [ ] Implement active/passive role handoff.
- [ ] Validate node failover and recovery timing.
- [ ] Validate rollback to previous generation on both nodes.

## Phase 4: Multi-Tenant VM Lifecycle

- [ ] Implement per-user VM create/start/stop/recycle operations.
- [ ] Enforce in-VM container resource limits.
- [ ] Validate concurrency behavior under expected user load.

## Phase 5: Failure Drills and Launch Gate

- [ ] Execute chaos drills (VM crash, host reboot, dependency outage).
- [ ] Verify no cross-session event bleed (`session_id`, `thread_id` scoped).
- [ ] Run 24h soak with periodic lifecycle churn.
- [ ] Confirm incident runbook and on-call checklist are complete.

## Target Runtime Architecture

```text
User Browser
  -> Hypervisor (control plane)
      -> User microVM (kernel boundary)
          -> Guest ingress
              -> sandbox-live container
              -> sandbox-dev container
              -> optional sidecars
```

## References

1. `docs/architecture/2026-02-26-local-first-ovh-execution-plan.md`
2. `docs/architecture/roadmap-dependency-tree.md`
3. `docs/runbooks/local-provider-matrix-validation.md`
4. `docs/runbooks/platform-secrets-sops-nix.md`
