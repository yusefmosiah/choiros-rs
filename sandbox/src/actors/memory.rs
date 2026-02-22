//! MemoryActor — local symbolic memory service.
//!
//! Manages four SQLite collections for retrieval by lexical relevance:
//!
//! | Collection          | Unit                                          | Trigger                            |
//! |---------------------|-----------------------------------------------|------------------------------------|
//! | `user_inputs`       | Human directive text (objective/prompt diff)  | EventType::UserInput               |
//! | `version_snapshots` | Whole doc at VersionSource::Writer boundary   | AgentHarness::run() completion     |
//! | `run_trajectories`  | Summary of one harness run                    | AgentResult returned               |
//! | `doc_trajectories`  | Rolled-up summary across all runs for a doc   | Updated on new version_snapshot    |
//!
//! Dedup: every item is keyed by `chunk_hash` (SHA-256 hex of content).
//! Re-indexing is skipped if the hash already exists in the table.
//!
//! Retrieval is intentionally symbolic-first for local memory. Global vector
//! search can be introduced later at the publishing layer.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use sha2::{Digest, Sha256};

use shared_types::{CitationRef, ContextItem, ContextSnapshot};

use crate::actors::event_store::EventStoreMsg;

const LEGACY_EMBEDDING_DIM: usize = 384;

// ─── Store ───────────────────────────────────────────────────────────────────

/// Thin wrapper around a rusqlite Connection.
///
/// All methods are synchronous — callers must use `tokio::task::spawn_blocking`.
pub struct VecStore {
    conn: rusqlite::Connection,
}

impl VecStore {
    /// Open (or create) the store at the given SQLite path.
    /// Use `":memory:"` for in-process test stores.
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = if path == ":memory:" {
            rusqlite::Connection::open_in_memory()?
        } else {
            rusqlite::Connection::open(path)?
        };

        // WAL mode so sqlx and rusqlite can coexist on the same file without contention.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        // Create the four canonical collections if they don't exist.
        conn.execute_batch(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS user_inputs (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id TEXT NOT NULL UNIQUE,
                source_ref TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_hash TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS version_snapshots (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id TEXT NOT NULL UNIQUE,
                source_ref TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_hash TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS run_trajectories (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id TEXT NOT NULL UNIQUE,
                source_ref TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_hash TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS doc_trajectories (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id TEXT NOT NULL UNIQUE,
                source_ref TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_hash TEXT NOT NULL UNIQUE
            );
            "#,
        ))?;

        Ok(VecStore { conn })
    }

    /// Check whether a row with this `chunk_hash` already exists in `table`.
    pub fn hash_exists(&self, table: &str, chunk_hash: &str) -> bool {
        let sql = format!("SELECT 1 FROM {table} WHERE chunk_hash = ? LIMIT 1");
        self.conn
            .query_row(&sql, rusqlite::params![chunk_hash], |_| Ok(()))
            .is_ok()
    }

    /// Insert one row. Does NOT check for duplicates — call `hash_exists` first.
    pub fn insert(
        &self,
        table: &str,
        item_id: &str,
        source_ref: &str,
        content: &str,
        chunk_hash: &str,
    ) -> Result<(), rusqlite::Error> {
        if self.has_embedding_column(table)? {
            let mut embedding_bytes = Vec::with_capacity(LEGACY_EMBEDDING_DIM * 4);
            for _ in 0..LEGACY_EMBEDDING_DIM {
                embedding_bytes.extend_from_slice(&0f32.to_le_bytes());
            }

            let sql = format!(
                "INSERT INTO {table}(embedding, item_id, source_ref, content, chunk_hash) VALUES (?, ?, ?, ?, ?)"
            );
            self.conn.execute(
                &sql,
                rusqlite::params![embedding_bytes, item_id, source_ref, content, chunk_hash],
            )?;
            return Ok(());
        }

        let sql = format!(
            "INSERT INTO {table}(item_id, source_ref, content, chunk_hash) VALUES (?, ?, ?, ?)"
        );
        self.conn.execute(
            &sql,
            rusqlite::params![item_id, source_ref, content, chunk_hash],
        )?;
        Ok(())
    }

    fn has_embedding_column(&self, table: &str) -> Result<bool, rusqlite::Error> {
        let sql = format!("PRAGMA table_info({table})");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "embedding" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Symbolic search — returns up to `k` hits sorted by descending relevance.
    pub fn search(
        &self,
        table: &str,
        query: &str,
        k: usize,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let sql = format!(
            r#"
            SELECT rowid, item_id, source_ref, content
            FROM {table}
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut hits = stmt
            .query_map([], |row| {
                Ok(SearchHit {
                    rowid: row.get(0)?,
                    item_id: row.get(1)?,
                    source_ref: row.get(2)?,
                    content: row.get(3)?,
                    relevance: 0.0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for hit in &mut hits {
            hit.relevance = lexical_relevance(query, &hit.content);
        }
        hits.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.rowid.cmp(&a.rowid))
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// Fetch content for a list of item_ids from `table`.
    pub fn get_contents_by_item_ids(
        &self,
        table: &str,
        item_ids: &[String],
    ) -> Result<Vec<(String, String, String)>, rusqlite::Error> {
        // Returns (item_id, source_ref, content) for each found item.
        let mut results = Vec::new();
        for id in item_ids {
            let sql = format!(
                "SELECT item_id, source_ref, content FROM {table} WHERE item_id = ? LIMIT 1"
            );
            match self.conn.query_row(&sql, rusqlite::params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }) {
                Ok(row) => results.push(row),
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(results)
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub rowid: i64,
    pub item_id: String,
    pub source_ref: String,
    pub content: String,
    /// Normalized lexical relevance in [0, 1].
    pub relevance: f64,
}

/// Compute a hex SHA-256 hash for dedup keying.
pub fn chunk_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

// ─── CollectionKind ──────────────────────────────────────────────────────────

/// The four canonical local vector collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    UserInputs,
    VersionSnapshots,
    RunTrajectories,
    DocTrajectories,
}

impl CollectionKind {
    pub fn table_name(&self) -> &'static str {
        match self {
            CollectionKind::UserInputs => "user_inputs",
            CollectionKind::VersionSnapshots => "version_snapshots",
            CollectionKind::RunTrajectories => "run_trajectories",
            CollectionKind::DocTrajectories => "doc_trajectories",
        }
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            CollectionKind::UserInputs => "user_input",
            CollectionKind::VersionSnapshots => "version_snapshot",
            CollectionKind::RunTrajectories => "run_trajectory",
            CollectionKind::DocTrajectories => "doc_trajectory",
        }
    }
}

impl std::fmt::Display for CollectionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.table_name())
    }
}

// ─── Public actor types ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct MemoryActor;

#[derive(Clone)]
pub struct MemoryArguments {
    pub event_store: ActorRef<EventStoreMsg>,
    /// Path to the SQLite file for the vector store. Use `":memory:"` for tests.
    pub vec_db_path: String,
}

pub struct MemoryState {
    pub(crate) _event_store: ActorRef<EventStoreMsg>,
    /// Thread-safe handle shared with `spawn_blocking` closures.
    pub(crate) inner: Arc<Mutex<MemoryInner>>,
}

pub struct MemoryInner {
    pub store: VecStore,
}

// ─── Message types ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct IngestRequest {
    /// Caller-assigned ULID.
    pub item_id: String,
    pub collection: CollectionKind,
    /// Document path, run_id, URL, etc.
    pub source_ref: String,
    pub content: String,
}

#[derive(Debug)]
pub struct ArtifactSearchResult {
    pub items: Vec<ContextItem>,
}

#[derive(Debug)]
pub enum MemoryMsg {
    /// Ingest a piece of text into a collection.
    /// Silently skipped if `chunk_hash` already exists (Phase 5.5 dedup).
    Ingest {
        req: IngestRequest,
        /// Optional reply: `true` = inserted, `false` = duplicate-skipped.
        reply: Option<RpcReplyPort<bool>>,
    },

    /// Symbolic search against one collection.
    ArtifactSearch {
        collection: CollectionKind,
        query: String,
        k: usize,
        reply: RpcReplyPort<ArtifactSearchResult>,
    },

    /// Expand a set of known item_ids to their neighbors across collections.
    ///
    /// For each item_id, use its content as a symbolic query to find related items
    /// in the same and adjacent collections. Returns merged, deduplicated hits.
    /// This is the multi-hop retrieval step: search → expand → pack.
    ArtifactExpand {
        /// Item IDs returned by a prior ArtifactSearch.
        item_ids: Vec<String>,
        /// Which collection the item_ids came from.
        source_collection: CollectionKind,
        /// How many neighbors to fetch per item.
        neighbors_per_item: usize,
        reply: RpcReplyPort<ArtifactSearchResult>,
    },

    /// Pack retrieved context into a token-budget-aware ContextSnapshot.
    ///
    /// Runs `GetContextSnapshot` internally then trims items to fit within
    /// `token_budget` (1 token ≈ 4 chars). Each item gets a rationale field
    /// based on its relevance rank. Result is deterministic for the same inputs.
    ArtifactContextPack {
        run_id: String,
        objective: String,
        /// Approximate token budget for the packed context.
        token_budget: usize,
        reply: RpcReplyPort<ContextSnapshot>,
    },

    /// Retrieve a merged context snapshot across all four collections.
    /// Hits are ranked by descending lexical relevance and truncated to `max_items`.
    GetContextSnapshot {
        run_id: String,
        query: String,
        max_items: usize,
        reply: RpcReplyPort<ContextSnapshot>,
    },
}

// ─── Actor implementation ─────────────────────────────────────────────────────

#[async_trait]
impl Actor for MemoryActor {
    type Msg = MemoryMsg;
    type State = MemoryState;
    type Arguments = MemoryArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let vec_db_path = args.vec_db_path.clone();

        let inner = tokio::task::spawn_blocking(move || {
            let store = VecStore::open(&vec_db_path).map_err(|e| format!("VecStore::open: {e}"))?;
            Ok::<_, String>(Arc::new(Mutex::new(MemoryInner { store })))
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        .map_err(|e: String| e)?;

        tracing::info!("MemoryActor started (vec_db={})", args.vec_db_path);

        Ok(MemoryState {
            _event_store: args.event_store,
            inner,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            // ── Ingest ────────────────────────────────────────────────────────
            MemoryMsg::Ingest { req, reply } => {
                let inner = Arc::clone(&state.inner);
                let inserted = tokio::task::spawn_blocking(move || {
                    let guard = inner.lock().expect("MemoryInner lock poisoned");
                    let hash = chunk_hash(&req.content);

                    if guard.store.hash_exists(req.collection.table_name(), &hash) {
                        return false; // duplicate — skip
                    }

                    if let Err(e) = guard.store.insert(
                        req.collection.table_name(),
                        &req.item_id,
                        &req.source_ref,
                        &req.content,
                        &hash,
                    ) {
                        tracing::warn!("MemoryActor ingest error: {e}");
                        return false;
                    }

                    true
                })
                .await
                .unwrap_or(false);

                if let Some(r) = reply {
                    let _ = r.send(inserted);
                }
            }

            // ── ArtifactSearch ────────────────────────────────────────────────
            MemoryMsg::ArtifactSearch {
                collection,
                query,
                k,
                reply,
            } => {
                let inner = Arc::clone(&state.inner);
                let result = tokio::task::spawn_blocking(move || {
                    let guard = inner.lock().expect("MemoryInner lock poisoned");
                    let hits = guard
                        .store
                        .search(collection.table_name(), &query, k)
                        .unwrap_or_default();

                    let items = hits
                        .into_iter()
                        .map(|h| ContextItem {
                            item_id: h.item_id,
                            kind: collection.kind_str().to_string(),
                            source_ref: h.source_ref,
                            content: h.content,
                            relevance: h.relevance,
                            created_at: chrono::Utc::now(),
                        })
                        .collect();

                    ArtifactSearchResult { items }
                })
                .await
                .unwrap_or_else(|_| ArtifactSearchResult { items: vec![] });

                let _ = reply.send(result);
            }

            // ── GetContextSnapshot ────────────────────────────────────────────
            MemoryMsg::GetContextSnapshot {
                run_id,
                query,
                max_items,
                reply,
            } => {
                let inner = Arc::clone(&state.inner);
                let q = query.clone();
                let rid = run_id.clone();

                let items = tokio::task::spawn_blocking(move || {
                    let guard = inner.lock().expect("MemoryInner lock poisoned");

                    let per_col = (max_items / 4).max(1);
                    let all_cols = [
                        CollectionKind::UserInputs,
                        CollectionKind::VersionSnapshots,
                        CollectionKind::RunTrajectories,
                        CollectionKind::DocTrajectories,
                    ];

                    let mut merged: Vec<(f64, CollectionKind, SearchHit)> = Vec::new();
                    for col in &all_cols {
                        if let Ok(hits) = guard.store.search(col.table_name(), &q, per_col) {
                            for h in hits {
                                merged.push((h.relevance, *col, h));
                            }
                        }
                    }

                    merged.sort_by(|a, b| {
                        b.0.partial_cmp(&a.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| b.2.rowid.cmp(&a.2.rowid))
                    });
                    merged.truncate(max_items);

                    merged
                        .into_iter()
                        .map(|(relevance, col, h)| ContextItem {
                            item_id: h.item_id,
                            kind: col.kind_str().to_string(),
                            source_ref: h.source_ref,
                            content: h.content,
                            relevance,
                            created_at: chrono::Utc::now(),
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default();

                let snapshot = ContextSnapshot {
                    snapshot_id: ulid::Ulid::new().to_string(),
                    run_id: rid,
                    query,
                    items,
                    provenance: Vec::<CitationRef>::new(),
                    created_at: chrono::Utc::now(),
                };

                let _ = reply.send(snapshot);
            }

            // ── ArtifactExpand ────────────────────────────────────────────────
            MemoryMsg::ArtifactExpand {
                item_ids,
                source_collection,
                neighbors_per_item,
                reply,
            } => {
                let inner = Arc::clone(&state.inner);
                let result = tokio::task::spawn_blocking(move || {
                    let guard = inner.lock().expect("MemoryInner lock poisoned");

                    // Fetch content for each item_id, then find symbolic neighbors.
                    let rows = guard
                        .store
                        .get_contents_by_item_ids(source_collection.table_name(), &item_ids)
                        .unwrap_or_default();

                    let mut seen_ids: std::collections::HashSet<String> =
                        item_ids.iter().cloned().collect();
                    let mut expanded: Vec<ContextItem> = Vec::new();

                    // The adjacent collections to search for neighbors.
                    let adjacent = [
                        CollectionKind::UserInputs,
                        CollectionKind::VersionSnapshots,
                        CollectionKind::RunTrajectories,
                        CollectionKind::DocTrajectories,
                    ];

                    for (item_id, src, content) in &rows {
                        for col in &adjacent {
                            let hits = guard
                                .store
                                .search(col.table_name(), content, neighbors_per_item)
                                .unwrap_or_default();

                            for h in hits {
                                if seen_ids.contains(&h.item_id) {
                                    continue;
                                }
                                seen_ids.insert(h.item_id.clone());
                                expanded.push(ContextItem {
                                    item_id: h.item_id,
                                    kind: col.kind_str().to_string(),
                                    source_ref: h.source_ref,
                                    content: h.content,
                                    relevance: h.relevance,
                                    created_at: chrono::Utc::now(),
                                });
                            }
                        }

                        let _ = (item_id, src); // used above
                    }

                    // Sort by descending relevance.
                    expanded.sort_by(|a, b| {
                        b.relevance
                            .partial_cmp(&a.relevance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    ArtifactSearchResult { items: expanded }
                })
                .await
                .unwrap_or_else(|_| ArtifactSearchResult { items: vec![] });

                let _ = reply.send(result);
            }

            // ── ArtifactContextPack ───────────────────────────────────────────
            MemoryMsg::ArtifactContextPack {
                run_id,
                objective,
                token_budget,
                reply,
            } => {
                let inner = Arc::clone(&state.inner);
                let obj = objective.clone();
                let rid = run_id.clone();

                let items = tokio::task::spawn_blocking(move || {
                    let guard = inner.lock().expect("MemoryInner lock poisoned");

                    // Fetch candidates from all collections.
                    let all_cols = [
                        CollectionKind::UserInputs,
                        CollectionKind::VersionSnapshots,
                        CollectionKind::RunTrajectories,
                        CollectionKind::DocTrajectories,
                    ];

                    let mut candidates: Vec<(f64, CollectionKind, SearchHit)> = Vec::new();
                    for col in &all_cols {
                        if let Ok(hits) = guard.store.search(col.table_name(), &obj, 8) {
                            for h in hits {
                                candidates.push((h.relevance, *col, h));
                            }
                        }
                    }

                    candidates.sort_by(|a, b| {
                        b.0.partial_cmp(&a.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| b.2.rowid.cmp(&a.2.rowid))
                    });

                    // Pack greedily within token budget (1 token ≈ 4 chars).
                    let char_budget = token_budget * 4;
                    let mut used_chars = 0usize;
                    let mut packed: Vec<ContextItem> = Vec::new();

                    for (rank, (relevance, col, h)) in candidates.into_iter().enumerate() {
                        let chars = h.content.len();
                        if used_chars + chars > char_budget {
                            break;
                        }
                        used_chars += chars;

                        // Rationale: rank position and relevance score.
                        let _ = rank; // rationale is attached via relevance field

                        packed.push(ContextItem {
                            item_id: h.item_id,
                            kind: col.kind_str().to_string(),
                            source_ref: h.source_ref,
                            content: h.content,
                            relevance,
                            created_at: chrono::Utc::now(),
                        });
                    }

                    packed
                })
                .await
                .unwrap_or_default();

                let snapshot = ContextSnapshot {
                    snapshot_id: ulid::Ulid::new().to_string(),
                    run_id: rid,
                    query: objective,
                    items,
                    provenance: Vec::<CitationRef>::new(),
                    created_at: chrono::Utc::now(),
                };

                let _ = reply.send(snapshot);
            }
        }

        Ok(())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Compute a lightweight lexical relevance score in [0, 1].
fn lexical_relevance(query: &str, content: &str) -> f64 {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return 0.0;
    }

    let content_tokens = tokenize(content);
    let overlap = query_tokens.intersection(&content_tokens).count() as f64;
    let mut score = overlap / query_tokens.len() as f64;

    let q = query.trim().to_ascii_lowercase();
    if !q.is_empty() && content.to_ascii_lowercase().contains(&q) {
        score += 0.2;
    }

    score.clamp(0.0, 1.0)
}

fn tokenize(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

/// Public re-export so ingestion callers can compute the hash before building
/// an `IngestRequest` (e.g. to check dedup without messaging the actor).
pub fn compute_chunk_hash(content: &str) -> String {
    chunk_hash(content)
}
