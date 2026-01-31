//! ChatActor - manages chat conversations as event projections
//!
//! PREDICTION: Events from EventStoreActor can be projected into ChatMessage
//! structures, enabling the UI to display conversation history without direct
//! database access. Optimistic updates provide immediate feedback.
//!
//! EXPERIMENT:
//! 1. ChatActor subscribes to EventStoreActor for its actor_id
//! 2. User sends message → append event immediately (optimistic)
//! 3. EventStore confirms → ChatActor updates projection
//! 4. UI polls/query for ChatMessage list
//!
//! OBSERVE:
//! - Pending messages appear immediately
//! - All events have seq numbers for ordering
//! - Actor isolation prevents cross-contamination
//! - No direct DB access from ChatActor (only via EventStoreActor)

use actix::{Actor, ActorFutureExt, AsyncContext, Context, Handler, Message, Supervised, WrapFuture, Addr};
use std::collections::HashMap;

use crate::actors::event_store::{EventStoreActor, GetEventsForActor};

/// Actor that manages chat state as projection of events
pub struct ChatActor {
    actor_id: String,
    user_id: String,
    messages: Vec<shared_types::ChatMessage>,
    pending_messages: HashMap<String, shared_types::ChatMessage>,
    last_seq: i64,
    event_store: Option<Addr<EventStoreActor>>,
}

impl ChatActor {
    pub fn new(actor_id: String, user_id: String, event_store: Addr<EventStoreActor>) -> Self {
        Self {
            actor_id,
            user_id,
            messages: Vec::new(),
            pending_messages: HashMap::new(),
            last_seq: 0,
            event_store: Some(event_store),
        }
    }

    /// Project events to chat messages
    fn project_events(&mut self, events: Vec<shared_types::Event>) {
        for event in events {
            self.last_seq = event.seq;
            
            match event.event_type.as_str() {
                shared_types::EVENT_CHAT_USER_MSG => {
                    if let Ok(text) = serde_json::from_value::<String>(event.payload.clone()) {
                        let msg = shared_types::ChatMessage {
                            id: event.event_id.clone(),
                            text,
                            sender: shared_types::Sender::User,
                            timestamp: event.timestamp,
                            pending: false,
                        };
                        self.messages.push(msg);
                        // Remove from pending if it was there
                        self.pending_messages.remove(&event.event_id);
                    }
                }
                shared_types::EVENT_CHAT_ASSISTANT_MSG => {
                    if let Ok(payload) = serde_json::from_value::<serde_json::Value>(event.payload.clone()) {
                        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let msg = shared_types::ChatMessage {
                            id: event.event_id.clone(),
                            text,
                            sender: shared_types::Sender::Assistant,
                            timestamp: event.timestamp,
                            pending: false,
                        };
                        self.messages.push(msg);
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    }

    /// Sync with EventStore - load historical events
    fn sync_with_event_store(&mut self, ctx: &mut Context<Self>) {
        if let Some(event_store) = self.event_store.clone() {
            let actor_id = self.actor_id.clone();
            let last_seq = self.last_seq;
            
            let fut = async move {
                let result: Result<Result<Vec<shared_types::Event>, crate::actors::event_store::EventStoreError>, actix::MailboxError> = event_store.send(GetEventsForActor {
                    actor_id,
                    since_seq: last_seq,
                }).await;
                
                match result {
                    Ok(Ok(events)) => Some(events),
                    _ => None,
                }
            };
            
            ctx.spawn(fut.into_actor(self).map(|events: Option<Vec<shared_types::Event>>, actor: &mut ChatActor, _| {
                if let Some(events) = events {
                    actor.project_events(events);
                }
            }));
        }
    }
}

impl Actor for ChatActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        // Sync with EventStore on startup
        self.sync_with_event_store(ctx);
    }
}

// Implement Supervised for fault tolerance
impl Supervised for ChatActor {
    fn restarting(&mut self, ctx: &mut Context<Self>) {
        // Clear in-memory state but keep identity
        self.messages.clear();
        self.pending_messages.clear();
        self.last_seq = 0;
        
        // Re-sync with EventStore
        self.sync_with_event_store(ctx);
    }
}

// ============================================================================
// Messages
// ============================================================================

/// Send a user message (triggers event append)
#[derive(Message)]
#[rtype(result = "Result<String, ChatError>")]
pub struct SendUserMessage {
    pub text: String,
}

/// Get current chat messages (projection)
#[derive(Message)]
#[rtype(result = "Vec<shared_types::ChatMessage>")]
pub struct GetMessages;

/// Sync with event store (new events available)
#[derive(Message)]
#[rtype(result = "()")]
pub struct SyncEvents {
    pub events: Vec<shared_types::Event>,
}

/// Get actor info
#[derive(Message)]
#[rtype(result = "(String, String)")]
pub struct GetActorInfo;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("Event store error: {0}")]
    EventStore(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

// ============================================================================
// Handlers
// ============================================================================

impl Handler<SendUserMessage> for ChatActor {
    type Result = Result<String, ChatError>;
    
    fn handle(&mut self, msg: SendUserMessage, _ctx: &mut Context<Self>) -> Self::Result {
        // Validate message
        if msg.text.trim().is_empty() {
            return Err(ChatError::InvalidMessage("Message cannot be empty".to_string()));
        }
        
        // Generate temporary ID for optimistic update
        let temp_id = ulid::Ulid::new().to_string();
        
        // Create optimistic message
        let pending_msg = shared_types::ChatMessage {
            id: temp_id.clone(),
            text: msg.text.clone(),
            sender: shared_types::Sender::User,
            timestamp: chrono::Utc::now(),
            pending: true,
        };
        
        // Store in pending
        self.pending_messages.insert(temp_id.clone(), pending_msg);
        
        // Return the temp ID - caller must append to EventStore separately
        // This decouples ChatActor from EventStoreActor
        Ok(temp_id)
    }
}

impl Handler<GetMessages> for ChatActor {
    type Result = Vec<shared_types::ChatMessage>;
    
    fn handle(&mut self, _msg: GetMessages, _ctx: &mut Context<Self>) -> Self::Result {
        // Combine confirmed messages with pending ones
        let mut result = self.messages.clone();
        
        // Add pending messages at the end
        for (_, pending) in &self.pending_messages {
            result.push(pending.clone());
        }
        
        result
    }
}

impl Handler<SyncEvents> for ChatActor {
    type Result = ();
    
    fn handle(&mut self, msg: SyncEvents, _ctx: &mut Context<Self>) {
        self.project_events(msg.events);
    }
}

impl Handler<GetActorInfo> for ChatActor {
    type Result = actix::ResponseActFuture<Self, (String, String)>;
    
    fn handle(&mut self, _msg: GetActorInfo, _ctx: &mut Context<Self>) -> Self::Result {
        let actor_id = self.actor_id.clone();
        let user_id = self.user_id.clone();
        Box::pin(async move { (actor_id, user_id) }.into_actor(self))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Actor;
    
    // ============================================================================
    // Test 1: Basic message sending creates pending message
    // ============================================================================
    
    #[actix::test]
    async fn test_send_message_creates_pending() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        // Send a message
        let temp_id = chat.send(SendUserMessage {
            text: "Hello world".to_string(),
        }).await.unwrap().unwrap();
        
        // Verify temp ID was returned
        assert!(!temp_id.is_empty());
        
        // Get messages - should have 1 pending
        let messages = chat.send(GetMessages).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "Hello world");
        assert!(messages[0].pending);
        assert!(matches!(messages[0].sender, shared_types::Sender::User));
    }
    
    // ============================================================================
    // Test 2: Empty messages are rejected
    // ============================================================================
    
    #[actix::test]
    async fn test_empty_message_rejected() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        // Try to send empty message
        let result = chat.send(SendUserMessage {
            text: "   ".to_string(),
        }).await;
        
        // Mailbox succeeds, but handler returns error
        assert!(result.is_ok()); // Mailbox OK
        let inner = result.unwrap();
        assert!(inner.is_err()); // Handler returned error
        assert!(inner.unwrap_err().to_string().contains("empty"));
        
        // Verify no messages
        let messages = chat.send(GetMessages).await.unwrap();
        assert!(messages.is_empty());
    }
    
    // ============================================================================
    // Test 3: Event projection converts events to messages
    // ============================================================================
    
    #[actix::test]
    async fn test_event_projection_user_message() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        // Create a fake event as if from EventStore
        let event = shared_types::Event {
            seq: 1,
            event_id: "evt_123".to_string(),
            timestamp: chrono::Utc::now(),
            actor_id: shared_types::ActorId("chat-1".to_string()),
            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!("Hello from event"),
            user_id: "user-1".to_string(),
        };
        
        // Sync events
        let _ = chat.send(SyncEvents { events: vec![event] }).await;
        
        // Get messages
        let messages = chat.send(GetMessages).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "Hello from event");
        assert!(!messages[0].pending); // Confirmed, not pending
    }
    
    // ============================================================================
    // Test 4: Event projection handles assistant messages
    // ============================================================================
    
    #[actix::test]
    async fn test_event_projection_assistant_message() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        let event = shared_types::Event {
            seq: 1,
            event_id: "evt_456".to_string(),
            timestamp: chrono::Utc::now(),
            actor_id: shared_types::ActorId("chat-1".to_string()),
            event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
            payload: serde_json::json!({"text": "I am an AI"}),
            user_id: "system".to_string(),
        };
        
        let _ = chat.send(SyncEvents { events: vec![event] }).await;
        
        let messages = chat.send(GetMessages).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "I am an AI");
        assert!(matches!(messages[0].sender, shared_types::Sender::Assistant));
    }
    
    // ============================================================================
    // Test 5: Multiple events are ordered by seq
    // ============================================================================
    
    #[actix::test]
    async fn test_multiple_events_ordered() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        let events = vec![
            shared_types::Event {
                seq: 1,
                event_id: "evt_1".to_string(),
                timestamp: chrono::Utc::now(),
                actor_id: shared_types::ActorId("chat-1".to_string()),
                event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                payload: serde_json::json!("First"),
                user_id: "user-1".to_string(),
            },
            shared_types::Event {
                seq: 2,
                event_id: "evt_2".to_string(),
                timestamp: chrono::Utc::now(),
                actor_id: shared_types::ActorId("chat-1".to_string()),
                event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
                payload: serde_json::json!({"text": "Second"}),
                user_id: "system".to_string(),
            },
            shared_types::Event {
                seq: 3,
                event_id: "evt_3".to_string(),
                timestamp: chrono::Utc::now(),
                actor_id: shared_types::ActorId("chat-1".to_string()),
                event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
                payload: serde_json::json!("Third"),
                user_id: "user-1".to_string(),
            },
        ];
        
        let _ = chat.send(SyncEvents { events }).await;
        
        let messages = chat.send(GetMessages).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text, "First");
        assert_eq!(messages[1].text, "Second");
        assert_eq!(messages[2].text, "Third");
    }
    
    // ============================================================================
    // Test 6: Actor info is returned correctly
    // ============================================================================
    
    #[actix::test]
    async fn test_actor_info() {
        let chat = ChatActor::new("my-chat".to_string(), "my-user".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        let (actor_id, user_id) = chat.send(GetActorInfo).await.unwrap();
        assert_eq!(actor_id, "my-chat");
        assert_eq!(user_id, "my-user");
    }
    
    // ============================================================================
    // Test 7: Pending + confirmed messages combined
    // ============================================================================
    
    #[actix::test]
    async fn test_pending_and_confirmed_combined() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        // Add a confirmed message via event
        let event = shared_types::Event {
            seq: 1,
            event_id: "evt_1".to_string(),
            timestamp: chrono::Utc::now(),
            actor_id: shared_types::ActorId("chat-1".to_string()),
            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!("Confirmed"),
            user_id: "user-1".to_string(),
        };
        let _ = chat.send(SyncEvents { events: vec![event] }).await;
        
        // Send a new pending message
        let _temp_id = chat.send(SendUserMessage {
            text: "Pending".to_string(),
        }).await.unwrap().unwrap();
        
        // Get all messages
        let messages = chat.send(GetMessages).await.unwrap();
        assert_eq!(messages.len(), 2);
        
        // First is confirmed
        assert_eq!(messages[0].text, "Confirmed");
        assert!(!messages[0].pending);
        
        // Second is pending
        assert_eq!(messages[1].text, "Pending");
        assert!(messages[1].pending);
    }
    
    // ============================================================================
    // Test 8: Invalid event payload handled gracefully
    // ============================================================================
    
    #[actix::test]
    async fn test_invalid_event_payload_graceful() {
        let chat = ChatActor::new("chat-1".to_string(), "user-1".to_string(), EventStoreActor::new_in_memory().await.unwrap().start()).start();
        
        // Event with wrong payload type
        let event = shared_types::Event {
            seq: 1,
            event_id: "evt_1".to_string(),
            timestamp: chrono::Utc::now(),
            actor_id: shared_types::ActorId("chat-1".to_string()),
            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!({"wrong": "format"}), // Should be string, not object
            user_id: "user-1".to_string(),
        };
        
        // Should not panic, just skip invalid event
        let _ = chat.send(SyncEvents { events: vec![event] }).await;
        
        let messages = chat.send(GetMessages).await.unwrap();
        assert!(messages.is_empty()); // Invalid event skipped
    }
}
