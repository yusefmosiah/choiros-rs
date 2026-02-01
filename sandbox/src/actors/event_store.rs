use actix::{Actor, Context, Handler, Message, WrapFuture};
use libsql::Connection;

/// Actor that manages the append-only event log
pub struct EventStoreActor {
    conn: Connection,
}

impl EventStoreActor {
    pub async fn new(database_path: &str) -> Result<Self, libsql::Error> {
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

        Ok(Self { conn })
    }

    #[allow(dead_code)]
    pub async fn new_in_memory() -> Result<Self, libsql::Error> {
        Self::new(":memory:").await
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

        Ok(())
    }
}

impl Actor for EventStoreActor {
    type Context = Context<Self>;
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Message)]
#[rtype(result = "Result<shared_types::Event, EventStoreError>")]
pub struct AppendEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor_id: String,
    pub user_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<shared_types::Event>, EventStoreError>")]
pub struct GetEventsForActor {
    pub actor_id: String,
    pub since_seq: i64,
}

#[derive(Message)]
#[rtype(result = "Result<Option<shared_types::Event>, EventStoreError>")]
pub struct GetEventBySeq {
    pub seq: i64,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("Database error: {0}")]
    Database(#[from] libsql::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Event not found: seq={0}")]
    EventNotFound(i64),

    #[error("Invalid timestamp format: {0}")]
    InvalidTimestamp(String),
}

// ============================================================================
// Handlers
// ============================================================================

impl Handler<AppendEvent> for EventStoreActor {
    type Result = actix::ResponseActFuture<Self, Result<shared_types::Event, EventStoreError>>;

    fn handle(&mut self, msg: AppendEvent, _ctx: &mut Context<Self>) -> Self::Result {
        let conn = self.conn.clone();

        Box::pin(
            async move {
                let event_id = ulid::Ulid::new().to_string();
                let payload_json = serde_json::to_string(&msg.payload)?;

                // Insert the event (libsql doesn't support RETURNING clause)
                // Clone values for params macro (it takes ownership)
                let actor_id_clone = msg.actor_id.clone();
                let user_id_clone = msg.user_id.clone();
                let event_id_for_query = event_id.clone();
                conn.execute(
                    r#"
                INSERT INTO events (event_id, event_type, payload, actor_id, user_id)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                    libsql::params![
                        event_id,
                        msg.event_type,
                        payload_json,
                        actor_id_clone,
                        user_id_clone
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
                    .ok_or_else(|| EventStoreError::EventNotFound(0))?;

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

                Ok(event)
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetEventsForActor> for EventStoreActor {
    type Result = actix::ResponseActFuture<Self, Result<Vec<shared_types::Event>, EventStoreError>>;

    fn handle(&mut self, msg: GetEventsForActor, _ctx: &mut Context<Self>) -> Self::Result {
        let conn = self.conn.clone();

        // Clone values before moving into async block
        let actor_id = msg.actor_id.clone();
        let since_seq = msg.since_seq;

        Box::pin(
            async move {
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
                        timestamp: chrono::DateTime::from_naive_utc_and_offset(
                            naive_dt,
                            chrono::Utc,
                        ),
                        event_type: row.get(3)?,
                        payload: serde_json::from_str(&row.get::<String>(4)?)?,
                        actor_id: shared_types::ActorId(row.get(5)?),
                        user_id: row.get(6)?,
                    };
                    events.push(event);
                }

                Ok(events)
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetEventBySeq> for EventStoreActor {
    type Result =
        actix::ResponseActFuture<Self, Result<Option<shared_types::Event>, EventStoreError>>;

    fn handle(&mut self, msg: GetEventBySeq, _ctx: &mut Context<Self>) -> Self::Result {
        let conn = self.conn.clone();

        Box::pin(
            async move {
                let mut rows = conn
                    .query(
                        r#"
                    SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                    FROM events
                    WHERE seq = ?1
                    "#,
                        [msg.seq],
                    )
                    .await?;

                match rows.next().await? {
                    Some(row) => {
                        // Parse SQLite datetime format: "2026-01-31 02:24:30"
                        let timestamp_str: String = row.get(2)?;
                        let naive_dt = chrono::NaiveDateTime::parse_from_str(
                            &timestamp_str,
                            "%Y-%m-%d %H:%M:%S",
                        )
                        .map_err(|e| EventStoreError::InvalidTimestamp(e.to_string()))?;

                        let event = shared_types::Event {
                            seq: row.get(0)?,
                            event_id: row.get(1)?,
                            timestamp: chrono::DateTime::from_naive_utc_and_offset(
                                naive_dt,
                                chrono::Utc,
                            ),
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
            .into_actor(self),
        )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Actor;

    #[actix::test]
    async fn test_append_and_retrieve_event() {
        let store = EventStoreActor::new_in_memory().await.unwrap().start();

        // Append an event
        let event = store
            .send(AppendEvent {
                event_type: "test.event".to_string(),
                payload: serde_json::json!({"foo": "bar"}),
                actor_id: "actor-1".to_string(),
                user_id: "user-1".to_string(),
            })
            .await
            .unwrap()
            .unwrap();

        assert!(event.seq > 0);
        assert_eq!(event.event_type, "test.event");
        assert_eq!(event.actor_id.0, "actor-1");

        // Retrieve events for actor
        let events = store
            .send(GetEventsForActor {
                actor_id: "actor-1".to_string(),
                since_seq: 0,
            })
            .await
            .unwrap()
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, event.seq);
    }

    #[actix::test]
    async fn test_get_events_since_seq() {
        let store = EventStoreActor::new_in_memory().await.unwrap().start();

        // Append multiple events
        for i in 0..5 {
            store
                .send(AppendEvent {
                    event_type: "test.event".to_string(),
                    payload: serde_json::json!({"index": i}),
                    actor_id: "actor-1".to_string(),
                    user_id: "user-1".to_string(),
                })
                .await
                .unwrap()
                .unwrap();
        }

        // Get events after seq 2
        let events = store
            .send(GetEventsForActor {
                actor_id: "actor-1".to_string(),
                since_seq: 2,
            })
            .await
            .unwrap()
            .unwrap();

        // Should get events with seq > 2
        assert_eq!(events.len(), 3);
        for event in &events {
            assert!(event.seq > 2);
        }
    }

    #[actix::test]
    async fn test_events_isolated_by_actor() {
        let store = EventStoreActor::new_in_memory().await.unwrap().start();

        // Events for different actors
        store
            .send(AppendEvent {
                event_type: "chat.msg".to_string(),
                payload: serde_json::json!({"text": "hello"}),
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
            })
            .await
            .unwrap()
            .unwrap();

        store
            .send(AppendEvent {
                event_type: "file.write".to_string(),
                payload: serde_json::json!({"path": "test.txt"}),
                actor_id: "writer-1".to_string(),
                user_id: "user-1".to_string(),
            })
            .await
            .unwrap()
            .unwrap();

        // Get events for chat actor only
        let chat_events = store
            .send(GetEventsForActor {
                actor_id: "chat-1".to_string(),
                since_seq: 0,
            })
            .await
            .unwrap()
            .unwrap();

        assert_eq!(chat_events.len(), 1);
        assert_eq!(chat_events[0].event_type, "chat.msg");
    }
}
