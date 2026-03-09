# Implementing ADR-0018: Drop Virtiofs, Adaptive Capacity

Date: 2026-03-09
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0018]

## Narrative Summary (1-minute read)

Replace virtiofs with a shared read-only squashfs nix-store image
(virtio-blk), enable KSM for memory deduplication, add adaptive idle
watchdog with memory-pressure awareness, add a capacity gate, and
configure 16 GB swap. Target: 3x VM capacity (58 → 170+ concurrent).

## Phase Status

```
Phase 1 (swap + capacity gate)      TODO — safety net first
Phase 2 (adaptive idle watchdog)    TODO — memory-pressure-aware hibernation
Phase 3 (nix-store squashfs image)  TODO — build image in flake, add to VM config
Phase 4 (drop virtiofs)             TODO — remove shares, shared=off, mergeable=on
Phase 5 (verify KSM)               TODO — measure pages_shared, per-VM RSS
Phase 6 (load test)                 TODO — heterogeneous test at new capacity
```

## Phase 1: Swap + Capacity Gate

### Step 1a: Add 16 GB swapfile

**File:** `nix/hosts/ovh-node-b-disks.nix`

```nix
swapDevices = [{
  device = "/swapfile";
  size = 16384;  # 16 GB in MB
}];
```

This creates `/swapfile` on first `nixos-rebuild switch`. NixOS handles
`mkswap` and `swapon` automatically.

### Step 1b: Capacity gate in ensure_running

**File:** `hypervisor/src/sandbox/mod.rs`

Before spawning a new VM in `ensure_running()`, check:

```rust
/// Check system capacity before spawning a new VM.
fn check_capacity(entries: &HashMap<String, UserSandboxes>) -> Result<(), String> {
    // Count running VMs
    let running: usize = entries.values()
        .map(|u| {
            u.roles.values().filter(|e| e.status == SandboxStatus::Running).count()
            + u.branches.values().filter(|e| e.status == SandboxStatus::Running).count()
        })
        .sum();

    const MAX_VMS: usize = 50;
    if running >= MAX_VMS {
        return Err(format!(
            "Server at capacity ({running}/{MAX_VMS} VMs). Please try again in 30 seconds."
        ));
    }

    // Check available memory from /proc/meminfo
    if let Ok(avail_mb) = read_available_memory_mb() {
        const MIN_AVAILABLE_MB: u64 = 1024;  // 1 GB minimum
        if avail_mb < MIN_AVAILABLE_MB {
            return Err(format!(
                "Insufficient memory ({avail_mb} MB available). Please try again in 30 seconds."
            ));
        }
    }

    Ok(())
}
```

Add to middleware: return 503 with `Retry-After: 30` header when
capacity check fails.

## Phase 2: Adaptive Idle Watchdog

**File:** `hypervisor/src/sandbox/mod.rs`

Replace the fixed-timeout watchdog loop with pressure-aware logic:

```rust
fn effective_idle_timeout(&self) -> Duration {
    let avail_pct = match read_memory_percent_available() {
        Ok(pct) => pct,
        Err(_) => return self.idle_timeout,  // fallback to configured
    };

    if avail_pct > 60 {
        self.idle_timeout                    // normal: 30 min
    } else if avail_pct > 30 {
        Duration::from_secs(300)             // warning: 5 min
    } else if avail_pct > 15 {
        Duration::from_secs(30)              // high: 30 sec
    } else {
        Duration::from_secs(0)               // critical: immediate
    }
}
```

At critical pressure (<15% available), also force-hibernate the N
least-recently-active VMs needed to reclaim memory, regardless of
idle duration.

Helper to read `/proc/meminfo`:

```rust
fn read_available_memory_mb() -> anyhow::Result<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo")?;
    for line in contents.lines() {
        if line.starts_with("MemAvailable:") {
            let kb: u64 = line.split_whitespace().nth(1)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("parse error"))?;
            return Ok(kb / 1024);
        }
    }
    Err(anyhow::anyhow!("MemAvailable not found"))
}

fn read_memory_percent_available() -> anyhow::Result<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo")?;
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line.split_whitespace().nth(1)
                .and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            avail_kb = line.split_whitespace().nth(1)
                .and_then(|s| s.parse().ok()).unwrap_or(0);
        }
    }
    if total_kb == 0 { return Err(anyhow::anyhow!("no MemTotal")); }
    Ok(avail_kb * 100 / total_kb)
}
```

## Phase 3: Nix-Store Squashfs Image

### Step 3a: Build squashfs in flake.nix

**File:** `flake.nix`

Add a new package that builds a squashfs image from the sandbox VM's
nix store closure:

```nix
packages.${system}.nix-store-image = let
  # Get the closure of the sandbox NixOS system
  sandboxSystem = self.nixosConfigurations.choiros-ch-sandbox-live
    .config.system.build.toplevel;
in pkgs.stdenv.mkDerivation {
  name = "nix-store-squashfs";
  nativeBuildInputs = [ pkgs.squashfsTools ];
  dontUnpack = true;
  buildPhase = ''
    # Export the closure paths
    nix-store --query --requisites ${sandboxSystem} > closure-paths.txt

    # Build squashfs from those paths
    mksquashfs $(cat closure-paths.txt) nix-store.squashfs \
      -comp lz4 -Xhc -no-xattrs -all-root \
      -root-becomes nix/store
  '';
  installPhase = ''
    mkdir -p $out
    mv nix-store.squashfs $out/
  '';
};
```

**Alternative (simpler):** Use `pkgs.makeSquashfs` or the
`closureInfo` pattern from nixpkgs. The key is that the squashfs
contains all nix store paths needed by the sandbox VM, mounted so
the guest sees them at `/nix/store/`.

### Step 3b: Deploy squashfs to host

The squashfs image needs to be on the host at a known path. Options:

**Option A (nix store):** Reference it in the NixOS config as a store
path. `nixos-rebuild switch` pulls it into `/nix/store/` automatically.
The cloud-hypervisor unit reads the path from config.

**Option B (fixed path):** Copy to `/opt/choiros/nix-store.squashfs`
during deploy. Simpler for the systemd unit to reference.

Recommended: **Option A** — stays in nix store, no manual copy step.
The path is injected into the cloud-hypervisor unit via the NixOS
config (same as vmRunnerLive today).

## Phase 4: Drop Virtiofs, Enable KSM

### Step 4a: Remove virtiofs shares from guest config

**File:** `nix/ch/sandbox-vm.nix`

```nix
# BEFORE:
shares = [
  { proto = "virtiofs"; tag = "nix-store"; ... }
  { proto = "virtiofs"; tag = "choiros-creds"; ... }
];

# AFTER:
shares = [];  # All virtiofs shares removed (ADR-0018)
```

### Step 4b: Add nix-store squashfs as second virtio-blk

**File:** `nix/ch/sandbox-vm.nix`

```nix
volumes = [
  {
    image = "data.img";
    mountPoint = "/opt/choiros/data/sandbox";
    size = 2048;
  }
  # Nix-store squashfs image — shared read-only across all VMs
  # Cloud-hypervisor --disk readonly=on,path=/nix/store/.../nix-store.squashfs
  # Guest mounts as squashfs at /nix/store
];
```

The microvm.nix module may not natively support a shared read-only
squashfs. If not, add the `--disk` flag directly in the
cloud-hypervisor ExecStart script (Phase 4d).

### Step 4c: Mount squashfs as /nix/store in guest

**File:** `nix/ch/sandbox-vm.nix`

Add a guest fstab entry or systemd mount for the squashfs block device:

```nix
fileSystems."/nix/store" = {
  device = "/dev/vdb";  # second virtio-blk device
  fsType = "squashfs";
  options = [ "ro" ];
};
```

The first virtio-blk is `/dev/vda` (data.img), second is `/dev/vdb`
(nix-store.squashfs).

### Step 4d: Modify cloud-hypervisor ExecStart

**File:** `nix/hosts/ovh-node.nix`

In the cloud-hypervisor@ service ExecStart, replace the sed-based
microvm-run approach with a direct cloud-hypervisor invocation that:

1. Removes `--fs` (no more virtiofs sockets)
2. Changes `--memory` to `shared=off,mergeable=on,size=1024M`
3. Adds second `--disk` for the squashfs image: `readonly=on,path=<squashfs-path>`

```bash
exec cloud-hypervisor \
  --cpus boot=2 \
  --memory shared=off,mergeable=on,size=1024M \
  --kernel $KERNEL --initramfs $INITRD --cmdline "$CMDLINE" \
  --disk path=data.img,readonly=off \
        path=$NIX_STORE_IMAGE,readonly=on \
  --net mac=$VM_MAC,tap=$TAP \
  --api-socket $API_SOCK \
  --serial tty --console null --watchdog --seccomp true
```

### Step 4e: Remove virtiofsd@ service dependency

**File:** `nix/hosts/ovh-node.nix`

- Remove `requires = [ "virtiofsd@%i.service" ]` from cloud-hypervisor@
- Remove `after = [ "virtiofsd@%i.service" ]` from cloud-hypervisor@
- Remove the virtiofsd@ service definition entirely
- Remove the virtiofs socket wait loop from ExecStart

### Step 4f: Credential injection via kernel cmdline (IMPLEMENTED)

Without virtiofs, the gateway token can no longer reach the guest via
a mounted EnvironmentFile. Solution: inject via kernel cmdline.

**Flow:** hypervisor writes token to state dir → cloud-hypervisor@ unit
reads it and appends `choir.gateway_token=<TOKEN>` to `--cmdline` →
guest oneshot extracts from `/proc/cmdline` → writes `/run/choiros-sandbox.env`
→ sandbox service reads via `EnvironmentFile`.

**Files changed:**
- `hypervisor/src/sandbox/systemd.rs`: `ensure()` writes `gateway-token` file
- `hypervisor/src/sandbox/mod.rs`: passes `provider_gateway_token` to `ensure()`
- `nix/hosts/ovh-node.nix`: cloud-hypervisor@ reads token file, seds into cmdline
- `nix/ch/sandbox-vm.nix`: `choir-extract-cmdline-secrets` oneshot + EnvironmentFile

The sandbox reads `CHOIR_PROVIDER_GATEWAY_TOKEN` from its process
environment (see sandbox/src/actors/model_config.rs:531).

## Phase 5: Verify KSM

After deploying Phases 3-4, verify KSM is working:

```bash
# On Node B, after VMs boot with shared=off,mergeable=on:
cat /sys/kernel/mm/ksm/run          # should be 1
cat /sys/kernel/mm/ksm/pages_scanned # should be > 0 (within minutes)
cat /sys/kernel/mm/ksm/pages_shared  # should be > 0 (identical pages found)
cat /sys/kernel/mm/ksm/pages_sharing # should be > pages_shared (many-to-one)

# Per-VM RSS should drop from ~338 MB to ~170 MB after KSM converges
ps -eo rss,comm | grep cloud-hyperviso | awk '{sum+=$1; n++} END {printf "avg: %d MB\n", sum/n/1024}'
```

## Phase 6: Load Test

Re-run the heterogeneous load test with the new configuration:

```bash
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
  npx playwright test heterogeneous-load-test.spec.ts --project=hypervisor
```

Compare against the Phase 4 (pre-ADR-0018) baseline:
- Per-VM RSS (expect ~170 MB vs 338 MB)
- virtiofsd count (expect 0 vs 172)
- Max concurrent VMs before pressure
- VM boot time (expect ~8-10s vs ~12s)
- KSM pages_shared / pages_sharing

## Files to Modify (Summary)

| File | Phase | Change |
|------|-------|--------|
| `nix/hosts/ovh-node-b-disks.nix` | 1 | Add 16 GB swapfile |
| `nix/hosts/ovh-node-a-disks.nix` | 1 | Add 16 GB swapfile |
| `hypervisor/src/sandbox/mod.rs` | 1,2 | Capacity gate + adaptive watchdog |
| `flake.nix` | 3 | Nix-store squashfs image package |
| `nix/ch/sandbox-vm.nix` | 4 | Remove shares, add squashfs volume, update mounts |
| `nix/hosts/ovh-node.nix` | 4 | Remove virtiofsd@, update cloud-hypervisor@ |

## What NOT to Do

- Don't build per-user squashfs images — one shared image for all VMs
- Don't add the squashfs to the data.img — it's a separate read-only disk
- Don't keep virtiofsd "just in case" — it's the entire memory bottleneck
- Don't skip swap — it's the cheapest safety net
- Don't set swap too small — 16 GB on 444 GB free disk is negligible
