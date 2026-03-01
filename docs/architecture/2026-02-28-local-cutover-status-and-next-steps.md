# Local Cutover Status + Next Steps (Mac + vfkit)

Date: 2026-02-28  
Status: In Progress  
Owner: platform/runtime

## Narrative Summary (1-minute read)

Local cutover is partially complete: runtime control and routing now target the vfkit/NixOS path, and Playwright proof specs/video capture are in place.  
Current hard blocker is infrastructure, not app logic: this Mac needs a working Linux builder VM so `aarch64-linux` derivations can be built for the NixOS guest runtime path.

Until the Linux builder is online, branch-runtime E2E tests time out at runtime startup (`/admin/sandboxes/.../branches/.../start`) because the underlying guest runtime cannot be provisioned.

## What Changed

1. Added vfkit-first local runtime flow and proof harness:
   1. Runtime control scripts in `scripts/ops/vfkit-*.sh`.
   2. Playwright branch proxy + vfkit terminal proof specs.
2. Added local Linux builder bootstrap automation:
   1. `scripts/ops/bootstrap-local-linux-builder.sh`
   2. `just builder-bootstrap-utm <vm>` and `just builder-bootstrap-ssh <host> <port> <user>`
3. Added local readiness/diagnostic command:
   1. `scripts/ops/check-local-cutover-status.sh`
   2. `just cutover-status` (use `--probe-builder` for live builder probe).

## What To Do Next

1. Bring up NixOS aarch64 in UTM (one-time VM install).
2. Run builder bootstrap:
   1. `just builder-bootstrap-utm <utm-vm-name>`
3. Verify readiness:
   1. `just cutover-status --probe-builder`
4. Re-run local proof:
   1. `just dev`
   2. `just test-e2e-vfkit-proof`
5. If proof passes, remove remaining deprecated compatibility paths and simplify ops/docs to one canonical flow.

## Current Risks / Boundaries

1. aarch64 builder solves immediate local unblock; native x86_64 builder should be added later for performance-heavy x86 workflows.
2. UTM CLI (`utmctl`) cannot create VMs from scratch; first VM definition/install is still a one-time manual step.
3. Auth overlay duplication in UI can still introduce E2E flake risk; helper is hardened but UI-level deduplication is still desirable.

## Suggested Near-Term Scope Split

1. Now:
   1. Stabilize aarch64 local builder and pass vfkit proof.
2. Next:
   1. Harden auth modal dedupe behavior in frontend.
   2. Add CI-like local smoke target for branch runtime start.
3. Later:
   1. Add native x86_64 Linux builder (remote or local VM) for dual-arch parity and faster non-cross builds.

## References

1. `docs/architecture/2026-02-28-cutover-stocktake-and-pending-work.md`
2. `docs/runbooks/vfkit-local-proof.md`
3. `docs/architecture/2026-02-28-3-tier-gap-closure-plan.md`
4. `scripts/ops/bootstrap-local-linux-builder.sh`
5. `scripts/ops/check-local-cutover-status.sh`
