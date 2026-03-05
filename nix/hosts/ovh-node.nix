# NixOS host configuration for OVH SYS-1 bare metal (x86_64-linux)
# Intel Xeon-E 2136 / 32GB RAM / 2x NVMe RAID1 / UEFI
{ config, lib, pkgs, ... }:
{
  boot.loader.efi.canTouchEfiVariables = true;
  boot.loader.efi.efiSysMountPoint = "/boot/efi";
  boot.loader.grub = {
    enable = true;
    efiSupport = true;
    devices = [ "nodev" ];
  };

  boot.initrd.availableKernelModules = [
    "ahci"
    "nvme"
    "sd_mod"
    "xhci_pci"
    "raid1"
    "md_mod"
  ];
  boot.swraid.enable = true;
  boot.swraid.mdadmConf = "MAILADDR root";

  networking.useDHCP = true;

  # SSH access
  services.openssh = {
    enable = true;
    openFirewall = true;
    settings = {
      PermitRootLogin = "prohibit-password";
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
    };
  };

  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILN3IIn6TzBBExWiJTJ7aDlA/LlEMXvjFlSfkKkV02TZ wiz@choiros-ovh"
  ];

  # Caddy reverse proxy (TLS termination -> hypervisor)
  services.caddy = {
    enable = true;
    # Caddyfile is overridden per-node via extraConfig or virtualHosts.
    # Default: reverse proxy to hypervisor on :9090.
    virtualHosts."http://:80" = {
      extraConfig = ''
        reverse_proxy 127.0.0.1:9090
      '';
    };
  };

  # Firewall
  networking.firewall = {
    enable = true;
    allowedTCPPorts = [
      22    # SSH
      80    # HTTP
      443   # HTTPS
      9090  # Hypervisor ingress (direct, for health checks)
    ];
  };

  # Nix settings
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    auto-optimise-store = true;
  };

  # System packages
  environment.systemPackages = with pkgs; [
    bash
    btop
    coreutils
    curl
    git
    gnugrep
    gnused
    htop
    jq
    openssl
    procps
    ripgrep
    sqlite
    tmux
    vim
  ];

  # Workspace and runtime directories
  systemd.tmpfiles.rules = [
    "d /opt/choiros 0755 root root -"
    "d /opt/choiros/bin 0755 root root -"
    "d /opt/choiros/workspace 0755 root root -"
    "d /opt/choiros/data 0750 root root -"
    "d /run/choiros/credentials/platform 0700 root root -"
  ];

  # ChoirOS Hypervisor service
  systemd.services.hypervisor = {
    description = "ChoirOS Hypervisor";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/hypervisor";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      Environment = [
        "HYPERVISOR_PORT=9090"
        "HYPERVISOR_DATABASE_URL=sqlite:/opt/choiros/data/hypervisor.db"
        "SANDBOX_VFKIT_CTL=/opt/choiros/bin/ovh-runtime-ctl"
        "SANDBOX_LIVE_PORT=8080"
        "SANDBOX_DEV_PORT=8081"
        "WEBAUTHN_RP_ID=localhost"
        "WEBAUTHN_RP_ORIGIN=http://localhost:9090"
      ];
    };
  };

  # ChoirOS Sandbox (live) service
  systemd.services.sandbox-live = {
    description = "ChoirOS Sandbox (live)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/sandbox";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      Environment = [
        "PORT=8080"
        "DATABASE_URL=sqlite:/opt/choiros/data/sandbox-live.db"
        "SQLX_OFFLINE=true"
        "CHOIR_SANDBOX_ROLE=live"
      ];
    };
  };

  # ChoirOS Sandbox (dev) service
  systemd.services.sandbox-dev = {
    description = "ChoirOS Sandbox (dev)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/sandbox";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      Environment = [
        "PORT=8081"
        "DATABASE_URL=sqlite:/opt/choiros/data/sandbox-dev.db"
        "SQLX_OFFLINE=true"
        "CHOIR_SANDBOX_ROLE=dev"
      ];
    };
  };

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
