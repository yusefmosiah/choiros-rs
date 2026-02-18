# Codesign Sketch + Questions: RLM, RuVector, Marginalia, and Memory Tiers

Date: 2026-02-17
Status: Pre-draft design sketch (not contract-authoritative)
Purpose: Capture current commitments, triangulate unknowns, and define the question set before drafting authoritative specs.

## Narrative Summary (1-minute read)

We are co-designing one architecture with three tightly coupled spines:
1. RLM control flow (`NextAction`, topology choice, delegation).
2. RuVector memory substrate (retrieval, expansion, packing).
3. Marginalia observation UX (semantic changesets, annotations, version navigation).

Current direction is explicit:
1. RLM working memory is in RAM.
2. Episodic memory is stored and retrieved from vectors.
3. Artifact memory is file-backed and authoritative.
4. All artifacts are also indexed in RuVector for retrieval and contextual expansion.
5. Append-only chat history is not the primary context mechanism; it remains an optimization/audit path.

This document is intentionally not a final contract. It is a sketch + question map used to converge before freezing schemas and rollout gates.

## What Changed

1. Consolidated the combined design space into one pre-draft artifact.
2. Converted recent decisions into explicit assumptions for codesign.
3. Added a triangulation matrix for unresolved decisions across RLM, RuVector, and Marginalia.
4. Added a question set that must be resolved before authoritative docs are drafted.

## What To Do Next

1. Resolve all `Gate 0` questions in this document.
2. Freeze v1 data contracts only after those answers are explicit.
3. Convert this sketch into authoritative architecture docs with test gates.
4. Start implementation only after contract + verification sign-off.

## Scope

In scope:
1. Data representation and retrieval/packing behavior for RuVector.
2. RLM model-callable retrieval interfaces and context composition shape.
3. Marginalia data dependencies (semantic changesets, annotation anchors, version graph hooks).
4. Promotion boundaries across working, episodic, and artifact memory.

Out of scope:
1. UI pixel design.
2. Provider/model-specific prompt tuning.
3. Production-scale infra details for global publish until local contracts are stable.

## Fixed Commitments (Assumed True for Codesign)

1. Filesystem artifacts are canonical truth for build/runtime outcomes.
2. All artifacts are indexed in RuVector for retrieval.
3. RLM chooses topology dynamically; deterministic rails remain for safety/operability.
4. Context should be composed per turn from callable retrieval APIs, not from long chat append by default.
5. Marginalia should consume semantic changes, provenance, and version context, not raw chat stream as primary UI.

## Sketch: Unified Memory and Retrieval Flow

```text
User objective
  -> RLM turn starts (RAM working memory only)
  -> Model calls retrieval APIs
       1) artifact_search(query, filters)
       2) artifact_expand(hit_ids, expansion_mode)
       3) artifact_context_pack(objective, token_budget)
  -> Context pack assembled
       - primary artifact slices
       - metadata/provenance
       - semantic change context
       - confidence/freshness signals
  -> Model decides NextAction (ToolCalls/FanOut/Recurse/Complete/Block)
  -> Tool/worker execution updates artifacts on disk
  -> Ingestion updates:
       - episodic vectors (trajectory/decision/outcome summaries)
       - artifact vectors (latest content + metadata)
       - event log append (audit/cost/replay path)
```

## Sketch: Data Planes

1. Working plane (RAM):
   - Turn-local and branch-local memory.
   - Discardable.
   - Not directly authoritative.

2. Episodic plane (RuVector vectors):
   - Run/session trajectories, decisions, outcomes, quality.
   - Retrieval substrate for planning and strategy recall.

3. Artifact plane (Files + RuVector mirrors):
   - Files are source of truth.
   - RuVector stores searchable artifact representations plus metadata.
   - Retrieved artifacts must resolve back to file refs/hashes.

## Sketch: Retrieval Modes Beyond Basic KNN

1. Similarity:
   - Top-k candidate artifacts/episodes.

2. Constrained reranking:
   - Scope (user/session/run), recency, success/quality, staleness penalties.

3. Context expansion:
   - Neighbor artifacts.
   - Related episode checkpoints.
   - Semantic-change neighbors.
   - Dependency/provenance edges.

4. Context packing:
   - Token-budgeted assembly with rationale and confidence.
   - Structured output for model use, not raw dump.

## Triangulation Matrix (Unknowns to Reconcile)

| Topic | Option A | Option B | Option C | Decision Gate |
|---|---|---|---|---|
| Artifact chunking | file-level | section/block-level | symbol/AST-level | Offline benchmark + retrieval precision on task set |
| Artifact embedding text | raw content | normalized + headers | summary + selected spans | Hit-rate + token efficiency |
| Episodic granularity | per step | per checkpoint | per episode with checkpoints | Plan quality uplift vs storage noise |
| Expansion depth | fixed deterministic | model-selected bounded | adaptive by confidence | Cost/latency + success delta |
| Staleness handling | hard invalidate on hash drift | soft downrank | hybrid with threshold | Wrong-context rate after edits |
| Marginalia anchors | offset-only | quote+context | hybrid with structure IDs | Re-anchor success across revisions |
| Semantic change schema | op taxonomy only | op + impact summary | op + impact + verification evidence | Human scan speed + regression detection |
| Promotion episodic -> permanent | manual only | policy assisted | fully automated with review queue | Precision/recall of promoted knowledge |

## Gate 0 Questions (Must Answer Before Drafting Authoritative Contracts)

1. What is the canonical artifact unit in RuVector v1 (file, section, symbol, or mixed)?
2. What minimum metadata is required per artifact record for safe context packing?
3. Which expansion edges are mandatory in v1 (`derived_from`, `changed_with`, `validated_by`, others)?
4. What staleness policy applies when file hash/version diverges from indexed content?
5. What are the hard token budget policies for context packs per model class?
6. What guarantees must `artifact_context_pack` provide (determinism, rationale fields, confidence)?
7. Which episodic records are allowed to influence topology choice (`NextAction`)?
8. What is the minimum semantic changeset shape needed for Marginalia consumption?
9. How are annotation anchors stored so they survive non-trivial document/code edits?
10. What is the acceptance threshold for replacing append-only chat context in default flows?

## Verification Sketch (Pre-Contract)

1. Build a fixed replay corpus of representative tasks (writer + coding + research).
2. Define baseline against current retrieval and planning behavior.
3. Evaluate each unknown via A/B variants with fixed budgets.
4. Score on:
   - objective success,
   - retrieval precision/recall,
   - context token cost,
   - latency,
   - wrong-context incidents,
   - human interpretability of semantic changes.
5. Promote only variants that beat baseline with statistically meaningful margin.

## Risks to Watch Early

1. Over-indexing raw artifacts without stable chunking can increase noise and pack irrelevant context.
2. Expansion without strict budgets can make RLM expensive and unstable.
3. Weak staleness controls can leak outdated code/doc context into planning turns.
4. Marginalia without robust anchors can collapse under frequent revisions.
5. Premature permanent-memory promotion can lock in low-quality patterns.

## Exit Condition for This Sketch

This pre-draft is complete when:
1. `Gate 0` questions have explicit answers.
2. A v1 contract outline exists for retrieval and context packing.
3. Verification matrix is accepted by architecture owners.
4. Authoritative drafting can proceed without unresolved core schema ambiguity.

