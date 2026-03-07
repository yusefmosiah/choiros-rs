# Disko disk configuration for OVH SYS-1 bare metal
# 2x NVMe drives in RAID1 with UEFI boot
# Root on btrfs (md RAID1) with subvolumes: @ (root), @data (per-user storage)
{
  disko.devices = {
    disk = {
      nvme0 = {
        type = "disk";
        device = "/dev/nvme0n1";
        content = {
          type = "gpt";
          partitions = {
            ESP = {
              size = "512M";
              type = "EF00";
              content = {
                type = "filesystem";
                format = "vfat";
                mountpoint = "/boot/efi";
                mountOptions = [ "umask=0077" ];
              };
            };
            mdraid-boot = {
              size = "1G";
              content = {
                type = "mdraid";
                name = "boot";
              };
            };
            mdraid-root = {
              size = "100%";
              content = {
                type = "mdraid";
                name = "root";
              };
            };
          };
        };
      };
      nvme1 = {
        type = "disk";
        device = "/dev/nvme1n1";
        content = {
          type = "gpt";
          partitions = {
            mdraid-boot = {
              size = "1G";
              content = {
                type = "mdraid";
                name = "boot";
              };
            };
            mdraid-root = {
              size = "100%";
              content = {
                type = "mdraid";
                name = "root";
              };
            };
          };
        };
      };
    };

    mdadm = {
      boot = {
        type = "mdadm";
        level = 1;
        metadata = "1.0";
        content = {
          type = "filesystem";
          format = "ext4";
          mountpoint = "/boot";
        };
      };
      root = {
        type = "mdadm";
        level = 1;
        content = {
          type = "btrfs";
          extraArgs = [ "-f" ];
          subvolumes = {
            "@" = {
              mountpoint = "/";
              mountOptions = [ "compress=zstd" "noatime" ];
            };
            "@data" = {
              mountpoint = "/data";
              mountOptions = [ "compress=zstd" "noatime" ];
            };
          };
        };
      };
    };
  };
}
