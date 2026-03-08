{
  description = "ChoirOS flake outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    microvm = {
      url = "github:microvm-nix/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, disko, microvm, ... }:
    let
      # Packages are x86_64-linux only (deployment target)
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

      # Workspace source (sandbox + hypervisor + shared-types)
      workspaceSrc = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (craneLib.filterCargoSources path type)
          || (builtins.baseNameOf path) == "Cargo.lock"
          || (pkgs.lib.hasInfix "/.sqlx/" (toString path))
          || (pkgs.lib.hasInfix "/migrations/" (toString path));
      };

      commonArgs = {
        src = workspaceSrc;
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

      # --- Frontend (dioxus-desktop) is a separate Cargo workspace ---
      frontendSrc = pkgs.lib.cleanSourceWith {
        src = ./dioxus-desktop;
        filter = path: type:
          (craneLib.filterCargoSources path type)
          || (builtins.baseNameOf path) == "Cargo.lock"
          || (pkgs.lib.hasSuffix ".js" path);
      };

      frontendCommonArgs = {
        src = frontendSrc;
        strictDeps = true;
        cargoLock = ./dioxus-desktop/Cargo.lock;
        postPatch = ''
          cp -r ${./shared-types} ./shared-types
          chmod -R u+w ./shared-types
          substituteInPlace Cargo.toml --replace-fail "../shared-types" "./shared-types"
          cat > shared-types/Cargo.toml <<'EOF'
[package]
name = "shared-types"
version = "0.1.0"
edition = "2021"
rust-version = "1.76"
authors = ["ChoirOS Team"]
license = "MIT"
description = "Shared types between frontend and backend"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.11", features = ["v4", "serde"] }
ulid = { version = "1.1", features = ["serde"] }
ts-rs = { version = "12.0", features = ["chrono-impl"] }

[dev-dependencies]
EOF
        '';
      };

      wasmToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
        targets = [ "wasm32-unknown-unknown" ];
      };
      wasmCraneLib = (crane.mkLib pkgs).overrideToolchain wasmToolchain;
      wasmArgs = frontendCommonArgs // {
        pname = "dioxus-desktop-wasm";
        CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
        doCheck = false;
        installPhaseCommand = "mkdir -p $out";
      };
      wasmArtifacts = wasmCraneLib.buildDepsOnly wasmArgs;
      wasmBuild = wasmCraneLib.buildPackage (wasmArgs // {
        cargoArtifacts = wasmArtifacts;
        cargoExtraArgs = "--lib";
        installPhaseCommand = ''
          mkdir -p $out/lib
          cp target/wasm32-unknown-unknown/release/dioxus_desktop.wasm $out/lib/
        '';
      });

      # VM runner store paths (for runtime-ctl injection)
      vmRunnerLive = self.nixosConfigurations.choiros-ch-sandbox-live
        .config.microvm.runner.cloud-hypervisor;
    in
    {
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

        frontend = pkgs.stdenv.mkDerivation {
          name = "dioxus-desktop-web";
          nativeBuildInputs = [ pkgs.wasm-bindgen-cli pkgs.binaryen ];
          dontUnpack = true;
          buildPhase = ''
            mkdir -p out
            wasm-bindgen --target web --out-dir out ${wasmBuild}/lib/dioxus_desktop.wasm
            wasm-opt -Os out/dioxus_desktop_bg.wasm -o out/dioxus_desktop_bg.wasm
          '';
          installPhase = ''
            mkdir -p $out/assets
            mv out/dioxus_desktop_bg.wasm $out/assets/
            mv out/dioxus_desktop.js $out/assets/
            cp -r ${./dioxus-desktop/public}/* $out/
            cat > $out/index.html <<'HTML'
            <!DOCTYPE html>
            <html>
              <head>
                <title>ChoirOS</title>
                <meta content="text/html;charset=utf-8" http-equiv="Content-Type">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta charset="UTF-8">
              </head>
              <body>
                <div id="main"></div>
                <script type="module">
                  import init from './assets/dioxus_desktop.js';
                  init();
                </script>
              </body>
            </html>
            HTML
          '';
        };

        runtime-ctl = pkgs.writeScriptBin "ovh-runtime-ctl" ''
          #!/usr/bin/env bash
          set -euo pipefail
          export PATH="${pkgs.lib.makeBinPath (with pkgs; [
            bash coreutils curl findutils gnugrep gnused gawk
            iproute2 procps socat util-linux cloud-hypervisor
          ])}:$PATH"
          ${builtins.readFile ./scripts/ops/ovh-runtime-ctl.sh}
        '';
      };

      nixosModules = {
        choiros-platform-secrets = import ./nix/modules/choiros-platform-secrets.nix;
      };

      nixosConfigurations.choiros-vfkit-user = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [
          microvm.nixosModules.microvm
          ./nix/vfkit/user-vm.nix
        ];
      };

      nixosConfigurations.choiros-ovh-node = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          choirosPackages = self.packages.${system};
          vmRunnerLive = vmRunnerLive;
        };
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node.nix
        ];
      };

      nixosConfigurations.choiros-a = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          choirosPackages = self.packages.${system};
          vmRunnerLive = vmRunnerLive;
        };
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node-a.nix
        ];
      };

      nixosConfigurations.choiros-b = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          choirosPackages = self.packages.${system};
          vmRunnerLive = vmRunnerLive;
        };
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node-b.nix
        ];
      };

      # Cloud-hypervisor sandbox microVMs (x86_64-linux, run on OVH hosts)
      nixosConfigurations.choiros-ch-sandbox-live = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          sandboxRole = "live";
          sandboxPort = 8080;
          vmIp = "10.0.0.10";
          vmMac = "52:54:00:00:00:0a";
          vmTap = "tap-live";
          sandboxPackage = self.packages.${system}.sandbox;
        };
        modules = [
          { nixpkgs.overlays = [ (import ./nix/overlays/virtiofsd-vhost-fix.nix) ]; }
          microvm.nixosModules.microvm
          ./nix/ch/sandbox-vm.nix
        ];
      };

      nixosConfigurations.choiros-ch-sandbox-dev = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          sandboxRole = "dev";
          sandboxPort = 8081;
          vmIp = "10.0.0.11";
          vmMac = "52:54:00:00:00:0b";
          vmTap = "tap-dev";
          sandboxPackage = self.packages.${system}.sandbox;
        };
        modules = [
          { nixpkgs.overlays = [ (import ./nix/overlays/virtiofsd-vhost-fix.nix) ]; }
          microvm.nixosModules.microvm
          ./nix/ch/sandbox-vm.nix
        ];
      };
    };
}
