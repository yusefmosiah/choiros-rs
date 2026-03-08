# Implementing ADR-0016: NixOS Declarative Deployment

Date: 2026-03-08
Kind: Guide
Status: Draft
Priority: 3
Requires: [ADR-0016]

## What This Guide Is

Sequenced implementation steps for ADR-0016. The ADR defines *what* and *why*.
This guide defines *how*, *where*, and *in what order*. Each phase has a
validation gate — pass it before moving to the next.

## Prerequisites

- SSH access to Node B (`ssh -i ~/.ssh/id_ed25519_ovh root@147.135.70.196`)
- Node A is production, do NOT touch it until Phase 5
- Current state: CI deploys binaries to `/opt/choiros/bin/` via `cp -f`
- Working virtio-blk + snapshot/restore on Node B (validated 2026-03-08)

## Current File Map

Understanding where everything lives today:

```
Root flake.nix
├── inputs: nixpkgs, flake-utils, disko, microvm
├── outputs: nixosConfigurations only (NO packages)
└── VM guest configs: choiros-ch-sandbox-{live,dev}

sandbox/flake.nix       → packages.sandbox    (crane + rust-overlay)
hypervisor/flake.nix    → packages.hypervisor  (crane + rust-overlay)
dioxus-desktop/flake.nix → packages.{web,desktop} (crane + wasm-bindgen)

CI deploys by:
  nix build ./sandbox#sandbox          → cp to /opt/choiros/bin/sandbox
  nix build ./hypervisor#hypervisor    → cp to /opt/choiros/bin/hypervisor
  nix build ./dioxus-desktop#web       → symlink at result-frontend
  nix build .#...microvm.runner...     → symlink at result-vm-live
  cp scripts/ops/ovh-runtime-ctl.sh    → /opt/choiros/bin/ovh-runtime-ctl
```

---

## Phase 1: Unified Root Flake

**Goal**: Root `flake.nix` builds all packages with shared dependencies.
Sub-flakes remain only for `nix develop` dev shells.

### Step 1.1: Add crane and rust-overlay to root flake inputs

```nix
# flake.nix inputs (add these)
crane.url = "github:ipetkov/crane";
rust-overlay = {
  url = "github:oxalica/rust-overlay";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

All three sub-flakes use identical inputs (`nixpkgs/nixos-unstable`,
`crane`, `rust-overlay`). Unifying them is safe.

### Step 1.2: Define shared Rust build infrastructure

Inside the `outputs` function, for `x86_64-linux`:

```nix
let
  system = "x86_64-linux";
  pkgs = import nixpkgs {
    inherit system;
    overlays = [
      (import rust-overlay)
      (import ./nix/overlays/virtiofsd-vhost-fix.nix)
    ];
  };

  toolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = [ "rust-src" "rustfmt" "clippy" ];
    targets = [ "wasm32-unknown-unknown" ];
  };

  craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

  # Workspace source with filters
  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      (craneLib.filterCargoSources path type)
      || (builtins.baseNameOf path) == "Cargo.lock"
      || (pkgs.lib.hasInfix "/.sqlx/" (toString path))
      || (pkgs.lib.hasInfix "/migrations/" (toString path))
      || (pkgs.lib.hasInfix "/config/" (toString path))
      || (pkgs.lib.hasInfix "/public/" (toString path));
  };

  commonArgs = {
    inherit src;
    strictDeps = true;
    nativeBuildInputs = with pkgs; [ pkg-config protobuf ];
    buildInputs = with pkgs; [ openssl ];
    SQLX_OFFLINE = "true";
  };

  # Build workspace deps ONCE (shared across sandbox + hypervisor)
  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
    pname = "choiros-workspace";
    version = "0.1.0";
  });
in
```

### Step 1.3: Define package outputs

```nix
packages.${system} = {
  sandbox = craneLib.buildPackage (commonArgs // {
    inherit cargoArtifacts;
    pname = "sandbox";
    version = "0.1.0";
    cargoExtraArgs = "-p sandbox --bin sandbox";
  });

  hypervisor = craneLib.buildPackage (commonArgs // {
    inherit cargoArtifacts;
    pname = "hypervisor";
    version = "0.1.0";
    cargoExtraArgs = "-p hypervisor --bin hypervisor";
  });

  frontend = /* copy from dioxus-desktop/flake.nix packages.web */;
};
```

The frontend (WASM) build is more complex — it needs a separate WASM
crane instance. Copy the `wasmToolchain`, `wasmCraneLib`, `wasmBuild`,
and `packages.web` definitions from `dioxus-desktop/flake.nix` into
the root flake. The shared-types handling (`postPatch`) must be preserved.

### Step 1.4: Keep sub-flakes for dev shells

Sub-flakes (`sandbox/flake.nix`, etc.) remain unchanged. They provide
`nix develop` shells for local development. They are NOT used for
deployment builds anymore.

### Validation Gate 1

```bash
# On Node B (or any x86_64-linux):
cd /opt/choiros/workspace
git pull

# Build from root flake (new)
nix build .#sandbox -o result-sandbox-new
nix build .#hypervisor -o result-hypervisor-new

# Build from sub-flakes (old)
nix build ./sandbox#sandbox -o result-sandbox-old
nix build ./hypervisor#hypervisor -o result-hypervisor-old

# Compare binaries (should be identical or functionally equivalent)
diff <(file result-sandbox-new/bin/sandbox) <(file result-sandbox-old/bin/sandbox)
ls -la result-sandbox-new/bin/sandbox result-sandbox-old/bin/sandbox

# Smoke test: run the binary
result-sandbox-new/bin/sandbox --help 2>&1 || true
result-hypervisor-new/bin/hypervisor --help 2>&1 || true
```

Binaries may differ in hash (different build environment) but must be
the same size and produce identical behavior.

**Do not proceed until both packages build successfully from the root flake.**

---

## Phase 2: Host Unit Rewiring

**Goal**: Hypervisor systemd service references nix store paths, not
`/opt/choiros/bin/`.

### Step 2.1: Pass packages to NixOS config via specialArgs

In root `flake.nix`, update nixosConfigurations:

```nix
nixosConfigurations.choiros-b = nixpkgs.lib.nixosSystem {
  system = "x86_64-linux";
  specialArgs = {
    choirosPackages = self.packages.x86_64-linux;
  };
  modules = [
    disko.nixosModules.disko
    ./nix/hosts/ovh-node-disk-config.nix
    ./nix/hosts/ovh-node-b.nix
  ];
};
```

### Step 2.2: Update ovh-node.nix to use package paths

```nix
# nix/hosts/ovh-node.nix
{ config, lib, pkgs, choirosPackages, ... }:
{
  # ... existing config ...

  systemd.services.hypervisor = {
    serviceConfig = {
      ExecStart = "${choirosPackages.hypervisor}/bin/hypervisor";
      # Remove WorkingDirectory — all paths must be absolute via env
      Environment = [
        "HYPERVISOR_PORT=9090"
        "HYPERVISOR_DATABASE_URL=sqlite:/opt/choiros/data/hypervisor.db"
        "SANDBOX_VFKIT_CTL=/opt/choiros/bin/ovh-runtime-ctl"  # Phase 3 replaces this
        "SANDBOX_LIVE_PORT=8080"
        "SANDBOX_DEV_PORT=8081"
        "FRONTEND_DIST=${choirosPackages.frontend}"
      ];
    };
  };
}
```

### Step 2.3: Audit CARGO_MANIFEST_DIR fallbacks in hypervisor

There are 3 instances in `hypervisor/src/`:

```
hypervisor/src/config.rs:47    — workspace_root fallback
hypervisor/src/config.rs:152   — workspace_root fallback
hypervisor/src/bin/vfkit-runtime-ctl.rs:181 — (vfkit-specific, not used on OVH)
```

For `config.rs`: the systemd Environment passes all needed paths.
Verify that `HYPERVISOR_DATABASE_URL` and `FRONTEND_DIST` are always
set. The `CARGO_MANIFEST_DIR` fallback in config.rs is used to derive
these when env vars are absent. With explicit env vars, the fallback
is dead code — but leave it for now (local dev still uses it).

### Validation Gate 2

```bash
# On Node B:
cd /opt/choiros/workspace && git pull

# Build the full system closure (does not activate)
nixos-rebuild build --flake .#choiros-b

# Inspect the generated hypervisor unit
cat /nix/var/nix/profiles/system/etc/systemd/system/hypervisor.service
# Verify ExecStart points to /nix/store/..., not /opt/choiros/bin/

# Dry-run: see what services would restart
nixos-rebuild dry-activate --flake .#choiros-b

# If it only restarts hypervisor, proceed
nixos-rebuild switch --flake .#choiros-b

# Verify
systemctl is-active hypervisor
curl -fsS http://127.0.0.1:9090/login | head -1
```

**Do not proceed until the hypervisor starts and serves requests
with `ExecStart` pointing to a nix store path.**

---

## Phase 3: Package runtime-ctl

**Goal**: `ovh-runtime-ctl` is a nix derivation with injected store paths.
No more mutable workspace symlinks for runner discovery.

### Step 3.1: Create the derivation

File: `nix/packages/ovh-runtime-ctl.nix`

```nix
{ pkgs, vmRunnerLive }:

let
  # The runner directory contains bin/{microvm-run,virtiofsd-run,tap-up}
  # and share/microvm/{system,virtiofs/*/source,tap-interfaces,...}
  runnerPath = "${vmRunnerLive}";
in
pkgs.writeShellApplication {
  name = "ovh-runtime-ctl";
  runtimeInputs = with pkgs; [
    btrfs-progs
    cloud-hypervisor
    coreutils
    curl
    findutils
    gnugrep
    gnused
    gawk
    iproute2
    procps
    socat
    util-linux
  ];
  text = builtins.readFile ../../scripts/ops/ovh-runtime-ctl.sh;
}
```

**Problem**: `writeShellApplication` runs shellcheck, and the current
script uses bash features (associative arrays, etc.) that may trigger
warnings. Also, we need to inject the runner path.

**Alternative approach** if shellcheck is too strict:

```nix
pkgs.writeScriptBin "ovh-runtime-ctl" ''
  #!${pkgs.bash}/bin/bash
  export PATH="${pkgs.lib.makeBinPath runtimeInputs}:$PATH"
  ${builtins.readFile ../../scripts/ops/ovh-runtime-ctl.sh}
''
```

### Step 3.2: Inject runner path into the script

The script currently has:
```bash
RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"
```

Replace with a fixed store path. Two approaches:

**A. Environment variable (simpler)**:
Set `CHOIR_VM_RUNNER_DIR` in the hypervisor systemd Environment:
```nix
"CHOIR_VM_RUNNER_DIR=${vmRunnerLive}"
```
And update the script:
```bash
RUNNER_DIR="${CHOIR_VM_RUNNER_DIR:-${WORKSPACE}/result-vm-${VM_NAME}}"
```

**B. Build-time substitution (purer)**:
Use `substituteAll` or string replacement in the derivation to bake
the path in. This eliminates the env var dependency but makes the
script less portable for debugging.

**Recommendation**: Use approach A (env var). It's explicit, debuggable,
and the hypervisor systemd unit already manages environment.

### Step 3.3: Wire into flake and NixOS config

In root `flake.nix`:
```nix
packages.${system}.runtime-ctl = pkgs.callPackage ./nix/packages/ovh-runtime-ctl.nix {
  vmRunnerLive = self.nixosConfigurations.choiros-ch-sandbox-live
    .config.microvm.runner.cloud-hypervisor;
};
```

In `ovh-node.nix`:
```nix
Environment = [
  "SANDBOX_VFKIT_CTL=${choirosPackages.runtime-ctl}/bin/ovh-runtime-ctl"
  "CHOIR_VM_RUNNER_DIR=${vmRunnerLive}"
  # ... other env vars
];
```

### Step 3.4: Update ovh-runtime-ctl.sh for env var runner path

```bash
# At top of script, replace:
WORKSPACE="${CHOIR_WORKSPACE_ROOT:-/opt/choiros/workspace}"
# With:
VM_RUNNER_DIR="${CHOIR_VM_RUNNER_DIR:-}"

# And replace:
RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"
# With:
if [[ -n "$VM_RUNNER_DIR" ]]; then
  RUNNER_DIR="$VM_RUNNER_DIR"
else
  RUNNER_DIR="${CHOIR_WORKSPACE_ROOT:-/opt/choiros/workspace}/result-vm-${VM_NAME}"
fi
```

This keeps backward compatibility during transition while preferring
the store path when available.

### Validation Gate 3

```bash
# On Node B:
nixos-rebuild switch --flake .#choiros-b

# Verify runtime-ctl is a store path
readlink $(which ovh-runtime-ctl) || true
systemctl show hypervisor | grep SANDBOX_VFKIT_CTL
# Should show /nix/store/...-ovh-runtime-ctl/bin/ovh-runtime-ctl

# Full VM lifecycle test
/opt/choiros/bin/ovh-runtime-ctl stop --runtime sandbox --role live --port 8080 --user-id test
systemctl restart hypervisor
# Wait for sandbox to boot
curl -fsS --retry 20 --retry-delay 3 http://10.0.0.10:8080/health

# Hibernate + restore
/opt/choiros/bin/ovh-runtime-ctl hibernate --runtime sandbox --role live --port 8080 --user-id test
/opt/choiros/bin/ovh-runtime-ctl ensure --runtime sandbox --role live --port 8080 --user-id test
curl -fsS http://10.0.0.10:8080/health
echo "PASS: runtime-ctl from nix store, full lifecycle works"
```

**Do not proceed until hibernate → restore works with the packaged runtime-ctl.**

---

## Phase 4: Guest VM Rewiring

**Goal**: Guest sandbox binary comes from `/nix/store` (via existing
virtiofs nix-store mount), not from the `choiros-bin` virtiofs share.

### Step 4.1: Update sandbox-vm.nix

The guest service currently does:
```nix
ExecStart = "/opt/choiros/bin/sandbox";
```
With `/opt/choiros/bin` mounted via virtiofs from the host.

Change to reference the sandbox package directly:

```nix
{ config, lib, pkgs, sandboxRole, sandboxPort, vmIp, vmMac, vmTap,
  sandboxPackage, ... }:
{
  systemd.services.choir-sandbox = {
    serviceConfig = {
      ExecStart = "${sandboxPackage}/bin/sandbox";
      # ... rest unchanged
    };
  };
}
```

Pass `sandboxPackage` via specialArgs in `flake.nix`:
```nix
nixosConfigurations.choiros-ch-sandbox-live = nixpkgs.lib.nixosSystem {
  specialArgs = {
    sandboxPackage = self.packages.x86_64-linux.sandbox;
    # ... other specialArgs
  };
};
```

### Step 4.2: Remove choiros-bin virtiofs share

In `sandbox-vm.nix`, remove:
```nix
{
  proto = "virtiofs";
  tag = "choiros-bin";
  source = "/opt/choiros/bin";
  mountPoint = "/opt/choiros/bin";
}
```

Down to 2 shares: `nix-store` + `choiros-creds`.

### Step 4.3: Update runtime-ctl socket count

```bash
# 2 shares now: nix-store, choiros-creds
if (( sock_count >= 2 )); then
```

### Step 4.4: Verify store closure includes sandbox

The sandbox binary's full runtime closure (libc, openssl, etc.) must
be in the host's `/nix/store`. Building the VM runner pulls in the
guest NixOS system which includes the sandbox package, so this should
be automatic. Verify:

```bash
nix-store -qR $(readlink result-vm-live) | grep sandbox
# Should show the sandbox store path
```

### Validation Gate 4

```bash
# On Node B: rebuild VM runner + host
nixos-rebuild switch --flake .#choiros-b

# Stop old VM, rebuild runner
nix build .#nixosConfigurations.choiros-ch-sandbox-live.config.microvm.runner.cloud-hypervisor \
  -o result-vm-live

# Stop and restart VM with new runner
ovh-runtime-ctl stop --runtime sandbox --role live --port 8080 --user-id test
systemctl restart hypervisor

# Wait for health
curl -fsS --retry 20 --retry-delay 3 http://10.0.0.10:8080/health

# Verify only 2 virtiofs sockets
ls /opt/choiros/vms/state/live/*-virtiofs-*.sock
# Should show: nix-store.sock, choiros-creds.sock (NO choiros-bin.sock)

# Verify sandbox binary is from nix store
ssh ... # (if guest SSH is available) readlink /proc/1/exe
# Or check cloud-hypervisor cmdline for the --fs shares

# Hibernate + restore test
ovh-runtime-ctl hibernate --runtime sandbox --role live --port 8080 --user-id test
ovh-runtime-ctl ensure --runtime sandbox --role live --port 8080 --user-id test
curl -fsS http://10.0.0.10:8080/health
echo "PASS: guest runs sandbox from nix store, 2 virtiofs shares, lifecycle works"
```

---

## Phase 5: CI Simplification

**Goal**: CI deploy becomes `nixos-rebuild` instead of component builds + cp.

### Step 5.1: Update ci.yml deploy step

Replace the entire deploy script:

```yaml
- name: Deploy to staging
  run: |
    ssh -i ~/.ssh/deploy_key -o StrictHostKeyChecking=no root@${{ secrets.OVH_NODE_B_HOST }} '
      set -euo pipefail
      cd /opt/choiros/workspace

      echo "==> Pulling latest code"
      git pull --ff-only origin main

      echo "==> Building and activating NixOS config"
      nixos-rebuild switch --flake .#choiros-b 2>&1

      echo "==> Waiting for hypervisor"
      sleep 2
      curl -fsS --retry 10 --retry-delay 3 --retry-all-errors http://127.0.0.1:9090/login > /dev/null

      echo "==> Staging deploy complete"
    '
```

This builds everything: sandbox, hypervisor, frontend, VM runner, runtime-ctl,
and system config. All in one atomic operation.

### Step 5.2: Update promote.yml

Same pattern for Node A:

```yaml
ssh root@${{ secrets.OVH_NODE_A_HOST }} '
  cd /opt/choiros/workspace
  git pull --ff-only origin main
  nixos-rebuild boot --flake .#choiros-a
  echo "Reboot required to activate new config"
'
```

Note: `boot` not `switch` for production. Requires manual reboot
(or automated reboot with health check).

For application-only changes (no kernel/networking), `switch` is safe:
```yaml
nixos-rebuild switch --flake .#choiros-a
```

Decide per-deploy based on what changed.

### Step 5.3: Remove legacy artifacts

- Remove `/opt/choiros/bin/` tmpfiles rules from `ovh-node.nix`
- Remove `WORKSPACE` references from runtime-ctl (only store paths)
- Remove bridge IP workaround from CI (fix in Phase 6)

### Validation Gate 5

```bash
# Push a trivial code change, verify CI:
# 1. CI runs nixos-rebuild switch on Node B
# 2. Hypervisor starts and serves requests
# 3. E2E tests pass
# 4. No cp -f, no /opt/choiros/bin/ references in CI logs
```

---

## Phase 6: Debug Bridge IP

**Goal**: Bridge gets its IP from NixOS config, not CI workaround.

### Step 6.1: Investigate

On Node B:
```bash
# Check if scripted networking or systemd-networkd manages the bridge
networkctl status br-choiros
systemctl status systemd-networkd
cat /etc/systemd/network/*.network 2>/dev/null

# Check NixOS-generated network config
cat /etc/nixos/configuration.nix  # (if exists)
ls /etc/systemd/network/

# Check what NixOS scripted networking does
systemctl status network-setup.service
journalctl -u network-setup.service | tail -20
```

### Step 6.2: Fix

Likely cause: NixOS scripted networking creates the bridge but
systemd-networkd doesn't assign the IP (or vice versa). The fix
depends on the investigation. Common solutions:

**If scripted networking**: Verify `networking.interfaces.br-choiros`
config generates correct ifcfg scripts.

**If systemd-networkd**: Add explicit networkd config:
```nix
systemd.network.networks."20-br-choiros" = {
  matchConfig.Name = "br-choiros";
  networkConfig.Address = "10.0.0.1/24";
};
```

### Validation Gate 6

```bash
# Reboot Node B
# After boot, without any manual intervention:
ip addr show br-choiros | grep "10.0.0.1"
# Must show the IP without CI workaround
```

---

## Phase 7: Remove CARGO_MANIFEST_DIR Fallbacks

**Goal**: Production code panics on missing env vars instead of
silently falling back to nonexistent paths.

This is a code quality phase, not a NixOS config phase. It can be
done in parallel with any phase above.

### Step 7.1: Audit all 22 instances in sandbox

```bash
grep -rn 'CARGO_MANIFEST_DIR' sandbox/src/
```

Each instance follows the pattern:
```rust
env::var("CHOIR_SANDBOX_ROOT")
    .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
```

### Step 7.2: Replace with explicit error

For production code paths:
```rust
fn sandbox_root() -> PathBuf {
    PathBuf::from(
        env::var("CHOIR_SANDBOX_ROOT")
            .expect("CHOIR_SANDBOX_ROOT must be set")
    )
}
```

For code that runs in tests or local dev, keep the fallback but
gate it on a feature flag or `cfg!(test)`:
```rust
fn sandbox_root() -> PathBuf {
    PathBuf::from(
        env::var("CHOIR_SANDBOX_ROOT").unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                env!("CARGO_MANIFEST_DIR").to_string()
            } else {
                panic!("CHOIR_SANDBOX_ROOT must be set in production")
            }
        })
    )
}
```

### Step 7.3: Create a shared helper

All 22 instances should use one function. Create in a shared module:

```rust
// sandbox/src/paths.rs
use std::path::PathBuf;

pub fn sandbox_root() -> PathBuf {
    env_path("CHOIR_SANDBOX_ROOT")
}

pub fn writer_root() -> PathBuf {
    std::env::var("CHOIR_WRITER_ROOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| sandbox_root())
}

fn env_path(var: &str) -> PathBuf {
    PathBuf::from(
        std::env::var(var).unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                env!("CARGO_MANIFEST_DIR").to_string()
            } else {
                panic!("{var} must be set")
            }
        })
    )
}
```

### Validation Gate 7

```bash
# All tests pass locally
cargo test --workspace --lib --bins

# Deploy to Node B, verify no panics
nixos-rebuild switch --flake .#choiros-b
curl -fsS http://10.0.0.10:8080/health
journalctl -u hypervisor --no-pager -n 50 | grep -i panic
# Must show no panics
```

---

## Summary: What Changes Where

| Phase | Files Modified | Deployed Via |
|-------|---------------|-------------|
| 1 | `flake.nix` | `nix build .#sandbox` (validate only) |
| 2 | `flake.nix`, `nix/hosts/ovh-node.nix` | `nixos-rebuild switch` on Node B |
| 3 | `nix/packages/ovh-runtime-ctl.nix` (new), `scripts/ops/ovh-runtime-ctl.sh`, `flake.nix`, `ovh-node.nix` | `nixos-rebuild switch` on Node B |
| 4 | `nix/ch/sandbox-vm.nix`, `flake.nix`, `ovh-runtime-ctl.sh` | `nixos-rebuild switch` + VM restart on Node B |
| 5 | `.github/workflows/ci.yml`, `.github/workflows/promote.yml` | CI pipeline |
| 6 | `nix/hosts/ovh-node.nix` | `nixos-rebuild switch` on Node B |
| 7 | `sandbox/src/**/*.rs` (22 files) | `nixos-rebuild switch` on Node B |

## Estimated Effort

| Phase | Effort | Risk | Dependencies |
|-------|--------|------|-------------|
| 1 | Medium | Medium | None |
| 2 | Low | Low | Phase 1 |
| 3 | Medium | Medium | Phase 2 |
| 4 | Low | Medium | Phase 3 |
| 5 | Low | Low | Phase 4 |
| 6 | Low | Low | Independent |
| 7 | Medium | Low | Independent |

Phases 6 and 7 can be done in parallel with any other phase.
Phases 1-5 are sequential.
