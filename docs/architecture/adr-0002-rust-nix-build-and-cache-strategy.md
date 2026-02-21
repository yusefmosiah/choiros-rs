# ADR-0002: Rust + Nix Build and Cache Strategy

Date: 2026-02-20  
Status: Draft  
Owner: ChoirOS runtime and deployment

## Narrative Summary (1-minute read)

ChoirOS should standardize on `crane` with a pinned Rust toolchain for Nix builds.
This gives reproducible builds with better CI cache reuse than plain `buildRustPackage`
while keeping normal Cargo development ergonomics in `nix develop`.

For binary cache infrastructure, we should start with a managed service to move fast
(`FlakeHub Cache` first choice, `Cachix` second choice) and defer self-hosting until
compliance, cost, or control requirements clearly justify operational overhead.

## What Changed

1. Added a concrete Rust-in-Nix decision for Phase 6c/6d/6e implementation.
2. Defined a fallback path to reduce adoption risk.
3. Added an explicit binary cache posture for CI and deployment rollout.

## What To Do Next

1. Implement `sandbox/flake.nix` using this ADR (`crane` + pinned toolchain).
2. Reuse the same pattern for `desktop/flake.nix` and `hypervisor/flake.nix`.
3. Add GitHub Actions cache integration with managed binary cache.
4. Re-evaluate self-hosted cache after 30 days of CI telemetry.

## Context

Phase 6 requires three flakes (`sandbox`, `desktop`, `hypervisor`) and reproducible,
cross-platform Rust builds (Mac dev + Linux deploy). We need a build strategy that is:

1. Reproducible via lockfiles and pinned toolchains.
2. Fast in CI with effective dependency artifact reuse.
3. Maintainable by a small fast-moving team.
4. Compatible with standard Cargo local workflows.

## Options Considered

1. `rustPlatform.buildRustPackage` only
2. `naersk`
3. `crane`
4. `crate2nix` / `dream2nix`

For cache infrastructure:

1. Managed binary cache (`FlakeHub Cache`, `Cachix`)
2. Self-hosted (`Attic`)
3. DIY store cache (`S3 + HTTP cache plumbing`)
4. GitHub Actions cache only

## Decision

### Rust + Nix build approach

Primary:

- Use `crane` with pinned Rust toolchain (`fenix` or `rust-overlay`) in all three flakes.

Fallback:

- Use `makeRustPlatform { rustc, cargo }` + `buildRustPackage` for components that
  need a lower-level nixpkgs-native path.

### Binary cache approach

Primary:

- Start with managed cache (prefer `FlakeHub Cache` for GitHub-first/OIDC flow).

Fallback:

- Use `Cachix` if policy or workflow fit is better.

Deferred:

- Self-host `Attic` only when compliance, data residency, cost, or policy controls
  materially require it.

## Why

1. `crane` improves CI throughput through better dependency artifact reuse across
   build/test/lint targets.
2. Pinned toolchain reduces Darwin/Linux drift and clarifies debugging boundaries.
3. Managed cache removes early infrastructure toil and secret/key management burden.
4. Fallback path keeps risk bounded when edge crates or platform quirks appear.

## Consequences

### Positive

- Faster CI on Rust workspace builds.
- Clear and explainable build architecture across `sandbox`, `desktop`, `hypervisor`.
- Lower early operational burden while deployment architecture stabilizes.

### Negative

- Added flake composition complexity versus basic `buildRustPackage`.
- Managed cache introduces vendor coupling and service dependency.
- Requires discipline around lockfile updates and toolchain pinning.

## Implementation Notes

1. All three flakes should expose:
   - `packages.<system>.<component>`
   - `devShells.<system>.default`
   - optional `checks.<system>.*` for CI parity
2. Keep local development workflow Cargo-first (`nix develop` then `cargo ...`).
3. Prefer native builds per target architecture over heavy cross-compilation.
4. Keep cache write permissions limited to trusted CI contexts.

## Re-evaluation Triggers

Revisit this ADR if any of the following become true:

1. Managed cache cost exceeds operating a self-hosted alternative.
2. Compliance requires customer-controlled cache storage/signing.
3. Cross-platform build failures indicate `crane` fit issues for critical components.
4. CI build times do not materially improve after rollout.

## Acceptance Criteria

1. `nix build .#sandbox` succeeds from `sandbox/flake.nix`.
2. `nix build .#desktop` succeeds from `desktop/flake.nix`.
3. `nix build .#hypervisor` succeeds from `hypervisor/flake.nix`.
4. CI uses binary substitution from managed cache for repeated builds.
5. Team can explain and operate the build system without ad hoc scripts.

## References

- `https://github.com/ipetkov/crane`
- `https://crane.dev/getting-started.html`
- `https://github.com/nix-community/fenix`
- `https://github.com/oxalica/rust-overlay`
- `https://docs.determinate.systems/flakehub/cache/`
- `https://docs.cachix.org/`
- `https://github.com/zhaofengli/attic`
