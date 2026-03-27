# Go Refactor Feasibility Study

Date: 2026-03-09
Kind: Report
Status: Active
Requires: []

## Narrative Summary (1-minute read)

ChoirOS could be rewritten in Go, but the question is not whether Go can replace
Rust syntax-for-syntax. The real question is whether the runtime semantics now
encoded through `ractor`, supervisors, and typed actor messaging should be
preserved, simplified, or deliberately redesigned.

Current repo evidence suggests a split answer. `hypervisor` is already a fairly
conventional async HTTP and control-plane service and would port cleanly.
`sandbox` is different: actor identity, supervision, capability dispatch, and
observability are structurally central there. At the same time, the active
product direction is increasingly Writer-centric living documents, with Terminal
and Researcher serving that flow rather than a generic chat-style actor mesh.

The strategic conclusion is:

1. Do not rewrite solely to reduce Rust build frustration.
2. A `hypervisor`-only rewrite is the right bounded experiment because it can be
   tested and swapped independently.
3. `sandbox` should not be approached first as a hard replacement. It is better
   treated as a protocol boundary with room for parallel implementations and
   compare-contrast evaluation.
4. A larger `sandbox` rewrite is only justified if ChoirOS wants the semantic
   simplification available in a Go-native hierarchical model:
   Conductor for global orchestration, Writer for run and document
   orchestration, Terminal for bounded execution orchestration, and Researcher
   as the scalable non-mutating evidence lane.

## What Changed

1. Grounded the rewrite question in the current repo rather than a generic
   Rust-versus-Go comparison.
2. Separated structural runtime semantics from incidental `ractor` mechanics.
3. Defined a Go-native target model that preserves ChoirOS intent without
   requiring literal actor-framework equivalence.
4. Added explicit memory-off and memory-on operating modes so the study does
   not depend on the partially integrated memory subsystem.
5. Ended with a recommendation matrix and a default recommendation.

## What To Do Next

1. Use this report to decide whether the real goal is faster local iteration,
   semantic simplification, or both.
2. If the goal is only build speed, prioritize Rust-side build and cache work
   before any runtime rewrite.
3. If the goal is to learn from Go with low blast radius, rewrite `hypervisor`,
   prove parity, and test a real swap.
4. If the goal is semantic simplification in `sandbox`, treat the runtime as a
   protocol and compare Rust and Go implementations side by side before any
   cutover.
5. Keep the memory subsystem optional until it proves useful in real Writer and
   Terminal flows.

## 1) Current System Snapshot

### Current fact

The current runtime is split across three Rust workspace members:
`shared-types`, `hypervisor`, and `sandbox`. `dioxus-desktop` is outside this
study. See `Cargo.toml`, `hypervisor/Cargo.toml`, and `sandbox/Cargo.toml`.

`hypervisor` is already close to a conventional Go-shaped service. Its `main`
function wires config, database access, session storage, sandbox registry, HTTP
routes, provider proxying, and auth middleware. That is normal async service
code, not a highly actor-shaped runtime. See `hypervisor/src/main.rs`.

`sandbox` is where actor and supervision mechanics are structurally central.
`sandbox/src/main.rs` spawns `EventStoreActor`, initializes app state, and
brings up `ApplicationSupervisor`. `sandbox/src/supervisor/mod.rs` then owns
the supervision tree, event relay, request ingestion, worker signaling, and
health reporting.

The architecture docs also make the current intent explicit:

- ChoirOS is described as a multi-agent runtime built on `ractor` supervision
  trees. See `docs/ATLAS.md`.
- The active runtime shape centers around EventStore-first observability and
  supervised workers. See `docs/actor-network-orientation.md`.
- EventStore is the single source of truth and EventBus is delivery-only. See
  `docs/adr-0001-eventstore-eventbus-reconciliation.md`.

### Current fact: product direction

The product direction is no longer best understood as "chat with tools." The
writer review states that the desired end-state is a single canonical living
markdown document per run, with marginalia and progress in separate lanes. See
`docs/archive/handoffs/2026-Q1/2026-02-26-writer-living-document-system-review.md`.

That matters because it shifts the core runtime question away from "how do we
preserve actor-to-actor chat semantics?" and toward "what architecture best
serves Writer-centric long-horizon work?"

## 2) What Is Structural vs Incidental in the Rust Runtime

### Structural semantics to preserve

These semantics appear to be intentional product and correctness boundaries,
independent of implementation language.

1. **EventStore is the source of truth.**
   `docs/adr-0001-eventstore-eventbus-reconciliation.md`
   defines the single-write rule, durable replay, and EventBus as delivery only.

2. **Scope isolation is mandatory.**
   The runtime depends on `session_id`, `thread_id`, `run_id`, and correlation
   metadata to prevent cross-instance bleed. This shows up in shared contracts
   and in supervisor ingestion paths. See `shared-types/src/lib.rs` and
   `sandbox/src/supervisor/mod.rs`.

3. **Conductor is orchestration-only and must remain non-blocking.**
   `docs/ATLAS.md` and `sandbox/src/actors/conductor/actor.rs` both reflect the
   rule that Conductor coordinates rather than executes tools directly.

4. **Writer is canonical for living-document state.**
   The Writer review and Writer actor implementation both point to Writer as the
   single authority for run-document state, versions, overlays, and inbound
   worker updates. See
   `docs/archive/handoffs/2026-Q1/2026-02-26-writer-living-document-system-review.md`
   and `sandbox/src/actors/writer/mod.rs`.

5. **Terminal is the mutable execution authority.**
   Terminal owns shell execution and now effectively serves as the boundary
   where mutable repo work, long-horizon coding tasks, and execution reporting
   happen. See `sandbox/src/actors/terminal.rs`.

6. **Researcher is a bounded evidence lane.**
   Researcher owns web and information gathering, is non-mutating by nature,
   and already has a cleaner horizontal scaling story than Terminal. See
   `sandbox/src/actors/researcher/mod.rs`.

### Incidental mechanics that do not need literal preservation

These are real implementation choices today, but they are not obviously part of
the product contract.

1. `ActorRef`
2. `ractor::call!`
3. registry lookups like `where_is`
4. supervision-event plumbing as the main programming model
5. actor mailbox identity as the primary way to represent ownership

In other words: ChoirOS needs ownership, routing, restart behavior, and
durable observability. It does not specifically need those concepts to be
represented as `ractor` cells and macros.

## 3) Go-Native Target Model

### Proposed model

The Go-native runtime should be organized around hierarchical orchestration,
service ownership, and durable IDs rather than literal actor ports.

The proposed layers are:

1. **Conductor**
   Global orchestration only. Owns cross-run policy, priority, budgets,
   cancellation, and app-level coordination.

2. **Writer**
   Run and document orchestration. Owns user-facing continuity, plan shape,
   acceptance criteria, canonical document state, versions, and the visible run
   narrative.

3. **Terminal**
   Bounded execution orchestration. Owns shell work, mutable repo operations,
   external coding-agent subprocesses, verification loops, and mutation safety.

4. **Researcher**
   Concurrent non-mutating worker lane. Owns bounded evidence gathering,
   synthesis, and retrieval support.

### Proposed model: important clarification

"Conductor is orchestration-only" does **not** mean "Conductor is the only
orchestrator."

The cleaner interpretation is:

- Conductor orchestrates globally.
- Writer orchestrates the run.
- Terminal orchestrates execution inside a bounded delegated job.

That is already where the repo is moving conceptually, even if the Rust
implementation still carries more actor-era scaffolding than the product
ultimately wants.

## 4) Hierarchical Orchestration Semantics

### Current fact

The repo already points toward layered authority:

- capability ownership docs say Conductor is orchestration-only and Writer is
  canonical for living-document mutation,
- Writer delegates to Researcher and Terminal,
- Terminal and Researcher already emit structured progress and completion into
  Writer-facing channels.

See:

- `docs/ATLAS.md`
- `docs/actor-network-orientation.md`
- `docs/cagent-spec-and-implementation-guide.md`
- `sandbox/src/actors/writer/mod.rs`
- `sandbox/src/actors/terminal.rs`
- `sandbox/src/actors/researcher/mod.rs`

### Proposed model

1. **Writer owns user-facing continuity.**
   Writer should own the run-level plan, acceptance criteria, visible state,
   canonical document truth, and the interpretation of worker outputs.

2. **Terminal owns long-horizon execution loops.**
   Terminal should own shell execution, external coding-agent invocation,
   implementer-verifier chains, and bounded retries or substeps within one
   delegated execution job.

3. **Researcher owns bounded evidence work.**
   Researcher should remain the lane for web and local reading synthesis,
   optimized for concurrency and low mutation risk.

4. **Conductor owns global policy only.**
   Conductor should manage cross-run policy, not directly hold the entire
   burden of decomposition for every execution detail.

This hierarchical model reduces the amount of power any one layer needs to hold
at once. That is strategically attractive even if no rewrite happens.

## 5) Memory-Off and Memory-On Operation

### Current fact

The current memory subsystem is partially integrated. The active implementation
is `sandbox/src/actors/memory.rs`. `SessionSupervisor` spawns it, and Conductor
attempts a best-effort `GetContextSnapshot` at run start. But the active runtime
does not appear to have meaningful producers calling `MemoryMsg::Ingest`, so the
service is often present but empty. See:

- `sandbox/src/supervisor/session.rs`
- `sandbox/src/actors/conductor/runtime/start_run.rs`
- `sandbox/src/actors/memory.rs`

### Proposed model

The architecture must work in two modes.

#### Memory-off baseline

- Memory is absent, disabled, or empty.
- Conductor still runs with bounded direct context and durable event/artifact
  references.
- Writer still owns the run and document.
- Terminal still owns mutable execution.
- Researcher still gathers evidence.
- Correctness and recovery still come from durable events and artifacts.

#### Memory-on mode

- Memory is optional shared retrieval infrastructure.
- Conductor may query it for bounded planning context.
- Writer may query it for long-lived run continuity, evidence recall, and
  document shaping.
- Terminal may query it arbitrarily during long coding work to avoid repeating
  expensive research.
- Researcher may populate it through evidence summaries and source-backed
  findings.

Memory failure should degrade retrieval quality, not runtime correctness.

## 6) Concurrency and Failure Semantics

### Current fact

The current runtime uses supervisors and actors to contain failure and keep work
partitioned. That is visible in `sandbox/src/supervisor/mod.rs`,
`sandbox/src/actors/conductor/actor.rs`, and the worker implementations.

### Proposed model

The Go-native equivalent should preserve the **semantics** of failure handling
without preserving the exact mechanics.

1. **One terminal lane per sandbox/workspace by default.**
   Terminal handles mutable execution, so safe serialization matters more than
   parallelism there.

2. **Many researchers may run concurrently.**
   Researcher is non-mutating and therefore horizontally scalable.

3. **Terminal can manage multiple subjobs, but not multiple uncontrolled
   mutating writers into the same repo.**
   Safe concurrency should be explicit and narrow.

4. **Writer can recover from Terminal failure.**
   Writer is the outer supervising loop for the run narrative and can restart,
   reissue, or resume delegated execution based on durable job state.

5. **Durable IDs replace implicit actor identity.**
   `run_id`, `job_id`, `subjob_id`, `call_id`, `session_id`, `thread_id`, and
   `correlation_id` should become the primary correctness handles.

In short: the semantics should move from "actor framework guarantees this" to
"runtime services and durable contracts guarantee this."

## 7) Terminal + Researcher + External Coding Agents

### Current fact

The repo already has three ingredients that matter here:

1. Terminal is the shell and execution boundary.
   See `sandbox/src/actors/terminal.rs`.
2. Researcher is the bounded evidence lane.
   See `sandbox/src/actors/researcher/mod.rs`.
3. `cagent` is already specified as a Go CLI for durable control over external
   coding-agent CLIs.
   See `docs/cagent-spec-and-implementation-guide.md`.

### Proposed model

The standard operating shape should be:

- many Researchers,
- one Terminal lane per sandbox/workspace,
- Writer as the canonical sink for progress and final meaning.

#### Terminal

Terminal remains the single mutable execution authority in a sandbox. It may:

- run shell commands,
- inspect and edit repo state,
- invoke external coding agents through CLI,
- run nested loops such as:
  - pass a spec to an implementer agent,
  - await completion,
  - launch a verifier agent,
  - await verification,
  - report status to Writer throughout.

#### Researcher

Researcher scales horizontally because it is non-mutating. It should support:

- one-off research jobs,
- run-scoped evidence production,
- long-lived evidence refresh for longer coding sessions.

#### Writer-owned evidence with lease-gated Terminal access

The best fit is not free-form worker ownership of durable research state.
Instead:

- Writer owns the long-lived run evidence space,
- Researchers populate and refresh it,
- Terminal gets lease-gated direct access to query or trigger bounded research
  against that run-owned evidence space.

That keeps Terminal powerful enough for long-horizon coding work without letting
it silently become the owner of durable research truth.

## 8) Rewrite Options and Recommendation

### Option A: No rewrite, address build pain in Rust

**What stays the same**

- current runtime architecture,
- supervision-tree-first implementation,
- existing actor contracts and tests.

**Benefits**

- lowest migration risk,
- preserves current code and test investment,
- build pain may be reducible through Nix, caches, crate graph work, and
  better workspace discipline.

**Costs**

- keeps current semantic complexity,
- does not automatically simplify Writer and Terminal boundaries.

### Option B: Rewrite `hypervisor` only, then test a real swap

**What changes**

- control-plane service becomes Go,
- sandbox runtime remains Rust.

**Benefits**

- technically feasible,
- limited blast radius,
- likely easy to package and operate,
- gives a real test of build, packaging, and ops ergonomics without touching the
  core runtime.

**Costs**

- does not solve the real runtime semantics question,
- leaves the most architecturally complex layer untouched.

### Option C: Treat `sandbox` as a protocol and run Rust and Go implementations
in parallel

**What changes**

- define the relevant `sandbox` surfaces as protocol contracts,
- keep the Rust implementation as the incumbent,
- build a Go implementation behind the same protocol where feasible,
- compare behavior, semantics, operability, and product fit directly.

**Benefits**

- avoids forcing an early all-or-nothing cutover,
- creates a cleaner architecture target because protocol surfaces have to be
  named explicitly,
- allows empirical compare-contrast instead of speculative replacement.

**Costs**

- requires discipline around boundary definition,
- can create temporary duplication,
- still does not remove the need to decide which semantics actually matter.

### Option D: Full `sandbox` replacement with a Go-native redesign

**What changes**

- keeps correctness boundaries,
- deliberately simplifies orchestration into Conductor, Writer, Terminal, and
  Researcher layers,
- makes memory optional,
- treats external coding-agent orchestration as a first-class Terminal concern.

**Benefits**

- strongest architectural payoff,
- aligns with the Writer-centric product direction,
- clarifies ownership and long-horizon work.

**Costs**

- largest design and migration burden,
- requires genuine product-confidence in the new model,
- cannot honestly be sold as a mere language swap.

### Recommendation

The default recommendation is:

1. **Do not rewrite solely for build times.**
2. **Use `hypervisor` as the first bounded Go experiment.** Rewrite it, verify
   parity, and prove that it can actually be swapped in.
3. **Treat `sandbox` as a protocol before treating it as a replacement target.**
   The near-term goal should be parallel implementations and explicit
   compare-contrast, not immediate cutover.
4. **Only justify a full `sandbox` rewrite if ChoirOS explicitly wants the
   semantic simplification around Writer, Terminal, and Researcher enough to pay
   the migration cost.**

The strongest near-term move is not "rewrite the whole runtime now." It is:

- clarify current Rust ownership boundaries,
- define `sandbox` protocol surfaces explicitly,
- run a bounded `hypervisor` rewrite,
- and use side-by-side `sandbox` work to learn which semantics should survive.

## 9) Acceptance Scenarios

The rewrite is only strategically successful if it can satisfy these scenarios
without backsliding on correctness.

1. **Writer-first coding run with living document**
   - user works through Writer,
   - Writer owns the canonical document,
   - worker outputs become progress, evidence, and completion updates rather
     than a scrolling chat log.

2. **One terminal plus multiple researchers**
   - Terminal owns mutable execution in one sandbox,
   - multiple Researchers can run concurrently,
   - Writer ingests both lanes coherently.

3. **Terminal requesting research during long coding work**
   - Terminal can request or query research without bloating its own live
     context,
   - research results feed both the run evidence space and Writer-visible state.

4. **External coding-agent direct edits in sandbox followed by build, test, and
   verify**
   - external coding agents may edit code directly,
   - Terminal remains responsible for safe sequencing and verification,
   - Writer reflects status and final meaning.

5. **Terminal failure and Writer-driven recovery**
   - Writer can reconstruct outstanding delegated work from durable state,
   - recovery does not require hidden in-memory actor continuity.

6. **Operation with memory disabled**
   - correctness still holds,
   - only retrieval quality is reduced.

7. **No cross-session or cross-thread bleed**
   - scope isolation remains intact for events, evidence, and worker activity.

## Factual Basis

This report is grounded in the current repo artifacts below.

- `docs/ATLAS.md`
- `docs/actor-network-orientation.md`
- `docs/adr-0001-eventstore-eventbus-reconciliation.md`
- `docs/cagent-spec-and-implementation-guide.md`
- `docs/archive/handoffs/2026-Q1/2026-02-26-writer-living-document-system-review.md`
- `hypervisor/src/main.rs`
- `shared-types/src/lib.rs`
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/actors/conductor/actor.rs`
- `sandbox/src/actors/terminal.rs`
- `sandbox/src/actors/researcher/mod.rs`
- `sandbox/src/actors/writer/mod.rs`
- `sandbox/src/supervisor/session.rs`
- `sandbox/src/actors/conductor/runtime/start_run.rs`
- `sandbox/src/actors/memory.rs`
