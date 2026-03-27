# Implementing ADR-0018: Drop Virtiofs, Adaptive Capacity

Date: 2026-03-09
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0018]

## Narrative Summary (1-minute read)

Virtiofs eliminated, erofs store disk active, KSM deduplicating, adaptive
watchdog and capacity gate deployed. Both nodes promoted. Per-VM cost
dropped from ~514 MB to ~443 MB. Next step: virtio-pmem to eliminate the
~100 MB guest page cache overhead (double caching) for a projected ~330 MB
per-VM.

## Phase Status

```
Phase 1 (swap + capacity gate)      DONE — deployed Node B 2026-03-09
Phase 2 (adaptive idle watchdog)    DONE — deployed Node B 2026-03-09
Phase 3 (erofs store disk)          DONE — microvm.nix auto-generates with shares=[]
Phase 4 (drop virtiofs + KSM)       DONE — shared=off, mergeable=on, THP=never
Phase 5 (verify KSM)               DONE — 1.7 GB saved at 58 VMs, 6.6x dedup ratio
Phase 6 (load test)                 DONE — 16 users, 100% pass, report written
Phase 7 (virtio-pmem + FSDAX)      PLANNED — eliminate double caching
```

## Phases 1-6: Completed

See load test report: `docs/state-report-2026-03-10-adr-0018-load-test.md`

### Key Implementation Details (for reference)

**Erofs store disk (Phase 3):** No custom image needed. Setting `shares=[]`
in `nix/ch/sandbox-vm.nix` triggers microvm.nix to auto-generate an erofs
disk from the VM's nix store closure. This is a single file in
`/nix/store/` on the host, added as a `--disk readonly=on` virtio-blk
device. The guest initrd mounts it at `/nix/store`.

**Credential injection (Phase 4):** Gateway token flows:
```
hypervisor writes state_dir/gateway-token
  → cloud-hypervisor@ reads file, seds into --cmdline
  → guest /proc/cmdline contains choir.gateway_token=<TOKEN>
  → choir-extract-cmdline-secrets oneshot extracts it
  → /run/choiros-sandbox.env written
  → choir-sandbox.service reads via EnvironmentFile
```

**KSM requires THP=never (Phase 5):** cloud-hypervisor calls
MADV_HUGEPAGE on VM memory. With THP=madvise, this creates 2 MB hugepages
that KSM cannot merge. Must set THP=never system-wide. VMs started before
the change must be restarted.

**Files modified:**
- `nix/ch/sandbox-vm.nix` — `shares=[]`, cmdline secret extraction oneshot
- `nix/hosts/ovh-node.nix` — virtiofsd removed, KSM+THP tmpfiles, cmdline injection
- `hypervisor/src/sandbox/systemd.rs` — writes gateway-token file
- `hypervisor/src/sandbox/mod.rs` — passes token to ensure(), capacity gate, adaptive watchdog
- `flake.nix` — removed squashfs derivation and virtiofsd overlay

### Issues Encountered and Resolved

1. **fileSystems conflict:** microvm.nix auto-generates erofs mount for
   `/nix/store`. Our custom squashfs mount conflicted. Fix: removed custom
   squashfs entirely (ef9d599).

2. **Failed to start Find NixOS closure:** Custom squashfs disk prepended
   before erofs disk, changing `/dev/vd*` ordering. microvm initrd couldn't
   find its closure. Fix: let microvm handle everything (ef9d599).

3. **KSM pages_shared = 0 after 52 scans:** THP=madvise + cloud-hypervisor
   MADV_HUGEPAGE = 2 MB pages KSM can't merge. Fix: THP=never (655dae2).

---

## Phase 7: Virtio-PMEM + FSDAX (Planned)

### Problem: Double Caching

With virtio-blk + erofs, the nix store data exists in two caches:

| Cache | Location | Reclaimable? | Size |
|-------|----------|-------------|------|
| Host page cache | Host RAM, managed by host kernel | Yes | ~100-500 MB shared |
| Guest page cache | Inside VM's 1024 MB allocation | Only by guest kernel | ~100 MB per VM |

This is why per-VM RSS is ~443 MB instead of the projected ~170 MB.
The guest page cache eats ~100 MB of each VM's RAM allocation for
nix store data that's already cached on the host.

### Solution: Virtio-PMEM with FSDAX

Virtio-pmem maps a host file directly into the guest's physical address
space as a PCI BAR (persistent memory device). With erofs FSDAX, the
guest reads files by accessing host pages directly through EPT — no
guest page cache allocation, no data copies.

```
Current (virtio-blk + erofs):
  Guest read → block I/O → virtqueue → host reads file → copies to guest RAM
  Data copies: 2 (host cache + guest cache)

Phase 7 (virtio-pmem + erofs FSDAX):
  Guest read → memory load → EPT translation → host mmap'd page
  Data copies: 1 (host page cache only, shared across all VMs)
```

### Step 7a: Check microvm.nix virtio-pmem support

The microvm.nix module may already support virtio-pmem for the store disk.
Check the module's options for a `storeOnPmem` or similar flag.

```bash
# On Node B, check what the generated microvm-run script does
cat /nix/store/...-microvm-run/bin/microvm-run | grep -E '(pmem|disk|store)'
```

If microvm.nix doesn't support pmem natively, we need to modify the
cloud-hypervisor@ ExecStart in `ovh-node.nix` to:
1. Remove the erofs `--disk` entry from the generated microvm-run script
2. Add `--pmem file=<erofs-image>,discard_writes=on` instead

### Step 7b: Build uncompressed erofs image

FSDAX requires uncompressed erofs. The microvm.nix module may build
compressed erofs by default. Check and override if needed:

```nix
# In sandbox-vm.nix or flake.nix, if microvm supports it:
microvm.storeOnDisk = {
  format = "erofs";
  compression = "none";  # Required for FSDAX
};
```

If microvm.nix doesn't expose this, we may need to build the erofs image
ourselves with `mkfs.erofs` (no compression flag).

### Step 7c: Modify cloud-hypervisor@ to use --pmem

**File:** `nix/hosts/ovh-node.nix`

In the sed transforms for the cloud-hypervisor@ ExecStart, replace the
erofs `--disk` with `--pmem`:

```bash
# After copying and modifying microvm-run:
# Remove the erofs --disk line and add --pmem instead
EROFS_IMAGE=$(grep -oP '(?<=path=)\S+\.erofs' "${STATE_DIR}/.microvm-run")
sed -i '/\.erofs/d' "${STATE_DIR}/.microvm-run"

# Add --pmem before --api-socket
sed -i "s|--api-socket|--pmem file=${EROFS_IMAGE},discard_writes=on --api-socket|" \
  "${STATE_DIR}/.microvm-run"
```

### Step 7d: Guest mount with FSDAX

The guest kernel should detect the virtio-pmem device as `/dev/pmem0`.
The erofs filesystem needs to be mounted with DAX enabled.

Check if the microvm.nix initrd handles pmem devices automatically.
If not, the guest needs a filesystem entry:

```nix
# In sandbox-vm.nix
fileSystems."/nix/store" = {
  device = "/dev/pmem0";
  fsType = "erofs";
  options = [ "ro" "dax" ];
};
```

This may conflict with the microvm module's auto-generated mount. Use
`lib.mkForce` if needed.

### Step 7e: Verify and measure

```bash
# On Node B, after deploying Phase 7:

# 1. Verify pmem device exists in guest
ssh guest 'ls -la /dev/pmem*'

# 2. Verify FSDAX mount
ssh guest 'mount | grep nix/store'
# Should show: /dev/pmem0 on /nix/store type erofs (ro,dax)

# 3. Verify no guest page cache for nix store
ssh guest 'cat /proc/meminfo | grep -E "(Cached|Active.file)"'
# Cached should be much lower than before (~100 MB less)

# 4. Measure per-VM RSS from host
ps -eo rss,comm | grep cloud-hyperviso | \
  awk '{sum+=$1; n++} END {printf "avg: %d MB (n=%d)\n", sum/n/1024, n}'
# Should be ~330 MB instead of ~443 MB

# 5. Test snapshot/restore with pmem
ovh-runtime-ctl hibernate <instance>
ovh-runtime-ctl ensure <instance>
curl http://10.0.0.x:8080/health
```

### Step 7f: KSM behavior with pmem

With virtio-pmem, the nix store pages are in the host page cache (shared
across VMs), not in guest RAM. This means:

- KSM doesn't need to dedup nix store pages (they're already shared)
- KSM still deduplicates guest kernel + application pages
- Total KSM savings may be lower in absolute terms but per-VM RSS is lower
- The net effect is better density

### Risks and Rollback

- If FSDAX doesn't work with the microvm.nix-generated erofs image
  (e.g., compressed), we stay on virtio-blk (current, working)
- If snapshot/restore breaks with pmem, we stay on virtio-blk
- Rollback: revert the sed changes in ovh-node.nix (erofs --disk still works)

### Experiment Plan (Node B)

1. SSH into Node B, manually test `--pmem` with one VM
2. If it works, update nix config and rebuild
3. Run heterogeneous load test to validate
4. Measure per-VM RSS and compare to current ~443 MB
5. If validated (< 350 MB per-VM), promote to Node A

---

## Files to Modify (Phase 7)

| File | Change |
|------|--------|
| `nix/hosts/ovh-node.nix` | cloud-hypervisor@ sed: replace erofs --disk with --pmem |
| `nix/ch/sandbox-vm.nix` | Possibly add /nix/store FSDAX mount (if initrd doesn't handle) |
| `flake.nix` | Possibly override erofs compression to none |

## What NOT to Do

- Don't build a separate erofs image — microvm.nix already handles it
- Don't remove KSM — it still helps with non-nix-store pages
- Don't skip THP=never — still needed for KSM on guest RAM pages
- Don't test on Node A first — use Node B for experiments
