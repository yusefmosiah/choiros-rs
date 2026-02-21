{
  description = "ChoirOS desktop";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachSystem [ "aarch64-darwin" "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        src = pkgs.lib.cleanSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
          cargoLock = ./Cargo.lock;
          postPatch = ''
            cp -r ${../shared-types} ./shared-types
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

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in {
        packages.desktop = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin sandbox-ui";
        });

        packages.default = self.packages.${system}.desktop;

        checks.desktop-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- -D warnings";
        });

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            toolchain
            dioxus-cli
            binaryen
            wasm-bindgen-cli
            just
          ];
        };
      });
}
