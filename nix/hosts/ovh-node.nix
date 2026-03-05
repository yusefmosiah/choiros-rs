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

  # Firewall
  networking.firewall = {
    enable = true;
    allowedTCPPorts = [
      22    # SSH
      80    # HTTP
      443   # HTTPS
      9090  # Hypervisor ingress
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

  # Workspace directory
  systemd.tmpfiles.rules = [
    "d /opt/choiros 0755 root root -"
    "d /opt/choiros/workspace 0755 root root -"
    "d /run/choiros/credentials/platform 0700 root root -"
  ];

  # Timezone
  time.timeZone = "UTC";

  system.stateVersion = "25.11";
}
