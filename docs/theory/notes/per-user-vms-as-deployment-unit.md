# Per-User VMs as the Deployment Unit

Date: 2026-03-08
Kind: Note
Status: Draft
Priority: 3
Requires: [ADR-0014, ADR-0016]

## Narrative Summary (1-minute read)

We currently think about deployment in terms of nodes: "deploy to Node A,"
"deploy to Node B." But the target architecture has per-user VMs that
hibernate when idle and restore on demand. In that world, the VM is the
deployment unit — not the node. Nodes are interchangeable capacity.
This reframes rolling deploys, upgrades, rollback, and even what
"staging" means.

## What Changed

The virtio-blk migration (2026-03-08) proved that VM snapshot/restore
works. A VM can hibernate with full state, then resume instantly. This
unlocks per-user VMs as a practical primitive, not a theoretical one.

## The Insight

Per-user VMs with hibernate/restore give us the same properties that
container orchestrators achieve with rolling deploys — but at a stronger
isolation boundary (full VM) and with a simpler mental model:

- **Upgrading a user**: hibernate old VM → boot new VM with new nix
  generation → user continues with new code
- **Rollback**: re-hibernate new VM → restore old VM → user is back
  on the previous version, with their previous state intact
- **Canary deploys**: upgrade 10% of users → monitor → proceed or rollback
- **Safe exploration**: user can have multiple VMs (different branches,
  different configs) and switch between them

The `/nix/store` virtiofs mount is the mechanism: each VM sees whatever
store generation the host has. A running VM has its binary in memory, so
it's not affected by host-side changes. New VMs pick up new store paths
naturally.

## Nodes Are Capacity, Not Environments

The current setup:
- Node A = "production" (choir-ip.com)
- Node B = "staging" (draft.choir-ip.com)

The target:
- Node A and Node B are interchangeable hosts in a 2-node fleet
- Both run the same NixOS config, same nix store
- User VMs are pinned to one node (no cross-node migration yet)
- "Staging" means "deploy the new build to both nodes, but only
  cold-boot test VMs with it — leave user VMs on their current generation
  until validated"

This eliminates the confusion of "which node has what." Both nodes
have everything. The question becomes: which generation is each user's
VM running?

## CARGO_MANIFEST_DIR: The Anti-Pattern

There are 22 instances in sandbox code where missing env vars fall back
to `env!("CARGO_MANIFEST_DIR")` — a compile-time nix store path that
doesn't exist at runtime. Every one is a silent production bug.

The fix: remove all CARGO_MANIFEST_DIR fallbacks. If `CHOIR_SANDBOX_ROOT`
isn't set, panic immediately with a clear error. This is the "fail fast"
principle from CLAUDE.md: "if wiring is wrong, let the run fail loudly."

The fallbacks exist because the code was written for local development
first. In production, all paths must be explicit. The NixOS service
definition in sandbox-vm.nix sets the env vars. If they're missing,
something is wrong with the deployment — and we need to know immediately,
not discover it later as a mysterious "File not found" error.

## User Filesystem Layout

The user's experience inside their VM should feel like a Linux desktop:

```
~/                          ← user home dir
├── Desktop/                ← desktop app state (icons, layout)
├── Documents/              ← writer output, files the user saves
│   └── conductor/
│       └── runs/
│           └── {run_id}/
│               └── draft.md
├── .config/                ← user settings
│   └── choiros/
│       └── model-config.toml
└── .local/
    └── share/
        └── choiros/
            └── .writer_revisions/  ← internal, hidden from user
```

This is standard XDG. The sandbox root (`/opt/choiros/data/sandbox`)
maps to the user's home. The virtio-blk disk image contains this
entire tree. On snapshot/restore, all of it is preserved.

What this means for the current code:
- `CHOIR_SANDBOX_ROOT` = the user's home (on virtio-blk)
- Terminal CWD should default to `~/` (the sandbox root)
- Files app "Home" should show `~/` contents
- `.writer_revisions/` should be in `.local/share/choiros/`, not at root
- `config/model-catalog.toml` should be platform-provided (in nix store
  or a read-only config share), with user overrides in `~/.config/choiros/`
- Desktop state (currently event-sourced in memory) could optionally
  have a filesystem representation in `~/Desktop/`

## Model Config: Platform vs User

Model configuration has two layers:

**Platform level** (operator-controlled, read-only to user):
- Available models and their provider routing
- Rate limits, cost budgets, usage caps
- Default model assignments per role (conductor, writer, etc.)
- Delivered via: nix store path (bundled in sandbox derivation) or
  read-only virtiofs share

**User level** (user-controlled):
- Model preferences ("I prefer Sonnet for writing")
- Custom prompts or system instructions
- Stored in: `~/.config/choiros/model-config.toml` on the virtio-blk disk

The runtime merges both: platform defaults + user overrides. The platform
config is the same across all VMs (it's in the nix store). User config
is per-VM (it's on their disk image).

## Encrypted VMs (Future)

The user mentioned: "it's better that the host can't inspect user files.
In future we look to encrypt the whole VM."

This is the right direction. With virtio-blk, the `data.img` file can
be encrypted (LUKS or dm-crypt). The key management options:

1. **User-held key**: User provides passphrase on first boot, VM
   derives encryption key. Host never sees plaintext data. Strongest
   privacy but requires user interaction on every cold boot.

2. **Platform-held key**: Encryption key in platform secrets, unlocked
   automatically. Protects against disk theft but not platform operator.

3. **Key escrow**: User key + platform recovery key. Balances privacy
   with operational recovery.

This is future work but the architecture supports it: the virtio-blk
disk is an opaque file from the host's perspective. Adding encryption
is a guest-side change only.

## Build Efficiency

Current: each sub-flake builds workspace deps independently (3x redundant).
Target: unified flake with shared `cargoArtifacts` (build deps once).

Additional optimization: build on one node, `nix copy` closure to the other.
Currently both nodes build independently. With `nix copy`:
- Node B builds (staging) → validate → `nix copy --to ssh://node-a` → switch
- Node A never builds, just receives pre-built closures
- Build time goes from 2x to 1x

Even better with a binary cache (Cachix, FlakeHub, or self-hosted Attic):
- CI builds closure → pushes to cache → both nodes pull from cache
- Neither node builds; both just download
- But this requires CI to run nix (DeterminateSystems/nix-installer-action)

For now with 2 nodes: build on Node B, copy to Node A. Simple and effective.

## Open Questions

1. **VM migration**: Can a user's VM move between nodes? Not yet (virtiofs
   mounts are node-local). Would require shared storage or a migration
   protocol. Not needed for 2 nodes.

2. **Multiple VMs per user**: The architecture supports it (each VM is
   a separate `data.img` + runner). UI for switching between them is
   undesigned.

3. **VM boot time budget**: Cold boot is ~6s. Is that acceptable for
   "switch to a different branch" UX? Hibernate/restore is instant.
   Could pre-warm VMs for anticipated switches.

4. **Disk space**: Each user VM has a 2GB `data.img`. With 100 users
   that's 200GB. With hibernated VMs (3GB memory dump each), 500GB+.
   The 1TB NVMe RAID gives ~900GB usable. Need to plan capacity.

5. **GC coordination**: When is it safe to GC old nix store paths?
   Only when no VM (running or hibernated) references them. Need a
   registry of active VM generations.
