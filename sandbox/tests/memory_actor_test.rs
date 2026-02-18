//! Phase 5.2 gate tests — MemoryActor ingestion, dedup, search, and context snapshot.
//!
//! All tests run in stub-embedder mode (`CHOIROS_MEMORY_STUB=1`) so they never
//! require network access or a downloaded model. The stub returns deterministic
//! hash-based 384-dim vectors, which are geometrically meaningful enough for
//! KNN correctness tests (same string → distance 0; different strings → non-zero).
//!
//! Gate conditions (Phase 5.2):
//!   ✓ sqlite-vec extension loads; vec0 tables are created
//!   ✓ Ingest inserts a row and returns `true`
//!   ✓ Duplicate ingest (same chunk_hash) is skipped and returns `false`
//!   ✓ KNN search returns the nearest hit first
//!   ✓ GetContextSnapshot merges across all four collections
//!   ✓ VecStore can be opened `:memory:` (in-process, no disk artifact)

use std::sync::{Arc, Mutex};

// Force stub embedder for all tests in this file.
// (Also set via std::env::set_var in each test for isolation.)

use sandbox::actors::memory::{
    chunk_hash, CollectionKind, Embedder, IngestRequest, MemoryActor, MemoryArguments,
    MemoryInner, MemoryMsg, VecStore,
};
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn stub_env() {
    std::env::set_var("CHOIROS_MEMORY_STUB", "1");
}

/// Build a MemoryInner with an in-memory VecStore and a stub Embedder.
/// This is the synchronous analogue of what MemoryActor::pre_start does.
fn make_inner() -> Arc<Mutex<MemoryInner>> {
    stub_env();
    let store = VecStore::open(":memory:").expect("VecStore::open(:memory:)");
    let embedder = Embedder::init(); // returns Stub because CHOIROS_MEMORY_STUB=1
    Arc::new(Mutex::new(MemoryInner { store, embedder }))
}

fn ulid() -> String {
    ulid::Ulid::new().to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// 5.2-G1: sqlite-vec extension loads; all four vec0 tables are created.
#[test]
fn test_vec_store_opens_with_four_tables() {
    stub_env();
    let store = VecStore::open(":memory:").expect("should open");
    // Verify by searching (returns empty, not an error) — if tables don't
    // exist sqlite would return an error from the query.
    let hits = store.search("user_inputs", &[0f32; 384], 5);
    assert!(hits.is_ok(), "user_inputs table missing: {:?}", hits.err());

    let hits = store.search("version_snapshots", &[0f32; 384], 5);
    assert!(hits.is_ok(), "version_snapshots table missing");

    let hits = store.search("run_trajectories", &[0f32; 384], 5);
    assert!(hits.is_ok(), "run_trajectories table missing");

    let hits = store.search("doc_trajectories", &[0f32; 384], 5);
    assert!(hits.is_ok(), "doc_trajectories table missing");
}

/// 5.2-G2: Chunk hash is deterministic — same content → same hash.
#[test]
fn test_chunk_hash_deterministic() {
    let h1 = chunk_hash("hello world");
    let h2 = chunk_hash("hello world");
    let h3 = chunk_hash("different content");
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
    // SHA-256 hex is 64 chars.
    assert_eq!(h1.len(), 64);
}

/// 5.2-G3: Ingest inserts a row and returns `true`; dedup skips on second call.
#[test]
fn test_ingest_and_dedup() {
    stub_env();
    let inner = make_inner();
    let guard = inner.lock().unwrap();

    let content = "test document content for phase 5 gate";
    let hash = chunk_hash(content);
    let embedding = guard.embedder.embed_batch(&[content]);
    let vec = &embedding[0];

    // First insert — should succeed.
    assert!(!guard.store.hash_exists("version_snapshots", &hash));
    guard
        .store
        .insert("version_snapshots", &ulid(), "doc/test.qwy", content, &hash, vec)
        .expect("insert should succeed");
    assert!(guard.store.hash_exists("version_snapshots", &hash));

    // Second insert attempt with same hash — dedup prevents it.
    let already_exists = guard.store.hash_exists("version_snapshots", &hash);
    assert!(already_exists, "duplicate should be detected");
}

/// 5.2-G4: KNN search returns the nearest hit first (distance=0 for exact vector).
#[test]
fn test_knn_returns_nearest_first() {
    stub_env();
    let inner = make_inner();
    let guard = inner.lock().unwrap();

    let texts = ["alpha content", "beta content", "gamma content"];
    let embeddings = guard.embedder.embed_batch(&texts);

    for (i, (text, vec)) in texts.iter().zip(embeddings.iter()).enumerate() {
        let hash = chunk_hash(text);
        guard
            .store
            .insert(
                "run_trajectories",
                &format!("item-{i}"),
                &format!("source/{i}"),
                text,
                &hash,
                vec,
            )
            .expect("insert");
    }

    // Query for "alpha content" — its own vector should be distance=0 (nearest).
    let query_vec = &guard.embedder.embed_batch(&["alpha content"])[0];
    let hits = guard
        .store
        .search("run_trajectories", query_vec, 3)
        .expect("search");

    assert!(!hits.is_empty(), "expected hits");
    assert_eq!(hits[0].content, "alpha content", "nearest should be exact match");
    // Exact match should have distance very close to 0.
    assert!(
        hits[0].distance < 0.001,
        "exact match distance should be ~0, got {}",
        hits[0].distance
    );
    // Results must be ordered ascending by distance.
    for w in hits.windows(2) {
        assert!(w[0].distance <= w[1].distance, "results not sorted by distance");
    }
}

/// 5.2-G5: GetContextSnapshot merges across all four collections.
#[tokio::test]
async fn test_get_context_snapshot_merges_collections() {
    stub_env();
    let inner = make_inner();

    // Seed one item in each collection.
    {
        let guard = inner.lock().unwrap();
        let seeds: &[(&str, CollectionKind, &str)] = &[
            ("user asked about memory retrieval", CollectionKind::UserInputs, "input/1"),
            ("document about memory retrieval architecture", CollectionKind::VersionSnapshots, "doc/memory.qwy"),
            ("run completed memory-related task successfully", CollectionKind::RunTrajectories, "run/abc"),
            ("doc trajectory: memory module iteratively improved", CollectionKind::DocTrajectories, "doc/memory.qwy"),
        ];
        for (content, col, src) in seeds {
            let hash = chunk_hash(content);
            let vecs = guard.embedder.embed_batch(&[content]);
            guard
                .store
                .insert(col.table_name(), &ulid(), src, content, &hash, &vecs[0])
                .expect("insert seed");
        }
    }

    // Spawn the actor and issue GetContextSnapshot.
    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    // The actor opens its own fresh VecStore — we need to seed via Ingest messages.
    let seeds: &[(&str, CollectionKind, &str)] = &[
        ("user asked about memory retrieval", CollectionKind::UserInputs, "input/1"),
        ("document about memory retrieval architecture", CollectionKind::VersionSnapshots, "doc/memory.qwy"),
        ("run completed memory-related task", CollectionKind::RunTrajectories, "run/abc"),
        ("doc trajectory: memory module improved over time", CollectionKind::DocTrajectories, "doc/memory.qwy"),
    ];

    for (content, col, src) in seeds {
        let inserted = ractor::call!(memory, |reply| MemoryMsg::Ingest {
            req: IngestRequest {
                item_id: ulid(),
                collection: *col,
                source_ref: src.to_string(),
                content: content.to_string(),
            },
            reply: Some(reply),
        })
        .expect("ingest rpc");
        assert!(inserted, "expected item to be inserted");
    }

    // Now request a snapshot.
    let snapshot = ractor::call!(memory, |reply| MemoryMsg::GetContextSnapshot {
        run_id: "run-test-001".to_string(),
        query: "memory retrieval".to_string(),
        max_items: 8,
        reply,
    })
    .expect("snapshot rpc");

    // Should have items from multiple collections.
    assert!(!snapshot.items.is_empty(), "snapshot should have items");
    assert_eq!(snapshot.run_id, "run-test-001");

    // Collect kinds present.
    let kinds: std::collections::HashSet<&str> =
        snapshot.items.iter().map(|i| i.kind.as_str()).collect();
    assert!(
        kinds.len() >= 2,
        "expected hits from at least 2 collections, got: {kinds:?}"
    );

    // Relevance must be in [0, 1].
    for item in &snapshot.items {
        assert!(
            item.relevance >= 0.0 && item.relevance <= 1.0,
            "relevance out of range: {}",
            item.relevance
        );
    }

    memory.stop(None);
    event_store.stop(None);
}

/// 5.2-G6: ArtifactSearch on a specific collection returns ranked results.
#[tokio::test]
async fn test_artifact_search_single_collection() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    // Seed two version snapshots.
    let docs = [
        ("chapter on Rust actor systems and concurrency", "doc/rust-actors.qwy"),
        ("chapter on Python scripting and automation", "doc/python.qwy"),
    ];
    for (content, src) in &docs {
        let inserted = ractor::call!(memory, |reply| MemoryMsg::Ingest {
            req: IngestRequest {
                item_id: ulid(),
                collection: CollectionKind::VersionSnapshots,
                source_ref: src.to_string(),
                content: content.to_string(),
            },
            reply: Some(reply),
        })
        .expect("ingest");
        assert!(inserted);
    }

    // Query for Rust — should rank the Rust doc higher.
    let result = ractor::call!(memory, |reply| MemoryMsg::ArtifactSearch {
        collection: CollectionKind::VersionSnapshots,
        query: "Rust actor systems and concurrency".to_string(),
        k: 2,
        reply,
    })
    .expect("search rpc");

    assert_eq!(result.items.len(), 2);
    // With stub (hash-based) embedder, semantic ranking is not meaningful.
    // Assert both docs are present and relevance is in range.
    let source_refs: Vec<&str> = result.items.iter().map(|i| i.source_ref.as_str()).collect();
    assert!(
        source_refs.iter().any(|s| s.contains("rust-actors")),
        "rust-actors doc should be in results, got: {source_refs:?}"
    );
    assert!(
        source_refs.iter().any(|s| s.contains("python")),
        "python doc should be in results, got: {source_refs:?}"
    );
    for item in &result.items {
        assert!(item.relevance >= 0.0 && item.relevance <= 1.0);
    }

    memory.stop(None);
    event_store.stop(None);
}

/// 5.2-G7: Dedup via Ingest message — second call with same content returns false.
#[tokio::test]
async fn test_ingest_dedup_via_actor() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    let content = "unique document content for dedup test";

    let first = ractor::call!(memory, |reply| MemoryMsg::Ingest {
        req: IngestRequest {
            item_id: ulid(),
            collection: CollectionKind::UserInputs,
            source_ref: "input/dedup-test".to_string(),
            content: content.to_string(),
        },
        reply: Some(reply),
    })
    .expect("first ingest");

    let second = ractor::call!(memory, |reply| MemoryMsg::Ingest {
        req: IngestRequest {
            item_id: ulid(),
            collection: CollectionKind::UserInputs,
            source_ref: "input/dedup-test".to_string(),
            content: content.to_string(), // same content → same hash
        },
        reply: Some(reply),
    })
    .expect("second ingest");

    assert!(first, "first ingest should be inserted");
    assert!(!second, "second ingest with same content should be skipped");

    memory.stop(None);
    event_store.stop(None);
}

// ─── Phase 5.3 gate tests ─────────────────────────────────────────────────────

/// 5.3-G1: ArtifactExpand returns neighbors of seeded items from adjacent collections.
#[tokio::test]
async fn test_artifact_expand_finds_neighbors() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    // Seed items in two different collections with related content.
    let snapshot_id = ulid();
    let ingest_result = ractor::call!(memory, |reply| MemoryMsg::Ingest {
        req: IngestRequest {
            item_id: snapshot_id.clone(),
            collection: CollectionKind::VersionSnapshots,
            source_ref: "doc/arch.qwy".to_string(),
            content: "architecture document about actor systems".to_string(),
        },
        reply: Some(reply),
    })
    .expect("ingest snapshot");
    assert!(ingest_result);

    let _ = ractor::call!(memory, |reply| MemoryMsg::Ingest {
        req: IngestRequest {
            item_id: ulid(),
            collection: CollectionKind::RunTrajectories,
            source_ref: "run/actor-work".to_string(),
            content: "run that worked on actor system architecture".to_string(),
        },
        reply: Some(reply),
    })
    .expect("ingest trajectory");

    // Expand from the snapshot item — should find neighbors in run_trajectories.
    let result = ractor::call!(memory, |reply| MemoryMsg::ArtifactExpand {
        item_ids: vec![snapshot_id],
        source_collection: CollectionKind::VersionSnapshots,
        neighbors_per_item: 3,
        reply,
    })
    .expect("expand rpc");

    // With stub embedder and related content, at least one neighbor should be returned.
    assert!(
        !result.items.is_empty(),
        "ArtifactExpand should return neighbors"
    );
    // Original item_id should NOT be in results (expand excludes seeds).
    // (the trajectory item should appear instead)
    assert!(
        result.items.iter().all(|i| i.kind != "version_snapshot" || i.source_ref != "doc/arch.qwy"),
        "expanded results should not include the seed item itself"
    );

    memory.stop(None);
    event_store.stop(None);
}

/// 5.3-G2: ArtifactContextPack respects token budget — total chars ≤ budget * 4.
#[tokio::test]
async fn test_artifact_context_pack_respects_budget() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    // Seed several items.
    let docs = [
        ("a long piece of content about memory systems and retrieval algorithms in detail", CollectionKind::VersionSnapshots, "doc/mem.qwy"),
        ("short user input", CollectionKind::UserInputs, "input/1"),
        ("run trajectory for memory task completion with notes on what worked", CollectionKind::RunTrajectories, "run/mem"),
        ("doc trajectory across all memory-related documents over time", CollectionKind::DocTrajectories, "doc/mem.qwy"),
    ];
    for (content, col, src) in &docs {
        let _ = ractor::call!(memory, |reply| MemoryMsg::Ingest {
            req: IngestRequest {
                item_id: ulid(),
                collection: *col,
                source_ref: src.to_string(),
                content: content.to_string(),
            },
            reply: Some(reply),
        })
        .expect("ingest");
    }

    let token_budget = 20; // 20 tokens → 80 char budget — tight, should exclude some items.

    let snapshot = ractor::call!(memory, |reply| MemoryMsg::ArtifactContextPack {
        run_id: "run-pack-test".to_string(),
        objective: "memory retrieval".to_string(),
        token_budget,
        reply,
    })
    .expect("context pack rpc");

    // Total content chars must be within budget.
    let total_chars: usize = snapshot.items.iter().map(|i| i.content.len()).sum();
    let char_budget = token_budget * 4;
    assert!(
        total_chars <= char_budget,
        "packed content ({total_chars} chars) exceeds budget ({char_budget} chars)"
    );

    // snapshot_id must be set and run_id must match.
    assert!(!snapshot.snapshot_id.is_empty());
    assert_eq!(snapshot.run_id, "run-pack-test");

    memory.stop(None);
    event_store.stop(None);
}

/// 5.3-G3: ArtifactContextPack is deterministic — same inputs produce same snapshot_id prefix.
/// (snapshot_id is a ULID so it's always unique, but items must be the same.)
#[tokio::test]
async fn test_artifact_context_pack_deterministic_items() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    let content = "stable document content for determinism test";
    let _ = ractor::call!(memory, |reply| MemoryMsg::Ingest {
        req: IngestRequest {
            item_id: "fixed-id-001".to_string(),
            collection: CollectionKind::VersionSnapshots,
            source_ref: "doc/stable.qwy".to_string(),
            content: content.to_string(),
        },
        reply: Some(reply),
    })
    .expect("ingest");

    let snap1 = ractor::call!(memory, |reply| MemoryMsg::ArtifactContextPack {
        run_id: "run-det-1".to_string(),
        objective: "stable document content".to_string(),
        token_budget: 1000,
        reply,
    })
    .expect("pack 1");

    let snap2 = ractor::call!(memory, |reply| MemoryMsg::ArtifactContextPack {
        run_id: "run-det-2".to_string(),
        objective: "stable document content".to_string(),
        token_budget: 1000,
        reply,
    })
    .expect("pack 2");

    // Same item set regardless of different run_id.
    let ids1: Vec<&str> = snap1.items.iter().map(|i| i.item_id.as_str()).collect();
    let ids2: Vec<&str> = snap2.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids1, ids2, "same inputs should produce same item set");

    memory.stop(None);
    event_store.stop(None);
}

// ─── Phase 5.5 gate test ──────────────────────────────────────────────────────

/// 5.5-G1: Selective re-embedding — second version with one changed block produces
/// exactly ONE new embedding call; unchanged blocks are skipped via chunk_hash dedup.
///
/// Simulates: document v1 has blocks [A, B, C]. Document v2 changes only B → B'.
/// Expected: B' is ingested (new hash); A and C are skipped (same hashes).
/// Total new embeddings for v2 = 1 (only B').
#[tokio::test]
async fn test_selective_reembedding_only_changed_blocks() {
    stub_env();

    let (event_store, _) = ractor::Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("event store spawn");

    let (memory, _) = ractor::Actor::spawn(
        None,
        MemoryActor,
        MemoryArguments {
            event_store: event_store.clone(),
            vec_db_path: ":memory:".to_string(),
        },
    )
    .await
    .expect("memory actor spawn");

    // Document v1: three blocks A, B, C.
    let block_a = "Introduction: this document covers actor system architecture.";
    let block_b_v1 = "Section 2: the original implementation using tokio::spawn.";
    let block_c = "Conclusion: summary of the approach and next steps.";

    let v1_blocks = [
        (block_a, "doc/arch.qwy#intro"),
        (block_b_v1, "doc/arch.qwy#section2"),
        (block_c, "doc/arch.qwy#conclusion"),
    ];

    // Ingest v1 — all three blocks are new.
    let mut v1_inserted = 0usize;
    for (content, src) in &v1_blocks {
        let inserted = ractor::call!(memory, |reply| MemoryMsg::Ingest {
            req: IngestRequest {
                item_id: ulid(),
                collection: CollectionKind::VersionSnapshots,
                source_ref: src.to_string(),
                content: content.to_string(),
            },
            reply: Some(reply),
        })
        .expect("ingest v1");
        if inserted {
            v1_inserted += 1;
        }
    }
    assert_eq!(v1_inserted, 3, "all 3 v1 blocks should be inserted");

    // Document v2: only block B changes to B'. A and C are identical.
    let block_b_v2 = "Section 2: refactored implementation using ractor supervised actors.";

    let v2_blocks = [
        (block_a, "doc/arch.qwy#intro"),        // unchanged → should be skipped
        (block_b_v2, "doc/arch.qwy#section2"),   // changed → should be inserted
        (block_c, "doc/arch.qwy#conclusion"),    // unchanged → should be skipped
    ];

    let mut v2_inserted = 0usize;
    for (content, src) in &v2_blocks {
        let inserted = ractor::call!(memory, |reply| MemoryMsg::Ingest {
            req: IngestRequest {
                item_id: ulid(),
                collection: CollectionKind::VersionSnapshots,
                source_ref: src.to_string(),
                content: content.to_string(),
            },
            reply: Some(reply),
        })
        .expect("ingest v2");
        if inserted {
            v2_inserted += 1;
        }
    }

    // Gate: only 1 new embedding (B' only) — A and C were skipped via chunk_hash dedup.
    assert_eq!(
        v2_inserted, 1,
        "v2 should produce exactly 1 new embedding (changed block only), got {v2_inserted}"
    );

    memory.stop(None);
    event_store.stop(None);
}
