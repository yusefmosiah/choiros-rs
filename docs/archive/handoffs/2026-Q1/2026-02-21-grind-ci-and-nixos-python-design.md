# Grind CI Loop + NixOS Python Strategy (Design)

Date: 2026-02-21  
Owner: runtime/deploy  
Status: proposed (implementation paused pending design alignment)

## Narrative Summary (1-minute read)

We are switching from push-and-pray to a grind-first release loop: make changes on the grind
host, run the same checks CI will run, rebuild runtime with `nixos-rebuild switch` when host
configuration changes are needed, then commit/push once.

The immediate blocker discovered during this loop is tooling ergonomics on NixOS for ad-hoc
editing helpers (`python3` unavailable in the base PATH). The best approach is to keep runtime
host state immutable and predictable, while using Nix-provided ephemeral or shell-scoped Python
for scripting work.

## What Changed

1. We confirmed the grind host execution path works end-to-end via SSH alias + dedicated key:
   - `ssh choiros-grind ...`
   - `just grind-check` from local runs remote `cargo check` via Nix shells.
2. We started a small feature candidate on grind only (not committed/pushed):
   - `sandbox/src/api/mod.rs`: extend `/health` with `instance_role` and `hostname`.
   - `sandbox/tests/desktop_api_test.rs`: assert new fields exist.
3. We paused before full CI-equivalent command sweep to produce this design and reduce churn.

## What To Do Next

1. Align on Python execution policy for grind host (recommended below).
2. Apply the policy consistently in grind scripts/commands.
3. Complete CI-equivalent checks on grind.
4. If host config was modified, run `nixos-rebuild switch`, then re-run health checks.
5. Commit and push once checks are green.
6. Verify GitHub `Nix CI/CD` run is green.
7. Add a tiny observable UI/API feature and verify live on grind.

## Design Goals

- Keep production reliability high by proving changes in a prod-like grind host first.
- Keep host configuration declarative and reproducible (NixOS-first).
- Minimize mutable host drift and one-off shell snowflakes.
- Ensure local and remote operators can run identical, documented commands.

## Scope

In scope:
- Grind-host-first implementation and validation workflow.
- CI parity checks needed for confidence.
- Python-on-NixOS execution patterns for scripting/editing helpers.

Out of scope:
- Re-architecting deployment pipeline.
- Broad lint/clippy debt unrelated to current feature work.

## Proposed Workflow (Authoritative)

1. Sync grind workspace to `origin/main`.
2. Make code change on grind in `/opt/choiros/workspace`.
3. Run fast correctness loop first:
   - `cargo fmt --check` (or format explicitly)
   - `nix develop ./sandbox --command cargo check -p sandbox`
   - targeted tests (exact binary/test, not broad filters)
4. Run CI-parity build loop:
   - `nix build ./sandbox#sandbox`
   - `nix build ./hypervisor#hypervisor`
   - `nix build ./dioxus-desktop#desktop`
5. If host system config changed, apply `sudo nixos-rebuild switch` and verify service health.
6. Commit + push from grind only after all checks pass.
7. Watch GitHub Actions and verify one clean run.

## Python on NixOS: Best-Approach Research

### Options considered

1. Install `python3` globally in host PATH and rely on it for ad-hoc scripts.
2. Use ephemeral Python for one-off commands:
   - `nix shell nixpkgs#python3 --command python3 ...`
3. Use project shell Python for repeatable work:
   - `nix develop ... --command python3 ...`
4. Define a dedicated helper app/script in flake outputs for stable scripted operations.

### Recommendation

Use a two-tier policy:

- Tier A (default): use `nix develop` or `nix shell` for Python invocations.
- Tier B (if frequent): promote recurring scripts into flake `apps` or `devShell` commands.

Rationale:
- Preserves NixOS declarative integrity and avoids hidden host drift.
- Keeps script runtime pinned by Nix inputs for reproducibility.
- Avoids coupling day-to-day engineering work to mutable host package state.

### Concrete command patterns

One-off script:

```bash
nix --extra-experimental-features nix-command \
    --extra-experimental-features flakes \
    shell nixpkgs#python3 --command \
    python3 - <<'PY'
print("hello from nix python")
PY
```

Project-scoped script:

```bash
nix --extra-experimental-features nix-command \
    --extra-experimental-features flakes \
    develop ./sandbox --command python3 scripts/some_helper.py
```

## Proposed Guardrails

- Do not depend on bare `python3` existing in host PATH.
- Prefer repo scripts and checked-in commands over ad-hoc multiline SSH snippets.
- Keep test runs exact/targeted (`./scripts/sandbox-test.sh --test ...`).
- Keep commits atomic: one feature/fix + tests per commit.

## Current Candidate Feature (Small, Observable)

Add deployment context to `/health` response:
- `instance_role`: from `CHOIROS_INSTANCE_ROLE` (fallback `unknown`)
- `hostname`: from `HOSTNAME` (fallback `unknown`)

Why this feature:
- Small blast radius.
- Easy to validate locally, on grind, and in prod.
- Useful for live debugging and distinguishing `sandbox-live` vs `sandbox-dev` context.

## Validation Plan

1. Run `just grind-check`.
2. Run targeted health endpoint test:
   - `./scripts/sandbox-test.sh --test desktop_api_test test_health_check`
3. Run three Nix builds matching CI.
4. Curl grind endpoints post-activation and confirm new fields are present.

## Rollback Plan

- Code rollback: revert commit on branch before merge.
- Host rollback (if system config changed): `sudo nixos-rebuild switch --rollback`.
- Runtime restart if needed:
  `sudo systemctl restart container@sandbox-live container@sandbox-dev hypervisor`
