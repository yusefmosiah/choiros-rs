# Research Prompt: Hyperbolic HNSW for ChoirOS Episodic Memory

## Context

ChoirOS is building a MemoryAgent — an episodic memory system for AI agents that
stores, retrieves, and learns from past interactions. The retrieval layer uses the
ruvector ecosystem (Rust-native vector search). We've confirmed that `ruvector-hyperbolic-hnsw`
exists as a fully implemented but unpublished crate (~1500+ LoC) in the ruvector monorepo.

We need to understand whether hyperbolic HNSW is the right geometry for our episodic
memory, how to integrate it, and what the practical tradeoffs are.

## What We Know

### The crate exists at `crates/ruvector-hyperbolic-hnsw/`

**7 source files:**
- `poincare.rs` — Full Poincaré ball model: `poincare_distance`, `mobius_add`,
  `exp_map`, `log_map`, `frechet_mean`, `parallel_transport`, `project_to_ball`,
  `fused_norms` (SIMD 4-wide unrolling), batch distance
- `hnsw.rs` — `HyperbolicHnsw` with multi-layer insert/search, tangent-space pruning
  fallback, `DualSpaceIndex` (Euclidean + hyperbolic mutual ranking fusion)
- `tangent.rs` — The speed trick: `TangentCache` precomputes log-map coordinates at
  Frechet centroid; `TangentPruner` does fast Euclidean pruning in tangent space then
  exact Poincaré ranking on survivors
- `shard.rs` — `ShardedHyperbolicHnsw` with per-shard curvature, `CurvatureRegistry`
  with canary testing and hot reload, hierarchy metrics (Spearman radius-depth correlation)
- `error.rs`, `lib.rs`, `tests/math_tests.rs`, `benches/hyperbolic_bench.rs`

**Also in the monorepo:**
- `ruvector-postgres/src/hyperbolic/` — Postgres extension with PoincaréBall + LorentzModel
- `ruvector-attention/src/hyperbolic/` — Hyperbolic attention: `HyperbolicAttention`,
  `MixedCurvatureAttention`, `LorentzCascadeAttention` with Busemann scoring
- `ruvector-dag/src/attention/hierarchical_lorentz.rs` — DAG-specific Lorentz attention

**It's excluded from workspace build** (`exclude = [...]` in root Cargo.toml) — builds
independently, has its own Cargo.lock, not yet published to crates.io.

### Our episodic memory structure is hierarchical

```
User goal (abstract/general)
  └── Conductor run (orchestration strategy)
        ├── Strategy chosen (plan-level)
        ├── Worker assignment 1 (task-level)
        │     ├── Tool call A (action-level)
        │     └── Tool call B (action-level)
        └── Worker assignment 2 (task-level)
              └── Finding discovered (leaf-level)
```

This is a tree. Trees embed perfectly in hyperbolic space with zero distortion.
In Euclidean space, tree distance relationships are inevitably distorted.

## What We Need to Learn

### 1. Practical performance characteristics

- What are the actual benchmark numbers from `benches/hyperbolic_bench.rs`?
  Poincaré distance is more expensive than L2 — how much more? Is the tangent
  space pruning trick fast enough that the overhead is acceptable?
- How does `DualSpaceIndex` (Euclidean prune + Poincaré rank) compare to pure
  Poincaré HNSW in both speed and recall? This hybrid approach might give us
  the best of both worlds.
- Insert throughput: how does building a hyperbolic HNSW compare to standard HNSW?
- Memory overhead of `TangentCache`?

### 2. Embedding pipeline integration

- MiniLM-L6-v2 produces Euclidean 384-dim embeddings. How do we get these into the
  Poincaré ball? Options:
  a. Post-hoc projection (normalize + scale into ball)
  b. Learned Poincaré embedding layer (exponential map from tangent space)
  c. Use a hyperbolic embedding model directly (do any exist as ONNX?)
- What curvature value works for our hierarchy depth (~5 levels)?
- Does the `exp_map` in `poincare.rs` handle the Euclidean-to-hyperbolic projection?

### 3. Integration path with rvf-runtime

- `rvf-runtime` uses `rvf-index` for HNSW, which has its own distance function.
  Can we plug `ruvector-hyperbolic-hnsw` into rvf-runtime's query path?
- Or do we use `ruvector-hyperbolic-hnsw` as a standalone in-memory index loaded
  from the .rvf file's raw vectors?
- The `DualSpaceIndex` approach suggests keeping both Euclidean (for fast prune)
  and Poincaré (for accurate rank) — how would this layer over RVF persistence?

### 4. Per-shard curvature and hierarchy detection

- `ShardedHyperbolicHnsw` allows different curvature per shard. Does this mean
  different memory types (episodes vs strategies vs findings) could have different
  optimal curvatures?
- The `HierarchyMetrics` with Spearman radius-depth correlation — can this auto-detect
  whether our episode data actually has hierarchical structure? (Validation metric.)
- Canary testing for curvature changes — how would this work in production? A/B test
  curvature values on live retrieval and measure quality?

### 5. Tangent space pruning economics

- The tangent space trick (log-map to centroid, prune with Euclidean, rank with Poincaré)
  is the key performance optimization. What's the typical prune ratio? If it prunes 90%
  of candidates with Euclidean, then only 10% need expensive Poincaré distance.
- How often does `TangentCache` need rebuilding? On every insert? Periodically?
- `tangent_micro_update` for incremental writes — what's the quality degradation vs
  full cache rebuild?

### 6. Comparison with Euclidean HNSW + SONA

- SONA (our planned learning layer) adjusts Euclidean embeddings via MicroLoRA to bias
  retrieval toward successful patterns. Does this conflict with hyperbolic geometry?
  Can SONA's LoRA matrices work in tangent space?
- Or is hyperbolic geometry a replacement for some of what SONA does? Hierarchy is
  captured by the geometry rather than learned by LoRA adjustments.
- Can both coexist: hyperbolic geometry for structural hierarchy, SONA for outcome-based
  learning?

### 7. The unpublished status — risk assessment

- The crate is excluded from workspace build. Is this because it's experimental,
  or because it's waiting for a stable API?
- Are there known issues, TODOs, or failing tests?
- What's the minimum Rust version? (ruvector-core requires 1.87)
- Could we vendor the crate directly into our workspace?

### 8. The broader hyperbolic ecosystem in ruvector

- `HyperbolicAttention` in `ruvector-attention` — could this enhance GNN message
  passing in hyperbolic space? (Hyperbolic GNN for episode graph refinement?)
- `MixedCurvatureAttention` — does this allow combining flat (Euclidean), positively
  curved (spherical), and negatively curved (hyperbolic) spaces?
- `LorentzCascadeAttention` with Busemann scoring — what does this provide beyond
  basic Poincaré?

## Deliverables

1. **Benchmark analysis** — concrete numbers for Poincaré distance, tangent pruning,
   insert throughput, and DualSpaceIndex recall vs pure Euclidean HNSW.
2. **Embedding pipeline design** — how to get MiniLM Euclidean embeddings into the
   Poincaré ball with minimal quality loss.
3. **Integration architecture** — how hyperbolic HNSW sits alongside RVF persistence,
   ruvector-core graph features, and SONA learning.
4. **Curvature strategy** — recommended starting curvature for 5-level episode hierarchy,
   and whether per-shard curvature is worth the complexity.
5. **Risk assessment** — maturity of the crate, vendoring strategy, fallback plan.
6. **Phase recommendation** — should this be Phase 2 (worth doing early because it
   fundamentally improves hierarchy representation) or Phase 4+ (optimization after
   basic system works)?
