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
    "d /opt/choiros/data 0755 root choiros -"
    "d /opt/choiros/data/sandbox 0750 choiros choiros -"
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
    before = [ "hypervisor.service" "sandbox-live.service" "sandbox-dev.service" ];
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

  # ChoirOS Hypervisor service
  systemd.services.hypervisor = {
    description = "ChoirOS Hypervisor";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "choiros-secrets-materialize.service" ];
    wants = [ "network-online.target" ];
    requires = [ "choiros-secrets-materialize.service" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/hypervisor";
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
        "SANDBOX_VFKIT_CTL=/opt/choiros/bin/ovh-runtime-ctl"
        "SANDBOX_LIVE_PORT=8080"
        "SANDBOX_DEV_PORT=8081"
        "FRONTEND_DIST=/opt/choiros/workspace/dioxus-desktop/target/dx/dioxus-desktop/release/web/public"
        "WEBAUTHN_RP_ID=choir-ip.com"
        "WEBAUTHN_RP_ORIGIN=https://choir-ip.com"
      ];
    };
  };

  # ChoirOS Sandbox (live) service — unprivileged, always uses provider gateway
  systemd.services.sandbox-live = {
    description = "ChoirOS Sandbox (live)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "choiros-secrets-materialize.service" ];
    wants = [ "network-online.target" ];
    requires = [ "choiros-secrets-materialize.service" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/sandbox";
      User = "choiros";
      Group = "choiros";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      EnvironmentFile = "/run/choiros/credentials/sandbox/sandbox.env";
      Environment = [
        "PORT=8080"
        "DATABASE_URL=sqlite:/opt/choiros/data/sandbox/sandbox-live.db"
        "SQLX_OFFLINE=true"
        "CHOIR_SANDBOX_ROLE=live"
        "CHOIR_PROVIDER_GATEWAY_BASE_URL=http://127.0.0.1:9090"
        "HOME=/var/lib/choiros"
        "CHOIR_WRITER_ROOT_DIR=/opt/choiros/data/sandbox"
      ];
    };
  };

  # ChoirOS Sandbox (dev) service — unprivileged, always uses provider gateway
  systemd.services.sandbox-dev = {
    description = "ChoirOS Sandbox (dev)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "choiros-secrets-materialize.service" ];
    wants = [ "network-online.target" ];
    requires = [ "choiros-secrets-materialize.service" ];
    serviceConfig = {
      ExecStart = "/opt/choiros/bin/sandbox";
      User = "choiros";
      Group = "choiros";
      Restart = "on-failure";
      RestartSec = 3;
      WorkingDirectory = "/opt/choiros/workspace";
      EnvironmentFile = "/run/choiros/credentials/sandbox/sandbox.env";
      Environment = [
        "PORT=8081"
        "DATABASE_URL=sqlite:/opt/choiros/data/sandbox/sandbox-dev.db"
        "SQLX_OFFLINE=true"
        "CHOIR_SANDBOX_ROLE=dev"
        "CHOIR_PROVIDER_GATEWAY_BASE_URL=http://127.0.0.1:9090"
        "HOME=/var/lib/choiros"
        "CHOIR_WRITER_ROOT_DIR=/opt/choiros/data/sandbox"
      ];
    };
  };

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
