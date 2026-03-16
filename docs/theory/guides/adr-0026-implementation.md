# Implementing ADR-0026: Self-Directing Agent Dispatch

Date: 2026-03-15
Kind: Guide
Status: Active
Priority: 2
Requires: [ADR-0026]

## Narrative Summary (1-minute read)

The live ChoirOS dispatch path is still the opposite of ADR-0026's target.
Today the desktop sends `ConductorExecuteRequest { objective, desktop_id,
output_mode, hints }`, the API rejects an empty objective, and the Rust
Conductor runs a model turn that routes the user prompt to `writer` or
`immediate_response`. No worker boots from `{"repo": "/workspace"}`. No worker
selects work from docs. No runtime path claims the lowest unblocked item from
the graph.

What does exist already is the beginning of the docs-as-program loop:

1. `docs/ATLAS.md` is the human entrypoint.
2. doc frontmatter encodes `Priority:` and `Requires:`.
3. `scripts/generate-atlas.sh` rebuilds the index from those docs.
4. `cagent work ready` already computes unblocked work from the repo's doc
   graph.

This guide turns ADR-0026 into a sequence that matches reality. Do not start by
deleting the current prompt-driven conductor. First make the docs/work graph
authoritative for worker selection. Then add a repo-only worker bootstrap
surface. Only after that should the external dispatch contract collapse to
`{"repo": "/workspace"}`.

## What Changed

- 2026-03-15: Initial implementation guide grounded in the live Rust conductor,
  `docs/ATLAS.md`, and `cagent` work graph.
- 2026-03-15: Clarified that the first machine-readable graph is the current
  docs frontmatter plus `cagent`, not a new temporal graph schema up front.
- 2026-03-15: Separated the future self-directing worker loop from the current
  human-facing conductor prompt flow.

## What To Do Next

1. Keep the current objective-driven conductor stable as the human interaction
   surface.
2. Define one deterministic "pick next work" contract against the existing
   docs frontmatter and `cagent` ready graph.
3. Prototype a repo-only worker entrypoint that accepts only a repo path,
   claims one ready item, does it, updates docs, and exits.
4. Move concurrency and machine-class selection on top of that worker loop
   rather than baking them into the dispatch payload.

---

## Source Of Truth

These files define the current implementation boundary and the first reusable
pieces for ADR-0026:

| File | Why it matters |
|------|----------------|
| `docs/ATLAS.md` | Human entrypoint and generated view of current doc priority/dependency state |
| `scripts/generate-atlas.sh` | The actual parser for doc frontmatter today (`Priority`, `Requires`, title, status) |
| `docs/practice/guides/docs-system-philosophy-and-practice.md` | Current docs lifecycle rules and the "ATLAS is a view, not the source of truth" invariant |
| `docs/state/reports/2026-03-15-cagent-docs-landscape-audit.md` | Confirms that theory ADRs map to ready plan work and reports map to attestations |
| `AGENTS.md` | Declares the `cagent` work surfaces agents are expected to use in this repo |
| `shared-types/src/lib.rs` | Current `ConductorExecuteRequest` contract still requires `objective` |
| `dioxus-desktop/src/api.rs` | Desktop submit path still sends objective-driven conductor requests |
| `sandbox/src/api/conductor.rs` | HTTP handler rejects empty objectives and records prompt-driven input |
| `sandbox/src/actors/conductor/runtime/start_run.rs` | Current run startup logic queries memory with the objective, runs a model routing turn, and seeds agenda items |

## Current State

1. The live dispatch contract is still prompt-first.
2. The live work graph already exists in docs plus `cagent`; the runtime just
   does not consume it yet.
3. `docs/ATLAS.md` is generated from frontmatter and should stay a view, not
   become the write authority.
4. The current Rust conductor is a human-facing orchestration surface, not the
   first target for repo-only self-directing dispatch.
5. Claiming, updates, notes, and attestations already have a repo-local system
   boundary in `cagent`.

## Phase Status

```text
Phase 1  (deterministic ready-work contract)    NOT STARTED
Phase 2  (repo-only worker bootstrap)           NOT STARTED
Phase 3  (one-task worker loop)                 NOT STARTED
Phase 4  (multi-worker claiming + capacity)     NOT STARTED
```

## Phase 1: Make The Existing Docs Graph Authoritative

Goal: define one deterministic way for a worker to discover the next unit of
work from the repo without receiving an objective string.

### Scope

- Use current doc frontmatter and `cagent` graph state as the first authority.
- Keep `docs/ATLAS.md` as the generated human view.
- Keep prompt-driven conductor APIs unchanged in this phase.
- Define deterministic worker selection order.

### Implementation Notes

1. Do not parse freeform prose from ADR bodies to discover readiness.
   The authoritative signals already exist:
   - doc frontmatter (`Priority`, `Requires`, `Status`)
   - `cagent` execution and dependency state
2. Do not make `ATLAS.md` the source of truth.
   It is a generated index and should remain derivable from docs.
3. Use the repo's explicit priority model first.
   `docs/ATLAS.md` already sorts theory docs by `Priority:`.
   A stable selection rule should therefore be:
   - smallest ready priority number first
   - then smallest numeric ADR/work identifier as the tie-breaker
4. The worker needs both layers:
   - docs for desired state and implementation guidance
   - `cagent` for ready/claimed/blocked execution state

### Exit Criteria

- A worker can compute the same ready set as `cagent work ready`.
- The ordering rule is deterministic and documented.
- No prompt text is required to identify the next work item.

## Phase 2: Add A Repo-Only Worker Bootstrap Surface

Goal: introduce a worker entrypoint whose only semantic input is the repo path.

### Scope

- Accept `{"repo": "/workspace"}` or equivalent CLI input only.
- Move work selection into the worker boot path.
- Keep the existing desktop prompt bar and conductor API for human-directed
  tasks until the self-directing path proves itself.

### Implementation Notes

1. Do not overload `ConductorExecuteRequest` as the first migration step.
   That contract is still used by the human-facing prompt bar and currently
   encodes intentional prompt-driven behavior.
2. The first self-directing prototype should be a bounded worker bootstrap:
   - open repo
   - read docs/work graph
   - claim one ready item
   - execute one task
   - publish updates
   - exit
3. The first prototype can live beside the current runtime as an isolated path.
   ADR-0024 already establishes that the durable destination is the Go rewrite;
   do not force the current Rust conductor to become both products at once.

### Exit Criteria

- There is a callable worker entrypoint that accepts only a repo path.
- It can locate and claim one ready work item without being told what to do.
- The human-facing conductor path remains intact.

## Phase 3: Build The One-Task Worker Loop

Goal: make a single worker able to complete exactly one ready item and update
the repo truth surfaces atomically.

### Scope

- Read the selected ADR/guide/state evidence from the repo.
- Perform one bounded task.
- Update docs, code, and tests together.
- Publish structured `cagent` updates, notes, and attestations as evidence.

### Implementation Notes

1. The worker loop should externalize progress through the existing work
   surfaces, not an ad hoc transcript:
   - `cagent work update`
   - `cagent work note-add`
   - `cagent work attest`
2. The atomic invariant from ADR-0026 is the real correctness rule:
   code, docs, and tests move together or the work queue lies.
3. A worker that finds no delta should shut down cleanly.
4. A worker that cannot proceed should record the blocker on the work item and
   exit instead of inventing hidden context.

### Exit Criteria

- A single worker can start from repo path alone and complete one bounded work
  item.
- The resulting repo state includes code/docs/test updates plus work evidence.
- Blocked runs produce explicit work updates instead of silent failure.

## Phase 4: Add Multi-Worker Claiming And Capacity

Goal: let multiple workers read the same graph and safely take different ready
items.

### Scope

- Reuse graph readiness and claim state rather than inventing a second
  scheduler.
- Let dispatcher decisions stay at the resource-allocation layer.
- Add machine-class hints only after the worker has read the work.

### Implementation Notes

1. The dispatcher should decide:
   - how many workers to launch
   - budget ceilings
   - default machine envelope
2. The worker should decide:
   - which ready item it actually claims
   - whether the work needs escalation to a larger machine class
   - whether the work is blocked or complete
3. Reuse the claim/update semantics already visible in `cagent` state instead
   of inventing a second locking system in parallel.

### Exit Criteria

- Multiple workers can claim different ready items from the same repo.
- No worker needs an injected objective or manual scope string.
- Capacity control stays separate from work selection.

## What Not To Do

- Do not delete the prompt-driven conductor before the repo-only path exists.
- Do not make `ATLAS.md` the mutable graph authority.
- Do not start by building a new temporal graph store if the existing docs plus
  `cagent` graph can already drive work selection.
- Do not smuggle task objectives back into the system through `hints`,
  environment variables, or hidden bootstrap prompts.

## Verification

- [ ] A worker can start from repo path alone and identify one ready item.
- [ ] Ready-item selection is deterministic from doc metadata plus graph state.
- [ ] Claim, note, update, and attestation evidence are recorded on the work
  item during execution.
- [ ] Docs, code, and tests remain coherent after one worker run.
- [ ] Launching multiple workers results in distinct claims rather than the same
  item being executed twice.
