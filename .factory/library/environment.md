# Environment

## Build Requirements

- Rust toolchain (stable, via rustup or nix)
- cargo, rustc, rustfmt, clippy
- just
- nix

## Mission-Specific Notes

- This mission is a **repo-local hard cutover** from `.cagent/` to `.cogent/` in `choiros-rs`.
- Do not change `~/cogent`.
- Do not remove the ATLAS system in this mission.
- Do not push; pushing to `main` triggers deployment.
- Heavy validators should run sequentially on this machine.

## Git Notes

- Pushing to main triggers GitHub Actions → OVH deployment to draft.choir-ip.com
- Do NOT push without explicit user approval
- The docs-flattening batch has already been committed; preserve it
- The remaining pre-mission dirtiness is `.cagent/supervisor.json`, which this mission removes

## Validation Notes

- Safe Nix validation uses `--no-write-lock-file` when the goal is read-only checking
- Final package/vendor-hash refresh must use an actual `nix build`
- Repo-root `cogent work list/ready` validation currently requires a repo-local `cogent serve` instance on `127.0.0.1:4242` with `COGENT_STATE_DIR=/Users/wiz/choiros-rs/.cogent`
- Pre-existing workspace clippy failure in `hypervisor/src/jobs.rs` is in scope for this mission and must be fixed
- Expensive live-model tests are explicitly disabled by default for this mission; post-deploy/push verification is the later place to run them

## Target Repo State Layout

- `.cogent/cogent.db` — repo-visible work graph database
- `.cogent/cogent-private.db` — private notes database (gitignored)
- `.cogent/raw/`, `.cogent/jobs/`, `.cogent/transfers/`, `.cogent/debriefs/` — runtime churn (gitignored)
- no `.cogent/supervisor.json`
