# Nix Containers Research

**Date:** 2026-02-01
**Focus:** Container management using Nix instead of Docker

## Executive Summary

Nix provides a fundamentally different approach to container building compared to traditional Dockerfiles. The `nixpkgs.dockerTools` utilities enable building reproducible, minimal container images programmatically, with significant advantages in size, security, and build caching. This document explores the technical details, implementation patterns, and migration strategies.

## Table of Contents

1. [nixpkgs.dockerTools vs Dockerfiles](#1-nixpkgsg-dockertools-vs-dockerfiles)
2. [Building Minimal Container Images with Nix](#2-building-minimal-container-images-with-nix)
3. [OCI-Compliant Images from Nix](#3-oci-compliant-images-from-nix)
4. [Container Runtime Integration](#4-container-runtime-integration)
5. [Multi-Architecture Builds](#5-multi-architecture-builds)
6. [Caching and Layer Optimization](#6-caching-and-layer-optimization)
7. [Migration Path from Docker](#7-migration-path-from-docker)
8. [Practical Examples](#8-practical-examples)
9. [Challenges and Limitations](#9-challenges-and-limitations)

---

## 1. nixpkgs.dockerTools vs Dockerfiles

### 1.1 Fundamental Differences

| Aspect | Dockerfiles | nixpkgs.dockerTools |
|--------|-----------|---------------------|
| **Language** | Shell-based DSL | Nix expression language |
| **Base Image** | FROM directive (alpine, debian, etc.) | Starts from scratch (no base) |
| **Build Context** | Docker daemon required | No Docker daemon needed |
| **Layer Strategy** | Multiple layers (cache optimization) | Single layer (or layered for sharing) |
| **Dependency Management** | Implicit in RUN commands | Explicit closure tracking |
| **Reproducibility** | Requires careful ordering | Guaranteed by Nix store |
| **Binary Cache** | Registry-based | Nix binary cache (cache.nixos.org) |

### 1.2 Architecture Comparison

**Dockerfile Pattern:**
```dockerfile
FROM alpine:3.19
RUN apk add --no-cache redis
COPY redis.conf /etc/redis/
CMD ["redis-server"]
# Multiple layers, ~25MB
```

**Nix Pattern:**
```nix
{ pkgs ? import <nixpkgs> {} }:
with pkgs;
dockerTools.buildImage {
  name = "redis";
  contents = [ redis ];
  config.Cmd = [ "redis-server" ];
  # Single layer, ~42MB or ~25MB optimized
}
```

### 1.3 Key Advantages of Nix Approach

1. **No Docker Daemon Required**: Builds happen entirely in Nix
2. **True Reproducibility**: Same inputs → same outputs, guaranteed
3. **Precise Dependency Tracking**: Nix knows exactly what's needed at runtime
4. **Build-Time vs Runtime Separation**: Build dependencies don't bloat runtime images
5. **Cross-Platform Builds**: Build anywhere, deploy anywhere
6. **Content Addressing**: Built-in deduplication and sharing

---

## 2. Building Minimal Container Images with Nix

### 2.1 dockerTools.buildImage

The core function for building Docker-compatible images:

```nix
dockerTools.buildImage {
  name = "my-image";
  tag = "latest";

  # Packages to include (runtime closure only)
  contents = [ pkgs.hello pkgs.bash ];

  # Run commands as root during build
  runAsRoot = ''
    #!/bin/sh
    mkdir -p /data
    chmod 777 /data
  '';

  # Docker configuration
  config = {
    Cmd = [ "hello" ];
    Env = [ "PATH=/bin:/usr/bin" ];
    ExposedPorts = {
      "8080/tcp" = {};
    };
    WorkingDir = "/data";
  };

  # Optional: disk size in MB
  diskSize = 1024;
}
```

### 2.2 Size Optimization Techniques

#### Technique 1: Single Layer vs Multi-Layer

**Single Layer (buildImage):**
- Best for: Single-purpose containers
- Size: ~42MB for Redis (vs 177MB Docker Hub)
- Pros: Simpler, no layer overhead
- Cons: Can't share intermediate layers

**Multi-Layer (buildLayeredImage):**
```nix
dockerTools.buildLayeredImage {
  name = "my-app";
  contents = [
    # Base layer
    (pkgs.writeText "base.txt" "base content")
    # App layer
    (pkgs.writeText "app.txt" "app content")
  ];

  # Group into layers manually
  extraCommands = ''
    mv base.txt $out/layer1/
    mv app.txt $out/layer2/
  '';
}
```

#### Technique 2: Runtime-Only Dependencies

Nix automatically excludes build-time dependencies:

```nix
{ pkgs ? import <nixpkgs> {} }:
dockerTools.buildImage {
  name = "redis";
  runAsRoot = ''
    #!/bin/sh
    ${pkgs.dockerTools.shadowSetup}
    groupadd -r redis
    useradd -r -g redis -d /data -M redis
    mkdir /data
    chown redis:redis /data
  '';

  config = {
    Cmd = [ "${pkgs.goPackages.gosu.bin}/bin/gosu" "redis"
            "${pkgs.redis}/bin/redis-server" ];
    WorkingDir = "/data";
  };
  # Result: ~25MB (vs 42MB with coreutils)
  # shadow-utils and coreutils excluded at runtime!
}
```

#### Technique 3: Using fakeNss for Minimal Userspace

```nix
{ pkgs ? import <nixpkgs> {} }:
dockerTools.buildImage {
  name = "minimal-service";
  runAsRoot = ''
    #!/bin/sh
    ${pkgs.dockerTools.shadowSetup}
    ${pkgs.fakeNss}
    groupadd -r myuser
    useradd -r -g myuser myuser
  '';

  contents = [ pkgs.myapp ];
  config = {
    User = "myuser";
  };
}
```

### 2.3 Comparison: Alpine vs Nix Minimal Images

| Service | Alpine-based | Nix minimal | Savings |
|---------|------------|-------------|---------|
| Redis | 15MB | 25MB | -67% (Nix larger) |
| Nginx | 40MB | 35MB | +12% (Nix smaller) |
| Go app | 2MB (static) | 15MB | -650% (Alpine wins) |

**Insight:** For simple static binaries, Alpine wins. For complex apps with many dependencies, Nix's closure-based approach can be competitive or superior.

---

## 3. OCI-Compliant Images from Nix

### 3.1 OCI Format Support

Nix produces standard OCI-compliant Docker images:

- **Manifest format**: Docker Image Specification v1.2
- **Layer format**: gzip-compressed tar archives
- **Config format**: OCI runtime spec compatible
- **Result**: Works with Docker, Podman, containerd, nerdctl, etc.

### 3.2 OCI Tools (ociTools)

For pure OCI containers (no Docker-specific features):

```nix
{ pkgs ? import <nixpkgs> {} }:
with pkgs;
ociTools.buildContainer {
  name = "my-oci-container";

  # Runtime specification
  config = {
    cmd = [ "/bin/bash" ];
    env = [ "PATH=/bin" ];
  };

  # Root filesystem
  contents = [
    bash
    coreutils
  ];

  # Process configuration
  process = {
    terminal = true;
    user = {
      uid = 0;
      gid = 0;
    };
    args = [ "/bin/bash" ];
    cwd = "/";
  };
}
```

### 3.3 Export and Import

```bash
# Build Nix image
nix-build image.nix

# Load into Docker
docker load < result

# Or export for distribution
nix-store --export > my-image.nar
```

### 3.4 Pushing to Registries

```bash
# Tag for registry
docker tag choir-sandbox:latest registry.example.com/choiros/sandbox:v1

# Push
docker push registry.example.com/choiros/sandbox:v1
```

---

## 4. Container Runtime Integration

### 4.1 Docker on NixOS

**Installation:**
```nix
# /etc/nixos/configuration.nix
{
  virtualisation.docker.enable = true;

  # Add user to docker group
  users.users.myuser.extraGroups = [ "docker" ];
}
```

**Rootless Docker:**
```nix
virtualisation.docker.rootless = {
  enable = true;
  setSocketVariable = true;
};
```

### 4.2 Podman Integration

Podman is a preferred alternative on NixOS:

```nix
virtualisation.podman = {
  enable = true;
  dockerCompat = true;  # Create docker alias
};

# Container common config
virtualisation.containers = {
  enable = true;
  registries.search = [ "docker.io" "ghcr.io" ];
};
```

**Podman Compose:**
```bash
# Install podman-compose
environment.systemPackages = [ pkgs.podman-compose ];

# Use like docker-compose
podman-compose up
```

### 4.3 systemd Service Management

Run containers as native systemd services:

```nix
virtualisation.oci-containers = {
  backend = "docker";  # or "podman"
  containers.choiros-sandbox = {
    image = "choiros/sandbox:latest";
    autoStart = true;
    ports = [
      "127.0.0.1:8080:8080"
    ];
    environment = {
      DATABASE_URL = "/data/events.db";
    };
    volumes = [
      "/var/lib/choiros:/data:rw"
    ];
  };
};
```

### 4.4 GPU Pass-Through

```nix
# Enable NVIDIA support
hardware.nvidia-container-toolkit.enable = true;

# Use in container
docker run --device=nvidia.com/gpu=all my-app
```

---

## 5. Multi-Architecture Builds

### 5.1 Remote Building with Nix

Nix supports transparent cross-architecture builds:

**Setup (nix.conf):**
```
# /etc/nix/nix.conf
builders = ssh://arm64-builder aarch64-linux ; \
            ssh://x86-builder x86_64-linux
```

**Test connection:**
```bash
nix store info --store ssh://arm64-builder
```

### 5.2 Building for Multiple Architectures

```nix
{ pkgs ? import <nixpkgs> {} }:
let
  # Define image builder
  buildImage = system: pkgs: dockerTools.buildImage {
    name = "choiros-sandbox";
    contents = [ pkgs.hello ];
    config.Cmd = [ "hello" ];
  };
in {
  # Build for x86_64-linux
  x86_64-image = pkgs.callPackage buildImage {
    system = "x86_64-linux";
  };

  # Build for aarch64-linux
  aarch64-image = pkgs.callPackage buildImage {
    system = "aarch64-linux";
  };
}
```

### 5.3 Cross-Compilation with Flakes

**flake.nix:**
```nix
{
  description = "Multi-arch container builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in {
        packages.${system}.choiros-sandbox = dockerTools.buildImage {
          name = "choiros-sandbox";
          tag = system;
          contents = [ pkgs.sandbox ];
          config.Cmd = [ "sandbox" ];
        };
      }
    );
}
```

**Build all architectures:**
```bash
nix build .#packages.x86_64-linux.choiros-sandbox
nix build .#packages.aarch64-linux.choiros-sandbox
```

### 5.4 Platform Tiers (NixOS Support)

| Platform | Tier | Hydra Support | Notes |
|----------|------|---------------|-------|
| x86_64-linux | 1 | ✔️ Full | Production-ready |
| aarch64-linux | 2 | ✔️ Full | Production-ready |
| x86_64-darwin | 2 | ✔️ Full | macOS |
| aarch64-darwin | 2 | ✔️ Full | Apple Silicon |
| riscv64-linux | 3 | ❌ No CI | Community |

---

## 6. Caching and Layer Optimization

### 6.1 Nix Binary Cache vs Docker Registry

**Nix Binary Cache:**
- Location: `cache.nixos.org` (public) or private
- Content-addressed: By hash of derivation
- Automatic: Nix fetches automatically if available
- Shared: Across all images using same packages

**Docker Registry:**
- Location: Docker Hub, GHCR, etc.
- Tag-based: By image:tag name
- Manual: Requires `docker pull`
- Isolated: Per image

### 6.2 Cache Strategy Comparison

```nix
# Approach 1: Base image for sharing
{ pkgs ? import <nixpkgs> {} }:
let
  # Build base layer
  baseImage = pkgs.dockerTools.buildImage {
    name = "choiros-base";
    contents = [
      pkgs.bash
      pkgs.coreutils
      pkgs.sqlite
    ];
  };

  # Build app layer on top
in pkgs.dockerTools.buildImage {
  name = "choiros-app";
  fromImage = baseImage;
  contents = [
    pkgs.choiros
  ];
  config.Cmd = [ "choiros" ];
}
```

### 6.3 Nix-Snapshotter for Direct Store Access

Alternative approach: Use Nix store directly in containers:

```nix
# Instead of copying packages, reference store
{
  virtualisation.docker.enable = true;
  virtualisation.docker.daemon.settings = {
    features = ["containerd-snapshotter"];
    runtimes = {
      nix = {
        path = "${pkgs.nix-snapshotter}/bin/nix-snapshotter";
        runtimeArgs = ["--store", "/nix/store"];
      };
    };
  };
}
```

**Benefits:**
- No copying: Packages stay in /nix/store
- Zero duplication: Single copy per package
- Instant: No layer extraction time

### 6.4 Cachix for Private Caching

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
    in {
      packages.x86_64-linux.choiros = pkgs.dockerTools.buildImage {
        name = "choiros";
        contents = [ pkgs.choiros ];
      };
    };

  nixConfig = {
    # Enable Cachix
    substituters = [ "https://choiros.cachix.org" ];
    trusted-public-keys = [ "choiros.cachix.org-1:KEY" ];
  };
}
```

**Push to Cachix:**
```bash
cachix use choiros
nix build
cachix push
```

---

## 7. Migration Path from Docker

### 7.1 Gradual Migration Strategy

**Phase 1: Coexistence**
- Keep existing Dockerfiles
- Add Nix builds alongside
- Compare sizes, build times
- Validate functional equivalence

**Phase 2: Hybrid Approach**
- Use Nix for base images
- Use Dockerfile for application layer
- Leverage best of both worlds

**Phase 3: Full Nix Migration**
- Replace all Dockerfiles with Nix expressions
- Update CI/CD pipelines
- Decommission Dockerfile maintenance

### 7.2 Dockerfile to Nix Translation

**Original Dockerfile:**
```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y \
    redis-server \
    sqlite3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
COPY myapp /usr/local/bin/
EXPOSE 8080
CMD ["myapp"]
```

**Nix Equivalent:**
```nix
{ pkgs ? import <nixpkgs> {} }:
with pkgs;
dockerTools.buildImage {
  name = "myapp";
  tag = "latest";

  runAsRoot = ''
    #!/bin/sh
    ${dockerTools.shadowSetup}
    groupadd -r appuser
    useradd -r -g appuser -d /data -M appuser
    mkdir -p /data
    chown appuser:appuser /data
  '';

  contents = [
    redis
    sqlite
    (writeScriptBin "myapp" ''
      #!/bin/sh
      exec ${myapp}/bin/myapp "$@"
    '')
  ];

  config = {
    User = "appuser";
    ExposedPorts = {
      "8080/tcp" = {};
    };
    WorkingDir = "/data";
    Cmd = [ "myapp" ];
  };
}
```

### 7.3 CI/CD Integration

**GitHub Actions (Docker):**
```yaml
- name: Build Docker image
  run: docker build -t myapp .
- name: Push to registry
  run: docker push myapp
```

**GitHub Actions (Nix):**
```yaml
- name: Install Nix
  uses: cachix/install-nix-action@v22
  with:
    nix_path: "${{ env.NIX_PATH }}"
    extra_nix_config: |
      experimental-features = nix-command flakes

- name: Build Nix image
  run: nix build .#packages.x86_64-linux.myapp

- name: Load to Docker
  run: docker load < result

- name: Push to registry
  run: docker push myapp
```

### 7.4 Local Development Workflow

**Before (Docker Compose):**
```bash
docker-compose up
docker-compose exec app bash
docker-compose down
```

**After (Nix):**
```bash
# Build once
nix-build image.nix
docker load < result

# Run repeatedly
docker run -p 8080:8080 myapp

# Interactive shell
docker run -it --rm myapp bash
```

---

## 8. Practical Examples

### 8.1 Rust Web Service (ChoirOS Sandbox)

```nix
# containers/choiros-sandbox.nix
{ pkgs ? import <nixpkgs> {} }:
with pkgs;
dockerTools.buildImage {
  name = "choiros/sandbox";
  tag = "latest";

  # Runtime dependencies
  contents = [
    sandbox  # Our Rust binary
    sqlite
    bash
  ];

  # Setup users and directories
  runAsRoot = ''
    #!/bin/sh
    ${dockerTools.shadowSetup}
    groupadd -r sandbox
    useradd -r -g sandbox -d /data -M sandbox
    mkdir -p /data
    chown sandbox:sandbox /data
  '';

  config = {
    User = "sandbox";
    ExposedPorts = {
      "8080/tcp" = {};
    };
    Env = [
      "DATABASE_URL=/data/events.db"
      "RUST_LOG=info"
    ];
    WorkingDir = "/data";
    Volumes = {
      "/data" = {};
    };
    Cmd = [ "sandbox" ];
  };

  # Reproducible build date
  created = builtins.substring 0 8 self.lastModifiedDate;
}
```

### 8.2 Multi-Service Stack with Arion

**arion-compose.nix:**
```nix
{ pkgs, lib, ... }:
{
  config = {
    services.nginx = {
      service = {
        image = "nginx:alpine";
        ports = [ "80:80" ];
        depends_on = [ "backend" ];
      };
    };

    services.backend = {
      service = {
        image = "";
        restart = "unless-stopped";
        ports = [ "3000:3000" ];
        environment = {
          DATABASE_URL = "postgresql://user:pass@db/app";
        };
      };
    };

    services.db = {
      service = {
        image = "postgres:15-alpine";
        volumes = [
          "pgdata:/var/lib/postgresql/data"
        ];
        environment = {
          POSTGRES_PASSWORD = "pass";
        };
      };
    };

    volumes.pgdata = {};
  };
}
```

**Run:**
```bash
arion up
arion ps
arion logs -f backend
```

### 8.3 Static Web Server

```nix
# containers/nginx-static.nix
{ pkgs ? import <nixpkgs> {} }:
with pkgs;
dockerTools.buildImage {
  name = "nginx-static";
  contents = [
    nginx
  ];

  copyToRoot = pkgs.buildEnv {
    name = "site-root";
    paths = [ ./static ];
  };

  config = {
    Cmd = [ "nginx" "-g" "daemon off;" ];
    ExposedPorts = {
      "80/tcp" = {};
    };
  };
}
```

### 8.4 Build Environment (buildNixShellImage)

```nix
# containers/dev-shell.nix
{ pkgs ? import <nixpkgs> {} }:
pkgs.dockerTools.buildNixShellImage {
  name = "choiros-dev";
  pkg = pkgs.rustPackages.stable.rust;
  extraBuildInputs = with pkgs.rustPackages.stable; [
    cargo
    rustc
    rustfmt
    clippy
  ];
  shellHook = ''
    export RUST_LOG=debug
    echo "ChoirOS dev shell ready!"
  '';
}
```

**Use:**
```bash
docker run -it choir-os-dev bash
# Inside container:
cargo build
cargo test
cargo clippy
```

---

## 9. Challenges and Limitations

### 9.1 Learning Curve

**Challenge:** Nix expression language is more complex than Dockerfile syntax
**Mitigation:**
- Start with simple `buildImage` examples
- Use templates and flakes
- Leverage community packages

### 9.2 Image Size

**Challenge:** Nix images can be larger than Alpine for simple binaries
**Mitigation:**
- Use `buildLayeredImage` for better sharing
- Consider static linking with musl
- Use Alpine as base if appropriate

### 9.3 Cross-Platform Builds

**Challenge:** Tier 2+ platforms have limited CI
**Mitigation:**
- Set up remote builders
- Use distributed builds
- Test on target platforms early

### 9.4 Docker Daemon Still Needed

**Challenge:** Loading images into Docker still requires Docker daemon
**Mitigation:**
- Use Podman as daemonless alternative
- Consider pure OCI with ociTools
- Use Nix-snapshotter for store-based approach

### 9.5 Ecosystem Integration

**Challenge:** Some tools assume Dockerfile workflows
**Mitigation:**
- Use Arion for Docker Compose compatibility
- Maintain Dockerfile for external tooling
- Extend tooling to support Nix builds

---

## 10. Recommendations

### 10.1 When to Use Nix for Containers

**Use Nix when:**
- Building complex applications with many dependencies
- Need reproducible builds across environments
- Want programmatic image generation
- Multi-architecture builds required
- Already using Nix/NixOS ecosystem

**Consider Docker when:**
- Building simple static binaries
- Existing Dockerfile ecosystem integration critical
- Team unfamiliar with Nix
- Alpine base sufficient for requirements

### 10.2 Best Practices

1. **Start Simple**: Begin with `dockerTools.buildImage`
2. **Use Flakes**: Manage dependencies declaratively
3. **Leverage Cache**: Use Cachix or private cache
4. **Test Thoroughly**: Validate images match Docker equivalents
5. **Document Migration**: Keep Dockerfiles for reference during transition
6. **Iterate Gradually**: Don't migrate everything at once
7. **Measure Impact**: Track build times, image sizes, deploy reliability

### 10.3 For ChoirOS

**Recommended Approach:**

**Phase 1 (Immediate):**
- Create `containers/sandbox.nix` for ChoirOS sandbox
- Build and compare with Docker image
- Measure size, build time, startup time

**Phase 2 (Short-term):**
- Add Arion for multi-service orchestration
- Set up multi-arch builds (x86_64 + aarch64)
- Configure private Cachix cache

**Phase 3 (Long-term):**
- Migrate all services to Nix-based images
- Set up remote builders for cross-compilation
- Consider Nix-snapshotter for production deployments

---

## 11. Further Reading

- [Nixpkgs Manual: dockerTools](https://nixos.org/manual/nixpkgs/stable/#sec-pkgs-dockerTools)
- [NixOS Wiki: Docker](https://nixos.wiki/wiki/Docker)
- [NixOS Wiki: Podman](https://nixos.wiki/wiki/Podman)
- [Cheap Docker Images with Nix](https://lucabrunox.github.io/2016/04/cheap-docker-images-with-nix_15.html)
- [Arion Documentation](https://docs.hercules-ci.com/arion/)
- [Nix Flakes Guide](https://nixos.wiki/wiki/Flakes)

---

## Appendix A: Complete ChoirOS Example

```nix
# containers/choiros-complete.nix
{ pkgs ? import <nixpkgs> {} }:
let
  inherit (pkgs) lib;

  # Helper function for user creation
  createUser = name: pkgs.runCommand "create-user-${name}" {
    buildInputs = [ pkgs.shadow-utils ];
    text = ''
      #!/bin/sh
      ${pkgs.dockerTools.shadowSetup}
      groupadd -r ${name}
      useradd -r -g ${name} -d /data -M ${name}
      mkdir -p /data
      chown ${name}:${name} /data
    '';
  };

  # Helper for static file serving
  serveStatic = path: pkgs.writeTextDir "static-root" path;

in pkgs.dockerTools.buildImage {
  name = "choiros/sandbox";
  tag = "v0.1.0";

  # Core runtime
  contents = with pkgs; [
    sandbox
    sqlite
    coreutils
    bash
    (createUser "sandbox")
  ];

  # Static web UI
  copyToRoot = pkgs.buildEnv {
    name = "choiros-static";
    paths = [ (serveStatic ./dioxus-desktop/dist) ];
  };

  config = {
    User = "sandbox";
    WorkingDir = "/data";

    Cmd = [ "sandbox" ];

    Env = [
      "DATABASE_URL=/data/events.db"
      "RUST_BACKTRACE=1"
      "RUST_LOG=choiros=debug,sandbox=info"
    ];

    ExposedPorts = {
      "8080/tcp" = {};
    };

    Volumes = {
      "/data" = {};
      "/nix/store" = {};
    };

    Labels = {
      "org.opencontainers.image.title" = "ChoirOS Sandbox";
      "org.opencontainers.image.description" = "Per-user ChoirOS instance";
      "org.opencontainers.image.version" = "0.1.0";
      "org.opencontainers.image.source" = "https://github.com/choiros/choiros-rs";
    };

    Healthcheck = {
      Test = [ "CMD" "curl" "-f" "http://localhost:8080/health" ];
      Interval = 30;
      Timeout = 3;
      Retries = 3;
      StartPeriod = 40;
    };
  };

  # Reproducible creation date
  created = lib.substring 0 8 (
    lib.lastModifiedDate (lib.cleanSource ./.)
  );

  # Metadata
  meta = {
    description = "ChoirOS Sandbox - Per-user ChoirOS instance";
    maintainers = [ "choiros-team" ];
    platforms = lib.platforms.linux;
  };
}
```

**Build:**
```bash
nix-build containers/choiros-complete.nix
docker load < result
docker run -p 8080:8080 -v choir-data:/data choir-os/sandbox:v0.1.0
```

---

**Document Status:** Research Complete
**Next Steps:** Create proof-of-concept for ChoirOS sandbox
