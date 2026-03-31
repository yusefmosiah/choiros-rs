# Local NixOS Builder VM Setup Handoff - 2026-02-28
Date: 2026-02-28
Kind: Snapshot
Status: Active
Requires: []

## Snapshot
- Provisioned a local UTM Apple-virtualized `aarch64` NixOS builder VM on macOS Apple Silicon.
  - VM profile used: 4 vCPU, 6 GB RAM, 70 GB disk, Shared (NAT), OpenGL off.
- Installed NixOS from `latest-nixos-minimal-aarch64-linux.iso`.
- Configured builder guest with:
  - `services.openssh.enable = true`
  - key-only auth (no password auth)
  - `remotebuild` user for remote builds
  - `builderadmin` maintenance user
  - `boot.loader.systemd-boot.enable = true`
  - `boot.loader.efi.canTouchEfiVariables = true`
- Configured host Nix daemon remote builder wiring:
  - `/etc/nix/machines`:
    - `ssh-ng://remotebuild@192.168.65.2 aarch64-linux /var/root/.ssh/nixbuilder_ed25519 4 1 big-parallel -`
  - `/etc/nix/nix.custom.conf` includes:
    - `builders = @/etc/nix/machines`
    - `builders-use-substitutes = true`

## Validated
- SSH to builder VM from host root key works:
  - `sudo ssh -i /var/root/.ssh/nixbuilder_ed25519 remotebuild@192.168.65.2 'whoami && uname -m'`
  - Output: `remotebuild` and `aarch64`
- Nix remote-builder proof succeeded:
  - `sudo -H nix build --max-jobs 0 -L --impure ...`
  - Output file contained `aarch64-linux`
- `sandbox` flake build resumed correctly when run without forcing all jobs remote:
  - `nix build ./sandbox#sandbox -L`

## Important Notes
- `utmctl` can fail in non-interactive/non-logged-in contexts (`OSStatus -1743`), so IP recovery used macOS DHCP leases when needed.
- Disk auto-detection originally selected read-only ISO disk (`/dev/sda`) in installer.
  - Installer script was hardened to pick writable disks only.
- Bootloader assertion failure during first install attempt was fixed by enabling systemd-boot + EFI in guest config.
- `--max-jobs 0` forces remote-only scheduling and fails for host-native `aarch64-darwin` derivations if only `aarch64-linux` builders are registered.

## Resume Checklist
1. Let current `nix build ./sandbox#sandbox -L` finish.
2. Continue the original parent task (now unblocked for Linux derivation capability locally).
3. Optional follow-up hardening:
   - persist guest config in a dedicated NixOS module/repo path
   - add fallback/emulated `x86_64-linux` on guest only if needed
   - add native remote `x86_64-linux` builder for performance-critical x86 workloads
