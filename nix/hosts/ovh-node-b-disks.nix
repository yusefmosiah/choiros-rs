# Node B disk/filesystem configuration (UUID-based, stable across reboots)
# Node B was partitioned by disko — these UUIDs match the actual disks.
{ ... }:
{
  fileSystems."/" = {
    device = "/dev/disk/by-uuid/3b71f2a6-7820-47a1-ba22-c44c65e31ea1";
    fsType = "btrfs";
    options = [ "subvol=@" "compress=zstd" "noatime" ];
  };

  fileSystems."/data" = {
    device = "/dev/disk/by-uuid/3b71f2a6-7820-47a1-ba22-c44c65e31ea1";
    fsType = "btrfs";
    options = [ "subvol=@data" "compress=zstd" "noatime" ];
  };

  fileSystems."/boot" = {
    device = "/dev/disk/by-uuid/a20c8956-eb6c-4a04-ad90-c384f6089a5e";
    fsType = "ext4";
  };

  fileSystems."/boot/efi" = {
    device = "/dev/disk/by-uuid/F9E9-6E35";
    fsType = "vfat";
    options = [ "umask=0077" ];
  };

  swapDevices = [ ];
}
