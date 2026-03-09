#!/bin/bash
# Node A rescue mode investigation script
# Run this after SSH'ing into rescue mode:
#   ssh root@51.81.93.94  (with rescue password from email)
#   bash < rescue-investigate-a.sh
# Or paste sections manually.

set -euo pipefail

echo "=== 1. Disk Layout ==="
lsblk -f
echo ""
blkid
echo ""

echo "=== 2. RAID Status ==="
cat /proc/mdstat 2>/dev/null || echo "No mdstat (md arrays may need assembly)"
echo ""

echo "=== 3. Assemble RAID arrays ==="
# Scan and assemble any md arrays
mdadm --assemble --scan 2>/dev/null || echo "mdadm assemble failed or already assembled"
cat /proc/mdstat 2>/dev/null || true
echo ""

echo "=== 4. Find and mount root filesystem ==="
# Try to find NixOS root — could be on md device or direct partition
# Check for btrfs first (Node B layout), then ext4 (legacy Node A layout)
for dev in /dev/md126 /dev/md127 /dev/md0 /dev/md1; do
  if [ -b "$dev" ]; then
    FS=$(blkid -o value -s TYPE "$dev" 2>/dev/null || echo "unknown")
    echo "  $dev: filesystem=$FS"
  fi
done
echo ""

echo "=== 5. Mount root and inspect ==="
mkdir -p /mnt/nixos

# Try btrfs mount first (if Node A was converted)
MOUNTED=0
for dev in /dev/md126 /dev/md127 /dev/md0 /dev/md1; do
  [ -b "$dev" ] || continue
  FS=$(blkid -o value -s TYPE "$dev" 2>/dev/null || echo "")
  if [ "$FS" = "btrfs" ]; then
    echo "Trying btrfs mount: $dev subvol=@"
    if mount -t btrfs -o subvol=@ "$dev" /mnt/nixos 2>/dev/null; then
      echo "  SUCCESS: btrfs root mounted from $dev"
      MOUNTED=1
      break
    fi
  elif [ "$FS" = "ext4" ]; then
    echo "Trying ext4 mount: $dev"
    if mount -t ext4 "$dev" /mnt/nixos 2>/dev/null; then
      echo "  SUCCESS: ext4 root mounted from $dev"
      MOUNTED=1
      break
    fi
  fi
done

if [ "$MOUNTED" = "0" ]; then
  echo "ERROR: Could not auto-mount root. Manual investigation needed."
  echo "Try: mount /dev/mdXXX /mnt/nixos"
  exit 1
fi

echo ""
echo "=== 6. NixOS Generations ==="
ls -la /mnt/nixos/nix/var/nix/profiles/ 2>/dev/null || echo "No profiles dir"
echo ""

# Show system generations
for gen in /mnt/nixos/nix/var/nix/profiles/system-*-link; do
  [ -e "$gen" ] || continue
  TARGET=$(readlink -f "$gen" 2>/dev/null || echo "broken")
  echo "  $(basename $gen) -> $TARGET"
done
echo ""

# Current default
echo "Current default:"
readlink -f /mnt/nixos/nix/var/nix/profiles/system 2>/dev/null || echo "  (broken)"
echo ""

echo "=== 7. GRUB Config ==="
# Check what GRUB is set to boot
if [ -f /mnt/nixos/boot/grub/grub.cfg ]; then
  echo "GRUB entries:"
  grep -E "^menuentry|set default" /mnt/nixos/boot/grub/grub.cfg | head -20
else
  echo "No grub.cfg found at /mnt/nixos/boot/grub/grub.cfg"
  # Check EFI
  ls /mnt/nixos/boot/efi/EFI/ 2>/dev/null || true
fi

# Also check /boot on separate partition
echo ""
echo "=== 8. Separate /boot partition ==="
for dev in /dev/md126 /dev/md127 /dev/md0 /dev/md1; do
  [ -b "$dev" ] || continue
  FS=$(blkid -o value -s TYPE "$dev" 2>/dev/null || echo "")
  if [ "$FS" = "ext4" ]; then
    LABEL=$(blkid -o value -s LABEL "$dev" 2>/dev/null || echo "")
    if [ -z "$LABEL" ]; then
      # Might be boot partition — try mounting
      mkdir -p /mnt/boot-check
      if mount "$dev" /mnt/boot-check 2>/dev/null; then
        if [ -d /mnt/boot-check/grub ]; then
          echo "Found /boot on $dev"
          echo "GRUB entries:"
          grep -E "^menuentry|set default" /mnt/boot-check/grub/grub.cfg 2>/dev/null | head -20
          echo ""
          echo "GRUB default:"
          cat /mnt/boot-check/grub/grubenv 2>/dev/null | head -5
        fi
        umount /mnt/boot-check
      fi
      rmdir /mnt/boot-check 2>/dev/null || true
    fi
  fi
done

echo ""
echo "=== 9. Network Config in Current Generation ==="
CURRENT=$(readlink -f /mnt/nixos/nix/var/nix/profiles/system 2>/dev/null || echo "")
if [ -n "$CURRENT" ] && [ -d "/mnt/nixos${CURRENT#/mnt/nixos}" ]; then
  # The system profile is a store path
  STORE_PATH="$CURRENT"
  echo "Current system: $STORE_PATH"

  # Check network-related service files
  echo ""
  echo "Network services:"
  find "/mnt/nixos/nix/store/" -maxdepth 3 -name "network-setup.service" -newer /mnt/nixos/nix/var/nix/profiles/system 2>/dev/null | head -5

  # Check systemd network files
  echo ""
  echo "networkd configs:"
  ls "/mnt/nixos/etc/systemd/network/" 2>/dev/null || echo "  (none or broken symlink)"
fi

echo ""
echo "=== 10. Recent Journal (if accessible) ==="
# Try to read the journal from the installed system
if [ -d /mnt/nixos/var/log/journal ]; then
  # Use journalctl against the mounted root
  journalctl -D /mnt/nixos/var/log/journal --no-pager -n 50 -p err 2>/dev/null || echo "Could not read journal"
else
  echo "No journal directory found"
fi

echo ""
echo "=== DONE ==="
echo "If root is mounted at /mnt/nixos, you can:"
echo "  - Roll back: ln -sfn system-N-link /mnt/nixos/nix/var/nix/profiles/system"
echo "  - Check generations: ls -la /mnt/nixos/nix/var/nix/profiles/system-*"
echo "  - Chroot: mount -t proc proc /mnt/nixos/proc && mount --rbind /dev /mnt/nixos/dev && mount --rbind /sys /mnt/nixos/sys && chroot /mnt/nixos"
