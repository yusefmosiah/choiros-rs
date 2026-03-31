# Architecture

How the system works at a high level, and what matters for the cagent→cogent hard cutover mission.

## System Overview

ChoirOS is a 3-tier system:

```
Hypervisor (control plane)
Sandbox (per-user runtime)
Dioxus Desktop (web/WASM client)
```

For this mission, the important boundary is the **repo-local work-graph integration** inside `sandbox/` plus the Nix packaging that makes that CLI available in worker/sandbox environments.

## Repo Work-Graph Integration

The repository has a repo-local work graph that the sandbox runtime uses for self-directed dispatch.

Primary integration points:
- `sandbox/src/self_directed_dispatch.rs` — runtime selection + claim logic
- `sandbox/src/bin/repo_worker_bootstrap.rs` — repo-only bootstrap entrypoint
- `README.md`, `CLAUDE.md`, `AGENTS.md` — operator-facing work-graph entrypoints
- `.githooks/pre-commit` — doc/work alignment automation
- `flake.nix` + `nix/ch/sandbox-vm.nix` — package/build/install surfaces for the work-graph CLI

The work-graph CLI is a subprocess dependency. In the current local `cogent` runtime, repo-root `cogent work *` commands are served through repo-local `cogent serve`, so runtime validation must start that service against `/Users/wiz/choiros-rs/.cogent` instead of relying on home/global state. The runtime and operator docs must agree on the same command vocabulary.

## Current-to-Target Cutover

Current repo state before implementation:
- active repo surfaces still use `cagent`
- repo state currently lives under `.cagent/`
- active Nix packaging and VM definitions still expose `cagent`
- deprecated adapter-era script/config/docs surfaces still exist

Target state after this mission:
- active repo surfaces use `cogent`
- repo state lives under `.cogent/`
- active Nix packaging and VM definitions expose `cogent`
- deprecated adapter-era active surfaces are removed

## Target Post-Cutover State

The target architecture after this mission is:

- repo state root: `.cogent/`
- public repo work graph DB: `.cogent/cogent.db`
- private local DB: `.cogent/cogent-private.db`
- no `supervisor.json` in repo state
- runtime and packaging use the `cogent` binary name
- deprecated adapter-era config and script surfaces are removed from active repo paths

Tracked artifact content under the repo state root must survive the rename.

## Important Invariants

Workers should preserve these invariants:

1. **Hard cutover, not fallback**
   - This mission does not add or depend on legacy `.cagent` fallback support.
   - `~/cogent` is out of scope.
   - The repo-local state root must be `.cogent/`.

2. **Behavior-preserving rename**
   - Existing repo work-graph content must remain usable after the rename.
   - The rename should not silently create a fresh empty graph.

3. **Runtime/package alignment**
   - Runtime code, flake outputs, built binaries, and VM package definitions must all agree on `cogent`.
   - Worker VM and sandbox VM package surfaces are part of this alignment, not an afterthought.

4. **Active-surface cleanup only**
   - Remove deprecated adapter references from active code/config/scripts/operator docs.
   - Do not turn this mission into archive/history cleanup.
   - ATLAS removal itself is deferred; only rename fallout needed to keep ATLAS generation consistent is in scope.

5. **Validation gate must go green**
   - This mission includes fixing the pre-existing workspace clippy blocker in `hypervisor/src/jobs.rs`.
   - Final acceptance requires cargo + nix + stale-reference validation to pass together.

## Validation-Relevant Surfaces

The mission will validate these surfaces:

- filesystem state under repo root
- Rust runtime command surfaces
- repo worker bootstrap entrypoint
- Nix package / VM package definitions
- hooks and active scripts
- root/operator docs

Exact validator commands live in `.factory/services.yaml`; workers should use those commands rather than inventing alternatives.

No web UI is required for this mission. Runtime validation does require a temporary repo-local `cogent serve` instance on `127.0.0.1:4242`, and it must stay pinned to the repo `.cogent/` state.
