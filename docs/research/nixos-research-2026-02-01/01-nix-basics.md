# Nix Basics for Rust Development

**Date:** 2026-02-01  
**Purpose:** Practical guide to using Nix for Rust development

---

## Table of Contents
1. [What is Nix and Why Use It for Rust](#what-is-nix-and-why-use-it-for-rust)
2. [Basic flake.nix Structure for Rust](#basic-flakenix-structure-for-rust)
3. [nix-shell vs nix develop for Rust Dev](#nix-shell-vs-nix-develop-for-rust-dev)
4. [Using rust-overlay for Latest Toolchain](#using-rust-overlay-for-latest-toolchain)
5. [Cross-Compilation Setup](#cross-compilation-setup)
6. [IDE Integration (rust-analyzer)](#ide-integration-rust-analyzer)

---

## What is Nix and Why Use It for Rust

### What is Nix?

Nix is a **purely functional package manager** and build system that provides:

- **Reproducible builds**: Same inputs always produce same outputs
- **Declarative configuration**: Define your entire environment in code
- **Atomic upgrades/rollbacks**: Switch between environments instantly
- **Isolation**: Dependencies don't conflict with system packages
- **Flakes**: Modern Nix feature for composable, reproducible projects

### Why Use Nix for Rust?

| Problem | Nix Solution |
|---------|-------------|
| "Works on my machine" | Pin exact Rust toolchain + dependencies |
| CI/CD inconsistencies | Same environment everywhere |
| Onboarding friction | One command: `nix develop` |
| Cross-compilation complexity | Nix handles toolchains automatically |
| rust-analyzer mismatches | IDE uses same toolchain as project |
| C library dependencies | Automatic linking (OpenSSL, zlib, etc.) |

### Real-World Benefits

```bash
# Clone any Nix-enabled Rust project
git clone https://github.com/example/rust-project
cd rust-project

# Enter exact development environment
nix develop

# Build with pinned toolchain
cargo build --release
```

No "install Rust", no "which version?", no missing system libraries.

---

## Basic flake.nix Structure for Rust

### Minimal Flake for Rust

```nix
{
  description = "A basic Rust project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my-project";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
          ];
        };
      });
}
```

### Anatomy of a Rust Flake

```nix
{
  # 1. INPUTS: External dependencies
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # Pin to specific commit for reproducibility:
    # nixpkgs.url = "github:NixOS/nixpkgs/abc123...";
  };

  # 2. OUTPUTS: What this flake produces
  outputs = { self, nixpkgs }: {
    # Packages: Build artifacts
    packages.x86_64-linux.default = ...;
    
    # Development shells: Interactive environments
    devShells.x86_64-linux.default = ...;
    
    # Apps: Runnable programs
    apps.x86_64-linux.default = ...;
  };
}
```

### Complete Example with System Dependencies

```nix
{
  description = "Rust project with native dependencies";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
        # Native libraries your Rust code links against
        nativeBuildInputs = with pkgs; [
          pkg-config          # Helps cargo find system libs
          openssl.dev         # OpenSSL headers
          zlib.dev            # zlib headers
        ];
        
        buildInputs = with pkgs; [
          openssl             # OpenSSL runtime
          zlib                # zlib runtime
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my-app";
          version = "0.1.0";
          src = ./.;
          
          inherit nativeBuildInputs buildInputs;
          
          cargoLock.lockFile = ./Cargo.lock;
          
          # For crates that need network during build
          # (not recommended, use cargo vendor instead)
          # networkBuild = true;
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          
          packages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer       # LSP server
            cargo-watch         # Auto-rebuild on changes
            cargo-edit          # cargo add/rm/upgrade
          ];
          
          # Environment variables
          RUST_BACKTRACE = 1;
          RUST_LOG = "debug";
          
          # Shell hook runs when entering the shell
          shellHook = ''
            echo "Rust development environment loaded!"
            echo "rustc: $(rustc --version)"
            echo "cargo: $(cargo --version)"
          '';
        };
      });
}
```

### Common Native Dependencies

| Crate | Nix Package | Notes |
|-------|-------------|-------|
| `openssl` | `openssl` + `openssl.dev` | HTTPS, crypto |
| `zlib` | `zlib` + `zlib.dev` | Compression |
| `libsqlite3-sys` | `sqlite` | Database |
| `libpq` | `postgresql` | PostgreSQL |
| `freetype` | `freetype` | Font rendering |
| `fontconfig` | `fontconfig` | Font discovery |
| `alsa-sys` | `alsa-lib` | Linux audio |
| `vulkan` | `vulkan-loader` | Graphics |

---

## nix-shell vs nix develop for Rust Dev

### The Old Way: nix-shell

```bash
# Using shell.nix (legacy approach)
nix-shell

# With specific package
nix-shell -p rustc cargo

# Pure environment (no system packages)
nix-shell --pure
```

**shell.nix** (legacy):
```nix
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    cargo
  ];
}
```

### The Modern Way: nix develop

```bash
# Enter development shell from flake.nix
nix develop

# With specific flake output
nix develop .#my-shell

# Run command without entering shell
nix develop -c cargo build

# Impure mode (include system PATH)
nix develop --impure
```

### Comparison

| Feature | `nix-shell` | `nix develop` |
|---------|-------------|---------------|
| Flakes support | No | Yes |
| Reproducibility | Depends on `<nixpkgs>` | Fully pinned |
| Syntax | `shell.nix` | `flake.nix` devShells |
| Lock file | No | `flake.lock` |
| Composability | Limited | High (flake inputs) |
| Future support | Maintenance mode | Active development |

### Recommendation for Rust Projects

**Use `nix develop`** for:
- New projects
- Team environments
- CI/CD pipelines
- Reproducibility requirements

**Use `nix-shell`** for:
- Legacy projects
- Quick one-off environments
- Ad-hoc package installation

### Practical Examples

```bash
# Quick Rust playground
nix-shell -p rustc cargo --run "cargo new hello && cd hello && cargo run"

# Full project environment
nix develop

# CI/CD build
nix develop -c cargo test --all-features

# Release build with exact same environment
nix develop -c cargo build --release
```

---

## Using rust-overlay for Latest Toolchain

### The Problem

Nixpkgs often lags behind Rust releases:
- Nixpkgs might have Rust 1.74
- You need Rust 1.75 for new features
- Waiting for nixpkgs update blocks development

### The Solution: rust-overlay

[rust-overlay](https://github.com/oxalica/rust-overlay) provides:
- Latest stable, beta, nightly
- Specific date nightlies
- Custom toolchains (clippy, rustfmt, rust-src)

### Setup with rust-overlay

```nix
{
  description = "Rust with latest toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        # Pick your toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        # Or specific version:
        # rustToolchain = pkgs.rust-bin.stable."1.75.0".default;
        # Or nightly:
        # rustToolchain = pkgs.rust-bin.nightly.latest.default;
        # Or specific nightly with components:
        # rustToolchain = pkgs.rust-bin.nightly."2024-01-01".default;
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.rust-analyzer
          ];
        };
      });
}
```

### Advanced Toolchain Configuration

```nix
{
  description = "Rust with custom toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        # Custom toolchain with specific components
        rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [
            "rust-src"           # For rust-analyzer
            "rustfmt"            # Code formatting
            "clippy"             # Linting
            "llvm-tools"         # Code coverage
          ];
          targets = [
            "x86_64-unknown-linux-gnu"
            "wasm32-unknown-unknown"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            pkgs.cargo-edit
            pkgs.cargo-tarpaulin   # Code coverage
          ];
          
          # Ensure rust-analyzer finds rust-src
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      });
}
```

### Available Toolchain Variants

```nix
# Minimal: rustc + cargo + rust-std
pkgs.rust-bin.stable.latest.minimal

# Default: minimal + rust-docs + clippy + rustfmt
pkgs.rust-bin.stable.latest.default

# Complete: default + miri + llvm-tools + etc
pkgs.rust-bin.stable.latest.complete

# Custom: Pick what you need
pkgs.rust-bin.stable.latest.minimal.override {
  extensions = [ "rust-src" "clippy" "rustfmt" ];
  targets = [ "wasm32-unknown-unknown" ];
}
```

### Pinning Rust Versions

```nix
# Exact stable version
rustToolchain = pkgs.rust-bin.stable."1.75.0".default;

# Exact nightly date
rustToolchain = pkgs.rust-bin.nightly."2024-01-15".default;

# Latest of each channel
rustToolchain = pkgs.rust-bin.stable.latest.default;
rustToolchain = pkgs.rust-bin.beta.latest.default;
rustToolchain = pkgs.rust-bin.nightly.latest.default;
```

---

## Cross-Compilation Setup

### Why Cross-Compilation Matters

- Build Linux binaries from macOS
- Target ARM from x86_64
- Create static binaries for containers
- Build for embedded targets

### Basic Cross-Compilation

```nix
{
  description = "Rust cross-compilation";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        # Target platform
        crossSystem = "aarch64-unknown-linux-gnu";
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ crossSystem ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.qemu              # For testing ARM binaries
          ];
          
          # Cross-compilation environment
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = 
            "${pkgs.pkgsCross.aarch64-multiplatform.stdenv.cc}/bin/aarch64-unknown-linux-gnu-gcc";
          
          shellHook = ''
            echo "Cross-compilation target: ${crossSystem}"
            echo "Build with: cargo build --target ${crossSystem}"
          '';
        };
      });
}
```

### Multi-Target Cross-Compilation

```nix
{
  description = "Rust multi-target builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        # Define all your targets
        targets = [
          "x86_64-unknown-linux-gnu"
          "x86_64-unknown-linux-musl"
          "aarch64-unknown-linux-gnu"
          "aarch64-unknown-linux-musl"
          "wasm32-unknown-unknown"
        ];
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          inherit targets;
        };
        
        # Cross-compilation toolchains
        crossPkgs = {
          aarch64-linux = pkgs.pkgsCross.aarch64-multiplatform;
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.qemu
            pkgs.cargo-cross   # Alternative cross-compilation tool
          ];
          
          # Linker configurations for each target
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = 
            "${crossPkgs.aarch64-linux.stdenv.cc}/bin/aarch64-unknown-linux-gnu-gcc";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = 
            "${crossPkgs.aarch64-linux.pkgsStatic.stdenv.cc}/bin/aarch64-unknown-linux-musl-gcc";
          
          shellHook = ''
            echo "Available targets:"
            ${pkgs.lib.concatMapStringsSep "\n" (t: "  - ${t}") targets}
          '';
        };
        
        # Build packages for each target
        packages = {
          aarch64-linux = 
            let
              crossPkgs = pkgs.pkgsCross.aarch64-multiplatform;
            in
            crossPkgs.rustPlatform.buildRustPackage {
              pname = "my-app";
              version = "0.1.0";
              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;
            };
        };
      });
}
```

### Static Binary Compilation (MUSL)

```nix
{
  description = "Static Rust binaries with MUSL";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        # Static build environment
        staticPkgs = pkgs.pkgsStatic;
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" ];
        };
      in
      {
        packages.static = staticPkgs.rustPlatform.buildRustPackage {
          pname = "my-static-app";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          
          # Static linking flags
          RUSTFLAGS = "-C target-feature=+crt-static";
        };
        
        devShells.static = pkgs.mkShell {
          packages = [ rustToolchain ];
          RUSTFLAGS = "-C target-feature=+crt-static";
        };
      });
}
```

### Common Cross-Compilation Targets

| Target | Use Case | Nix Package |
|--------|----------|-------------|
| `x86_64-unknown-linux-gnu` | Standard Linux | Native |
| `x86_64-unknown-linux-musl` | Static Linux | `pkgsStatic` |
| `aarch64-unknown-linux-gnu` | ARM64 Linux | `pkgsCross.aarch64-multiplatform` |
| `aarch64-apple-darwin` | Apple Silicon | Native on macOS |
| `x86_64-apple-darwin` | Intel macOS | Native on macOS |
| `wasm32-unknown-unknown` | WebAssembly | `wasm-pack` |
| `x86_64-pc-windows-gnu` | Windows | `pkgsCross.mingwW64` |

---

## IDE Integration (rust-analyzer)

### The Challenge

rust-analyzer needs:
1. Same Rust version as the project
2. Access to `rust-src` for standard library analysis
3. Environment variables (PKG_CONFIG_PATH, etc.)
4. Native library paths for FFI crates

### VS Code Integration

#### 1. Nix Environment Selector Extension

Install: `arrterian.nix-env-selector`

```json
// .vscode/settings.json
{
  "nixEnvSelector.nixFile": "${workspaceFolder}/flake.nix",
  "nixEnvSelector.args": [".#devShells.default"]
}
```

#### 2. Manual Configuration

```json
// .vscode/settings.json
{
  "rust-analyzer.server.path": "rust-analyzer",
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.rustfmt.extraArgs": ["+nightly"]
}
```

#### 3. direnv Integration (Recommended)

Install extensions:
- `mkhl.direnv` - Auto-load Nix environment
- `rust-lang.rust-analyzer`

```bash
# Install direnv
nix-env -iA nixpkgs.direnv

# Add to shell
echo 'eval "$(direnv hook bash)"' >> ~/.bashrc

# In project root
echo "use flake" > .envrc
direnv allow
```

```json
// .vscode/settings.json (with direnv)
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

### JetBrains RustRover / IntelliJ

```bash
# Enter Nix shell first
nix develop

# Launch IDE from within the shell
rustrover .
```

Or use direnv plugin:
1. Install direnv plugin
2. Add `.envrc` with `use flake`
3. IDE automatically picks up environment

### Emacs

```elisp
;; With direnv
(use-package direnv
  :config (direnv-mode))

;; rust-analyzer will use Nix environment automatically
(use-package lsp-mode
  :hook (rust-mode . lsp))
```

### Vim/Neovim

```lua
-- With direnv
vim.opt.shell = "bash"
vim.g.direnv_silent_load = 1

-- rust-analyzer via lspconfig
require('lspconfig').rust_analyzer.setup({
  settings = {
    ['rust-analyzer'] = {
      cargo = { features = 'all' },
      checkOnSave = { command = 'clippy' }
    }
  }
})
```

### Complete IDE-Ready Flake

```nix
{
  description = "Rust project with full IDE support";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };
        
        nativeBuildInputs = with pkgs; [
          pkg-config
          openssl.dev
        ];
        
        buildInputs = with pkgs; [
          openssl
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            pkgs.cargo-edit
          ];
          
          # Critical for rust-analyzer
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          
          # For debugging
          RUST_BACKTRACE = 1;
          
          shellHook = ''
            echo "Rust environment ready!"
            echo "Toolchain: $(rustc --version)"
            echo "rust-analyzer: $(rust-analyzer --version)"
            
            # Verify rust-src is available
            if [ -d "$RUST_SRC_PATH" ]; then
              echo "✓ rust-src found"
            else
              echo "✗ rust-src NOT found - rust-analyzer may not work correctly"
            fi
          '';
        };
      });
}
```

### Troubleshooting rust-analyzer

| Issue | Solution |
|-------|----------|
| "Failed to find sysroot" | Set `RUST_SRC_PATH` in flake |
| "can't find crate for std" | Add `rust-src` extension |
| Missing native libs | Add to `buildInputs` |
| Version mismatch | Use rust-overlay pinned version |
| High CPU/memory | Exclude `target/` in IDE settings |

### VS Code Workspace Settings

```json
// .vscode/settings.json
{
  "nixEnvSelector.nixFile": "${workspaceFolder}/flake.nix",
  
  "rust-analyzer.server.path": "rust-analyzer",
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.extraArgs": ["--all-targets"],
  "rust-analyzer.cargo.buildScripts.enable": true,
  "rust-analyzer.procMacro.enable": true,
  
  "files.watcherExclude": {
    "**/target/**": true,
    "**/.git/**": true
  },
  
  "search.exclude": {
    "**/target": true,
    "**/Cargo.lock": true
  }
}
```

---

## Quick Reference

### Essential Commands

```bash
# Enter development environment
nix develop

# Build the project
nix build

# Run the project
nix run

# Format nix files
nix fmt

# Update flake inputs
nix flake update

# Check flake without building
nix flake check

# Garbage collect old builds
nix-collect-garbage -d
```

### Common Flake Templates

```bash
# Rust template
nix flake init -t templates#rust

# From rust-overlay
nix flake init -t github:oxalica/rust-overlay
```

### Directory Structure

```
my-rust-project/
├── flake.nix           # Main Nix configuration
├── flake.lock          # Pinned dependencies
├── Cargo.toml          # Rust manifest
├── Cargo.lock          # Pinned Rust deps
├── .vscode/
│   └── settings.json   # IDE configuration
├── .envrc              # direnv configuration
└── src/
    └── main.rs
```

---

## Summary

Nix provides Rust developers with:

1. **Reproducible environments** - Same toolchain everywhere
2. **Latest Rust versions** - Via rust-overlay
3. **Easy cross-compilation** - Nix handles toolchains
4. **IDE integration** - rust-analyzer works out of the box
5. **Team onboarding** - One command: `nix develop`
6. **CI/CD consistency** - Same environment as development

Start with a basic flake, add rust-overlay for version control, and use direnv for seamless IDE integration.
