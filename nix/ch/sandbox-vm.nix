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

    # Mutable sandbox data on virtio-blk — survives VM snapshot/restore
    # (virtiofs can't restore FUSE file handle state, virtio-blk can)
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
