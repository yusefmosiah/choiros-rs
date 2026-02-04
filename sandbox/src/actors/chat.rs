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

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::HashMap;

use crate::actors::event_store::EventStoreMsg;

/// Actor that manages chat state as projection of events
pub struct ChatActor;

/// Arguments for spawning ChatActor
pub struct ChatActorArguments {
    pub actor_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
}

/// State for ChatActor
pub struct ChatActorState {
    actor_id: String,
    user_id: String,
    messages: Vec<shared_types::ChatMessage>,
    pending_messages: HashMap<String, shared_types::ChatMessage>,
    last_seq: i64,
    event_store: ActorRef<EventStoreMsg>,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by ChatActor
#[derive(Debug)]
pub enum ChatActorMsg {
    /// Send a user message (triggers event append)
    SendUserMessage {
        text: String,
        reply: RpcReplyPort<Result<String, ChatError>>,
    },
    /// Get current chat messages (projection)
    GetMessages {
        reply: RpcReplyPort<Vec<shared_types::ChatMessage>>,
    },
    /// Sync with event store (new events available)
    SyncEvents {
        events: Vec<shared_types::Event>,
    },
    /// Get actor info
    GetActorInfo {
        reply: RpcReplyPort<(String, String)>,
    },
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error, Clone)]
pub enum ChatError {
    #[allow(dead_code)]
    #[error("Event store error: {0}")]
    EventStore(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

impl From<serde_json::Error> for ChatError {
    fn from(e: serde_json::Error) -> Self {
        ChatError::Serialization(e.to_string())
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

#[async_trait]
impl Actor for ChatActor {
    type Msg = ChatActorMsg;
    type State = ChatActorState;
    type Arguments = ChatActorArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            chat_actor_id = %args.actor_id,
            "ChatActor starting"
        );

        let state = ChatActorState {
            actor_id: args.actor_id,
            user_id: args.user_id,
            messages: Vec::new(),
            pending_messages: HashMap::new(),
            last_seq: 0,
            event_store: args.event_store,
        };

        // Sync with EventStore on startup
        let event_store = state.event_store.clone();
        let actor_id = state.actor_id.clone();
        let last_seq = state.last_seq;

        tokio::spawn(async move {
            match sync_with_event_store(&event_store, &actor_id, last_seq).await {
                Ok(events) => {
                    if let Some(events) = events {
                        let _ = myself.cast(ChatActorMsg::SyncEvents { events });
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to sync with event store: {}", e);
                }
            }
        });

        Ok(state)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ChatActorMsg::SendUserMessage { text, reply } => {
                let result = self.handle_send_user_message(text, state);
                let _ = reply.send(result);
            }
            ChatActorMsg::GetMessages { reply } => {
                let messages = self.handle_get_messages(state);
                let _ = reply.send(messages);
            }
            ChatActorMsg::SyncEvents { events } => {
                self.handle_sync_events(events, state);
            }
            ChatActorMsg::GetActorInfo { reply } => {
                let info = self.handle_get_actor_info(state);
                let _ = reply.send(info);
            }
        }
        Ok(())
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl ChatActor {
    fn handle_send_user_message(
        &self,
        text: String,
        state: &mut ChatActorState,
    ) -> Result<String, ChatError> {
        // Validate message
        if text.trim().is_empty() {
            return Err(ChatError::InvalidMessage(
                "Message cannot be empty".to_string(),
            ));
        }

        // Generate temporary ID for optimistic update
        let temp_id = ulid::Ulid::new().to_string();

        // Create optimistic message
        let pending_msg = shared_types::ChatMessage {
            id: temp_id.clone(),
            text: text.clone(),
            sender: shared_types::Sender::User,
            timestamp: chrono::Utc::now(),
            pending: true,
        };

        // Store in pending
        state.pending_messages.insert(temp_id.clone(), pending_msg);

        // Return the temp ID - caller must append to EventStore separately
        // This decouples ChatActor from EventStoreActor
        Ok(temp_id)
    }

    fn handle_get_messages(&self, state: &ChatActorState) -> Vec<shared_types::ChatMessage> {
        // Combine confirmed messages with pending ones
        let mut result = state.messages.clone();

        // Add pending messages at the end
        for pending in state.pending_messages.values() {
            result.push(pending.clone());
        }

        result
    }

    fn handle_sync_events(&self, events: Vec<shared_types::Event>, state: &mut ChatActorState) {
        for event in events {
            state.last_seq = event.seq;

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
                        state.messages.push(msg);
                        // Remove from pending if it was there
                        state.pending_messages.remove(&event.event_id);
                    }
                }
                shared_types::EVENT_CHAT_ASSISTANT_MSG => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        let text = payload
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let msg = shared_types::ChatMessage {
                            id: event.event_id.clone(),
                            text,
                            sender: shared_types::Sender::Assistant,
                            timestamp: event.timestamp,
                            pending: false,
                        };
                        state.messages.push(msg);
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    }

    fn handle_get_actor_info(&self, state: &ChatActorState) -> (String, String) {
        (state.actor_id.clone(), state.user_id.clone())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Sync with EventStore - load historical events
async fn sync_with_event_store(
    event_store: &ActorRef<EventStoreMsg>,
    actor_id: &str,
    last_seq: i64,
) -> Result<Option<Vec<shared_types::Event>>, ractor::RactorErr<EventStoreMsg>> {
    let result = ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
        actor_id: actor_id.to_string(),
        since_seq: last_seq,
        reply,
    })?;

    match result {
        Ok(events) => Ok(Some(events)),
        Err(_) => Ok(None),
    }
}

/// Convenience function to send a user message
pub async fn send_user_message(
    chat: &ActorRef<ChatActorMsg>,
    text: impl Into<String>,
) -> Result<Result<String, ChatError>, ractor::RactorErr<ChatActorMsg>> {
    ractor::call!(chat, |reply| ChatActorMsg::SendUserMessage {
        text: text.into(),
        reply,
    })
}

/// Convenience function to get messages
pub async fn get_messages(
    chat: &ActorRef<ChatActorMsg>,
) -> Result<Vec<shared_types::ChatMessage>, ractor::RactorErr<ChatActorMsg>> {
    ractor::call!(chat, |reply| ChatActorMsg::GetMessages { reply })
}

/// Convenience function to sync events
pub async fn sync_events(
    chat: &ActorRef<ChatActorMsg>,
    events: Vec<shared_types::Event>,
) -> Result<(), ractor::MessagingErr<ChatActorMsg>> {
    chat.cast(ChatActorMsg::SyncEvents { events })
}

/// Convenience function to get actor info
pub async fn get_actor_info(
    chat: &ActorRef<ChatActorMsg>,
) -> Result<(String, String), ractor::RactorErr<ChatActorMsg>> {
    ractor::call!(chat, |reply| ChatActorMsg::GetActorInfo { reply })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    // ============================================================================
    // Test 1: Basic message sending creates pending message
    // ============================================================================

    #[tokio::test]
    async fn test_send_message_creates_pending() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

        // Send a message
        let temp_id = send_user_message(&chat_ref, "Hello world")
            .await
            .unwrap()
            .unwrap();

        // Verify temp ID was returned
        assert!(!temp_id.is_empty());

        // Get messages - should have 1 pending
        let messages = get_messages(&chat_ref).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "Hello world");
        assert!(messages[0].pending);
        assert!(matches!(messages[0].sender, shared_types::Sender::User));

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 2: Empty messages are rejected
    // ============================================================================

    #[tokio::test]
    async fn test_empty_message_rejected() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

        // Try to send empty message
        let result = send_user_message(&chat_ref, "   ").await;

        // RPC succeeds, but handler returns error
        assert!(result.is_ok()); // RPC OK
        let inner = result.unwrap();
        assert!(inner.is_err()); // Handler returned error
        assert!(inner.unwrap_err().to_string().contains("empty"));

        // Verify no messages
        let messages = get_messages(&chat_ref).await.unwrap();
        assert!(messages.is_empty());

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 3: Event projection converts events to messages
    // ============================================================================

    #[tokio::test]
    async fn test_event_projection_user_message() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

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
        sync_events(&chat_ref, vec![event]).await.unwrap();

        // Get messages
        let messages = get_messages(&chat_ref).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "Hello from event");
        assert!(!messages[0].pending); // Confirmed, not pending

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 4: Event projection handles assistant messages
    // ============================================================================

    #[tokio::test]
    async fn test_event_projection_assistant_message() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

        let event = shared_types::Event {
            seq: 1,
            event_id: "evt_456".to_string(),
            timestamp: chrono::Utc::now(),
            actor_id: shared_types::ActorId("chat-1".to_string()),
            event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
            payload: serde_json::json!({"text": "I am an AI"}),
            user_id: "system".to_string(),
        };

        sync_events(&chat_ref, vec![event]).await.unwrap();

        let messages = get_messages(&chat_ref).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "I am an AI");
        assert!(matches!(
            messages[0].sender,
            shared_types::Sender::Assistant
        ));

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 5: Multiple events are ordered by seq
    // ============================================================================

    #[tokio::test]
    async fn test_multiple_events_ordered() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

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

        sync_events(&chat_ref, events).await.unwrap();

        let messages = get_messages(&chat_ref).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text, "First");
        assert_eq!(messages[1].text, "Second");
        assert_eq!(messages[2].text, "Third");

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 6: Actor info is returned correctly
    // ============================================================================

    #[tokio::test]
    async fn test_actor_info() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "my-chat".to_string(),
                user_id: "my-user".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

        let (actor_id, user_id) = get_actor_info(&chat_ref).await.unwrap();
        assert_eq!(actor_id, "my-chat");
        assert_eq!(user_id, "my-user");

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 7: Pending + confirmed messages combined
    // ============================================================================

    #[tokio::test]
    async fn test_pending_and_confirmed_combined() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

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
        sync_events(&chat_ref, vec![event]).await.unwrap();

        // Send a new pending message
        let _temp_id = send_user_message(&chat_ref, "Pending")
            .await
            .unwrap()
            .unwrap();

        // Get all messages
        let messages = get_messages(&chat_ref).await.unwrap();
        assert_eq!(messages.len(), 2);

        // First is confirmed
        assert_eq!(messages[0].text, "Confirmed");
        assert!(!messages[0].pending);

        // Second is pending
        assert_eq!(messages[1].text, "Pending");
        assert!(messages[1].pending);

        // Cleanup
        chat_ref.stop(None);
    }

    // ============================================================================
    // Test 8: Invalid event payload handled gracefully
    // ============================================================================

    #[tokio::test]
    async fn test_invalid_event_payload_graceful() {
        let (event_store_ref, _event_handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (chat_ref, _chat_handle) = Actor::spawn(
            None,
            ChatActor,
            ChatActorArguments {
                actor_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store_ref,
            },
        )
        .await
        .unwrap();

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
        sync_events(&chat_ref, vec![event]).await.unwrap();

        let messages = get_messages(&chat_ref).await.unwrap();
        assert!(messages.is_empty()); // Invalid event skipped

        // Cleanup
        chat_ref.stop(None);
    }
}
