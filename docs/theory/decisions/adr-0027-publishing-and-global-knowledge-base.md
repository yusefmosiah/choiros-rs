# ADR-0027: Publishing and Global Knowledge Base

Date: 2026-03-11
Kind: Decision
Status: Draft
Priority: 3
Requires: [ADR-0011, ADR-0019, ADR-0026]
Owner: platform/runtime

## Narrative Summary (1-minute read)

Publishing is the outward-facing projection of the per-user knowledge graph.
The user's KB is private -- the accumulated residue of all their activity in
Choir. Publishing promotes selected subgraphs from the per-user KB to a global
shared KB. Published pieces carry version history, citation edges, and
relationships to other published pieces. The global KB uses DoltDB for
git-style versioning (hundreds or thousands of versions per popular piece).
The per-user KB stays SQLite (ADR-0019).

## What Changed

Previous thinking (ADR-0011) framed publishing as state/compute decoupling --
separating the published artifact from the runtime. That still holds. What is
new: publishing is a graph operation (promote subgraph from private to public),
not a file operation (export document). The relationship between pieces, their
sources, their version history -- all of that is structure in the graph, not
metadata on a file.

## What To Do Next

1. Finalize per-user KB schema (ADR-0019) -- publishing depends on having a
   graph to promote from.
2. Design the promotion operation: which nodes, which edges, privacy trimming.
3. Stand up DoltDB instance on hypervisor for global KB prototype.
4. Define the published piece schema in the global graph (version history,
   authorship, license, media type).
5. Build the simplest possible publish flow: user marks a document node as
   "publish", promoted to global graph, viewable at a URL.

## Decision

### 1. Publishing Is Subgraph Promotion

User selects nodes (a document, its citations, supporting research) and
promotes them from the per-user SQLite graph to the global DoltDB graph. The
edges between promoted nodes are preserved. Edges to non-promoted nodes are
trimmed -- private context stays private.

This means the publish operation is not "export this file." It is "project
this connected subgraph into the public namespace, preserving internal
structure and trimming external references that point to private state."

### 2. Version History Is Native

DoltDB's prolly trees with structural sharing mean storing hundreds of
versions of a published piece costs proportional to diffs, not full
documents. Every edit to a published piece is a Dolt commit. Readers can
view any historical version. Diff between versions is a first-class
operation.

This replaces the `stable`/`candidate` pointer model from ADR-0011 with
something richer: the full commit graph is the version history, not just two
named pointers. Pointer semantics (`stable`, `candidate`, rollback) layer on
top as branch refs in the Dolt commit graph.

### 3. The Global KB Is a Shared Graph, Not a Document Store

Published pieces have edges to other published pieces -- citations, responses,
derivatives, translations. This structure emerges from the publishing act, not
from manual linking. If a user's source node had an edge to another user's
published node, that edge carries through on publish.

The global graph schema mirrors the per-user temporal graph schema (ADR-0019
Section 11) with additional fields:

- `author_id` (user who published)
- `license` (content license)
- `media_type` (text, audio, video, interactive)
- `published_at` / `updated_at`
- `source_user_node_id` (opaque ref back to origin, for the author only)

### 4. Publishing Does Not Copy, It Projects

The per-user KB retains the full private context (drafts, failed attempts,
private notes). The global KB gets the promoted subgraph. Updates to the
published piece flow from per-user to global (user pushes new version). The
user remains the authority over their published content.

Consequences:

- Deleting a published piece removes it from the global graph. Other pieces
  that cited it retain dangling edges (citation target gone), which is the
  correct semantic -- the citation existed, the target was withdrawn.
- The per-user KB keeps its full history regardless of global KB state.
- There is no reverse sync from global to per-user. The per-user graph is
  upstream.

### 5. Choir as Streaming and Media Platform

Once published pieces include video, audio, and interactive content (not just
text), and the desktop app supports fullscreen playback, Choir becomes a
transparent media layer. The publishing infrastructure is content-type
agnostic -- nodes in the graph can be any media type. The graph structure
(citations, responses, playlists, series) provides the navigation.

This is not a feature to build now. It is a constraint on the graph schema:
do not bake in text-only assumptions. `media_type` on nodes, byte-range
addressability for large blobs, and edge kinds that support sequential
ordering (playlist, series, chapter) keep the door open.

### 6. Revenue Model Implied

Publishing is the natural monetization layer:

- **Free tier**: private KB (SQLite), limited publishing (quota on global
  graph nodes), standard worker pool.
- **Paid tier**: DoltDB per-user KB (richer versioning per ADR-0019
  Section 12), unlimited publishing, larger storage, priority worker pool
  access.

The free tier is fully functional for private use. Publishing is the
value-add that justifies payment -- your work becomes durable, versioned,
citable, and discoverable in the global graph.

## Promotion Operation Design

The publish flow at minimum:

1. User selects a root node (typically a document).
2. System traverses outward edges to a bounded depth, collecting the subgraph.
3. User reviews the subgraph and deselects any nodes they want to keep private.
4. Edges to deselected or untraversed nodes are trimmed.
5. The remaining subgraph is written to the global DoltDB graph as a Dolt
   commit, with authorship and timestamp metadata.
6. The published root node gets a stable URL: `/p/{publish_id}`.

Updates repeat the same flow. Dolt commit history preserves all prior
versions. The `stable` pointer (ADR-0011) maps to the Dolt branch HEAD.

## Risks

1. **Privacy leakage in subgraph promotion.** Edges or node metadata may
   carry private context into the global graph. Mitigation: explicit user
   review step, allowlist of promotable node kinds and edge kinds, automated
   scrubbing of provenance fields that reference private state.
2. **DoltDB operational complexity.** Running a Dolt instance on the
   hypervisor adds a new stateful service. Mitigation: start with a single
   embedded Dolt instance, not a cluster. Scale concerns are distant.
3. **Graph schema drift between per-user and global.** The two schemas will
   evolve at different rates. Mitigation: share node/edge kind vocabularies
   as a shared-types contract, not as schema coupling.
4. **Abuse of the global graph.** Spam publishing, copyright violation,
   illegal content. Mitigation: publishing quotas, content policy enforcement
   at promotion time, moderation tooling (ADR-0025 admin dashboard).

## Consequences

### Positive

- Publishing becomes a structured graph operation with native versioning.
- The global KB accumulates a citation and relationship graph that grows
  more useful as more users publish.
- Revenue model aligns with value delivery (private is free, public is paid).
- Media-type agnosticism keeps the platform extensible.

### Tradeoffs

- DoltDB is a new operational dependency on the hypervisor.
- The promotion operation requires careful privacy engineering.
- Graph-based publishing is more complex than file-based publishing, though
  the user-facing UX can hide that complexity.

## References

- ADR-0011: Bootstrap Into Publishing (State/Compute Decoupling)
- ADR-0019: Per-User Memory Curation and Retrieval (Sections 10-13)
- ADR-0026: Self-Directing Agent Dispatch
