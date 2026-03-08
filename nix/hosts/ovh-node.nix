# NixOS host configuration for OVH SYS-1 bare metal (x86_64-linux)
# Intel Xeon-E 2136 / 32GB RAM / 2x NVMe RAID1 / UEFI
{ config, lib, pkgs, choirosPackages, vmRunnerLive, ... }:
{
  nixpkgs.overlays = [
    (import ../overlays/virtiofsd-vhost-fix.nix)
  ];
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
    "btrfs"
  ];
  boot.swraid.enable = true;
  boot.swraid.mdadmConf = "MAILADDR root";

  networking.useDHCP = true;

  # Bridge for sandbox microVMs (cloud-hypervisor TAP interfaces)
  networking.bridges.br-choiros = {
    interfaces = []; # TAP devices added dynamically by runtime-ctl
  };
  networking.interfaces.br-choiros.ipv4.addresses = [{
    address = "10.0.0.1";
    prefixLength = 24;
  }];

  # NAT for VM internet access (e.g., DNS resolution)
  boot.kernel.sysctl."net.ipv4.ip_forward" = 1;
  networking.nat = {
    enable = true;
    internalInterfaces = [ "br-choiros" ];
    externalInterface = "eno1";
  };

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
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAfYv0qn1XjuKuddQqmDEk/nS3NUP/6+1pG9/DRq4NUS github-actions-deploy@choiros"
  ];

  # Unprivileged user for sandbox processes
  users.users.choiros = {
    isSystemUser = true;
    group = "choiros";
    home = "/var/lib/choiros";
    createHome = true;
  };
  users.groups.choiros = {};

  # Caddy reverse proxy (TLS termination -> hypervisor)
  services.caddy = {
    enable = true;
    virtualHosts."choir-ip.com" = {
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
    trustedInterfaces = [ "br-choiros" ]; # Allow VM traffic
  };

  # Nix settings
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    auto-optimise-store = true;
  };

  # System packages
  environment.systemPackages = with pkgs; [
    bash
    btrfs-progs
    btop
    cloud-hypervisor
    coreutils
    curl
    git
    gnugrep
    gnused
    htop
    iproute2
    jq
    openssl
    procps
    ripgrep
    socat
    sqlite
    tmux
    vim
    virtiofsd
  ];

  # Workspace directory (still needed for git pull during deploys)
  systemd.tmpfiles.rules = [
    "d /opt/choiros 0755 root root -"
    "d /opt/choiros/workspace 0755 root root -"
    "d /opt/choiros/data 0755 root choiros -"
    "d /opt/choiros/data/sandbox 0750 choiros choiros -"
    # Per-user storage on btrfs @data subvolume
    "d /data/users 0755 root root -"
    "d /data/snapshots 0755 root root -"
    # Persistent secrets (survive reboot, on NVMe)
    "d /opt/choiros/secrets 0700 root root -"
    "d /opt/choiros/secrets/platform 0700 root root -"
    "d /opt/choiros/secrets/sandbox 0700 root root -"
    # Runtime credentials (tmpfs, populated by materialize service)
    "d /run/choiros/credentials/platform 0700 root root -"
    "d /run/choiros/credentials/sandbox 0700 choiros choiros -"
  ];

  # Materialize secrets from persistent NVMe storage to tmpfs on boot.
  # Secrets are delivered once via SCP to /opt/choiros/secrets/ and survive reboots.
  # This oneshot copies them to /run/choiros/credentials/ where services consume them.
  systemd.services.choiros-secrets-materialize = {
    description = "Materialize ChoirOS secrets from persistent storage to tmpfs";
    wantedBy = [ "multi-user.target" ];
    after = [ "local-fs.target" ];
    before = [ "hypervisor.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      set -euo pipefail

      # Platform secrets (root-owned, consumed by hypervisor via LoadCredential)
      src=/opt/choiros/secrets/platform
      dst=/run/choiros/credentials/platform
      mkdir -p "$dst"
      if [ -d "$src" ] && [ "$(ls -A "$src" 2>/dev/null)" ]; then
        cp -a "$src"/. "$dst"/
        chmod 0700 "$dst"
        chmod 0600 "$dst"/*
        echo "Materialized $(ls "$dst" | wc -l) platform credential files"
      else
        echo "WARNING: No platform secrets found in $src" >&2
      fi

      # Sandbox secrets (choiros-owned, consumed by sandbox services)
      src=/opt/choiros/secrets/sandbox
      dst=/run/choiros/credentials/sandbox
      mkdir -p "$dst"
      if [ -d "$src" ] && [ "$(ls -A "$src" 2>/dev/null)" ]; then
        cp -a "$src"/. "$dst"/
        chown -R choiros:choiros "$dst"
        chmod 0700 "$dst"
        chmod 0600 "$dst"/*
        echo "Materialized $(ls "$dst" | wc -l) sandbox credential files"
      else
        echo "WARNING: No sandbox secrets found in $src" >&2
      fi
    '';
  };

  # ChoirOS Hypervisor service — ExecStart points to nix store path
  systemd.services.hypervisor = {
    description = "ChoirOS Hypervisor";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "choiros-secrets-materialize.service" ];
    wants = [ "network-online.target" ];
    requires = [ "choiros-secrets-materialize.service" ];
    serviceConfig = {
      ExecStart = "${choirosPackages.hypervisor}/bin/hypervisor";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      LoadCredential = [
        "zai_api_key:/run/choiros/credentials/platform/zai_api_key"
        "kimi_api_key:/run/choiros/credentials/platform/kimi_api_key"
        "openai_api_key:/run/choiros/credentials/platform/openai_api_key"
        "inception_api_key:/run/choiros/credentials/platform/inception_api_key"
        "tavily_api_key:/run/choiros/credentials/platform/tavily_api_key"
        "brave_api_key:/run/choiros/credentials/platform/brave_api_key"
        "exa_api_key:/run/choiros/credentials/platform/exa_api_key"
        "aws_bedrock:/run/choiros/credentials/platform/aws_bedrock"
        "provider_gateway_token:/run/choiros/credentials/platform/provider_gateway_token"
      ];
      EnvironmentFile = "/run/choiros/credentials/platform/hypervisor.env";
      Environment = [
        "HYPERVISOR_PORT=9090"
        "HYPERVISOR_DATABASE_URL=sqlite:/opt/choiros/data/hypervisor.db"
        "SANDBOX_VFKIT_CTL=${choirosPackages.runtime-ctl}/bin/ovh-runtime-ctl"
        "SANDBOX_LIVE_PORT=8080"
        "SANDBOX_DEV_PORT=8081"
        "FRONTEND_DIST=${choirosPackages.frontend}"
        "CHOIR_VM_RUNNER_DIR=${vmRunnerLive}"
      ];
    };
  };

  # VM state directory
  systemd.tmpfiles.settings."10-choiros-vms" = {
    "/opt/choiros/vms".d = { mode = "0755"; user = "root"; group = "root"; };
    "/opt/choiros/vms/state".d = { mode = "0755"; user = "root"; group = "root"; };
  };

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
