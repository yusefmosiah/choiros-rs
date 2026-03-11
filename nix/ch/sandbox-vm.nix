# Sandbox guest NixOS config for microVMs (x86_64-linux).
# Used on OVH bare metal hosts. Per-instance values (role, port, IP, MAC)
# and transport choices are passed via specialArgs from flake.nix.
{ config, lib, pkgs, sandboxRole, sandboxPort, vmIp, vmMac, vmTap,
  sandboxPackage, sandboxHypervisor ? "cloud-hypervisor",
  sandboxStoreDiskInterface ? "blk", guestProfile ? "minimal", ... }:
{
  networking.hostName = "sandbox-${sandboxRole}";

  microvm = {
    hypervisor = sandboxHypervisor;
    vcpu = 2;
    mem = 1024;

    interfaces = [{
      type = "tap";
      id = vmTap;
      mac = vmMac;
    }];

    # Mutable sandbox runtime state on virtio-blk (/dev/vda).
    # (cloud-hypervisor snapshots capture virtio-blk state atomically.)
    volumes = [{
      image = "data.img";
      mountPoint = "/opt/choiros/data/sandbox";
      size = 2048;
    }];

    # Keep the transport explicit so we can build blk and pmem runners from
    # the same guest config and compare them intentionally.
    storeDiskInterface = sandboxStoreDiskInterface;

    # ADR-0018: All virtiofs shares removed. With shares=[], the microvm
    # module automatically generates an erofs disk for the nix store closure.
    # This is a single shared file in /nix/store, referenced by all VMs.
    # Combined with shared=off (KSM), identical pages are deduplicated.
    # Creds share dropped — gateway token injected via kernel cmdline.
    shares = [];
  };

  # ADR-0018 Phase 7: virtio-pmem kernel config.
  #
  # lib.mkForce overrides NixOS autoModules which otherwise sets
  # ACPI_NFIT=m → LIBNVDIMM=m → VIRTIO_PMEM=m. With mkForce, these
  # become true built-ins: driver present during PCI enumeration,
  # no module loading delay, ~2-3s faster boot.
  #
  # FS_DAX enables filesystem-level DAX: erofs reads nix store via
  # direct EPT mapping (zero guest page cache, ~100MB/VM savings).
  boot.kernelPatches = lib.mkIf (sandboxStoreDiskInterface == "pmem") [{
    name = "microvm-builtins";
    patch = null;
    structuredExtraConfig = with lib.kernel; {
      # Disable options that `select LIBNVDIMM` before VIRTIO_PMEM
      ACPI_NFIT = lib.mkForce no;
      X86_PMEM_LEGACY = lib.mkForce no;
      # Core virtio transport
      VIRTIO = lib.mkForce yes;
      VIRTIO_PCI = lib.mkForce yes;
      VIRTIO_BLK = lib.mkForce yes;
      VIRTIO_NET = lib.mkForce yes;
      # Persistent memory
      VIRTIO_PMEM = lib.mkForce yes;
      LIBNVDIMM = lib.mkForce yes;
      # DAX (filesystem-level direct access)
      DAX = lib.mkForce yes;
      FS_DAX = lib.mkForce yes;
      # Filesystems
      EROFS_FS = lib.mkForce yes;
      EXT4_FS = lib.mkForce yes;
    };
  }];

  # Belt-and-suspenders: if VIRTIO_PMEM ends up as a module despite mkForce,
  # ensure the initrd loads it so /dev/pmem0 appears before nix-store mount.
  boot.initrd.availableKernelModules = lib.mkIf (sandboxStoreDiskInterface == "pmem") [
    "virtio_pmem" "libnvdimm" "nd_pmem" "nd_btt"
    "virtio_pci" "virtio_blk" "virtio_net"
  ];

  # Uncompressed erofs for DAX support. Compressed erofs (lz4) cannot use
  # DAX because decompression requires a page cache buffer. Uncompressed
  # erofs is ~25-35% larger on disk but enables zero-copy DAX reads:
  # guest accesses host page cache directly via EPT, no guest page cache.
  microvm.storeDiskErofsFlags = lib.mkIf (sandboxStoreDiskInterface == "pmem") [];

  # The forked microvm.nix guest module now handles blk vs pmem device selection
  # for /nix/store and the volume drive-letter mapping for data.img. Keep Choir's
  # guest config transport-agnostic here so both interfaces remain buildable.

  # DHCP networking on the br-choiros bridge (ADR-0014: per-user VMs).
  # Host runs dnsmasq DHCP on the bridge. Guest gets IP via DHCP.
  # Match all virtio-net interfaces (VM only has one NIC).
  networking.useDHCP = false;
  systemd.network = {
    enable = true;
    networks."10-vm" = {
      matchConfig.Driver = "virtio_net";
      networkConfig = {
        DHCP = "ipv4";
      };
      dhcpV4Config = {
        UseDNS = true;
        UseRoutes = true;
      };
    };
  };

  # ADR-0018: Extract gateway token from kernel cmdline and write env file.
  # The host injects choir.gateway_token=<TOKEN> into --cmdline.
  # This oneshot runs before the sandbox service and exports it.
  systemd.services.choir-extract-cmdline-secrets = {
    description = "Extract ChoirOS secrets from kernel cmdline";
    wantedBy = [ "multi-user.target" ];
    before = [ "choir-sandbox.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      set -euo pipefail
      ENV_FILE="/run/choiros-sandbox.env"
      : > "$ENV_FILE"

      # Parse choir.gateway_token=VALUE from /proc/cmdline
      for param in $(cat /proc/cmdline); do
        case "$param" in
          choir.gateway_token=*)
            echo "CHOIR_PROVIDER_GATEWAY_TOKEN=''${param#choir.gateway_token=}" >> "$ENV_FILE"
            ;;
        esac
      done

      chmod 0600 "$ENV_FILE"
    '';
  };

  # Sandbox service — binary from nix store (erofs via virtio-pmem)
  systemd.services.choir-sandbox = {
    description = "ChoirOS Sandbox (${sandboxRole})";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "choir-extract-cmdline-secrets.service" ];
    wants = [ "network-online.target" ];
    requires = [ "choir-extract-cmdline-secrets.service" ];
    serviceConfig = {
      ExecStart = "${sandboxPackage}/bin/sandbox";
      Restart = "on-failure";
      RestartSec = 1;
      # ADR-0018: Gateway token extracted from kernel cmdline by oneshot above
      EnvironmentFile = [ "-/run/choiros-sandbox.env" ];
      Environment = [
        "PORT=${toString sandboxPort}"
        "DATABASE_URL=sqlite:/opt/choiros/data/sandbox/sandbox-${sandboxRole}.db"
        "SQLX_OFFLINE=true"
        "CHOIR_SANDBOX_ROLE=${sandboxRole}"
        "CHOIR_PROVIDER_GATEWAY_BASE_URL=http://10.0.0.1:9090"
        "HOME=/var/lib/choiros"
        "CHOIR_SANDBOX_ROOT=/opt/choiros/data/sandbox"
        "CHOIR_WRITER_ROOT_DIR=/opt/choiros/data/sandbox"
        "CHOIR_WORKSPACE_DIR=/opt/choiros/data/sandbox/workspace"
      ];
    };
  };

  # Allow sandbox port through firewall
  networking.firewall.allowedTCPPorts = [ sandboxPort ];

  environment.systemPackages = with pkgs;
    # Minimal profile: just enough to run the sandbox service
    (if guestProfile == "minimal" then [
      coreutils
      curl
      procps
    ]
    # Worker profile: full dev toolchain for build/test/E2E workflows
    else if guestProfile == "worker" then [
      # Core utilities
      coreutils
      curl
      procps
      bash
      gnused
      gnugrep
      gawk
      findutils
      which
      file
      less
      htop

      # Version control
      git
      openssh

      # Build toolchain
      gcc
      gnumake
      pkg-config
      openssl

      # Node.js + Playwright (E2E testing)
      nodejs_22
      # Playwright's bundled chromium needs these system libs
      nss
      nspr
      atk
      at-spi2-atk
      cups
      libdrm
      expat
      libxkbcommon
      pango
      cairo
      alsa-lib
      mesa
      xorg.libX11
      xorg.libXcomposite
      xorg.libXdamage
      xorg.libXext
      xorg.libXfixes
      xorg.libXrandr
      xorg.libxcb
      glib
      dbus
      gtk3

      # Rust toolchain (for building sandbox/worker code)
      rustc
      cargo

      # Go toolchain (ADR-0024: hypervisor rewrite, general dev)
      go

      # Useful for debugging
      strace
      gdb
    ]
    else throw "Unknown guestProfile: ${guestProfile}");

  system.stateVersion = "25.11";
}
