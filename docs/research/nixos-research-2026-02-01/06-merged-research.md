# NixOS Research: Merged Findings

**Date:** 2026-02-01  
**Project:** ChoirOS Deployment Architecture  
**Scope:** Nix fundamentals, NixOS server deployment, container strategies, Rust toolchain, and EC2 patterns

---

## Executive Summary

This document consolidates research on using Nix/NixOS as the deployment foundation for ChoirOS. The research covers five key areas: Nix fundamentals, NixOS server configuration, container integration, Rust toolchain management, and EC2 deployment patterns.

**Key Findings:**
- Nix provides reproducible builds and declarative package management ideal for Rust projects
- NixOS offers immutable infrastructure with rollback capabilities perfect for server deployment
- Container strategies range from pure Nix builds to Docker integration
- Rust toolchain management via Nix ensures consistent compiler versions across environments
- EC2 deployment patterns include AMI baking, nixos-rebuild, and tools like Colmena/Morph

**Recommendation:** Adopt Nix for local development and CI, with NixOS for production server deployment using AMI-based patterns.

---

## Nix Fundamentals

### What is Nix?

Nix is a **purely functional package manager** and build system that treats package management as a functional programming problem. Key characteristics:

- **Reproducibility:** Same inputs always produce same outputs
- **Declarative:** System state defined in `.nix` files, not shell commands
- **Atomic upgrades/rollbacks:** Changes are all-or-nothing with instant rollback
- **Multi-version support:** Multiple versions of same package coexist
- **Hermetic builds:** Isolated from system state, only sees declared dependencies

### Core Concepts

#### The Nix Store
All packages live in `/nix/store/` with cryptographic hash prefixes:
```
/nix/store/abc123...-rustc-1.75.0/
/nix/store/def456...-cargo-1.75.0/
```

#### Nix Language Basics
```nix
# Variables
let
  pkgs = import <nixpkgs> {};
in
# Functions
pkgs.mkShell {
  buildInputs = [ pkgs.rustc pkgs.cargo ];
}
```

#### Flakes (Modern Nix)
Flakes provide:
- Lock files for reproducibility (`flake.lock`)
- Inputs/outputs schema
- Composable configurations

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
  
  outputs = { self, nixpkgs }: {
    packages.default = nixpkgs.legacyPackages.x86_64-linux.hello;
  };
}
```

### Nix for Rust Development

#### Basic Rust Shell
```nix
# shell.nix
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    openssl
    pkg-config
  ];
  
  RUST_BACKTRACE = 1;
}
```

#### Using rust-overlay (Latest Toolchain)
```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  
  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [ 
          (pkgs.rust-bin.stable.latest.default)
          pkgs.openssl
          pkgs.pkg-config
        ];
      };
    };
}
```

#### Crane for Rust Builds
Crane provides efficient Nix builds for Rust by caching dependencies separately from the main build:

```nix
{
  inputs.crane.url = "github:ipetkov/crane";
  
  outputs = { self, nixpkgs, crane }:
    let
      craneLib = crane.mkLib nixpkgs;
    in {
      packages.default = craneLib.buildPackage {
        src = craneLib.cleanCargoSource ./.;
        # Dependencies cached separately
      };
    };
}
```

### Key Benefits for ChoirOS

1. **CI/CD Reproducibility:** Same build environment locally and in CI
2. **Dependency Caching:** Nix store caches between builds
3. **Cross-compilation:** Easy target configuration for ARM64
4. **Development Shells:** `nix develop` provides consistent dev environment
5. **Docker Integration:** Build minimal Docker images from Nix

---

## NixOS on EC2

### NixOS Overview

NixOS is a Linux distribution built on Nix principles:
- Entire system configuration in `/etc/nixos/configuration.nix`
- Atomic upgrades with `nixos-rebuild switch`
- Rollback to previous generations on boot
- Immutable infrastructure by design

### EC2 Deployment Patterns

#### Pattern 1: Official NixOS AMIs

Use pre-built NixOS AMIs from the NixOS project:

```bash
# Find latest AMI
aws ec2 describe-images \
  --owners 427812963091 \
  --filters "Name=name,Values=nixos/23.11*" \
  --query 'Images[*].[ImageId,Name]'

# Launch instance
aws ec2 run-instances \
  --image-id ami-xxxxxxxxxxxxx \
  --instance-type t3.medium \
  --key-name my-key
```

**Pros:**
- Quick to get started
- Official support
- Regular updates

**Cons:**
- Generic configuration
- Manual post-deployment setup

#### Pattern 2: Custom AMI with Packer

Build custom AMIs with ChoirOS pre-installed:

```nix
# nixos-config.nix
{ config, pkgs, ... }:

{
  imports = [ <nixpkgs/nixos/modules/virtualisation/amazon-image.nix> ];
  
  ec2.hvm = true;
  
  # ChoirOS-specific configuration
  services.choiros = {
    enable = true;
    package = pkgs.choiros;
    configFile = "/etc/choiros/config.toml";
  };
  
  # System packages
  environment.systemPackages = with pkgs; [
    git
    htop
    docker
  ];
  
  # Auto-upgrade
  system.autoUpgrade = {
    enable = true;
    dates = "weekly";
  };
}
```

```bash
# Build AMI
nixos-generate -f amazon-ec2 -c nixos-config.nix
```

**Pros:**
- Fully configured at boot
- Fast instance startup
- Immutable deployments

**Cons:**
- AMI build time
- Storage costs for AMIs

#### Pattern 3: NixOS Anywhere (Remote Install)

Install NixOS on existing EC2 instances:

```bash
# From local machine
nix run github:nix-community/nixos-anywhere -- \
  --flake .#choiros-server \
  root@ec2-xx-xx-xx-xx.compute-1.amazonaws.com
```

**Pros:**
- Works with any base AMI
- No custom AMI needed

**Cons:**
- Complex initial setup
- Slower than pre-baked AMIs

#### Pattern 4: Colmena/Morph (Multi-Host)

For managing multiple NixOS hosts:

```nix
# flake.nix
{
  inputs.colmena.url = "github:zhaofengli/colmena";
  
  outputs = { self, nixpkgs, colmena }: {
    colmena = {
      meta.nixpkgs = import nixpkgs { system = "x86_64-linux"; };
      
      defaults = { pkgs, ... }: {
        deployment.targetHost = null; # Set per-host
        imports = [ ./common.nix ];
      };
      
      web-server = { pkgs, ... }: {
        deployment.targetHost = "ec2-xx-xx.compute-1.amazonaws.com";
        services.nginx.enable = true;
      };
      
      app-server = { pkgs, ... }: {
        deployment.targetHost = "ec2-yy-yy.compute-1.amazonaws.com";
        services.choiros.enable = true;
      };
    };
  };
}
```

```bash
# Deploy to all hosts
colmena apply

# Deploy to specific host
colmena apply --on web-server
```

**Pros:**
- Single command multi-host deployment
- Parallel execution
- Secrets management

**Cons:**
- Additional tool to learn
- SSH key management

### EC2-Specific NixOS Configuration

#### EBS Optimization
```nix
{
  # Use EBS-optimized instance settings
  boot.loader.grub.device = "/dev/nvme0n1";
  
  fileSystems."/data" = {
    device = "/dev/nvme1n1";
    fsType = "ext4";
    autoFormat = true;
  };
}
```

#### CloudWatch Integration
```nix
{
  services.amazon-cloudwatch-agent = {
    enable = true;
    config = ''
      {
        "metrics": {
          "namespace": "ChoirOS",
          "metrics_collected": {
            "disk": { "measurement": ["used_percent"] },
            "mem": { "measurement": ["used_percent"] }
          }
        }
      }
    '';
  };
}
```

#### Auto-scaling Hook
```nix
{
  systemd.services.choiros-autoscale = {
    description = "ChoirOS Auto-scaling Lifecycle Hook";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.choiros}/bin/choiros-register";
    };
  };
}
```

---

## Container Strategy

### Nix vs Docker

| Aspect | Nix | Docker |
|--------|-----|--------|
| **Build** | Pure, reproducible | Layer-based, imperative |
| **Image Size** | Minimal (only runtime deps) | Larger (base image + layers) |
| **Security** | No base OS, single binary | Container runtime isolation |
| **Caching** | Content-addressed store | Layer caching |
| **Orchestration** | Limited native support | Kubernetes, ECS, etc. |

### Strategy 1: Pure Nix Containers (nix2container)

Build OCI containers directly from Nix without Docker:

```nix
{
  inputs.nix2container.url = "github:nlewo/nix2container";
  
  outputs = { self, nixpkgs, nix2container }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      nix2containerLib = nix2container.packages.x86_64-linux.nix2container;
    in {
      packages.x86_64-linux.choiros-container = nix2containerLib.buildImage {
        name = "choiros";
        tag = "latest";
        config = {
          Entrypoint = [ "${pkgs.choiros}/bin/choiros" ];
          Env = [ "RUST_LOG=info" ];
        };
      };
    };
}
```

```bash
# Build and load into Docker
nix run .#choiros-container.copyToDockerDaemon

# Or push directly to registry
nix run .#choiros-container.copyToRegistry
```

**Pros:**
- Smallest possible images
- No base image vulnerabilities
- Fast builds with Nix caching

**Cons:**
- Limited tooling ecosystem
- No native orchestration

### Strategy 2: Docker with Nix-built Binaries

Use Nix to build, Docker to package:

```nix
# Build ChoirOS binary
packages.choiros-binary = craneLib.buildPackage {
  src = craneLib.cleanCargoSource ./.;
};

# Create Docker image
packages.choiros-docker = pkgs.dockerTools.buildLayeredImage {
  name = "choiros";
  tag = "latest";
  contents = [ 
    packages.choiros-binary
    pkgs.cacert  # SSL certificates
  ];
  config = {
    Cmd = [ "/bin/choiros" ];
    ExposedPorts = {
      "8080/tcp" = {};
    };
  };
};
```

```bash
# Load into Docker
docker load < $(nix build .#choiros-docker --print-out-paths)
```

**Pros:**
- Familiar Docker workflow
- Works with existing orchestration
- Smaller than typical Docker builds

**Cons:**
- Still requires Docker runtime
- Layer caching less efficient

### Strategy 3: NixOS Containers (systemd-nspawn)

Lightweight containers using NixOS:

```nix
{
  containers.choiros = {
    autoStart = true;
    config = { pkgs, ... }: {
      services.choiros.enable = true;
      networking.firewall.allowedTCPPorts = [ 8080 ];
    };
  };
}
```

**Pros:**
- Full NixOS in container
- Systemd integration
- Minimal overhead

**Cons:**
- Linux only
- Less portable than Docker

### Strategy 4: Kubernetes with Nix

Deploy to K8s using Nix-generated manifests:

```nix
{
  packages.k8s-manifests = pkgs.writeText "choiros-k8s.yaml" ''
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: choiros
    spec:
      replicas: 3
      selector:
        matchLabels:
          app: choiros
      template:
        metadata:
          labels:
            app: choiros
        spec:
          containers:
          - name: choiros
            image: choiros:${self.shortRev or "latest"}
            ports:
            - containerPort: 8080
  '';
}
```

**Recommendation for ChoirOS:**
- **Development:** Pure Nix (nix2container) for fast iteration
- **Production:** Docker with Nix-built binaries for ECS/EKS compatibility
- **Future:** Evaluate NixOS containers for on-premise deployments

---

## Decision Matrix

### When to Use What

| Scenario | Recommendation | Rationale |
|----------|---------------|-----------|
| **Local Development** | Nix Flake + direnv | Reproducible dev environment, instant setup |
| **CI/CD Builds** | Nix + Crane | Cached dependencies, consistent builds |
| **Single EC2 Server** | NixOS AMI | Full system management, atomic updates |
| **Multi-Server Setup** | Colmena + NixOS | Single-command deployment, consistency |
| **Container Deployment** | Docker + Nix binary | Compatibility with existing infrastructure |
| **Serverless/Lambda** | Nix-built ZIP | Minimal cold start, precise dependencies |
| **On-Premise** | NixOS bare metal | Complete system control |

### ChoirOS Deployment Options

#### Option A: NixOS EC2 (Recommended)
- **Best for:** Production server deployment
- **Pros:** Immutable, atomic updates, full system control
- **Cons:** Learning curve, smaller ecosystem
- **Cost:** Standard EC2 pricing

#### Option B: Docker on ECS/EKS
- **Best for:** Teams familiar with Docker/Kubernetes
- **Pros:** Familiar tooling, managed orchestration
- **Cons:** More complex, larger attack surface
- **Cost:** ECS/EKS fees + EC2

#### Option C: Binary on Standard Linux
- **Best for:** Quick migration from existing setup
- **Pros:** Minimal change, familiar
- **Cons:** Manual dependency management
- **Cost:** Standard EC2 pricing

---

## Implementation Roadmap

### Phase 1: Development Environment (Week 1)
- [ ] Create `flake.nix` with Rust toolchain
- [ ] Set up crane for efficient builds
- [ ] Configure direnv for automatic shell
- [ ] Document setup in README

### Phase 2: CI/CD Integration (Week 2)
- [ ] GitHub Actions with Nix
- [ ] Cachix for build artifact caching
- [ ] Automated Docker image builds
- [ ] Integration test pipeline

### Phase 3: Staging Deployment (Week 3)
- [ ] Build custom NixOS AMI
- [ ] Deploy to staging EC2
- [ ] Configure basic monitoring
- [ ] Document deployment process

### Phase 4: Production Deployment (Week 4)
- [ ] Production AMI with hardened config
- [ ] Multi-AZ deployment with Colmena
- [ ] Automated backups
- [ ] Runbook documentation

### Phase 5: Optimization (Week 5-6)
- [ ] Evaluate nix2container vs Docker
- [ ] Implement blue/green deployment
- [ ] Add auto-scaling hooks
- [ ] Performance tuning

---

## Open Questions

### Technical
1. **Secrets Management:** How to handle AWS credentials, database passwords in NixOS? Options:
   - agenix (age-encrypted secrets in repo)
   - sops-nix (Mozilla SOPS integration)
   - AWS Secrets Manager at runtime

2. **Database Migrations:** Should migrations run as systemd services on deploy or as separate jobs?

3. **Asset Pipeline:** How to integrate Dioxus frontend builds into Nix derivation?

4. **Monitoring:** Use CloudWatch Agent or Prometheus/Grafana stack?

5. **SSL/TLS:** Let's Encrypt via NixOS module or AWS ACM with ALB?

### Organizational
1. **Team Onboarding:** How steep is the Nix learning curve for new team members?
2. **Debugging:** How to debug production issues in immutable NixOS?
3. **Rollback Strategy:** How far back do we keep NixOS generations?

### Strategic
1. **Multi-cloud:** How portable is this setup to GCP/Azure?
2. **Serverless Future:** Should we plan for Lambda/edge deployment?
3. **Community:** Contribute ChoirOS modules back to nixpkgs?

---

## Appendix: Quick Reference

### Essential Commands

```bash
# Enter development shell
nix develop

# Build project
nix build

# Build Docker image
nix build .#dockerImage && docker load < result

# Deploy to NixOS server
nixos-rebuild switch --target-host root@server --flake .#server

# Deploy with Colmena
colmena apply

# Update flake inputs
nix flake update

# Garbage collect old generations
nix-collect-garbage -d
```

### Useful Resources

- [Nix Pills](https://nixos.org/guides/nix-pills/) - Tutorial series
- [NixOS Manual](https://nixos.org/manual/nixos/stable/) - Official docs
- [Crane](https://github.com/ipetkov/crane) - Rust + Nix
- [nix2container](https://github.com/nlewo/nix2container) - Container builds
- [Colmena](https://github.com/zhaofengli/colmena) - Multi-host deployment
- [agenix](https://github.com/ryantm/agenix) - Secret management

---

*Document generated by merge worker consolidating research from: nix-basics, nixos-server, containers-nix, rust-toolchain, ec2-patterns*
