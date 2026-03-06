# Docs v2 Problem Framing

Date: 2026-03-06
Kind: Note
Status: Active
Priority: 5
Requires: []
Owner: Platform / DX / Documentation

## Narrative Summary (1-minute read)

ChoirOS does not primarily have a "need a better wiki engine" problem. It has a
canonicality problem.

The active platform story is currently split across architecture docs, runbooks, checkpoints,
reports, and handoffs. [`docs/architecture/NARRATIVE_INDEX.md`](../architecture/NARRATIVE_INDEX.md)
works as a manual atlas, but it is hand-maintained and mixes stable architecture, in-flight
plans, and operational state.

The goal of `docs/wiki` v2 is not to invent a clever document programming language up front.
The goal is to establish a low-ceremony, git-native knowledge system that tells humans and
agents:

- what ChoirOS is
- what is true today
- what is changing right now
- where canonical truth should be updated while work is underway

Any future compiler, transclusion syntax, or atlas algorithm should be justified only if it
serves that goal.

## What Changed

- Reframed the problem from "design a docs language" to "fix canonicality, adaptivity, and
  navigability."
- Separated the knowledge problem from the execution-state problem.
- Deferred decisions about transclusion-by-section, named exports, and a richer compiler until
  after a smaller docs taxonomy pilot.
- Documented a phased path from `docs/` v1 to `docs/wiki/` v2 without requiring a full
  migration or a new cognitive framework on day one.

## Where We Are Today

### Current Corpus Shape

As of 2026-03-06, the visible `docs/` corpus is roughly:

- `docs/architecture/`: 38 markdown files
- `docs/archive/`: 159 markdown files
- `docs/runbooks/`: 9 markdown files
- `docs/research/`: 10 markdown files
- `docs/checkpoints/`: 1 markdown file
- `docs/handoffs/`: 3 markdown files
- `docs/reports/`: 4 markdown files
- `docs/design/`: 4 markdown files

This is not inherently bad. The problem is that the active platform canon is spread across
multiple genres.

### What Works

- The corpus is git-native and readable in raw Markdown.
- ADRs and major architecture docs already follow a readable summary structure.
- [`docs/architecture/NARRATIVE_INDEX.md`](../architecture/NARRATIVE_INDEX.md) provides a
  real entrypoint instead of forcing readers to guess.
- Checkpoints and handoffs already capture volatile state separately from some architecture
  material, even if the boundary is not yet clean.

### What Hurts

- Active truth is scattered across architecture docs, runbooks, checkpoints, and reports.
- It is often unclear which document is the canonical place to mutate when the platform
  changes during implementation.
- Stable architecture, proposed changes, and current operational status are mixed together.
- The current atlas is hand-curated, which is useful but fragile.
- Archive and active material coexist in the same broad namespace, which raises the cost of
  orientation for both humans and agents.

## What Problem Are We Actually Solving?

We are solving this:

How do we maintain a low-ceremony documentation system that serves as the living platform
canon for both humans and agents, supports in-flight mutation while work is underway, and
clearly distinguishes stable truth, active change, and operational state?

More concretely, the system needs to answer:

1. What is ChoirOS trying to become?
2. What is true about the platform today?
3. What is changing right now?
4. Where should a new idea, design change, or correction be recorded?
5. How does a fresh human or agent find the active story without rereading the entire archive?

## What We Are Not Solving

- We are not trying to replace git history.
- We are not trying to replace task tracking, CI, or runtime checkpointing.
- We are not trying to make documentation fully deterministic or workflow-driven.
- We are not trying to build a CMS or a heavyweight external knowledge platform.
- We are not trying to force authors into a complex schema before the basic taxonomy is proven.

## Reframed Model

The documentation system should separate three different concerns:

### 1. Knowledge Plane

This is the platform canon:

- philosophy
- architecture
- features
- durable decisions
- roadmap
- glossary
- current synthesized state

This is the future home of `docs/wiki/`.

### 2. Change Plane

This captures mutation while the work is still live:

- proposals
- active migrations
- open questions
- unresolved tradeoffs

This material may later be folded into the knowledge plane or archived.

### 3. Execution Plane

This captures volatile operational state:

- checkpoints
- handoffs
- run artifacts
- verification notes

This material is useful, but it should not be mistaken for the primary platform canon.

## Design Principles

- Markdown-first: unrendered Markdown must stay readable.
- Git-native: docs remain normal repository artifacts.
- Low ceremony: updating the canon during active work must be cheap.
- Typed enough: docs need lightweight structure, not a full ontology.
- Adaptive: the system must allow mid-flight mutation without pretending every idea is already
  accepted truth.
- Layered: accepted truth, proposals, and volatile state must remain distinguishable.
- View-oriented: the atlas is a generated or curated view over canonical units, not the only
  place where truth lives.
- Testable: any future compiler or linter should enforce a small number of useful invariants,
  not encode an overdesigned document language.

## Proposed v2 Scope

The first version of `docs/wiki` should be smaller than the abstract design space.

### Phase 0: Taxonomy Before Compiler

Start by defining a clean information architecture:

- `docs/wiki/atlas/`
- `docs/wiki/concepts/`
- `docs/wiki/systems/`
- `docs/wiki/features/`
- `docs/wiki/decisions/`
- `docs/wiki/proposals/`
- `docs/wiki/roadmaps/`
- `docs/wiki/runbooks/`
- `docs/wiki/status/`
- `docs/wiki/glossary/`

And keep execution-state material outside the main wiki:

- `docs/state/checkpoints/`
- `docs/state/handoffs/`

### Phase 1: Minimal Metadata

Before introducing transclusion or query syntax, prove a small schema:

- `id`
- `title`
- `kind`
- `status`
- `updated`
- `relates_to`

That is enough to support validation, indexing, and later graph building without forcing a
heavier authoring model.

### Phase 2: Atlas as a View

Keep the atlas deliberately simple at first:

- one curated top-level index
- generated backlinks or registries only if they reduce real maintenance cost
- no mandatory transclusion system until the taxonomy is stable

### Phase 3: Compiler Features Only If Earned

Only add transclusion, queries, or richer compilation once there is a concrete maintenance
problem that cannot be handled well by the taxonomy plus a curated atlas.

## Deferred Decisions

These questions are real, but they should remain deferred for now:

- Should transclusion target sections, anchors, or something more explicit?
- Should the atlas be fully generated, partially generated, or fully curated?
- Should canonical docs compile into a graph JSON for agent use?
- What validation rules are genuinely useful versus ceremonial?
- What level of document coupling is acceptable?

These should be answered after a small pilot, not before one.

## Decision Criteria For Future Features

Any proposed compiler or language feature should pass these tests:

1. Does it reduce duplicated truth?
2. Does it preserve raw Markdown readability?
3. Does it keep authoring cheaper, not more expensive?
4. Can it be validated deterministically?
5. Does it improve orientation for a fresh human or agent?
6. Is it solving an observed maintenance problem rather than an imagined one?

## What To Do Next

1. Agree on the problem statement in this document before designing syntax.
2. Define the minimal docs taxonomy and metadata contract.
3. Pilot `docs/wiki/` with a small active slice of the corpus rather than migrating everything.
4. Move checkpoints and handoffs into an explicit execution-state namespace.
5. Re-evaluate whether transclusion, queries, or a compiler are still needed after the pilot.
