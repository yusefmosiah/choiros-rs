//! EventStoreActor - Append-only event log using ractor
//!
//! This actor provides persistent storage for events using SQLite (libsql).
//! It supports both file-based and in-memory databases.
//!
//! # Architecture
//!
//! - Uses ractor for actor model (converted from Actix)
//! - Uses libsql for SQLite database access
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
use libsql::Connection;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};

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
    conn: Connection,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by EventStoreActor
#[derive(Debug)]
pub enum EventStoreMsg {
    /// Append a new event to the store
    Append {
        event: AppendEvent,
        reply: RpcReplyPort<Result<shared_types::Event, EventStoreError>>,
    },
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
    /// Get a single event by its sequence number
    GetEventBySeq {
        seq: i64,
        reply: RpcReplyPort<Result<Option<shared_types::Event>, EventStoreError>>,
    },
}

impl EventStoreActor {
    async fn new_with_path(database_path: &str) -> Result<Connection, libsql::Error> {
        // Ensure parent directory exists for file-based databases
        if database_path != ":memory:" {
            if let Some(parent) = std::path::Path::new(database_path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }

        let db = libsql::Builder::new_local(database_path).build().await?;
        let conn = db.connect()?;

        // Run migrations manually (libsql doesn't have built-in migration runner)
        Self::run_migrations(&conn).await?;

        Ok(conn)
    }

    async fn run_migrations(conn: &Connection) -> Result<(), libsql::Error> {
        // Create events table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT UNIQUE NOT NULL,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                user_id TEXT NOT NULL DEFAULT 'system'
            )
            "#,
            (),
        )
        .await?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_actor_id ON events(actor_id)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type)",
            (),
        )
        .await?;

        // Add scope columns for safe session/thread isolation.
        let mut table_info = conn.query("PRAGMA table_info(events)", ()).await?;
        let mut has_session_id = false;
        let mut has_thread_id = false;
        while let Some(row) = table_info.next().await? {
            let col_name: String = row.get(1)?;
            if col_name == "session_id" {
                has_session_id = true;
            }
            if col_name == "thread_id" {
                has_thread_id = true;
            }
        }

        if !has_session_id {
            conn.execute("ALTER TABLE events ADD COLUMN session_id TEXT", ())
                .await?;
        }
        if !has_thread_id {
            conn.execute("ALTER TABLE events ADD COLUMN thread_id TEXT", ())
                .await?;
        }

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_session_thread ON events(session_id, thread_id)",
            (),
        )
        .await?;

        // Backfill any existing scoped payload rows into explicit columns.
        conn.execute(
            r#"
            UPDATE events
            SET
                session_id = COALESCE(session_id, json_extract(payload, '$.scope.session_id')),
                thread_id = COALESCE(thread_id, json_extract(payload, '$.scope.thread_id'))
            WHERE session_id IS NULL OR thread_id IS NULL
            "#,
            (),
        )
        .await?;

        Ok(())
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

        let conn = match args {
            EventStoreArguments::File(path) => {
                tracing::info!(database_path = %path, "Opening file-based database");
                Self::new_with_path(&path).await.map_err(|e| {
                    ActorProcessingErr::from(format!("Failed to open database: {e}"))
                })?
            }
            EventStoreArguments::InMemory => {
                tracing::info!("Opening in-memory database");
                Self::new_with_path(":memory:").await.map_err(|e| {
                    ActorProcessingErr::from(format!("Failed to open in-memory database: {e}"))
                })?
            }
        };

        Ok(EventStoreState { conn })
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
            EventStoreMsg::GetEventBySeq { seq, reply } => {
                let result = self.handle_get_event_by_seq(seq, state).await;
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

impl From<libsql::Error> for EventStoreError {
    fn from(e: libsql::Error) -> Self {
        EventStoreError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for EventStoreError {
    fn from(e: serde_json::Error) -> Self {
        EventStoreError::Serialization(e.to_string())
    }
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
        let conn = &state.conn;
        let event_id = ulid::Ulid::new().to_string();
        let payload_json = serde_json::to_string(&msg.payload)?;
        let scope_session_id = msg
            .payload
            .get("scope")
            .and_then(|s| s.get("session_id"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let scope_thread_id = msg
            .payload
            .get("scope")
            .and_then(|s| s.get("thread_id"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);

        // Insert the event (libsql doesn't support RETURNING clause)
        // Clone values for params macro (it takes ownership)
        let actor_id_clone = msg.actor_id.clone();
        let user_id_clone = msg.user_id.clone();
        let event_id_for_query = event_id.clone();
        conn.execute(
            r#"
            INSERT INTO events (event_id, event_type, payload, actor_id, user_id, session_id, thread_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            libsql::params![
                event_id,
                msg.event_type,
                payload_json,
                actor_id_clone,
                user_id_clone,
                scope_session_id,
                scope_thread_id
            ],
        )
        .await?;

        // Retrieve the inserted row
        let mut rows = conn
            .query(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE event_id = ?1
                "#,
                [event_id_for_query.as_str()],
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or(EventStoreError::EventNotFound(0))?;

        // Parse SQLite datetime format: "2026-01-31 02:24:30"
        let timestamp_str: String = row.get(2)?;
        let naive_dt = chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

        let event = shared_types::Event {
            seq: row.get(0)?,
            event_id: row.get(1)?,
            timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
            event_type: row.get(3)?,
            payload: serde_json::from_str(&row.get::<String>(4)?)?,
            actor_id: shared_types::ActorId(row.get(5)?),
            user_id: row.get(6)?,
        };

        Ok(event)
    }

    async fn handle_get_events_for_actor(
        &self,
        actor_id: String,
        since_seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        let conn = &state.conn;

        let mut rows = conn
            .query(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE actor_id = ?1 AND seq > ?2
                ORDER BY seq ASC
                "#,
                libsql::params![actor_id, since_seq],
            )
            .await?;

        let mut events = Vec::new();
        while let Some(row) = rows.next().await? {
            // Parse SQLite datetime format: "2026-01-31 02:24:30"
            let timestamp_str: String = row.get(2)?;
            let naive_dt =
                chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

            let event = shared_types::Event {
                seq: row.get(0)?,
                event_id: row.get(1)?,
                timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
                event_type: row.get(3)?,
                payload: serde_json::from_str(&row.get::<String>(4)?)?,
                actor_id: shared_types::ActorId(row.get(5)?),
                user_id: row.get(6)?,
            };
            events.push(event);
        }

        Ok(events)
    }

    async fn handle_get_events_for_actor_with_scope(
        &self,
        actor_id: String,
        session_id: String,
        thread_id: String,
        since_seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        let conn = &state.conn;

        let mut rows = conn
            .query(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
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
                libsql::params![actor_id, since_seq, session_id, thread_id],
            )
            .await?;

        let mut events = Vec::new();
        while let Some(row) = rows.next().await? {
            let timestamp_str: String = row.get(2)?;
            let naive_dt =
                chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

            let event = shared_types::Event {
                seq: row.get(0)?,
                event_id: row.get(1)?,
                timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
                event_type: row.get(3)?,
                payload: serde_json::from_str(&row.get::<String>(4)?)?,
                actor_id: shared_types::ActorId(row.get(5)?),
                user_id: row.get(6)?,
            };
            events.push(event);
        }

        Ok(events)
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
        let conn = &state.conn;
        let safe_limit = limit.clamp(1, 1000);

        let mut rows = conn
            .query(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE seq > ?1
                  AND (?2 IS NULL OR event_type LIKE (?2 || '%'))
                  AND (?3 IS NULL OR actor_id = ?3)
                  AND (?4 IS NULL OR user_id = ?4)
                ORDER BY seq ASC
                LIMIT ?5
                "#,
                libsql::params![since_seq, event_type_prefix, actor_id, user_id, safe_limit],
            )
            .await?;

        let mut events = Vec::new();
        while let Some(row) = rows.next().await? {
            let timestamp_str: String = row.get(2)?;
            let naive_dt =
                chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

            events.push(shared_types::Event {
                seq: row.get(0)?,
                event_id: row.get(1)?,
                timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
                event_type: row.get(3)?,
                payload: serde_json::from_str(&row.get::<String>(4)?)?,
                actor_id: shared_types::ActorId(row.get(5)?),
                user_id: row.get(6)?,
            });
        }

        Ok(events)
    }

    async fn handle_get_event_by_seq(
        &self,
        seq: i64,
        state: &mut EventStoreState,
    ) -> Result<Option<shared_types::Event>, EventStoreError> {
        let conn = &state.conn;

        let mut rows = conn
            .query(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE seq = ?1
                "#,
                [seq],
            )
            .await?;

        match rows.next().await? {
            Some(row) => {
                // Parse SQLite datetime format: "2026-01-31 02:24:30"
                let timestamp_str: String = row.get(2)?;
                let naive_dt =
                    chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
                        .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

                let event = shared_types::Event {
                    seq: row.get(0)?,
                    event_id: row.get(1)?,
                    timestamp: chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc),
                    event_type: row.get(3)?,
                    payload: serde_json::from_str(&row.get::<String>(4)?)?,
                    actor_id: shared_types::ActorId(row.get(5)?),
                    user_id: row.get(6)?,
                };
                Ok(Some(event))
            }
            None => Ok(None),
        }
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

/// Convenience function to get an event by sequence number
pub async fn get_event_by_seq(
    store: &ActorRef<EventStoreMsg>,
    seq: i64,
) -> Result<Result<Option<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>>
{
    ractor::call!(store, |reply| EventStoreMsg::GetEventBySeq { seq, reply })
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
