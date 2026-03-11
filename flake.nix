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
    microvm = {
      url = "github:yusefmosiah/microvm.nix/main";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, microvm, ... }:
    let
      # Packages are x86_64-linux only (deployment target)
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          (import rust-overlay)
          # ADR-0018: virtiofsd overlay removed — no more virtiofs shares
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

      mkSandboxVm = {
        sandboxRole,
        sandboxPort,
        vmIp,
        vmMac,
        vmTap,
        sandboxHypervisor ? "cloud-hypervisor",
        sandboxStoreDiskInterface ? "blk",
        guestProfile ? "minimal",
      }:
        nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          specialArgs = {
            inherit
              sandboxRole
              sandboxPort
              vmIp
              vmMac
              vmTap
              sandboxHypervisor
              sandboxStoreDiskInterface
              guestProfile
              ;
            sandboxPackage = self.packages.${system}.sandbox;
          };
          modules = [
            microvm.nixosModules.microvm
            ./nix/ch/sandbox-vm.nix
          ];
        };

      # VM runner store paths (for host injection into machine-classes.toml)
      vmRunnerChPmem = self.nixosConfigurations.choiros-ch-sandbox-live
        .config.microvm.runner.cloud-hypervisor;
      vmRunnerChBlk = self.nixosConfigurations.choiros-ch-sandbox-live-blk
        .config.microvm.runner.cloud-hypervisor;
      vmRunnerFcPmem = self.nixosConfigurations.choiros-fc-sandbox-live
        .config.microvm.runner.firecracker;
      vmRunnerFcBlk = self.nixosConfigurations.choiros-fc-sandbox-live-blk
        .config.microvm.runner.firecracker;

      # Worker image runners (thick guest with dev toolchain)
      vmRunnerWorkerChPmem = self.nixosConfigurations.choiros-worker-ch-pmem
        .config.microvm.runner.cloud-hypervisor;
      vmRunnerWorkerChBlk = self.nixosConfigurations.choiros-worker-ch-blk
        .config.microvm.runner.cloud-hypervisor;

      # Legacy alias (still used by ovh-node.nix cloud-hypervisor@ template)
      vmRunnerLive = vmRunnerChPmem;

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
            # wasm-bindgen generates a snippets/ dir with JS dependencies
            if [ -d out/snippets ]; then
              cp -r out/snippets $out/assets/snippets
            fi
            cp -r ${./dioxus-desktop/public}/* $out/
            cat > $out/index.html <<'HTMLEOF'
            <!DOCTYPE html>
            <html>
              <head>
                <title>ChoirOS</title>
                <meta content="text/html;charset=utf-8" http-equiv="Content-Type">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta charset="UTF-8">
                <style>
                  * { margin: 0; padding: 0; box-sizing: border-box; }
                  html, body { height: 100%; overflow: hidden; background: #0a0e1a; }
                  #bios-boot {
                    position: fixed; inset: 0; z-index: 99999;
                    background: #0a0e1a;
                    font-family: 'IBM Plex Mono', 'Menlo', 'Consolas', monospace;
                    font-size: 13px; line-height: 1.55;
                    color: #c0c8d8;
                    padding: 1.5rem 2rem;
                    overflow: hidden;
                    transition: opacity 0.4s ease-out;
                  }
                  #bios-boot.fade-out { opacity: 0; pointer-events: none; }
                  .bios-header {
                    color: #7aa2f7; font-weight: bold; font-size: 14px;
                    border-bottom: 1px solid #2a3050; padding-bottom: 0.5rem;
                    margin-bottom: 0.75rem;
                  }
                  .bios-line { opacity: 0; animation: line-in 0.06s forwards; white-space: pre; }
                  .bios-ok { color: #9ece6a; }
                  .bios-warn { color: #e0af68; }
                  .bios-dim { color: #565f89; }
                  .bios-bright { color: #c0caf5; }
                  @keyframes line-in { to { opacity: 1; } }
                  @keyframes cursor-blink {
                    0%, 100% { opacity: 1; } 50% { opacity: 0; }
                  }
                  .bios-cursor {
                    display: inline-block; width: 7px; height: 13px;
                    background: #c0caf5; vertical-align: text-bottom;
                    animation: cursor-blink 1s step-end infinite;
                    margin-left: 2px;
                  }
                </style>
              </head>
              <body>
                <div id="bios-boot">
                  <div class="bios-header">ChoirOS v0.1.0 System Bootstrap</div>
                  <div id="bios-lines"></div>
                </div>
                <div id="main"></div>
                <script>
                  (function() {
                    var lines = [
                      { t: "POST: CPU . . . . . . . . . . . . . . ", s: "ok", d: 80 },
                      { t: "POST: Memory 2048 MB . . . . . . . .  ", s: "ok", d: 60 },
                      { t: "POST: Storage (virtio-blk) . . . . .  ", s: "ok", d: 90 },
                      { t: "POST: Network (virtio-net) . . . . .  ", s: "ok", d: 70 },
                      { t: "POST: virtiofs /nix/store . . . . . . ", s: "ok", d: 100 },
                      { t: "", d: 40 },
                      { t: "Loading kernel modules:", s: "", d: 50, cls: "bios-bright" },
                      { t: "  sandbox-runtime . . . . . . . . . . ", s: "ok", d: 45 },
                      { t: "  provider-gateway  . . . . . . . . . ", s: "ok", d: 40 },
                      { t: "  event-bus . . . . . . . . . . . . . ", s: "ok", d: 35 },
                      { t: "  conductor . . . . . . . . . . . . . ", s: "ok", d: 40 },
                      { t: "  writer-runtime  . . . . . . . . . . ", s: "ok", d: 45 },
                      { t: "  terminal-actor  . . . . . . . . . . ", s: "ok", d: 35 },
                      { t: "", d: 40 },
                      { t: "Initializing display server:", s: "", d: 60, cls: "bios-bright" },
                      { t: "  WASM runtime  . . . . . . . . . . . ", s: "loading", d: 0, id: "wasm-line" },
                    ];
                    var el = document.getElementById("bios-lines");
                    var i = 0;
                    var totalDelay = 0;
                    lines.forEach(function(line) {
                      totalDelay += line.d;
                      setTimeout(function() {
                        var div = document.createElement("div");
                        div.className = "bios-line" + (line.cls ? " " + line.cls : "");
                        div.style.animationDelay = "0s";
                        if (line.id) div.id = line.id;
                        if (line.t === "") {
                          div.innerHTML = "&nbsp;";
                        } else if (line.s === "ok") {
                          div.innerHTML = '<span class="bios-dim">' + line.t + '</span><span class="bios-ok">[  OK  ]</span>';
                        } else if (line.s === "loading") {
                          div.innerHTML = '<span class="bios-dim">' + line.t + '</span><span class="bios-warn">[  ..  ]</span>';
                        } else {
                          div.textContent = line.t;
                        }
                        el.appendChild(div);
                      }, totalDelay);
                    });
                    window.__biosBootEl = document.getElementById("bios-boot");
                    window.__biosComplete = function() {
                      var wl = document.getElementById("wasm-line");
                      if (wl) wl.innerHTML = '<span class="bios-dim">  WASM runtime  . . . . . . . . . . . </span><span class="bios-ok">[  OK  ]</span>';
                      var ready = document.createElement("div");
                      ready.className = "bios-line bios-bright";
                      ready.style.marginTop = "0.5rem";
                      ready.innerHTML = 'System ready. <span class="bios-cursor"></span>';
                      el.appendChild(ready);
                      setTimeout(function() {
                        document.getElementById("bios-boot").classList.add("fade-out");
                        setTimeout(function() {
                          document.getElementById("bios-boot").style.display = "none";
                        }, 500);
                      }, 400);
                    };
                  })();
                </script>
                <script type="module">
                  import init from './assets/dioxus_desktop.js';
                  init().then(function() {
                    // WASM loaded — signal will be sent from Dioxus after auth
                  });
                </script>
              </body>
            </html>
            HTMLEOF
          '';
        };

        runtime-ctl = pkgs.writeTextFile {
          name = "ovh-runtime-ctl";
          executable = true;
          destination = "/bin/ovh-runtime-ctl";
          text = let
            runtimePath = pkgs.lib.makeBinPath (with pkgs; [
              bash btrfs-progs coreutils curl findutils gnugrep gnused gawk
              iproute2 procps socat util-linux cloud-hypervisor
            ]);
            # Read the script and strip lines that conflict with our wrapper
            rawLines = pkgs.lib.splitString "\n"
              (builtins.readFile ./scripts/ops/ovh-runtime-ctl.sh);
            filteredLines = builtins.filter (line:
              !(pkgs.lib.hasPrefix "#!" line)
              && line != "set -euo pipefail"
              && !(pkgs.lib.hasPrefix "export PATH=" line)
            ) rawLines;
            scriptBody = builtins.concatStringsSep "\n" filteredLines;
          in ''
            #!${pkgs.bash}/bin/bash
            set -euo pipefail
            export PATH="${runtimePath}:$PATH"
            ${scriptBody}
          '';
        };
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

      nixosConfigurations.choiros-a = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          choirosPackages = self.packages.${system};
          vmRunnerLive = vmRunnerLive;
          vmStoreDiskInterface = "pmem";
          inherit vmRunnerChPmem vmRunnerChBlk vmRunnerFcPmem vmRunnerFcBlk
                 vmRunnerWorkerChPmem vmRunnerWorkerChBlk;
        };
        modules = [
          ./nix/hosts/ovh-node-hardware.nix
          ./nix/hosts/ovh-node-a-disks.nix
          ./nix/hosts/ovh-node-a.nix
        ];
      };

      nixosConfigurations.choiros-b = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          choirosPackages = self.packages.${system};
          vmRunnerLive = vmRunnerLive;
          vmStoreDiskInterface = "pmem";
          inherit vmRunnerChPmem vmRunnerChBlk vmRunnerFcPmem vmRunnerFcBlk
                 vmRunnerWorkerChPmem vmRunnerWorkerChBlk;
        };
        modules = [
          ./nix/hosts/ovh-node-hardware.nix
          ./nix/hosts/ovh-node-b-disks.nix
          ./nix/hosts/ovh-node-b.nix
        ];
      };

      # Sandbox matrix for transport and backend comparisons on OVH hosts.
      nixosConfigurations.choiros-ch-sandbox-live = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "pmem";
      };

      nixosConfigurations.choiros-ch-sandbox-live-blk = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "blk";
      };

      nixosConfigurations.choiros-ch-sandbox-dev = mkSandboxVm {
        sandboxRole = "dev";
        sandboxPort = 8081;
        vmIp = "10.0.0.11";
        vmMac = "52:54:00:00:00:0b";
        vmTap = "tap-dev";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "pmem";
      };

      nixosConfigurations.choiros-ch-sandbox-dev-blk = mkSandboxVm {
        sandboxRole = "dev";
        sandboxPort = 8081;
        vmIp = "10.0.0.11";
        vmMac = "52:54:00:00:00:0b";
        vmTap = "tap-dev";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "blk";
      };

      nixosConfigurations.choiros-fc-sandbox-live = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "firecracker";
        sandboxStoreDiskInterface = "pmem";
      };

      nixosConfigurations.choiros-fc-sandbox-live-blk = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "firecracker";
        sandboxStoreDiskInterface = "blk";
      };

      nixosConfigurations.choiros-fc-sandbox-dev = mkSandboxVm {
        sandboxRole = "dev";
        sandboxPort = 8081;
        vmIp = "10.0.0.11";
        vmMac = "52:54:00:00:00:0b";
        vmTap = "tap-dev";
        sandboxHypervisor = "firecracker";
        sandboxStoreDiskInterface = "pmem";
      };

      nixosConfigurations.choiros-fc-sandbox-dev-blk = mkSandboxVm {
        sandboxRole = "dev";
        sandboxPort = 8081;
        vmIp = "10.0.0.11";
        vmMac = "52:54:00:00:00:0b";
        vmTap = "tap-dev";
        sandboxHypervisor = "firecracker";
        sandboxStoreDiskInterface = "blk";
      };

      # Worker image VMs (thick guest with dev toolchain + Playwright)
      # Only ch-pmem and ch-blk — workers use cloud-hypervisor for KSM density.
      nixosConfigurations.choiros-worker-ch-pmem = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "pmem";
        guestProfile = "worker";
      };

      nixosConfigurations.choiros-worker-ch-blk = mkSandboxVm {
        sandboxRole = "live";
        sandboxPort = 8080;
        vmIp = "10.0.0.10";
        vmMac = "52:54:00:00:00:0a";
        vmTap = "tap-live";
        sandboxHypervisor = "cloud-hypervisor";
        sandboxStoreDiskInterface = "blk";
        guestProfile = "worker";
      };
    };
}
