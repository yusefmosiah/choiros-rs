# Snapshot Portability Across VM Sizings

Date: 2026-03-11
Kind: Research Note
Status: Active
Priority: 3
Requires: [ADR-0014, ADR-0018]

## Narrative Summary (1-minute read)

We investigated whether cloud-hypervisor and Firecracker VM snapshots can be
restored with different vCPU counts or memory sizes than when they were taken.
The answer is: cloud-hypervisor probably supports it (config.json is explicitly
documented as editable between snapshot/restore), but it is undocumented
territory with no guarantees. Firecracker does not support it -- their docs
require identical software and hardware configuration. Our current code
unconditionally uses `--restore source_url=...` with no config overrides, so
a snapshot taken at 2 vCPU / 1 GB will always restore at 2 vCPU / 1 GB
regardless of the user's current machine class. Cross-sizing restore is
theoretically possible for cloud-hypervisor but would require careful
validation, and is not feasible for Firecracker without upstream changes.

## What We Found

### Cloud-Hypervisor

**Snapshot format.** A cloud-hypervisor snapshot directory contains three
artifacts:
- `config.json` -- full VM configuration (human-readable JSON)
- `state.json` -- device and vCPU register state
- `memory-ranges` -- guest RAM pages

**Official docs say config.json is editable.** The snapshot_restore.md doc
states: *"[config.json] is stored in a human readable format so that it could
be modified between the snapshot and restore phases to achieve some very
special use cases. But for most cases, manually modifying the configuration
should not be needed."* This is the closest thing to official support for
cross-sizing restore.

**No explicit constraints documented.** The docs do not say what can or
cannot be changed in config.json. There are no validation rules, error
messages, or compatibility matrices published for config modifications.

**Memory hotplug restore was broken, now fixed.** Issue #3165 reported that
ACPI-hotplugged memory regions were not recreated on restore. This was
fixed in PR #3208 (MemoryManager rework for migration). This suggests the
memory subsystem is now more flexible about configuration changes.

**vCPU state is tightly coupled.** The snapshot preserves per-vCPU register
state (GPRs, MSRs, FPU, LAPIC). Reducing vCPU count would leave orphaned
register state; increasing it would leave new vCPUs with no saved state.
The guest kernel's CPU topology (ACPI MADT table, `/sys/devices/system/cpu/`)
was established at boot and won't match a changed vCPU count.

**Memory size changes have two sub-problems:**
1. *Increasing memory:* The snapshot's memory-ranges file contains exactly
   the bytes for the original size. New pages would be uninitialized. The
   guest kernel's memory map (e820/ACPI) was set at boot and won't know
   about the extra memory without a hotplug event post-restore.
2. *Decreasing memory:* The snapshot contains pages that won't fit. The
   restore would either fail (can't map pages beyond the new limit) or
   silently lose data.

**Restore CLI.** Our nix config uses:
```
cloud-hypervisor --restore "source_url=file://${SNAPSHOT_DIR}" --api-socket "$API_SOCK"
```
The `--restore` flag accepts only `source_url` and an optional `prefault`
parameter. There is no config override mechanism at the CLI level -- changes
must go through editing config.json in the snapshot directory before restore.

**Verdict: theoretically possible for cloud-hypervisor, but high risk.**
Editing config.json to change vCPU count would likely crash or hang the guest
(CPU topology mismatch). Editing memory size would require the guest to
discover the new memory via ACPI hotplug after resume, which may or may not
work depending on the kernel. Neither path is tested or documented upstream.

### Firecracker

**Snapshots require identical configuration.** The official snapshot-support.md
states: *"Snapshots must be resumed on software and hardware configuration
which is identical to what they were generated on."*

**No config override on /snapshot/load.** The Firecracker `/snapshot/load` API
accepts a snapshot file, a memory backend config, and optionally
`enable_diff_snapshots`. There is no parameter to override vCPU count or
memory size.

**Cross-architecture compatibility is a tracked feature request.** Issue #2941
asks for snapshot portability across different architectures. It is labeled
"Roadmap: Tracked" but has not been implemented as of 2026-03.

**GIC version must match (ARM64).** On aarch64, the GIC version (v2 vs v3)
must be identical between snapshot and restore.

**Verdict: not supported.** Firecracker explicitly requires identical
configuration. Cross-sizing restore would require upstream changes to the
Firecracker snapshot format and restore path.

### microvm.nix

microvm.nix generates the `microvm-run` script that becomes the
cloud-hypervisor (or firecracker) command line. The machine class system
(ADR-0014 Phase 6) writes `machine-vcpu` and `machine-memory-mb` files
to the VM state directory, and the nix boot script applies these via sed
to the generated run script.

**These overrides only apply to cold boot.** The restore path
(lines 349-354 of ovh-node.nix) bypasses the entire microvm-run generation
and sed-override pipeline, jumping straight to `--restore source_url=...`.
Any machine class change made after the snapshot was taken is silently
ignored on restore.

### Our Code

**Snapshot lifecycle** (hypervisor/src/sandbox/systemd.rs):
- `hibernate()`: pauses VM, calls `vm.snapshot` API, saves to
  `{state_dir}/vm-snapshot/`, stops the process chain
- `ensure()` / `prepare_vm_state()`: checks for `vm-snapshot/state.json`,
  sets `boot-mode=restore` if present
- `stop()`: deletes `vm-snapshot/` (hard stop = no snapshot preserved)

**No config editing on restore.** The code never reads or modifies the
snapshot's `config.json`. The `ch_api_snapshot()` function writes the
snapshot directory; the nix boot script reads it verbatim on restore.

**Class mismatch handling** (docs/theory/notes/2026-03-11-deferred-machine-class-items.md):
The deferred-items doc already identifies this gap: "Invalidate the VM
snapshot (snapshots are not portable across hypervisors)." The proposed
fix is stop + invalidate + cold boot, not cross-sizing restore.

**ADR-0014 section 1.5** states: *"VM snapshots are valid only within the
same machine class AND same nix generation."*

## Feasibility Assessment

### Can we implement cross-sizing snapshot restore?

**vCPU count changes: No (practical).** Even if cloud-hypervisor accepts the
edited config.json, the guest kernel's CPU topology is baked at boot. Adding
or removing CPUs post-restore would require the guest to support CPU hotplug
AND for the hypervisor to correctly synthesize ACPI hotplug events after
restore. This is fragile, untested, and the failure mode is a hung or
panicking guest.

**Memory size increases: Maybe (cloud-hypervisor only).** If we edit
config.json to increase memory, cloud-hypervisor might accept it. The guest
would see the original memory at restore and could potentially discover new
memory via ACPI hotplug. However:
- This requires `hotplug_size` to be set in the original config
- Guest kernel must have ACPI memory hotplug support
- We would need to trigger a `vm.resize` API call after resume
- No upstream testing or documentation for this path
- Firecracker: not possible at all

**Memory size decreases: No.** The snapshot memory file contains pages for
the full original size. Restoring into a smaller memory space would lose
data or fail to map.

**Same-class restore (current): Works well.** The existing approach of
invalidating snapshots on class change and cold booting is correct and safe.

### Cost-benefit analysis

The engineering cost of cross-sizing restore is high (config.json editing,
post-restore hotplug orchestration, guest kernel requirements, per-hypervisor
code paths, extensive testing matrix) for marginal benefit. A cold boot takes
8-14 seconds. Class changes are rare (account tier upgrades). The current
"invalidate snapshot and cold boot" approach handles this correctly.

## What To Do Next

1. **Keep the current approach.** Snapshot invalidation on class mismatch is
   the right design. Cross-sizing restore is not worth the complexity.

2. **Implement the deferred auto-migration item.** When `ensure_running`
   detects a class mismatch, stop + invalidate + cold boot with the new
   class. This is already documented in the deferred items note.

3. **Implement generation-aware snapshot invalidation.** Currently snapshots
   are invalidated on every hypervisor restart, not just on nix generation
   changes. The fix (compare `/nix/var/nix/profiles/system` target) is
   straightforward and recovers fast-restore for config-only restarts.

4. **If memory upsizing becomes critical later**, investigate the
   cloud-hypervisor `vm.resize` API as a post-restore step. This would
   let a restored VM gain additional memory without a full reboot, but
   only for cloud-hypervisor and only for memory increases. File this as
   a future research item, not a current priority.

5. **Do not pursue cross-sizing for Firecracker.** Their snapshot contract
   explicitly requires identical configuration. Wait for upstream progress
   on issue #2941.

## Sources

- [cloud-hypervisor snapshot_restore.md](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/snapshot_restore.md)
- [cloud-hypervisor memory hotplug restore fix (PR #3208)](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/3165)
- [cloud-hypervisor virtiofs restore issue #6931](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6931)
- [cloud-hypervisor memory docs](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/memory.md)
- [Firecracker snapshot-support.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md)
- [Firecracker snapshot compatibility issue #2941](https://github.com/firecracker-microvm/firecracker/issues/2941)
- [Firecracker snapshot versioning](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/versioning.md)
