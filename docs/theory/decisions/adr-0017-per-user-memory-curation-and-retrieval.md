# ADR-0017: Per-User Memory Curation and Retrieval

Date: 2026-03-09
Kind: Decision
Status: Draft
Priority: 4
Requires: [ADR-0001]
Supersedes: []
Authors: wiz + Codex

## Narrative Summary (1-minute read)

ChoirOS memory is a per-user, document-centered retrieval subsystem. It is not a hidden conversation bucket, not a second source of truth, and not the global knowledge base.

Living documents remain canonical. Memory is a derived, provenance-preserving query layer around document work, user objectives, run outcomes, and verification artifacts. A background `MemoryCurator` turns high-signal scoped activity into durable memory artifacts. A passive `MemoryActor` stores and retrieves those artifacts for Conductor, Writer, and later Terminal and Researcher.

This ADR intentionally stops at per-user memory. Global or org-level knowledge comes later as a separate ADR that can reuse the same projector/query patterns without collapsing scope boundaries.

## What Changed

- Defines memory as per-user retrieval, not global knowledge.
- Introduces `MemoryCurator` as the always-running background actor.
- Keeps `MemoryActor` passive: ingest plus query only.
- Makes living documents the canonical authored record for memory provenance.
- Defers global knowledge, temporal KG, and publication concerns to a later ADR.

## Context

### The Current Code Path

The live implementation today is a small symbolic `MemoryActor`:

- `SessionSupervisor` spawns it
- Conductor can query `GetContextSnapshot` best-effort at run start
- there are no meaningful runtime producers
- default application wiring uses `:memory:` storage

This means memory currently exists as optional retrieval infrastructure but is usually empty and non-durable in practice.

### The Problem

We need a background process that evolves useful per-user memory over time. But we do not want:

- raw logs dumped into retrieval
- conversational scratch state treated as truth
- memory mutating documents directly
- global knowledge concerns mixed into per-user memory design

### Why Documents Are Central

Living documents are the primary human-readable authored surface in ChoirOS. They should remain the canonical narrative layer. Memory must preserve provenance back to documents, document versions, claims, sources, and producing activities.

That means memory should behave like a per-user projection over document-centered work, not like an opaque LLM transcript cache.

## Decision

### 1. Scope Boundary

Memory is per-user.

Future knowledge is global or org-global.

These systems may share patterns, but they are not the same subsystem and should not share authority.

### 2. Actor Topology

Per session, the runtime should have two memory actors with different responsibilities:

- `MemoryActor`: passive storage and retrieval boundary
- `MemoryCurator`: always-running background curator that watches scoped activity and writes memory artifacts

Recommended topology:

```text
ApplicationSupervisor
  -> SessionSupervisor
     -> MemoryActor
     -> MemoryCurator
     -> ConductorSupervisor
     -> WriterSupervisor
     -> TerminalSupervisor
     -> ResearcherSupervisor
```

`MemoryCurator` consumes scoped events from EventStore/EventBus and explicit high-value signals from actors. It decides what becomes memory-worthy. `MemoryActor` stores and returns retrieval artifacts.

### 3. Documents Are Canonical; Memory Is Derived

Living documents and their versions are the canonical authored record.

Memory artifacts are derived from:

- user objectives and prompt diffs
- document versions and claim-level changes
- Writer canonical outputs
- Terminal execution outcomes and verification results
- Researcher evidence summaries and cited findings

Memory never replaces document state. It only returns context and provenance.

### 4. Memory Artifacts Must Preserve Provenance

Every memory artifact should preserve enough provenance to trace it back to canonical work.

Minimum provenance shape:

- `user_id`
- `session_id` when applicable
- `thread_id` when applicable
- `run_id` when applicable
- `source_document_id` when applicable
- `source_version_id` when applicable
- `source_claim_id` when applicable
- `produced_by_actor`
- `produced_by_activity`
- `created_at`
- `source_refs` or `citation_refs`

This is required so memory can support Writer and Terminal without becoming an unverifiable blob.

### 5. Query Model

Memory remains optional and best-effort.

Required behavior:

- if memory is disabled, the system still works
- if memory is empty, the system still works
- if memory retrieval fails, only retrieval degrades
- memory never defines correctness

Initial query surfaces:

- Conductor: bounded best-effort context injection
- Writer: explicit retrieval for document continuity and planning
- Terminal: explicit retrieval during long coding and verification loops
- Researcher: optional later

### 6. Retrieval Model

Use symbolic lexical retrieval first.

Current rationale:

- it already exists
- it is simple and inspectable
- the current gap is curation and integration, not retrieval sophistication

Vector or embedding retrieval is explicitly deferred until real traces show the symbolic path is insufficient.

### 7. Curation Policy

`MemoryCurator` does not write raw logs or moment-by-moment summaries.

It writes bounded artifacts that improve future work. First-class artifact categories are:

- `user_inputs`: normalized user objectives and important prompt changes
- `version_snapshots`: canonical document or section snapshots worth carrying forward
- `run_trajectories`: bounded run outcomes, especially what succeeded or failed
- `doc_trajectories`: durable continuity signals across work on the same document

Over time these may be refined, but the rule stays the same: curate useful artifacts, not raw chatter.

### 8. Isolation Rules

Per-user isolation is mandatory.

Additional scoped isolation is required where relevant:

- `session_id`
- `thread_id`
- `run_id`

No retrieval surface may bleed memory across users, and thread/run scoped retrieval must avoid accidental cross-instance contamination.

### 9. Relationship to Future Knowledge

Global or org knowledge is out of scope for this ADR.

The future knowledge subsystem may reuse these patterns:

- projector or curator actor
- passive query actor
- provenance-preserving artifacts
- lexical search plus later richer retrieval

But knowledge has different admission rules, sharing rules, and truth semantics. It will be decided separately.

## Non-Goals

- Defining a global knowledge base
- Defining publication or org-sharing policy
- Making memory canonical state
- Storing raw tool chatter or full transcripts as memory
- Requiring vector infrastructure in v1
- Allowing memory to mutate documents directly

## Implementation Direction

### Phase 1: Make Memory Coherent

- keep `MemoryActor` passive
- add `MemoryCurator`
- make memory optional at boot, not only optional at query callsites
- add observability for query attempts, empty retrieval, ingest inserts, ingest skips, and ingest failures

### Phase 2: Make Memory Non-Empty

Initial producers in order:

1. Writer canonical outputs and user objective summaries
2. Terminal completion and verification summaries
3. Researcher evidence summaries

### Phase 3: Make Memory Queryable by the Right Actors

- keep Conductor best-effort injection
- add Writer query points
- add Terminal query points
- verify degradation when memory is empty, off, or failing

### Phase 4: Revisit Richer Retrieval Only if Needed

Only after:

- real runs are populating memory
- traces show what is and is not retrieved
- symbolic retrieval is demonstrably limiting outcomes

## Consequences

### Positive

- clear separation between curation and storage
- memory remains useful without becoming a shadow state machine
- documents stay central
- provenance remains visible
- future knowledge design can reuse the pattern cleanly

### Negative

- adds another background actor and another curation contract
- provenance-preserving artifacts are more work than transcript dumping
- some useful information will be intentionally omitted to keep memory high-signal

### Risks

- `MemoryCurator` may over-ingest and pollute retrieval
  - Mitigation: explicit artifact types, bounded records, observability
- memory may drift toward canonical truth if Writer starts trusting it too much
  - Mitigation: documents remain canonical by ADR contract
- scope bleed between session/thread/run may reappear through curator logic
  - Mitigation: require scope metadata on ingest and retrieval

## Verification

- [ ] `MemoryActor` can be disabled and the system still runs correctly
- [ ] `MemoryCurator` emits bounded artifacts rather than raw logs
- [ ] Writer can retrieve document-relevant prior context with provenance
- [ ] Terminal can retrieve prior verification outcomes without bloating prompt state
- [ ] no cross-user bleed
- [ ] no cross-thread bleed
- [ ] memory artifacts trace back to document versions, activities, or cited evidence

## References

- [2026-03-09-memory-architecture-exploration.md](/Users/wiz/choiros-rs/docs/state/snapshots/2026-03-09-memory-architecture-exploration.md)
- [adr-0001-eventstore-eventbus-reconciliation.md](/Users/wiz/choiros-rs/docs/practice/decisions/adr-0001-eventstore-eventbus-reconciliation.md)
