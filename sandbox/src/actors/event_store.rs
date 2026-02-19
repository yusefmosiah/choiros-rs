//! EventStoreActor - Append-only event log using ractor
//!
//! This actor provides persistent storage for events using SQLite (sqlx).
//! It supports both file-based and in-memory databases.
//!
//! # Architecture
//!
//! - Uses ractor for actor model
//! - Uses sqlx for SQLite database access with compile-time checked migrations
//! - Supports append-only event log pattern
//! - Events are immutable and ordered by sequence number
//!
//! # Example
//!
//! ```rust,ignore
//! use ractor::{Actor, call};
//!
//! // Spawn with file-based database
//! let (store_ref, _handle) = Actor::spawn(
//!     None,
//!     EventStoreActor,
//!     EventStoreArguments::File("/path/to/events.db".to_string()),
//! ).await?;
//!
//! // Append an event
//! let event = call!(store_ref, |reply| EventStoreMsg::Append {
//!     event: AppendEvent {
//!         event_type: "test.event".to_string(),
//!         payload: json!({"key": "value"}),
//!         actor_id: "actor-1".to_string(),
//!         user_id: "user-1".to_string(),
//!     },
//!     reply,
//! })?;
//! ```

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use sqlx::SqlitePool;

/// Actor that manages the append-only event log
#[derive(Debug, Default)]
pub struct EventStoreActor;

/// Arguments for spawning EventStoreActor
#[derive(Debug, Clone)]
pub enum EventStoreArguments {
    /// File-based database path
    File(String),
    /// In-memory database (for testing)
    InMemory,
}

/// State for EventStoreActor
pub struct EventStoreState {
    pool: SqlitePool,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by EventStoreActor
#[derive(Debug)]
pub enum EventStoreMsg {
    /// Append a new event to the store (with reply)
    Append {
        event: AppendEvent,
        reply: RpcReplyPort<Result<shared_types::Event, EventStoreError>>,
    },
    /// Append a new event to the store (fire-and-forget)
    AppendAsync { event: AppendEvent },
    /// Get all events for an actor since a specific sequence number
    GetEventsForActor {
        actor_id: String,
        since_seq: i64,
        reply: RpcReplyPort<Result<Vec<shared_types::Event>, EventStoreError>>,
    },
    /// Get events for an actor scoped to a session/thread pair.
    GetEventsForActorWithScope {
        actor_id: String,
        session_id: String,
        thread_id: String,
        since_seq: i64,
        reply: RpcReplyPort<Result<Vec<shared_types::Event>, EventStoreError>>,
    },
    /// Get recent events with optional filters for logging/observability views.
    GetRecentEvents {
        since_seq: i64,
        limit: i64,
        event_type_prefix: Option<String>,
        actor_id: Option<String>,
        user_id: Option<String>,
        reply: RpcReplyPort<Result<Vec<shared_types::Event>, EventStoreError>>,
    },
    /// Get the latest sequence number currently present in the event log.
    GetLatestSeq {
        reply: RpcReplyPort<Result<Option<i64>, EventStoreError>>,
    },
    /// Get a single event by its sequence number
    GetEventBySeq {
        seq: i64,
        reply: RpcReplyPort<Result<Option<shared_types::Event>, EventStoreError>>,
    },
    /// Find events by corr_id field inside the payload JSON.
    /// Used by harness recovery to check whether a pending reply has already
    /// arrived (written as `tool.result` or `subharness.result` by the actor
    /// that completed work).
    GetEventsByCorrId {
        corr_id: String,
        event_type_prefix: Option<String>,
        reply: RpcReplyPort<Result<Vec<shared_types::Event>, EventStoreError>>,
    },
    /// Get the latest harness checkpoint event for a given run_id.
    /// Used by harness recovery to reconstruct in-flight state after a crash.
    GetLatestHarnessCheckpoint {
        run_id: String,
        reply: RpcReplyPort<Result<Option<shared_types::Event>, EventStoreError>>,
    },
}

impl EventStoreActor {
    async fn open_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
        use std::str::FromStr;

        let opts = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(opts).await?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| sqlx::Error::Configuration(format!("migration failed: {e}").into()))?;

        Ok(pool)
    }
}

#[async_trait]
impl Actor for EventStoreActor {
    type Msg = EventStoreMsg;
    type State = EventStoreState;
    type Arguments = EventStoreArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "EventStoreActor starting"
        );

        let pool = match args {
            EventStoreArguments::File(path) => {
                tracing::info!(database_path = %path, "Opening file-based database");
                // Ensure parent directory exists
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                Self::open_pool(&format!("sqlite:{path}"))
                    .await
                    .map_err(|e| {
                        ActorProcessingErr::from(format!("Failed to open database: {e}"))
                    })?
            }
            EventStoreArguments::InMemory => {
                tracing::info!("Opening in-memory database");
                Self::open_pool("sqlite::memory:").await.map_err(|e| {
                    ActorProcessingErr::from(format!("Failed to open in-memory database: {e}"))
                })?
            }
        };

        Ok(EventStoreState { pool })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "EventStoreActor started successfully"
        );
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            EventStoreMsg::Append { event, reply } => {
                let result = self.handle_append(event, state).await;
                let _ = reply.send(result);
            }
            EventStoreMsg::AppendAsync { event } => {
                let _ = self.handle_append(event, state).await;
            }
            EventStoreMsg::GetEventsForActor {
                actor_id,
                since_seq,
                reply,
            } => {
                let result = self
                    .handle_get_events_for_actor(actor_id, since_seq, state)
                    .await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetEventsForActorWithScope {
                actor_id,
                session_id,
                thread_id,
                since_seq,
                reply,
            } => {
                let result = self
                    .handle_get_events_for_actor_with_scope(
                        actor_id, session_id, thread_id, since_seq, state,
                    )
                    .await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetRecentEvents {
                since_seq,
                limit,
                event_type_prefix,
                actor_id,
                user_id,
                reply,
            } => {
                let result = self
                    .handle_get_recent_events(
                        since_seq,
                        limit,
                        event_type_prefix,
                        actor_id,
                        user_id,
                        state,
                    )
                    .await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetLatestSeq { reply } => {
                let result = self.handle_get_latest_seq(state).await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetEventBySeq { seq, reply } => {
                let result = self.handle_get_event_by_seq(seq, state).await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetEventsByCorrId {
                corr_id,
                event_type_prefix,
                reply,
            } => {
                let result = self
                    .handle_get_events_by_corr_id(&corr_id, event_type_prefix.as_deref(), state)
                    .await;
                let _ = reply.send(result);
            }
            EventStoreMsg::GetLatestHarnessCheckpoint { run_id, reply } => {
                let result = self
                    .handle_get_latest_harness_checkpoint(&run_id, state)
                    .await;
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
            "EventStoreActor stopped"
        );
        Ok(())
    }
}

// ============================================================================
// Data Types
// ============================================================================

/// Event to append to the store
#[derive(Debug, Clone)]
pub struct AppendEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor_id: String,
    pub user_id: String,
}

impl AppendEvent {
    /// Create a new AppendEvent
    pub fn new(
        event_type: impl Into<String>,
        payload: impl serde::Serialize,
        actor_id: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            event_type: event_type.into(),
            payload: serde_json::to_value(payload)?,
            actor_id: actor_id.into(),
            user_id: user_id.into(),
        })
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur in EventStoreActor
#[derive(Debug, thiserror::Error, Clone)]
pub enum EventStoreError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Event not found: seq={0}")]
    EventNotFound(i64),

    #[error("Invalid timestamp format: {0}")]
    InvalidTimestamp(String),
}

impl From<sqlx::Error> for EventStoreError {
    fn from(e: sqlx::Error) -> Self {
        EventStoreError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for EventStoreError {
    fn from(e: serde_json::Error) -> Self {
        EventStoreError::Serialization(e.to_string())
    }
}

// ============================================================================
// Row mapping helper
// ============================================================================

struct EventRow {
    seq: i64,
    event_id: String,
    timestamp: String,
    event_type: String,
    payload: String,
    actor_id: String,
    user_id: String,
}

fn parse_event_row(row: EventRow) -> Result<shared_types::Event, EventStoreError> {
    // SQLite stores timestamps as TEXT in the format "YYYY-MM-DD HH:MM:SS".
    let naive_dt = chrono::NaiveDateTime::parse_from_str(&row.timestamp, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

    Ok(shared_types::Event {
        seq: row.seq,
        event_id: row.event_id,
        timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
        event_type: row.event_type,
        payload: serde_json::from_str(&row.payload)?,
        actor_id: shared_types::ActorId(row.actor_id),
        user_id: row.user_id,
    })
}

// ============================================================================
// Message Handlers
// ============================================================================

impl EventStoreActor {
    async fn handle_append(
        &self,
        msg: AppendEvent,
        state: &mut EventStoreState,
    ) -> Result<shared_types::Event, EventStoreError> {
        let event_id = ulid::Ulid::new().to_string();
        let payload_json = serde_json::to_string(&msg.payload)?;
        let scope_session_id: Option<String> = msg
            .payload
            .get("scope")
            .and_then(|s| s.get("session_id"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let scope_thread_id: Option<String> = msg
            .payload
            .get("scope")
            .and_then(|s| s.get("thread_id"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);

        let row = sqlx::query_as!(
            EventRow,
            r#"
            INSERT INTO events (event_id, event_type, payload, actor_id, user_id, session_id, thread_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING
                seq as "seq!",
                event_id,
                timestamp,
                event_type,
                payload,
                actor_id,
                user_id
            "#,
            event_id,
            msg.event_type,
            payload_json,
            msg.actor_id,
            msg.user_id,
            scope_session_id,
            scope_thread_id,
        )
        .fetch_one(&state.pool)
        .await?;

        parse_event_row(row)
    }

    async fn handle_get_events_for_actor(
        &self,
        actor_id: String,
        since_seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        let rows = sqlx::query_as!(
            EventRow,
            r#"
            SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE actor_id = ?1 AND seq > ?2
            ORDER BY seq ASC
            "#,
            actor_id,
            since_seq,
        )
        .fetch_all(&state.pool)
        .await?;

        rows.into_iter().map(parse_event_row).collect()
    }

    async fn handle_get_events_for_actor_with_scope(
        &self,
        actor_id: String,
        session_id: String,
        thread_id: String,
        since_seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        let rows = sqlx::query_as!(
            EventRow,
            r#"
            SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE actor_id = ?1
              AND seq > ?2
              AND (
                  (session_id = ?3 AND thread_id = ?4)
                  OR (
                      session_id IS NULL
                      AND thread_id IS NULL
                      AND json_extract(payload, '$.scope.session_id') = ?3
                      AND json_extract(payload, '$.scope.thread_id') = ?4
                  )
              )
            ORDER BY seq ASC
            "#,
            actor_id,
            since_seq,
            session_id,
            thread_id,
        )
        .fetch_all(&state.pool)
        .await?;

        rows.into_iter().map(parse_event_row).collect()
    }

    async fn handle_get_recent_events(
        &self,
        since_seq: i64,
        limit: i64,
        event_type_prefix: Option<String>,
        actor_id: Option<String>,
        user_id: Option<String>,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        let safe_limit = limit.clamp(1, 1000);
        // Build LIKE pattern outside the query.
        let type_pattern: Option<String> = event_type_prefix.map(|p| format!("{p}%"));

        let rows = sqlx::query_as!(
            EventRow,
            r#"
            SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE seq > ?1
              AND (?2 IS NULL OR event_type LIKE ?2)
              AND (?3 IS NULL OR actor_id = ?3)
              AND (?4 IS NULL OR user_id = ?4)
            ORDER BY seq ASC
            LIMIT ?5
            "#,
            since_seq,
            type_pattern,
            actor_id,
            user_id,
            safe_limit,
        )
        .fetch_all(&state.pool)
        .await?;

        rows.into_iter().map(parse_event_row).collect()
    }

    async fn handle_get_event_by_seq(
        &self,
        seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Option<shared_types::Event>, EventStoreError> {
        let maybe_row = sqlx::query_as!(
            EventRow,
            r#"
            SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE seq = ?1
            "#,
            seq,
        )
        .fetch_optional(&state.pool)
        .await?;

        maybe_row.map(parse_event_row).transpose()
    }

    async fn handle_get_latest_seq(
        &self,
        state: &mut EventStoreState,
    ) -> Result<Option<i64>, EventStoreError> {
        let row = sqlx::query!("SELECT MAX(seq) as max_seq FROM events")
            .fetch_one(&state.pool)
            .await?;
        Ok(row.max_seq)
    }

    /// Find all events whose payload contains `"corr_id": "<corr_id>"`.
    /// Optionally filter by event_type prefix.
    /// Used by harness recovery to check whether a pending reply already landed.
    async fn handle_get_events_by_corr_id(
        &self,
        corr_id: &str,
        event_type_prefix: Option<&str>,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        // SQLite JSON1: use json_extract to match corr_id in payload
        let corr_id_pattern = format!("%\"corr_id\":\"{corr_id}\"%");
        let rows = if let Some(prefix) = event_type_prefix {
            let like_prefix = format!("{prefix}%");
            sqlx::query_as!(
                EventRow,
                r#"
                SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE payload LIKE ?1
                  AND event_type LIKE ?2
                ORDER BY seq ASC
                "#,
                corr_id_pattern,
                like_prefix,
            )
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as!(
                EventRow,
                r#"
                SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE payload LIKE ?1
                ORDER BY seq ASC
                "#,
                corr_id_pattern,
            )
            .fetch_all(&state.pool)
            .await?
        };

        rows.into_iter().map(parse_event_row).collect()
    }

    /// Get the most recent `harness.checkpoint` event for a given run_id.
    /// Returns None if no checkpoint exists (run never checkpointed or already
    /// complete and cleaned up).
    async fn handle_get_latest_harness_checkpoint(
        &self,
        run_id: &str,
        state: &mut EventStoreState,
    ) -> Result<Option<shared_types::Event>, EventStoreError> {
        let run_id_pattern = format!("%\"run_id\":\"{run_id}\"%");
        let maybe_row = sqlx::query_as!(
            EventRow,
            r#"
            SELECT seq as "seq!", event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE event_type = 'harness.checkpoint'
              AND payload LIKE ?1
            ORDER BY seq DESC
            LIMIT 1
            "#,
            run_id_pattern,
        )
        .fetch_optional(&state.pool)
        .await?;

        maybe_row.map(parse_event_row).transpose()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convenience function to append an event
pub async fn append_event(
    store: &ActorRef<EventStoreMsg>,
    event: AppendEvent,
) -> Result<Result<shared_types::Event, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::Append { event, reply })
}

/// Convenience function to get events for an actor
pub async fn get_events_for_actor(
    store: &ActorRef<EventStoreMsg>,
    actor_id: impl Into<String>,
    since_seq: i64,
) -> Result<Result<Vec<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::GetEventsForActor {
        actor_id: actor_id.into(),
        since_seq,
        reply,
    })
}

/// Convenience function to get events for an actor scoped by session/thread.
pub async fn get_events_for_actor_with_scope(
    store: &ActorRef<EventStoreMsg>,
    actor_id: impl Into<String>,
    session_id: impl Into<String>,
    thread_id: impl Into<String>,
    since_seq: i64,
) -> Result<Result<Vec<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::GetEventsForActorWithScope {
        actor_id: actor_id.into(),
        session_id: session_id.into(),
        thread_id: thread_id.into(),
        since_seq,
        reply,
    })
}

/// Convenience function to get recent events with optional filters.
pub async fn get_recent_events(
    store: &ActorRef<EventStoreMsg>,
    since_seq: i64,
    limit: i64,
    event_type_prefix: Option<String>,
    actor_id: Option<String>,
    user_id: Option<String>,
) -> Result<Result<Vec<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::GetRecentEvents {
        since_seq,
        limit,
        event_type_prefix,
        actor_id,
        user_id,
        reply,
    })
}

/// Convenience function to get the latest event sequence number.
pub async fn get_latest_seq(
    store: &ActorRef<EventStoreMsg>,
) -> Result<Result<Option<i64>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::GetLatestSeq { reply })
}

/// Convenience function to get an event by sequence number
pub async fn get_event_by_seq(
    store: &ActorRef<EventStoreMsg>,
    seq: i64,
) -> Result<Result<Option<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>>
{
    ractor::call!(store, |reply| EventStoreMsg::GetEventBySeq { seq, reply })
}

/// Find events matching a corr_id in their payload.
/// Optionally filter by event_type prefix (e.g. "tool.result" or "harness.result").
pub async fn get_events_by_corr_id(
    store: &ActorRef<EventStoreMsg>,
    corr_id: impl Into<String>,
    event_type_prefix: Option<String>,
) -> Result<Result<Vec<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> {
    ractor::call!(store, |reply| EventStoreMsg::GetEventsByCorrId {
        corr_id: corr_id.into(),
        event_type_prefix,
        reply,
    })
}

/// Get the latest harness.checkpoint event for a run_id.
/// Returns None if the run has no checkpoint yet (not started or already cleaned up).
pub async fn get_latest_harness_checkpoint(
    store: &ActorRef<EventStoreMsg>,
    run_id: impl Into<String>,
) -> Result<Result<Option<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>>
{
    ractor::call!(store, |reply| EventStoreMsg::GetLatestHarnessCheckpoint {
        run_id: run_id.into(),
        reply,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ractor::Actor;

    #[tokio::test]
    async fn test_append_and_retrieve_event() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Append an event
        let event = append_event(
            &store_ref,
            AppendEvent {
                event_type: "test.event".to_string(),
                payload: serde_json::json!({"foo": "bar"}),
                actor_id: "actor-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert!(event.seq > 0);
        assert_eq!(event.event_type, "test.event");
        assert_eq!(event.actor_id.0, "actor-1");

        // Retrieve events for actor
        let events = get_events_for_actor(&store_ref, "actor-1", 0)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, event.seq);

        // Cleanup
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_get_events_since_seq() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Append multiple events
        for i in 0..5 {
            append_event(
                &store_ref,
                AppendEvent {
                    event_type: "test.event".to_string(),
                    payload: serde_json::json!({"index": i}),
                    actor_id: "actor-1".to_string(),
                    user_id: "user-1".to_string(),
                },
            )
            .await
            .unwrap()
            .unwrap();
        }

        // Get events after seq 2
        let events = get_events_for_actor(&store_ref, "actor-1", 2)
            .await
            .unwrap()
            .unwrap();

        // Should get events with seq > 2
        assert_eq!(events.len(), 3);
        for event in &events {
            assert!(event.seq > 2);
        }

        // Cleanup
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_events_isolated_by_actor() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Events for different actors
        append_event(
            &store_ref,
            AppendEvent {
                event_type: "chat.msg".to_string(),
                payload: serde_json::json!({"text": "hello"}),
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: "file.write".to_string(),
                payload: serde_json::json!({"path": "test.txt"}),
                actor_id: "writer-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        // Get events for chat actor only
        let chat_events = get_events_for_actor(&store_ref, "chat-1", 0)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(chat_events.len(), 1);
        assert_eq!(chat_events[0].event_type, "chat.msg");

        // Cleanup
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_get_events_for_actor_with_scope() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                payload: shared_types::chat_user_payload(
                    "msg-in-scope",
                    Some("session-1".to_string()),
                    Some("thread-1".to_string()),
                ),
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                payload: shared_types::chat_user_payload(
                    "msg-other-thread",
                    Some("session-1".to_string()),
                    Some("thread-2".to_string()),
                ),
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        let scoped =
            get_events_for_actor_with_scope(&store_ref, "chat-1", "session-1", "thread-1", 0)
                .await
                .unwrap()
                .unwrap();

        assert_eq!(scoped.len(), 1);
        assert_eq!(
            shared_types::parse_chat_user_text(&scoped[0].payload).as_deref(),
            Some("msg-in-scope")
        );

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_get_recent_events_with_filters() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({"task_id": "t1"}),
                actor_id: "supervisor-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: "chat.user_msg".to_string(),
                payload: serde_json::json!({"text": "hello"}),
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        append_event(
            &store_ref,
            AppendEvent {
                event_type: "worker.task.completed".to_string(),
                payload: serde_json::json!({"task_id": "t1"}),
                actor_id: "supervisor-1".to_string(),
                user_id: "user-2".to_string(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        let filtered = get_recent_events(
            &store_ref,
            0,
            10,
            Some("worker.task".to_string()),
            Some("supervisor-1".to_string()),
            Some("user-1".to_string()),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_type, "worker.task.started");
        assert_eq!(filtered[0].actor_id.0, "supervisor-1");
        assert_eq!(filtered[0].user_id, "user-1");

        store_ref.stop(None);
    }
}
