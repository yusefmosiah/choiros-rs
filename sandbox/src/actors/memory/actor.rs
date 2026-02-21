//! MemoryAgent Actor Implementation

use async_trait::async_trait;
use ru::Connection;
use lru::LruCache;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::protocol::{
    MemoryAgentArguments, MemoryAgentError, MemoryAgentMsg, MemoryStats, PatternId,
    PatternMatch, PatternOutcome, SearchConfig, StoredPattern,
};

/// Actor that manages vector-based semantic memory
#[derive(Debug, Default)]
pub struct MemoryAgent;

/// State for MemoryAgent
pub struct MemoryAgentState {
    pub(crate) conn: Connection,
    /// Cache for embeddings to avoid regenerating
    pub(crate) embedding_cache: Arc<Mutex<LruCache<String, Vec<f32>>>>,
    /// Claude Flow MCP endpoint URL
    pub(crate) claude_flow_url: String,
    /// HTTP client for MCP calls
    pub(crate) http_client: reqwest::Client,
    /// Namespace for memory isolation (session_id)
    /// Reserved for future multi-tenant memory isolation
    #[allow(dead_code)]
    pub(crate) namespace: String,
}

#[async_trait]
impl Actor for MemoryAgent {
    type Msg = MemoryAgentMsg;
    type State = MemoryAgentState;
    type Arguments = MemoryAgentArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "MemoryAgent starting"
        );

        let (database_path, claude_flow_url) = match args {
            MemoryAgentArguments::File(path) => (path, "http://localhost:3000".to_string()),
            MemoryAgentArguments::InMemory => {
                (":memory:".to_string(), "http://localhost:3000".to_string())
            }
            MemoryAgentArguments::WithEndpoint {
                database_path,
                claude_flow_url,
            } => (database_path, claude_flow_url),
        };

        // Initialize database
        let conn = Self::init_database(&database_path)
            .await
            .map_err(|e| ActorProcessingErr::from(format!("Failed to initialize database: {e}")))?;

        // Initialize embedding cache (1000 entries)
        let embedding_cache = Arc::new(Mutex::new(LruCache::new(
            NonZeroUsize::new(1000).unwrap(),
        )));

        // Create HTTP client
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                ActorProcessingErr::from(format!("Failed to create HTTP client: {e}"))
            })?;

        // Generate namespace from session
        let namespace = format!("choir-session-{}", ulid::Ulid::new());

        Ok(MemoryAgentState {
            conn,
            embedding_cache,
            claude_flow_url,
            http_client,
            namespace,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            MemoryAgentMsg::StorePattern {
                key,
                description,
                pattern_type,
                metadata,
                success,
                reward,
                reply,
            } => {
                let result = self
                    .handle_store_pattern(
                        key, description, pattern_type, metadata, success, reward, state,
                    )
                    .await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::QueryPatterns {
                query,
                config,
                reply,
            } => {
                let result = self.handle_query_patterns(query, config, state).await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::QueryWithEmbedding {
                embedding,
                config,
                reply,
            } => {
                let result = self
                    .handle_query_with_embedding(embedding, config, state)
                    .await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::GetPattern {
                pattern_id,
                reply,
            } => {
                let result = self.handle_get_pattern(pattern_id, state).await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::UpdateOutcome {
                pattern_id,
                success,
                reward_delta,
                reply,
            } => {
                let result = self
                    .handle_update_outcome(pattern_id, success, reward_delta, state)
                    .await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::CleanupOldPatterns {
                older_than_days,
                max_success_rate,
                reply,
            } => {
                let result = self
                    .handle_cleanup_old_patterns(older_than_days, max_success_rate, state)
                    .await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::GetStats { reply } => {
                let result = self.handle_get_stats(state).await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::GenerateEmbedding { text, reply } => {
                let result = self.generate_embedding(&text, state).await;
                let _ = reply.send(result);
            }

            MemoryAgentMsg::StoreEventPattern {
                event,
                outcome,
                reply,
            } => {
                let result = self.handle_store_event_pattern(event, outcome, state).await;
                let _ = reply.send(result);
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "MemoryAgent stopped"
        );
        Ok(())
    }
}

// ============================================================================
// Helper Methods
// ============================================================================

impl MemoryAgent {
    async fn init_database(database_path: &str) -> Result<Connection, MemoryAgentError> {
        // Ensure parent directory exists
        if database_path != ":memory:" {
            if let Some(parent) = std::path::Path::new(database_path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }

        let db = libsql::Builder::new_local(database_path).build().await?;
        let conn = db.connect()?;

        // Initialize sqlite-vec extension
        conn.execute("SELECT load_extension('sqlite_vec')", ())
            .await
            .ok();

        // Create patterns table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS patterns (
                pattern_id TEXT PRIMARY KEY,
                key TEXT UNIQUE NOT NULL,
                description TEXT NOT NULL,
                pattern_type TEXT NOT NULL,
                metadata TEXT NOT NULL,
                success_count INTEGER DEFAULT 0,
                failure_count INTEGER DEFAULT 0,
                reward REAL DEFAULT 0.5,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
            (),
        )
        .await?;

        // Create virtual table for vector search using sqlite-vec
        conn.execute(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS pattern_embeddings USING vec0(
                pattern_id TEXT PRIMARY KEY,
                embedding FLOAT[384]
            )
            "#,
            (),
        )
        .await?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_patterns_type ON patterns(pattern_type)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_patterns_key ON patterns(key)",
            (),
        )
        .await?;

        Ok(conn)
    }

    /// Generate embedding using Claude Flow MCP endpoint
    pub(crate) async fn generate_embedding(
        &self,
        text: &str,
        state: &MemoryAgentState,
    ) -> Result<Vec<f32>, MemoryAgentError> {
        // Check cache first
        {
            let mut cache = state.embedding_cache.lock().await;
            if let Some(embedding) = cache.get(text) {
                return Ok(embedding.clone());
            }
        }

        // Call Claude Flow MCP embeddings_generate endpoint
        let url = format!(
            "{}/mcp/claude-flow/embeddings_generate",
            state.claude_flow_url
        );

        let request_body = serde_json::json!({
            "text": text,
            "normalize": true,
        });

        let response = state
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(MemoryAgentError::EmbeddingGeneration(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let result: serde_json::Value = response.json().await?;
        let embedding: Vec<f32> = result
            .get("embedding")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| {
                MemoryAgentError::EmbeddingGeneration("Invalid response format".to_string())
            })?;

        // Cache the result
        {
            let mut cache = state.embedding_cache.lock().await;
            cache.put(text.to_string(), embedding.clone());
        }

        Ok(embedding)
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl MemoryAgent {
    async fn handle_store_pattern(
        &self,
        key: String,
        description: String,
        pattern_type: String,
        metadata: serde_json::Value,
        success: bool,
        reward: Option<f32>,
        state: &mut MemoryAgentState,
    ) -> Result<PatternId, MemoryAgentError> {
        let pattern_id = PatternId::new();
        let embedding = self.generate_embedding(&description, state).await?;

        let metadata_json = serde_json::to_string(&metadata)?;
        let reward_value = reward.unwrap_or(if success { 0.8 } else { 0.2 });

        // Insert pattern
        state
            .conn
            .execute(
                r#"
            INSERT INTO patterns (pattern_id, key, description, pattern_type, metadata, success_count, failure_count, reward)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(key) DO UPDATE SET
                description = excluded.description,
                metadata = excluded.metadata,
                success_count = patterns.success_count + excluded.success_count,
                failure_count = patterns.failure_count + excluded.failure_count,
                reward = (patterns.reward + excluded.reward) / 2.0,
                updated_at = datetime('now')
            "#,
                libsql::params![
                    pattern_id.0.clone(),
                    key,
                    description,
                    pattern_type,
                    metadata_json,
                    if success { 1 } else { 0 },
                    if success { 0 } else { 1 },
                    reward_value
                ],
            )
            .await?;

        // Insert embedding - format as JSON array for sqlite-vec
        let embedding_json = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        state
            .conn
            .execute(
                r#"
            INSERT INTO pattern_embeddings (pattern_id, embedding)
            VALUES (?1, vec_f32(?2))
            ON CONFLICT(pattern_id) DO UPDATE SET
                embedding = excluded.embedding
            "#,
                libsql::params![pattern_id.0.clone(), embedding_json],
            )
            .await?;

        Ok(pattern_id)
    }

    async fn handle_query_patterns(
        &self,
        query: String,
        config: SearchConfig,
        state: &mut MemoryAgentState,
    ) -> Result<Vec<PatternMatch>, MemoryAgentError> {
        let embedding = self.generate_embedding(&query, state).await?;
        self.handle_query_with_embedding(embedding, config, state)
            .await
    }

    async fn handle_query_with_embedding(
        &self,
        embedding: Vec<f32>,
        config: SearchConfig,
        state: &mut MemoryAgentState,
    ) -> Result<Vec<PatternMatch>, MemoryAgentError> {
        // Format embedding as JSON array for sqlite-vec
        let embedding_json = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

        // Query using sqlite-vec KNN search
        // sqlite-vec uses vec_distance_L2() for similarity search
        let mut rows = state
            .conn
            .query(
                r#"
            SELECT
                p.pattern_id,
                p.key,
                p.description,
                p.pattern_type,
                p.metadata,
                p.success_count,
                p.failure_count,
                p.reward,
                p.created_at,
                vec_distance_L2(e.embedding, vec_f32(?1)) as distance
            FROM pattern_embeddings e
            JOIN patterns p ON e.pattern_id = p.pattern_id
            WHERE (?2 IS NULL OR p.pattern_type = ?2)
            ORDER BY distance
            LIMIT ?3
            "#,
                libsql::params![embedding_json, config.pattern_type, config.limit as i64],
            )
            .await?;

        let mut matches = Vec::new();
        while let Some(row) = rows.next().await? {
            let pattern_id: String = row.get(0)?;
            let key: String = row.get(1)?;
            let description: String = row.get(2)?;
            let pattern_type: String = row.get(3)?;
            let metadata_str: String = row.get(4)?;
            let success_count: i64 = row.get(5)?;
            let failure_count: i64 = row.get(6)?;
            let reward: f64 = row.get(7)?;
            let created_at_str: String = row.get(8)?;
            let distance: f64 = row.get(9)?;

            let total = success_count + failure_count;
            let success_rate = if total > 0 {
                success_count as f32 / total as f32
            } else {
                0.5
            };

            // Convert distance to similarity (sqlite-vec returns L2 distance)
            // Similarity = 1 / (1 + distance) for L2 distance
            let similarity = 1.0 / (1.0 + distance as f32);

            if similarity < config.threshold {
                continue;
            }

            let metadata: serde_json::Value = serde_json::from_str(&metadata_str)?;
            let created_at = chrono::NaiveDateTime::parse_from_str(
                &created_at_str,
                "%Y-%m-%d %H:%M:%S",
            )
            .map_err(|e| MemoryAgentError::Database(e.to_string()))?;

            matches.push(PatternMatch {
                pattern_id: PatternId(pattern_id),
                key,
                description,
                pattern_type,
                metadata,
                success_rate,
                reward: reward as f32,
                created_at: chrono::DateTime::from_naive_utc_and_offset(created_at, chrono::Utc),
                similarity,
            });
        }

        Ok(matches)
    }

    async fn handle_get_pattern(
        &self,
        pattern_id: PatternId,
        state: &mut MemoryAgentState,
    ) -> Result<Option<StoredPattern>, MemoryAgentError> {
        let mut rows = state
            .conn
            .query(
                r#"
            SELECT
                pattern_id, key, description, pattern_type, metadata,
                success_count, failure_count, reward, created_at, updated_at
            FROM patterns
            WHERE pattern_id = ?1
            "#,
                [pattern_id.0],
            )
            .await?;

        match rows.next().await? {
            Some(row) => {
                let metadata_str: String = row.get(4)?;
                let created_at_str: String = row.get(8)?;
                let updated_at_str: String = row.get(9)?;

                Ok(Some(StoredPattern {
                    pattern_id: PatternId(row.get(0)?),
                    key: row.get(1)?,
                    description: row.get(2)?,
                    pattern_type: row.get(3)?,
                    metadata: serde_json::from_str(&metadata_str)?,
                    success_count: row.get(5)?,
                    failure_count: row.get(6)?,
                    reward: row.get::<f64>(7)? as f32,
                    created_at: chrono::DateTime::from_naive_utc_and_offset(
                        chrono::NaiveDateTime::parse_from_str(
                            &created_at_str,
                            "%Y-%m-%d %H:%M:%S",
                        )
                        .map_err(|e| MemoryAgentError::Database(e.to_string()))?,
                        chrono::Utc,
                    ),
                    updated_at: chrono::DateTime::from_naive_utc_and_offset(
                        chrono::NaiveDateTime::parse_from_str(
                            &updated_at_str,
                            "%Y-%m-%d %H:%M:%S",
                        )
                        .map_err(|e| MemoryAgentError::Database(e.to_string()))?,
                        chrono::Utc,
                    ),
                }))
            }
            None => Ok(None),
        }
    }

    async fn handle_update_outcome(
        &self,
        pattern_id: PatternId,
        success: bool,
        reward_delta: f32,
        state: &mut MemoryAgentState,
    ) -> Result<(), MemoryAgentError> {
        state
            .conn
            .execute(
                r#"
            UPDATE patterns
            SET
                success_count = success_count + ?1,
                failure_count = failure_count + ?2,
                reward = MIN(1.0, MAX(0.0, reward + ?3)),
                updated_at = datetime('now')
            WHERE pattern_id = ?4
            "#,
                libsql::params![
                    if success { 1 } else { 0 },
                    if success { 0 } else { 1 },
                    reward_delta,
                    pattern_id.0
                ],
            )
            .await?;

        Ok(())
    }

    async fn handle_cleanup_old_patterns(
        &self,
        older_than_days: i64,
        max_success_rate: f32,
        state: &mut MemoryAgentState,
    ) -> Result<usize, MemoryAgentError> {
        // First get the count
        let mut count_rows = state
            .conn
            .query(
                r#"
            SELECT COUNT(*) FROM patterns
            WHERE created_at < datetime('now', ?1 || ' days')
            AND (CAST(success_count AS FLOAT) / NULLIF(success_count + failure_count, 0)) < ?2
            "#,
                libsql::params![-older_than_days, max_success_rate],
            )
            .await?;

        let count: i64 = if let Some(row) = count_rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        // Delete old patterns
        state
            .conn
            .execute(
                r#"
            DELETE FROM pattern_embeddings
            WHERE pattern_id IN (
                SELECT pattern_id FROM patterns
                WHERE created_at < datetime('now', ?1 || ' days')
                AND (CAST(success_count AS FLOAT) / NULLIF(success_count + failure_count, 0)) < ?2
            )
            "#,
                libsql::params![-older_than_days, max_success_rate],
            )
            .await?;

        state
            .conn
            .execute(
                r#"
            DELETE FROM patterns
            WHERE created_at < datetime('now', ?1 || ' days')
            AND (CAST(success_count AS FLOAT) / NULLIF(success_count + failure_count, 0)) < ?2
            "#,
                libsql::params![-older_than_days, max_success_rate],
            )
            .await?;

        Ok(count as usize)
    }

    async fn handle_get_stats(
        &self,
        state: &mut MemoryAgentState,
    ) -> Result<MemoryStats, MemoryAgentError> {
        // Total patterns
        let mut total_rows = state
            .conn
            .query("SELECT COUNT(*) FROM patterns", ())
            .await?;
        let total_patterns: i64 = if let Some(row) = total_rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        // Patterns by type
        let mut type_rows = state
            .conn
            .query(
                "SELECT pattern_type, COUNT(*) FROM patterns GROUP BY pattern_type",
                (),
            )
            .await?;

        let mut patterns_by_type = Vec::new();
        while let Some(row) = type_rows.next().await? {
            patterns_by_type.push((row.get(0)?, row.get(1)?));
        }

        // Average success rate
        let mut avg_rows = state
            .conn
            .query(
                r#"
            SELECT AVG(CAST(success_count AS FLOAT) / NULLIF(success_count + failure_count, 0))
            FROM patterns
            WHERE success_count + failure_count > 0
            "#,
                (),
            )
            .await?;

        let avg_success_rate: f64 = if let Some(row) = avg_rows.next().await? {
            row.get(0).unwrap_or(0.5)
        } else {
            0.5
        };

        // Total embeddings
        let mut embed_rows = state
            .conn
            .query("SELECT COUNT(*) FROM pattern_embeddings", ())
            .await?;
        let total_embeddings: i64 = if let Some(row) = embed_rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        Ok(MemoryStats {
            total_patterns,
            patterns_by_type,
            avg_success_rate: avg_success_rate as f32,
            total_embeddings,
        })
    }

    async fn handle_store_event_pattern(
        &self,
        event: crate::actors::event_store::AppendEvent,
        outcome: PatternOutcome,
        state: &mut MemoryAgentState,
    ) -> Result<PatternId, MemoryAgentError> {
        // Extract description from event payload
        let description = format!("{}: {}", event.event_type, event.payload);

        // Determine pattern type from event type
        let pattern_type = if event.event_type.contains("terminal") {
            "terminal_command"
        } else if event.event_type.contains("conductor") {
            "conductor_task"
        } else if event.event_type.contains("writer") {
            "writer_operation"
        } else {
            "generic_event"
        }
        .to_string();

        // Build metadata
        let metadata = serde_json::json!({
            "event_type": event.event_type,
            "actor_id": event.actor_id,
            "user_id": event.user_id,
            "payload": event.payload,
            "outcome": outcome,
        });

        let key = format!("{}-{}", event.event_type, event.actor_id);

        self.handle_store_pattern(
            key,
            description,
            pattern_type,
            metadata,
            outcome.success,
            Some(if outcome.success { 0.9 } else { 0.1 }),
            state,
        )
        .await
    }
}
