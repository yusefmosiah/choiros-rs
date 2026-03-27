# ADR-0016: NixOS Declarative Deployment

Date: 2026-03-08
Kind: Decision
Status: Accepted
Priority: 3
Requires: [ADR-0002, ADR-0014]
Owner: ChoirOS infrastructure

## Narrative Summary (1-minute read)

Deployment is now fully declarative. `nixos-rebuild switch` is the single
mechanism for all changes — OS, application binaries, VM runners, and
frontend assets are all nix store paths referenced by systemd units.
CI pushes to main, Node B deploys atomically via `nixos-rebuild switch
--flake .#choiros-b`. Node A promotion uses `--flake .#choiros-a`.

The unified root flake builds all packages with shared `cargoArtifacts`
(deps built once). Sub-flakes remain only for `nix develop` dev shells.
CARGO_MANIFEST_DIR fallbacks are eliminated — release builds use
centralized `crate::paths::{sandbox_root, writer_root}`.

## What Changed

Implemented 2026-03-08. All 7 phases complete on Node B:

1. Unified root flake with shared crane cargoArtifacts
2. Host systemd units reference nix store paths (no `/opt/choiros/bin/`)
3. runtime-ctl packaged as nix derivation with injected PATH
4. Guest VM uses sandbox from nix store (3→2 virtiofs shares)
5. CI simplified to single `nixos-rebuild switch` command
6. Bridge IP — deferred (NixOS networking quirk, workaround in place)
7. CARGO_MANIFEST_DIR fallbacks replaced with `crate::paths` module

## What To Do Next

1. Recover Node A from failed promotion (rescue mode, rollback generation)
2. Re-attempt Node A promotion with `nixos-rebuild boot` (not switch)
3. Debug bridge IP persistence (Phase 6)
4. Evaluate microvm.nix declarative mode (Phase 7, future)

---

## Current Architecture: What We Have

### The Two Deployment Paths Problem

**Path 1: NixOS system closure** (via `nixos-rebuild switch`)
- Controls: kernel, bootloader, systemd units, Caddy, firewall, users, networking
- Stored in: `/nix/var/nix/profiles/system-{N}-link` → `/nix/store/...`
- Updated: rarely, manually via SSH
- Rollback: GRUB boot menu, `nixos-rebuild --rollback`

**Path 2: Application binaries** (via CI scripts)
- Controls: sandbox binary, hypervisor binary, runtime-ctl script, frontend WASM, VM runner
- Stored in: `/opt/choiros/bin/` (mutable, outside nix store)
- Updated: every push to main, via `cp -f` in CI
- Rollback: none (old binary overwritten)

These paths are **decoupled**: the systemd unit says `ExecStart = "/opt/choiros/bin/hypervisor"`
but `/opt/choiros/bin/hypervisor` is not managed by NixOS. This means:

- `nixos-rebuild switch` doesn't update the hypervisor binary
- CI can update the binary without touching the NixOS generation
- There's no way to atomically roll back both system and application together
- The system generation and application version can be arbitrarily out of sync

### What Each Sub-Flake Does

Three independent sub-flakes, each with their own inputs:

```
sandbox/flake.nix    → packages.x86_64-linux.sandbox     (crane, rust-overlay)
hypervisor/flake.nix → packages.x86_64-linux.hypervisor  (crane, rust-overlay)
dioxus-desktop/flake.nix → packages.x86_64-linux.web     (crane, wasm-bindgen)
```

Root `flake.nix` has NO package outputs. It only defines `nixosConfigurations`.
The sub-flakes are built independently with `nix build ./sandbox#sandbox`.

### CI Deploy Flow (Current)

```bash
# On Node B via SSH:
git pull --ff-only origin main
nix build ./sandbox#sandbox -o result-sandbox          # sub-flake build
nix build ./hypervisor#hypervisor -o result-hypervisor  # sub-flake build
nix build ./dioxus-desktop#web -o result-frontend       # sub-flake build
nix build .#nixosConfigurations.choiros-ch-sandbox-live.config.microvm.runner.cloud-hypervisor -o result-vm-live
cp -f result-sandbox/bin/sandbox /opt/choiros/bin/sandbox          # MANUAL COPY
cp -f result-hypervisor/bin/hypervisor /opt/choiros/bin/hypervisor # MANUAL COPY
cp -f scripts/ops/ovh-runtime-ctl.sh /opt/choiros/bin/ovh-runtime-ctl  # MANUAL COPY
ip addr add 10.0.0.1/24 dev br-choiros  # WORKAROUND
systemctl restart hypervisor
```

Problems:
- 4 separate nix builds with separate dependency caches (no shared cargoArtifacts)
- Manual file copies with no atomicity or rollback
- Bridge IP workaround indicates NixOS config bug
- VM runner build is disconnected from host system generation
- No relationship between "what's deployed" and any nix generation

### How microvm.nix Works

The `microvm.nix` module has two modes:

**Declarative** (`microvm.vms.<name>` on host): VM config is part of host NixOS generation.
`nixos-rebuild switch` on host rebuilds VM, creates `microvm@<name>.service`.

**Imperative** (separate build, `microvm` CLI): VM flake stored in `/var/lib/microvms/<name>/`.
Host `nixos-rebuild` does NOT affect VMs. `microvm -u <name>` updates.

We use **neither** — we build the runner separately and manage it with a shell script
(`ovh-runtime-ctl.sh`). This is a third, ad-hoc mode.

### What nixos-rebuild Actually Does

1. Evaluates the flake for the target nixosConfiguration
2. Builds the full system closure (kernel, initrd, systemd, all packages)
3. `switch`: activates immediately (restarts changed services) AND sets as boot default
4. `boot`: only sets as boot default (takes effect on next reboot)
5. Creates new generation in `/nix/var/nix/profiles/system-{N}-link`

**Key safety consideration**: `switch` restarts services and can break SSH if networking
changes. `boot` is safer for remote servers — if the new config fails to boot, GRUB
lets you select the previous generation (or OVH rescue mode).

---

## Target Architecture: What We Want

### Single Source of Truth

One `nixos-rebuild switch` (or `boot`) deploys everything:
- System packages (kernel, Caddy, cloud-hypervisor, virtiofsd)
- Application binaries (sandbox, hypervisor, runtime-ctl, frontend)
- VM runner configuration
- Systemd service definitions with nix store paths

### Unified Root Flake

```nix
# Root flake.nix — ALL packages + ALL NixOS configs
{
  outputs = { self, nixpkgs, crane, rust-overlay, microvm, disko, ... }: {

    # Application packages (built once, shared cargoArtifacts)
    packages.x86_64-linux = {
      sandbox = ...;      # crane buildPackage -p sandbox
      hypervisor = ...;   # crane buildPackage -p hypervisor
      frontend = ...;     # wasm-bindgen + wasm-opt
      runtime-ctl = ...;  # writeShellApplication wrapping the script
    };

    # NixOS host configs reference packages via self
    nixosConfigurations.choiros-a = nixpkgs.lib.nixosSystem {
      specialArgs = { choirosPackages = self.packages.x86_64-linux; };
      modules = [ ./nix/hosts/ovh-node-a.nix ];
    };

    # VM guest configs
    nixosConfigurations.choiros-ch-sandbox-live = ...;
  };
}
```

### Systemd Services Reference Nix Store Paths

```nix
# In ovh-node.nix
{ choirosPackages, ... }: {
  systemd.services.hypervisor = {
    serviceConfig = {
      ExecStart = "${choirosPackages.hypervisor}/bin/hypervisor";
      Environment = [
        "SANDBOX_VFKIT_CTL=${choirosPackages.runtime-ctl}/bin/ovh-runtime-ctl"
        "FRONTEND_DIST=${choirosPackages.frontend}"
      ];
    };
  };
}
```

No more `/opt/choiros/bin/` on the host. Binaries live in `/nix/store/` and
are referenced by their content-addressed hash. Rolling back a NixOS generation
atomically rolls back all binaries.

### Guest-Side Binary Path Refactoring

The host is only half the problem. The guest VM (`sandbox-vm.nix`) also has
imperative binary sourcing:

```nix
# Current: guest mounts host /opt/choiros/bin via virtiofs
{ tag = "choiros-bin"; source = "/opt/choiros/bin"; mountPoint = "/opt/choiros/bin"; }
# Guest service runs: ExecStart = "/opt/choiros/bin/sandbox"
```

This must also be refactored. The guest's sandbox binary comes from the host's
nix store (already shared via the `nix-store` virtiofs mount). The fix is to
make the guest service reference a store path directly:

```nix
# Target: guest ExecStart points into shared /nix/store
ExecStart = "${choirosPackages.sandbox}/bin/sandbox";
# Remove the choiros-bin virtiofs share entirely
```

This eliminates another virtiofs mount (down to 2: nix-store + creds),
simplifies the guest config, and means the sandbox binary version is locked
to whatever the host's nix store contains — which is exactly what
`nixos-rebuild` manages.

**Prerequisite**: The sandbox store path and its runtime closure must be
present in the host's `/nix/store` (they will be, since building the VM
runner pulls in the sandbox as a dependency). The guest sees them via the
existing `nix-store` virtiofs share.

### CI Becomes Simple

```bash
# On Node B via SSH:
cd /opt/choiros/workspace
git pull --ff-only origin main
nixos-rebuild switch --flake .#choiros-b
```

One command. NixOS builds everything, installs it atomically, restarts changed
services. If the build fails, nothing changes. If the switch fails, `--rollback`.

### VM Runner Integration

Two options to evaluate:

**Option A: microvm.nix declarative mode**
- Add `microvm.vms.sandbox-live` to the host NixOS config
- `nixos-rebuild` on host rebuilds VM runner automatically
- `microvm@sandbox-live.service` manages VM lifecycle
- Replaces `ovh-runtime-ctl.sh` with standard systemd
- Pro: fully declarative, no scripts
- Con: host rebuild triggers VM rebuild (coupling)

**Option B: microvm.nix imperative mode**
- VM flake stored in `/var/lib/microvms/sandbox-live/`
- Host `nixos-rebuild` doesn't affect VMs
- `microvm -u sandbox-live` updates VM independently
- Pro: VM lifecycle decoupled from host
- Con: still need a mechanism to update VMs

**Option C: Package runtime-ctl, inject runner store paths** (incremental but complete)
- Package `ovh-runtime-ctl` as a nix derivation (`writeShellApplication`)
- Inject the VM runner store path at build time (no more `WORKSPACE/result-vm-*` discovery)
- Inject all tool paths (cloud-hypervisor, virtiofsd, socat, curl, ip) via `runtimeInputs`
- The derivation replaces mutable `RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"` with
  a fixed store path: `RUNNER_DIR="${vmRunner}/share/microvm"`
- Host NixOS config references the packaged runtime-ctl in hypervisor Environment
- Pro: eliminates mutable workspace symlinks, runtime-ctl becomes reproducible
- Con: still shell-script lifecycle management (not systemd-native)

Current runtime-ctl discovers runners via mutable workspace symlinks:
```bash
# Current: mutable path
RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"  # /opt/choiros/workspace/result-vm-live
```
Target:
```bash
# Target: store path injected at build time
RUNNER_DIR="/nix/store/...-microvm-cloud-hypervisor-sandbox-live/share/microvm"
```

Recommendation: Start with **Option C** (package runtime-ctl with store-path runner),
evaluate microvm.nix declarative mode after the basic refactor is stable.

---

## Transitional Status

Once this ADR is accepted, the following artifacts are **legacy/transitional**:

- `ci.yml` deploy step (component builds + `cp -f` + bridge workaround)
- `promote.yml` deploy step (same pattern)
- `/opt/choiros/bin/` directory and tmpfiles rules creating it
- Sub-flakes used for deployment builds (`sandbox/flake.nix`, `hypervisor/flake.nix`,
  `dioxus-desktop/flake.nix` remain for `nix develop` dev shells only)
- `RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"` in runtime-ctl
- Any documentation describing `/opt/choiros/bin/*` as the supported artifact path

These remain functional during transition but should not be extended or treated
as the target architecture.

---

## Implementation Phases

### Phase 1: Unified Root Flake (root packages)

Move all package builds from sub-flakes into root `flake.nix`:
- Single `craneLib` with shared `cargoArtifacts` (workspace deps built once)
- `packages.x86_64-linux.{sandbox, hypervisor, frontend}`
- Sub-flakes remain for `nix develop` (dev shells) but are not used for deployment
- VM runner build stays as `nixosConfigurations.*.config.microvm.runner.*` (already in root)

**Risk**: Medium. Crane workspace builds may have edge cases. Test thoroughly.
**Validation**: `nix build .#sandbox`, `nix build .#hypervisor` produce identical
binaries to current sub-flake builds (compare with `diff`).

**Caveats**: The sub-flakes each pin their own `rust-overlay` and `crane` inputs.
The unified flake must pin these once. Verify that the shared toolchain version
matches what each sub-flake was using.

### Phase 2: Host Unit Rewiring (host services reference store paths)

Update `ovh-node.nix` to accept packages via `specialArgs`:
- `ExecStart = "${choirosPackages.hypervisor}/bin/hypervisor"`
- `FRONTEND_DIST = "${choirosPackages.frontend}"`
- Remove `/opt/choiros/bin/{hypervisor,sandbox}` from tmpfiles rules
- Keep `/opt/choiros/bin/ovh-runtime-ctl` temporarily (packaged in Phase 3)

**Important**: Hypervisor Rust code has fallback paths in `config.rs` that resolve
relative to CWD or `CARGO_MANIFEST_DIR` when env vars are absent. The systemd
unit must continue to pass absolute paths via Environment. Do not remove
`WorkingDirectory` without verifying all path resolutions are absolute.

**Risk**: Low. Pure config change, same binaries.
**Validation**: `nixos-rebuild build --flake .#choiros-b` succeeds. Service starts
and serves requests.

### Phase 3: Package runtime-ctl (store-path runner inputs)

Convert `ovh-runtime-ctl.sh` into a nix derivation:
- `pkgs.writeShellApplication { name = "ovh-runtime-ctl"; ... }`
- Inject `runtimeInputs` for all tools (cloud-hypervisor, socat, curl, ip, etc.)
- Inject VM runner store path at build time (replaces `WORKSPACE/result-vm-*`)
- Reference packaged runtime-ctl in hypervisor Environment:
  `SANDBOX_VFKIT_CTL = "${choirosPackages.runtime-ctl}/bin/ovh-runtime-ctl"`
- Remove `/opt/choiros/bin/ovh-runtime-ctl` from CI deploy

**Risk**: Medium. The script uses bash features that `writeShellApplication`
validates with shellcheck. May need minor script adjustments.
**Validation**: Hibernate → restore cycle works. Cold boot works. Socat forwarding works.

### Phase 4: Guest VM Rewiring (guest sandbox binary from store)

Refactor `sandbox-vm.nix` to source the sandbox binary from `/nix/store` instead
of the `choiros-bin` virtiofs mount:
- Remove the `choiros-bin` virtiofs share (down to 2 shares: nix-store + creds)
- Change guest `ExecStart` to a nix store path (resolved at VM build time)
- The sandbox binary's runtime closure must be in the host's `/nix/store` (it will
  be, since building the VM runner already pulls it in)

This also means one fewer virtiofsd socket, further simplifying snapshot/restore.

**Risk**: Medium. Must verify the sandbox binary's full runtime closure is
accessible via the `nix-store` virtiofs mount.
**Validation**: VM boots, sandbox starts, health check passes.

### Phase 5: Safe Deployment Pipeline (CI simplification)

Replace CI deploy script with:
```bash
ssh root@$NODE_B 'cd /opt/choiros/workspace && git pull && nixos-rebuild boot --flake .#choiros-b'
```

Use `boot` (not `switch`) for safety — changes take effect on next reboot.
Add health check after reboot confirmation.

Consider `deploy-rs` for automatic rollback (magic rollback: if SSH connectivity
lost within 30s of activation, auto-reverts to previous generation).

**Risk**: Low if using `boot`. Medium if using `switch`.
**Validation**: Deploy to Node B, reboot, verify services come up.

### Phase 6: Debug Bridge IP

Investigate why `networking.interfaces.br-choiros.ipv4.addresses` doesn't
persist the bridge IP on Node B. This may be a systemd-networkd vs scripted
networking conflict. Fix the NixOS config so the CI workaround can be removed.

**Risk**: Low but requires understanding NixOS networking internals.
**Validation**: After `nixos-rebuild switch`, `ip addr show br-choiros` shows 10.0.0.1/24.

### Phase 7: Reassess microvm.nix Declarative Mode

With Phases 1-6 complete, evaluate whether `microvm.vms.sandbox-live` in the
host NixOS config would replace `ovh-runtime-ctl.sh` entirely. The tradeoff
is coupling (host rebuild triggers VM rebuild) vs simplicity (systemd manages
everything). This is a future decision, not a requirement for ADR-0016.

---

## VM Generation Pinning and GC Safety

The guest VM mounts the host's `/nix/store` via virtiofs. This creates a
liveness contract: the host must not garbage-collect store paths that a
running or hibernated VM depends on.

### How NixOS GC Works

`nix-collect-garbage` removes store paths that have no GC roots. GC roots are:
- The current system profile (`/nix/var/nix/profiles/system`)
- Previous generations (until explicitly deleted with `--delete-older-than`)
- Any symlink in `/nix/var/nix/gcroots/`
- Any path reachable from the above (transitive closure)

### What Keeps VM Store Paths Alive

The VM runner (e.g., `result-vm-live`) is a symlink to a nix store path.
That store path's closure includes: the guest NixOS system, the sandbox
binary, the kernel, initrd, and all runtime dependencies. As long as the
symlink exists, all of these are GC roots.

**Current state**: `result-vm-live` is a workspace symlink created by
`nix build -o result-vm-live`. This is a valid GC root as long as the
symlink exists in `/opt/choiros/workspace/`.

**Target state**: The VM runner store path should be registered as a
proper GC root in `/nix/var/nix/gcroots/`, independent of workspace
symlinks. This can be done by adding it to the host's NixOS system
closure (Phase 4) or by explicit `nix-store --add-root`.

### Deploy-Time VM Coordination

When a new system closure is built (new sandbox binary, new VM runner):

1. New runner closure is built → new store paths appear in `/nix/store`
2. Old runner symlink still exists → old store paths remain rooted
3. Running VMs continue using old binary (loaded in memory, not affected)
4. Hibernated VMs have their state saved including memory → on restore,
   they resume the old binary from memory (also not affected)
5. **Next cold boot** uses the new runner → new VM gets new binary

The transition is natural: VMs pick up new versions on their next cold
boot. No forced restart needed. Old store paths stay rooted until:
- All VMs using the old runner are stopped (not just hibernated)
- The old runner symlink is removed or overwritten
- `nix-collect-garbage` is run

### Safety Rules for GC

- Never run `nix-collect-garbage` while VMs are hibernated unless
  their runner closure is explicitly rooted
- After deploying a new runner, keep the old symlink until all
  hibernated VMs from that generation have been cold-booted on the new one
- In production: only GC after confirming all VMs are on the current generation

### Future: Per-User VM Generations

When each user has their own VM (ADR-0014 Phase 3+), each VM can be
on a different generation. This enables:
- **Safe upgrades**: Boot user's new VM with new generation, keep old
  VM hibernated as rollback
- **Gradual rollout**: Upgrade a subset of users, monitor, then proceed
- **User-initiated rollback**: User can switch back to their old VM
  if the new version has issues

Each user's VM runner closure is a separate GC root. GC is safe when
no VM (running or hibernated) references the store path.

---

## Safety Rules

1. **Always test on Node B first.** Never deploy untested changes to Node A.
2. **Use `nixos-rebuild boot`** for changes touching networking, kernel, or bootloader.
3. **Use `nixos-rebuild switch`** only for application-level changes (new binary version).
4. **Never `nixos-rebuild switch` on both nodes simultaneously.**
5. **Always `nixos-rebuild build`** first to verify the build succeeds before switch/boot.
6. **Keep previous generation accessible** — never `nix-collect-garbage` on production
   until the new generation is validated.
7. **Consider deploy-rs** for automatic rollback on SSH connectivity loss.

---

## References

- NixOS Wiki: nixos-rebuild — https://wiki.nixos.org/wiki/Nixos-rebuild
- Nixcademy: Magic Deployments — https://nixcademy.com/posts/nixos-rebuild/
- Serokell: deploy-rs — https://serokell.io/blog/deploy-rs
- Crane: Workspace builds — https://github.com/ipetkov/crane/discussions/31
- microvm.nix: Declaring MicroVMs — https://microvm-nix.github.io/microvm.nix/declaring.html
- ADR-0002: Rust + Nix Build and Cache Strategy
- ADR-0014: Per-User Storage and Desktop Sync
