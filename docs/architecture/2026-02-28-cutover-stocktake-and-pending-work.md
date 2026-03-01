# Cutover Stocktake + Pending Work (While NixOS VM Is Installing)

Date: 2026-02-28  
Status: Active  
Owner: platform/runtime

## Narrative Summary (1-minute read)

Cutover code and test scaffolding are in place for vfkit-first local runtime, but local Linux builder
infrastructure is not complete yet (`/etc/nix/machines` is missing), so guest runtime provisioning
cannot finish. This is an infrastructure gate, not a control-plane architecture gate.

While NixOS VM installation is in progress, we can still close useful work: tighten docs, reduce
deprecated-path ambiguity, and harden test structure so final builder hookup is mostly mechanical.

## What Changed

1. Local status tooling exists:
   1. `just cutover-status`
   2. `scripts/ops/check-local-cutover-status.sh`
2. Linux builder bootstrap exists for both UTM and generic SSH:
   1. `just builder-bootstrap-utm <vm>`
   2. `just builder-bootstrap-ssh <host> <port> <user>`
3. vfkit local proof harness exists:
   1. `just test-e2e-vfkit-proof`
   2. Playwright video/trace artifact capture
4. Current blocking check is concrete:
   1. `/etc/nix/machines` not yet wired on this Mac

## What To Do Next

1. Finish one-time NixOS aarch64 VM install in UTM.
2. Run `just builder-bootstrap-utm <vm-name>`.
3. Re-check status with `just cutover-status --probe-builder`.
4. Re-run proof path:
   1. `just dev`
   2. `just test-e2e-vfkit-proof`
5. If proof passes, remove remaining compatibility/deprecation paths in runtime + docs.

## Work We Can Complete Before VM Install Finishes

1. Documentation cleanup:
   1. Keep `docs/architecture/NARRATIVE_INDEX.md` aligned to active cutover sequence only.
   2. Mark deprecated operator docs as historical references where needed.
2. Test hardening (no Linux builder needed to start):
   1. Add unit tests around runtime registry/path selection logic.
   2. Add integration tests for runtime API contract validation and error envelopes.
   3. Keep E2E specs deterministic with explicit startup waits and assertions.
3. Command ergonomics:
   1. Keep `just` flow canonical (`dev`, `cutover-status`, `test-e2e-vfkit-proof`).
   2. Avoid alias sprawl; remove stale command variants after proof pass.

## Scope Boundaries (Now vs Later)

1. Solve now:
   1. aarch64 local builder path for vfkit/NixOS local parity.
   2. Reliable local proof with video artifacts.
2. Solve later:
   1. x86_64 Linux builder parity for performance-heavy or strict dual-arch workflows.
   2. Full production backend parity (OVH/cloud-hypervisor) once local path is stable.

## Ready/Blocked Checklist

- [x] vfkit runtime control scripts checked in.
- [x] Playwright proof specs + video capture wired.
- [x] Builder bootstrap automation checked in.
- [ ] Linux builder registration complete (`/etc/nix/machines` exists and usable).
- [ ] `just cutover-status --probe-builder` passing.
- [ ] `just test-e2e-vfkit-proof` passing against real vfkit guest runtime.
