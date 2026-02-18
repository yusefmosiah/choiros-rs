# Nix Setup Runbook

Narrative Summary (1-minute read)
----------------------------------
We are setting up a Nix-first development and deployment stack for ChoirOS. This
runbook covers three phases:

1. **Mac** — nix-darwin + home-manager, replacing Homebrew for tooling
2. **Mac: local VMs** — microvm.nix (vfkit backend) running NixOS, for running
   sandbox instances in isolation using Apple Virtualization.framework
3. **EC2** — NixOS host running the sandbox in rootless Podman containers

The sandbox (Rust/axum/ractor/libsql/baml) is the primary workload. The
dioxus-desktop frontend (WASM, built with `dx`) and hypervisor (thin Rust router,
not yet built out) each get their own Nix flake derivations.

What Changed
------------
Nothing yet. This is a forward-looking runbook.

What To Do Next
---------------
Start at Phase 1, Step 1.1. Each phase is a prerequisite for the next.
Before Phase 3: run `cargo sqlx prepare --workspace` in sandbox/ with a running DB
and commit the `.sqlx/` directory — required for the crane build to work offline.

---

## Dependency Map

Before writing any Nix, understand what we're building:

```
Workspace: choiros-rs/
├── sandbox/          axum + ractor + libsql + baml + portable-pty
│                     runtime deps: SQLite DB file, .env secrets, static/ dir
│                     build deps: cargo, sqlx-cli (for migrations), baml codegen
├── shared-types/     serde + ts-rs (generates TypeScript bindings)
├── hypervisor/       thin tokio router, minimal deps (not built out yet)
└── dioxus-desktop/   Dioxus 0.7 WASM frontend
                      build deps: dx CLI, wasm-pack/wasm-bindgen
                      excludes from workspace Cargo.toml (separate build)
```

Key build-time requirements:
- Rust 1.88+ (current stable)
- `sqlx-cli` with SQLite feature (for `cargo sqlx migrate run`)
- `baml-cli` — codegen tool, run manually, NOT needed by `cargo build` (see below)
- `dx` (dioxus CLI 0.7.3) for the frontend — in nixpkgs-unstable as `dioxus-cli`
- `wasm32-unknown-unknown` target for dioxus-desktop
- `just` task runner
- `libsql` links against sqlite3 at build time (needs `libsqlite3` or bundled)

### baml: library vs CLI distinction

This is important. There are two separate baml dependencies with different Nix implications:

**`baml` crate (runtime library, version 0.218)**
- Listed in `Cargo.toml` / `Cargo.lock`
- Used by `cargo build` to compile the sandbox binary
- Handled by Cargo's normal dependency resolution — fully reproducible via `Cargo.lock`
- No special Nix treatment needed

**`baml-cli` (codegen tool, version 0.218)**
- Reads `baml_src/*.baml` files and generates `sandbox/src/baml_client/`
- The generated `baml_client/` is **checked into git** — it is not gitignored
- There is **no `build.rs`** in sandbox — `cargo build` never calls baml-cli
- Therefore: `cargo build` does NOT need baml-cli at all
- baml-cli is only needed when you edit `.baml` source files and need to regenerate

This means baml-cli is a developer tool like `sqlx-cli`, not a build-time dependency.
It is not in nixpkgs. The pragmatic approach and the pure approach are documented in
the devShell section below.

### dioxus-cli (dx): confirmed in nixpkgs-unstable

`dioxus-cli` is in nixpkgs-unstable at version **0.7.3** (matching what is installed).
The nixpkgs derivation:
- Builds from source via `rustPlatform.buildRustPackage` — fully pure
- Uses `no-downloads` feature (no network at build time, as required by Nix)
- Pins and wraps `wasm-bindgen-cli` at `0.2.108` automatically — `dx` will find it
- `disable-telemetry` is on by default

Use `pkgs.dioxus-cli` directly. No custom derivation needed.

Runtime requirements (sandbox):
- SQLite DB at `../data/events.db` relative to sandbox/ (configurable via DATABASE_URL)
- `.env` file with: AWS_REGION, AWS_BEARER_TOKEN_BEDROCK, OPENAI_API_KEY, ZAI_API_KEY,
  KIMI_API_KEY, RESEND_API_KEY, TAVILY_API_KEY, BRAVE_API_KEY, EXA_API_KEY,
  SPRITES_API_TOKEN, CHOIR_SANDBOX_PROVIDER

Secrets are NOT baked into Nix configs. They are injected at runtime via:
- `.env` file on the host (dev)
- Container env vars or AWS Secrets Manager (prod)

---

## Phase 1: nix-darwin + home-manager on macOS

### 1.1 Install Nix

Using the Determinate Systems installer — it enables flakes by default and
has a real uninstaller (the official installer has neither).

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix \
  | sh -s -- install

# Restart your shell, then verify:
nix --version   # should print nix (Nix) 2.x.x
```

### 1.2 Create the nix-darwin flake

```bash
mkdir -p ~/.config/nix-darwin
cd ~/.config/nix-darwin
```

Create `flake.nix`:

```nix
{
  description = "ChoirOS macOS dev environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    nix-darwin = {
      url = "github:nix-darwin/nix-darwin/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    home-manager = {
      url = "github:nix-community/home-manager/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, nix-darwin, home-manager }: {
    darwinConfigurations."Yusefs-iMac" = nix-darwin.lib.darwinSystem {
      system = "aarch64-darwin";
      modules = [
        ./configuration.nix
        home-manager.darwinModules.home-manager
        {
          home-manager.useGlobalPkgs = true;
          home-manager.useUserPackages = true;
          home-manager.users.wiz = import ./home.nix;
        }
      ];
    };
  };
}
```

Create `configuration.nix` (system-level — manages Nix itself and system packages):

```nix
{ pkgs, ... }: {
  # System packages — tools needed globally
  environment.systemPackages = with pkgs; [
    # Rust toolchain is managed per-project via rust-overlay or rustup
    # Keep rustup here for now; migrate to per-project flakes later
    rustup

    # Build tools
    just
    pkg-config

    # CLI utilities
    git
    ripgrep
    fd
    bat
    jq
    curl
    wget
  ];

  # Use zsh system-wide
  programs.zsh.enable = true;

  # Allow nix-darwin to manage /etc/nix/nix.conf
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  system.stateVersion = 5;
  nixpkgs.hostPlatform = "aarch64-darwin";
}
```

Create `home.nix` (user-level — dotfiles, programs, user packages):

```nix
{ pkgs, ... }: {
  home.stateVersion = "25.11";
  home.username = "wiz";
  home.homeDirectory = "/Users/wiz";

  # Git config (replaces ~/.gitconfig)
  programs.git = {
    enable = true;
    userName = "wiz";
    userEmail = "yusef@choir.chat";  # update as needed
    extraConfig = {
      init.defaultBranch = "main";
      pull.rebase = true;
    };
  };

  # Zsh config
  programs.zsh = {
    enable = true;
    autosuggestion.enable = true;
    syntaxHighlighting.enable = true;
    shellAliases = {
      ll = "ls -la";
      g = "git";
      cb = "cargo build";
      ct = "cargo test";
      cr = "cargo run";
    };
    initContent = ''
      # Rust (via rustup, until per-project flakes)
      export PATH="$HOME/.cargo/bin:$PATH"
    '';
  };

  # User-scope packages
  home.packages = with pkgs; [
    htop
    tree
    tmux
  ];

  programs.home-manager.enable = true;
}
```

### 1.3 Bootstrap (first time only)

```bash
cd ~/.config/nix-darwin

# nix-darwin isn't installed yet, so we bootstrap with nix run:
nix run nix-darwin/master#darwin-rebuild -- switch --flake .

# After this, darwin-rebuild is on PATH. Future updates:
darwin-rebuild switch --flake ~/.config/nix-darwin
```

### 1.4 Per-project dev shell for choiros-rs

Rather than putting Rust in the global config, use a per-project `flake.nix`
dev shell. This pins the exact toolchain and native deps the project needs.

Add `flake.nix` to the repo root (or use `nix develop` from the existing one
once created):

```nix
{
  description = "ChoirOS sandbox dev shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Pin to the exact Rust version the workspace specifies
        rustToolchain = pkgs.rust-bin.stable."1.88.0".default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [ "wasm32-unknown-unknown" ];  # for dioxus-desktop
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain

            # Native deps for sandbox crates
            pkg-config
            openssl
            sqlite          # for libsql (sqlite3 header)
            libiconv        # macOS only — required by some crates

            # Build tools
            just
            sqlx-cli        # for cargo sqlx migrate run

            # baml-cli: NOT in nixpkgs. Intentionally omitted from pure inputs.
            # See "baml-cli: impure dev tool" note in shellHook below.

            # Dev utilities
            cargo-watch
            cargo-nextest
          ];

          # On macOS, tell linker where to find sqlite3
          PKG_CONFIG_PATH = "${pkgs.sqlite.dev}/lib/pkgconfig";
          LIBSQLITE3_SYS_USE_PKG_CONFIG = "1";

          shellHook = ''
            echo "ChoirOS dev shell ready"
            echo "Rust: $(rustc --version)"
            echo ""
            echo "NOTE: baml-cli is not managed by Nix."
            echo "  baml_client/ is checked in — cargo build works without it."
            echo "  Only needed when editing baml_src/*.baml files."
            echo "  To install: cargo install baml-cli@0.218.0"
            if command -v baml &>/dev/null; then
              echo "  baml-cli: $(baml --version)"
            else
              echo "  baml-cli: not installed"
            fi
            echo ""
            echo "Run 'just dev-sandbox' to start the backend"
          '';
        };
      }
    );
}
```

Usage:
```bash
cd ~/choiros-rs
nix develop          # drops into dev shell with all deps
just dev-sandbox     # runs sandbox
just test            # runs tests
```

#### baml-cli: the impurity tradeoff

`cargo install baml-cli@0.218.0` is intentionally impure. This is acceptable because:

1. The generated output (`sandbox/src/baml_client/`) is checked into git
2. `cargo build` never invokes baml-cli — it compiles the already-generated files
3. baml-cli is only run when `.baml` source files change, which is infrequent
4. The version is pinned explicitly in `baml_src/generators.baml` (`version "0.217.0"`)

The tradeoff: a developer's local `baml-cli` install is not content-addressed and
differs from what CI might use. Acceptable for a codegen tool whose output is reviewed
via normal git diff before commit.

**If you want a pure derivation for baml-cli** (e.g., for CI reproducibility),
create `nix/baml-cli.nix`:

```nix
{ rustPlatform, fetchCrate, pkg-config, openssl, lib }:

rustPlatform.buildRustPackage rec {
  pname = "baml-cli";
  version = "0.218.0";

  src = fetchCrate {
    inherit pname version;
    # Get hash: nix-prefetch-url --unpack \
    #   https://crates.io/api/v1/crates/baml-cli/0.218.0/download
    hash = "sha256-AAAA...";
  };

  # Get cargoHash: set to lib.fakeHash, run nix build, copy hash from error
  cargoHash = "sha256-BBBB...";

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl ];

  meta = {
    description = "BAML code generator CLI";
    homepage = "https://boundaryml.com";
    license = lib.licenses.asl20;
    mainProgram = "baml";
  };
}
```

Add to devShell buildInputs: `(pkgs.callPackage ./nix/baml-cli.nix {})`

This is a ~30-minute task to get the hashes right. Do it when you next bump baml
versions or need CI to run `baml generate`.

---

## Phase 2: microvm.nix + vfkit on macOS (NixOS agent VMs)

This runs NixOS VMs locally using Apple's Virtualization.framework via vfkit.
Use case: run the sandbox in an isolated VM with the workspace mounted via virtiofs.

### Constraints (confirmed from microvm.nix docs)

- virtiofs shares: YES — built in, no separate virtiofsd daemon
- TAP/bridge networking: NO — user-mode (port forwarding) only
- 9p shares: NO — virtiofs only
- Requires: macOS 13+, Apple Silicon or Intel with VT-x

### 2.1 Add vfkit and microvm.nix

Add `vfkit` to `configuration.nix` systemPackages:
```nix
environment.systemPackages = with pkgs; [
  # ... existing packages ...
  vfkit   # Apple Virtualization.framework CLI
];
```

Add microvm.nix input to `~/.config/nix-darwin/flake.nix`:

```nix
inputs = {
  # ... existing inputs ...
  microvm = {
    url = "github:microvm-nix/microvm.nix";
    inputs.nixpkgs.follows = "nixpkgs";
  };
};
```

### 2.2 Define a sandbox VM

Add to `~/.config/nix-darwin/flake.nix` outputs, alongside darwinConfigurations:

```nix
# NixOS VM for running the sandbox locally
nixosConfigurations.sandbox-vm = nixpkgs.lib.nixosSystem {
  system = "aarch64-linux";  # arm64 — matches Apple Silicon host
  modules = [
    microvm.nixosModules.microvm
    ({ pkgs, ... }: {
      microvm = {
        hypervisor = "vfkit";
        vcpu = 4;
        mem = 4096;  # 4 GB

        # Workspace mounted read-write via virtiofs
        shares = [
          {
            proto = "virtiofs";
            tag = "workspace";
            source = "/Users/wiz/choiros-rs";
            mountPoint = "/workspace";
          }
          {
            proto = "virtiofs";
            tag = "data";
            source = "/Users/wiz/.choiros-data";  # persisted DB + logs
            mountPoint = "/data";
          }
          # NOTE: .env secrets are NOT mounted here.
          # Inject at runtime via SSH + env vars or a secrets file.
        ];

        # vfkit: no TAP, user networking only
        # SSH accessible via port forwarding
      };

      services.openssh = {
        enable = true;
        settings.PermitRootLogin = "yes";  # dev convenience; harden for prod
      };

      environment.systemPackages = with pkgs; [
        git
        sqlite
        just
        ripgrep
        curl
        # Rust toolchain for building inside the VM
        (rust-bin.stable.latest.default.override {
          extensions = [ "clippy" "rustfmt" ];
        })
        sqlx-cli
        pkg-config
        openssl
      ];

      # rust-overlay for the VM
      nixpkgs.overlays = [ rust-overlay.overlays.default ];

      system.stateVersion = "25.11";
    })
  ];
};
```

You'll also need `rust-overlay` in flake inputs (same as Phase 1).

### 2.3 Prepare host dirs and run

```bash
mkdir -p ~/.choiros-data

# Build and run the VM
nix run .#nixosConfigurations.sandbox-vm.config.microvm.runner

# In another terminal, SSH in (port forwarding — vfkit assigns a port)
# Check microvm docs for exact port forwarding syntax with vfkit
ssh root@localhost -p <forwarded-port>

# Inside VM:
cd /workspace
DATABASE_URL=/data/events.db just dev-sandbox
```

### 2.4 Secrets inside the VM

Do not mount `.env` via virtiofs (it has live API keys). Options:
- SSH in and `export` vars manually for dev sessions
- Use a separate virtiofs share pointing to a `.env` file that is gitignored
  and lives outside the repo: `source = "/Users/wiz/.choiros-secrets/sandbox.env"`
- For prod: use AWS Secrets Manager and fetch at startup

---

## Phase 3: NixOS on EC2 + rootless Podman

### 3.1 Launch a NixOS EC2 instance

```bash
# Find the latest NixOS 25.11 AMI for x86_64
aws ec2 describe-images \
  --owners 427812963091 \
  --filters 'Name=name,Values=nixos/25.11*' \
             'Name=architecture,Values=x86_64' \
  --query 'sort_by(Images, &CreationDate)[-1].{ID:ImageId,Name:Name}' \
  --output table

# Launch (t3.large recommended: 2 vCPU, 8 GB — sandbox + ractor actors)
aws ec2 run-instances \
  --image-id ami-XXXXXXXX \
  --instance-type t3.large \
  --key-name your-key \
  --security-group-ids sg-XXXXXXXX \
  --block-device-mappings 'DeviceName=/dev/xvda,Ebs={VolumeSize=40}' \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=choiros-sandbox}]'

ssh root@<ec2-ip>   # NixOS AMI default user is root
```

### 3.2 NixOS configuration for EC2

Push your NixOS config to the instance via nixos-rebuild or nixos-anywhere.
The EC2 instance's `/etc/nixos/configuration.nix`:

```nix
{ pkgs, lib, ... }: {
  imports = [ ./hardware-configuration.nix ];

  # Bootloader (standard for EC2)
  boot.loader.grub.device = "/dev/xvda";

  networking.hostName = "choiros-sandbox";
  networking.firewall.allowedTCPPorts = [ 22 8080 ];

  # Rootless Podman — no root daemon
  virtualisation.podman = {
    enable = true;
    dockerCompat = true;
    defaultNetwork.settings.dns_enabled = true;
  };

  # Required for rootless user namespaces
  security.unprivilegedUsernsClone = true;

  # Service user for running the sandbox container
  users.users.chorus = {
    isNormalUser = true;
    uid = 1001;
    # Rootless Podman needs sub-uid/gid ranges
    subUidRanges = [{ startUid = 100000; count = 65536; }];
    subGidRanges = [{ startGid = 100000; count = 65536; }];
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAA... your-public-key"
    ];
  };

  # Workspace and data directories
  systemd.tmpfiles.rules = [
    "d /opt/choiros/workspace 0755 chorus chorus -"
    "d /opt/choiros/data      0750 chorus chorus -"
  ];

  # SSH
  services.openssh.enable = true;

  system.stateVersion = "25.11";
}
```

Apply:
```bash
# On the EC2 instance
nixos-rebuild switch   # after editing /etc/nixos/configuration.nix
```

Or from your Mac with nixos-anywhere (wipes and reinstalls — dev instances only):
```bash
nix run github:nix-community/nixos-anywhere -- \
  --flake .#choiros-ec2 root@<ec2-ip>
```

### 3.3 Container image for the sandbox (crane + dockerTools)

We use **crane** (not a Dockerfile) to build the image. This gives us:
- Fully reproducible, content-addressed output — same inputs always produce same image
- No Docker daemon required to build
- `buildLayeredImage` deduplicates Nix store paths into separate layers, so shared
  deps (openssl, libc) are cached across pushes and don't bloat each image
- Image contains only the binary's closure — no base OS, no apt, typically 30-80 MB
  vs 200+ MB for a Debian-based Dockerfile

#### How crane's two-derivation model works

crane splits the build into two Nix derivations:
1. `buildDepsOnly` — strips source to stubs, compiles only `Cargo.lock` dependencies,
   stores result as `cargoArtifacts` in the Nix store
2. `buildPackage` — inherits `cargoArtifacts`, compiles only your actual source

This means changing `src/main.rs` only rebuilds derivation 2. Changing `Cargo.lock`
rebuilds derivation 1 (deps) and derivation 2 inherits the new artifacts. With Cachix,
the `cargoArtifacts` derivation can be shared across machines and CI — no team member
rebuilds deps another has already built.

#### Cross-compilation: aarch64-darwin → x86_64-linux

Building the EC2 image on your Mac requires cross-compilation. Crane supports this
via nixpkgs's `pkgsCross` with `callPackage` splicing (which automatically routes
`nativeBuildInputs` to the build host and `buildInputs` to the target).

**Alternative:** configure a Linux Nix remote builder (see note at end of this section).
If cross-compilation proves painful (particularly for libsql's C build script), the
remote builder is a clean escape hatch.

#### SQLX offline mode (required for Nix builds)

Nix builds have no network access and no persistent state. SQLx normally connects to
a live DB to verify queries at compile time. You must use offline mode:

```bash
# In your development environment (with a running DB):
cd sandbox
cargo sqlx prepare --workspace
# Commits .sqlx/ directory containing query metadata
git add .sqlx
git commit -m "update sqlx offline data"
```

Then in the flake, set `SQLX_OFFLINE = "true"` and include `.sqlx/` in the source
fileset. The `.sqlx/` directory must be kept in sync with your SQL queries.

#### `nix/crane-image.nix`

Create this file in the repo:

```nix
# nix/crane-image.nix
# Builds the sandbox binary and OCI image using crane + dockerTools.
# Intended for cross-compilation from aarch64-darwin to x86_64-linux.
#
# Usage:
#   nix build .#sandbox-image
#   docker load < result
#   skopeo copy docker-archive:result docker://your-registry/sandbox:latest

{
  nixpkgs,
  crane,
  rust-overlay,
  system,           # local (build) system, e.g. "aarch64-darwin"
  crossSystem,      # target system, e.g. "x86_64-linux"
}:

let
  pkgs = import nixpkgs {
    localSystem = system;
    inherit crossSystem;
    overlays = [ (import rust-overlay) ];
  };

  craneLib = (crane.mkLib pkgs).overrideToolchain (p:
    p.rust-bin.stable."1.88.0".default.override {
      targets = [ "x86_64-unknown-linux-gnu" ];
    }
  );

  inherit (pkgs) lib;

  # Source: include Rust files + non-Rust assets needed at build/runtime
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      (craneLib.fileset.commonCargoSources ../.)
      ../sandbox/migrations    # SQL migration files
      ../sandbox/static        # static assets served by axum
      ../.sqlx                 # SQLx offline query metadata (must be committed)
    ];
  };

  # callPackage performs "splicing": nativeBuildInputs go to build host (Mac),
  # buildInputs go to target (Linux). Required for correct cross-compilation.
  crateExpression = {
    openssl,
    sqlite,
    pkg-config,
    lib,
    stdenv,
  }:
    let
      commonArgs = {
        inherit src;
        strictDeps = true;
        doCheck = false;    # cannot run x86_64 tests on aarch64 host

        nativeBuildInputs = [
          pkg-config          # runs on build host, finds target headers
        ];

        buildInputs = [
          openssl             # linked into sandbox binary
          sqlite              # libsql uses sqlite3 headers
        ];

        # Build only the sandbox crate from the workspace
        cargoExtraArgs = "--package sandbox --locked";

        # SQLx: no live DB at build time
        SQLX_OFFLINE = "true";

        # Prevent openssl-sys from trying to build openssl from source
        OPENSSL_NO_VENDOR = "1";
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in
      craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });

  sandbox-bin = pkgs.callPackage crateExpression {};

in {
  # The binary derivation (for nix build .#sandbox-bin)
  inherit sandbox-bin;

  # OCI image (for nix build .#sandbox-image)
  sandbox-image = pkgs.dockerTools.buildLayeredImage {
    name = "sandbox";
    tag = "latest";

    contents = [
      sandbox-bin
      pkgs.cacert    # TLS root certificates for outbound HTTPS (LLM API calls)
    ];

    # Copy non-Nix-store assets into image filesystem
    extraCommands = ''
      mkdir -p app/migrations app/static
      cp -r ${../sandbox/migrations}/. app/migrations/
      cp -r ${../sandbox/static}/. app/static/
    '';

    config = {
      Cmd        = [ "${sandbox-bin}/bin/sandbox" ];
      WorkingDir = "/app";
      ExposedPorts = { "8080/tcp" = {}; };
      Env = [
        "RUST_LOG=info"
        # DATABASE_URL, API keys, etc. injected at runtime via --env-file
      ];
    };
  };
}
```

Wire it into the repo-level `flake.nix` outputs:

```nix
let
  craneOutputs = import ./nix/crane-image.nix {
    inherit nixpkgs crane rust-overlay;
    system = "aarch64-darwin";   # your Mac
    crossSystem = "x86_64-linux"; # EC2 target
  };
in {
  packages.sandbox-bin   = craneOutputs.sandbox-bin;
  packages.sandbox-image = craneOutputs.sandbox-image;
}
```

#### Build and deploy

```bash
# Build the OCI image (cross-compiles on your Mac)
nix build .#sandbox-image

# Load into local Docker/Podman for testing
docker load < result

# Push to a registry
skopeo copy \
  docker-archive:$(nix build .#sandbox-image --print-out-paths --no-link) \
  docker://your-registry/sandbox:latest

# On EC2: pull and run
podman pull your-registry/sandbox:latest
podman run --rm -it \
  --name choiros-sandbox \
  --security-opt no-new-privileges \
  --cap-drop ALL \
  --cap-add NET_BIND_SERVICE \
  -p 8080:8080 \
  -v /opt/choiros/data:/app/data:z \
  --env-file /opt/choiros/.env \
  your-registry/sandbox:latest
```

Note: the workspace bind-mount (`-v workspace:/workspace`) from the earlier draft is
removed. The crane image bundles migrations and static assets directly. The data
directory (SQLite DB) is still a bind mount since it must persist across container
restarts.

#### Remote builder as escape hatch

Cross-compiling from macOS to Linux can break on crates with complex C build scripts
(libsql bundles a C sqlite fork; portable-pty uses OS pty headers). If you hit linker
errors or C compilation failures, configure a Linux Nix remote builder instead:

```bash
# Add to ~/.config/nix/nix.conf (or managed by nix-darwin):
# builders = ssh://chorus@<ec2-ip> x86_64-linux

# Then build natively on Linux:
nix build .#sandbox-image --system x86_64-linux
# Nix SSHes to the builder, builds there, copies result back
```

The remote builder produces identical output to local cross-compilation — same
content-addressed derivations — but avoids the cross-compilation complexity entirely.
Add the EC2 instance as a Nix builder in your nix-darwin config:

```nix
# In configuration.nix:
nix.buildMachines = [{
  hostName = "<ec2-ip>";
  system = "x86_64-linux";
  sshUser = "chorus";
  sshKey = "/Users/wiz/.ssh/your-key";
  maxJobs = 4;
  speedFactor = 2;
  supportedFeatures = [ "nixos-test" "benchmark" "big-parallel" "kvm" ];
}];
nix.distributedBuilds = true;
```

#### Known gotchas

- **libsql C deps**: libsql bundles a sqlite C fork. In cross builds, `stdenv.cc`
  becomes the cross C compiler automatically via `callPackage`. If you get C
  compilation errors, add `pkgs.pkgsBuildHost.stdenv.cc` to `nativeBuildInputs`
  explicitly.
- **`.sqlx/` not committed**: `cargo build` will fail with "offline mode requires
  .sqlx/ data". Run `cargo sqlx prepare` and commit the directory.
- **`portable-pty` on cross**: build succeeds (it's C FFI headers), but verify no
  linker errors. If you see missing symbols, add `pkgs.libc.dev` to `buildInputs`.
- **`doCheck = false`**: required for cross builds. Tests are x86_64 binaries and
  cannot run on aarch64. Run tests separately in the default (native) devShell.

### 3.4 Run the sandbox container

```bash
# On EC2 as the chorus user (rootless)
podman run --rm -it \
  --name choiros-sandbox \
  --security-opt no-new-privileges \
  --cap-drop ALL \
  --cap-add NET_BIND_SERVICE \
  -p 8080:8080 \
  -v /opt/choiros/workspace:/workspace:z,ro \
  -v /opt/choiros/data:/data:z \
  --env-file /opt/choiros/.env \
  choiros-sandbox:latest
```

The `:z` flag on bind mounts relabels SELinux context (needed for rootless Podman).
`--cap-drop ALL` + `--cap-add NET_BIND_SERVICE` is the minimal capability set.

**Secrets:** Put your `.env` at `/opt/choiros/.env` on the EC2 host, chmod 600,
owned by the chorus user. It is never baked into the image or a Nix config.

### 3.5 Declare as a systemd service (optional)

In the NixOS EC2 config, add:

```nix
virtualisation.oci-containers = {
  backend = "podman";
  containers.choiros-sandbox = {
    image = "choiros-sandbox:latest";
    autoStart = true;
    ports = [ "8080:8080" ];
    volumes = [
      "/opt/choiros/workspace:/workspace:z,ro"
      "/opt/choiros/data:/data:z"
    ];
    environmentFiles = [ "/opt/choiros/.env" ];
    extraOptions = [
      "--security-opt=no-new-privileges"
      "--cap-drop=ALL"
      "--cap-add=NET_BIND_SERVICE"
    ];
  };
};
```

`nixos-rebuild switch` will create a `podman-choiros-sandbox.service` unit.

---

## Phase 4: dioxus-desktop Nix flake

The dioxus-desktop frontend is excluded from the workspace Cargo.toml. It needs
its own flake (or can share the repo-level one with a separate devShell).

Add to the repo `flake.nix` a separate devShell:

```nix
devShells.frontend = pkgs.mkShell {
  buildInputs = with pkgs; [
    rustToolchain     # same as default shell, includes wasm32-unknown-unknown target
    dioxus-cli        # nixpkgs-unstable: 'dx' 0.7.3, confirmed in nixpkgs
                      # wraps wasm-bindgen-cli@0.2.108 automatically — no separate install
    nodejs            # for any JS tooling dx invokes
    pkg-config
    libiconv          # macOS
  ];
  shellHook = ''
    echo "Dioxus frontend dev shell"
    echo "dx: $(dx --version)"
    echo "Run 'dx serve --port 3000' to start"
  '';
};
```

Notes on dioxus-cli in nixpkgs:
- `pkgs.dioxus-cli` is the attribute name (nixpkgs-unstable, version 0.7.3)
- It wraps `wasm-bindgen-cli` at `0.2.108` via `wrapProgram` — do NOT also add
  `wasm-bindgen-cli` separately or you may get version conflicts
- The `no-downloads` feature is set, meaning dx will not attempt to fetch
  wasm-bindgen at runtime (it's bundled via the wrapper)

Usage:
```bash
cd ~/choiros-rs/dioxus-desktop
nix develop ..#frontend    # uses the parent flake's frontend shell
dx serve --port 3000
```

---

## Phase 5: hypervisor Nix flake

Hypervisor is a thin tokio binary today (just a router shell). It shares the
same dev shell as the sandbox (Phase 1 flake). When it grows into the edge
router / VM orchestrator it's intended to be, it will get its own derivation.

For now:
```bash
nix develop              # repo-level dev shell
cargo build -p hypervisor
just dev-hypervisor
```

---

## Migration: Homebrew → Nix

Packages currently installed via Homebrew that move to Nix:
- `git`, `ripgrep`, `fd`, `bat`, `jq`, `just` → `configuration.nix systemPackages`
- `rustup` → `configuration.nix` for now; per-project `rust-overlay` eventually
- `dx` (dioxus-cli) → `flake.nix` frontend devShell
- `sqlx-cli` → `flake.nix` default devShell

After confirming all tools work via Nix:
```bash
brew list | while read pkg; do
  echo "Check: $pkg"
done
# Audit manually, then:
brew remove <package>   # one at a time
# Eventually:
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/uninstall.sh)"
```

Do not rush this. Keep Homebrew until nix-darwin is stable on your machine.

---

## Open Questions / Future Work

- **baml-cli Nix derivation**: write `nix/baml-cli.nix` when next bumping baml
  versions or adding CI codegen. Template is in Phase 1.4 above.
- **libsql cross-compilation**: libsql bundles a C sqlite fork. The `callPackage`
  cross build should handle it, but this is unverified. If it fails, fall back to
  the remote builder approach (Phase 3.3).
- **`.sqlx/` offline data**: must be committed before the crane build works. Run
  `cargo sqlx prepare --workspace` from the sandbox directory with a running DB,
  commit `.sqlx/`, and keep it in sync when queries change.
- **EC2 NixOS config as flake output**: the EC2 `configuration.nix` should move into
  the repo as a `nixosConfigurations.choiros-ec2` output in `flake.nix` (or a
  dedicated `nix/ec2/flake.nix`), so `nixos-rebuild` and `nixos-anywhere` can target
  it directly.
- **Secret management upgrade**: current approach is a plaintext `.env` file on the
  EC2 host. Upgrade path: `sops-nix` or `agenix` for declarative encrypted secrets
  that are decrypted at activation time, never stored plaintext.
- **Phase 3 → metal upgrade**: swap `t3.large` for `c6i.metal` + add
  `microvm.nixosModules.host` to the NixOS EC2 config. The crane image and Podman
  setup are replaced by microvm.nix virtiofs shares. No changes to the sandbox
  binary itself.
