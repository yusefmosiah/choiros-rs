# ADR-0017: Temporal Knowledge Graph, Citations, and Queryable Knowledge

Date: 2026-03-09
Kind: Decision
Status: Draft
Priority: 3
Requires: [ADR-0001]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

Living documents make claims. Claims need sources. Sources go stale. Documents cite
each other and the global knowledge base. Users and agents need to query across all
of this. The embedding treadmill (model lock-in, re-indexing costs, API dependency)
is a trap.

This ADR defines three interlocking subsystems: (1) a **temporal knowledge graph**
materialized as a CQRS read projection from the EventStore, (2) a **first-class
citation system** with content-addressable claims, provenance tracking, and freshness
TTLs, and (3) a **queryable knowledge layer** built on Tantivy BM25 + structured
graph queries + LLM re-ranking — deliberately avoiding dense vector embeddings as
the primary retrieval mechanism.

## What Changed

- Introduced temporal facts table as a read projection from EventStore (bitemporal model)
- Elevated citations from metadata to first-class entities with lifecycle and trust scores
- Chose Tantivy (Rust Lucene) over vector DB as the primary retrieval engine
- Made living documents and the global KB queryable through a unified search interface
- Defined a citation registry actor for cross-document provenance tracking

## Context

### The Problems

**1. Claims without provenance are noise.** Living documents contain assertions derived
from OSINT feeds, other documents, and agent research. Without traceable citations, a
reader cannot assess whether "seismic activity in region X is elevated" comes from USGS
10 minutes ago or from a stale web search last week.

**2. Knowledge decays.** A citation to a USGS earthquake feed has a TTL of minutes. A
citation to an academic paper has a TTL of months. Without freshness tracking, the system
cannot distinguish current intelligence from historical artifacts.

**3. Documents cite documents.** When Document A cites Document B's Claim 7, and Claim 7
is later revised, Document A's citation is silently broken. Content-addressable claims
solve this — hash the claim, detect when the target changes.

**4. The embedding lock-in trap.** Vector databases require committing to an embedding
model. When a better model arrives (and it will), you must re-embed your entire corpus
and rebuild indices. Self-hosted embedding models are operationally expensive. API
embeddings create vendor dependency. BM25 has no model to lock into.

**5. Published documents need to be queryable.** Before OSINT feeds, we're enabling
document publishing. Published living documents must be queryable alongside the global
KB — by other agents, by users, and by the documents' own research cycles.

### Key Insight: EventStore Is Already the Write Side

The EventStore is an append-only event log. A temporal knowledge graph is a read-side
projection of that log — structured facts materialized from events, with validity
intervals tracking when each fact was true. This is textbook CQRS: events are the
source of truth, the TKG is a queryable view.

Similarly, citations are already events (`citation.proposed`, `citation.confirmed`).
A citation registry actor materializes a cross-document index from those events.

### Prior Art

- **Bitemporal databases** (SQL:2011): two time axes — valid time (when true in world)
  and transaction time (when recorded in system). Zep/Graphiti uses this for AI agent memory.
- **Nanopublications**: atomic, citable, attributed assertions with four parts — assertion,
  provenance, publication info, identity. We adopt the decomposition without the RDF.
- **The Underlay** (MIT): content-addressable assertions with PROV-O provenance. Validates
  the content-hash approach at scale.
- **Raphtory**: Rust temporal graph engine with lazy materialization of time-windowed views.
  Proves the approach works in Rust at scale (129M edges in 25 seconds).
- **Tantivy**: Rust search library (Lucene-equivalent), used by Quickwit, Qdrant, ParadeDB.
  BM25 achieves 0.92 F1 on TREC benchmarks. Sub-millisecond latency.

## Decision

### Part 1: Temporal Knowledge Graph

Facts are materialized from EventStore events into a bitemporal fact table. Each fact
tracks both when it was true in the world (valid time) and when the system learned it
(transaction time).

```sql
CREATE TABLE temporal_facts (
    fact_id       INTEGER PRIMARY KEY,
    subject_type  TEXT NOT NULL,     -- "document", "entity", "agent", "feed"
    subject_id    TEXT NOT NULL,
    predicate     TEXT NOT NULL,     -- "claims", "authored_by", "references", "located_at"
    object_type   TEXT NOT NULL,
    object_id     TEXT NOT NULL,
    valid_from    TEXT NOT NULL,     -- ISO8601, when fact became true
    valid_until   TEXT,              -- NULL = still valid
    recorded_at   TEXT NOT NULL,     -- when system learned this
    superseded_at TEXT,              -- when this record was corrected (NULL = current)
    source_event_seq INTEGER,       -- FK to event_store.seq
    confidence    REAL DEFAULT 1.0,
    metadata      TEXT               -- JSON for extension
);

CREATE INDEX idx_facts_subject ON temporal_facts(subject_id, predicate, valid_from);
CREATE INDEX idx_facts_object  ON temporal_facts(object_id, predicate, valid_from);
CREATE INDEX idx_facts_time    ON temporal_facts(valid_from, valid_until);
```

**Query patterns** (no SPARQL — just SQL):
- **Point-in-time snapshot**: `WHERE valid_from <= ?T AND (valid_until IS NULL OR valid_until > ?T)`
- **What changed in range**: `WHERE valid_from BETWEEN ?T1 AND ?T2 OR valid_until BETWEEN ?T1 AND ?T2`
- **Entity timeline**: `WHERE subject_id = ? AND predicate = ? ORDER BY valid_from`

**Materializer**: A `FactProjectorActor` subscribes to the EventBus, watches for
relevant events (document versions, OSINT feed data, research findings), extracts
structured facts, and writes them to the temporal_facts table. When new facts contradict
existing ones, it sets `valid_until` on the old fact.

This is a **Slowly Changing Dimension Type 2** (SCD2) — well-understood, efficient,
trivial to implement in SQLite.

### Part 2: First-Class Citations

Citations are promoted from metadata annotations to entities with their own lifecycle,
provenance, freshness tracking, and trust scores.

#### Citable Claims

Each claim within a document is addressable by position AND content hash:

```rust
struct Claim {
    claim_id: String,                    // ULID
    document_id: String,
    version_id: u64,
    claim_index: u32,                    // position within document
    content_hash: String,                // BLAKE3 of normalized claim text
    content: String,
    confidence: f64,                     // computed from citations
    created_at: DateTime<Utc>,
    created_by: String,                  // agent that authored this claim
}
```

Content hashing uses BLAKE3 (fast, available via `blake3` crate) on normalized claim
text (trimmed, whitespace-collapsed). When a cited claim is edited, the hash changes,
and the citing document's citation is flagged as potentially stale.

#### Citation Records

Inspired by nanopublication structure (assertion + provenance + publication info + identity)
but implemented as flat Rust structs, not RDF:

```rust
struct Citation {
    citation_id: String,                 // ULID

    // What is citing (the "from" side)
    citing_document_id: String,
    citing_version_id: u64,
    citing_claim_id: Option<String>,     // None = document-level citation

    // What is cited (the "to" side)
    cited_kind: CitedKind,
    cited_id: String,                    // URL, document_id, event_id, etc.
    cited_content_hash: Option<String>,  // hash of cited material at citation time

    // Provenance (PROV-inspired, not PROV-O)
    generated_by_activity: String,       // "research_run:01ABC", "writer_revision:01DEF"
    attributed_to: String,               // agent ID

    // Freshness
    cited_at: DateTime<Utc>,
    source_checked_at: DateTime<Utc>,
    source_data_timestamp: Option<DateTime<Utc>>,
    ttl_seconds: Option<u64>,            // source-dependent freshness window

    // Trust
    source_trust: f64,                   // 0.0..1.0, per source type
    excerpt: Option<String>,             // supporting excerpt from source

    // Lifecycle
    status: CitationStatus,
}

enum CitedKind {
    ExternalUrl,
    OsintFeed,          // GDELT, USGS, financial API
    LivingDocument,     // another document in the system
    GlobalKb,           // shared knowledge base entry
    VersionSnapshot,    // specific version of a document
}

enum CitationStatus {
    Proposed,           // researcher suggested
    Confirmed,          // writer accepted
    Stale,              // past TTL, needs re-verification
    Invalidated,        // source inaccessible or contradicted
    Superseded,         // replaced by newer citation
}
```

#### Source-Dependent TTLs

| Source Type | Default TTL | Rationale |
|-------------|-------------|-----------|
| USGS earthquake feed | 5 minutes | Seismic data updates continuously |
| Financial market data | 1 minute (market hours) | Prices move fast |
| GDELT events | 1 hour | 15-min batch updates, allow for processing lag |
| News RSS | 30 minutes | Articles don't change but relevance decays |
| Another living document | Tied to source doc update frequency | Re-check when source updates |
| Academic paper / static URL | 30 days | Content rarely changes |
| Global KB entry | 1 hour | KB entries may be updated by other agents |

#### Trust Propagation

Simple weighted formula, not a Bayesian network:

```
claim_confidence = mean(source_trust[i] * freshness_factor[i]) * corroboration_bonus
```

Where:
- `freshness_factor` decays from 1.0 to 0.3 as citation ages past TTL
- `corroboration_bonus` = 1.0 + 0.1 * (independent_source_count - 1), capped at 1.5
- Source trust defaults: government feeds 0.95, established news 0.80, GDELT 0.70,
  web search 0.50, other living documents inherit their own confidence

When a source's trust changes or a citation goes stale, confidence propagates to all
claims citing it. This is handled by the citation registry actor.

#### Citation Registry Actor

A new actor that materializes a cross-document citation index from citation events:

- Maintains a deduplicated index of all cited sources across all documents
- Tracks the reverse index: which documents cite which sources
- Detects corroboration: when two documents independently cite the same source
- Propagates staleness: when a source is re-checked and found changed, emits
  `citation.stale` events for all documents citing it
- Emits `citation.content_hash_mismatch` when a cited claim's content hash no longer
  matches (the target claim was edited)

This is the WikiCite Shared Citations pattern, implemented as an actor.

### Part 3: Queryable Knowledge Layer

Living documents and the global KB are queryable through a three-layer retrieval stack
that deliberately avoids dense vector embeddings as the primary mechanism.

#### Layer 1: Tantivy Full-Text Search (Primary)

All living documents and KB entries indexed with BM25 via Tantivy. This is the workhorse.

```
tantivy = "0.22"   # Rust crate
```

**Schema fields per document:**
- `document_id` (stored, indexed)
- `title` (text, indexed with boost)
- `body` (text, indexed)
- `claims` (text, indexed with boost) — extracted claim texts, higher weight
- `source_type` (facet) — "living_document", "kb_entry", "osint_feed"
- `topic_facet` (facet) — hierarchical topic taxonomy
- `confidence` (fast field) — for filtering by minimum confidence
- `updated_at` (fast field) — for recency weighting
- `author_agent` (facet) — which agent(s) contributed

**Faceted search** is the killer feature for structured knowledge: "Show me all claims
about earthquakes from OSINT sources with confidence > 0.8 in the last 24 hours."
No embeddings needed.

**Incremental indexing**: Tantivy supports add/delete operations. When a document is
updated, delete the old entry and add the new one. The `IndexWriter` handles segment
merging automatically.

#### Layer 2: Structured Graph Queries (Precision)

For queries that text search cannot answer — "What contradicts claim X?",
"What is the provenance chain for assertion Y?", "What claims were derived from
source Z?" — query the temporal facts table and citation registry directly.

These are SQL queries against `temporal_facts` and the citation index, not SPARQL.
The graph structure lives in the foreign key relationships between facts, claims,
citations, and documents.

For graph traversal (multi-hop queries, related claim discovery), use `petgraph`
in-memory for the active working set:

```
petgraph = "0.6"    # Rust crate
```

The citation graph itself is a retrieval signal: documents heavily cited about topic X
are likely relevant to queries about X, analogous to PageRank.

#### Layer 3: LLM Re-ranking (Quality)

Top-50 BM25 candidates are re-ranked by the LLM via existing BAML infrastructure.
This is the single highest-quality improvement per unit of effort, and it's essentially
free since the LLM infrastructure already exists.

```
Query → Tantivy BM25 (top 50) → LLM re-rank (top 5-10) → Results
```

The re-ranking prompt is simple: "Given this query and these excerpts, rank by relevance
and explain your ranking." This gives semantic understanding without embedding lock-in —
you swap the LLM, not the index.

#### Why Not Vector DB?

| Concern | Vector DB | This approach |
|---------|-----------|---------------|
| Model lock-in | Committed to one embedding model | No embedding model dependency |
| Re-indexing cost | Must re-embed entire corpus on model change | Index is model-independent |
| Operational complexity | Embedding service + vector store | Tantivy is a library, not a service |
| Self-hosting embeddings | GPU required, or API dependency | CPU only |
| Retrieval quality | Good for semantic similarity | BM25 + LLM re-rank matches quality |
| Structured queries | Weak (vectors don't encode structure) | Strong (SQL + graph) |
| Faceted filtering | Bolted on | Native in Tantivy |

**Escape hatch**: If we later decide embeddings are valuable, they can be added as a
supplementary index — a disposable layer that can be rebuilt when models improve.
SPLADE sparse embeddings via `embed_anything` crate (ONNX, no PyTorch) are the
lowest-risk option, since they output weighted terms (interpretable, debuggable).

### Unified Query Interface

A single `KnowledgeQueryActor` presents a unified interface:

```rust
enum KnowledgeQuery {
    /// Full-text search across all documents and KB
    Search {
        query: String,
        filters: SearchFilters,
        limit: usize,
    },
    /// Structured query: find claims by subject/predicate/object
    FactQuery {
        subject_id: Option<String>,
        predicate: Option<String>,
        object_id: Option<String>,
        at_time: Option<DateTime<Utc>>,
    },
    /// Citation query: what cites this? what does this cite?
    CitationQuery {
        target_id: String,
        direction: CitationDirection,  // Inbound, Outbound, Both
    },
    /// Provenance chain: trace a claim back to its sources
    ProvenanceChain {
        claim_id: String,
        max_depth: usize,
    },
}

struct SearchFilters {
    source_types: Option<Vec<String>>,
    min_confidence: Option<f64>,
    date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    topic_facets: Option<Vec<String>>,
    exclude_stale: bool,
}
```

Published living documents are indexed in Tantivy alongside KB entries. From the
query interface's perspective, a published document is just another searchable
entity with `source_type: "living_document"` — no separate system needed.

## Implementation Phases

### Phase 1: Temporal Facts Table + Fact Projector
- Add `temporal_facts` SQLite table via migration
- Implement `FactProjectorActor` that watches EventBus for document versions and
  OSINT feed events, materializes structured facts
- Point-in-time and range queries as SQL

### Phase 2: Citation Schema + Registry
- Extend `CitationRecord` in shared-types with freshness, trust, content hash fields
- Implement claim extraction: diff consecutive document versions to identify new/changed claims
- Implement `CitationRegistryActor` for cross-document citation tracking
- Background citation auditor checking TTLs, emitting staleness events

### Phase 3: Tantivy Search Index
- Add Tantivy as dependency, define schema with facets
- Index all existing documents and KB entries
- Incremental indexing on document update events
- Basic search API endpoint

### Phase 4: Unified Query Interface + LLM Re-ranking
- `KnowledgeQueryActor` combining Tantivy search, fact queries, citation queries
- LLM re-ranking via BAML for search results
- Graph-based retrieval using citation links as relevance signals
- Expose via API for frontend and agent consumption

### Phase 5 (Future): Publishing Layer
- Published documents enter the Tantivy index with `source_type: "published_document"`
- Other users' agents can query published documents alongside KB
- Citation links across published documents create a public knowledge graph

## Consequences

### Positive
- No embedding model lock-in — primary retrieval is model-independent
- Citations are traceable from claim to source with freshness and trust
- Temporal facts enable "what was true when?" queries (essential for OSINT)
- Published documents are queryable without separate infrastructure
- Builds on existing EventStore/actor architecture (CQRS natural fit)
- Tantivy is a library (no service to operate), Rust-native, production-grade

### Negative
- BM25 misses pure semantic similarity (mitigated by LLM re-ranking)
- Claim extraction from documents requires LLM calls (cost)
- Citation tracking adds complexity to the writer flow
- Temporal facts table grows with every fact change (mitigated by SCD2 compactness)

### Risks
- Claim extraction quality depends on LLM capability — bad extraction means bad citations
  - Mitigation: start with coarse-grained claims (paragraph-level), refine later
- Tantivy index and SQLite can diverge if updates fail partially
  - Mitigation: transactional updates, eventual consistency via EventStore replay
- Trust scores are subjective and may not generalize
  - Mitigation: make trust defaults configurable, allow user overrides

## Key Crate Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tantivy` | 0.22+ | Full-text search with BM25, facets |
| `blake3` | 1.x | Content-addressable claim hashing |
| `petgraph` | 0.6+ | In-memory graph traversal for citation networks |

All three are pure Rust, no C dependencies, no external services.

## References

- [Raphtory — Rust temporal graph engine](https://github.com/Pometry/Raphtory)
- [Graphiti/Zep — bitemporal KG for AI agent memory](https://github.com/getzep/graphiti)
- [Tantivy — Rust search engine library](https://github.com/quickwit-oss/tantivy)
- [Oxigraph — Rust SPARQL database](https://github.com/oxigraph/oxigraph)
- [The Underlay — content-addressable assertions](https://underlay.mit.edu/)
- [Nanopublications](https://nanopub.net/)
- [W3C PROV-O](https://www.w3.org/TR/prov-o/)
- [Knowledge-Based Trust (Google, 2015)](https://arxiv.org/abs/1502.03519)
- [WikiCite Shared Citations](https://meta.wikimedia.org/wiki/WikiCite/Shared_Citations)
- [Survey: Temporal Knowledge Graph Representation Learning](https://arxiv.org/abs/2403.04782)

## Verification

- [ ] `temporal_facts` table exists and is populated by FactProjectorActor
- [ ] Citations have content hashes that detect target edits
- [ ] TTL-based staleness detection works for at least 3 source types
- [ ] Tantivy index returns relevant results for test queries
- [ ] Published documents appear in search results alongside KB entries
- [ ] Provenance chain query traces a claim through 2+ hops to original source
- [ ] LLM re-ranking measurably improves result quality over raw BM25
