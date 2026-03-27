# ADR-0024: ChoirOS Go Rewrite — Hypervisor Decomposition and Sandbox Migration

Date: 2026-03-11
Kind: Decision
Status: Proposed
Priority: 2
Requires: [ADR-0007, ADR-0014, ADR-0021]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

ChoirOS is moving to Go. Not as a language preference experiment, but because
Rust compile times and memory requirements are incompatible with the build
pool model (ADR-0014) and the Choir-on-Choir bootstrap goal.

The hypervisor (~6.5k LOC) decomposes first — it's control-plane HTTP handlers
with clean external boundaries. The sandbox runtime migrates second — that's
the real bootstrap proof: Choir rewrites its own runtime using its own coding
agent, deployment, testing, and promotion machinery.

The Go rewrite removes BAML (replacing structured-output parsing with native
tool-use protocol), simplifies the agent loop to the standard call-execute
pattern, and enables sub-second builds in the shared pool. The Rust codebase
becomes the reference implementation — it can continue to evolve autonomously
or serve as a fallback.

Key concrete motivations:

- Build times are in the critical path of the development loop itself. When
  AI agents iterate every few minutes, a 30-second Rust build is a tax on
  every thought. Go gives sub-second rebuilds. This is not developer
  convenience — it is a multiplier on iteration speed. The right framing:
  "Go until you have a profiler trace that proves you need Rust."
- Rust cargo build: minutes, gigs of RAM. Cannot run in user VMs.
- Go build: seconds, hundreds of MB. Pool turns around updates fast.
- BAML adds token overhead, latency, and build complexity. Native tool-use
  is simpler and cheaper.
- The docs are the spec. When docs are correct and complete, a coding agent
  can execute the rewrite. This ADR and its guides are that spec.

## What Changed

- 2026-03-11: Added build-time critical path rationale, Go concurrency model
  section, corrected cagent/choir.go relationship, sandbox build-as-UX framing.
- 2026-03-11: Expanded from hypervisor-only to full ChoirOS Go migration path.
  Added sandbox rewrite as primary bootstrap target, BAML removal rationale,
  build pool implications, and the docs-as-spec principle.
- 2026-03-11: Refined from "rewrite the whole hypervisor" to "decompose and
  extract services behind feature flags."
- 2026-03-11: Initial ADR from Go feasibility study recommendations.

## What To Do Next

1. Complete ADR-0014 build pool and promotion infrastructure.
2. Carve explicit service boundaries inside the current hypervisor.
3. Introduce Go-owned hypervisor services behind feature flags.
4. Write the sandbox Go rewrite spec as implementation guides (the docs are
   the work queue for the coding agent).
5. Execute the sandbox rewrite using Choir's own coding agent and build pool.
6. Decommission Rust-owned paths incrementally as parity is proven.

---

## Context

The Go refactor feasibility study (`docs/state-report-go-refactor-feasibility-2026-03-09.md`)
recommended:

1. Do not rewrite solely for build times.
2. Use `hypervisor` as the first bounded Go experiment.
3. Treat `sandbox` as a protocol before treating it as a replacement target.

The hypervisor is the right first target because it contains multiple concerns
that should already be separately owned:

- auth and session edge
- runtime registry and route pointers
- VM lifecycle management
- provider gateway
- reverse proxy / sandbox routing
- deployment and readiness policy integration

The current Rust hypervisor is therefore not a bad codebase so much as an
overfull control-plane process. The design problem is separation of concerns
first, then language replacement second.

---

## Decision 1: Decompose Before Replacing

Do not run two whole hypervisors as the primary migration story. Instead:

1. define explicit internal service boundaries
2. extract one concern at a time behind a feature flag or routing switch
3. retire the Rust-owned implementation for that concern only after parity is
   proven

The first extraction targets should come from hypervisor concerns with the
cleanest contracts and lowest coordination burden.

Candidate extraction order:

- provider gateway
- runtime registry / promotion state
- VM lifecycle manager
- auth/session edge
- sandbox HTTP/WS proxy

This keeps migration comprehensible and avoids the operational confusion of
"which whole hypervisor is currently authoritative?"

---

## Decision 2: Introduce Go Services Incrementally

Go services should be added behind explicit feature flags or ownership flags.

Examples:

- `CHOIR_PROVIDER_GATEWAY_BACKEND=rust|go`
- `CHOIR_RUNTIME_REGISTRY_BACKEND=rust|go`
- `CHOIR_VM_LIFECYCLE_BACKEND=rust|go`

The Rust hypervisor can remain the ingress shell during decomposition while
delegating selected responsibilities to Go-owned services. Later, if it makes
sense, the ingress shell itself can also be replaced.

Behavioral parity remains the success criterion for each extracted boundary:
same API, same persistence semantics, same failure handling, same operational
runbooks.

### What stays the same during extraction

- Public HTTP routes and response shapes
- SQLite database schema (8 tables, same migrations)
- systemd and runtime-ctl integration
- deploy shape: Nix build, CI deploy, systemd-managed services
- rollback expectation: feature flag or service owner flip, not heroics

### What changes incrementally

- Ownership of one service boundary at a time moves from Rust to Go
- Feature flags control which implementation is authoritative
- The "hypervisor" becomes a composition of smaller services instead of one
  undifferentiated binary boundary

### Consequences

- Migration risk is paid one concern at a time
- Ownership boundaries become explicit before the full rewrite question is
  forced
- Rust and Go can coexist without requiring "two whole hypervisors"

---

## Decision 3: Behavioral Parity via E2E Testing

Each extracted service is correct when the existing Playwright E2E suite and
targeted stress tests pass with that service boundary owned by Go.

Testing strategy:

1. Run Playwright E2E tests against the normal deployment shape
2. Turn on one Go service boundary
3. Re-run the relevant end-to-end and stress suites
4. Compare: auth flow, sandbox boot, proxy behavior, provider gateway,
   WebSocket streaming, idle hibernation, rollback behavior

The system should not require modified E2E tests just because ownership moved
from Rust to Go.

### Consequences

- E2E remains the authority for user-visible behavior
- service extraction can be gated independently
- performance comparison can happen per service boundary, not only for an
  all-or-nothing rewrite

---

## Decision 4: Nix-Native Build and Deploy

Each Go service must be buildable via nix flake and deployable through the
existing CI pipeline. The deploy path must remain git-pinned and CI-owned.

```nix
# example service
provider-gateway-go = pkgs.buildGoModule {
  pname = "provider-gateway";
  version = "0.1.0";
  src = ./provider-gateway-go;
  vendorHash = "...";
};
```

The deploy flow remains: push to main -> CI deploys pinned code -> systemd
restarts or flips service ownership. No manual host drift is part of the
intended path.

### Consequences

- Same deploy pipeline, service-by-service language replacement
- Rollback is a feature-flag or service-owner change
- CI remains the source of truth during migration

---

## Decision 5: Sandbox Rewrite Is the Stronger Bootstrap Milestone

The sandbox runtime is not the first migration target in this ADR, but it is
the more meaningful long-term bootstrap target.

Why:

- the sandbox is where Choir's actual product runtime lives
- it hosts long-lived user and agent behavior
- it is where "Choir develops Choir" becomes a real claim

So the sequence is:

1. decompose the control plane
2. extract Go-owned hypervisor services behind flags
3. use that cleaner boundary to make a future sandbox rewrite tractable

The hypervisor side proves replaceable infrastructure. The sandbox side will
prove self-authored platform runtime.

## Decision 6: Sandbox Go Rewrite Is the Bootstrap Proof

The sandbox runtime is where Choir's actual product lives — agents,
orchestration, the writer, terminal, researcher, events, websockets. Rewriting
it in Go through Choir's own infrastructure proves the platform can build
itself.

### Why Go for the sandbox

1. **Build times.** Rust cargo build takes minutes and gigs of RAM. The build
   pool (ADR-0014) needs fast turnaround. Go builds in seconds with hundreds
   of MB. This is a concrete infrastructure requirement, not a preference.

2. **BAML removal.** The Rust sandbox uses BAML for structured LLM output,
   which forces a DECIDE→EXECUTE two-phase loop instead of standard tool-use.
   Go sandbox uses native tool-use protocol (Anthropic Messages API, OpenAI
   function calling). Simpler, cheaper (fewer tokens), lower latency.

3. **Agent loop simplification.** The current Rust agent harness is 1700+
   lines. The standard tool-use loop is ~100 lines in any language. The Go
   version starts simple and stays simple.

4. **cagent is a proof-of-concept, not a foundation.** cagent validates that
   Go works for agentic coding. Its 4000-line slop files are evidence that
   Go's build speed makes code disposable — you can rewrite faster than you
   can refactor. choir.go starts fresh with proper design, informed by what
   cagent proved but not derived from its code.

5. **Iteration speed.** Agent logic is fast-changing and experimental. Go's
   edit-compile-test cycle is seconds. Rust's is minutes. For the part of the
   system that changes most often, this matters.

6. **Build speed is the product UX.** The sandbox is what users experience as
   development speed. Build latency is perceived latency. That is the product.
   Runtime performance (Rust's edge) is invisible to users — LLM API calls
   take seconds regardless of language. Build speed is what users feel.

### What the sandbox Go rewrite preserves

- Actor model (Go channels + goroutines, same semantics as ractor)
- Supervision tree (explicit restart, lifecycle management)
- EventStore/EventBus (SQLite append-only log, in-memory pub/sub)
- WebSocket streaming
- All HTTP API contracts
- All typed message envelopes
- The writer living document model
- The conductor→app-agent→worker hierarchy

### What changes

- BAML → native tool-use protocol
- ractor actors → goroutine-based actors with typed channels
- Agent harness → standard tool-use loop (~100 lines)
- Build artifact → single Go binary (fast to compile, deploy, iterate)

### The docs-as-spec principle

The Go rewrite is executed by reading the docs. ADR-0021 (writer), ADR-0014
(VM lifecycle), this ADR, and all implementation guides define what the system
should do. The delta between the docs and the Go code is the work. The coding
agent reads the spec, writes Go, the build pool compiles it, the promotion
system verifies it.

This is the self-sustaining loop: docs define the target, the agent codes to
the target, verification confirms it, docs update to reflect new reality.

### Sequence

1. **Hypervisor decomposition first** (Decisions 1-5 above). Proves the Go
   deploy pipeline, exercises E2E parity testing, low blast radius.
2. **Sandbox Go rewrite second.** Uses the proven pipeline. Executed by
   Choir's own coding agent running in the build pool. Each module migrated
   independently: events first, then actors, then API, then agents.
3. **Rust codebase becomes reference implementation.** Can evolve
   autonomously, serve as fallback, or diverge its own way.

## Decision 7: Prerequisites for Bootstrap

Before the sandbox Go rewrite can be self-hosted (Choir rewrites Choir), these
must be in place:

1. **ADR-0014 build pool** — shared worker VMs that compile, test, verify.
   Without this, there's nowhere to run the coding agent or build Go.
2. **ADR-0014 promotion** — verified promotion from pool to user sandbox.
   Without this, build results can't be safely applied.
3. **microvm.nix 2x2 matrix validated** — blk/pmem × cloud-hypervisor/firecracker
   all tested and stress-tested. Without this, the VM substrate is uncertain.
4. **Writer contract fix (ADR-0021)** — workers send signals not diffs.
   Without this, the documentation agent can't work properly.
5. **Native tool-use agent loop** — either BAML removed from Rust, or the
   first agent loop written in Go (via cagent). Without this, there's no
   coding agent to execute the rewrite.
6. **Docs current and complete** — ADRs, guides, and state reports accurate
   enough that a coding agent can read them and produce correct code. The
   docs are the work queue. If they're stale, the agent writes to a wrong
   spec.

### Readiness sequence

```
microvm.nix 2x2 matrix testing          ← in progress
  ↓
ADR-0014: build pool + promotion        ← next
  ↓
Writer contract fix (ADR-0021 Phase 4)   ← unblocks doc agent
  ↓
Native agent loop (cagent or BAML removal) ← unblocks coding agent
  ↓
Docs audit and completion                ← the spec must be right
  ↓
Sandbox Go rewrite via Choir bootstrap   ← the proof
```

## Non-Goals

- running two whole hypervisors as the canonical migration story
- rewriting the entire hypervisor in one jump
- changing public contracts during extraction
- adding new product features during migration
- rewriting the sandbox before the prerequisites are met

---

## Boundary Reminder

The current hypervisor communicates with sandboxes via:

- HTTP/WebSocket proxy (network boundary)
- runtime-ctl binary invocation (process boundary)
- systemd unit management (OS boundary)

These protocol boundaries are what make decomposition and later sandbox
replacement feasible.

---

## Go Concurrency Model

No actor framework needed. Go's primitives are the actor model:

- Goroutines are actors.
- Channels are mailboxes.
- `select` is the receive loop.

The fundamental insight is spatial vs temporal. Shared memory is spatial:
data exists at a location, readers go look at it. Communication is temporal:
data arrives at a moment, you wait for it. Both are valid. The mistake is
using one where you need the other.

### Pattern: channels for control, shared reads for context, single-writer for mutations

- **Channels** for control flow: wake-up signals, results, lifecycle events
  (spawn, shutdown, error). These are temporal — something happened, react
  to it.
- **Shared reads** for context: agent tree state, event log, session data.
  These are spatial — the data exists, go read it when you need it. Use
  sync.RWMutex or atomic pointers. No channel needed for "what is the
  current state?"
- **Single-writer** for mutations: event store owner, state manager. One
  goroutine owns the mutable state, receives mutation requests via channel,
  applies them. Everyone else reads via shared read path.

This maps directly to the existing ractor actor model:

| ractor concept | Go equivalent |
|----------------|---------------|
| Actor + handle | goroutine + channel |
| ActorRef::cast | `ch <- msg` (fire and forget) |
| ActorRef::call | `ch <- Request{reply: replyCh}` + `<-replyCh` |
| handle_message | `select { case msg := <-ch: ... }` |
| supervision | parent goroutine with `select` on child error channels |

### GC as strategic debt

Go's garbage collector introduces microseconds of pause. This is cheap
interest in exchange for massive developer and agent time savings. It is
irrelevant when the bottleneck is LLM API latency measured in seconds. GC
pause optimization is a problem to have after you have a profiler trace
showing it matters — not before.

---

## Source References

- Go feasibility study: `docs/state-report-go-refactor-feasibility-2026-03-09.md`
- Hypervisor source: `hypervisor/src/`
- E2E tests: `tests/playwright/`
- Capacity stress test: `tests/playwright/capacity-stress-test.spec.ts`
- Systemd service: `nix/hosts/ovh-node.nix`
- ADR-0022 (concurrency optimizations): apply to Go version too
