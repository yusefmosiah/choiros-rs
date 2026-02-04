# NixOS Coding Agent Sandboxes: Merged Research Summary and Plan

**Goal**
Design a secure, cost-conscious sandbox infrastructure for untrusted Rust code and long-lived ractor services. It must support fast event-driven tasks, stable actor workloads, and a unified macOS + remote control plane using Nix.

**Primary Constraints**
- Untrusted code execution (Rust with `unsafe` and FFI is in-scope risk)
- Mixed workloads: short-lived tasks and long-lived ractor services
- Multi-sandbox per user and high density
- Unified workflow across macOS dev and remote production
- Transition path: AWS credits now, OVH (or other bare metal) later

**Core Requirements**
- Fast cold starts for event-driven tasks
- Stable, stateful execution for ractor actors
- Strong isolation boundaries
- Declarative reproducibility (Nix)
- Cost-aware scaling and density

---

**Architecture Overview**
A single source of truth for sandbox definitions, realized in two modes:
- Container mode for non-metal AWS (Podman; optionally gVisor for extra isolation)
- MicroVM mode for bare metal (Firecracker or Cloud Hypervisor)

A Rust orchestrator exposes a narrow backend trait boundary and compiles against either backend via feature flags. The same sandbox NixOS module builds either an OCI image or a MicroVM definition.

**Key Decision**
Treat MicroVMs as "cattle" by default, with the ability to promote to "pets" for long-lived ractor services.

---

**Isolation Technology Tradeoffs (Summary)**

| Technology | Isolation | Startup | Performance | Fit |
|---|---|---|---|---|
| Podman (runc/crun) | Namespaces/cgroups | Fastest | Near-native | Best for AWS non-metal dev |
| Podman + gVisor | Syscall interception | Fast | CPU/IO overhead | Extra defense-in-depth |
| Firecracker | MicroVM | ~100-200ms | Near-native | Best for short-lived tasks |
| Cloud Hypervisor | MicroVM | ~150-250ms | Near-native | Best for long-lived services |
| Kata (CH/QEMU) | VM | Slower | Near-native | K8s style environments |

**Implications for Ractor**
- gVisor overhead can impact syscall-heavy async workloads.
- Firecracker is excellent for ephemeral tasks but limited for shared filesystem needs.
- Cloud Hypervisor supports more VM flexibility (e.g., hotplug) and better fits long-lived actor services.

---

**Unified Nix Design**
Single flake drives macOS dev and Linux hosts:
- `nix-darwin` + `home-manager` for macOS
- `nixosConfigurations` for AWS/OVH
- One sandbox module used to build:
  - OCI images (container mode)
  - MicroVM definitions (microvm.nix)

**Compile-Time Backend Switch (Rust)**
- Feature-gated modules select backend at build time
- Optional runtime override for emergency rollback

Example shape:
```
#[cfg(feature = "microvm")]
mod backend { pub use microvm_backend::*; }

#[cfg(feature = "container")]
mod backend { pub use podman_backend::*; }
```

---

**Deployment Path**

**Phase 1: AWS Credits (non-metal)**
- Use NixOS AMI + Podman
- Optional gVisor for higher-risk workloads
- Focus on orchestration + sandbox definition stability

**Phase 2: AWS Metal (optional, ARM)**
- a1.metal is ARM (aarch64). Firecracker is supported.
- Pros: lower cost than large x86 metal
- Cons: cross-compile or build on-host; memory density is limited

**Phase 3: OVH Bare Metal**
- Same host config, better price/performance
- MicroVMs with KVM and shared store
- Scale density and long-lived service reliability

---

**MicroVM Strategy**

**Ephemeral Pool (short tasks)**
- Firecracker with pre-warmed snapshots
- Fast restore for event-driven jobs

**Persistent Tier (long-lived ractor services)**
- Cloud Hypervisor with writable overlay
- Disable ballooning for stability
- CPU pinning for predictable latency

**Storage and Sharing**
- Use shared Nix store from host where supported
- Prefer Cloud Hypervisor when host/guest file sharing is required

---

**Control Plane and Tooling**
- Rust orchestrator with two backends
- Nix for reproducible sandbox images/VMs
- Colmena or nixos-anywhere for deploys
- macOS dev experience via nix-darwin and (optionally) Lima/vfkit

---

**Cost Notes (Directional)**
- Large AWS metal is expensive and burns credits quickly
- a1.metal is viable but ARM-specific and memory-constrained
- OVH dedicated is typically a better long-term density target

---

**Open Questions to Resolve Early**
- Target architecture: x86_64 vs aarch64
- Required host<->guest filesystem sharing
- Expected sandbox density per host
- Which workloads require gVisor vs full MicroVM isolation
- Snapshot strategy for ephemeral pool

---

# Plan for Next Steps

**1. Confirm Requirements and Targets (1-3 days)**
1. Decide primary architecture (x86_64 or aarch64).
2. Define workload split: ephemeral vs long-lived.
3. Specify sharing requirements (workspace mounts, Nix store sharing).

**2. Establish the Nix Flake Skeleton (3-5 days)**
1. Create single sandbox module.
2. Add container realization (OCI image build).
3. Add microvm.nix realization (MicroVM config).
4. Add macOS dev flake (nix-darwin + home-manager).

**3. Build the Orchestrator Interface (1-2 weeks)**
1. Define `SandboxBackend` trait and `SandboxHandle` API.
2. Implement container backend (Podman).
3. Add feature-gated microvm backend stub.
4. Build two derivations via Nix features.

**4. AWS Dev Phase (1-2 weeks)**
1. Deploy NixOS AMI + Podman.
2. Run load tests and measure cold-start time.
3. Validate multi-sandbox concurrency and resource limits.

**5. MicroVM Phase (2-4 weeks)**
1. Stand up bare-metal host (OVH or AWS metal).
2. Implement MicroVM backend.
3. Validate snapshot pool for event-driven tasks.
4. Validate long-lived ractor service stability.

**6. Production Hardening (ongoing)**
1. Monitoring and per-sandbox cost attribution.
2. Security gates for untrusted workloads.
3. Bin-packing / scheduling optimization.
4. Documentation for operators and users.

---

If you want, I can turn this into concrete files and code scaffolding (flake, modules, orchestrator trait, and a container backend) in this repo.
