# Grind to Prod Release Flow

## Narrative Summary (1-minute read)

The release process is grind-first and closure-based: build and validate on grind, then copy
the exact Nix store closures to prod and switch binaries atomically. This removes
"green CI but broken prod" drift from ad-hoc host state and makes prod reproducible from a
single release manifest.

The infrastructure model is two identical hosts with different traffic roles:

- `grind`: active development, pre-release validation, release build origin.
- `prod`: non-interactive runtime node, updated only from promoted closures.

Both hosts should share the same Nix module graph and differ only in explicit host inputs
(hostname, network identity, DNS, secrets materialization).

This runbook introduces scripts that:

1. Build release outputs on grind and record exact store paths.
2. Copy those store paths from grind to prod without rebuilding.
3. Apply the release on prod by updating `/opt/choiros/bin/*` symlinks.
4. Capture host state snapshots for drift debugging.
5. Fail fast if deploy store paths are missing/unrealizable unless host fallback is explicitly enabled.

## What Changed

1. Added release manifest builder: `scripts/ops/build-release-manifest.sh`.
2. Added release applier for hosts: `scripts/ops/apply-release-manifest.sh`.
3. Added promotion script: `scripts/ops/promote-grind-to-prod.sh`.
4. Added state snapshot tool: `scripts/ops/host-state-snapshot.sh`.
5. Added Just recipes for build/promotion/snapshot operations.
6. Defined two-host operations policy (identical nodes, active/passive role flip).

## What To Do Next

1. Run grind E2E checks and generate a release manifest from a clean commit.
2. Promote that exact release to prod with `promote-grind-to-prod.sh`.
3. Capture and archive grind/prod state snapshots on every release and every incident.
4. Move host definitions into `nixosConfigurations` so system state (not just app binaries) is
   declarative.
5. Split CI policy into "validate" and "manual release" lanes (details below).

## Nix 101 (ChoirOS)

If you are new to Nix, keep this mental model:

1. Source commit: immutable app code state in git.
2. Derivation: the exact build recipe for an output.
3. Store path: content-addressed output under `/nix/store/<hash>-name`.
4. Closure: a store path plus all transitive runtime dependencies it needs.
5. Activation: point runtime symlinks/services at those exact store paths.

Why closures matter for deploy:

1. You build once on grind.
2. You copy the exact closures to prod.
3. Prod does not recompile different bits and does not drift.

Quick closure inspection commands:

```bash
# Build and print output path without mutating ./result symlink.
nix --extra-experimental-features nix-command --extra-experimental-features flakes \
  build ./hypervisor#hypervisor --no-link --print-out-paths

# Show closure size.
nix path-info -Sh /nix/store/<hash>-hypervisor-*

# Show transitive dependency tree.
nix-store -q --tree /nix/store/<hash>-hypervisor-*
```

## Closure Promotion Diagram

```text
git commit (main)
      |
      v
grind: nix build (sandbox/hypervisor/desktop)
      |
      v
store paths + full closures in /nix/store
      |
      | nix copy --from ssh://grind --to ssh://prod <paths...>
      v
prod: exact same closures present
      |
      v
apply-release-manifest -> /opt/choiros/bin/* -> systemd restart/health checks
```

## Operating Model (Two Identical Servers)

1. Host parity:
   - Same Nix modules and service graph on both hosts.
   - Differences allowed only through explicit host vars (DNS, host keys, machine identity).
2. Role assignment:
   - One node is active traffic target.
   - One node is standby/canary target for promotion rehearsal and rollback readiness.
3. Promotion model:
   - Build once on grind.
   - Copy exact closures to prod.
   - Switch symlinks and restart services.
4. Drift policy:
   - No interactive config edits on prod.
   - No mutable source-of-truth repo on prod.
   - Drift is detected via `host-state-snapshot.sh`, then fixed by declarative redeploy.

## Source of Truth Rules

1. Canonical source of truth is git + flake + release manifest.
2. `prod` is runtime-only; it may keep a read-only checkout for diagnostics but must not be used
   as the deploy authority.
3. Mutable data is explicit and scoped to runtime directories (`/opt/choiros/data` or
   `/var/lib/choir`) with backups before schema-affecting releases.

## CI and Release Policy

Target policy:

1. Validation CI:
   - Runs on pull requests.
   - Verifies formatting, lint, tests, and buildability.
2. Release CI / CD:
   - Manual trigger only (`workflow_dispatch`) for hypervisor release promotion.
   - Produces release manifest and auditable artifact metadata.
3. Runtime evolution:
   - Hypervisor upgrades are explicit manual releases.
   - Sandbox and desktop behavior can evolve inside Choir runtime paths without requiring every
     app iteration to be a full host release.

Until CI trigger changes are merged, assume push-to-main behavior may still run and treat it as
validation, not authoritative promotion.

## Authoritative Flow

1. On grind, sync and validate:
   - `git fetch --all --prune`
   - `git checkout main && git pull --ff-only`
   - `just grind-check`
   - Playwright smoke against grind URL.
2. Build release manifest on grind:
   - `./scripts/ops/build-release-manifest.sh`
   - Optional cache sanity check first: `just cache-check`
3. Promote exact outputs to prod:
   - `./scripts/ops/promote-grind-to-prod.sh --grind <grind-host> --prod <prod-host>`
4. Capture snapshots on both hosts:
   - `./scripts/ops/host-state-snapshot.sh --output /var/log/choiros/state-<host>.env`
5. Verify:
    - `curl -fsS https://os.choir-ip.com/health`
    - Run public Playwright hypervisor project.

## Failover and Rollback

1. Keep previous manifest snapshots under `/opt/choiros/backups/`.
2. For rollback, re-apply the previous manifest with
   `scripts/ops/apply-release-manifest.sh <previous-manifest>`.
3. Re-run health checks and Playwright smoke.
4. Capture post-rollback host snapshot for incident records.

## Drift Debugging Standard

When prod behavior differs from grind, compare snapshots first. The snapshot includes:

- host generation and `nixos-version`
- repo `HEAD` and dirty status
- resolved binary paths + SHA256 hashes
- service active/substate/result
- local health checks and hypervisor `EnvironmentFiles`

If binary hashes differ, release drift occurred. If hashes match but behavior differs, inspect
runtime data (`/opt/choiros/data`) and secrets wiring.
