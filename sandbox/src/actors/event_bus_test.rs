//! EventBusActor tests - comprehensive test suite for ractor-based event bus
//!
//! Tests follow the recovery standard: STRICTER testing after system changes.

#[cfg(test)]
mod tests {
    use super::*;
    use ractor::{concurrency::Duration, Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // ============================================================================
    // Test Fixtures and Helpers
    // ============================================================================

    /// Mock EventStoreActor for testing EventBusActor in isolation
    #[derive(Default)]
    struct MockEventStore {
        events: Arc<Mutex<Vec<Event>>>,
    }

    #[derive(Debug, Clone)]
    enum MockEventStoreMsg {
        Append(Event),
        GetEvents { reply: RpcReplyPort<Vec<Event>> },
    }

    impl ractor::Message for MockEventStoreMsg {}

    #[async_trait::async_trait]
    impl Actor for MockEventStore {
        type Msg = MockEventStoreMsg;
        type State = Arc<Mutex<Vec<Event>>>;
        type Arguments = ();

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            _args: (),
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(self.events.clone())
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            match message {
                MockEventStoreMsg::Append(event) => {
                    state.lock().await.push(event);
                }
                MockEventStoreMsg::GetEvents { reply } => {
                    let events = state.lock().await.clone();
                    let _ = reply.send(events);
                }
            }
            Ok(())
        }
    }

    /// Test subscriber actor that collects received events
    struct TestSubscriber {
        received_events: Arc<Mutex<Vec<Event>>>,
    }

    #[async_trait::async_trait]
    impl Actor for TestSubscriber {
        type Msg = Event;
        type State = Arc<Mutex<Vec<Event>>>;
        type Arguments = ();

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            _args: (),
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(self.received_events.clone())
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            state.lock().await.push(message);
            Ok(())
        }
    }

    /// Helper to create a test event
    fn test_event(topic: &str, payload: serde_json::Value) -> Event {
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: EventType::Custom("test".to_string()),
            topic: topic.to_string(),
            payload,
            timestamp: chrono::Utc::now(),
            source: "test".to_string(),
            correlation_id: None,
        }
    }

    // ============================================================================
    // Unit Tests: Core Functionality
    // ============================================================================

    #[tokio::test]
    async fn test_event_bus_starts_successfully() {
        // Given: Mock event store
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .expect("Failed to spawn mock store");

        // When: Spawn EventBusActor
        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .expect("Failed to spawn event bus");

        // Then: Actor should be running
        assert_eq!(bus_ref.get_status(), ractor::ActorStatus::Running);

        // Cleanup
        bus_ref.stop(None);
        let _ = bus_handle.await;
    }

    #[tokio::test]
    async fn test_publish_without_subscribers_does_not_fail() {
        // Given: EventBusActor with no subscribers
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let event = test_event("test.topic", json!({"message": "hello"}));

        // When: Publish event with no subscribers
        let result = ractor::call!(
            bus_ref,
            EventBusMsg::Publish {
                event,
                persist: false,
            },
            Duration::from_secs(5)
        );

        // Then: Should succeed (no error)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_subscriber_receives_published_event() {
        // Given: EventBusActor and subscriber
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received_events = Arc::new(Mutex::new(Vec::new()));
        let (sub_ref, _sub_handle) = Actor::spawn(
            Some("test-sub".to_string()),
            TestSubscriber {
                received_events: received_events.clone(),
            },
            (),
        )
        .await
        .unwrap();

        // Subscribe to topic
        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "test.topic".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        // Give subscription time to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        let event = test_event("test.topic", json!({"message": "hello"}));

        // When: Publish event
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event: event.clone(),
                persist: false,
            }
        )
        .unwrap();

        // Give event time to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Subscriber should receive the event
        let events = received_events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "test.topic");
        assert_eq!(events[0].payload, json!({"message": "hello"}));
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_same_event() {
        // Given: EventBusActor with multiple subscribers
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received_1 = Arc::new(Mutex::new(Vec::new()));
        let received_2 = Arc::new(Mutex::new(Vec::new()));

        let (sub1_ref, _sub1_handle) = Actor::spawn(
            Some("test-sub-1".to_string()),
            TestSubscriber {
                received_events: received_1.clone(),
            },
            (),
        )
        .await
        .unwrap();

        let (sub2_ref, _sub2_handle) = Actor::spawn(
            Some("test-sub-2".to_string()),
            TestSubscriber {
                received_events: received_2.clone(),
            },
            (),
        )
        .await
        .unwrap();

        // Subscribe both to same topic
        for sub in [&sub1_ref, &sub2_ref] {
            ractor::cast!(
                bus_ref,
                EventBusMsg::Subscribe {
                    topic: "test.topic".to_string(),
                    subscriber: sub.clone(),
                }
            )
            .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        let event = test_event("test.topic", json!({"message": "broadcast"}));

        // When: Publish event
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event: event.clone(),
                persist: false,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Both subscribers receive the event
        assert_eq!(received_1.lock().await.len(), 1);
        assert_eq!(received_2.lock().await.len(), 1);
        assert_eq!(received_1.lock().await[0].id, event.id);
        assert_eq!(received_2.lock().await[0].id, event.id);
    }

    #[tokio::test]
    async fn test_topic_isolation() {
        // Given: Subscribers on different topics
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received_a = Arc::new(Mutex::new(Vec::new()));
        let received_b = Arc::new(Mutex::new(Vec::new()));

        let (sub_a_ref, _sub_a_handle) = Actor::spawn(
            Some("test-sub-a".to_string()),
            TestSubscriber {
                received_events: received_a.clone(),
            },
            (),
        )
        .await
        .unwrap();

        let (sub_b_ref, _sub_b_handle) = Actor::spawn(
            Some("test-sub-b".to_string()),
            TestSubscriber {
                received_events: received_b.clone(),
            },
            (),
        )
        .await
        .unwrap();

        // Subscribe to different topics
        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "topic.a".to_string(),
                subscriber: sub_a_ref.clone(),
            }
        )
        .unwrap();

        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "topic.b".to_string(),
                subscriber: sub_b_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // When: Publish to topic.a only
        let event = test_event("topic.a", json!({"data": "a-only"}));
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event,
                persist: false,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Only subscriber A receives it
        assert_eq!(received_a.lock().await.len(), 1);
        assert_eq!(received_b.lock().await.len(), 0);
    }

    #[tokio::test]
    async fn test_unsubscribe_removes_subscriber() {
        // Given: Subscribed subscriber
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let (sub_ref, _sub_handle) = Actor::spawn(
            Some("test-sub".to_string()),
            TestSubscriber {
                received_events: received.clone(),
            },
            (),
        )
        .await
        .unwrap();

        // Subscribe
        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "test.topic".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish first event
        let event1 = test_event("test.topic", json!({"seq": 1}));
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event: event1,
                persist: false,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(received.lock().await.len(), 1);

        // When: Unsubscribe
        ractor::cast!(
            bus_ref,
            EventBusMsg::Unsubscribe {
                topic: "test.topic".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish second event
        let event2 = test_event("test.topic", json!({"seq": 2}));
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event: event2,
                persist: false,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Subscriber should not receive second event
        assert_eq!(received.lock().await.len(), 1);
    }

    // ============================================================================
    // Unit Tests: Wildcard Topics
    // ============================================================================

    #[tokio::test]
    async fn test_wildcard_subscription_receives_matching_events() {
        // Given: Subscriber with wildcard pattern
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let (sub_ref, _sub_handle) = Actor::spawn(
            Some("test-sub".to_string()),
            TestSubscriber {
                received_events: received.clone(),
            },
            (),
        )
        .await
        .unwrap();

        // Subscribe to wildcard pattern
        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "worker.*".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // When: Publish to matching topics
        let events = vec![
            test_event("worker.task", json!({})),
            test_event("worker.job", json!({})),
            test_event("worker.process", json!({})),
            test_event("other.topic", json!({})), // Should not match
        ];

        for event in events {
            ractor::cast!(
                bus_ref,
                EventBusMsg::Publish {
                    event,
                    persist: false,
                }
            )
            .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Should receive 3 matching events
        assert_eq!(received.lock().await.len(), 3);
    }

    // ============================================================================
    // Unit Tests: Persistence
    // ============================================================================

    #[tokio::test]
    async fn test_persist_true_stores_event() {
        // Given: EventBusActor with persist enabled
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref.clone(),
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let event = test_event("test.topic", json!({"data": "persist-me"}));

        // When: Publish with persist=true
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event: event.clone(),
                persist: true,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Event should be in store
        let stored_events = ractor::call!(
            store_ref,
            MockEventStoreMsg::GetEvents { reply: _ },
            Duration::from_secs(5)
        )
        .unwrap();

        assert_eq!(stored_events.len(), 1);
        assert_eq!(stored_events[0].id, event.id);
    }

    #[tokio::test]
    async fn test_persist_false_does_not_store_event() {
        // Given: EventBusActor
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref.clone(),
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let event = test_event("test.topic", json!({}));

        // When: Publish with persist=false
        ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event,
                persist: false,
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Then: Store should be empty
        let stored_events = ractor::call!(
            store_ref,
            MockEventStoreMsg::GetEvents { reply: _ },
            Duration::from_secs(5)
        )
        .unwrap();

        assert_eq!(stored_events.len(), 0);
    }

    // ============================================================================
    // Unit Tests: Event Ordering
    // ============================================================================

    #[tokio::test]
    async fn test_events_delivered_in_publish_order() {
        // Given: Subscriber
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let (sub_ref, _sub_handle) = Actor::spawn(
            Some("test-sub".to_string()),
            TestSubscriber {
                received_events: received.clone(),
            },
            (),
        )
        .await
        .unwrap();

        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "test.topic".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // When: Publish events in sequence
        let events: Vec<Event> = (0..5)
            .map(|i| test_event("test.topic", json!({"seq": i})))
            .collect();

        for event in &events {
            ractor::cast!(
                bus_ref,
                EventBusMsg::Publish {
                    event: event.clone(),
                    persist: false,
                }
            )
            .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Then: Events received in order
        let received = received.lock().await;
        assert_eq!(received.len(), 5);
        for (i, event) in received.iter().enumerate() {
            assert_eq!(event.payload["seq"], i);
        }
    }

    // ============================================================================
    // Unit Tests: Error Handling
    // ============================================================================

    #[tokio::test]
    async fn test_publish_to_nonexistent_topic_succeeds() {
        // Publishing to topic with no subscribers should not fail
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let event = test_event("nonexistent.topic", json!({}));

        // Should not panic or error
        let result = ractor::cast!(
            bus_ref,
            EventBusMsg::Publish {
                event,
                persist: false,
            }
        );

        assert!(result.is_ok());
    }

    // ============================================================================
    // Integration Tests
    // ============================================================================

    #[tokio::test]
    async fn test_high_throughput_event_streaming() {
        // Test: Can handle many events quickly
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let (sub_ref, _sub_handle) = Actor::spawn(
            Some("test-sub".to_string()),
            TestSubscriber {
                received_events: received.clone(),
            },
            (),
        )
        .await
        .unwrap();

        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "load.test".to_string(),
                subscriber: sub_ref.clone(),
            }
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish 1000 events
        let start = std::time::Instant::now();
        for i in 0..1000 {
            let event = test_event("load.test", json!({"index": i}));
            ractor::cast!(
                bus_ref,
                EventBusMsg::Publish {
                    event,
                    persist: false,
                }
            )
            .unwrap();
        }

        // Wait for all to be received
        let timeout = Duration::from_secs(10);
        let start_wait = std::time::Instant::now();
        loop {
            let count = received.lock().await.len();
            if count >= 1000 {
                break;
            }
            if start_wait.elapsed() > timeout {
                panic!("Timeout waiting for events. Received: {}", count);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let elapsed = start.elapsed();
        let received = received.lock().await;
        assert_eq!(received.len(), 1000);

        // Log performance
        tracing::info!(
            "Published 1000 events in {:?} ({:.0} events/sec)",
            elapsed,
            1000.0 / elapsed.as_secs_f64()
        );
    }

    #[tokio::test]
    async fn test_concurrent_subscribers_no_message_loss() {
        // Test: Multiple subscribers all receive all messages
        let (store_ref, _store_handle) = Actor::spawn(
            Some("test-store".to_string()),
            MockEventStore::default(),
            (),
        )
        .await
        .unwrap();

        let args = EventBusArguments {
            event_store: store_ref,
            config: EventBusConfig::default(),
        };

        let (bus_ref, _bus_handle) = Actor::spawn(
            Some("test-bus".to_string()),
            EventBusActor,
            args,
        )
        .await
        .unwrap();

        // Create 10 subscribers
        let mut subscriber_refs = Vec::new();
        let mut received_vecs = Vec::new();

        for i in 0..10 {
            let received = Arc::new(Mutex::new(Vec::new()));
            let (sub_ref, _sub_handle) = Actor::spawn(
                Some(format!("test-sub-{}", i)),
                TestSubscriber {
                    received_events: received.clone(),
                },
                (),
            )
            .await
            .unwrap();

            subscriber_refs.push(sub_ref.clone());
            received_vecs.push(received);

            ractor::cast!(
                bus_ref,
                EventBusMsg::Subscribe {
                    topic: "concurrent.test".to_string(),
                    subscriber: sub_ref,
                }
            )
            .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish 100 events
        for i in 0..100 {
            let event = test_event("concurrent.test", json!({"index": i}));
            ractor::cast!(
                bus_ref,
                EventBusMsg::Publish {
                    event,
                    persist: false,
                }
            )
            .unwrap();
        }

        // Wait for all subscribers to receive all events
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify each subscriber got all 100 events
        for (i, received) in received_vecs.iter().enumerate() {
            let count = received.lock().await.len();
            assert_eq!(
                count, 100,
                "Subscriber {} received {} events instead of 100",
                i, count
            );
        }
    }

    // ============================================================================
    // Property-Based Tests (Conceptual - would use proptest in real implementation)
    // ============================================================================

    #[tokio::test]
    async fn test_event_id_uniqueness() {
        // Property: All events have unique IDs
        let mut ids = std::collections::HashSet::new();

        for _ in 0..1000 {
            let event = test_event("test", json!({}));
            assert!(
                ids.insert(event.id.clone()),
                "Duplicate event ID generated: {}",
                event.id
            );
        }
    }

    #[tokio::test]
    async fn test_topic_matching_properties() {
        // Property: Topic matching follows expected rules
        let test_cases = vec![
            ("a.b.c", "a.b.c", true),     // Exact match
            ("a.b.c", "a.b.*", true),     // Wildcard at end
            ("a.b.c", "a.*", true),       // Wildcard at parent
            ("a.b.c", "*", true),         // Root wildcard
            ("a.b.c", "a.b.d", false),    // Different leaf
            ("a.b.c", "x.*", false),      // Different root
            ("a.b", "a.b.c", false),      // Pattern shorter than topic
        ];

        for (topic, pattern, should_match) in test_cases {
            let event = test_event(topic, json!({}));
            let matches = event.matches_topic(pattern);
            assert_eq!(
                matches, should_match,
                "Topic '{}' should{} match pattern '{}'",
                topic,
                if should_match { "" } else { " not" },
                pattern
            );
        }
    }
}
