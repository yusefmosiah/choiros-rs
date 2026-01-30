use actix::{Actor, Context, Handler, Message, WrapFuture};
use sqlx::SqlitePool;

/// Actor that manages the append-only event log
pub struct EventStoreActor {
    pool: SqlitePool,
}

impl EventStoreActor {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePool::connect(database_url).await?;
        
        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;
        
        Ok(Self { pool })
    }
    
    pub async fn new_in_memory() -> Result<Self, sqlx::Error> {
        Self::new("sqlite::memory:").await
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
    Database(#[from] sqlx::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Event not found: seq={0}")]
    EventNotFound(i64),
}

// ============================================================================
// Database Row Model
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct EventRow {
    seq: i64,
    event_id: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    event_type: String,
    payload: String, // JSON stored as string
    actor_id: String,
    user_id: String,
}

impl EventRow {
    fn to_shared_event(&self) -> Result<shared_types::Event, serde_json::Error> {
        let payload = serde_json::from_str(&self.payload)?;
        Ok(shared_types::Event {
            seq: self.seq,
            event_id: self.event_id.clone(),
            timestamp: self.timestamp,
            event_type: self.event_type.clone(),
            payload,
            actor_id: shared_types::ActorId(self.actor_id.clone()),
            user_id: self.user_id.clone(),
        })
    }
}

// ============================================================================
// Handlers
// ============================================================================

impl Handler<AppendEvent> for EventStoreActor {
    type Result = actix::ResponseActFuture<Self, Result<shared_types::Event, EventStoreError>>;
    
    fn handle(&mut self, msg: AppendEvent, _ctx: &mut Context<Self>) -> Self::Result {
        let pool = self.pool.clone();
        
        Box::pin(async move {
            let event_id = ulid::Ulid::new().to_string();
            let payload_json = serde_json::to_string(&msg.payload)?;
            
            let row = sqlx::query_as::<_, EventRow>(
                r#"
                INSERT INTO events (event_id, event_type, payload, actor_id, user_id)
                VALUES (?1, ?2, ?3, ?4, ?5)
                RETURNING seq, event_id, timestamp, event_type, payload, actor_id, user_id
                "#
            )
            .bind(&event_id)
            .bind(&msg.event_type)
            .bind(&payload_json)
            .bind(&msg.actor_id)
            .bind(&msg.user_id)
            .fetch_one(&pool)
            .await?;
            
            Ok(row.to_shared_event()?)
        }.into_actor(self))
    }
}

impl Handler<GetEventsForActor> for EventStoreActor {
    type Result = actix::ResponseActFuture<Self, Result<Vec<shared_types::Event>, EventStoreError>>;
    
    fn handle(&mut self, msg: GetEventsForActor, _ctx: &mut Context<Self>) -> Self::Result {
        let pool = self.pool.clone();
        
        Box::pin(async move {
            let rows = sqlx::query_as::<_, EventRow>(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE actor_id = ?1 AND seq > ?2
                ORDER BY seq ASC
                "#
            )
            .bind(&msg.actor_id)
            .bind(msg.since_seq)
            .fetch_all(&pool)
            .await?;
            
            let mut events = Vec::new();
            for row in rows {
                events.push(row.to_shared_event()?);
            }
            
            Ok(events)
        }.into_actor(self))
    }
}

impl Handler<GetEventBySeq> for EventStoreActor {
    type Result = actix::ResponseActFuture<Self, Result<Option<shared_types::Event>, EventStoreError>>;
    
    fn handle(&mut self, msg: GetEventBySeq, _ctx: &mut Context<Self>) -> Self::Result {
        let pool = self.pool.clone();
        
        Box::pin(async move {
            let row = sqlx::query_as::<_, EventRow>(
                r#"
                SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
                FROM events
                WHERE seq = ?1
                "#
            )
            .bind(msg.seq)
            .fetch_optional(&pool)
            .await?;
            
            match row {
                Some(r) => Ok(Some(r.to_shared_event()?)),
                None => Ok(None),
            }
        }.into_actor(self))
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
        let event = store.send(AppendEvent {
            event_type: "test.event".to_string(),
            payload: serde_json::json!({"foo": "bar"}),
            actor_id: "actor-1".to_string(),
            user_id: "user-1".to_string(),
        }).await.unwrap().unwrap();
        
        assert!(event.seq > 0);
        assert_eq!(event.event_type, "test.event");
        assert_eq!(event.actor_id.0, "actor-1");
        
        // Retrieve events for actor
        let events = store.send(GetEventsForActor {
            actor_id: "actor-1".to_string(),
            since_seq: 0,
        }).await.unwrap().unwrap();
        
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, event.seq);
    }
    
    #[actix::test]
    async fn test_get_events_since_seq() {
        let store = EventStoreActor::new_in_memory().await.unwrap().start();
        
        // Append multiple events
        for i in 0..5 {
            store.send(AppendEvent {
                event_type: "test.event".to_string(),
                payload: serde_json::json!({"index": i}),
                actor_id: "actor-1".to_string(),
                user_id: "user-1".to_string(),
            }).await.unwrap().unwrap();
        }
        
        // Get events after seq 2
        let events = store.send(GetEventsForActor {
            actor_id: "actor-1".to_string(),
            since_seq: 2,
        }).await.unwrap().unwrap();
        
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
        store.send(AppendEvent {
            event_type: "chat.msg".to_string(),
            payload: serde_json::json!({"text": "hello"}),
            actor_id: "chat-1".to_string(),
            user_id: "user-1".to_string(),
        }).await.unwrap().unwrap();
        
        store.send(AppendEvent {
            event_type: "file.write".to_string(),
            payload: serde_json::json!({"path": "test.txt"}),
            actor_id: "writer-1".to_string(),
            user_id: "user-1".to_string(),
        }).await.unwrap().unwrap();
        
        // Get events for chat actor only
        let chat_events = store.send(GetEventsForActor {
            actor_id: "chat-1".to_string(),
            since_seq: 0,
        }).await.unwrap().unwrap();
        
        assert_eq!(chat_events.len(), 1);
        assert_eq!(chat_events[0].event_type, "chat.msg");
    }
}