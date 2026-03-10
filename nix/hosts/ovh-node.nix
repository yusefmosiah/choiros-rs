# NixOS host configuration for OVH SYS-1 bare metal (x86_64-linux)
# Intel Xeon-E 2136 / 32GB RAM / 2x NVMe RAID1 / UEFI
{ config, lib, pkgs, choirosPackages, vmRunnerLive, ... }:
{
  # ADR-0018: virtiofsd overlay removed — no more virtiofs shares.
  nixpkgs.overlays = [];
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

  # DHCP server for per-user sandbox VMs on br-choiros (ADR-0014)
  # MAC→IP reservations written by hypervisor to /var/lib/dnsmasq/choiros-hosts
  # and picked up via dhcp-hostsfile. SIGHUP reloads without restart.
  services.dnsmasq = {
    enable = true;
    resolveLocalQueries = false; # Don't override /etc/resolv.conf
    settings = {
      interface = "br-choiros";
      bind-interfaces = true;
      dhcp-range = "10.0.0.100,10.0.0.254,255.255.255.0,12h";
      dhcp-option = [ "option:router,10.0.0.1" "option:dns-server,1.1.1.1,8.8.8.8" ];
      dhcp-hostsfile = "/var/lib/dnsmasq/choiros-hosts";
      # Don't provide DNS — just DHCP
      port = 0;
    };
  };
  systemd.tmpfiles.settings."10-dnsmasq" = {
    "/var/lib/dnsmasq".d = { mode = "0755"; user = "root"; group = "root"; };
    "/var/lib/dnsmasq/choiros-hosts".f = { mode = "0644"; user = "root"; group = "root"; };
  };

  # KSM (Kernel Same-page Merging) — deduplicates identical memory pages across VMs
  # KSM control lives in /sys/kernel/mm/ksm/ (not /proc/sys/), so we use a tmpfile rule.
  # THP must be disabled: KSM only works on 4KB pages, not 2MB hugepages.
  # Cloud-hypervisor calls MADV_HUGEPAGE which blocks KSM merging.
  systemd.tmpfiles.settings."10-ksm" = {
    "/sys/kernel/mm/ksm/run".w = { argument = "1"; };
    "/sys/kernel/mm/ksm/sleep_millisecs".w = { argument = "200"; };
    "/sys/kernel/mm/ksm/pages_to_scan".w = { argument = "1000"; };
    "/sys/kernel/mm/transparent_hugepage/enabled".w = { argument = "never"; };
  };

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
    # virtiofsd removed — ADR-0018 replaced virtiofs with virtio-blk
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
        # Clear stale dnsmasq hosts — hypervisor will re-populate on each ensure()
        : > /var/lib/dnsmasq/choiros-hosts 2>/dev/null || true
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
          echo "TAP $TAP already exists, ensuring bridge membership"
        else
          echo "Creating TAP $TAP"
          ${pkgs.iproute2}/bin/ip tuntap add dev "$TAP" mode tap vnet_hdr multi_queue
        fi
        # Always ensure TAP is on bridge and up (bridge may reset during rebuild)
        ${pkgs.iproute2}/bin/ip link set "$TAP" master br-choiros 2>/dev/null || true
        ${pkgs.iproute2}/bin/ip link set "$TAP" up
        echo "TAP $TAP on br-choiros, up"
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

  # ADR-0018: virtiofsd@ service REMOVED — no more virtiofs shares.
  # Nix store is served via virtio-pmem (erofs, DAX-capable).

  # cloud-hypervisor VM (cold boot or snapshot restore based on boot-mode file)
  systemd.services."cloud-hypervisor@" = {
    description = "Cloud Hypervisor VM for sandbox %i";
    requires = [ "tap-setup@%i.service" ];
    after = [ "tap-setup@%i.service" ];
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

        # Read per-instance config (ADR-0014: written by hypervisor Rust code)
        VM_MAC="52:54:00:00:00:0a"  # default for legacy "live"
        if [[ -f "''${STATE_DIR}/vm-mac" ]]; then
          VM_MAC=$(cat "''${STATE_DIR}/vm-mac")
        fi
        TAP="tap-''${INSTANCE}"
        TAP="''${TAP:0:15}"  # IFNAMSIZ limit

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
          echo "Cold booting VM (MAC=''${VM_MAC} TAP=''${TAP})"

          # ADR-0018: Read gateway token for kernel cmdline injection
          GATEWAY_TOKEN=""
          if [[ -f "''${STATE_DIR}/gateway-token" ]]; then
            GATEWAY_TOKEN=$(cat "''${STATE_DIR}/gateway-token")
          fi

          # ADR-0018: Build cloud-hypervisor command from microvm-run, replacing:
          # - MAC/TAP/socket names (ADR-0014 per-user)
          # - --memory shared=on → shared=off (enables KSM, no virtiofs needed)
          # - Inject gateway token into kernel cmdline
          # - Phase 7: Convert erofs --disk to --pmem (eliminates guest page cache)
          ${pkgs.gnused}/bin/sed \
            -e "s/mac=52:54:00:00:00:0a/mac=''${VM_MAC}/" \
            -e "s/tap=tap-live/tap=''${TAP}/" \
            -e "s/sandbox-live\.sock/sandbox-''${INSTANCE}.sock/g" \
            -e "s/shared=on/shared=off/" \
            "''${RUNNER_DIR}/bin/microvm-run" > "''${STATE_DIR}/.microvm-run"

          # Phase 7: Convert erofs store disk from --disk to --pmem.
          # virtio-pmem maps the file directly into guest physical memory (DAX),
          # eliminating the ~100MB guest page cache overhead per VM.
          # The microvm-run has: --disk 'erofs-entry' 'data-entry'
          # We need: --disk 'data-entry' --pmem file=<padded-erofs>,discard_writes=on
          EROFS_PATH=$(${pkgs.gnugrep}/bin/grep -oP "path=/nix/store/[^,]+\.erofs" "''${STATE_DIR}/.microvm-run" | head -1 | cut -d= -f2)
          if [[ -n "$EROFS_PATH" ]]; then
            # Replace the entire --disk section: remove erofs entry, keep data entry
            ${pkgs.gnused}/bin/sed -i \
              "s|--disk 'num_queues=[0-9]*,path=/nix/store/[^']*\.erofs,readonly=on' |--disk |" \
              "''${STATE_DIR}/.microvm-run"

            # Pad erofs to 2MiB alignment (required by cloud-hypervisor --pmem).
            # Use ONE shared copy at /opt/choiros/vms/store-disk-padded.erofs
            # (not per-VM — all VMs use the same read-only image).
            PADDED_EROFS="/opt/choiros/vms/store-disk-padded.erofs"
            EROFS_SIZE=$(stat -c%s "$EROFS_PATH")
            ALIGN=$((2 * 1024 * 1024))
            ALIGNED_SIZE=$(( ((EROFS_SIZE + ALIGN - 1) / ALIGN) * ALIGN ))
            if [[ ! -f "$PADDED_EROFS" ]] || [[ $(stat -c%s "$PADDED_EROFS") -ne $ALIGNED_SIZE ]]; then
              cp "$EROFS_PATH" "$PADDED_EROFS"
              truncate -s "$ALIGNED_SIZE" "$PADDED_EROFS"
            fi

            # Add --pmem before --api-socket
            ${pkgs.gnused}/bin/sed -i "s|--api-socket|--pmem file=''${PADDED_EROFS},discard_writes=on --api-socket|" "''${STATE_DIR}/.microvm-run"
          fi

          # Inject gateway token into kernel --cmdline (append before closing quote)
          if [[ -n "$GATEWAY_TOKEN" ]]; then
            ${pkgs.gnused}/bin/sed -i \
              "s|' --seccomp| choir.gateway_token=''${GATEWAY_TOKEN}' --seccomp|" \
              "''${STATE_DIR}/.microvm-run"
          fi

          chmod +x "''${STATE_DIR}/.microvm-run"
          exec "''${STATE_DIR}/.microvm-run"
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

  # socat port forwarding (localhost:HOST_PORT -> VM_IP:8080)
  # ADR-0014: reads vm-ip and host-port from state dir config files
  # written by SystemdLifecycle.ensure() in Rust.
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
        STATE_DIR="/opt/choiros/vms/state/''${INSTANCE}"

        # Read per-instance config (written by hypervisor Rust code)
        if [[ -f "''${STATE_DIR}/vm-ip" ]]; then
          VM_IP=$(cat "''${STATE_DIR}/vm-ip")
        else
          # Fallback for legacy instances
          case "$INSTANCE" in
            live*) VM_IP="10.0.0.100" ;;
            dev*)  VM_IP="10.0.0.101" ;;
            *)     VM_IP="10.0.0.100" ;;
          esac
        fi

        echo "Waiting for sandbox health at ''${VM_IP}:8080"
        MAX_WAIT=90
        ELAPSED=0
        while (( ELAPSED < MAX_WAIT )); do
          if ${pkgs.curl}/bin/curl -fsS --connect-timeout 2 "http://''${VM_IP}:8080/health" &>/dev/null; then
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
        STATE_DIR="/opt/choiros/vms/state/''${INSTANCE}"

        # Read per-instance config
        if [[ -f "''${STATE_DIR}/vm-ip" ]] && [[ -f "''${STATE_DIR}/host-port" ]]; then
          VM_IP=$(cat "''${STATE_DIR}/vm-ip")
          HOST_PORT=$(cat "''${STATE_DIR}/host-port")
        else
          # Fallback for legacy instances
          case "$INSTANCE" in
            live*) VM_IP="10.0.0.100"; HOST_PORT="8080" ;;
            dev*)  VM_IP="10.0.0.101"; HOST_PORT="8081" ;;
            *)     VM_IP="10.0.0.100"; HOST_PORT="8080" ;;
          esac
        fi

        echo "Starting socat 127.0.0.1:''${HOST_PORT} -> ''${VM_IP}:8080"
        exec ${pkgs.socat}/bin/socat \
          TCP-LISTEN:''${HOST_PORT},bind=127.0.0.1,reuseaddr,fork \
          TCP:''${VM_IP}:8080
      '' + " %i";
    };
  };

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
