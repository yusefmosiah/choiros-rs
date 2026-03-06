# OVH Config and Deployment Entrypoint (Comprehensive)

Date: 2026-03-04
Kind: Guide
Status: Accepted
Requires: []
Owner: Platform / Runtime / Infra

## Narrative Summary (1-minute read)

This is the single entrypoint for OVH bring-up and deployment review.

If you have been away for a few days, read this file first. It gives:

1. The current target topology and priorities.
2. The exact document order to follow.
3. A practical execution checklist from account setup to deployment proof.
4. Links to all authoritative ADRs/runbooks for deeper detail.

## What Changed

1. Added one OVH-focused “big picture” entrypoint for config + deployment.
2. Consolidated active OVH document order into one place.
3. Added a start-to-finish checklist for current bootstrap scope.

## What To Do Next

1. Follow the `Fast Re-Onboarding Path` section in order.
2. Track progress using `Execution Checklist`.
3. Drop into linked docs for each phase when implementation detail is needed.

## Current Target (Authoritative)

1. Two OVH US-East `SYS-1` nodes:
   1. `ns1004307.ip-51-81-93.us` (`51.81.93.94`)
   2. `ns106285.ip-147-135-70.us` (`147.135.70.196`)
2. Control-plane secrets model:
   1. Service-account OAuth2
   2. Secret Manager for values
   3. KMS for crypto operations
3. Runtime lifecycle:
   1. Current implementation is still `ensure|stop`
   2. Target lifecycle expands to `create/start/stop/snapshot/restore/delete/get/list`
4. Product milestone order:
   1. OVH bring-up and hardening
   2. Choir bootstrap loop
   3. Publishing bootstrap

## Fast Re-Onboarding Path

Read these in order:

1. [Narrative Index](../architecture/NARRATIVE_INDEX.md)
2. [Wave Plan: Local -> OVH -> Publishing](../architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md)
3. [ADR-0012: OVH US-East Secrets + Two-Node Lifecycle](../architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md)
4. [OVH US-East Bootstrap Runbook](./ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md)
5. [Platform Secrets Policy Runbook](./platform-secrets-sops-nix.md)
6. [ADR-0010: Minimal 80/20 VM Lifecycle API](../architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md)
7. [ADR-0011: Bootstrap Into Publishing](../architecture/adr-0011-bootstrap-into-publishing-state-compute-decoupling.md)

## Document Map by Task

1. Big-picture status and sequence:
   1. [Wave Plan](../architecture/2026-02-28-wave-plan-local-to-ovh-bootstrap.md)
   2. [Roadmap Dependency Tree](../architecture/roadmap-dependency-tree.md)
2. OVH account, secrets, and trust boundaries:
   1. [ADR-0008](../architecture/adr-0008-ovh-selfhosted-secrets-architecture.md)
   2. [ADR-0012](../architecture/adr-0012-ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md)
   3. [OVH US-East Bootstrap Runbook](./ovh-us-east-bootstrap-secrets-and-compute-lifecycle.md)
3. Runtime lifecycle and capacity:
   1. [ADR-0010](../architecture/adr-0010-ovh-bootstrap-vm-fleet-capacity-and-minimal-lifecycle-api.md)
   2. [3-Tier Gap Closure Plan](../architecture/2026-02-28-3-tier-gap-closure-plan.md)
4. Local proof and deployment-shape gates:
   1. [Local vfkit NixOS Miniguide](./2026-02-28-local-vfkit-nixos-miniguide.md)
   2. [vfkit Local Proof](./vfkit-local-proof.md)
   3. [Local Provider Matrix Validation](./local-provider-matrix-validation.md)
5. Current ops checkpoint and next steps:
   1. [Deployment Checkpoint (2026-03-05)](./2026-03-05-deployment-checkpoint-and-next-steps.md)
   2. [Archive index](../archive/README.md)

## Execution Checklist (Current)

1. Confirm local strict gate is green (`cutover-status`, canonical hypervisor e2e, provider matrix).
2. Verify OVH account identity path (service account, token mint, IAM policy scope).
3. Verify secret flow (Secret Manager -> host credential files -> `LoadCredential` consumers).
4. Converge primary OVH node and pass login -> desktop -> prompt loop.
5. Converge secondary OVH node and run manual failover drill.
6. Verify rollback drill and capture evidence.
7. Activate bootstrap loop and run one success + one intentional failure diagnostic pass.

## Scope Boundaries

In scope:

1. OVH config/deployment readiness.
2. Secrets and lifecycle correctness.
3. Two-node manual failover hardening.

Out of scope:

1. Full automatic fleet orchestrator.
2. Memory/multimedia/audio expansion before platform gates.
3. Reintroducing AWS-era deployment lanes.

## Operator Notes

1. Prioritize authoritative docs under `docs/architecture` and `docs/runbooks`.
2. Treat `docs/archive/*` as historical context only.
3. If a conflict appears between docs, prefer newer ADR status + wave plan order.
