# Rust Toolchain Management with Nix

**Date:** 2026-02-01
**Purpose:** Comprehensive guide for managing Rust toolchains using Nix, comparing approaches and providing practical examples

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Comparing Rust Toolchain Approaches](#comparing-rust-toolchain-approaches)
3. [Setting Up Multiple Rust Versions](#setting-up-multiple-rust-versions)
4. [Cargo Workspace Configuration](#cargo-workspace-configuration)
5. [Build Dependencies and Native Libraries](#build-dependencies-and-native-libraries)
6. [GitHub Actions CI with Nix](#github-actions-ci-with-nix)
7. [Developer Experience Tips](#developer-experience-tips)
8. [Complete Working Examples](#complete-working-examples)

---

## Executive Summary

[LEARNING] ARCHITECTURE: Nix provides three primary approaches for Rust toolchain management: fenix (community overlay), rust-overlay (Mozilla's nixpkgs-mozilla), and rustup (imperative tool). Each has distinct trade-offs in reproducibility, flexibility, and ease of use.

**Key Findings:**
- **fenix**: Best for most projects - community-maintained, supports rust-toolchain.toml, excellent flake integration
- **rust-overlay**: Good for pinning specific nightly versions, requires more manual configuration
- **rustup**: Not recommended for Nix - breaks reproducibility, imperative state management
- **Nixpkgs built-in**: Simplest but limited to stable releases, no nightly support

**Recommended Stack:**
- **Development**: fenix with rust-toolchain.toml
- **CI**: Same flake.nix for reproducible builds
- **Cross-compilation**: fenix with crossSystem
- **Legacy projects**: rust-overlay for specific nightly pinning

---

## Comparing Rust Toolchain Approaches

### 2.1 Overview of Options

| Approach | Source | Best For | Pros | Cons |
|----------|--------|----------|------|------|
| **fenix** | nix-community | Most projects | rust-toolchain.toml support, flakes-native, active development | Additional input dependency |
| **rust-overlay** | Mozilla/nixpkgs-mozilla | Specific nightly pinning | Direct from Mozilla, granular control | More complex setup |
| **rustup** | rust-lang.org | Quick experiments | Familiar workflow | Breaks Nix reproducibility |
| **nixpkgs** | NixOS/nixpkgs | Simple stable-only | No extra inputs, cached | Limited versions, no nightly |

### 2.2 fenix (Recommended)

[LEARNING] DOCS: fenix is the modern, community-maintained solution that integrates seamlessly with rust-toolchain.toml files.

**Features:**
- Reads `rust-toolchain.toml` directly
- Provides complete toolchain (rustc, cargo, clippy, rustfmt, rust-analyzer)
- Supports all channels (stable, beta, nightly)
- Excellent flake integration
- Binary cache available (cache.nixos.org)

**Basic flake.nix:**
```nix
{
  description = "Rust project with fenix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      toolchain = fenix.packages.${system}.fromToolchainFile {
        file = ./rust-toolchain.toml;
        sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          toolchain
          pkgs.openssl
          pkgs.pkg-config
        ];
      };
    };
}
```

**rust-toolchain.toml:**
```toml
[toolchain]
channel = "1.76.0"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
targets = ["x86_64-unknown-linux-gnu"]
profile = "default"
```

### 2.3 rust-overlay

[LEARNING] DOCS: rust-overlay from Mozilla provides direct access to Mozilla's Rust builds, useful for pinning specific nightly dates.

**Features:**
- Direct from Mozilla's infrastructure
- Pin to specific nightly dates
- Access to less common components
- More manual configuration required

**Basic flake.nix:**
```nix
{
  description = "Rust project with rust-overlay";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };
      
      rustChannel = pkgs.rustChannelOf {
        channel = "nightly";
        date = "2024-01-15";
        sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          rustChannel.rust
          pkgs.openssl
          pkgs.pkg-config
        ];
      };
    };
}
```

### 2.4 rustup (Not Recommended)

[LEARNING] SECURITY: rustup downloads binaries outside Nix's control, breaking reproducibility and potentially introducing supply chain risks.

**Why avoid rustup in Nix:**
- Downloads binaries imperatively
- No cryptographic verification through Nix
- State stored outside Nix store
- Breaks pure evaluation
- CI builds may differ from local builds

**If you must use rustup:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      pkgs.rustup
      pkgs.openssl
      pkgs.pkg-config
    ];
    
    shellHook = ''
      export RUSTUP_HOME="$PWD/.rustup"
      export CARGO_HOME="$PWD/.cargo"
      export PATH="$CARGO_HOME/bin:$PATH"
      
      if [ ! -d "$RUSTUP_HOME" ]; then
        rustup toolchain install stable
      fi
    '';
  };
}
```

### 2.5 Nixpkgs Built-in

**Simplest option for stable Rust:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.clippy
      pkgs.rustfmt
      pkgs.rust-analyzer
    ];
  };
}
```

**Limitations:**
- Only stable releases
- Version tied to nixpkgs revision
- No nightly support
- Limited target support

---

## Setting Up Multiple Rust Versions

### 3.1 Per-Project Toolchain with fenix

[LEARNING] ARCHITECTURE: Use rust-toolchain.toml for project-specific versions, allowing different projects to use different Rust versions without conflicts.

**Project A (Stable):**
```toml
# rust-toolchain.toml
[toolchain]
channel = "1.76.0"
components = ["rustfmt", "clippy", "rust-analyzer"]
```

**Project B (Nightly):**
```toml
# rust-toolchain.toml
[toolchain]
channel = "nightly-2024-01-15"
components = ["rustfmt", "clippy", "rust-src", "miri"]
targets = ["wasm32-unknown-unknown"]
```

**flake.nix supporting both:**
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      
      # Read toolchain from rust-toolchain.toml
      rustToolchain = fenix.packages.${system}.fromToolchainFile {
        file = ./rust-toolchain.toml;
        sha256 = builtins.readFile ./rust-toolchain.sha256;
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          rustToolchain
          pkgs.openssl
          pkgs.pkg-config
        ];
        
        # Ensure cargo uses the right toolchain
        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
      };
    };
}
```

### 3.2 Multiple Toolchains in One Project

**For projects needing multiple Rust versions:**
```nix
{
  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      
      stable = fenix.packages.${system}.stable;
      nightly = fenix.packages.${system}.latest;
    in {
      devShells.${system} = {
        # Default: stable toolchain
        default = pkgs.mkShell {
          buildInputs = [
            stable.toolchain
          ];
        };
        
        # Nightly for experimental features
        nightly = pkgs.mkShell {
          buildInputs = [
            nightly.toolchain
          ];
        };
        
        # Both available
        combined = pkgs.mkShell {
          buildInputs = [
            stable.toolchain
            nightly.toolchain
          ];
          
          shellHook = ''
            # Create aliases for different versions
            alias cargo-stable='cargo +stable'
            alias cargo-nightly='cargo +nightly'
          '';
        };
      };
    };
}
```

### 3.3 Cross-Compilation Targets

[LEARNING] DOCS: Nix makes cross-compilation straightforward by providing target-specific toolchains and sysroots.

**Adding cross-compilation targets:**
```nix
{
  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      
      # Base toolchain
      baseToolchain = fenix.packages.${system}.stable.toolchain;
      
      # Additional targets
      targets = [
        "aarch64-unknown-linux-gnu"
        "x86_64-pc-windows-gnu"
        "wasm32-unknown-unknown"
      ];
      
      # Toolchain with all targets
      fullToolchain = baseToolchain.override {
        targets = map (t: fenix.packages.${system}.targets.${t}.stable.rust-std) targets;
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          fullToolchain
          # Cross-compilation tools
          pkgs.gcc-aarch64-linux-gnu
          pkgs.mingw-w64
          pkgs.wasm-pack
        ];
        
        # Configure cargo for cross-compilation
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.gcc-aarch64-linux-gnu}/bin/aarch64-linux-gnu-gcc";
      };
    };
}
```

---

## Cargo Workspace Configuration

### 4.1 Workspace Structure with Nix

[LEARNING] ARCHITECTURE: Nix works seamlessly with Cargo workspaces. Use fenix to ensure all workspace members use the same toolchain.

**Example workspace:**
```
my-workspace/
├── Cargo.toml          # Workspace definition
├── rust-toolchain.toml # Shared toolchain
├── flake.nix           # Nix configuration
├── shared-types/
│   ├── Cargo.toml
│   └── src/
├── backend/
│   ├── Cargo.toml
│   └── src/
└── frontend/
    ├── Cargo.toml
    └── src/
```

**Cargo.toml:**
```toml
[workspace]
members = ["shared-types", "backend", "frontend"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.76"

[workspace.dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

**flake.nix for workspace:**
```nix
{
  description = "Rust workspace with Nix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
        # Get toolchain from rust-toolchain.toml
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        };
        
        # Build dependencies
        buildDeps = with pkgs; [
          openssl
          pkg-config
        ];
        
        # Development tools
        devTools = with pkgs; [
          rustToolchain
          cargo-edit
          cargo-watch
          cargo-audit
          cargo-deny
        ];
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = buildDeps ++ devTools;
          
          shellHook = ''
            echo "Rust workspace ready!"
            echo "Available commands:"
            echo "  cargo build --workspace"
            echo "  cargo test --workspace"
            echo "  cargo clippy --workspace"
            echo "  cargo watch -x 'check --workspace'"
          '';
        };
        
        # Build all workspace members
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my-workspace";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          
          nativeBuildInputs = buildDeps;
          
          # Build all workspace members
          buildPhase = ''
            cargo build --workspace --release
          '';
          
          installPhase = ''
            mkdir -p $out/bin
            cp target/release/backend $out/bin/
            cp target/release/frontend $out/bin/
          '';
        };
      });
}
```

### 4.2 Workspace with SQLx

**For projects using SQLx (like ChoirOS):**
```nix
{
  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.openssl
            pkgs.pkg-config
            pkgs.sqlite
            pkgs.sqlx-cli
          ];
          
          # SQLx environment
          SQLX_OFFLINE = "true";
          DATABASE_URL = "sqlite:./dev.db";
          
          shellHook = ''
            # Ensure migrations are run
            if [ ! -f dev.db ]; then
              sqlx database create
              sqlx migrate run
            fi
          '';
        };
      });
}
```

### 4.3 Caching Workspace Dependencies

[LEARNING] PERFORMANCE: Use crane or naersk for efficient incremental builds with proper caching of workspace dependencies.

**Using crane for workspace:**
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, crane, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      
      rustToolchain = fenix.packages.${system}.fromToolchainFile {
        file = ./rust-toolchain.toml;
        sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      };
      
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      
      # Common arguments for crane
      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
        
        nativeBuildInputs = with pkgs; [
          openssl
          pkg-config
        ];
      };
      
      # Build dependencies only (cached)
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      
      # Build workspace
      workspace = craneLib.buildPackage (commonArgs // {
        inherit cargoArtifacts;
        cargoExtraArgs = "--workspace";
      });
    in {
      packages.${system} = {
        default = workspace;
        deps = cargoArtifacts;
      };
      
      devShells.${system}.default = craneLib.devShell {
        checks = self.checks.${system};
        packages = [
          rustToolchain
          pkgs.cargo-edit
          pkgs.cargo-watch
        ];
      };
    };
}
```

---

## Build Dependencies and Native Libraries

### 5.1 Common Native Dependencies

[LEARNING] DOCS: Rust projects often need system libraries. Nix makes these explicit and reproducible.

**OpenSSL (most common):**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.openssl
      pkgs.pkg-config
    ];
    
    # Ensure openssl-sys can find OpenSSL
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
  };
}
```

**PostgreSQL (for sqlx/diesel):**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.postgresql
      pkgs.libpq
      pkgs.pkg-config
      pkgs.sqlx-cli
    ];
    
    # PostgreSQL environment
    PGDATA = "$PWD/.postgres";
    DATABASE_URL = "postgres://localhost/mydb";
    
    shellHook = ''
      # Initialize PostgreSQL if needed
      if [ ! -d "$PGDATA" ]; then
        initdb -D "$PGDATA"
        pg_ctl -D "$PGDATA" start
        createdb mydb
      fi
    '';
  };
}
```

**SQLite (for embedded databases):**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.sqlite
      pkgs.sqlx-cli
    ];
    
    SQLX_OFFLINE = "true";
  };
}
```

### 5.2 Graphics and GPU Dependencies

**For wgpu/bevy projects:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      # Graphics libraries
      pkgs.vulkan-loader
      pkgs.vulkan-validation-layers
      pkgs.libxkbcommon
      pkgs.wayland
      pkgs.libGL
      # X11 support
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXi
      pkgs.xorg.libXrandr
    ];
    
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.libGL
    ];
  };
}
```

### 5.3 macOS Frameworks

**For macOS-specific dependencies:**
```nix
{
  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = fenix.packages.${system}.stable.toolchain;
        
        # macOS-specific frameworks
        frameworks = pkgs.darwin.apple_sdk.frameworks;
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            frameworks.Security
            frameworks.CoreServices
            frameworks.CoreFoundation
            frameworks.Foundation
            frameworks.AppKit
            frameworks.WebKit
          ];
        };
      });
}
```

### 5.4 Custom Build Scripts

**For projects with custom build.rs:**
```nix
{
  packages.default = pkgs.rustPlatform.buildRustPackage {
    pname = "my-project";
    version = "0.1.0";
    src = ./.;
    cargoLock.lockFile = ./Cargo.lock;
    
    nativeBuildInputs = with pkgs; [
      # Build-time tools
      cmake
      python3
      protobuf
    ];
    
    buildInputs = with pkgs; [
      # Runtime libraries
      openssl
    ];
    
    # Environment for build.rs
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    
    # Custom build phases
    preBuild = ''
      echo "Running custom pre-build steps..."
      make -C vendor/some-lib
    '';
  };
}
```

---

## GitHub Actions CI with Nix

### 6.1 Basic CI Setup

[LEARNING] DOCS: Use the official DeterminateSystems actions for reliable Nix installation in CI.

**.github/workflows/ci.yml:**
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      # Install Nix with flakes support
      - uses: DeterminateSystems/nix-installer-action@main
      
      # Optional: Use binary cache
      - uses: DeterminateSystems/magic-nix-cache-action@main
      
      - name: Build
        run: nix build
      
      - name: Run tests
        run: nix develop --command cargo test --workspace
      
      - name: Run clippy
        run: nix develop --command cargo clippy --workspace -- -D warnings
      
      - name: Check formatting
        run: nix develop --command cargo fmt --check
```

### 6.2 Multi-Platform CI

**Build on Linux, macOS, and Windows (WSL):**
```yaml
name: CI

on: [push, pull_request]

jobs:
  build:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        include:
          - os: ubuntu-latest
            system: x86_64-linux
          - os: macos-latest
            system: aarch64-darwin
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      - uses: DeterminateSystems/magic-nix-cache-action@main
      
      - name: Build for ${{ matrix.system }}
        run: nix build .#packages.${{ matrix.system }}.default
      
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: binary-${{ matrix.system }}
          path: result/bin/
```

### 6.3 Caching with Cachix

[LEARNING] PERFORMANCE: Cachix provides fast binary caching for Nix builds, dramatically reducing CI times.

**Setup with Cachix:**
```yaml
name: CI

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      
      # Setup Cachix
      - uses: cachix/cachix-action@v14
        with:
          name: my-project-cache
          authToken: ${{ secrets.CACHIX_AUTH_TOKEN }}
      
      - name: Build
        run: nix build
```

**flake.nix with Cachix:**
```nix
{
  description = "My Project";

  nixConfig = {
    extra-substituters = ["https://my-project-cache.cachix.org"];
    extra-trusted-public-keys = [
      "my-project-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, fenix }: {
    # ...
  };
}
```

### 6.4 Release Automation

**Automated releases with Nix:**
```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            system: x86_64-linux
          - os: macos-latest
            target: aarch64-apple-darwin
            system: aarch64-darwin
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      
      - name: Build release
        run: nix build .#packages.${{ matrix.system }}.default
      
      - name: Package
        run: |
          mkdir -p dist
          cp -r result/bin/* dist/
          tar -czf myapp-${{ matrix.target }}.tar.gz -C dist .
      
      - name: Upload to release
        uses: softprops/action-gh-release@v1
        with:
          files: myapp-${{ matrix.target }}.tar.gz
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### 6.5 Testing Multiple Rust Versions

**Test matrix with different Rust versions:**
```yaml
name: Test Rust Versions

on: [push, pull_request]

jobs:
  test:
    strategy:
      matrix:
        rust:
          - stable
          - beta
          - nightly
    
    runs-on: ubuntu-latest
    
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      
      - name: Test with ${{ matrix.rust }}
        run: |
          nix develop .#${{ matrix.rust }} --command cargo test --workspace
```

**flake.nix with version matrix:**
```nix
{
  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fenixPkgs = fenix.packages.${system};
        
        mkDevShell = toolchain: pkgs.mkShell {
          buildInputs = [
            toolchain
            pkgs.openssl
            pkgs.pkg-config
          ];
        };
      in {
        devShells = {
          default = mkDevShell fenixPkgs.stable.toolchain;
          stable = mkDevShell fenixPkgs.stable.toolchain;
          beta = mkDevShell fenixPkgs.beta.toolchain;
          nightly = mkDevShell fenixPkgs.latest.toolchain;
        };
      });
}
```

---

## Developer Experience Tips

### 7.1 Shell Hooks and Automation

[LEARNING] DOCS: Use shellHook to automate common setup tasks and provide helpful reminders.

**Comprehensive dev shell:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.openssl
      pkgs.pkg-config
      pkgs.cargo-edit
      pkgs.cargo-watch
      pkgs.cargo-audit
      pkgs.cargo-deny
      pkgs.sqlx-cli
    ];
    
    shellHook = ''
      # Welcome message
      echo "🦀 Rust development environment loaded"
      echo ""
      
      # Show Rust version
      echo "Rust version: $(rustc --version)"
      echo "Cargo version: $(cargo --version)"
      echo ""
      
      # Check for rust-toolchain.toml
      if [ -f rust-toolchain.toml ]; then
        echo "✓ Using rust-toolchain.toml"
      else
        echo "⚠ No rust-toolchain.toml found"
      fi
      
      # Setup git hooks (optional)
      if [ -d .git ] && [ ! -f .git/hooks/pre-commit ]; then
        echo "Setting up git hooks..."
        cat > .git/hooks/pre-commit <<'HOOK'
#!/bin/sh
set -e

echo "Running pre-commit checks..."
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace --lib
HOOK
        chmod +x .git/hooks/pre-commit
        echo "✓ Pre-commit hook installed"
      fi
      
      # Database setup (if applicable)
      if command -v sqlx >/dev/null 2>&1; then
        if [ ! -f dev.db ] && [ -d migrations ]; then
          echo "Setting up development database..."
          sqlx database create
          sqlx migrate run
          echo "✓ Database initialized"
        fi
      fi
      
      echo ""
      echo "Available commands:"
      echo "  cargo build --workspace    # Build all workspace members"
      echo "  cargo test --workspace     # Run all tests"
      echo "  cargo watch -x check       # Auto-rebuild on changes"
      echo "  cargo audit                # Check for security vulnerabilities"
      echo "  cargo deny check           # Check license compliance"
    '';
  };
}
```

### 7.2 IDE Integration

**VS Code with rust-analyzer:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.rust-analyzer
      pkgs.vscode-extensions.rust-lang.rust-analyzer
    ];
    
    # rust-analyzer needs to find the Rust source
    RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
    
    shellHook = ''
      # Generate VS Code settings if not present
      if [ ! -f .vscode/settings.json ]; then
        mkdir -p .vscode
        cat > .vscode/settings.json <<'VSCODE'
{
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.workspace": true,
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.procMacro.enable": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
VSCODE
        echo "✓ VS Code settings created"
      fi
    '';
  };
}
```

### 7.3 Justfile Integration

**Using Just with Nix:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.just
      pkgs.cargo-watch
    ];
    
    shellHook = ''
      # Show available just commands
      if [ -f Justfile ]; then
        echo ""
        echo "Available just commands:"
        just --list --unsorted
      fi
    '';
  };
}
```

**Justfile:**
```just
# List available commands
help:
    @just --list

# Build the project
build:
    cargo build --workspace

# Build for release
release:
    cargo build --workspace --release

# Run tests
test:
    cargo test --workspace

# Run clippy
lint:
    cargo clippy --workspace -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Run the development server
dev:
    cargo watch -x 'run --bin backend'

# Run database migrations
migrate:
    sqlx migrate run

# Create a new migration
new-migration name:
    sqlx migrate add {{name}}

# Security audit
audit:
    cargo audit

# Check licenses
deny:
    cargo deny check

# Clean build artifacts
clean:
    cargo clean
    rm -rf target/
```

### 7.4 Direnv Integration

**Automatic shell activation with direnv:**
```nix
# .envrc
use flake
```

**flake.nix with direnv support:**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    buildInputs = [
      rustToolchain
      pkgs.direnv
      pkgs.nix-direnv
    ];
    
    shellHook = ''
      # Remind about direnv
      if [ -z "$DIRENV_DIR" ]; then
        echo "💡 Tip: Run 'direnv allow' to automatically activate this shell"
      fi
    '';
  };
}
```

### 7.5 Troubleshooting Common Issues

**Issue: SSL certificate errors**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  };
}
```

**Issue: Missing dynamic libraries at runtime**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.openssl
      pkgs.zlib
    ];
  };
}
```

**Issue: proc-macro2 build failures**
```nix
{
  devShells.${system}.default = pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.gcc
      pkgs.libclang
    ];
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
  };
}
```

**Issue: Outdated flake inputs**
```bash
# Update all inputs
nix flake update

# Update specific input
nix flake update nixpkgs

# Lock specific input version
nix flake lock --update-input fenix
```

---

## Complete Working Examples

### 8.1 Simple Binary Project

**flake.nix:**
```nix
{
  description = "Simple Rust CLI tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = fenix.packages.${system}.stable.toolchain;
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my-cli";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };
        
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo-edit
          ];
        };
      });
}
```

### 8.2 Web Application with SQLx

**flake.nix:**
```nix
{
  description = "Rust web app with SQLx";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        };
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "web-app";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          
          nativeBuildInputs = with pkgs; [
            pkg-config
            sqlx-cli
          ];
          
          buildInputs = with pkgs; [
            openssl
            sqlite
          ];
          
          SQLX_OFFLINE = "true";
        };
        
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.openssl
            pkgs.pkg-config
            pkgs.sqlite
            pkgs.sqlx-cli
            pkgs.cargo-watch
          ];
          
          SQLX_OFFLINE = "true";
          DATABASE_URL = "sqlite:./dev.db";
          
          shellHook = ''
            if [ ! -f dev.db ] && [ -d migrations ]; then
              sqlx database create
              sqlx migrate run
            fi
          '';
        };
      });
}
```

### 8.3 Cross-Compilation Project

**flake.nix:**
```nix
{
  description = "Cross-compilation example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fenixPkgs = fenix.packages.${system};
        
        # Toolchain with cross-compilation targets
        toolchain = fenixPkgs.combine [
          fenixPkgs.stable.toolchain
          fenixPkgs.targets.aarch64-unknown-linux-gnu.stable.rust-std
          fenixPkgs.targets.x86_64-pc-windows-gnu.stable.rust-std
        ];
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            toolchain
            pkgs.gcc-aarch64-linux-gnu
            pkgs.mingw-w64
          ];
          
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = 
            "${pkgs.gcc-aarch64-linux-gnu}/bin/aarch64-linux-gnu-gcc";
        };
        
        packages = {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "cross-example";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
          };
          
          aarch64 = (pkgs.pkgsCross.aarch64-multiplatform.rustPlatform.buildRustPackage {
            pname = "cross-example";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
          });
        };
      });
}
```

### 8.4 Workspace with Multiple Crates

**flake.nix:**
```nix
{
  description = "Rust workspace example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, crane, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        };
        
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [
            openssl
            pkg-config
          ];
        };
        
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        
        workspace = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--workspace";
        });
      in {
        packages = {
          default = workspace;
          backend = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            cargoExtraArgs = "--package backend";
          });
          frontend = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            cargoExtraArgs = "--package frontend";
          });
        };
        
        devShells.default = craneLib.devShell {
          packages = [
            rustToolchain
            pkgs.cargo-edit
            pkgs.cargo-watch
            pkgs.just
          ];
        };
      });
}
```

### 8.5 Rust + NixOS Container

**flake.nix:**
```nix
{
  description = "Rust app packaged as OCI container";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = fenix.packages.${system}.stable.toolchain;
        
        myApp = pkgs.rustPlatform.buildRustPackage {
          pname = "my-app";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };
        
        container = pkgs.dockerTools.buildImage {
          name = "my-app";
          tag = "latest";
          
          contents = [
            myApp
            pkgs.cacert
            pkgs.tzdata
          ];
          
          config = {
            Cmd = [ "${myApp}/bin/my-app" ];
            ExposedPorts = {
              "8080/tcp" = {};
            };
            Env = [
              "RUST_LOG=info"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
          };
        };
      in {
        packages = {
          default = myApp;
          container = container;
        };
        
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo-watch
          ];
        };
      });
}
```

---

## Quick Reference

### Essential Commands

```bash
# Enter development shell
nix develop

# Build the project
nix build

# Build specific package
nix build .#packages.x86_64-linux.backend

# Update flake inputs
nix flake update

# Check flake
nix flake check

# Run with cargo
nix develop --command cargo test

# Build container
nix build .#container && docker load < result
```

### Common Inputs

```nix
{
  inputs = {
    # Nixpkgs
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    
    # Rust toolchain (recommended)
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    
    # Alternative Rust (Mozilla)
    rust-overlay = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
    
    # Build optimization
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    
    # Utilities
    flake-utils.url = "github:numtide/flake-utils";
  };
}
```

### File Locations

- `flake.nix` - Main Nix configuration
- `flake.lock` - Locked dependency versions
- `rust-toolchain.toml` - Rust toolchain specification
- `.envrc` - direnv configuration (optional)
- `Justfile` - Task runner commands (optional)

---

## References

- [fenix Documentation](https://github.com/nix-community/fenix)
- [nixpkgs-mozilla](https://github.com/mozilla/nixpkgs-mozilla)
- [crane Documentation](https://github.com/ipetkov/crane)
- [Nixpkgs Rust Platform](https://nixos.org/manual/nixpkgs/stable/#rust)
- [Rust Toolchain File](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file)
- [Nix Flakes Wiki](https://nixos.wiki/wiki/Flakes)
- [DeterminateSystems Nix Installer](https://github.com/DeterminateSystems/nix-installer)
- [Cachix Documentation](https://docs.cachix.org/)

---

**Document Status:** Complete
**Last Updated:** 2026-02-01
