# ADR-0026: Self-Directing Agent Dispatch

Date: 2026-03-11
Kind: Decision
Status: Proposed
Priority: 2
Requires: [ADR-0021, ADR-0024]
Owner: platform/runtime

## Narrative Summary (1-minute read)

The dispatch API for agent work converges to `{"repo": "/workspace"}`. No
objective, no scope, no task specification. The agent boots, reads the project
documentation, identifies the lowest-numbered unblocked task from the
dependency graph, and does it. The intelligence is in the documentation
structure, not the dispatch.

Prompting is harmful. It is fragile, model-dependent, and loses context.
Well-structured docs are a self-executing program.

## What Changed

2026-03-15:

- Added `docs/adr-0026-implementation.md` to ground this ADR in
  the live Rust conductor path and current `cagent` work graph.
- Clarified that the first machine-readable work index should be the existing
  docs frontmatter plus `cagent` graph, not a net-new temporal graph before the
  worker loop exists.

Previous thinking assumed dispatch needed structured task descriptions:
objectives, scope, ADR references. This is still prompting with extra steps.

The correct model: docs describe desired state, state reports describe current
state, the delta is the work. No prompt anywhere in the loop.

## What To Do Next

1. Validate the current docs frontmatter + `cagent` work graph as the first
   machine-readable doc index. Add a richer temporal graph only after the
   worker loop needs it.
2. Build the dependency graph walker that identifies unblocked work.
3. Prototype a single-worker dispatch loop: boot worker VM, worker reads docs,
   worker does one task, worker updates docs.
4. Add concurrency: multiple workers reading the same graph, each claiming
   different unblocked tasks.

---

## Core Principles

### 1. Minimal dispatch API

The dispatch payload is `{"repo": "/workspace"}`. Everything else — machine
class, adapter, scope — is derived from the work itself. The dispatcher does
not tell the worker what to do. The worker reads the repo and figures it out.

### 2. Documentation is the program

The project's documentation IS the program. ATLAS.md is the index. ADRs are
control flow. Dependency links between ADRs are edges in a DAG. State reports
are runtime assertions.

An incomplete doc is a bug, not a paperwork gap. A missing dependency link is
broken control flow. A stale state report is a lie about current reality.

### 3. Concurrency falls out of the dependency graph

If three tasks are unblocked at the same priority, dispatch three workers. The
graph IS the parallelism map. No scheduler logic decides what can run in
parallel — the dependency structure already encodes it.

### 4. Workers compute deltas, not follow instructions

Workers don't receive objectives. They read docs, compute the delta between
desired state and current state, and close the gap. A worker that finishes and
finds no remaining delta shuts down. A worker that finds an unresolvable
blocker reports it and shuts down.

### 5. Atomic doc-code-test invariant

The docs-as-work-queue pattern requires an atomic invariant: code, tests, and
docs are updated together. If docs drift from code, the work queue is wrong.
If tests drift from docs, the verification layer is wrong. Every commit that
changes behavior must update all three.

## The Dispatcher's Role

Pure resource allocation. Which machine class, how many concurrent workers,
budget constraints. The dispatcher does not know or care what work gets done.
It is a scheduler, not an orchestrator.

The dispatcher's decisions:

- **How many workers**: bounded by budget and machine availability.
- **Which machine class**: derived from the type of work (compile-heavy vs
  IO-heavy), which the worker itself can signal after reading the task.
- **When to stop**: budget exhausted, or no unblocked tasks remain.

The dispatcher's non-decisions:

- What task each worker picks up.
- How the worker executes the task.
- What "done" looks like for any given task.

## Relationship to cagent

cagent is a proof-of-concept for Go agentic coding, not the foundation for
this system. The dispatch primitive described here is a choir.go capability.
cagent validated that Go is the right language; choir.go implements the right
architecture.

## Relationship to ADR-0021 (Writer)

Writer's external API must support two access patterns:

- **Spatial reads**: any consumer can read document state directly, no message
  round-trip. Agent workers, voice clients, and publishing systems all read
  the same docs through the same path.
- **Temporal writes**: mutations flow through channels to the single writer.
  No concurrent writes. No merge conflicts. One authority.

The API shape is forced by having real external consumers — agent workers that
read docs to find work, voice clients that read docs to present content. It is
not designed in isolation.
