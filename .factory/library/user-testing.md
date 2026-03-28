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

## Flow Validator Guidance: Command-Line/Shell

**Isolation Context:**
- All assertions share the same repo root: `/Users/wiz/choiros-rs`
- All assertions use the same `cogent_repo_serve` service on `127.0.0.1:4242`
- State directory: `/Users/wiz/choiros-rs/.cogent`
- Evidence directory: `/Users/wiz/.factory/missions/a61db9bf-5a3d-4916-8c3f-6b1c137aa282/evidence/runtime-packaging/<group-id>/`

**Shared Resources:**
- The `cogent_repo_serve` service is already running (DO NOT restart)
- Use `COGENT_STATE_DIR=/Users/wiz/choiros-rs/.cogent` for all cogent commands
- Repository state is read-only for validation (DO NOT modify tracked files)

**Testing Tools:**
- Shell commands: `test`, `ls`, `git`, `rg`, etc.
- Cargo: `cargo test`, `cargo run`, `cargo check`, etc.
- Nix: `nix flake show`, `nix build`, `nix eval`, etc.
- SQLite: `sqlite3` for database inspection

**Constraints:**
- DO NOT modify repository files
- DO NOT restart the cogent_repo_serve service
- DO NOT create `.cagent/` or modify home/global cogent state
- Save all evidence files to your assigned evidence directory
- For Nix builds that produce `result` symlinks, capture evidence before cleanup
