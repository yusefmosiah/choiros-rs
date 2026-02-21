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
        src = pkgs.lib.cleanSourceWith {
          src = ../.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || (builtins.baseNameOf path) == "Cargo.lock";
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          cargoExtraArgs = "--manifest-path dioxus-desktop/Cargo.toml";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in {
        packages.desktop = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--manifest-path dioxus-desktop/Cargo.toml --bin sandbox-ui";
        });

        packages.default = self.packages.${system}.desktop;

        checks.desktop-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--manifest-path dioxus-desktop/Cargo.toml --all-targets -- -D warnings";
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
