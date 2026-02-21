{
  description = "ChoirOS sandbox";

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
            || (builtins.baseNameOf path) == "Cargo.lock"
            || (pkgs.lib.hasPrefix (toString ../. + "/sandbox/.sqlx/") (toString path))
            || (pkgs.lib.hasPrefix (toString ../. + "/sandbox/migrations/") (toString path));
        };

        commonArgs = {
          inherit src;
          pname = "sandbox";
          version = "0.1.0";
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config protobuf ];
          buildInputs = with pkgs; [ openssl onnxruntime ];
          cargoExtraArgs = "-p sandbox";
          ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime${pkgs.stdenv.hostPlatform.extensions.sharedLibrary}";
          SQLX_OFFLINE = "true";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in {
        packages.sandbox = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "-p sandbox --bin sandbox";
        });

        packages.default = self.packages.${system}.sandbox;

        checks.sandbox-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "-p sandbox --all-targets -- -D warnings";
        });

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            toolchain
            just
            sqlx-cli
            pkg-config
            protobuf
            openssl
            onnxruntime
          ];
          SQLX_OFFLINE = "true";
          ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime${pkgs.stdenv.hostPlatform.extensions.sharedLibrary}";
        };
      });
}
