# Handoff: Push-to-Switch Deploy Streamlining

Date: 2026-02-21

## Narrative Summary (1-minute read)

We now have sops-nix platform secrets active on grind and prod, and deploys are green.
Current CI deploy still builds sandbox/hypervisor/desktop artifacts and installs binaries
into `/opt/choiros/bin/*` before activation. We moved activation to `nixos-rebuild switch`
in CI so host convergence and secret rendering happen in one step, but binary delivery is
still a staged bridge.

Target steady state remains: push to `main` -> host pulls commit -> `nixos-rebuild switch`
with no separate binary install lane.

## What Changed

1. Verified latest `main` workflow run succeeded (`Nix CI/CD`, run `22266831468`).
2. Updated deploy workflow to activate via `nixos-rebuild switch` instead of manual
   `systemctl restart ...`.
3. Kept existing binary build/install steps for now to avoid release behavior regression
   while service `ExecStart` still points at `/opt/choiros/bin/*`.

## What To Do Next

1. Move host service binaries from `/opt/choiros/bin/*` to declarative Nix derivations in
   host config (or flake `nixosConfigurations`) so switch alone advances runtime version.
2. Move `/etc/nixos/configuration.nix` into repo flake outputs:
   - `nixosConfigurations.choiros-prod`
   - `nixosConfigurations.choiros-grind`
3. Update CI deploy script to:
   - checkout target SHA
   - run `nixos-rebuild switch --flake /opt/choiros/deploy-repo#choiros-prod`
   - run health checks
4. Remove legacy binary install staging from workflow once service paths are Nix-managed.

## Current Constraints

- Host config still references `/opt/choiros/bin/hypervisor` and `/opt/choiros/bin/sandbox`.
- Because of this, push+switch without binary staging does not yet guarantee new app bits.
- sops-nix is active and decrypts required platform keys at activation time.

## Verification Commands

```bash
gh run list --workflow "Nix CI/CD" --limit 5
```

```bash
aws ssm send-command ... "systemctl show hypervisor --property=EnvironmentFiles"
```

```bash
aws ssm send-command ... "systemctl is-active hypervisor container@sandbox-live container@sandbox-dev"
```
