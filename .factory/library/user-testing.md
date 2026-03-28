# User Testing

## Validation Surface

This mission has no browser or TUI testing surface. Validation is through command-line, file-state, and grep-based assertions.

Primary validation surfaces:
- repo-local state under `.cogent/`
- Rust runtime command surface
- repo worker bootstrap entrypoint
- Nix package + VM package definitions
- hooks and retained scripts
- root/operator docs

The installed `cogent` CLI currently routes `work list/ready` through `cogent serve`, so runtime validation must start the `cogent_repo_serve` service from `.factory/services.yaml` before repo-root `cogent work *` checks or `repo_worker_bootstrap --dry-run`.

Live/costly model-eval tests are not part of this mission's acceptance execution. They must be disabled by default so workspace testing is offline-safe; live verification is deferred until a later post-deploy/push workflow.

### Automated Validation

- `cargo check --workspace --locked`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`
- `nix flake check --no-build --no-write-lock-file`
- `nix build .#packages.x86_64-linux.cogent` when a reachable `x86_64-linux` builder exists; otherwise use an equivalent temporary `x86_64-linux` Nix build proof locally and defer the exact attrpath build to post-push/GitHub verification
- scoped `rg` sweeps for stale active-surface references

### Manual Validation

- Start the `cogent_repo_serve` service from `.factory/services.yaml` before runtime command checks
- Verify `.cogent/` exists and `.cagent/` does not
- Verify `cogent.db` / `cogent-private.db` exist under `.cogent/`
- Verify `supervisor.json` is absent
- Verify `repo_worker_bootstrap --repo <repo> --dry-run` returns JSON on the renamed surface
- Verify the built package exposes `result/bin/cogent` via the exact attrpath build when possible, or via an equivalent temporary `x86_64-linux` Nix build proof when local builder capacity is unavailable
- Verify active-surface stale-reference checks are clean

## Active-Surface Grep Scope

Use scoped grep checks over:
- `README.md`
- `CLAUDE.md`
- `AGENTS.md`
- `sandbox/src/`
- `hypervisor/src/` only when relevant to the clippy fix
- `flake.nix`
- `nix/`
- `.githooks/`
- `scripts/`
- `.gitignore`
- active operator docs touched by the mission

Do not use archive-wide grep as a hard gate for this mission.

## Validation Concurrency

### Resource Cost

- Heavy validators are local but non-trivial
- `nix flake check --no-build` can consume roughly 3.4 GiB RSS on this machine
- Run top-level validators sequentially
- Max concurrent validators: `1`
