# Cloud-hypervisor guest NixOS config for sandbox microVMs (x86_64-linux).
# Used on OVH bare metal hosts. Per-instance values (role, port, IP, MAC)
# are passed via specialArgs from flake.nix.
{ config, lib, pkgs, sandboxRole, sandboxPort, vmIp, vmMac, vmTap,
  sandboxPackage, ... }:
{
  networking.hostName = "sandbox-${sandboxRole}";

  microvm = {
    hypervisor = "cloud-hypervisor";
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

    # ADR-0018: All virtiofs shares removed. With shares=[], the microvm
    # module automatically generates an erofs disk for the nix store closure.
    # This is a single shared file in /nix/store, referenced by all VMs.
    # Combined with shared=off (KSM), identical pages are deduplicated.
    # Creds share dropped — gateway token injected via kernel cmdline.
    shares = [];
  };

  # ADR-0018 Phase 7: virtio-pmem support.
  # Include pmem/dax kernel modules in the initrd filesystem.
  boot.initrd.availableKernelModules = [
    "virtio_pmem" "nd_pmem" "nd_virtio" "libnvdimm"
    "dax" "dax_pmem"
  ];

  # Custom initrd oneshot to load virtio_pmem before the /nix/store mount.
  # boot.initrd.kernelModules doesn't populate modules-load.d/nixos.conf
  # in the microvm systemd initrd, and rd.modules_load= via cmdline causes
  # systemd-modules-load.service to fail (encrypted_key cipher dependency).
  # This targeted modprobe bypasses both issues.
  boot.initrd.systemd.services.load-virtio-pmem = {
    description = "Load virtio_pmem kernel module";
    wantedBy = [ "sysinit.target" ];
    before = [ "sysroot-nix-store.mount" ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "/bin/modprobe virtio_pmem";
    };
  };

  # Override nix-store mount: use /dev/pmem0 instead of /dev/disk/by-label/nix-store.
  # The microvm module generates a by-label mount, but pmem devices don't
  # create by-label symlinks without proper udev rules in initrd.
  fileSystems."/nix/store" = lib.mkForce {
    device = "/dev/pmem0";
    fsType = "erofs";
    options = [ "x-initrd.mount" "x-systemd.after=load-virtio-pmem.service" "ro" ];
  };

  # With pmem, erofs is no longer a --disk device. data.img moves from
  # /dev/vdb to /dev/vda (it's now the only --disk entry).
  fileSystems."/opt/choiros/data/sandbox" = lib.mkForce {
    device = "/dev/vda";
    fsType = "ext4";
    options = [ "defaults" ];
  };

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

  environment.systemPackages = with pkgs; [
    coreutils
    curl
    procps
  ];

  system.stateVersion = "25.11";
}
