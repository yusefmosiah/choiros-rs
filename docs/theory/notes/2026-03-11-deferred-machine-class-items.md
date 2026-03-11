# Deferred Machine Class Items

Date: 2026-03-11
Kind: Note
Status: Active
Priority: 2
Requires: [ADR-0014]

Items discovered during ADR-0014 Phase 6 implementation that are worth
doing but not blocking the current milestone (multi-class E2E stress tests).

## Automatic class migration on mismatch

When `ensure_running` detects the running VM's class differs from the user's
current preference, it should:

1. Stop the current VM (hard stop, not hibernate)
2. Invalidate the VM snapshot (snapshots are not portable across hypervisors)
3. Cold-boot with the new class

User data (data.img) is portable — it's a plain ext4 image symlinked from
the btrfs subvolume. No data migration needed.

Current behavior: class change takes effect only on next cold boot. Running
VMs keep their original class until stopped by idle timeout or manual action.

## Generation-aware snapshot invalidation

VM snapshots break when nix store paths change (nixos-rebuild switch). The
real trigger is a nix generation change, not a hypervisor restart. Current
code invalidates snapshots on every hypervisor restart, which is
over-aggressive.

Fix: read `/nix/var/nix/profiles/system` target at snapshot time, write it
to the snapshot dir. On restore, compare against current target. Only
invalidate if they differ.

This recovers fast-restore for VMs that survive a hypervisor restart within
the same nix generation (e.g. config-only changes, hypervisor binary update
without nixos-rebuild).

## data.img isolation security review

Per-user data lives in btrfs subvolumes at `/data/users/{user_id}/data.img`.
The symlink from the VM state dir points into this. Current protections:

- Host directories are root-owned
- VM state dirs are root-owned
- Guest VMs access data.img via virtio-blk (block device, not filesystem mount)
- No guest-to-host filesystem traversal path

Worth reviewing:
- Can a guest VM influence which data.img it gets? (No — host writes the
  symlink before VM boot, guest has no control over block device assignment)
- Are btrfs subvolume permissions sufficient? (Currently root-only)
- Should we add MAC labeling or mount namespace isolation?
- What happens if two VMs try to mount the same data.img concurrently?
  (ext4 corruption — enforce single-writer via the registry's per-user lock)

Priority: before public multitenant. Not blocking current E2E testing since
all test users are ephemeral.
