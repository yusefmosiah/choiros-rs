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

- 2026-03-15: Tightens the draft into an execution plan.
  - Defines scope boundaries between per-user memory, global publishing, and
    published serving/runtime behavior.
  - Maps ADR-0011 `stable`/`candidate` semantics onto Dolt branches instead of
    treating them as a replaced model.
  - Defines a bounded v1 promotion contract, privacy scrub rules, and
    withdrawal semantics.
  - Adds phased implementation direction and verification criteria.
- 2026-03-11: Initial draft.
  - Reframed publishing from file export to graph promotion.
  - Declared DoltDB as the global KB store and SQLite as the per-user default.

## What To Do Next

1. Finalize the shared node/edge vocabulary between ADR-0019 and publishing so
   promotion has stable kinds to allowlist and scrub.
2. Implement a promotion planner against the per-user SQLite graph:
   root selection, bounded traversal, review set, trim set, scrub set.
3. Stand up a single embedded DoltDB instance on the hypervisor for the global
   KB prototype with branch policy for `stable` and `candidate`.
4. Build the smallest publish path:
   `POST /v1/publishes`, `POST /v1/publishes/{id}/promote`,
   `POST /v1/publishes/{id}/rollback`,
   `POST /v1/publishes/{id}/withdraw`, `GET /p/{publish_id}`.
5. Add audit, moderation, and quota rails before opening global discovery.

## Context

ADR-0019 defines the private per-user temporal graph. ADR-0011 defines the
runtime and promotion semantics for serving published work. This ADR fills the
missing middle: how private graph structure becomes public graph structure
without leaking private context.

The key planning mistake to avoid is collapsing three different concerns into
one system:

1. Per-user KB: private, upstream, SQLite-first, activity-derived.
2. Global KB: shared, published, versioned, Dolt-backed.
3. Published runtime: routes, read views, prompts, forks, reconcile workers.

They are connected, but they are not the same authority. The per-user graph is
upstream. The global KB is a projection. The published runtime is how that
projection is served and interacted with.

## Decision

### 1. Publishing Is Bounded Subgraph Promotion

User selects nodes (a document, its citations, supporting research) and
promotes them from the per-user SQLite graph to the global DoltDB graph. The
edges between promoted nodes are preserved. Edges to non-promoted nodes are
trimmed -- private context stays private.

This means the publish operation is not "export this file." It is "project
this connected subgraph into the public namespace, preserving internal
structure and trimming external references that point to private state."

Implications:

- The publish unit is a graph slice, not a blob.
- The promoted subgraph must be explicit and reviewable before commit.
- Promotion must be deterministic from a root node plus traversal/scrub policy.

### 2. The Global KB Is a Shared Graph, Not a Document Store

Published pieces have edges to other published pieces -- citations, responses,
derivatives, translations. This structure emerges from the publishing act, not
from manual linking.

The global graph schema mirrors the per-user temporal graph schema (ADR-0019
Section 11) but adds publication-specific fields on the published root and on
published revisions:

- `publish_id`
- `author_id`
- `license`
- `media_type`
- `created_at`
- `updated_at`
- `withdrawn_at` (nullable)
- `source_user_node_id` (author-visible only)
- `source_user_version_id` (author-visible only)

Cross-publish edge rule:

- If the destination is already public, keep the graph edge.
- If the destination is included in the same promotion set, keep the edge.
- If the destination is private and not promoted, trim the edge.
- If the user wants public attribution to a private source, emit a bounded
  citation payload on the published node, not a live edge back into the
  private graph.

### 3. Version History Is Native

DoltDB's prolly trees with structural sharing mean storing hundreds of
versions of a published piece costs proportional to diffs, not full
documents. Every edit to a published piece is a Dolt commit. Readers can
view any historical version. Diff between versions is a first-class
operation.

This does not replace ADR-0011 pointer semantics. It implements them:

- `candidate` is a Dolt branch for staged but not yet serving revisions.
- `stable` is a Dolt branch for the currently served revision.
- rollback is a branch move or revert commit, depending on audit policy.

The full commit graph is the history. `stable` and `candidate` remain the
serving contract.

### 4. Publishing Does Not Copy, It Projects

The per-user KB retains the full private context (drafts, failed attempts,
private notes). The global KB gets the promoted subgraph. Updates to the
published piece flow from per-user to global (user pushes new version). The
user remains the authority over their published content.

Consequences:

- There is no reverse sync from global to per-user. The per-user graph is
  upstream.
- The per-user KB keeps its full history regardless of global KB state.
- Withdrawal is not hard deletion. A withdrawn piece is tombstoned on the
  serving branch, and readers see that the target was withdrawn.
- Hard purge is reserved for policy/legal cases and is an admin action, not
  the normal user-facing unpublish path.

### 5. Promotion Must Enforce an Admission and Scrub Policy

Privacy safety is part of the publish contract, not a UX suggestion.

Admission rules:

- Only allowlisted node kinds can be promoted in v1.
- Only allowlisted edge kinds can cross into the global graph in v1.
- Private provenance fields must be scrubbed or replaced with bounded public
  metadata.
- Promotion must fail closed if the planner cannot classify a node, edge, or
  field.

Initial v1 allowlist:

- Node kinds: `document`, `claim`, `citation`, `source`, `argument`, `media`
- Edge kinds: `cites`, `supports`, `contradicts`, `responds_to`, `contains`

Everything else stays private until deliberately modeled.

### 6. Minimal v1 Publish Contract

The first implementation should be intentionally narrow:

1. User selects a root node, usually a `document`.
2. System traverses a bounded depth over allowlisted edges.
3. System produces a review set, trim set, and scrub summary.
4. User confirms or deselects nodes before first publish.
5. System writes the promoted graph to Dolt as a `candidate` revision.
6. User promotes `candidate` to `stable`.
7. The published root gets a stable URL at `/p/{publish_id}`.

Updates repeat the same flow. Reader prompts, forks, and reconcile workers
remain governed by ADR-0011; this ADR only defines what the published state is.

### 7. Choir as Streaming and Media Platform

Once published pieces include video, audio, and interactive content (not just
text), and the desktop app supports fullscreen playback, Choir becomes a
transparent media layer. The publishing infrastructure is content-type
agnostic -- nodes in the graph can be any media type. The graph structure
(citations, responses, playlists, series) provides the navigation.

This is not a feature to build now. It is a constraint on the graph schema:
do not bake in text-only assumptions. `media_type` on nodes, byte-range
addressability for large blobs, and edge kinds that support sequential
ordering (playlist, series, chapter) keep the door open.

### 8. Revenue Model Implied

Publishing is the natural monetization layer:

- **Free tier**: private KB (SQLite), limited publishing (quota on global
  graph nodes), standard worker pool.
- **Paid tier**: DoltDB per-user KB (richer versioning per ADR-0019
  Section 12), unlimited publishing, larger storage, priority worker pool
  access.

The free tier is fully functional for private use. Publishing is the
value-add that justifies payment -- your work becomes durable, versioned,
citable, and discoverable in the global graph.

## Implementation Direction

### Phase 1: Shared Contracts

- Finalize shared node and edge kind vocabulary with ADR-0019.
- Define scrub rules for private provenance fields.
- Define publish identifiers and revision metadata.

### Phase 2: Single-Piece Publish Flow

- Stand up one hypervisor-local DoltDB instance.
- Implement promotion planning and review against the per-user SQLite graph.
- Support root document plus directly connected public citation graph.
- Ship `candidate`, `stable`, promote, rollback, and withdraw flows.

### Phase 3: Published Runtime Integration

- Wire the published graph into `GET /p/{publish_id}` and status views.
- Reuse ADR-0011 runtime modes against the graph-backed published state.
- Add audit records for publish, promote, rollback, withdraw, and reconcile.

### Phase 4: Discovery and Moderation

- Add search and related-piece discovery over the global graph.
- Add quota enforcement, abuse workflows, and admin moderation tooling.
- Add policy/legal purge flow distinct from normal withdrawal.

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

## Verification

- [ ] Promotion is deterministic from root node plus traversal/scrub policy.
- [ ] A publish review clearly shows promoted nodes, trimmed edges, and fields
  scrubbed before commit.
- [ ] `stable` and `candidate` semantics from ADR-0011 are preserved on top of
  Dolt revision history.
- [ ] Rollback can restore a prior served revision without deleting commit
  history or bypassing audit.
- [ ] Withdrawing a piece removes it from normal serving without destroying
  audit history.
- [ ] Private-only nodes and provenance fields do not appear in the global KB.
- [ ] Cross-publish citations resolve as graph edges only when the destination
  is public.
- [ ] `GET /p/{publish_id}` can render the published root and version metadata.

## References

- ADR-0011: Bootstrap Into Publishing (State/Compute Decoupling)
- ADR-0019: Per-User Memory Curation and Retrieval (Sections 10-14)
- ADR-0026: Self-Directing Agent Dispatch
