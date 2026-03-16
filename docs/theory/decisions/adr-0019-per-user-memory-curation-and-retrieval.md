# ADR-0019: Per-User Memory Curation and Retrieval

Date: 2026-03-09
Kind: Decision
Status: Draft
Priority: 4
Requires: [ADR-0001]
Supersedes: []
Authors: wiz + Codex

## Narrative Summary (1-minute read)

ChoirOS memory is a per-user, activity-driven retrieval subsystem. It is not a hidden conversation bucket, not a second source of truth, and not the global knowledge base.

The knowledge graph is not a code graph or a document graph. It is a user activity graph. Users write research, plan trips, build products, curate media, and have conversations. All of these produce nodes and edges. Living in Choir builds the graph automatically through event projection; users never manually maintain it. Memory is a derived, provenance-preserving query layer over this activity — "what do I know about X?" is a graph query, not a retrieval operation.

Living documents remain canonical as the human-authored surface. A background `MemoryCurator` turns high-signal scoped activity into durable memory artifacts. A passive `MemoryActor` stores and retrieves those artifacts for Conductor, Writer, and later Terminal and Researcher.

The structural backbone is a per-user temporal knowledge graph in SQLite. The unit is the user, not the project — projects are clusters within the user's graph, and cross-project edges (dependencies, shared patterns) live naturally in this model. Storage starts with SQLite (already in data.img, ~1MB overhead), with DoltDB as the upgrade path for the global published KB and for paid users who need versioned knowledge.

This ADR intentionally stops at per-user memory. Global or org-level knowledge comes later as a separate ADR that can reuse the same projector/query patterns without collapsing scope boundaries.

## What Changed

- Defines memory as per-user retrieval, not global knowledge.
- Introduces `MemoryCurator` as the always-running background actor.
- Keeps `MemoryActor` passive: ingest plus query only.
- Makes living documents the canonical authored record for memory provenance.
- Defers global knowledge, temporal KG, and publication concerns to a later ADR.
- (2026-03-11) Adds per-user knowledge graph as the unit of structure (Section 10).
- (2026-03-11) Adds temporal graph schema for SQLite (Section 11).
- (2026-03-11) Adds storage tier progression: SQLite now, DoltDB global, DoltDB paid-user (Section 12).
- (2026-03-11) Adds doc-to-graph-to-view progression for project indexing (Section 13).
- (2026-03-11) Generalizes KB beyond code-focused use: activity-driven graph projection from all user work (Section 14).
- (2026-03-15) Adds an implementation guide at `docs/theory/guides/adr-0019-implementation.md` grounded in the live Rust memory path.

## What To Do Next

- Add temporal graph schema (Section 11) to the per-user SQLite database as
  a migration when the first graph producer is ready.
- Implement graph projection in `MemoryCurator`: extract nodes/edges from
  ADR dependencies, file references, and cross-document relationships.
- Build a prototype ATLAS.md renderer that reads from the graph instead of
  scanning the filesystem.
- Evaluate DoltDB as the global KB store when hypervisor-side publishing
  lands (ADR-0024/0025 Go services are a natural host).

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

### 10. Per-User Knowledge Graph (Not Per-Project)

The unit of the knowledge graph is the user, not the project.

A user's knowledge graph spans all their projects. Projects are clusters of
densely connected nodes within the user's graph, not separate graphs. This
matters because cross-project edges are real and common: an ADR in project A
depends on a library decision in project B, a testing pattern learned in
project C applies to project D. These edges only make sense in a per-user
graph. Per-project graphs cannot represent them without an awkward
cross-graph join layer.

Implications:

- The graph lives in the user's storage, not in project-level storage
- Node and edge identifiers include project context as metadata, not as
  a partition boundary
- Queries can scope to a single project (filter by cluster) or span the
  full user graph (cross-project retrieval)

### 11. Temporal Graph Schema

The knowledge graph is temporal. Nodes and edges carry `valid_from` and
`valid_to` timestamps so the graph represents how knowledge evolved, not
just its current state. Events record mutations for audit and replay.

Target schema (SQLite, lives in per-user data.img):

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    kind TEXT,        -- 'adr', 'file', 'concept', 'test', 'agent'
    data JSON,
    valid_from INTEGER,
    valid_to INTEGER
);

CREATE TABLE edges (
    src TEXT,
    dst TEXT,
    kind TEXT,        -- 'depends_on', 'implements', 'tests', 'blocks'
    data JSON,
    valid_from INTEGER,
    valid_to INTEGER
);

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    ts INTEGER,
    actor TEXT,
    action TEXT,
    target TEXT,
    data JSON
);
```

`valid_to IS NULL` means the node or edge is current. Point-in-time queries
use `valid_from <= ?t AND (valid_to IS NULL OR valid_to > ?t)`.

This schema is additive to the existing per-user SQLite database. It does not
replace the existing memory artifact tables; it provides the structured graph
layer that memory artifacts can project into over time.

### 12. Storage Tier Progression

Memory storage is not one-size-fits-all. The progression follows actual need:

**Now: SQLite per-user.**
Already exists in data.img. Add the temporal graph schema (Section 11) when
the first graph producer is ready. SQLite baseline memory cost is ~1MB. On a
430-470MB per-VM budget, this is negligible.

**Global knowledge base: DoltDB.**
When publishing exists and the global KB needs versioning, DoltDB earns its
keep. Go-native, embeddable, MySQL-compatible, prolly trees with structural
sharing. Runs as the hypervisor-side published knowledge store, not inside
user VMs.

**Per-user upgrade: DoltDB for paid users.**
When a user has enough data that versioning matters and they are paying for
the memory overhead. Dolt loads its commit graph into memory (10-20% of DB
size as RAM). For an active knowledge base, that could be 50-100MB — viable
for a paid tier with larger VM allocations, not viable on the free-tier
430-470MB budget.

Why not Dolt per-user from the start: Dolt is a Go GC'd runtime. Inside a
user VM already running a sandbox process, that is a second Go runtime with
its own heap and GC pressure. At 50-100MB for an active KB, that is 10-25%
of the current VM budget consumed by the storage engine alone. SQLite avoids
this entirely.

### 13. From Markdown Docs to Graph Views

The progression for project indexing:

1. **Current**: Markdown files are the source of truth. ATLAS.md is a
   generated index over those files.
2. **Next**: The temporal graph (Section 11) becomes the machine-readable
   project index. `MemoryCurator` projects document structure, ADR
   dependencies, and cross-file relationships into graph nodes and edges.
3. **Then**: ATLAS.md becomes a view rendered from the graph, not the
   source of truth. Other views (dependency diagrams, staleness reports,
   impact analysis) render from the same graph.

Documents remain the human-authored surface. The graph is the
machine-readable structure extracted from that surface. Views are
human-readable projections of the graph. The document is upstream of the
graph, not downstream.

### 14. Activity-Driven Graph Projection

The knowledge graph is not a code graph. It is a user activity graph.

Users do all sorts of things in Choir: write research, plan trips, build
products, curate media, have conversations. All of these produce nodes and
edges. The graph is not limited to software concepts like ADRs, files, and
tests. It captures whatever the user works on, in whatever domain they work
in.

**The graph builds itself from user activity.** Users never manually create
nodes or edges. Living in Choir builds the graph. Events from normal use
automatically project into KB structure:

- User edits a document: node updated, edges to related nodes refreshed
- User asks Conductor to research X: research nodes created, citation edges
  to sources
- User publishes a piece: node promoted to global KB, version snapshot taken
- User starts a new project: cluster node created, edges to shared concepts
  across existing projects
- User has a conversation: key insights extracted as concept nodes, linked
  to the conversation context

**Memory is not a separate feature. It is a view over the activity graph.**
"What do I know about X?" is a graph query that traverses nodes and edges,
not a retrieval operation that searches a flat artifact store. The temporal
dimension (Section 11) answers "when did I learn this?" and "how has my
understanding evolved?" These are time-scoped graph traversals, not keyword
searches.

**The node `kind` field (Section 11 schema) is intentionally freeform to
support this.** Beyond code-focused kinds like `adr`, `file`, `concept`,
`test`, `agent`, the field accommodates any domain:

- Research: `source`, `citation`, `argument`, `draft`, `published`
- Travel: `destination`, `booking`, `preference`, `constraint`
- Conversation: `conversation`, `insight`, `question`, `decision`
- Media: `media`, `playlist`, `annotation`, `tag`

The kind is a string, not an enum. New domains do not require schema
changes. Edge kinds follow the same principle: `cites`, `contradicts`,
`inspires`, `budgets`, `schedules` are all valid alongside `depends_on` and
`implements`.

**The event-to-graph projection is the key automation.** EventStore already
captures all user activity as the observability backbone (CLAUDE.md). A
projection layer watches that event stream and maintains the graph as a
side effect. This is the Watcher pattern (from CLAUDE.md naming
reconciliation): a persistent observer on the event stream that detects
graph-relevant events and writes corresponding node/edge mutations.

The projection is not a batch job. It runs continuously as part of
`MemoryCurator`, processing events as they arrive. The curator decides
which events are graph-worthy (most are not) and what structure to extract.
This keeps the graph high-signal without requiring the user to curate
anything manually.

Implications for the existing design:

- `MemoryCurator` (Section 2) gains a graph projection responsibility
  alongside its existing artifact curation role. These are complementary:
  artifacts are the retrieval units, the graph is the structural index
  over them.
- The curation policy (Section 7) extends to graph projection: not every
  event produces a node, just as not every event produces an artifact.
  The curator applies the same bounded, high-signal filtering.
- The query model (Section 5) gains graph traversal as a query surface
  alongside lexical retrieval. Graph queries answer structural questions
  ("what depends on X?", "what did I research about Y?") that lexical
  search handles poorly.

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
