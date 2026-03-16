# VFKit Local Proof Runbook (Mac)

Date: 2026-02-28
Kind: Guide
Status: Accepted
Requires: []
Owner: platform/runtime

## Narrative Summary (1-minute read)

This runbook proves the local runtime path is vfkit + NixOS guest, not the old
process compatibility path.

Canonical ingress for this proof is `http://127.0.0.1:9090` (hypervisor path).
`http://127.0.0.1:3000` is still a direct sandbox/dev path and is not the cutover
proof surface.

Proof has two checks:

1. Hypervisor starts/stops runtimes through the Rust host controller
   (`target/debug/vfkit-runtime-ctl` in local dev).
2. The Terminal app (video recorded by Playwright) prints NixOS guest identity
   (`/etc/os-release` includes `NixOS`).

## What Changed

1. Hypervisor runtime lifecycle is vfkit-only (process fallback removed).
2. Added vfkit host/guest runtime control scripts.
3. Added root flake output `nixosConfigurations.choiros-vfkit-user`.
4. Added Playwright proof spec: `vfkit-terminal-proof.spec.ts`.

## What To Do Next

1. Check current readiness with `just cutover-status`.
2. Bootstrap a local Linux builder (UTM or SSH) so `aarch64-linux` derivations can build.
3. Re-check with `just cutover-status --probe-builder`.
4. Start local stack with `just dev`.
5. Run `just test-e2e-vfkit-proof`.
6. Review generated video artifact and trace.
7. Use the operator quickstart:
   1. `docs/practice/guides/local-vfkit-nixos-miniguide.md`

## Prerequisites

1. macOS host with Nix installed.
2. SSH key available for guest login.
3. UI assets built (`just local-build-ui`).
4. Local Linux builder configured (`just builder-bootstrap-utm <vm-name>` or `just builder-bootstrap-ssh <host> <port> <user>`).

Recommended environment:

```bash
export CHOIR_VFKIT_SSH_PUBKEY="$(cat ~/.ssh/id_ed25519.pub)"
```

## Builder Bootstrap

Use one of these:

```bash
# UTM VM path (starts VM, resolves guest IP, bootstraps remote Nix, wires local nix builder config)
just builder-bootstrap-utm <utm-vm-name>

# Generic SSH path
just builder-bootstrap-ssh <host> <port> <user>
```

Implementation script:

1. `scripts/ops/bootstrap-local-linux-builder.sh`

Readiness check:

```bash
just cutover-status
just cutover-status --probe-builder
```

Expected gating behavior:

1. Before builder registration, `just cutover-status` should fail on `/etc/nix/machines`.
2. After bootstrap, `just cutover-status --probe-builder` should pass fully.

## Start Stack

```bash
just local-build-ui
just dev
just dev-status
```

## Automated Proof (Video)

```bash
# Optional clean reset before proof (enabled by default in test recipe)
just vfkit-reset

just test-e2e-vfkit-proof
```

Artifacts:

1. `tests/artifacts/playwright/test-results/**/video.webm`
2. `tests/artifacts/playwright/test-results/**/trace.zip`
3. `tests/artifacts/playwright/html-report/index.html`

## Manual Proof in Terminal App

1. Open `http://localhost:9090`.
2. Log in.
3. Open the `Terminal` app.
4. Wait for `Connected` (allow up to ~10 seconds on startup).
5. Run:

```bash
cat /etc/os-release
```

Expected output includes:

```text
NAME="NixOS"
```

## Runtime Control Files

1. Host controller (primary): `hypervisor/src/bin/vfkit-runtime-ctl.rs`
2. Host dispatcher: `scripts/ops/vfkit-runtime-ctl.sh`
3. Guest runtime ctl: `scripts/ops/vfkit-guest-runtime-ctl.sh`
4. Guest VM config: `nix/vfkit/user-vm.nix`
5. Local reset helper: `scripts/ops/vfkit-reset.sh`
