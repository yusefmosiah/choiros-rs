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

1. Implement Phase 1: MemoryAgent actor skeleton + EventRelay ingestion pipeline.
2. Implement Phase 2: MemoryProvider trait + conductor/worker retrieval integration.
3. Implement Phase 3: SONA trajectory tracking and adaptive retrieval.
4. Design the global knowledge store schema and publish/subscribe protocol.

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

The ruvector monorepo contains 13+ crates organized into two independent subsystems:
the older `ruvector-core` (redb-backed) and the newer RVF stack (append-only file format
with progressive HNSW). We use the RVF stack.

### The RVF Stack (what we use)

| Crate | Version | Size | Purpose in ChoirOS |
|---|---|---|---|
| `rvf-runtime` | 0.1.0 | 5.5K+ LoC | The vector store runtime. API: `create`, `ingest_batch`, `query`, `delete`, `compact`. Append-only segment-based `.rvf` files. Single-writer / multi-reader. COW branching for snapshots. Orchestrates `rvf-index` for HNSW search. |
| `rvf-index` | 0.1.0 | 2.7K LoC | Pure-Rust HNSW implementation with **progressive 3-tier loading** (Layer A/B/C). Stored as `INDEX_SEG` segments inside `.rvf` files. No external dependencies. `no_std` compatible. |
| `rvf-types` | 0.1.0 | 3.2K LoC | Format specification as Rust types. Segment headers, type enums, flags. `no_std` compatible. Transitive dependency of `rvf-runtime`. |
| `ruvector-sona` | 0.1.5 | 7.4K LoC | Adaptive learning. MicroLoRA + EWC++ + ReasoningBank. Independent of storage layer — works with any vector source. Makes retrieval improve over time. |
| `ort` | latest | varies | ONNX Runtime for MiniLM-L6-v2 embeddings. 384-dim, ~1ms/embed on CPU, ~22MB model file. |

### Why RVF over ruvector-core

`ruvector-core` (v2.0.1, 8.9K LoC) uses redb for storage and the external `hnsw_rs`
crate for indexing. It works, but it's the older approach in the ruvector ecosystem.

`rvf-runtime` is the newer, purpose-built vector file format with key advantages:

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
| `ruvector-core` | Uses redb + external `hnsw_rs`. The older storage approach. We use RVF instead. No integration path between ruvector-core and RVF exists today. |
| `ruvllm` (v2.0.2, 84K LoC) | Full local LLM inference engine (Candle framework, Metal/CUDA shaders, paged attention, GGUF loading). ChoirOS uses external model providers (Claude Bedrock, ZaiGLM) via BAML. Our embedding needs are served by MiniLM-L6-v2 via ONNX Runtime — a 22MB encoder model, not a generative LLM. Using ruvllm for this would triple compile time and binary size for zero value. |

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

A `MemoryRecord` is the unit of episodic memory:

```rust
pub struct MemoryRecord {
    pub id: String,                    // ULID
    pub embedding: Vec<f32>,           // 384-dim from MiniLM-L6-v2
    pub text: String,                  // human-readable content
    pub source: MemorySource,          // where this came from
    pub kind: MemoryKind,              // classification
    pub quality_score: Option<f32>,    // SONA outcome score (0.0-1.0)
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,   // flexible extra data
}

pub enum MemorySource {
    UserInput,           // direct user messages
    ConductorPlan,       // conductor's assignment strategy
    WorkerResult,        // terminal/researcher output
    WriterRevision,      // living document mutations
    TrajectoryOutcome,   // SONA learning outcome
}

pub enum MemoryKind {
    Episode,             // a thing that happened
    Strategy,            // a plan that was executed
    Finding,             // a fact that was discovered
    Pattern,             // a recurring observation
}
```

### 3.3 Ingestion Pipeline (EventRelay -> MemoryAgent)

Zero modifications to existing event emitters. A `MemoryEventRelay` subscribes
to EventBus topics and forwards high-signal events to MemoryAgent for embedding
and storage.

```
EventStore -> EventRelayActor -> EventBus -> MemoryEventRelay -> MemoryAgent
                                                (filter + map)    (embed + store)
```

**Event type mapping (~15 high-signal events):**

| Event Type | MemorySource | MemoryKind | What it captures |
|---|---|---|---|
| `chat.user_msg` | UserInput | Episode | What the user asked for |
| `conductor.run.started` | ConductorPlan | Strategy | What strategy the conductor chose |
| `conductor.assignment.dispatched` | ConductorPlan | Strategy | Which capability was assigned what |
| `conductor.run.completed` | ConductorPlan | Strategy | Final outcome of the plan |
| `conductor.run.failed` | ConductorPlan | Strategy | What went wrong |
| `worker.report.received` | WorkerResult | Finding | What a worker discovered/produced |
| `worker.task.progress` | WorkerResult | Episode | Intermediate worker state |
| `worker.task.document_update` | WriterRevision | Finding | Living doc mutations |
| `researcher.search.completed` | WorkerResult | Finding | Research results |
| `terminal.command.completed` | WorkerResult | Episode | Shell execution outcomes |
| `writer.revision.created` | WriterRevision | Finding | Document version changes |

### 3.4 Retrieval: Three Trigger Points

**Trigger 1: User input (before conductor plans)**

```
User message arrives
  -> MemoryAgent.Recall { query: user_message, top_k: 5, kinds: [all] }
  -> Returns: Vec<RetrievedMemory> with SONA-adjusted scores
  -> Injected into conductor's system context before planning
  -> Conductor sees: "Relevant history: [episodes]"
```

**Trigger 2: Living doc creation/mutation**

```
Writer creates or patches a document
  -> MemoryAgent.Recall { query: doc_content_summary, top_k: 3, kinds: [Finding, Pattern] }
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
/// Implemented by ActorMemoryProvider which holds ActorRef<MemoryAgentMsg>.
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

pub struct MemoryFilter {
    pub kinds: Option<Vec<MemoryKind>>,
    pub sources: Option<Vec<MemorySource>>,
    pub since: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
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

## 7. Implementation Phases

### Phase 1: Skeleton + Ingestion (1 week)

- Create `sandbox/src/actors/memory/mod.rs` — MemoryAgent actor
- Create `sandbox/src/actors/memory/protocol.rs` — message types
- Create `sandbox/src/actors/memory/embedder.rs` — MiniLM via ort
- Create `sandbox/src/actors/memory/relay.rs` — EventBus subscriber
- Add to supervision tree in `supervisor/session.rs`
- Add `rvf-runtime`, `ruvector-sona`, `ort` to `sandbox/Cargo.toml`
- Gate: events flow from EventBus -> MemoryAgent -> embedded and stored in `.rvf` file

### Phase 2: Retrieval Integration (1 week)

- Create `sandbox/src/actors/memory/provider.rs` — MemoryProvider trait
- Inject into `ConductorArguments` and `model_gateway.rs`
- Inject into `TerminalAdapter::get_system_context()`
- Inject into `ResearcherAdapter::get_system_context()`
- Inject into Writer at synthesis/planning points
- Wire through `app_state.rs`
- Gate: conductor and workers receive relevant memory context on every turn

### Phase 3: SONA Learning (1 week)

- Map conductor run lifecycle events to SONA trajectories
- Implement quality score derivation heuristic
- Enable MicroLoRA instant learning on trajectory completion
- Enable EWC++ background consolidation on timer
- Enable ReasoningBank cluster updates
- Persist SONA state to `data/{user_id}/sona.json`
- Gate: retrieval scores change based on trajectory outcomes

### Phase 4: Global Knowledge Store (after hypervisor + auth)

- Deploy standalone rvf-runtime instance as platform service
- Implement publish API (user -> global store, COW branch from local .rvf)
- Implement query API (user VM -> global store)
- Implement two-tier retrieval merge (local + global)
- Implement SONA coordination loop (cross-user pattern aggregation)
- Gate: users benefit from each other's published learnings

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
