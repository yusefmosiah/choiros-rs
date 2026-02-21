{
  description = "ChoirOS hypervisor";

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
          pname = "hypervisor";
          version = "0.1.0";
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ];
          cargoExtraArgs = "-p hypervisor";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in {
        packages.hypervisor = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "-p hypervisor --bin hypervisor";
        });

        packages.default = self.packages.${system}.hypervisor;

        checks.hypervisor-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "-p hypervisor --all-targets -- -D warnings";
        });

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            toolchain
            just
            sqlx-cli
            pkg-config
            openssl
          ];
          SQLX_OFFLINE = "true";
        };
      });
}
