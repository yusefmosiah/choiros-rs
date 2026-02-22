# Deployment Contract: Current AWS -> OVH microVM Cutover

Date: 2026-02-22  
Owner: platform/runtime

## Narrative Summary (1-minute read)

Today, production deployment is AWS SSM-driven and host convergence is done via
`nixos-rebuild switch`. That is the current source of truth and should be treated
as canonical until OVH cutover is complete.

For OVH, AWS SSM is not required. The deployment primitive should become
"push revision + host switch" on OVH nodes, executed over SSH (or an OVH-native
remote execution service), using the same host-side switch script shape.

The core rule is stable across providers: deploy scripts must be explicit,
versioned in-repo, and shared between CI and manual operations.

## What Changed

1. Removed obsolete deploy script:
   - Deleted `scripts/deploy.sh` (legacy direct service restart flow).
2. Added explicit AWS SSM deploy scripts:
   - `scripts/deploy/aws-ssm-deploy.sh` (local/CI entrypoint).
   - `scripts/deploy/host-switch.sh` (host-side switch + health checks).
3. Updated task runner deployment commands:
   - Added `just deploy-aws-ssm`.
   - Deprecated `just deploy-ec2` with explicit failure and migration message.

## What To Do Next

1. Update CI workflow to call `scripts/deploy/aws-ssm-deploy.sh` instead of
   embedding a large inline shell script.
2. Add OVH deploy entrypoint (for example `scripts/deploy/ovh-ssh-deploy.sh`)
   that reuses `scripts/deploy/host-switch.sh` shape without AWS dependencies.
3. During cutover, remove AWS-only deploy logic from active paths once OVH is
   production and rollback-tested.

## Current Canonical Deploy Paths

- AWS now:
  - `just deploy-aws-ssm`
  - wraps `scripts/deploy/aws-ssm-deploy.sh`
- Host switch logic:
  - `scripts/deploy/host-switch.sh`
  - performs checkout, optional store path install bridge, `nixos-rebuild switch`,
    and container/hypervisor health checks.

## Do We Need AWS SSM?

- Short answer: only while runtime is on AWS.
- For OVH target topology, no.
- On OVH, use SSH or a controlled remote executor, but keep the same switch
  contract and health checks.

## Provider-Independent Deployment Contract

Any deploy path must do all of the following, in order:

1. Resolve target revision (`RELEASE_SHA`).
2. Converge host configuration (`nixos-rebuild switch`).
3. Verify hypervisor health endpoint.
4. Verify both sandbox runtime units (`live` and `dev`) are listening + healthy.
5. Emit diagnostics on failure (systemctl, journalctl, socket state).

If a deploy path does not satisfy these checks, it is non-canonical.
