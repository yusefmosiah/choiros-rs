{ config, lib, pkgs, ... }:
let
  workspaceRoot =
    let env = builtins.getEnv "CHOIR_WORKSPACE_ROOT";
    in if env != "" then env else "/Users/wiz/choiros-rs";

  guestStateRoot =
    let env = builtins.getEnv "CHOIR_VFKIT_GUEST_STATE_ROOT";
    in if env != "" then env else "/Users/wiz/.local/share/choiros/vfkit/guest";

  macAddress =
    let env = builtins.getEnv "CHOIR_VFKIT_MAC_ADDR";
    in if env != "" then env else "56:46:4b:49:54:01";

  guestName =
    let env = builtins.getEnv "CHOIR_VFKIT_GUEST_NAME";
    in if env != "" then env else "choiros-vfkit-user";

  hostSystem =
    let env = builtins.getEnv "CHOIR_VFKIT_HOST_SYSTEM";
    in if env != "" then env else "aarch64-darwin";

  vmHostPackages = import pkgs.path {
    system = hostSystem;
  };

  guestCtl = pkgs.writeShellApplication {
    name = "choir-vfkit-guest-runtime-ctl";
    runtimeInputs = with pkgs; [
      bash
      cargo
      coreutils
      curl
      gcc
      gnugrep
      gnused
      nixos-container
      openssl
      openssl.dev
      pkg-config
      protobuf
      rustc
      sqlite
    ];
    text = builtins.readFile ../../scripts/ops/vfkit-guest-runtime-ctl.sh;
  };
in
{
  networking.hostName = guestName;

  services.openssh = {
    enable = true;
    openFirewall = true;
    settings = {
      PermitRootLogin = "yes";
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
    };
  };

  users.users.root.openssh.authorizedKeys.keys =
    let key = builtins.getEnv "CHOIR_VFKIT_SSH_PUBKEY";
    in lib.optional (key != "") key;

  boot.enableContainers = true;
  virtualisation.containers.enable = true;

  microvm = {
    hypervisor = "vfkit";
    vmHostPackages = vmHostPackages;
    storeOnDisk = false;
    writableStoreOverlay = "/nix/.rw-store";
    vcpu = 4;
    mem = 4096;

    interfaces = [
      {
        type = "user";
        id = "vmnet0";
        mac = macAddress;
      }
    ];

    volumes = [
      {
        image = "nix-store-overlay.img";
        label = "nix-store";
        mountPoint = config.microvm.writableStoreOverlay;
        size = 16384;
      }
    ];

    shares = [
      {
        proto = "virtiofs";
        tag = "ro-store";
        source = "/nix/store";
        mountPoint = "/nix/.ro-store";
      }
      {
        proto = "virtiofs";
        tag = "workspace";
        source = workspaceRoot;
        mountPoint = "/workspace";
      }
      {
        proto = "virtiofs";
        tag = "guest-state";
        source = guestStateRoot;
        mountPoint = "/var/lib/choiros";
      }
    ];
  };

  nix.optimise.automatic = false;
  nix.settings.auto-optimise-store = false;
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  nix.settings.max-jobs = 2;
  nix.settings.cores = 2;

  # Default guest limits are often too low for first-time nixos-container builds.
  systemd.settings.Manager.DefaultLimitNOFILE = "262144";
  systemd.services.nix-daemon.serviceConfig.LimitNOFILE = 262144;

  environment.systemPackages = with pkgs; [
    bash
    btop
    cargo
    coreutils
    curl
    gcc
    git
    gnugrep
    gnused
    openssl
    openssl.dev
    pkg-config
    procps
    protobuf
    ripgrep
    rustc
    sqlite
    guestCtl
  ];

  system.stateVersion = "25.11";
}
