# Platform Project Checklist: Deploy Learnings -> OVH + microvm.nix

## Narrative Summary (1-minute read)

We will complete one more production deployment in the current architecture to capture concrete
operational learnings, then pivot platform runtime from AWS-hosted container/process patterns to
OVH bare metal with microVM isolation. During this transition, we keep shipping velocity high by
stabilizing the current path first, then hardening toward a trust model where sandbox runtimes do
not hold provider API keys. The long-term target is clear: hypervisor/control-plane mediation for
provider access, strict isolation boundaries, and lower infrastructure cost while preserving AWS
credits primarily for Bedrock model usage.

## What Changed

- Platform direction is now explicit:
  - Near-term: observe and learn from the next deploy in current shape.
  - Mid-term: remove provider secrets from sandbox environments.
  - Long-term: move compute/isolation to OVH bare metal + microvm.nix.
- Security model clarified:
  - Hypervisor/control-plane should obviate sandbox need for raw provider keys.
  - Sandbox should become keyless for model/search/email providers.
- Economics and risk rationale clarified:
  - Preserve AWS credits for Bedrock usage.
  - Improve privilege-escalation resistance via VM-level kernel isolation.

## What To Do Next

## Phase 0 - Current Deploy Observation (Finish This First)

- [ ] Deploy latest fixes to production via the current CI path.
- [ ] Capture run-level evidence for prompt-bar flows (success + failure cases).
- [ ] Verify error UX now surfaces provider/config failures (not generic timeout).
- [ ] Verify secret render and process injection behavior in production logs/telemetry.
- [ ] Record exact timeline: deploy start, switch complete, first healthy prompt, anomalies.

## Phase 1 - Learnings Writeup (AWS Experiment)

- [ ] Create a concise postmortem/retrospective doc with:
  - [ ] What worked (deploy flow, secrets model, observability wins).
  - [ ] What failed (timeouts, error opacity, secret propagation gaps).
  - [ ] Risk analysis (credential exposure surface, proxy trust assumptions).
  - [ ] Operational toil (manual steps, brittle paths, rollback confidence).
  - [ ] Cost analysis and rationale for OVH pivot.
- [ ] Include explicit go/no-go criteria for keeping any AWS infra in active path.

## Phase 2 - Security Baseline in Current Architecture

- [ ] Define a policy: sandbox must not require raw provider keys after cutover.
- [ ] Add a migration flag for provider routing mode:
  - [ ] `direct-provider` (temporary)
  - [ ] `hypervisor-gateway` (target)
- [ ] Add request validation/policy layer for mediated provider calls:
  - [ ] provider allowlist
  - [ ] per-request size bounds
  - [ ] per-session/user quotas
  - [ ] request timeout budgets
- [ ] Add audit trail for provider mediation events (request metadata, outcome, duration).

## Phase 3 - Keyless Sandbox Transition

- [ ] Build hypervisor/control-plane provider gateway endpoints.
- [ ] Route sandbox provider traffic through gateway instead of direct provider calls.
- [ ] Remove provider key env vars from sandbox runtime config.
- [ ] Add startup guard in sandbox: fail fast if provider keys are present (after cutover flag).
- [ ] Add integration tests proving sandbox execution succeeds without provider keys.
- [ ] Add negative tests proving direct provider egress is blocked when gateway mode enabled.

## Phase 4 - OVH Bare Metal + microvm.nix Foundation

- [ ] Provision OVH bare metal host(s) with reproducible NixOS bootstrap.
- [ ] Define microvm.nix host layout:
  - [ ] control-plane VM(s) / host services
  - [ ] sandbox VM pool model
  - [ ] network segmentation and firewall policy
- [ ] Implement image build pipeline for sandbox VM artifacts.
- [ ] Establish secure secret material handling on host/control-plane only.
- [ ] Validate VM lifecycle operations (create/start/stop/recycle) under load.

## Phase 5 - Migration and Cutover

- [ ] Stand up staging parity environment on OVH.
- [ ] Run end-to-end tests (prompt bar, writer, terminal, websocket streams).
- [ ] Run chaos/failure drills (provider outage, VM crash, control-plane restart).
- [ ] Define cutover runbook with rollback criteria and rollback command path.
- [ ] Execute production cutover in maintenance window with live verification checklist.

## Phase 6 - Post-Cutover Hardening

- [ ] Remove deprecated AWS compute/deploy components not in target architecture.
- [ ] Keep AWS account focused on Bedrock/API usage only (if retained).
- [ ] Tighten observability dashboards for VM isolation and provider mediation latencies.
- [ ] Conduct security review on new trust boundaries and incident response playbooks.
- [ ] Document finalized reference architecture and operating procedures.

## Acceptance Criteria (Project Complete)

- [ ] Sandbox runtimes operate without raw provider API keys.
- [ ] Provider access is mediated through trusted control-plane/hypervisor path with audit logs.
- [ ] Production runs return explicit actionable errors for config/auth/provider failures.
- [ ] OVH microVM platform is primary production runtime with tested rollback.
- [ ] Deployment path is reproducible, low-toil, and documented end-to-end.
