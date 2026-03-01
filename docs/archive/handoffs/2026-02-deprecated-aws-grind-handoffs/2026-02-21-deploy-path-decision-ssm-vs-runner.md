# Handoff: Deploy Path Decision (SSM vs Grind Runner)

Date: 2026-02-21  
Owner: runtime/deploy  
Status: proposed

## Narrative Summary (1-minute read)

We are seeing repeated deploy failures in `Deploy EC2 via AWS SSM`, but current evidence shows the primary blocker is runtime health (`sandbox-live` does not become reachable on `127.0.0.1:8080`), not SSM transport itself.

Switching to a grind-box runner will likely improve debugging speed and observability, but will not by itself fix the sandbox startup defect.

Recommendation: use a hybrid path now.

- Keep SSM as the production actuation path (least inbound access, IAM/audit visibility).
- Add a grind-runner deploy path for iterative debugging and controlled canaries.
- Unify deploy logic into one script so both paths execute the same steps.

## What Changed

1. Decision framing clarified:
   - SSM is not the root-cause of the current health failures.
   - SSM is currently poor for fast diagnosis due output truncation and friction.
2. Tradeoff analysis completed:
   - **SSM strengths:** no inbound SSH from GitHub runners, IAM audit trail, simpler cloud-control-plane posture.
   - **SSM weaknesses:** hard to inspect long/interactive logs, slower feedback loops.
   - **Runner strengths:** direct and rich logs, easier iterative debugging, lower friction for service-level diagnostics.
   - **Runner weaknesses:** more operational burden, credential and host-hardening requirements, larger CI attack surface.
3. Architecture direction set:
   - Production deploy remains SSM-first.
   - Grind-runner path added as a debug/canary lane, not an immediate full replacement.

## What To Do Next

1. Factor deploy logic into a shared script:
   - Add `scripts/deploy/ec2-deploy.sh` with strict mode and clear phases:
     - build/install
     - restart services
     - health checks
     - targeted diagnostics
   - Both SSM job and runner job must call this same script.
2. Add grind-runner workflow lane:
   - New workflow/job: `deploy-ec2-runner` using self-hosted grind runner.
   - Trigger mode: `workflow_dispatch` initially.
   - Protect with environment approvals and branch restriction.
3. Keep SSM lane as production gate:
   - Keep current `deploy-ec2` SSM job for `main`.
   - Use runner lane for rapid triage until sandbox startup is stable.
4. Define promotion criteria:
   - If runner lane resolves startup issue and SSM lane passes 3 consecutive runs, keep SSM as default prod path.
   - Only consider full switch away from SSM if runner materially improves reliability, not just ergonomics.

## Decision Matrix

- `SSM only`: secure and auditable, but slow diagnosis during failures.
- `Runner only`: fast iteration, but higher operational/security footprint.
- `Hybrid (recommended)`: keep production safety rails while gaining a high-velocity debug lane.

## Immediate Tactical Plan (next session)

1. Implement `scripts/deploy/ec2-deploy.sh` from current inline workflow script.
2. Add `workflow_dispatch` runner-based deploy job that calls the script.
3. Add upload of diagnostics artifact from runner lane (`journalctl`, `systemctl show`, socket state).
4. Use runner lane to isolate/fix `sandbox-live` startup failure.
5. Re-run SSM lane to validate production path is green.
