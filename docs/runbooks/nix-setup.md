# Nix Setup Runbook

Narrative Summary (1-minute read)
----------------------------------
We are setting up a Nix-first development and deployment stack for ChoirOS. This
runbook covers three phases:

1. **Mac** — nix-darwin + home-manager, replacing Homebrew for tooling
2. **Mac: local VMs** — microvm.nix (vfkit backend) running NixOS, for running
   sandbox instances in isolation using Apple Virtualization.framework
3. **EC2** — NixOS host running the sandbox in native NixOS containers

The sandbox (Rust/axum/ractor/sqlx/baml) is the primary workload. The
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
├── sandbox/          axum + ractor + sqlx + baml + portable-pty
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
- SQLite tooling/libs should be available for sqlite-linked crates and local tools

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
            sqlite          # sqlite headers/libs for local tooling
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

## Phase 3: NixOS on EC2 + native NixOS containers

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

### 3.2 NixOS host configuration for EC2

Push your NixOS config to the instance via nixos-rebuild or nixos-anywhere.
The EC2 instance's `/etc/nixos/configuration.nix`:

```nix
{ pkgs, lib, ... }: {
  imports = [ ./hardware-configuration.nix ];

  # Bootloader (standard for EC2)
  boot.loader.grub.device = "/dev/xvda";

  networking.hostName = "choiros-sandbox";
  networking.firewall.allowedTCPPorts = [ 22 8080 8081 9090 ];

  # Native NixOS container support (systemd-nspawn)
  boot.enableContainers = true;
  virtualisation.containers.enable = true;

  # Host directories for sandbox state and code
  systemd.tmpfiles.rules = [
    "d /opt/choiros/workspace 0755 root root -"
    "d /opt/choiros/data      0750 root root -"
    "d /opt/choiros/data/live 0750 root root -"
    "d /opt/choiros/data/dev  0750 root root -"
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

This section is deprecated for the current AWS path.

For this phase, prefer native NixOS containers (`containers.<name>`) instead of OCI
images managed by Podman. Keep OCI image work only as a fallback bridge.

### 3.4 Define live/dev sandboxes as NixOS containers

In the same EC2 `configuration.nix`, add:

```nix
containers.sandbox-live = {
  autoStart = true;
  privateNetwork = true;
  hostAddress = "10.233.1.1";
  localAddress = "10.233.1.2";
  forwardPorts = [{ protocol = "tcp"; hostPort = 8080; containerPort = 8080; }];
  privateUsers = "pick";
  bindMounts = {
    "/workspace" = { hostPath = "/opt/choiros/workspace"; isReadOnly = true; };
    "/data" = { hostPath = "/opt/choiros/data/live"; isReadOnly = false; };
  };
  config = { pkgs, ... }: {
    services.openssh.enable = false;
    networking.firewall.allowedTCPPorts = [ 8080 ];
    systemd.services.sandbox = {
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStart = "/opt/choiros/bin/sandbox";
        Restart = "always";
        RestartSec = 2;
      };
      environment = {
        PORT = "8080";
        DATABASE_URL = "sqlite:/data/events.db";
        SQLX_OFFLINE = "true";
      };
    };
    system.stateVersion = "25.11";
  };
};

containers.sandbox-dev = {
  autoStart = true;
  privateNetwork = true;
  hostAddress = "10.233.2.1";
  localAddress = "10.233.2.2";
  forwardPorts = [{ protocol = "tcp"; hostPort = 8081; containerPort = 8080; }];
  privateUsers = "pick";
  bindMounts = {
    "/workspace" = { hostPath = "/opt/choiros/workspace"; isReadOnly = true; };
    "/data" = { hostPath = "/opt/choiros/data/dev"; isReadOnly = false; };
  };
  config = { pkgs, ... }: {
    services.openssh.enable = false;
    networking.firewall.allowedTCPPorts = [ 8080 ];
    systemd.services.sandbox = {
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStart = "/opt/choiros/bin/sandbox";
        Restart = "always";
        RestartSec = 2;
      };
      environment = {
        PORT = "8080";
        DATABASE_URL = "sqlite:/data/events.db";
        SQLX_OFFLINE = "true";
      };
    };
    system.stateVersion = "25.11";
  };
};
```

Apply and verify:

```bash
nixos-rebuild switch
nixos-container list
systemctl status container@sandbox-live
systemctl status container@sandbox-dev
curl http://127.0.0.1:8080/health
curl http://127.0.0.1:8081/health
```

### 3.5 Operations notes

- Keep `privateUsers = "pick"` enabled for safer UID/GID isolation.
- Do not expose `8080/8081/9090` to `0.0.0.0/0`; restrict to operator CIDR or private
  network.
- Keep user/API secrets brokered by hypervisor; never mount platform secrets into
  containers.
- Rollback uses host generations: `nixos-rebuild switch --rollback`.

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
- **Sandbox artifact delivery into containers**: choose one path for `/opt/choiros/bin/sandbox`
  (remote `nix build`, rsync, or host-native flake package build) and make it the
  only supported path.
- **`.sqlx/` offline data**: keep committed and in sync; required for reliable host-side
  flake builds of the sandbox binary.
- **EC2 NixOS config as flake output**: the EC2 `configuration.nix` should move into
  the repo as a `nixosConfigurations.choiros-ec2` output in `flake.nix` (or a
  dedicated `nix/ec2/flake.nix`), so `nixos-rebuild` and `nixos-anywhere` can target
  it directly.
- **Secret management upgrade**: current approach is a plaintext `.env` file on the
  EC2 host. Upgrade path: `sops-nix` or `agenix` for declarative encrypted secrets
  that are decrypted at activation time, never stored plaintext.
- **Phase 3 → stronger isolation**: migrate from NixOS containers to microVMs
  (`microvm.nix`/Firecracker-class boundary) when threat model or tenant density
  requires stronger kernel isolation.
