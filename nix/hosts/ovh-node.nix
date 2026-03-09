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

  # Hardware config (initrd modules, swraid) is in ovh-node-hardware.nix

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
    # ADR-0017: hypervisor calls btrfs/mkfs.ext4/systemctl from Rust
    path = [ pkgs.btrfs-progs pkgs.e2fsprogs pkgs.util-linux pkgs.curl ];
    serviceConfig = {
      # Invalidate VM snapshots on (re)start — snapshot memory state references
      # virtiofsd socket paths that change on every rebuild. Cold boot is safe;
      # user data on virtio-blk (data.img) survives regardless.
      ExecStartPre = pkgs.writeShellScript "invalidate-vm-snapshots" ''
        for snap in /opt/choiros/vms/state/*/vm-snapshot; do
          [ -d "$snap" ] && rm -rf "$snap" && echo "invalidated: $snap"
        done
        # Also clear boot-mode files so ensure() picks cold boot
        for bm in /opt/choiros/vms/state/*/boot-mode; do
          [ -f "$bm" ] && rm -f "$bm"
        done
        exit 0
      '';
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
        # ADR-0017: opt-in to systemd-native VM lifecycle
        "CHOIR_SYSTEMD_LIFECYCLE=1"
      ];
    };
  };

  # VM state directory
  systemd.tmpfiles.settings."10-choiros-vms" = {
    "/opt/choiros/vms".d = { mode = "0755"; user = "root"; group = "root"; };
    "/opt/choiros/vms/state".d = { mode = "0755"; user = "root"; group = "root"; };
  };

  # ── ADR-0017: systemd unit templates for VM lifecycle ──────────────────
  # Each unit is parameterized by instance ID (%i), e.g., "live" or "dev".
  # The hypervisor starts these via `systemctl start socat-sandbox@live`,
  # which pulls in the full dependency chain automatically.

  # TAP device setup (oneshot, persists after exit for other units)
  systemd.services."tap-setup@" = {
    description = "TAP device for sandbox %i";
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = pkgs.writeShellScript "tap-setup-start" ''
        set -euo pipefail
        INSTANCE="$1"
        TAP="tap-''${INSTANCE}"
        # Truncate to 15 chars (IFNAMSIZ limit)
        TAP="''${TAP:0:15}"
        if ${pkgs.iproute2}/bin/ip link show "$TAP" &>/dev/null; then
          echo "TAP $TAP already exists"
          exit 0
        fi
        echo "Creating TAP $TAP on br-choiros"
        ${pkgs.iproute2}/bin/ip tuntap add dev "$TAP" mode tap vnet_hdr multi_queue
        ${pkgs.iproute2}/bin/ip link set "$TAP" master br-choiros
        ${pkgs.iproute2}/bin/ip link set "$TAP" up
      '' + " %i";
      ExecStop = pkgs.writeShellScript "tap-setup-stop" ''
        set -euo pipefail
        INSTANCE="$1"
        TAP="tap-''${INSTANCE}"
        TAP="''${TAP:0:15}"
        if ${pkgs.iproute2}/bin/ip link show "$TAP" &>/dev/null; then
          echo "Removing TAP $TAP"
          ${pkgs.iproute2}/bin/ip link delete "$TAP" 2>/dev/null || true
        fi
      '' + " %i";
    };
  };

  # virtiofsd (runs the microvm.nix-generated virtiofsd-run script)
  systemd.services."virtiofsd@" = {
    description = "virtiofsd for sandbox %i";
    requires = [ "tap-setup@%i.service" ];
    after = [ "tap-setup@%i.service" ];
    serviceConfig = {
      Type = "simple";
      KillMode = "control-group";
      TimeoutStopSec = 10;
      WorkingDirectory = "/opt/choiros/vms/state/%i";
      ExecStart = pkgs.writeShellScript "virtiofsd-start" ''
        set -euo pipefail
        INSTANCE="$1"
        STATE_DIR="/opt/choiros/vms/state/''${INSTANCE}"
        RUNNER_DIR="${vmRunnerLive}"

        # Clean stale sockets
        rm -f "''${STATE_DIR}"/*-virtiofs-*.sock

        cd "$STATE_DIR"
        exec "''${RUNNER_DIR}/bin/virtiofsd-run"
      '' + " %i";
    };
  };

  # cloud-hypervisor VM (cold boot or snapshot restore based on boot-mode file)
  systemd.services."cloud-hypervisor@" = {
    description = "Cloud Hypervisor VM for sandbox %i";
    requires = [ "virtiofsd@%i.service" ];
    after = [ "virtiofsd@%i.service" ];
    serviceConfig = {
      Type = "simple";
      KillMode = "control-group";
      TimeoutStopSec = 15;
      WorkingDirectory = "/opt/choiros/vms/state/%i";
      ExecStart = pkgs.writeShellScript "cloud-hypervisor-start" ''
        set -euo pipefail
        INSTANCE="$1"
        STATE_DIR="/opt/choiros/vms/state/''${INSTANCE}"
        RUNNER_DIR="${vmRunnerLive}"
        BOOT_MODE_FILE="''${STATE_DIR}/boot-mode"
        SNAPSHOT_DIR="''${STATE_DIR}/vm-snapshot"
        API_SOCK="''${STATE_DIR}/sandbox-''${INSTANCE}.sock"

        cd "$STATE_DIR"

        # Wait for virtiofsd sockets (max 30s)
        ELAPSED=0
        while (( ELAPSED < 30 )); do
          SOCK_COUNT=$(find "$STATE_DIR" -maxdepth 1 \( -name "*-virtiofs-nix-store.sock" -o -name "*-virtiofs-choiros-creds.sock" \) 2>/dev/null | wc -l)
          if (( SOCK_COUNT >= 2 )); then
            echo "virtiofsd sockets ready (''${SOCK_COUNT} found in ''${ELAPSED}s)"
            break
          fi
          sleep 1
          ELAPSED=$((ELAPSED + 1))
        done

        BOOT_MODE="cold"
        if [[ -f "$BOOT_MODE_FILE" ]]; then
          BOOT_MODE=$(cat "$BOOT_MODE_FILE")
        fi

        if [[ "$BOOT_MODE" == "restore" ]] && [[ -d "$SNAPSHOT_DIR" ]] && [[ -f "$SNAPSHOT_DIR/state.json" ]]; then
          echo "Restoring VM from snapshot"
          rm -f "$API_SOCK"
          exec ${pkgs.cloud-hypervisor}/bin/cloud-hypervisor \
            --restore "source_url=file://''${SNAPSHOT_DIR}" \
            --api-socket "$API_SOCK"
        else
          echo "Cold booting VM"
          exec "''${RUNNER_DIR}/bin/microvm-run"
        fi
      '' + " %i";
      # After VM starts from restore, resume vCPUs
      ExecStartPost = pkgs.writeShellScript "cloud-hypervisor-resume" ''
        set -euo pipefail
        INSTANCE="$1"
        STATE_DIR="/opt/choiros/vms/state/''${INSTANCE}"
        API_SOCK="''${STATE_DIR}/sandbox-''${INSTANCE}.sock"
        BOOT_MODE_FILE="''${STATE_DIR}/boot-mode"

        BOOT_MODE="cold"
        if [[ -f "$BOOT_MODE_FILE" ]]; then
          BOOT_MODE=$(cat "$BOOT_MODE_FILE")
        fi

        if [[ "$BOOT_MODE" == "restore" ]]; then
          # Wait for API socket
          for i in $(seq 1 15); do
            [[ -S "$API_SOCK" ]] && break
            sleep 1
          done
          if [[ -S "$API_SOCK" ]]; then
            sleep 1
            ${pkgs.curl}/bin/curl -s --max-time 5 --unix-socket "$API_SOCK" \
              -X PUT "http://localhost/api/v1/vm.resume" 2>/dev/null || true
            echo "VM resumed from snapshot"
          fi
        fi
      '' + " %i";
    };
  };

  # socat port forwarding (localhost:PORT -> VM_IP:PORT)
  systemd.services."socat-sandbox@" = {
    description = "Socat port forward for sandbox %i";
    requires = [ "cloud-hypervisor@%i.service" ];
    after = [ "cloud-hypervisor@%i.service" ];
    bindsTo = [ "cloud-hypervisor@%i.service" ];
    serviceConfig = {
      Type = "simple";
      KillMode = "control-group";
      ExecStartPre = pkgs.writeShellScript "socat-wait-health" ''
        set -euo pipefail
        INSTANCE="$1"
        # Derive VM IP from instance name
        case "$INSTANCE" in
          live*) VM_IP="10.0.0.10" ;;
          dev*)  VM_IP="10.0.0.11" ;;
          *)     VM_IP="10.0.0.10" ;;
        esac
        # Derive port from env or default
        PORT="''${CHOIR_SANDBOX_PORT:-8080}"
        case "$INSTANCE" in
          live*) PORT="8080" ;;
          dev*)  PORT="8081" ;;
        esac

        echo "Waiting for sandbox health at ''${VM_IP}:''${PORT}"
        MAX_WAIT=90
        ELAPSED=0
        while (( ELAPSED < MAX_WAIT )); do
          if ${pkgs.curl}/bin/curl -fsS --connect-timeout 2 "http://''${VM_IP}:''${PORT}/health" &>/dev/null; then
            echo "Sandbox healthy after ''${ELAPSED}s"
            exit 0
          fi
          sleep 3
          ELAPSED=$((ELAPSED + 3))
        done
        echo "WARNING: Sandbox not healthy after ''${MAX_WAIT}s, starting socat anyway"
      '' + " %i";
      ExecStart = pkgs.writeShellScript "socat-start" ''
        set -euo pipefail
        INSTANCE="$1"
        case "$INSTANCE" in
          live*) VM_IP="10.0.0.10"; PORT="8080" ;;
          dev*)  VM_IP="10.0.0.11"; PORT="8081" ;;
          *)     VM_IP="10.0.0.10"; PORT="8080" ;;
        esac

        echo "Starting socat 127.0.0.1:''${PORT} -> ''${VM_IP}:''${PORT}"
        exec ${pkgs.socat}/bin/socat \
          TCP-LISTEN:''${PORT},bind=127.0.0.1,reuseaddr,fork \
          TCP:''${VM_IP}:''${PORT}
      '' + " %i";
    };
  };

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
