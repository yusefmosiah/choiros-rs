# Cloud-hypervisor guest NixOS config for sandbox microVMs (x86_64-linux).
# Used on OVH bare metal hosts. Per-instance values (role, port, IP, MAC)
# are passed via specialArgs from flake.nix.
{ config, lib, pkgs, sandboxRole, sandboxPort, vmIp, vmMac, vmTap, ... }:
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

    volumes = [{
      image = "data.img";
      mountPoint = "/var/lib/choiros";
      size = 2048;
    }];

    shares = [
      {
        # Host nix store (read-only) — needed because the sandbox binary
        # is dynamically linked against specific nix store paths.
        proto = "virtiofs";
        tag = "nix-store";
        source = "/nix/store";
        mountPoint = "/nix/store";
      }
      {
        proto = "virtiofs";
        tag = "choiros-bin";
        source = "/opt/choiros/bin";
        mountPoint = "/opt/choiros/bin";
      }
      {
        proto = "virtiofs";
        tag = "choiros-data";
        source = "/opt/choiros/data/sandbox";
        mountPoint = "/opt/choiros/data/sandbox";
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

  # Sandbox service
  systemd.services.choir-sandbox = {
    description = "ChoirOS Sandbox (${sandboxRole})";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/sandbox";
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
        "CHOIR_WRITER_ROOT_DIR=/opt/choiros/data/sandbox"
      ];
    };
  };

  # Allow sandbox port through firewall
  networking.firewall.allowedTCPPorts = [ sandboxPort ];

  # SSH for debugging (use host's authorized key)
  services.openssh = {
    enable = true;
    openFirewall = true;
    settings = {
      PermitRootLogin = "yes";
      PasswordAuthentication = false;
    };
  };
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILN3IIn6TzBBExWiJTJ7aDlA/LlEMXvjFlSfkKkV02TZ wiz@choiros-ovh"
  ];

  environment.systemPackages = with pkgs; [
    coreutils
    curl
    procps
  ];

  system.stateVersion = "25.11";
}
