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
    mem = 3072;

    interfaces = [{
      type = "tap";
      id = vmTap;
      mac = vmMac;
    }];

    # Mutable sandbox runtime state on virtio-blk — needed for VM snapshot/restore.
    # (cloud-hypervisor snapshots capture virtio-blk state atomically; virtiofs
    # can't restore FUSE file handles after restore.)
    volumes = [{
      image = "data.img";
      mountPoint = "/opt/choiros/data/sandbox";
      size = 2048;
    }];

    shares = [
      {
        # Host nix store (read-only) — needed because the sandbox binary
        # and its runtime closure live in /nix/store on the host.
        proto = "virtiofs";
        tag = "nix-store";
        source = "/nix/store";
        mountPoint = "/nix/store";
      }
      {
        proto = "virtiofs";
        tag = "choiros-creds";
        source = "/run/choiros/credentials/sandbox";
        mountPoint = "/run/choiros/credentials/sandbox";
      }
      # NOTE: per-user data is on virtio-blk (data.img), NOT virtiofs.
      # virtiofs FUSE handles are not captured by cloud-hypervisor VM snapshots
      # (issue #6931), so mutable data must use virtio-blk which IS snapshot-safe.
      # The data.img file lives on a per-user btrfs subvolume on the host,
      # symlinked into the VM state dir by runtime-ctl.
    ];
  };

  # Static networking on the br-choiros bridge.
  # Use systemd-networkd with MAC-based matching since interface names
  # are unpredictable inside cloud-hypervisor (enp0sX, not eth0).
  networking.useDHCP = false;
  systemd.network = {
    enable = true;
    networks."10-vm" = {
      matchConfig.MACAddress = vmMac;
      networkConfig = {
        Address = "${vmIp}/24";
        Gateway = "10.0.0.1";
        DNS = [ "1.1.1.1" "8.8.8.8" ];
      };
    };
  };

  # Sandbox service — binary from nix store (via virtiofs /nix/store mount)
  systemd.services.choir-sandbox = {
    description = "ChoirOS Sandbox (${sandboxRole})";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${sandboxPackage}/bin/sandbox";
      Restart = "on-failure";
      RestartSec = 1;
      EnvironmentFile = "/run/choiros/credentials/sandbox/sandbox.env";
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
