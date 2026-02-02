# NixOS on AWS EC2 Research

Date: 2026-02-01
Research Focus: NixOS deployment on AWS EC2 instances

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [NixOS AMI Options](#nixos-ami-options)
3. [Infrastructure as Code](#infrastructure-as-code)
4. [Deployment Tools](#deployment-tools)
5. [Secrets Management](#secrets-management)
6. [System Updates and Rollbacks](#system-updates-and-rollbacks)
7. [Monitoring and Observability](#monitoring-and-observability)
8. [Operational Concerns](#operational-concerns)
9. [Configuration Examples](#configuration-examples)
10. [Deployment Patterns](#deployment-patterns)

## Executive Summary

[LEARNING] ARCHITECTURE: NixOS offers a purely functional approach to system configuration. On EC2, this means AMI-based deployment with declarative configuration through Nix expressions.

**Key Findings:**
- Official NixOS AMIs are available per AWS region from download page
- nixos-rebuild build-image replaces deprecated nixos-generators for image building
- NixOps is in low-maintenance mode; deploy-rs is the recommended alternative for Nix flakes
- sops-nix and agenix provide secrets management with age encryption
- System rollbacks are built-in via GRUB boot menu
- Monitoring supported through NixOS modules (Prometheus, logging)

**Recommended Stack:**
- Image building: `nixos-rebuild build-image`
- Deployment: `deploy-rs` with Nix flakes
- Secrets: `sops-nix` or `agenix`
- Updates: `nixos-rebuild switch` with automatic rollback safety

## NixOS AMI Options

### Official AMIs

NixOS provides official AMIs for Amazon EC2 on the download page:
- URL: https://nixos.org/download.html#nixos-amazon
- AMIs are region-specific (one per AWS region)
- Updated with each NixOS release
- Recommended for production use as they're tested and maintained

### Custom AMI Building

[LEARNING] DEPRECATED: nixos-generators has been archived (Jan 30, 2026) and replaced by `nixos-rebuild build-image` in NixOS 25.05.

**Building Custom AMIs with nixos-rebuild build-image:**

```bash
# Build an Amazon EC2 image
nixos-rebuild build-image --image-variant amazon

# Or build from a specific flake
nixos-rebuild build-image --image-variant amazon --flake .#myhost

# For flakes, the image is available as:
# config.system.build.images.amazon
```

**Configuration Example for EC2:**

```nix
{ config, pkgs, ... }: {
  imports = [ ./hardware-configuration.nix ];

  # Boot configuration
  boot.loader.grub.device = "/dev/xvda";  # For EC2, typically xvda
  
  # EC2-specific networking (DHCP by default)
  networking.useDHCP = true;

  # Enable SSH for access
  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "prohibit-password";  # or "yes" for debugging
      PasswordAuthentication = false;
    };
  };

  # AWS-specific: cloud-init or user-data processing may be needed
  # This depends on your specific use case
}
```

**AMI Lifecycle:**
1. Build image locally with `nixos-rebuild build-image`
2. Upload AMI to AWS (requires AWS CLI tools)
3. Register as new AMI
4. Launch instances from AMI
5. Update configuration with `nixos-rebuild switch`

**Supported Image Variants:**
- `amazon` - Amazon EC2 image
- `iso` - Bootable ISO
- `qcow` - QEMU/KVM image
- `raw` - Raw disk image
- `kexec` - Kexec bundle for "kexec jump"

See the [nixos-generators compatibility table](https://github.com/nix-community/nixos-generators) for more formats.

## Infrastructure as Code

NixOS is fundamentally designed for infrastructure as code through its declarative configuration model.

### Nix Flakes

Nix flakes provide reproducible and composable configurations:

```nix
{
  description = "NixOS on EC2 Deployment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    # Secret management
    sops-nix.url = "github:Mic92/sops-nix";
    # Deployment tool
    deploy-rs.url = "github:serokell/deploy-rs";
  };

  outputs = { self, nixpkgs, sops-nix, deploy-rs }: {
    nixosConfigurations.ec2-instance = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        sops-nix.nixosModules.sops
      ];
    };

    deploy.nodes.ec2-instance = {
      hostname = "nixos-ec2";
      profiles.system = {
        user = "root";
        path = deploy-rs.lib.x86_64-linux.activate.nixos 
          self.nixosConfigurations.ec2-instance;
      };
    };
  };
}
```

### Configuration Modularity

NixOS configurations are modular:

```nix
# Base configuration shared across instances
{ config, ... }: {
  # Common services
  services.openssh.enable = true;
  services.fail2ban.enable = true;
  
  # Common packages
  environment.systemPackages = with pkgs; [
    pkgs.vim
    pkgs.git
    pkgs.htop
  ];
}

# EC2-specific configuration
{ config, ... }: {
  # EC2 instance type-specific settings
  boot.kernelParams = [ "console=ttyS0" ];  # Serial console
  networking.useDHCP = true;
}
```

## Deployment Tools

### NixOps (Not Recommended)

[LEARNING] NixOps is in low-maintenance mode and not recommended for new projects.

- Multi-cloud support: AWS, Hetzner, GCE, VirtualBox
- Python-based tool with Nix expression backend
- Supports declarative network and resource specifications
- Experimental rewrite: https://github.com/nixops4/nixops4

### deploy-rs (Recommended)

[LEARNING] deploy-rs is an actively maintained simple multi-profile Nix-flake deploy tool.

**Features:**
- Multi-profile support (deploy multiple profiles to one node)
- Magic rollback (prevents breaking connectivity changes)
- Supports NixOS, home-manager, and custom profiles
- SSH-based deployment
- Works with Nix flakes

**Example Configuration:**

```nix
{
  inputs.deploy-rs.url = "github:serokell/deploy-rs";

  outputs = { self, nixpkgs, deploy-rs }: {
    deploy.nodes.ec2-prod = {
      hostname = "nixos-prod.example.com";
      sshUser = "deploy";
      profiles.system = {
        user = "root";
        path = deploy-rs.lib.x86_64-linux.activate.nixos 
          self.nixosConfigurations.nixos-prod;
      };
      # Order of profile deployment
      profilesOrder = [ "system" ];
    };
  };
}
```

**Deploy Commands:**

```bash
# Deploy all profiles
deploy .#ec2-prod

# Deploy specific profile
deploy .#ec2-prod.system

# Try deployment without applying (dry run)
deploy .#ec2-prod --build-only

# Deploy multiple targets
deploy --targets .#host1.system .#host2.web
```

### Manual Deployment with nixos-rebuild

For simple deployments, manual deployment is viable:

```bash
# Build configuration on remote host
nixos-rebuild build --flake .#myhost --target-host ec2-instance-ip

# Switch to new configuration
nixos-rebuild switch --flake .#myhost --target-host ec2-instance-ip --install-bootloader

# Test configuration before switching
nixos-rebuild test --flake .#myhost --target-host ec2-instance-ip
```

## Secrets Management

Secrets management is critical for EC2 deployments. Two primary tools: sops-nix and agenix.

### sops-nix

[LEARNING] sops-nix provides atomic, declarative, and reproducible secret provisioning based on sops.

**Features:**
- Supports GPG and age encryption
- AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault support via sops
- Version control friendly (encrypted files in git)
- CI compatible (secrets in Nix store)
- Supports multiple file formats: YAML, JSON, INI, dotenv, binary
- Atomic upgrades and rollback support

**Configuration Example:**

```nix
{
  inputs.sops-nix.url = "github:Mic92/sops-nix";

  outputs = { self, nixpkgs, sops-nix }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        ./configuration.nix
        sops-nix.nixosModules.sops
      ];
    };
  };
}
```

**Secret Definition:**

```nix
# configuration.nix
{ config, ... }: {
  sops.defaultSopsFile = ./secrets/secrets.yaml;
  
  sops.age.keyFile = "/var/lib/sops-nix/key.txt";  # or use SSH host keys
  sops.age.generateKey = true;  # Generate key if doesn't exist
  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];

  # Define secrets
  sops.secrets.mysql-password = {};
  sops.secrets.aws-access-key = {
    mode = "0440";
    owner = "mysql";
    group = "mysql";
  };

  # Secret templates (inject secrets into config files)
  sops.templates."my-config.toml".content = ''
    password = "${config.sops.placeholder.mysql-password}"
  '';

  # Restart services when secrets change
  sops.secrets.mysql-password.restartUnits = [ "mysql.service" ];
}
```

**Using sops-nix:**

```bash
# Edit secret (opens in EDITOR)
sops secrets/secrets.yaml

# Re-encrypt secrets (e.g., after adding new recipient)
sops --rekey
```

### agenix

[LEARNING] agenix uses age encryption with SSH keys. Simple and small code base.

**Features:**
- Uses SSH keys (public keys from hosts/users)
- Secrets stored in Nix store
- No GPG dependency
- Supports armored output (Base64) for readable diffs
- Simple CLI: `agenix -e file.age`

**Configuration Example:**

```nix
{
  inputs.agenix.url = "github:ryantm/agenix";

  outputs = { self, nixpkgs, agenix }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        ./configuration.nix
        agenix.nixosModules.default
      ];
    };
  };
}
```

**Secret Definition:**

```nix
# configuration.nix
{ config, ... }: {
  age.secrets.my-secret.file = ./secrets/my-secret.age;
  age.secrets.my-secret.mode = "0400";
  age.secrets.my-secret.owner = "service-user";
  
  # SSH keys to use for decryption (defaults to host keys)
  age.identityPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
}
```

**Using agenix:**

```bash
# Create secret
agenix -e my-secret.age

# Edit existing secret
agenix -e my-secret.age -i ~/.ssh/id_ed25519

# Re-encrypt after changing recipients
agenix -r
```

### Choosing Between sops-nix and agenix

| Feature | sops-nix | agenix |
|---------|----------|--------|
| Encryption | age, GPG | age only |
| KMS Integration | Yes (AWS, GCP, Azure, Vault) | No |
| File Formats | YAML, JSON, INI, dotenv, binary | YAML, JSON, binary |
| Complexity | More features, steeper learning curve | Simpler, smaller codebase |
| Templates | Built-in | Manual (use config.sops.placeholder) |
| Multi-key | Shamir secret sharing support | All keys must decrypt |
| CI Integration | Excellent (Nix store) | Excellent (Nix store) |

**Recommendation:** Use sops-nix for production deployments requiring KMS integration. Use agenix for simpler setups or when you prefer minimal dependencies.

## System Updates and Rollbacks

### nixos-rebuild Commands

NixOS provides robust update and rollback mechanisms:

```bash
# Build new configuration (doesn't switch to it)
nixos-rebuild build

# Switch to new configuration
nixos-rebuild switch

# Test configuration (creates temporary generation)
nixos-rebuild test

# Build configuration without evaluating it
nixos-rebuild dry-build
```

### Boot Menu Rollbacks

NixOS automatically maintains previous configurations in the GRUB boot menu:

```
NixOS - Configuration 1 (Generation 154...) [Default]
NixOS - Configuration 2 (Generation 153...)
NixOS - Configuration 3 (Generation 152...)
```

**Rollback Procedure:**

```bash
# Reboot and select previous configuration from boot menu
# Then activate it
nixos-rebuild switch --rollback-generation 3
```

### Safe Update Workflow

```bash
# 1. Build and test
nixos-rebuild test

# 2. If test succeeds, switch
nixos-rebuild switch

# 3. If issues, reboot and select previous generation
# 4. Optionally, rollback to previous stable generation
nixos-rebuild switch --rollback-generation 3
```

**Important Files:**
- `/nix/var/nix/profiles/system` - System profile
- `/run/current-system` - Symlink to current generation
- `/boot/loader/entries` - Boot menu entries

## Monitoring and Observability

NixOS provides built-in monitoring capabilities through modules.

### Logging

```nix
{ config, ... }: {
  # Enable journald for persistent logging
  services.journald.extraConfig = ''
    SystemMaxUse=2G
    MaxRetentionSec=30day
  '';

  # Configure logrotate for additional log management
  services.logrotate = {
    enable = true;
    settings = {
      "/var/log/nginx/*.log" = {
        weekly = true;
        rotate = 52;
        compress = true;
        delaycompress = true;
      };
    };
  };
}
```

### Prometheus Monitoring

```nix
{ config, pkgs, ... }: {
  services.prometheus = {
    enable = true;
    port = 9090;
    retentionTime = "15d";
    scrapeConfigs = [
      {
        job_name = "node";
        static_configs = [{
          targets = [ "localhost:9100" ];
        }];
      }
    ];
  };

  # Node exporter
  services.prometheus.exporters.node = {
    enable = true;
    enabledCollectors = [ "systemd" "filesystem" ];
    disabledCollectors = [ "textfile" "time" ];
    port = 9100;
  };

  # NGINX exporter (if using NGINX)
  services.prometheus.exporters.nginx = {
    enable = true;
    port = 9113;
  };
}
```

### CloudWatch Integration

For AWS CloudWatch monitoring:

```bash
# Install CloudWatch agent
environment.systemPackages = with pkgs; [ pkgs.amazon-cloudwatch-agent ];

# Configure as service
services.amazon-cloudwatch-agent = {
  enable = true;
  settings = {
    metrics_collection_interval = 60;
    namespace = "CWAgent";
  };
};
```

### Health Checks

```nix
{ config, pkgs, ... }: {
  # Simple health check service
  systemd.services.healthcheck = {
    description = "System Health Check";
    script = ''
      #!/bin/sh
      set -eu
  
      echo "=== System Health Check ==="
      echo "Disk Usage:"
      df -h | grep -E '^/dev/'
      echo ""
      echo "Memory Usage:"
      free -h
      echo ""
      echo "Load Average:"
      uptime | awk -F'load average:' {print $2}''
      echo ""
      echo "Failed Services:"
      systemctl list-units --failed
    '';
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "/run/current-system/sw/bin/healthcheck";
    };
  };

  systemd.timers.healthcheck = {
    description = "Run health check every 5 minutes";
    timerConfig.OnCalendar = "*:0/5";
    wantedBy = [ "timers.target" ];
  };
}
```

## Operational Concerns

### Instance Sizing

NixOS image size considerations:
- Base NixOS image: ~2-3 GB
- Additional packages add to size
- Nix store can grow over time
- EC2 instance types with 8+ GB RAM recommended for builds

### Store Management

```nix
{ config, ... }: {
  # Automatic garbage collection
  nix.gc.automatic = true;
  
  # Keep last 30 days of generations
  nix.gc.options = "--delete-older-than 30d";
  
  # Optimize store
  nix.optimise.automatic = true;
}
```

### SSH Access

Best practices for EC2 SSH:
- Use key-based authentication only
- Disable password authentication in production
- Use AWS Systems Manager for key management
- Consider SSH bastion host for production clusters

```nix
{ config, ... }: {
  services.openssh = {
    enable = true;
    settings = {
      PasswordAuthentication = false;
      PermitRootLogin = "prohibit-password";
      X11Forwarding = false;
      AllowTcpForwarding = false;
      GatewayPorts = null;
    };
  };
}
```

### Firewall Security

```nix
{ config, ... }: {
  networking.firewall = {
    enable = true;
    allowedTCPPorts = [ 22 80 443 ];  # SSH, HTTP, HTTPS
    allowedUDPPorts = [ 53 ];  # DNS
    extraCommands = ''
      iptables -A INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
      iptables -A INPUT -m state --state INVALID -j DROP
    '';
  };
}
```

### Backup Strategy

```nix
{ config, pkgs, ... }: {
  # BorgBackup for encrypted backups
  services.borgbackup.jobs.nixos-backup = {
    paths = [ "/etc" "/var/lib" "/home" ];
    exclude = [ "/home/*/.cache" "/var/cache" "/var/tmp" ];
    compression = "lz4";
    encryption = {
      mode = "repokey";
      passphraseFile = "/run/secrets/borg-passphrase";
    };
    prune = {
      keepWithin = "1d";
      keepHourly = 24;
      keepDaily = 30;
      keepWeekly = 52;
      keepMonthly = 24;
      keepYearly = 10;
    };
    environment = {
      BORG_RSH = "borg@backup-server.com:/var/backup/nixos";
      BORG_PASSCOMMAND = "cat /run/secrets/borg-passphrase";
    };
  };

  systemd.timers.nixos-backup = {
    description = "Run BorgBackup every 2 hours";
    timerConfig.OnCalendar = "*:0/2:00";
    wantedBy = [ "timers.target" ];
  };
}
```

## Configuration Examples

### Minimal EC2 Instance Configuration

```nix
# configuration.nix
{ config, pkgs, ... }: {
  imports = [ ./hardware-configuration.nix ];

  # Boot
  boot.loader.grub.device = "/dev/xvda";
  boot.kernelParams = [ "console=ttyS0,115200n8" ];

  # Networking
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ 22 ];

  # SSH
  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "prohibit-password";
      PasswordAuthentication = false;
    };
  };

  # Time synchronization
  services.chrony.enable = true;
  time.timeZone = "UTC";

  # System packages
  environment.systemPackages = with pkgs; [
    pkgs.vim
    pkgs.git
    pkgs.htop
    pkgs.curl
    pkgs.jq
  ];

  # User
  users.users.deployer = {
    isNormalUser = true;
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIL0idNvgGiucWgup/mP78zyC23uFjYq0evcWdjGQUaBH"
    ];
  };

  # Nix store settings
  nix.gc.automatic = true;
  nix.gc.options = "--delete-older-than 30d";
}
```

### Full Stack with Monitoring and Secrets

```nix
# flake.nix
{
  description = "Production NixOS on EC2";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    sops-nix.url = "github:Mic92/sops-nix";
  };

  outputs = { self, nixpkgs, sops-nix }: {
    nixosConfigurations.prod = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration/base.nix
        ./configuration/ec2.nix
        ./configuration/monitoring.nix
        sops-nix.nixosModules.sops
      ];
    };
  };
}
```

```nix
# configuration/base.nix
{ config, ... }: {
  # Common settings across all instances
  environment.etc."ssh/sshd_config.d/10-no-password-login.conf".text = ''
    PasswordAuthentication no
    PermitRootLogin no
  '';

  nix.settings = {
    experimental-features = [ "nix-command" ];
  };
}
```

```nix
# configuration/ec2.nix
{ config, ... }: {
  boot.loader.grub.device = "/dev/xvda";
  networking.useDHCP = true;
  
  sops.defaultSopsFile = ./secrets/prod.yaml;
  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
}
```

```nix
# configuration/monitoring.nix
{ config, pkgs, ... }: {
  services.prometheus = {
    enable = true;
    port = 9090;
  };
  
  services.prometheus.exporters.node = {
    enable = true;
    enabledCollectors = [ "systemd" ];
    port = 9100;
  };
  
  services.nginx = {
    enable = true;
    virtualHosts."prometheus" = {
      locations."/" = {
        proxyPass = "http://localhost:9090";
      };
    };
  };
}
```

## Deployment Patterns

### Pattern 1: Blue-Green Deployment with Auto-Rollback

Deploy using deploy-rs with automatic rollback on failure:

```nix
# deploy.nix
{
  inputs.deploy-rs.url = "github:serokell/deploy-rs";

  outputs = { self, nixpkgs, deploy-rs }: {
    deploy.nodes = {
      node1 = {
        hostname = "nixos-01.example.com";
        profilesOrder = [ "system" ];
        profiles.system = {
          user = "root";
          path = deploy-rs.lib.x86_64-linux.activate.nixos 
            self.nixosConfigurations.node1;
        };
        # Auto rollback on failure
        magicRollback = true;
        sshUser = "deploy";
      };
    };
  };
}
```

### Pattern 2: Multi-Environment Deployment

Different configurations for dev/stage/prod:

```nix
{
  outputs = { self, nixpkgs, sops-nix }: {
    # Development environment
    nixosConfigurations.dev = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration/common.nix
        ./configuration/dev.nix
        sops-nix.nixosModules.sops
      ];
    };

    # Production environment
    nixosConfigurations.prod = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration/common.nix
        ./configuration/prod.nix
        sops-nix.nixosModules.sops
      ];
    };
  };
}
```

### Pattern 3: Golden AMI Pattern

Build a golden AMI and use user-data for per-instance configuration:

```nix
# Golden AMI configuration (minimal)
{ config, ... }: {
  # Basic system only
  services.openssh.enable = true;
  networking.useDHCP = true;
}

# Per-instance configuration (user-data)
# EC2 user-data can be used to set:
# - Hostname
# - SSH keys
# - Additional secrets
# - Instance-specific settings

# Example user-data (cloud-init compatible):
#cloud-config
packages:
  - vim
  - htop
runcmd:
  - [ sh, -c ]
  - |
    hostnamectl set-hostname nixos-prod-01
```

### Pattern 4: Immutable Infrastructure

Treat instances as immutable - rebuild on change:

```nix
{
  outputs = { self, nixpkgs, deploy-rs }: {
    deploy.nodes.prod-web = {
      profiles.system = {
        # Always rebuild from scratch
        path = deploy-rs.lib.x86_64-linux.activate.custom 
          (pkgs.writeScriptBin "rebuild-system" ''
            #!/bin/sh
            nixos-rebuild switch --flake .#prod-web
          '');
      };
    };
  };
}
```

## Recommended Workflow

### Initial Setup

1. **Build Golden AMI**
   ```bash
   nixos-rebuild build-image --image-variant amazon
   # Upload to AWS and register
   ```

2. **Set Up Secrets**
   ```bash
   sops secrets/prod.yaml
   # Add SSH keys and secrets
   sops --rekey
   ```

3. **Deploy First Instance**
   ```bash
   deploy .#prod-node1
   ```

### Daily Operations

1. **Apply Changes**
   ```bash
   # Update configuration
   deploy .#prod-node1

   # Or manual for simple changes
   nixos-rebuild switch --flake .#prod-node1 --target-host host-ip
   ```

2. **Monitor Health**
   ```bash
   # Check service status
   ssh root@host "systemctl status"
   
   # Review logs
   ssh root@host "journalctl -u nginx -f"
   ```

3. **Rotate Secrets**
   ```bash
   # Update secret file
   sops secrets/prod.yaml
   
   # Re-encrypt with new keys
   sops --rekey
   
   # Deploy
   deploy .#prod-node1
   ```

4. **Clean Store**
   ```bash
   ssh root@host "nix-collect-garbage -d"
   ```

### Recovery Procedures

1. **Configuration Breaks System**
   - Reboot and select previous generation in GRUB menu
   - Investigate issue
   - Rollback: `nixos-rebuild switch --rollback-generation N`
   - Fix configuration
   - Deploy fixed version

2. **Lost SSH Access**
   - Use AWS Systems Manager Serial Console
   - Enable temporary root password via AWS User Data
   - Investigate SSH configuration
   - Fix and redeploy

3. **Store Corruption**
   - Boot to previous generation
   - Delete corrupted generation
   - Force garbage collection
   - Rebuild system

## References

- [NixOS Manual](https://nixos.org/manual/nixos/stable/)
- [NixOS Download Page](https://nixos.org/download.html)
- [sops-nix Documentation](https://github.com/Mic92/sops-nix)
- [agenix Documentation](https://github.com/ryantm/agenix)
- [deploy-rs Documentation](https://github.com/serokell/deploy-rs)
- [NixOps (Deprecated)](https://github.com/NixOS/nixops)
- [NixOS Modules - Prometheus](https://search.nixos.org/options?query=prometheus)
- [NixOS Modules - Logging](https://search.nixos.org/options?query=services.logging)

## Appendix: Quick Reference

### Essential Commands

```bash
# Image building
nixos-rebuild build-image --image-variant amazon

# Configuration management
nixos-rebuild switch
nixos-rebuild test
nixos-rebuild build

# Deployment
deploy .#hostname
deploy .#hostname.profile-name

# Secrets
sops secrets/file.yaml
agenix -e secret.age

# Store maintenance
nix-collect-garbage -d
nix-store-optimise

# System management
systemctl status service-name
journalctl -u service-name -f
```

### File Locations

- `/etc/nixos/configuration.nix` - System configuration
- `/etc/nixos/hardware-configuration.nix` - Hardware-specific config
- `/nix/var/nix/profiles/system` - System profile
- `/run/secrets` - Decrypted secrets (sops-nix)
- `/run/agenix` - Decrypted secrets (agenix)
- `/nix/store` - Nix store
- `/var/log/journal` - System logs

### Useful NixOS Options

- `boot.loader.grub.device` - Boot device
- `networking.useDHCP` - Enable DHCP
- `services.openssh.enable` - Enable SSH server
- `nix.gc.automatic` - Automatic garbage collection
- `nix.optimise.automatic` - Automatic store optimization
- `systemd.services.*.enable` - Enable systemd service
