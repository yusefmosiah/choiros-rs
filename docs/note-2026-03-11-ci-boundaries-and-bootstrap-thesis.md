# CI Boundaries and the Bootstrap Thesis

Date: 2026-03-11
Kind: Note
Status: Draft
Priority: 2
Requires: [ADR-0016, ADR-0023, ADR-0024]

## Narrative Summary (1-minute read)

The `microvm.nix` transport work was correct, but the way we tested it
temporarily stepped outside the normal git -> CI -> staging path. That
created avoidable ambiguity: when Node B later misbehaved, it was harder to
separate code defects from host drift and runtime state left behind by
out-of-band validation.

The operational lesson is simple: infra experiments are allowed, but they
must converge back into the CI-controlled deployment path quickly or be
killed. This is especially important for runtime and deployment work, where
"just one manual canary" can leave state that the normal system does not know
how to reason about.

In parallel, the Go work is not just a language preference experiment. The
deeper requirement is separation of concerns: the current hypervisor owns too
many unrelated jobs. Decomposing those jobs into separately owned services,
then gradually feature-flagging Rust ownership off in favor of Go services, is
the cleaner path.

The hypervisor side is still useful bootstrap work, but the sandbox rewrite is
the stronger milestone. When Choir can rewrite its own runtime behavior
through its own deployment, testing, and promotion machinery, that is the
deeper proof that the platform can rewrite itself.

## What Changed

- 2026-03-11: captured lessons from the `microvm.nix` transport experiment,
  the failed AWS nested-KVM spike, and the Node B recovery that followed.
- 2026-03-11: connected ADR-0024 to the broader Choir bootstrap goal.
- 2026-03-11: refined the Go direction from monolithic replacement to
  service extraction behind feature flags.

## What To Do Next

1. Keep `microvm.nix` work branch-based, pinned, and CI-replayable.
2. Treat manual host-side overrides as diagnostics only, not as a delivery lane.
3. Use hypervisor decomposition as the first operational Go lane, but treat
   sandbox replacement as the stronger long-term bootstrap milestone.

---

## The Immediate Lesson from `microvm.nix`

The transport work itself was good:

- `blk` and `pmem` now both exist as first-class store transports
- Cloud Hypervisor and Firecracker both build against that matrix
- the `microvm.nix` fork is pinned from `choiros-rs`

What went wrong was not the idea. It was the path:

- manual Node B canaries from dirty worktrees
- temporary host-local reconciliation
- repo state and host state diverging during validation
- CI resuming after the fact, rather than owning the entire promotion

That created exactly the kind of ambiguity infra should avoid:

- is the regression in the code?
- in the deploy workflow?
- in stale in-memory state?
- or in manual host drift left behind by experimentation?

The answer later turned out to be a real hypervisor state bug, but we had to
spend time disproving the more avoidable possibility: that staging was broken
because we had stepped outside the normal lane.

## Rule: Novel Infra Must Rejoin CI Fast

The rule should be:

- experiments may begin outside CI
- they must converge back into git-pinned, CI-replayable state quickly
- if they do not, they should be abandoned or reduced to local-only research

More concretely:

- a forked dependency is acceptable if it is pinned in `flake.lock`
- a new runtime path is acceptable if CI can deploy it from `main`
- a host-local override is acceptable only as a short-lived diagnostic
- a dirty worktree on staging is not an acceptable steady state

This is not bureaucracy. It is how we keep infra legible.

## The AWS `m8i` Attempt Was a Good Spike, and a Good Stop

The nested-KVM AWS experiment was reasonable:

- bursty capacity sounded attractive
- `m8i` looked like it might support the right path
- it was worth a short, bounded attempt

The result was also clear:

- too much compatibility work
- too much proprietary-system friction
- too much time and money for too little confidence

So the right conclusion is:

- nested AWS KVM is out of scope for now
- anyone who wants metavirtualization can carry their own downstream hacks
- Choir should focus on the path that is working: OVH bare metal, pinned Nix,
  CI deploys, explicit runtime classes

This is not failure. It is scope discipline.

## Why This Matters for ADR-0014

ADR-0014 is about per-user storage, runtime promotion, verification, and
rollback. That architecture depends on one hard operational precondition:

- the platform must know what state is authoritative

If staging behavior can depend on manual files or ad hoc host mutations, then
promotion semantics are muddy before the real per-user promotion system even
arrives.

So there is a direct connection:

- per-user promotion theory needs reproducible runtime classes
- reproducible runtime classes need CI-owned deploy state
- therefore infra experimentation must not create semi-secret deployment lanes

This is why "options" in infra must mean reproducible options, not artisanal
box state.

## The Bootstrap Thesis

The Go work has a larger meaning than "Go may be nicer for control-plane
code."

The first requirement is architectural: the current hypervisor is doing
multiple jobs that should be separately owned. If we do not separate those
concerns first, we are just rewriting accidental coupling.

So the cleaner path is:

- decompose the hypervisor into clear service boundaries
- move one boundary at a time to Go
- feature-flag Rust ownership off as parity is proven

That makes the control plane a bootstrap test in a narrow sense.

The narrow milestone is not:

- "the hypervisor was rewritten in Go"

The narrower meaningful milestone is:

- "Choir used its own deployment, test, and promotion machinery to replace
  parts of its control plane safely"

But the stronger milestone is:

- "Choir used Choir to rewrite Choir's runtime behavior"

That is why the sandbox rewrite matters more than the hypervisor rewrite from
the bootstrap point of view.

Meaning:

- the product can host the work
- the runtime can test the work
- the deployment system can stage the work
- the promotion system can approve and roll back the work

The hypervisor decomposition is therefore an enabling step. The sandbox
rewrite is closer to the real thesis.

## What To Write in the Other Docs

This idea belongs in notes first, not as a hard ADR claim.

Recommended placement:

- keep this document as the reflective note
- keep ADR-0024 focused on decomposition and service extraction, not a
  heroic one-shot rewrite
- mention the bootstrap significance briefly in ADR-0024 implementation work
  only if it stays operational and concrete
- keep ADR-0014 implementation focused on runtime classes, promotion,
  verification, and rollback

In other words:

- notes can carry the thesis
- ADRs should carry only the parts we are willing to defend as immediate design

## Practical Operating Rules

For future infra experiments:

- Prefer branch-based experiments with pinned dependencies.
- Land the minimum host/config changes needed to make CI reproduce the result.
- Do not leave staging on a dirty worktree.
- Do not let manual deployment become the de facto release path.
- If a platform spike cannot become CI-shaped quickly, stop spending time on it.

For the Go rewrite:

- treat decomposition as the first task
- treat feature-flagged ownership transfer as the migration mechanism
- treat E2E parity as the acceptance gate
- treat CI deployability as part of correctness
- treat rollback as mandatory, not optional
- treat sandbox replacement as the stronger bootstrap proof

## References

- [ADR-0016](/Users/wiz/choiros-rs/docs/adr-0016-nixos-declarative-deployment.md)
- [ADR-0016 Implementation](/Users/wiz/choiros-rs/docs/adr-0016-implementation.md)
- [ADR-0023](/Users/wiz/choiros-rs/docs/adr-0023-microvm-store-disk-transport-selection.md)
- [ADR-0024](/Users/wiz/choiros-rs/docs/adr-0024-hypervisor-go-rewrite.md)
- [ADR-0024 Implementation](/Users/wiz/choiros-rs/docs/adr-0024-implementation.md)
- [Per-User VMs as the Deployment Unit](/Users/wiz/choiros-rs/docs/note-per-user-vms-as-deployment-unit.md)
