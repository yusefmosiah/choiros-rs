# MemoryAgent Architecture: Episodic Memory + Global Knowledge

Date: 2026-02-16
Status: Design (no code written)
Author: Human + Agent collaborative design

## Narrative Summary (1-minute read)

ChoirOS agents do deterministic work on the filesystem — grep, find, read, write.
That doesn't change. The filesystem is the source of truth for the current state of things.

MemoryAgent adds **episodic memory**: the associative layer that fires at the moment of
new input and surfaces resonant patterns from past sessions. "You worked on something like
this 3 days ago and here's what succeeded." "This new living doc overlaps with 2 existing
ones." "Last time this objective appeared, the conductor chose strategy Y and it scored well."

This is what grep cannot do — fuzzy temporal/semantic association across sessions and users.

Two scopes:
- **Local (per-user):** Session history, conductor strategy outcomes, personal patterns.
  Stored as `.rvf` files (RuVector Format) inside the user's Firecracker VM.
- **Global (platform):** Published documents, agents, apps — intellectual property that
  users opt into sharing. Stored in a central RVF-backed service. Users benefit from
  each other's published learnings.

SONA (Self-Optimizing Neural Architecture) makes local retrieval scoring improve over time.
It doesn't train a model — it adjusts embeddings in-place using tiny LoRA matrices so
that queries bias toward patterns that led to successful outcomes.

## What Changed

- Clarified that filesystem/grep/find remains the primary agent retrieval path for
  deterministic work. MemoryAgent is the associative layer on top, not a replacement.
- Identified three trigger points: user input, living doc creation, SONA trajectory completion.
- Added global knowledge layer for published intellectual property across the platform.
- Completed deep analysis of the full RuVector ecosystem (13+ crates).
- **Corrected storage layer:** RVF (RuVector Format) is the file format for vector
  persistence, not redb. The RVF stack (`rvf-runtime` + `rvf-index` + `rvf-types`)
  provides append-only vector storage with progressive HNSW indexing built in.
  `ruvector-core` with redb is a separate, older system — we use RVF instead.
- Only `ruvllm` is excluded (local LLM inference — wrong abstraction for us).

## What To Do Next

1. Implement Phase 1: MemoryAgent actor + RVF storage + HNSW search + SONA learning
   + hypergraph episode grouping + causal memory. This is the minimum viable
   *intelligent* episodic memory, not just a vector store.
2. Implement Phase 2: Hyperbolic HNSW + DualSpaceIndex for hierarchy-aware retrieval.
3. Implement Phase 3: GNN neural search + reflexion + skills + Cypher queries.
4. Implement Phase 4: Global knowledge store with published IP and cross-user SONA.

---

## 1. The Two Retrieval Paths

ChoirOS agents have two fundamentally different retrieval needs:

### Deterministic retrieval (filesystem)

```
User asks: "fix the bug in the auth module"
  -> Terminal agent greps for auth-related files
  -> Reads them, understands the code, patches it
  -> This is grep/find/read — exact, deterministic, filesystem-native
```

This is what Claude Code and Codex do. It's what our Terminal and Researcher agents
do via `file_read`, `file_write`, `file_edit`, `bash`. Nothing changes here.

### Associative retrieval (episodic memory)

```
User asks: "fix the bug in the auth module"
  -> Before conductor plans, MemoryAgent fires on the input
  -> Returns: "3 days ago you fixed a similar auth race condition by adding
     a mutex in session_manager.rs — that run succeeded with high quality score"
  -> Conductor hydrates its planning context with this episode
  -> Plans more effectively because it has relevant history
```

This is what grep cannot do. The input "fix the bug in the auth module" has no
exact-match relationship to a past event about "auth race condition in session_manager.rs".
The connection is semantic — similar intent, similar domain, similar outcome patterns.

**Rule: Filesystem is truth. Memory is resonance. They serve different purposes.**

---

## 2. RuVector Ecosystem: What We Use and What We Skip

The ruvector monorepo contains 13+ crates. We use the RVF stack for durable vector
storage, plus ruvector's graph, GNN, and learning crates for intelligent retrieval.

### Core Dependencies (Phase 1-2)

| Crate | Version | Purpose in ChoirOS |
|---|---|---|
| `rvf-runtime` | 0.1.0 | Vector store runtime. Append-only `.rvf` files with progressive HNSW. The durable persistence layer. |
| `rvf-index` | 0.1.0 | Pure-Rust HNSW with progressive Layer A/B/C loading. Automatic via rvf-runtime. |
| `rvf-types` | 0.1.0 | Format spec types. Transitive dependency of rvf-runtime. |
| `ruvector-sona` | 0.1.5 | Adaptive learning. MicroLoRA + EWC++ + ReasoningBank. Makes retrieval improve over time. |
| `ruvector-core` | 2.0.1 | HypergraphIndex, CausalMemory, AgenticDB, HybridSearch, MMR. In-memory graph intelligence loaded from RVF-persisted vectors. |
| `ort` | latest | ONNX Runtime for MiniLM-L6-v2 embeddings. 384-dim, ~1ms/embed on CPU. |

### Extended Dependencies (Phase 3-4)

| Crate | Purpose in ChoirOS |
|---|---|
| `ruvector-graph` | Cypher query language — parser, cost-based optimizer, pipeline executor. Expressive episode queries. |
| `ruvector-gnn` | GCN, GraphSAGE, GAT, neural/differentiable search. Graph-refined retrieval. |
| `ruvector-filter` | Rich filter DSL (geo, text match, null, exists). Advanced metadata filtering. |
| `ruvector-delta-graph` | Delta-aware traversal, shortest path, connected components. |
| `ruvector-dag` | DAG traversal (topological, DFS, BFS). Dependency ordering for episode chains. |

### Why RVF for persistence (with ruvector-core for graph intelligence)

`ruvector-core` uses redb for its own internal storage, but **we don't use its
storage layer**. We use ruvector-core's in-memory graph structures (HypergraphIndex,
CausalMemory, AgenticDB, HybridSearch, MMR) populated from RVF-persisted data.
The graph structures are rebuilt on startup from the `.rvf` file's Journal and
Meta segments.

`rvf-runtime` is the durable persistence layer with key advantages:

1. **Self-contained `.rvf` files.** One file = one vector store. No external state,
   no separate index files, no database process. The file IS the database.
   Perfect for per-user isolation in Firecracker VMs.

2. **Progressive HNSW loading (Layer A/B/C).** The HNSW graph is split into three
   independently-loadable tiers stored as separate `INDEX_SEG` segments:

   | Layer | Contents | Load Time | Recall@10 |
   |-------|----------|-----------|-----------|
   | A | Entry points + top HNSW layers + cluster centroids | < 5 ms | ~0.70 |
   | B | Partial adjacency for hot nodes (10-20% of data) | 100ms-1s | ~0.85 |
   | C | Full HNSW adjacency for every node at every level | Seconds | >= 0.95 |

   You can start answering queries at 70% recall within 5ms of opening the file.
   Full recall improves in the background. This is ideal for MemoryAgent startup.

3. **Append-only writes.** No in-place mutation. Crash-safe by construction.
   Compaction reclaims space from deleted records without blocking queries.

4. **COW branching.** Fork a vector store like a Git branch. Useful for:
   - Snapshotting user memory before experimental sessions
   - Publishing a copy of local patterns to global store without modifying the original

5. **Minimal dependency chain.** `rvf-runtime` -> `rvf-types` -> (optionally) `serde`.
   The HNSW implementation in `rvf-index` has zero runtime dependencies.

6. **Firecracker-friendly.** Append-only files + mmap survive VM snapshot/restore
   naturally. No database process to restart.

### The HNSW Implementation in rvf-index

`rvf-index` contains a self-contained HNSW implementation (Malkov & Yashunin 2018):

- **Algorithm:** Greedy search at upper layers, beam search at layer 0
- **Defaults:** M=16, M0=32, ef_construction=200
- **Layer selection:** `level = floor(-ln(rand) * (1/ln(M)))`
- **Bidirectional edges** with pruning (keep closest `max_neighbors`)
- **`VectorStore` trait abstraction** — vectors accessed through a trait, not stored in graph
- **`no_std` compatible** — uses `BTreeMap`/`BTreeSet` as fallbacks

The binary wire format for `INDEX_SEG` segments uses delta-encoded neighbor IDs
(LEB128 varints) with restart points every 64 nodes for random access. Compact
on disk, fast to deserialize.

### What we skip

| Crate | Why we skip it |
|---|---|
| `ruvllm` (v2.0.2, 84K LoC) | Full local LLM inference engine (Candle framework, Metal/CUDA shaders, paged attention, GGUF loading). ChoirOS uses external model providers (Claude Bedrock, ZaiGLM) via BAML. Our embedding needs are served by MiniLM-L6-v2 via ONNX Runtime — a 22MB encoder model, not a generative LLM. Using ruvllm for this would triple compile time and binary size for zero value. |
| `ruvector-gnn-node` | Node.js bindings for GNN. We're pure Rust. |
| `ruvector-gnn-wasm` | WASM bindings for GNN. Not needed server-side. |
| `ruvector-postgres` | PostgreSQL extension with GNN operators. We don't use Postgres for vectors. (Though the GCN/GraphSAGE implementations there are the reference source.) |

### Additional RVF crates (available but not required for Phase 1)

The `crates/rvf/` directory contains additional crates that may be useful later:

- `rvf-wire` — Low-level segment header I/O
- `rvf-cli` — CLI tools (`rvf create`, `rvf ingest`, `rvf query`) — useful for debugging
- Others (TBD as ecosystem matures)

These are optional. `rvf-runtime` + `rvf-types` is the minimal dependency set,
and `rvf-runtime` pulls in `rvf-index` internally.

### Ecosystem health caveat

The entire RuVector ecosystem is young:
- Created November 2025, single maintainer (`ruvnet`)
- ~4K total crate downloads for `ruvector-core`
- 891 commits, actively developed
- `rust_version` requirement is 1.87

Mitigation: Wrap behind traits (`VectorStore`, `LearningEngine`) so implementations
can be swapped if the ecosystem stalls or breaks. Pin exact versions. The `.rvf` file
format is simple enough (append-only segments) that we could write our own reader
if the crate is abandoned.

---

## 3. Local Memory Architecture (Per-User Episodic Memory)

### 3.1 Actor Placement

```
ApplicationSupervisor
  ├── EventBusActor
  ├── EventRelayActor (polls EventStore -> broadcasts to EventBus)
  └── SessionSupervisor
        ├── DesktopSupervisor
        ├── TerminalSupervisor
        ├── ResearcherSupervisor
        ├── WriterSupervisor
        └── MemorySupervisor          <-- NEW
              └── MemoryAgent(user_id) <-- per-user actor
```

MemoryAgent is session-scoped. Each user gets their own actor with their own
vector store and SONA state. Persistence path: `data/{user_id}/memory.rvf`
for vectors (HNSW index baked into the file), `data/{user_id}/sona.json` for
learned weights.

### 3.2 What Gets Stored

A `MemoryRecord` is the unit of episodic memory. It carries **provenance metadata**
(who, when, where, from what event) but NOT semantic categories. The semantic
structure — "this is a strategy", "this is a finding", "these cluster together" —
emerges from the embeddings, the HNSW graph topology, the hypergraph edges,
and SONA's learned weights. We don't hand-label it.

```rust
pub struct MemoryRecord {
    pub id: String,                    // ULID
    pub embedding: Vec<f32>,           // 384-dim from MiniLM-L6-v2
    pub text: String,                  // human-readable content
    pub quality_score: Option<f32>,    // SONA outcome score (0.0-1.0)

    // Provenance (scope + traceability, NOT semantic classification)
    pub user_id: String,
    pub session_id: String,
    pub run_id: Option<String>,        // conductor run that produced this
    pub actor_id: String,              // which actor emitted the source event
    pub event_type: String,            // raw event type from EventStore
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,   // flexible extra data from the source event
}
```

**Why no `MemoryKind` or `MemorySource` enums:**

The embeddings already encode semantic structure. HNSW clusters similar memories.
Layer A's IVF centroids partition the space into natural categories. SONA learns
which regions correlate with good outcomes. Hardcoding `Strategy` vs `Finding` vs
`Pattern` is:
- Redundant with what the embedding space already represents
- A maintenance burden (every new domain needs new variants)
- A source of miscategorization (is a research synthesis a "finding" or a "pattern"?)
- An obstacle to cross-domain discovery (a coding "strategy" and a research
  "methodology" might cluster together naturally — enum labels would hide this)

What we DO need is provenance: who produced this, when, in what session, from what
event. That's for scoping, isolation, and traceability — not for semantic filtering.

**Filtering by "kind" is replaced by filtering by embedding region.** If the
conductor wants strategies, it phrases its query as a strategy-like sentence.
The embedding space does the rest. If it wants temporal scope, it filters by
timestamp. If it wants session scope, it filters by session_id.

### 3.3 Ingestion Pipeline (EventRelay -> MemoryAgent)

Zero modifications to existing event emitters. A `MemoryEventRelay` subscribes
to EventBus topics and forwards high-signal events to MemoryAgent for embedding
and storage.

```
EventStore -> EventRelayActor -> EventBus -> MemoryEventRelay -> MemoryAgent
                                                (filter + map)    (embed + store)
```

**Event type mapping (~15 high-signal events):**

The relay forwards these events to MemoryAgent. The `event_type` string and
`actor_id` are preserved as provenance. No semantic classification is applied —
the embedding captures the semantics.

| Event Type | What it captures |
|---|---|
| `chat.user_msg` | What the user asked for |
| `conductor.run.started` | What strategy the conductor chose |
| `conductor.assignment.dispatched` | Which capability was assigned what |
| `conductor.run.completed` | Final outcome of the plan |
| `conductor.run.failed` | What went wrong |
| `worker.report.received` | What a worker discovered/produced |
| `worker.task.progress` | Intermediate worker state |
| `worker.task.document_update` | Living doc mutations |
| `researcher.search.completed` | Research results |
| `terminal.command.completed` | Shell execution outcomes |
| `writer.revision.created` | Document version changes |

### 3.4 Retrieval: Three Trigger Points

**Trigger 1: User input (before conductor plans)**

```
User message arrives
  -> MemoryAgent.Recall { query: user_message, top_k: 5 }
  -> Returns: Vec<RetrievedMemory> with SONA-adjusted scores
  -> Injected into conductor's system context before planning
  -> Conductor sees: "Relevant history: [episodes]"
```

**Trigger 2: Living doc creation/mutation**

```
Writer creates or patches a document
  -> MemoryAgent.Recall { query: doc_content_summary, top_k: 3 }
  -> Returns: related documents and past patterns
  -> Writer sees: "Related prior work: [docs/patterns]"
```

**Trigger 3: SONA trajectory completion**

```
Conductor run completes (success or failure)
  -> MemoryAgent.EndTrajectory { trajectory_id, quality_score }
  -> SONA adjusts LoRA weights based on outcome
  -> Future retrievals for similar queries bias toward successful patterns
  -> This is the learning loop
```

### 3.5 MemoryProvider Trait (Integration Contract)

```rust
/// Trait for injecting memory retrieval into existing adapters.
pub trait MemoryProvider: Send + Sync {
    async fn recall(
        &self,
        query: &str,
        top_k: usize,
        filter: Option<MemoryFilter>,
    ) -> Result<Vec<RetrievedMemory>, MemoryError>;
}

pub struct RetrievedMemory {
    pub record: MemoryRecord,
    pub score: f32,              // raw HNSW distance
    pub sona_score: f32,         // SONA-adjusted score (biased by learning)
}

/// Filters are provenance-based (scope, time, actor), not semantic.
/// Semantic filtering happens through the query embedding itself.
pub struct MemoryFilter {
    pub since: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub actor_id: Option<String>,
    pub event_type_prefix: Option<String>,  // e.g. "conductor." or "worker."
}
```

### 3.6 Integration Points (Exact Locations)

| What | File | Where | Change |
|---|---|---|---|
| Spawn MemoryAgent | `supervisor/session.rs` | `SessionSupervisor::pre_start` | Add `MemorySupervisor` spawn alongside existing domain supervisors |
| Route creation msg | `supervisor/mod.rs` | `ApplicationSupervisorMsg` | Add `GetOrCreateMemory { user_id, reply }` variant |
| Conductor args | `actors/conductor/actor.rs` | `ConductorArguments` struct | Add `memory_provider: Option<Arc<dyn MemoryProvider>>` |
| Conductor planning | `actors/conductor/model_gateway.rs` | `CAPABILITY_ROUTING_GUIDANCE` | Inject retrieved memories into system context before routing |
| Worker system context | `actors/terminal.rs` | `get_system_context()` | Append relevant memories to terminal agent's system prompt |
| Worker system context | `actors/researcher/adapter.rs` | `get_system_context()` | Append relevant memories to researcher's system prompt |
| Writer context | `actors/writer/mod.rs` | synthesis/planning points | Inject related documents and patterns |
| App state wiring | `app_state.rs` | `ensure_supervisor()` pattern | Add `ensure_memory()` mirroring existing pattern |
| EventBus subscription | MemoryEventRelay (new) | startup | Subscribe to ~15 event topics listed above |

### 3.7 Resource Envelope

Per-user memory footprint for the local episodic store:

| Tier | Memories | HNSW Index | SONA State | .rvf File | Total RAM | Disk |
|---|---|---|---|---|---|---|
| Standard (10K) | 10K records | Progressive (Layer A ~100KB, full ~15MB) | ~2 KB LoRA + ~500 KB ReasoningBank | ~20 MB | 30-60 MB | 25 MB |
| Pro (100K) | 100K records | Progressive (Layer A ~1MB, full ~150MB) | ~2 KB LoRA + ~5 MB ReasoningBank | ~200 MB | 200-400 MB | 250 MB |

Progressive loading advantage: Layer A loads in <5ms at ~70% recall. The MemoryAgent
is answering queries almost immediately on actor startup, with recall improving in
the background as Layers B and C load.

MiniLM-L6-v2 model: ~22 MB loaded once per process (shared across users in multi-tenant).

Firecracker VM sizing:
- Standard tier: 1 vCPU, 512 MiB RAM, 5 GiB disk (~$8-12/mo)
- Pro tier: 2 vCPU, 1 GiB RAM, 10 GiB disk (~$15-20/mo)

---

## 4. Global Knowledge Architecture (Platform-Wide)

### 4.1 The Vision

Once we deploy with hypervisor, auth, and user accounts, users will publish:
- **Living documents** — research reports, guides, analysis
- **Agents** — custom agent configurations and skill packages
- **Apps** — desktop applications built on the ChoirOS runtime

These are forms of **intellectual property**. When published, their embeddings
enter a global RVF-backed index that all users benefit from.

### 4.2 Two-Tier Memory Model

```
┌─────────────────────────────────────────────────┐
│              GLOBAL KNOWLEDGE STORE              │
│    (Central rvf-runtime instance, platform-wide) │
│                                                   │
│  Published documents, agents, apps               │
│  Cross-user pattern aggregation                  │
│  Platform-level SONA (coordination loop)         │
│                                                   │
│  Access: read by all users, write by publish API │
└──────────────┬──────────────────────────────────┘
               │ query on new input
               │ (augments local results)
               │
┌──────────────┴──────────────────────────────────┐
│         LOCAL EPISODIC MEMORY (per-user)         │
│  (In-VM rvf-runtime instance, single .rvf file)  │
│                                                   │
│  Session history, strategies, personal patterns  │
│  Per-user SONA (instant + background loops)      │
│                                                   │
│  Access: private to the user                     │
└─────────────────────────────────────────────────┘
```

### 4.3 What Gets Published to Global

When a user explicitly publishes a living document, agent, or app:

```rust
pub struct GlobalRecord {
    pub id: String,                      // ULID
    pub embedding: Vec<f32>,             // 384-dim
    pub content_type: GlobalContentType,
    pub title: String,
    pub summary: String,                 // human-readable abstract
    pub author_id: String,               // user who published
    pub published_at: DateTime<Utc>,
    pub version: String,
    pub tags: Vec<String>,
    pub quality_metrics: QualityMetrics, // derived from SONA trajectories
    pub access_policy: AccessPolicy,     // public, org-only, paid-tier
}

pub enum GlobalContentType {
    Document,        // living document / research report
    Agent,           // agent configuration + skill package
    App,             // desktop application package
    Pattern,         // aggregated learning pattern (anonymized)
}

pub struct QualityMetrics {
    pub avg_trajectory_score: f32,   // how well did runs using this content score?
    pub usage_count: u64,            // how many times has this been retrieved?
    pub user_rating: Option<f32>,    // explicit user feedback
}
```

### 4.4 How Global Retrieval Works

At the moment of new input, the retrieval pipeline queries both stores:

```
User input arrives
  │
  ├── Query local MemoryAgent (per-user, fast, ~1ms)
  │     Returns: personal episodes, strategies, patterns
  │
  └── Query global knowledge store (platform-wide, ~5-10ms)
        Returns: relevant published docs, agents, patterns from other users
  │
  ├── Merge and rank results (SONA-adjusted scores)
  │
  └── Inject top-k into conductor/worker system context
```

The merge respects a priority order:
1. **Local high-score matches** — personal history always gets priority
2. **Global high-quality matches** — published content with strong quality metrics
3. **Local low-score matches** — older or less-relevant personal history
4. **Global exploratory matches** — potentially relevant but unproven

### 4.5 Global Store Infrastructure (Future)

The global store is NOT inside any user's VM. It's a platform service:

```
┌─────────────────────────────────────────┐
│     Global Knowledge Service             │
│  (rvf-runtime backed, single .rvf file)  │
│                                           │
│  ├── Progressive HNSW (all published)    │
│  ├── SONA coordination loop              │
│  ├── Publish API (authed)                │
│  ├── Query API (authed, rated)           │
│  └── COW branches for versioned snapshots│
└─────────────────────────────────────────┘
         │              │
    ┌────┘              └────┐
    │                        │
  User A's VM           User B's VM
  (local .rvf)          (local .rvf)
```

**SONA coordination loop (global level):**
SONA's three learning loops map to this architecture:
- **Instant loop** — runs in each user's local MemoryAgent (sub-ms)
- **Background loop** — runs in each user's local MemoryAgent (periodic consolidation)
- **Coordination loop** — runs in the global store, aggregates cross-user patterns

When many users' trajectories indicate that a particular published document leads
to successful outcomes, the coordination loop's aggregated SONA weights boost that
document's retrieval score for everyone. Collective intelligence emergence.

### 4.6 Privacy Boundary

Hard rule: **Local memory never leaves the user's VM without explicit publish action.**

- Session history, personal strategies, private patterns: stay local
- Published documents, agents, apps: enter global store when user clicks publish
- Aggregated patterns: anonymized trajectory scores only (no raw content)
- Users can unpublish at any time (removes from global index)

---

## 5. SONA Learning: How the System Gets Smarter

### 5.1 What SONA Actually Does

SONA does **not** train a language model. It adjusts retrieval embeddings.

The mechanism:
1. **MicroLoRA** (rank-2, ~2KB): Tiny weight matrices that warp the embedding space.
   When a query comes in, the embedding is transformed by LoRA before HNSW search.
   This biases search results toward patterns that historically led to good outcomes.

2. **EWC++** (Elastic Weight Consolidation): Prevents catastrophic forgetting.
   When new patterns are learned, EWC++ ensures old successful patterns aren't
   overwritten. The system remembers what worked last month even as it learns
   new patterns this week.

3. **ReasoningBank**: K-means++ cluster lookup over trajectory histories. Stores
   compressed representations of past reasoning chains. When a similar situation
   arises, the ReasoningBank provides "here's how we reasoned about this before."

### 5.2 Trajectory Mapping to Conductor Runs

Each conductor run is a SONA trajectory:

```
conductor.run.started        -> BeginTrajectory { id, query_embedding }
conductor.assignment.*       -> AddTrajectoryStep { action, result_embedding }
worker.report.received       -> AddTrajectoryStep { action, result_embedding }
conductor.run.completed      -> EndTrajectory { quality_score: 0.0-1.0 }
conductor.run.failed         -> EndTrajectory { quality_score: 0.0 }
```

Quality score derivation (heuristic, refined over time):
- Run completed + user didn't retry = 0.8 baseline
- Run completed + user explicitly approved = 1.0
- Run completed + user retried with different phrasing = 0.4
- Run failed = 0.0
- Run completed + subsequent related runs succeed = 0.9 (retroactive boost)

### 5.3 What "Getting Smarter" Looks Like

**Week 1:** User asks conductor to "set up a new Rust project with tests."
Conductor plans: create project, add deps, write tests. Worker executes. Score: 0.8.

**Week 2:** User asks "scaffold a new service module with integration tests."
Memory retrieves: Week 1 episode (similar intent). SONA score is boosted because
the Week 1 trajectory scored well. Conductor's planning context includes: "Previously,
creating project structure + adding deps + writing tests in that order worked well (0.8)."
Conductor makes a better plan faster.

**Week 3:** User asks "add a new actor with tests to the sandbox."
Memory retrieves: both Week 1 and Week 2 episodes. SONA has learned that the pattern
{scaffold structure -> add dependencies -> write tests} leads to good outcomes in this
user's workflow. The conductor's context is richer and more relevant.

**This is episodic memory with reinforcement.** The system doesn't just remember —
it remembers what worked.

---

## 6. Embedding Pipeline

### 6.1 MiniLM-L6-v2 via ONNX Runtime

```
Text input
  -> Tokenize (WordPiece, vocab bundled with model)
  -> Run through MiniLM-L6-v2 ONNX model (~22 MB)
  -> Mean pooling over token embeddings
  -> L2 normalize
  -> Output: Vec<f32> of length 384
```

Performance: ~1ms per embedding on CPU. No GPU needed. No API call.

The `ort` crate provides the ONNX Runtime binding. The model file
(`all-MiniLM-L6-v2.onnx`) is bundled with the binary or downloaded on first run.

### 6.2 Why Not API Embeddings

- Latency: API embeddings add 50-200ms per call. Local is ~1ms.
- Cost: At ingestion rates of ~100 events/minute during active use, API costs add up.
- Privacy: Embeddings of user sessions never leave the VM.
- Availability: No network dependency for the memory system.

### 6.3 Why Not ruvllm

`ruvllm` is a full generative LLM inference engine (84K LoC, Candle framework,
Metal/CUDA shaders). We need a 22MB encoder model for 384-dim embeddings.
Using ruvllm for this is like using a crane to pick up a pencil.

---

## 7. Implementation Phases (Serialized, Verifiable Chunks)

Each chunk has a gate — a concrete test that proves the chunk works before
moving on. No chunk depends on decisions we haven't made yet. Later chunks
can be reordered or dropped without invalidating earlier work.

**How much do we decide now?** Only what's needed for the current chunk.
The architecture supports all the advanced features (hyperbolic, GNN, Cypher,
global store) but nothing forces us to commit to their exact integration
until we reach that chunk. Each chunk is a standalone improvement.

### Chunk 1a: Actor Skeleton + Embedding (2-3 days)

- Create `sandbox/src/actors/memory/mod.rs` — MemoryAgent ractor actor
- Create `sandbox/src/actors/memory/protocol.rs` — MemoryAgentMsg types
- Create `sandbox/src/actors/memory/embedder.rs` — MiniLM via ort
- Add to supervision tree in `supervisor/session.rs`
- Add `rvf-runtime`, `ort` to `sandbox/Cargo.toml`
- MemoryAgent opens/creates a `.rvf` file per user

**Gate:** MemoryAgent actor starts, receives a text string via message,
embeds it with MiniLM, stores the vector in the `.rvf` file, and can
retrieve it by KNN query. Unit test proves round-trip embed→store→retrieve.

### Chunk 1b: Event Ingestion (2-3 days)

- Create `sandbox/src/actors/memory/relay.rs` — MemoryEventRelay
- Subscribe to EventBus topics (~15 event types)
- Map events to MemoryRecord types (kind, source, metadata)
- MemoryAgent receives mapped events, embeds, stores

**Gate:** Run a conductor session. Verify that events flow through
EventBus → MemoryEventRelay → MemoryAgent → `.rvf` file. Assert
that user messages, conductor plans, and worker results all appear
as stored vectors with correct metadata.

### Chunk 1c: Retrieval Integration (2-3 days)

- Create `sandbox/src/actors/memory/provider.rs` — MemoryProvider trait
- Inject into ConductorArguments and model_gateway.rs
- Inject into terminal/researcher `get_system_context()`
- Wire through `app_state.rs`

**Gate:** Start a session, do some work, start a new session. Verify
that the conductor's system context contains relevant memories from
the previous session. The conductor should "know" what happened before.

### Chunk 1d: Hypergraph Episodes (2-3 days)

- Map conductor run lifecycle events to hyperedge records
- Serialize hyperedges to RVF Journal entries (entry_type 0x02)
- Rebuild HypergraphIndex from Journal on startup
- Add k_hop_neighbors expansion to retrieval pipeline

**Gate:** Complete a conductor run. Verify that a single hyperedge
connects the prompt, plan, assignments, and results. Query any
fragment and verify that `k_hop_neighbors` returns the full episode.

### Chunk 1e: Causal Memory + SONA (2-3 days)

- Add `ruvector-sona`, `ruvector-core` to Cargo.toml
- Track strategy→outcome cause-effect edges
- Implement quality score derivation heuristic
- Enable SONA MicroLoRA on trajectory completion
- Add utility-scored retrieval: `U = α*similarity + β*causal + γ*recency`

**Gate:** Complete 5+ conductor runs with mixed success/failure. Verify
that retrieval for similar future queries ranks successful strategies
higher than failed ones. The ranking should change after SONA learning.

### Chunk 1f: Runtime Control Surface (2-3 days)

- See Section 13 for the full control surface spec
- Add MemoryConfig to protocol (toggles, parameters)
- Expose via API endpoints for runtime adjustment
- Add memory inspection endpoint (what does the agent remember?)

**Gate:** Toggle memory off via API, verify conductor no longer receives
memory context. Toggle back on. Adjust top_k, verify result count changes.
Query the inspection endpoint, see stored memories with scores.

---

*Chunks 1a through 1f are the foundation. Each is independently verifiable.*
*Total: ~2-3 weeks for a working episodic memory with episode grouping,*
*causal reasoning, adaptive learning, and runtime control.*

---

### Chunk 2a: Hyperbolic HNSW (1 week)

- Vendor `ruvector-hyperbolic-hnsw` into workspace
- Implement Euclidean→Poincaré projection step
- Build DualSpaceIndex alongside RVF's Euclidean HNSW
- Assign hierarchy depth to memory types

**Gate:** Store memories at different hierarchy depths. Query for strategies
specifically. Verify that hyperbolic retrieval correctly separates goals
from strategies from findings, where flat Euclidean HNSW mixes them.

### Chunk 2b: Mixed Curvature Attention (3-5 days)

- Integrate MixedCurvatureAttention from `ruvector-attention`
- Combined content (Euclidean) + hierarchy (hyperbolic) scoring

**Gate:** Compare retrieval quality with and without mixed curvature on a
test corpus with clear hierarchy. Measure ranking improvement.

### Chunk 3a-3c: GNN, Reflexion, Skills (1-2 weeks each)

- Each is an independent improvement, can be reordered
- GNN: neural search refinement on ambiguous queries
- Reflexion: self-critique storage and retrieval
- Skills: auto-consolidation of repeated successful patterns

**Gate per chunk:** Measurable retrieval quality improvement on test corpus.

### Chunk 4: Global Knowledge Store (after hypervisor + auth)

- Platform service, publish/subscribe, cross-user SONA
- Not blocked by any earlier chunk decisions

---

## 8. Files to Create

| File | Purpose |
|---|---|
| `sandbox/src/actors/memory/mod.rs` | MemoryAgent ractor actor — embed, store, recall, learn |
| `sandbox/src/actors/memory/protocol.rs` | MemoryAgentMsg, MemoryRecord, RetrievedMemory, MemoryFilter types |
| `sandbox/src/actors/memory/embedder.rs` | MiniLMEmbedder — `ort` crate wrapping all-MiniLM-L6-v2.onnx |
| `sandbox/src/actors/memory/relay.rs` | MemoryEventRelay — EventBus subscriber, event-to-memory mapper |
| `sandbox/src/actors/memory/provider.rs` | MemoryProvider trait + ActorMemoryProvider (RPC to MemoryAgent) |

## 9. Files to Modify

| File | Change |
|---|---|
| `sandbox/Cargo.toml` | Add `rvf-runtime`, `ruvector-sona`, `ort` dependencies |
| `supervisor/session.rs` | Spawn MemorySupervisor in SessionSupervisor::pre_start |
| `supervisor/mod.rs` | Add GetOrCreateMemory to ApplicationSupervisorMsg |
| `actors/conductor/actor.rs` | Add memory_provider to ConductorArguments + ConductorState |
| `actors/conductor/model_gateway.rs` | Inject recalled memories into system context |
| `actors/terminal.rs` | Add memory retrieval in get_system_context |
| `actors/researcher/adapter.rs` | Add memory retrieval in get_system_context |
| `actors/writer/mod.rs` | Add memory retrieval at synthesis/planning/delegation points |
| `app_state.rs` | Wire MemoryAgent ref through ensure_memory() pattern |

---

## 10. Open Questions

1. **Embedding model upgrade path:** MiniLM-L6-v2 is good enough to start (384-dim,
   ~1ms). If we need better quality later, we can swap to a larger ONNX model
   (e.g., bge-small-en-v1.5 at 384-dim or bge-base-en-v1.5 at 768-dim) without
   changing the architecture. The embedder is behind a trait.

2. **Quality score ground truth:** The initial heuristic (did the user retry?) is
   rough. We may want explicit feedback mechanisms ("was this helpful?") or implicit
   signals (time-to-next-prompt, session continuation patterns).

3. **Global store deletion:** When a user unpublishes, we need to remove from
   the global index. RVF supports soft-delete with tombstone journal segments
   and background compaction — cleaner than HNSW-only approaches. But we should
   verify compaction behavior at scale.

4. **Cross-user privacy in SONA coordination:** The global SONA coordination loop
   aggregates trajectory scores but must never leak raw content across users.
   Anonymized quality metrics only.

5. **RVF ecosystem maturity:** Single-maintainer, v0.1.0, very new (Feb 2026).
   We should pin versions, wrap behind traits, and have a fallback plan. The `.rvf`
   file format is simple enough (append-only segments with documented headers) that
   we could write our own reader/writer if the crate is abandoned. The trait boundary
   (`VectorStore`, `LearningEngine`) makes the storage layer swappable.

6. **COW branching for publish workflow:** When a user publishes local patterns to
   the global store, RVF's COW branching could enable zero-copy forking — the
   published branch shares storage with the local file until divergence. Need to
   verify this works across VM boundaries (local .rvf in Firecracker, global .rvf
   on platform service).

---

## 11. Beyond Flat Vector Search: Graph, Causal Memory, and What's Automatic

Plain vector similarity (`query(embedding, k)`) is the baseline. The ruvector
ecosystem provides significantly more structure that maps well to episodic memory.
This section explains what exists, what happens automatically, what we build, and
why it matters for ChoirOS.

### 11.1 The HNSW Graph IS a Graph

HNSW (Hierarchical Navigable Small World) is not just a search algorithm — it's a
multi-layer graph where nodes are vectors and edges connect semantically similar items.

In `rvf-index`, this graph is fully exposed:

```rust
// Each HNSW layer is an adjacency list
pub struct HnswLayer {
    pub adjacency: BTreeMap<u64, Vec<u64>>,  // node_id -> neighbor_ids
}

pub struct HnswGraph {
    pub layers: Vec<HnswLayer>,  // layer 0 = all nodes, higher = fewer nodes
    pub entry_point: u64,
    pub max_layer: u32,
    pub m: usize,                // max edges per node (default 16)
    // ...
}
```

**What this means for episodic memory:**

The HNSW graph naturally clusters similar memories together. If you stored 1000
episodes, the graph's adjacency structure implicitly encodes "these episodes are
related" — not because we labeled them, but because their embeddings are close.

**Automatic:** When you `ingest_batch()`, the HNSW graph is built/updated.
Edges are created between semantically similar vectors. No extra work needed.

**Manual (we build):** Graph traversal beyond KNN search. The adjacency data is
public, so we can walk it — but there's no `find_related_chain()` API. We compose
this ourselves.

### 11.2 What Happens Automatically (Zero Extra Work)

These capabilities work out of the box with `rvf-runtime` and/or `ruvector-core`:

**1. KNN vector search with metadata filtering**

```rust
// "Find 5 most similar episodes that are strategies from this week"
let results = store.query(&query_embedding, 5, &QueryOptions {
    filter: Some(FilterExpr::And(vec![
        FilterExpr::Eq(FIELD_KIND, FilterValue::String("strategy")),
        FilterExpr::Ge(FIELD_TIMESTAMP, FilterValue::Int(week_start_ts)),
    ])),
    ..Default::default()
});
```

The filter is evaluated during the HNSW search — not as a post-filter.
`ruvector-core` even auto-selects pre-filter vs post-filter strategy based on
estimated selectivity.

**2. Progressive HNSW loading (RVF)**

When MemoryAgent starts, it opens the `.rvf` file. Layer A loads in <5ms:
- Cluster centroids (IVF-style partitions)
- Top HNSW layers
- Entry points

Queries work immediately at ~70% recall. Layers B and C load in the background,
improving to 95%+ recall. The agent doesn't block on full index load.

**3. Hybrid vector + keyword search (ruvector-core)**

```rust
// Combine semantic similarity with BM25 keyword matching
let hybrid = HybridSearch::new(vector_index, bm25_index);
let results = hybrid.search(&query_embedding, "auth mutex race", k, alpha);
```

Useful when episodic memory needs both semantic similarity AND keyword presence.
Example: "Find episodes about authentication" should match episodes containing
"auth", "login", "session" even if the semantic embedding is slightly off.

**4. MMR diversity-aware retrieval (ruvector-core)**

```rust
// Return k results that are relevant BUT diverse (not all about the same thing)
let mmr = MMRSearch::new(lambda: 0.7);  // 0.7 = bias toward relevance
let results = mmr.search(&query_embedding, k);
```

Important for conductor context: you want 5 relevant memories, not 5 paraphrases
of the same episode.

### 11.3 The Hypergraph and Causal Memory (ruvector-core)

This is the most interesting capability for episodic memory. `ruvector-core` includes
a `HypergraphIndex` and `CausalMemory` module.

**Hypergraph:** A graph where edges can connect more than 2 nodes simultaneously.
A regular graph edge is A--B. A hyperedge can be {A, B, C, D} — "these things are
all related as a group."

This maps naturally to conductor runs:

```
Hyperedge: "auth-fix-run-2026-02-15"
  Nodes: [user_prompt, conductor_plan, terminal_assignment, 
          researcher_assignment, worker_result_1, worker_result_2, 
          final_outcome]
  
  = "all of these things happened together as one episode"
```

**Concrete APIs:**

```rust
let mut hg = HypergraphIndex::new(384);  // 384-dim embeddings

// Add episode nodes
hg.add_entity(prompt_id, &prompt_embedding)?;
hg.add_entity(plan_id, &plan_embedding)?;
hg.add_entity(result_id, &result_embedding)?;

// Connect them as a hyperedge (one episode = one run)
hg.add_hyperedge(Hyperedge {
    id: run_id,
    nodes: vec![prompt_id, plan_id, result_id],
    weight: quality_score,
    metadata: json!({ "run_id": run_id, "timestamp": ts }),
})?;

// Later: "find episodes related to this query"
let related_edges = hg.search_hyperedges(&query_embedding, k)?;

// "what else was part of the same episode as this result?"
let neighbors = hg.k_hop_neighbors(result_id, 1)?;
// Returns: {prompt_id, plan_id} — the other nodes in the same hyperedge

// 2-hop: "what episodes are connected to episodes connected to this?"
let extended = hg.k_hop_neighbors(result_id, 2)?;
// Returns: nodes from related hyperedges — transitive episode discovery
```

**Temporal hyperedges:**

```rust
hg.add_temporal_hyperedge(TemporalHyperedge {
    id: "morning-session-feb-15",
    nodes: vec![...],
    time_bucket: 20260215_08,  // hour-level bucket
    weight: 0.9,
    metadata: json!({...}),
})?;

// "What happened between 8am and noon on Feb 15?"
let episodes = hg.query_temporal_range(20260215_08, 20260215_12)?;
```

**Causal memory (utility-scored retrieval):**

```rust
let mut causal = CausalMemory::new(384);

// Record: "using terminal for auth fixes causes good outcomes"
causal.add_causal_edge(
    cause: terminal_strategy_id,
    effect: success_outcome_id,
    context_nodes: vec![auth_domain_id],
    description: "terminal strategy succeeded for auth domain",
    embedding: &strategy_embedding,
    latency_ms: 5000,  // how long the run took
)?;

// Later: "given this query, what strategy should I use?"
// Utility = alpha*similarity + beta*causal_uplift - gamma*latency
let ranked = causal.query_with_utility(
    &query_embedding,
    action_id: terminal_strategy_id,
    k: 5,
)?;
```

The `causal_uplift` term means strategies that historically CAUSED good outcomes
get boosted. This is different from SONA (which adjusts embeddings) — causal
memory explicitly models cause-effect relationships and uses them in ranking.

### 11.4 AgenticDB: Agent-Native Memory Tables

`ruvector-core::agentic` provides 5 pre-built tables designed specifically for
agent memory patterns:

**1. Reflexion Episodes (self-critique memory)**

```rust
db.store_episode(
    task: "fix auth race condition",
    actions: vec!["grep for mutex", "read session_manager.rs", "add lock"],
    observations: vec!["found shared state", "no synchronization", "test passes"],
    critique: "Should have checked for other shared state in the module",
)?;

// Later: "have I worked on something like this before?"
let past = db.retrieve_similar_episodes(&query_embedding, k)?;
// Returns the episode with critique — agent learns from past mistakes
```

**2. Skills Library (consolidated action patterns)**

```rust
db.create_skill(
    name: "rust-actor-scaffold",
    description: "Scaffold a new ractor actor with message types and supervision",
    params: vec!["actor_name", "message_types"],
    examples: vec![...],
)?;

// Auto-detect repeated patterns and consolidate into skills
db.auto_consolidate(&recent_action_sequences, similarity_threshold: 0.85)?;

// "What skills do I have for this kind of task?"
let skills = db.search_skills(&query_embedding, k)?;
```

**3. Session State Index (turn-scoped)**

```rust
// Scoped to session with TTL
db.session_state.store(session_id, turn_number, &state_embedding, metadata)?;
db.session_state.query_session(session_id, &query_embedding, k)?;
```

**4. Witness Log (audit trail)**

Hash-chained entries. Each entry includes the hash of the previous entry.
Tamper-evident: if any entry is modified, the chain breaks.

```rust
db.witness_log.append(event_data)?;
db.witness_log.verify_chain()?;  // returns true if unbroken
```

**CRITICAL CAVEAT:** AgenticDB defaults use **placeholder hash-based embeddings**
(character-level hashing, NOT semantic). "dog" and "cat" are NOT similar; "dog"
and "god" ARE similar. You MUST provide a real `EmbeddingProvider` via
`with_embedding_provider()`. Our MiniLM pipeline satisfies this.

### 11.5 Cypher Query Language (ruvector-graph)

ruvector includes a **full Cypher query language implementation** in the
`ruvector-graph` crate. This is not a stub — it's a complete recursive-descent
parser with cost-based optimization and a pipeline query executor.

**Components:**

| Module | Purpose |
|---|---|
| `cypher/lexer.rs` | Tokenizer for Cypher syntax |
| `cypher/parser.rs` | Recursive-descent parser producing AST |
| `cypher/ast.rs` | Full abstract syntax tree types |
| `cypher/optimizer.rs` | Cost-based query optimization (predicate pushdown, join reordering, constant folding) |
| `cypher/semantic.rs` | Type checking and semantic analysis |
| `executor/operators.rs` | NodeScan, EdgeScan, HyperedgeScan, Filter, Join, Aggregate, Sort, Limit, Project |
| `executor/pipeline.rs` | Iterator-model pipeline execution with RowBatch |
| `executor/parallel.rs` | Rayon-based parallel execution |
| `executor/cache.rs` | LRU query result cache with TTL |

**Supported Cypher features:**

```cypher
-- Pattern matching with labels
MATCH (n:Episode)-[r:CAUSED]->(b:Outcome)

-- Filtering
WHERE n.quality_score > 0.8 AND n.kind = 'strategy'

-- Variable-length paths (multi-hop)
MATCH p = (a)-[*1..5]->(b)

-- Projections and aggregations
RETURN n.text AS episode, COUNT(n), AVG(n.quality_score)
ORDER BY n.timestamp DESC
LIMIT 10

-- Mutations
CREATE (e:Episode {text: "...", kind: "strategy"})
MERGE (a)-[:RELATES_TO]->(b)

-- Undirected relationships
MATCH (a)-[r]-(b)
```

**Vector-extended Cypher** (`hybrid/cypher_extensions.rs`):

```cypher
-- Semantic similarity in Cypher
MATCH (n:Episode)
WHERE n.embedding SIMILAR TO $query_vector
RETURN n, semanticScore(n)

-- Semantic path ranking
MATCH p = (a)-[:RELATES_TO*1..3]->(b)
RETURN p, avg_embedding(p)  -- centroid of embeddings along path
```

**CLI access:**

```bash
ruvector graph query --cypher "MATCH (n:Episode) WHERE n.kind = 'strategy' RETURN n"
ruvector graph shell  # interactive Cypher REPL
```

**How this helps ChoirOS episodic memory:**

Instead of composing programmatic Rust queries for every retrieval pattern,
we can express complex episode queries in Cypher:

```cypher
-- "Find strategies that led to successful outcomes in auth-related episodes"
MATCH (prompt:Episode {kind: 'episode'})-[:PART_OF]->(run:Run)
      -[:USED_STRATEGY]->(s:Episode {kind: 'strategy'})
      -[:CAUSED]->(outcome:Episode {kind: 'finding'})
WHERE prompt.text SIMILAR TO $query
  AND outcome.quality_score > 0.8
RETURN s.text, outcome.quality_score
ORDER BY outcome.quality_score DESC
LIMIT 5
```

This is dramatically more expressive than composing k-hop + filter + sort manually.

### 11.6 GNN: Graph Neural Networks (ruvector-gnn)

ruvector has a **real, production-grade GNN subsystem**. Pure Rust, no
PyTorch/Candle/ONNX dependency — uses `ndarray` for tensor operations.

**Implemented architectures:**

| Architecture | What it does | Implementation |
|---|---|---|
| **GCN** (Graph Convolutional Networks) | Kipf & Welling 2016. Aggregates neighbor features through spectral convolution with degree normalization. | `ruvector-postgres/src/gnn/gcn.rs` |
| **GraphSAGE** | Hamilton et al. 2017. Samples neighbors and aggregates with Mean/MaxPool/LSTM. Scales to large graphs. | `ruvector-postgres/src/gnn/graphsage.rs` |
| **GAT** (Graph Attention Networks) | Multi-head scaled dot-product attention over neighbors. Learns which neighbors matter most. | `ruvector-gnn/src/layer.rs` |
| **RuvectorLayer** (custom) | Combines message passing + multi-head attention + edge-weight aggregation + GRU state updates + layer norm + dropout. The flagship layer. | `ruvector-gnn/src/layer.rs` |

**Core GNN infrastructure:**

```rust
// Message passing framework (the core GNN operation)
trait MessagePassing {
    fn message(&self, source: &[f32], target: &[f32]) -> Vec<f32>;
    fn aggregate(&self, messages: &[Vec<f32>]) -> Vec<f32>;
    fn update(&self, node: &[f32], aggregated: &[f32]) -> Vec<f32>;
}

// propagate() runs message passing across all nodes (parallelized with rayon)
fn propagate(graph: &AdjacencyList, features: &Matrix, layer: &impl MessagePassing);
fn propagate_weighted(graph: &AdjacencyList, features: &Matrix, weights: &[f32], layer: &impl MessagePassing);
```

**Training infrastructure:**
- Optimizers: SGD with momentum, Adam (with bias correction)
- Loss functions: MSE, Cross Entropy, Binary Cross Entropy, InfoNCE contrastive, local contrastive (graph-structure-aware)
- EWC (Elastic Weight Consolidation) — prevents catastrophic forgetting
- Cosine annealing + warmup + plateau LR scheduling
- Experience replay with reservoir sampling

**GNN-enhanced search modes** (`ruvector-gnn/src/query.rs`):

```rust
// Standard vector search
RuvectorQuery::vector_search(embedding, k)

// GNN-enhanced: refine results using graph neighborhood
RuvectorQuery::neural_search(embedding, k, gnn_depth: 2)

// Extract k-hop subgraph around results
RuvectorQuery::subgraph_search(embedding, k)

// Soft attention with temperature-controlled ranking
RuvectorQuery::differentiable_search(embedding, k, temperature: 0.1)
```

The key operation is `hierarchical_forward()` — it processes a query through
GNN layers over the HNSW hierarchy, using the graph structure to refine
which vectors are considered relevant. This means search results are influenced
not just by embedding distance, but by the topology of the memory graph.

**How GNNs help ChoirOS episodic memory:**

1. **Neural search refinement:** When you query "fix auth bug", basic HNSW returns
   the 5 closest vectors. GNN neural search additionally considers: what are the
   neighbors of those 5 vectors in the graph? If a result has many neighbors that
   are also relevant, its score gets boosted. If a result is isolated (no relevant
   neighbors), it gets demoted. The graph structure adds signal.

2. **Learned aggregation:** GAT attention learns WHICH neighbor relationships matter
   most. Over time, the GNN can learn that "strategy -> outcome" edges are more
   informative than "prompt -> strategy" edges for a given query type.

3. **Subgraph extraction:** `subgraph_search` returns not just matching nodes but
   their entire k-hop neighborhood. Feed this to the conductor and it gets a
   complete episode context, not just isolated fragments.

4. **Continual learning (EWC):** The GNN weights can be trained incrementally
   without forgetting past patterns. This stacks with SONA — SONA adjusts
   embeddings, GNN adjusts the graph-based refinement.

### 11.7 Hyperbolic HNSW (ruvector-hyperbolic-hnsw)

**Status: Fully implemented, unpublished.** The crate exists at
`crates/ruvector-hyperbolic-hnsw/` in the ruvector monorepo (~1500+ LoC,
7 source files) but is excluded from the workspace build and not yet on crates.io.
It builds independently with its own Cargo.lock.

**This is not a stub.** It has:

- **Full Poincaré ball model** (`poincare.rs`): `poincare_distance`, `mobius_add`,
  `exp_map`, `log_map`, `frechet_mean`, `parallel_transport`, `project_to_ball`,
  SIMD-optimized `fused_norms` (4-wide unrolling), batch distance

- **Complete HNSW in hyperbolic space** (`hnsw.rs`): `HyperbolicHnsw` struct with
  multi-layer insert/search, greedy traversal, ef-bounded candidates, connection
  pruning, tangent-space pruning fallback. Plus `DualSpaceIndex` — synchronized
  Euclidean + hyperbolic HNSW with reciprocal rank fusion (fast Euclidean prune,
  accurate Poincaré rank)

- **Tangent space speed trick** (`tangent.rs`): `TangentCache` precomputes log-map
  coordinates at the Frechet centroid. `TangentPruner` does two-phase search: fast
  Euclidean distance in tangent space to prune candidates, then exact Poincaré
  distance only on survivors. `tangent_micro_update` for incremental writes.

- **Sharded with per-shard curvature** (`shard.rs`): `ShardedHyperbolicHnsw` allows
  different hierarchy branches to have different optimal curvatures.
  `CurvatureRegistry` with canary testing (A/B test curvature values on live traffic)
  and hot reload (reproject vectors + rebuild tangent cache on curvature change).
  `HierarchyMetrics` computes Spearman radius-depth correlation and distance
  distortion — auto-validates whether data actually benefits from hyperbolic geometry.

- **Comprehensive tests** (`tests/math_tests.rs`): Möbius identity/inverse/
  gyrocommutativity, exp-log inverse, distance symmetry/identity/triangle inequality,
  numerical stability at boundary, Frechet mean convergence, tangent ordering,
  dual-space fusion, edge cases

- **Criterion benchmarks** (`benches/hyperbolic_bench.rs`): All operations benchmarked
  at various sizes

**Distance metrics:** `Poincare`, `Euclidean`, `Cosine`, `Hybrid`

**Supporting hyperbolic code elsewhere in the monorepo:**
- `ruvector-postgres/src/hyperbolic/` — PoincaréBall + LorentzModel with simsimd SIMD
- `ruvector-attention/src/hyperbolic/` — `HyperbolicAttention`, `MixedCurvatureAttention`,
  `LorentzCascadeAttention` with Busemann scoring and horosphere weights
- `ruvector-dag/src/attention/hierarchical_lorentz.rs` — DAG-specific Lorentz attention

**Why hyperbolic HNSW matters for episodic memory:**

Hyperbolic space has a fundamental property: it represents hierarchy with
exponentially less distortion than Euclidean space. The Poincaré ball model
places general concepts near the center and specific concepts near the boundary.
Tree structures embed perfectly.

ChoirOS episodic memory is inherently hierarchical:

```
User goal (general, near center of Poincaré ball)
  └── Conductor run (more specific, further from center)
        ├── Strategy chosen
        ├── Worker assignment 1
        │     └── Sub-findings (near boundary)
        └── Worker assignment 2
              └── Sub-findings (near boundary)
```

In Euclidean 384-dim space, this hierarchy is distorted — the "distance" between
a general goal and a specific sub-finding is poorly calibrated. In hyperbolic space,
the hierarchy is preserved naturally by the metric.

**The DualSpaceIndex is particularly interesting for us:** it keeps both Euclidean and
hyperbolic HNSW graphs synchronized. Queries use Euclidean for fast candidate pruning
(cheap L2 distance), then Poincaré for accurate ranking (captures hierarchy). Best of
both worlds — speed of Euclidean, structure of hyperbolic.

**Open research questions** (see `docs/research/2026-02-16-hyperbolic-hnsw-research-prompt.md`):
- Embedding pipeline: MiniLM produces Euclidean vectors. How to project into Poincaré ball?
- Curvature: what value for ~5-level episode hierarchy?
- SONA compatibility: can LoRA adjustments work in tangent space?
- Integration with rvf-runtime persistence
- Vendoring strategy (crate is unpublished)

### 11.8 What We Build vs What's Given (Corrected)

| Need | Provided by ruvector | What we build |
|---|---|---|
| "Find similar past episodes" | HNSW KNN search | Wire to MemoryAgent protocol |
| "Filter by time/type/session" | FilterExpression / FilterExpr | Define our field schema |
| "Diverse results, not duplicates" | MMR search | Configure lambda parameter |
| "Keyword + semantic hybrid" | HybridSearch + BM25 | Build BM25 index alongside HNSW |
| "Group episode components" | Hypergraph hyperedges | Map conductor runs to hyperedges |
| "What else was in this episode?" | k_hop_neighbors(id, 1) | Call after initial retrieval |
| "What happened this morning?" | query_temporal_range() | Map timestamps to time buckets |
| "What strategy worked here?" | CausalMemory.query_with_utility() | Record cause-effect edges from outcomes |
| "Learn from past mistakes" | Reflexion episodes | Feed critiques into store_episode() |
| "Detect repeated patterns" | auto_consolidate() in Skills Library | Feed successful action sequences |
| "Audit trail for memory" | WitnessLog hash chains | Append memory operations |
| "Progressive fast startup" | Layer A/B/C in RVF | Automatic with rvf-runtime |
| "Complex graph queries" | **Cypher query language** | Write Cypher queries for episode patterns |
| "Graph-refined search" | **GNN neural_search / differentiable_search** | Configure GNN depth and train on episode graph |
| "Subgraph context extraction" | **GNN subgraph_search** | Wire to conductor context hydration |
| "Hierarchical episode embedding" | **Hyperbolic HNSW** (fully implemented, unpublished) | Vendor crate, design Euclidean-to-Poincaré projection pipeline |
| "Compose all of the above" | Cypher + GNN query modes help, but orchestration is ours | MemoryAgent orchestrates the full pipeline |

### 11.9 How This Maps to ChoirOS Episodic Memory (Revised)

**Phase 1 (baseline):** RVF storage + HNSW KNN + metadata filtering.
Covers core episodic retrieval. Filter by kind, source, time, session.

**Phase 2 (structured episodes):** Hypergraph edges for conductor runs +
Cypher queries for complex episode patterns. The graph becomes queryable
with a real query language rather than programmatic composition only.

**Phase 3 (intelligent retrieval):** GNN neural search refinement + SONA
adaptive learning + CausalMemory utility scoring. Three complementary
mechanisms for improving retrieval quality:
- SONA adjusts embeddings based on trajectory outcomes
- GNN refines search using graph topology (neighbor relevance)
- CausalMemory boosts strategies with proven causal success

**Phase 4 (deep intelligence):** Reflexion episodes + skills consolidation +
hyperbolic embeddings for hierarchical structure. The system doesn't just
remember episodes — it extracts patterns, learns from mistakes, and represents
knowledge hierarchy naturally.

**Phase 5 (global knowledge):** The Cypher query language becomes essential
at the global store level, where cross-user published content forms a rich
graph that needs expressive querying beyond flat vector search.

### 11.10 The Full Crate Map (Corrected)

The ruvector monorepo is much larger than initially surveyed. Here are all
the crates relevant to ChoirOS memory:

| Crate | What it provides | When we use it |
|---|---|---|
| `rvf-runtime` | .rvf file format, vector store, progressive HNSW | Phase 1 (storage) |
| `rvf-index` | Pure-Rust HNSW with Layer A/B/C progressive loading | Phase 1 (automatic via rvf-runtime) |
| `rvf-types` | Format spec types | Phase 1 (transitive dep) |
| `ruvector-sona` | MicroLoRA + EWC++ + ReasoningBank adaptive learning | Phase 2 (learning) |
| `ruvector-core` | HypergraphIndex, CausalMemory, AgenticDB, HybridSearch, MMR | Phase 2-3 (graph + advanced search) |
| `ruvector-graph` | Cypher parser + optimizer + pipeline executor | Phase 2-3 (query language) |
| `ruvector-gnn` | GCN, GraphSAGE, GAT, neural/differentiable search | Phase 3 (graph-refined search) |
| `ruvector-filter` | Rich filter expression DSL (geo, text match, null, exists) | Phase 2 (advanced filtering) |
| `ruvector-delta-graph` | Delta-aware traversal, shortest path, connected components | Phase 3 (graph analysis) |
| `ruvector-dag` | DAG traversal (topological, DFS, BFS), query plan DAGs | Phase 3 (dependency ordering) |
| `ruvector-hyperbolic-hnsw` | Poincaré ball HNSW, DualSpaceIndex, tangent pruning, per-shard curvature | Phase 2-3 (hierarchy-aware retrieval) |
| `ort` | ONNX Runtime for MiniLM-L6-v2 embeddings | Phase 1 (embeddings) |

**Not used:**
| Crate | Why |
|---|---|
| `ruvllm` | Local LLM inference — we use API models via BAML |
| `ruvector-gnn-node` | Node.js bindings — we're pure Rust |
| `ruvector-gnn-wasm` | WASM bindings — not needed server-side |
| `ruvector-postgres` | PostgreSQL extension — we don't use Postgres for vectors (though its GCN/GraphSAGE/hyperbolic code is reference material) |
| `ruvector-hyperbolic-hnsw-wasm` | WASM bindings for hyperbolic HNSW — not needed server-side |

### 11.11 Correcting Previous Errors

The earlier version of this section incorrectly stated:

1. ~~"No GNN exists"~~ — **Wrong.** `ruvector-gnn` has full GCN, GraphSAGE, GAT,
   and a custom RuvectorLayer with message passing, multi-head attention, GRU
   updates, and continual learning (EWC). Pure Rust, no ML framework dependency.

2. ~~"No graph query language"~~ — **Wrong.** `ruvector-graph` has a complete
   Cypher parser with cost-based optimization, a pipeline query executor, and
   vector-extended Cypher (`SIMILAR TO`, `semanticScore()`, `avg_embedding()`).

3. ~~"Hyperbolic HNSW is not implemented"~~ — **Wrong.** `ruvector-hyperbolic-hnsw`
   is a fully implemented crate (~1500+ LoC) with Poincaré ball model, tangent
   space pruning, DualSpaceIndex (Euclidean+hyperbolic fusion), per-shard
   curvature with canary testing, comprehensive math tests, and criterion
   benchmarks. It's excluded from the workspace build but builds independently.

The earlier research only examined crates published to crates.io and their
docs.rs pages. The monorepo contains many additional crates that are implemented
but not yet published.

---

## 12. Putting It All Together: The Unified Memory Architecture

The ruvector ecosystem gives us five powerful subsystems. Each is well-built
internally but they are mostly **not integrated with each other** in the upstream
codebase. The integration is ours to build. This section lays out how hypergraphs,
hyperbolic HNSW, GNNs, Cypher, and SONA compose into a unified episodic memory
system for ChoirOS.

### 12.1 The Integration Reality (What Connects to What Upstream)

In ruvector upstream, cross-feature integration is sparse:

```
Feature families (well-built internally, poorly bridged):

  Hypergraph (ruvector-core, ruvector-graph)
       │
       ✗  no bridge to GNN, hyperbolic, or SONA
       │
  Hyperbolic HNSW (ruvector-hyperbolic-hnsw)
       │
       ✓  MixedCurvatureAttention bridges Euclidean ↔ Hyperbolic
       │
  GNN (ruvector-gnn)
       │
       ✗  operates on pairwise HNSW edges only, not hyperedges
       │
  Cypher (ruvector-graph)
       │
       ~  HyperedgePattern in AST + semantic analysis, parser not yet wired
       │
  SONA (ruvector-sona)
       │
       ✗  standalone, no graph/hypergraph awareness
```

**The only deep cross-feature bridge that exists upstream**: `MixedCurvatureAttention`
in `ruvector-attention`, which splits embeddings into Euclidean + Hyperbolic
components, computes attention in both spaces, and combines via learned mixing
weight. There's also a 3-space variant (`MixedCurvatureFusedAttention`) adding
Spherical geometry with SIMD optimization.

**Everything else is our integration work.** This is actually good — we compose
the pieces to fit our episodic memory semantics rather than being constrained
by upstream assumptions.

### 12.2 The Three Layers of Memory

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 3: CAUSAL GRAPH (Hypergraph + CausalMemory)             │
│                                                                  │
│  "What caused what? What strategies worked?"                     │
│                                                                  │
│  Structure: Hyperedges connecting N entities per episode         │
│  Geometry: Flat (topology is the signal, not embedding distance) │
│  Query: Cypher patterns + utility-scored retrieval               │
│  Learning: Causal uplift tracking (success frequency)            │
└──────────────────────────┬──────────────────────────────────────┘
                           │ nodes reference vectors in Layer 2
                           │
┌──────────────────────────┴──────────────────────────────────────┐
│  LAYER 2: HIERARCHICAL INDEX (Hyperbolic HNSW)                  │
│                                                                  │
│  "Where does this memory sit in the hierarchy?"                  │
│                                                                  │
│  Structure: HNSW graph in Poincaré ball                          │
│  Geometry: Hyperbolic (hierarchy-preserving, depth-aware)        │
│  Query: DualSpaceIndex (Euclidean prune → Poincaré rank)        │
│  Learning: Per-shard curvature optimization                      │
└──────────────────────────┬──────────────────────────────────────┘
                           │ vectors persisted from Layer 1
                           │
┌──────────────────────────┴──────────────────────────────────────┐
│  LAYER 1: DURABLE STORAGE (RVF + SONA)                          │
│                                                                  │
│  "What happened? What's the raw content?"                        │
│                                                                  │
│  Structure: Append-only .rvf file with progressive HNSW          │
│  Geometry: Euclidean (MiniLM-L6-v2, 384-dim)                    │
│  Query: KNN + metadata filters                                   │
│  Learning: SONA MicroLoRA adjusts embeddings based on outcomes   │
└─────────────────────────────────────────────────────────────────┘
```

### 12.3 How a Query Flows Through All Three Layers

When a user says "fix the auth bug" and we need to retrieve relevant episodic memory:

```
Step 1: EMBED
  "fix the auth bug" → MiniLM → 384-dim Euclidean embedding
  → SONA MicroLoRA transform (bias toward successful patterns)
  → project into Poincaré ball (exp_map at origin)

Step 2: LAYER 1 — Fast recall (RVF, ~1-5ms)
  Query the .rvf file with progressive HNSW
  Returns: 20 candidate memories by raw embedding similarity
  Filter: kind=*, source=*, last 30 days

Step 3: LAYER 2 — Hierarchy-aware reranking (Hyperbolic HNSW, ~2-5ms)
  Feed 20 candidates into DualSpaceIndex
  Euclidean prune: eliminate distant candidates cheaply
  Poincaré rank: reorder by hyperbolic distance
  Effect: general strategies float up, specific sub-findings
  cluster with their parent episodes
  Returns: 10 candidates with hierarchy-aware scores

Step 4: LAYER 3 — Causal expansion (Hypergraph, ~1-3ms)
  For top 10 candidates, query the hypergraph:
    k_hop_neighbors(candidate_id, 1) → expand each to full episode
    query_with_utility(embedding, strategy_id, k) → rank strategies
      by U = 0.7*similarity + 0.2*causal_uplift - 0.1*latency
  Returns: 5 complete episodes with causal context

Step 5: FORMAT for conductor
  Each episode includes:
    - What the user asked (prompt node)
    - What strategy was chosen (plan node)
    - What the outcome was (result nodes)
    - Quality score and causal success rate
  Injected into conductor's system context as structured memory
```

**Total retrieval latency: ~5-15ms.** Well within the budget for a conductor
wake cycle that will spend 2-5 seconds on an LLM call anyway.

### 12.4 How Episodes Are Stored (Hypergraph Construction)

When a conductor run completes, the ingestion pipeline constructs a hyperedge:

```
Event: conductor.run.completed
  ├── prompt_id   (user's original message)
  ├── plan_id     (conductor's assignment strategy)
  ├── worker_ids  (terminal/researcher assignments)
  ├── result_ids  (worker findings/outputs)
  └── outcome_id  (success/failure + quality score)

→ MemoryAgent receives via EventRelay

→ Embed all components:
    prompt_embedding  = MiniLM("fix the auth bug")
    plan_embedding    = MiniLM("assign terminal to grep for mutex patterns")
    result_embedding  = MiniLM("found race condition in session_manager.rs")
    outcome_embedding = MiniLM("successfully added mutex, tests pass")

→ Store in Layer 1 (RVF):
    ingest_batch([prompt_emb, plan_emb, result_emb, outcome_emb],
                 [prompt_id, plan_id, result_id, outcome_id],
                 metadata: {kind, source, timestamp, session_id, run_id})

→ Index in Layer 2 (Hyperbolic HNSW):
    Project all embeddings to Poincaré ball
    Insert with hierarchy depth:
      prompt  → depth 0 (near center, general)
      plan    → depth 1 (intermediate)
      results → depth 2 (specific, near boundary)

→ Connect in Layer 3 (Hypergraph):
    Hyperedge {
      id: run_id,
      nodes: [prompt_id, plan_id, result_id, outcome_id],
      embedding: mean(all_embeddings),  // edge-level embedding
      confidence: quality_score,
      metadata: { strategy_type: "terminal", domain: "auth" }
    }
    CausalEdge: cause=plan_id, effect=outcome_id,
                context=[prompt_id], latency=5000ms

→ SONA trajectory:
    EndTrajectory { quality_score: 0.85 }
    → MicroLoRA adjusts embedding space
    → ReasoningBank stores compressed trajectory
```

### 12.5 Hypergraph Deep Dive: Two Implementations

ruvector has **two separate hypergraph implementations** at different abstraction levels:

**1. `ruvector-core::advanced::hypergraph` — Vector-centric**

Designed for embedding-aware retrieval. Hyperedges carry their own embeddings.
Storage is in-memory HashMap-based bipartite graph (entity ↔ hyperedge).

```rust
pub struct Hyperedge {
    pub id: String,
    pub nodes: Vec<VectorId>,           // N-ary
    pub description: String,
    pub embedding: Vec<f32>,            // embedding OF the relationship
    pub confidence: f32,
    pub metadata: HashMap<String, String>,
}

pub struct HypergraphIndex {
    entities: HashMap<VectorId, Vec<f32>>,
    hyperedges: HashMap<String, Hyperedge>,
    temporal_index: HashMap<u64, Vec<String>>,       // time_bucket → edge IDs
    entity_to_hyperedges: HashMap<VectorId, HashSet<String>>,  // bipartite
    hyperedge_to_entities: HashMap<String, HashSet<VectorId>>,
}
```

Key APIs: `add_entity`, `add_hyperedge`, `add_temporal_hyperedge`,
`search_hyperedges` (kNN over edge embeddings), `k_hop_neighbors` (BFS through
hyperedges), `query_temporal_range`.

**CausalMemory** wraps this with utility-scored retrieval:
`U = alpha*similarity + beta*causal_uplift - gamma*latency`

**2. `ruvector-graph::hyperedge` — Property-graph-centric**

Designed for typed, persistent, role-bearing relationships. Upstream uses redb
for storage, but **we persist these to RVF instead** (see 12.5a below).

```rust
pub struct Hyperedge {
    pub id: HyperedgeId,
    pub nodes: Vec<NodeId>,
    pub edge_type: String,              // typed (e.g., "EPISODE", "MEETING")
    pub description: Option<String>,
    pub properties: Properties,         // arbitrary key-value properties
    pub confidence: f32,
}

pub struct HyperedgeWithRoles {
    pub hyperedge: Hyperedge,
    pub roles: HashMap<NodeId, String>, // node → role assignment
}
```

Fluent builder API:
```rust
let episode = HyperedgeBuilder::new(
        vec![prompt_id, plan_id, result_id], "CONDUCTOR_RUN")
    .description("Auth bug fix run")
    .confidence(0.85)
    .property("strategy", "terminal")
    .property("domain", "auth")
    .build();
```

Roles let each node have a function within the episode:
```rust
let mut run = HyperedgeWithRoles::new(episode);
run.assign_role(prompt_id, "trigger");
run.assign_role(plan_id, "strategy");
run.assign_role(result_id, "outcome");
let strategies = run.nodes_with_role("strategy"); // → [plan_id]
```

In-memory indexing via `HyperedgeNodeIndex` (concurrent DashMap: node → Set<hyperedge_ids>).

### 12.5a Hypergraph Persistence in RVF (No redb)

RVF is our single persistence layer. The `.rvf` file format has the building
blocks to store hypergraph structure natively — no redb, no second database.

**RVF segment types relevant to hypergraph storage:**

| Segment | Hex | What it stores | How we use it |
|---|---|---|---|
| **Vec** | `0x01` | Raw vector payloads | Node embeddings (episodes, strategies, findings) |
| **Index** | `0x02` | HNSW adjacency lists | Vector similarity graph (automatic) |
| **Overlay** | `0x03` | "Graph overlay deltas, partition updates" | **Hyperedge records** — this segment is explicitly graph-aware |
| **Journal** | `0x04` | Typed mutation entries (extensible `entry_type: u8`) | **Hyperedge add/remove/update** mutations |
| **Meta** | `0x07` | Arbitrary key-value metadata (`Bytes(Vec<u8>)`) | Per-vector hyperedge membership lists |
| **MetaIdx** | `0x0D` | Metadata inverted indexes | Secondary index over hyperedge properties |
| **Profile** | `0x0B` | Domain profile declaration | `DomainProfile::RvGraph` (magic `0x52475248`, ext `.rvgraph`) |

**The storage strategy (three tiers, incrementally adopted):**

**Tier 1 (Phase 1-2, zero format changes):**

Use existing RVF primitives. No custom segment types needed.

```
Per-vector metadata (via ingest_batch):
  MetadataEntry {
    field_id: HYPEREDGE_MEMBERSHIP,   // u16 field ID
    value: Bytes(cbor([              // CBOR-encoded list of hyperedge IDs
      "run-2026-02-15-auth-fix",
      "session-morning-feb-15"
    ]))
  }

Hyperedge records (via Journal entries):
  Journal segment {
    entry_type: 0x02,    // ADD_HYPEREDGE (we define this)
    payload: cbor({
      id: "run-2026-02-15-auth-fix",
      nodes: [prompt_id, plan_id, result_id, outcome_id],
      edge_type: "CONDUCTOR_RUN",
      roles: { prompt_id: "trigger", plan_id: "strategy", ... },
      confidence: 0.85,
      properties: { domain: "auth", strategy: "terminal" },
      embedding: [0.12, 0.34, ...],   // edge-level embedding
      timestamp: 1739577600,
    })
  }

File-level profile:
  Profile segment: DomainProfile::RvGraph
  Extension: .rvgraph
```

On MemoryAgent startup: read all Journal entries with `entry_type >= 0x02`,
deserialize hyperedge records, rebuild the in-memory `HypergraphIndex` and
`HyperedgeNodeIndex`. This is fast — CBOR deserialization of a few thousand
hyperedges takes < 10ms.

**Tier 2 (Phase 3, Overlay segments):**

Use the Overlay segment (`0x03`) for batch hyperedge records. The Overlay
segment has no defined header struct yet in rvf-types — we define our own:

```rust
struct OverlayHeader {
    overlay_type: u8,        // 0x01 = HYPEREDGE_BATCH
    entry_count: u32,
    epoch: u32,
}

// Payload: packed array of hyperedge records
// More efficient than individual Journal entries for bulk operations
```

Overlay segments are preserved byte-for-byte through compaction, so
hypergraph structure survives background maintenance.

**Tier 3 (Phase 4+, custom segment type):**

Allocate a dedicated segment type for hyperedges:

```rust
const HYPEREDGE_SEG: u8 = 0x30;  // in unallocated range 0x24..0xEF

struct HyperedgeSegHeader {
    edge_count: u32,
    dimension: u16,           // embedding dimension
    has_roles: bool,
    has_temporal: bool,
}
```

Existing RVF tooling preserves unknown segment types byte-for-byte through
compaction, so this is safe. The range `0x24..0xEF` is open for custom types.

**Why this works without redb:**

- **Append-only Journal** gives us ordered hyperedge mutation history (add,
  remove, update). Chain-linked via `prev_journal_seg_id` for consistency.
- **Per-vector metadata** gives us hyperedge membership per node. The
  `MetaIdx` segment enables filtered search ("find all vectors in hyperedge X").
- **Overlay segments** give us batch hyperedge storage that's explicitly
  graph-aware in RVF's design intent.
- **Single .rvf file** = vectors + HNSW index + hypergraph structure + metadata.
  No external database. Perfect for per-user Firecracker VMs.
- **On startup**, rebuild in-memory structures from the Journal. The `.rvf`
  file is the single source of truth.

**Which in-memory structures we rebuild:**

```
.rvf file on disk
  │
  ├── Vec segments → vectors (loaded by rvf-runtime automatically)
  ├── Index segments → HNSW graph (loaded by rvf-runtime automatically)
  ├── Journal entries (type 0x02+) → HypergraphIndex + HyperedgeNodeIndex
  ├── Meta entries → per-vector hyperedge membership
  └── Overlay segments → batch hyperedge records (if Tier 2)
```

`ruvector-core`'s `HypergraphIndex` (bipartite HashMap) and `CausalMemory`
are rebuilt from Journal entries. `ruvector-graph`'s typed `HyperedgeWithRoles`
are deserialized from the same entries. Both are in-memory query structures
backed by the single `.rvf` file.

**Which one for ChoirOS?**

Use both in-memory representations, backed by RVF:
- **ruvector-core's HypergraphIndex** for embedding-aware retrieval and causal
  memory (Layer 3 query path). Fast similarity search over hyperedge embeddings.
- **ruvector-graph's typed Hyperedge** for structured queries with roles,
  properties, and (future) Cypher. Expressive pattern matching.

Both rebuild from the same Journal entries on startup. One `.rvf` file,
two in-memory views, zero redb.

### 12.6 GNN Refinement: When and How

GNN neural search is a **refinement layer**, not used on every query.

**When to activate:**
- Top-k results have similar scores (ambiguous ranking)
- Query is multi-faceted (benefits from graph-aware disambiguation)
- Conductor explicitly requests deep recall

**How it works on our episode graph:**

The GNN operates on the HNSW graph topology (Layer 2), not the hypergraph
(Layer 3). HNSW edges encode semantic similarity — if two memories are HNSW
neighbors, the GNN propagates information between them.

```
Standard search:    query → HNSW → top-k by distance
GNN neural search:  query → HNSW → candidates → GNN message passing → reranked
```

`hierarchical_forward()` processes the query through GNN layers:
1. Find candidates in top HNSW layers (coarse)
2. Aggregate neighbor embeddings via multi-head attention per candidate
3. Propagate down to finer layers with refined scores
4. Differentiable soft-attention produces final ranking

**The bridge we build:** After GNN reranks Layer 2 results, expand to Layer 3
(hypergraph) for full episode context. GNN tells us WHICH memories are most
relevant; hypergraph tells us WHAT ELSE was part of those episodes.

**Hyperedge-aware GNN (future):** ruvector's GNN currently operates on pairwise
edges only. To run GNN over hyperedges, we'd use clique expansion (decompose
each hyperedge into pairwise edges) or build a lightweight HyperGCN layer.
This is Phase 4+ work.

### 12.7 Mixed Curvature Attention: The Euclidean-Hyperbolic Bridge

The one deeply integrated cross-feature in ruvector upstream:

```rust
// Each embedding is split into components:
//   e[:d/2] = Euclidean component (content similarity)
//   e[d/2:] = Hyperbolic component (hierarchical position)

// Attention in both spaces:
let w_euclidean = dot_product(q_euc, k_euc);         // content match
let w_hyperbolic = -poincare_distance(q_hyp, k_hyp); // hierarchy match

// Learned mixing:
let alpha = sigmoid(mixing_weight);  // trained parameter
let combined = (1.0 - alpha) * w_euclidean + alpha * w_hyperbolic;

// Aggregation:
let euclidean_agg = weighted_sum(values_euc, softmax(combined));
let hyperbolic_agg = frechet_mean(values_hyp, softmax(combined));
```

For episodic memory: a single attention operation considers BOTH "is this memory
about the same topic?" (Euclidean) AND "is this memory at the right level of
specificity?" (Hyperbolic). The `mixing_weight` learns the optimal balance.

The 3-space variant (`MixedCurvatureFusedAttention`) adds Spherical geometry
for cyclic/periodic patterns (daily routines, recurring tasks). Available but
likely Phase 4+.

### 12.8 What ruvector DOESN'T Integrate (Gaps We Fill)

| Gap | What's missing upstream | What we build |
|---|---|---|
| GNN on hyperedges | GNN takes pairwise edges only | Clique expansion or lightweight HyperGCN layer |
| Hyperbolic hypergraph | No bridge between Poincaré geometry and hyperedge structure | Embed hyperedge centroids (Fréchet mean of nodes) in Poincaré ball. Hyperedge depth = mean depth of member nodes. |
| Cypher hyperedge execution | AST + semantic analysis exist, parser not wired | Use programmatic Rust API initially. Contribute parser fix upstream when stable. |
| SONA + graph | SONA adjusts embeddings, not topology | SONA adjusts Layer 1 embeddings. Causal memory adjusts Layer 3 topology scores. They stack: SONA biases WHICH memories surface, causal memory biases WHICH strategies rank highest. |
| Temporal + attention on hyperedges | TemporalBTSPAttention works on DAGs, not hyperedges | Use temporal hyperedges for filtering, attention mechanisms for ranking within results. |
| TopologyGated memory | Coherence gating exists but not wired to MemoryAgent | Use topology gating to freeze ingestion during context switches and resume during focused sessions. |

### 12.9 The Unified Retrieval Pipeline (Pseudocode)

```rust
impl MemoryAgent {
    async fn recall(&self, query: &str, top_k: usize) -> Vec<RetrievedMemory> {
        // 1. Embed
        let euclidean = self.embedder.embed(query);             // MiniLM 384-dim
        let sona_adjusted = self.sona.transform(&euclidean);    // MicroLoRA bias
        let poincare = exp_map_origin(&sona_adjusted);          // project to ball

        // 2. Layer 1: Fast recall from .rvf file
        let candidates = self.rvf_store.query(
            &sona_adjusted, top_k * 4,
            &QueryOptions {
                filter: self.time_and_scope_filter(),
                ..default()
            }
        );

        // 3. Layer 2: Hierarchy-aware reranking
        let reranked = self.hyperbolic_hnsw.search_dual(
            &euclidean, &poincare, top_k * 2, ef_search: 64
        );

        // 4. Optional: GNN refinement (if scores are ambiguous)
        let refined = if scores_are_ambiguous(&reranked) {
            self.gnn.neural_search(&sona_adjusted, top_k * 2, depth: 2)
        } else {
            reranked
        };

        // 5. Layer 3: Hypergraph episode expansion
        let mut episodes = Vec::new();
        for candidate in refined.iter().take(top_k) {
            let episode_nodes = self.hypergraph.k_hop_neighbors(candidate.id, 1);
            let causal_utility = if candidate.record.event_type.starts_with("conductor.") {
                self.causal_memory.query_with_utility(
                    &sona_adjusted, candidate.id, 1
                ).map(|r| r.utility_score)
            } else { None };

            episodes.push(RetrievedMemory {
                record: candidate.record.clone(),
                hnsw_score: candidate.distance,
                hyperbolic_depth: poincare_norm(&candidate.poincare_emb),
                causal_utility,
                episode_context: episode_nodes,
                sona_score: self.sona.adjusted_score(candidate.distance),
            });
        }

        // 6. Final composite ranking
        episodes.sort_by(|a, b| composite_score(b).cmp(&composite_score(a)));
        episodes.truncate(top_k);
        episodes
    }
}

fn composite_score(m: &RetrievedMemory) -> f32 {
    0.4 * m.hnsw_score
    + 0.3 * m.sona_score
    + 0.2 * m.causal_utility.unwrap_or(0.0)
    + 0.1 * (1.0 - m.hyperbolic_depth)  // prefer general over deep-specific
}
```

### 12.10 Why Each Mechanism Exists (Value vs Complexity)

Plain vector similarity (RVF + SONA alone) handles the 80% case. You embed a query,
find nearest neighbors, get relevant memories. The question is: what retrieval
failures does the 80% solution have, and are the remaining mechanisms worth the
complexity to fix them?

**Problem 1: Flat space can't represent hierarchy.**

A user goal ("make the app secure"), a strategy ("audit all auth endpoints"), and a
specific finding ("session_manager.rs line 42 has a race condition") all produce
similar embeddings — they're all "about auth." But they're at completely different
levels of abstraction. When the conductor needs a strategy, it gets goals, strategies,
and findings jumbled together. It can't distinguish "a high-level plan that worked"
from "a specific line-level fix."

**Hyperbolic HNSW fixes this.** Poincaré distance encodes depth. Goals near the center,
strategies in the middle, findings near the boundary. "Give me strategies for auth"
returns the middle layer, not the leaves. The geometry does the work — no metadata
filtering heuristics needed.

**Problem 2: Isolated memories lose episode context.**

Plain KNN returns 5 individual memory fragments. But episodes aren't individual facts —
they're groups of things that happened together. "The user asked X, the conductor
planned Y, terminal found Z, and the outcome was W" is one coherent story. Without
grouping, the conductor sees 5 unrelated scraps and has to infer relationships.

**Hypergraph hyperedges fix this.** Each conductor run is a hyperedge connecting all
its components. When any fragment matches, `k_hop_neighbors` expands to the full
episode. The conductor sees complete stories, not scraps. Roles (`trigger`, `strategy`,
`outcome`) let it understand the structure of each episode.

**Problem 3: Similarity isn't causality.**

"This strategy is semantically similar to the query" doesn't mean "this strategy will
work." A strategy that failed spectacularly might have the highest cosine similarity
to the current situation. Plain vector search can't distinguish "similar and successful"
from "similar and disastrous."

**CausalMemory fixes this.** Explicit cause-effect edges on the hypergraph track which
strategies led to which outcomes. `query_with_utility` boosts strategies with proven
causal success. This is a different signal from similarity — it's "similar AND it worked."

**Problem 4: Topology carries information that embeddings don't.**

Two memories might have identical embedding distances to the query, but one is
connected to many other relevant memories (a hub in the knowledge graph) and the
other is isolated. The hub is probably more informative — it's part of a rich episode
cluster. Plain KNN can't see this.

**GNN fixes this.** Message passing over the HNSW graph aggregates neighbor information.
A memory whose graph neighbors are also relevant gets boosted. An isolated memory with
no relevant neighbors gets demoted. The graph structure adds signal that embeddings
alone don't carry. But this only matters at scale (thousands of memories with
ambiguous rankings).

### 12.11 Value / Complexity Assessment

| Mechanism | Value | Complexity | When it matters | Verdict |
|---|---|---|---|---|
| Hypergraph + CausalMemory | **High** — episode context and causal reasoning are immediately useful from the first conductor run | **Low** — Journal entries in RVF, in-memory HashMap rebuild on startup, success counters | Always. Even 10 episodes benefit from grouping and outcome tracking. | **Phase 1.** Ship with the skeleton. |
| SONA | **High** — outcome-biased retrieval improves with every completed run | **Low** — standalone crate, 7.4K LoC, lightweight deps | After ~50 runs when there's enough trajectory data to learn from | **Phase 1.** Cheap to include, compounds over time. |
| Hyperbolic HNSW | **High for planning** — hierarchy-aware retrieval prevents level-of-abstraction confusion | **Medium** — vendor unpublished crate, add Euclidean→Poincaré projection step, DualSpaceIndex | After ~100 episodes at multiple hierarchy depths. Before that, flat HNSW is fine. | **Phase 2.** Add when episode depth exists. |
| GNN | **Moderate** — topology-aware ranking helps when KNN scores are ambiguous | **High** — training loop, model weight management, EWC for continual learning | After ~1000 memories when graph structure is rich enough to carry signal | **Phase 3.** Optimization layer, not foundation. |
| Cypher queries | **Moderate** — expressive episode patterns beyond programmatic composition | **Medium** — parser not fully wired upstream, programmatic API works now | When episode graphs are complex enough to need declarative queries | **Phase 3.** Use programmatic API first. |
| MixedCurvatureAttention | **Moderate** — bridges Euclidean content and hyperbolic hierarchy in one operation | **Medium** — needs hyperbolic embeddings to exist first | When both geometry types are in play (after Phase 2) | **Phase 3.** Refines Phase 2 output. |

### 12.12 Phased Rollout (Revised by Value)

| Phase | What | Why first | Crates |
|---|---|---|---|
| **1** | RVF storage + HNSW KNN + metadata filters + SONA + **hypergraph episodes + causal memory** | Episode grouping and "similar AND it worked" are useful from day one. Low complexity — Journal entries in RVF, in-memory rebuild, success counters. This is the minimum viable *intelligent* episodic memory, not just a vector store. | `rvf-runtime`, `ruvector-sona`, `ruvector-core` (HypergraphIndex, CausalMemory), `ort` |
| **2** | Hyperbolic HNSW + DualSpaceIndex | Hierarchy-aware retrieval prevents level-of-abstraction confusion in conductor planning. Needs enough episodes at different depths to matter. | `ruvector-hyperbolic-hnsw` (vendor), `ruvector-attention` (MixedCurvature) |
| **3** | GNN neural search + reflexion episodes + skills consolidation + Cypher | Graph-topology-aware ranking, self-critique memory, pattern extraction. These are optimization layers that compound on the structural foundation from Phases 1-2. | `ruvector-gnn`, `ruvector-graph` (Cypher), `ruvector-core` (AgenticDB) |
| **4** | Global knowledge store + cross-user SONA + published IP tracking | The platform play. Users publish learnings, everyone benefits. Requires hypervisor + auth + deploy. | Platform service, `ruvector-graph` (full Cypher) |

**Phase 1 is deliberately bigger than before.** The insight: hypergraph + causal memory
are low-complexity additions that transform the system from "vector search" to "episodic
memory with causal reasoning." The Journal-based RVF persistence means no additional
infrastructure — it's all in the same `.rvf` file. The value/complexity ratio is too
good to defer.

### 12.13 What Makes This More Than Fancy Grep

Each mechanism addresses a retrieval dimension that the previous ones can't:

| Dimension | Mechanism | What it captures |
|---|---|---|
| Content similarity | HNSW KNN (Euclidean) | "About the same topic" |
| Outcome learning | SONA MicroLoRA | "Patterns that succeeded rank higher" |
| Episode context | Hypergraph hyperedges | "These things happened together as one story" |
| Causal reasoning | CausalMemory utility | "This strategy caused good outcomes" |
| Hierarchical depth | Hyperbolic HNSW (Poincaré) | "At the right level of abstraction" |
| Graph topology | GNN message passing | "Well-connected memories are more informative" |
| Self-critique | Reflexion episodes | "What I learned from past mistakes" |
| Pattern extraction | Skills consolidation | "Repeated successful sequences → reusable skills" |

No single mechanism covers all dimensions. But **hypergraph + causal memory + SONA**
(Phase 1) covers the four most impactful dimensions for conductor planning quality.
The remaining mechanisms compound on that foundation.

**Filesystem handles "what exists now."**

**This memory system handles "what happened before, what worked, what failed, what's
related, and what to do differently this time."**

---

## 13. Runtime Control Surface

The MemoryAgent is not a black box. Users and the conductor need runtime control
over what gets remembered, how it's retrieved, and what's active.

### 13.1 MemoryConfig (Runtime-Adjustable)

```rust
pub struct MemoryConfig {
    pub enabled: bool,                          // master toggle
    pub ingest_enabled: bool,                   // stop recording, keep retrieval
    pub ingest_min_length: usize,               // skip trivially short content
    pub ingest_event_types: Option<HashSet<String>>, // whitelist event types (None = all)

    pub retrieval_enabled: bool,                // stop injecting context, keep recording
    pub retrieval_top_k: usize,                 // how many memories to surface (default 5)
    pub retrieval_time_window: Option<Duration>,// only recall from last N days
    pub retrieval_expand_episodes: bool,        // k_hop expansion on/off

    // Layer toggles (decide at query time, not build time)
    pub use_causal_scoring: bool,               // default true
    pub use_sona: bool,                         // default true
    pub use_hyperbolic: bool,                   // default false until Phase 2
    pub use_gnn: bool,                          // default false until Phase 3

    pub sona_learning_rate: f32,                // default 0.01
    pub causal_alpha: f32,                      // similarity weight (default 0.7)
    pub causal_beta: f32,                       // causal uplift (default 0.2)
    pub causal_gamma: f32,                      // latency penalty (default 0.1)
}
```

No `ingest_kinds` or `retrieval_kinds` filters. Semantic categories are not
hand-labeled — they're emergent from embeddings, HNSW topology, and SONA
learning. Filtering by `event_type` prefix (e.g., `"conductor."`) gives
provenance-based scoping where needed.

All fields have sensible defaults. On first boot, memory is enabled with basic
retrieval and all advanced layers off until implemented.

### 13.2 Inspection and Maintenance

```rust
pub struct MemoryStats {
    pub total_vectors: usize,
    pub total_hyperedges: usize,
    pub total_causal_edges: usize,
    pub sona_trajectory_count: usize,
    pub rvf_file_size_bytes: u64,
    pub last_ingest_at: Option<DateTime<Utc>>,
    pub last_recall_at: Option<DateTime<Utc>>,
}
```

API endpoints:
```
GET    /api/memory/config              → current MemoryConfig
PUT    /api/memory/config              → update (partial merge)
GET    /api/memory/stats               → MemoryStats
GET    /api/memory/search?q=...&k=5    → raw memory search (inspection)
GET    /api/memory/episodes            → recent episodes with summaries
GET    /api/memory/episodes/:id        → full episode (all nodes + roles)
POST   /api/memory/compact             → trigger .rvf compaction
POST   /api/memory/snapshot            → COW branch export
```

### 13.3 What Runtime Control Enables

- User pauses memory during a sensitive session → `ingest_enabled: false`
- Conductor narrows retrieval for a focused task → `retrieval_time_window: 7d`
- Developer inspects agent memory → `/api/memory/search?q=auth`
- User disables causal scoring for pure similarity → `use_causal_scoring: false`
- Publish workflow → COW snapshot for export to global store
- Disk management → trigger compaction, check file size

---

## 14. Memory Beyond Agentic Coding

The architecture so far is biased toward conductor plans and terminal executions.
But ChoirOS is a general-purpose desktop for research, creative writing, and IP
development — not just agentic coding.

### 14.1 The Four Domains

| Domain | Episodes | Valuable memories | Hierarchy |
|---|---|---|---|
| **Agentic Coding** | Conductor runs, terminal executions, code changes | "This strategy worked for auth bugs" | goal → strategy → assignment → finding |
| **Research** | Search queries, source discovery, reading sessions, synthesis | "I found this source 2 weeks ago", "this inquiry was a dead end" | question → sub-questions → sources → findings → synthesis |
| **Creative Writing** | Drafting sessions, revision cycles, theme development | "This character arc builds toward X", "the voice shifted and the user liked it" | project → themes → scenes → details |
| **IP Development** | Document creation, agent building, publish events | "This published doc got high usage", "this agent evolved from a simpler version" | portfolio → projects → artifacts → versions |

### 14.2 The Primitives Are Universal

The memory architecture doesn't need domain-specific code paths or categories:

| Primitive | How it works across domains |
|---|---|
| Vector similarity | The query embedding finds similar content regardless of domain. A "research methodology" and a "coding strategy" may cluster naturally. |
| Hyperedge episodes | Any multi-step process becomes a hyperedge: conductor run, research session, writing session, dev cycle. The structure is the same. |
| Roles in hyperedge | Generic: trigger/context/action/outcome. Not "conductor strategy" — just "the thing that caused the next thing." |
| Causal edges | Strategy → outcome. Domain-agnostic: "this approach led to this result." |
| SONA learning | Learns which embedding regions correlate with good outcomes. Doesn't know about domains — just patterns of success. |
| Temporal queries | Filter by timestamp. Universal. |
| Hierarchy (hyperbolic) | Depth emerges from the content: abstract goals → concrete actions → specific findings. True in every domain. |

**No `MemoryKind` or `MemorySource` enums.** The embedding space, HNSW topology,
Layer A centroids, and SONA learning discover the natural categories. A coding
strategy and a research methodology might cluster together — that's a feature,
not a bug. Hardcoded labels would hide this cross-domain resonance.

### 14.3 Domains Shape Retrieval via Query, Not Filters

The conductor shapes retrieval through HOW it phrases the query, not by
filtering on hardcoded categories:

```
Coding:     "What strategies have worked for authentication bugs?"
Research:   "What sources have I found about distributed systems?"
Creative:   "What themes have I been developing in the narrative?"
Publishing: "Which published documents got high engagement?"
```

The embedding space finds the right memories for each query. If the conductor
wants only recent memories, it uses `retrieval_time_window`. If it wants only
memories from a specific session, it uses `session_id` filter. These are
provenance filters, not semantic categories.

### 14.4 What We Don't Decide Now

- Exact event-to-memory mappings for research/creative (define when those agents exist)
- Publishing workflow details (define when hypervisor + auth exist)
- Domain-specific quality heuristics (SONA learns these from trajectories)
- Cross-domain memory blending (emerges naturally from shared embedding space)
- Whether categories are ever needed (if they are, SONA's ReasoningBank and
  Layer A's centroids provide learned categories, not hardcoded enums)

What we do decide now:
- No hardcoded semantic categories. Provenance metadata only.
- Runtime control supports per-session tuning of retrieval parameters
- Hyperedge roles are generic (trigger/context/action/outcome)
- The embedding space is the semantic index. Everything else is provenance.
