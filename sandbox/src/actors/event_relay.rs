//! EventRelayActor - committed event relay from EventStore to EventBus.
//!
//! ADR-0001:
//! - EventStore is canonical source of truth.
//! - EventBus is delivery plane only.
//! This actor relays committed EventStore rows into EventBus topics.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::time::Duration;

use crate::actors::event_bus::{Event, EventBusMsg, EventType};
use crate::actors::event_store::EventStoreMsg;

#[derive(Debug, Clone)]
pub struct EventRelayArguments {
    pub event_store: ActorRef<EventStoreMsg>,
    pub event_bus: ActorRef<EventBusMsg>,
    pub poll_interval_ms: u64,
}

pub struct EventRelayState {
    pub event_store: ActorRef<EventStoreMsg>,
    pub event_bus: ActorRef<EventBusMsg>,
    pub since_seq: i64,
}

#[derive(Debug)]
pub enum EventRelayMsg {
    Tick,
    GetCursor { reply: RpcReplyPort<i64> },
    SetEventBus { event_bus: ActorRef<EventBusMsg> },
}

#[derive(Debug, Default)]
pub struct EventRelayActor;

#[async_trait]
impl Actor for EventRelayActor {
    type Msg = EventRelayMsg;
    type State = EventRelayState;
    type Arguments = EventRelayArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let interval = Duration::from_millis(args.poll_interval_ms.max(100));
        let tick_ref = myself.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = tick_ref.cast(EventRelayMsg::Tick);
            }
        });

        Ok(EventRelayState {
            event_store: args.event_store,
            event_bus: args.event_bus,
            since_seq: 0,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            EventRelayMsg::Tick => {
                if let Err(e) = self.relay_once(state).await {
                    tracing::warn!(error = %e, "Event relay tick failed");
                }
            }
            EventRelayMsg::GetCursor { reply } => {
                let _ = reply.send(state.since_seq);
            }
            EventRelayMsg::SetEventBus { event_bus } => {
                state.event_bus = event_bus;
            }
        }
        Ok(())
    }
}

impl EventRelayActor {
    async fn relay_once(&self, state: &mut EventRelayState) -> Result<(), String> {
        let events = ractor::call!(state.event_store, |reply| EventStoreMsg::GetRecentEvents {
            since_seq: state.since_seq,
            limit: 500,
            event_type_prefix: None,
            actor_id: None,
            user_id: None,
            reply
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        if events.is_empty() {
            return Ok(());
        }

        for stored in events {
            let relay_payload = match stored.payload {
                serde_json::Value::Object(mut obj) => {
                    obj.insert(
                        "committed_event".to_string(),
                        serde_json::json!({
                            "seq": stored.seq,
                            "event_id": stored.event_id,
                            "event_type": stored.event_type,
                            "timestamp": stored.timestamp.to_rfc3339(),
                            "actor_id": stored.actor_id.0,
                            "user_id": stored.user_id,
                        }),
                    );
                    serde_json::Value::Object(obj)
                }
                other => serde_json::json!({
                    "value": other,
                    "committed_event": {
                        "seq": stored.seq,
                        "event_id": stored.event_id,
                        "event_type": stored.event_type,
                        "timestamp": stored.timestamp.to_rfc3339(),
                        "actor_id": stored.actor_id.0,
                        "user_id": stored.user_id,
                    }
                }),
            };

            let event = Event::new(
                EventType::Custom(stored.event_type.clone()),
                stored.event_type.clone(),
                relay_payload,
                stored.actor_id.0.clone(),
            )
            .map_err(|e| e.to_string())?;

            ractor::cast!(
                state.event_bus,
                EventBusMsg::Publish {
                    event,
                    persist: false,
                }
            )
            .map_err(|e| e.to_string())?;

            // Advance cursor only after successful fanout publish.
            state.since_seq = state.since_seq.max(stored.seq);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_bus::{EventBusActor, EventBusArguments, EventBusConfig};
    use crate::actors::event_store::{AppendEvent, EventStoreActor, EventStoreArguments};
    use async_trait::async_trait;
    use ractor::{Actor, ActorRef};
    use tokio::sync::mpsc;

    #[derive(Debug, Default)]
    struct CollectorActor;

    #[derive(Debug)]
    enum CollectorMsg {
        Event(Event),
    }

    struct CollectorState {
        tx: mpsc::UnboundedSender<Event>,
    }

    #[async_trait]
    impl Actor for CollectorActor {
        type Msg = CollectorMsg;
        type State = CollectorState;
        type Arguments = mpsc::UnboundedSender<Event>;

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            args: Self::Arguments,
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(CollectorState { tx: args })
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            match message {
                CollectorMsg::Event(event) => {
                    let _ = state.tx.send(event);
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_event_relay_publishes_committed_events() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (bus_ref, _bus_handle) = Actor::spawn(
            None,
            EventBusActor,
            EventBusArguments {
                event_store: None,
                config: EventBusConfig::default(),
            },
        )
        .await
        .unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
        let (collector_ref, _collector_handle) =
            Actor::spawn(None, CollectorActor, tx).await.unwrap();

        // Adapter actor to satisfy EventBus subscriber type (ActorRef<Event>).
        #[derive(Debug, Default)]
        struct EventBridge;
        #[async_trait]
        impl Actor for EventBridge {
            type Msg = Event;
            type State = ActorRef<CollectorMsg>;
            type Arguments = ActorRef<CollectorMsg>;

            async fn pre_start(
                &self,
                _myself: ActorRef<Self::Msg>,
                args: Self::Arguments,
            ) -> Result<Self::State, ActorProcessingErr> {
                Ok(args)
            }

            async fn handle(
                &self,
                _myself: ActorRef<Self::Msg>,
                message: Self::Msg,
                state: &mut Self::State,
            ) -> Result<(), ActorProcessingErr> {
                let _ = state.cast(CollectorMsg::Event(message));
                Ok(())
            }
        }

        let (bridge_ref, _bridge_handle) = Actor::spawn(None, EventBridge, collector_ref)
            .await
            .unwrap();

        ractor::cast!(
            bus_ref,
            EventBusMsg::Subscribe {
                topic: "worker.task.*".to_string(),
                subscriber: bridge_ref,
            }
        )
        .unwrap();

        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({"task_id":"t1"}),
                actor_id: "application_supervisor".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (relay_ref, _relay_handle) = Actor::spawn(
            None,
            EventRelayActor,
            EventRelayArguments {
                event_store: store_ref.clone(),
                event_bus: bus_ref.clone(),
                poll_interval_ms: 10_000,
            },
        )
        .await
        .unwrap();

        let _ = relay_ref.cast(EventRelayMsg::Tick);
        let relayed = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(relayed.topic, "worker.task.started");
        assert_eq!(
            relayed.event_type,
            EventType::Custom("worker.task.started".to_string())
        );
        assert!(relayed.payload.get("committed_event").is_some());
    }

    #[tokio::test]
    async fn test_event_relay_does_not_advance_cursor_when_bus_unavailable() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (bus_ref, _bus_handle) = Actor::spawn(
            None,
            EventBusActor,
            EventBusArguments {
                event_store: None,
                config: EventBusConfig::default(),
            },
        )
        .await
        .unwrap();

        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({"task_id":"t-failover"}),
                actor_id: "application_supervisor".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (relay_ref, _relay_handle) = Actor::spawn(
            None,
            EventRelayActor,
            EventRelayArguments {
                event_store: store_ref.clone(),
                event_bus: bus_ref.clone(),
                poll_interval_ms: 10_000,
            },
        )
        .await
        .unwrap();

        // Simulate EventBus outage.
        bus_ref.stop(None);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = relay_ref.cast(EventRelayMsg::Tick);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let cursor = ractor::call!(relay_ref, |reply| EventRelayMsg::GetCursor { reply }).unwrap();
        assert_eq!(cursor, 0, "cursor must not advance when publish fails");
    }

    #[tokio::test]
    async fn test_event_relay_resumes_delivery_after_bus_rebind() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let (bus_ref_1, _bus_handle_1) = Actor::spawn(
            None,
            EventBusActor,
            EventBusArguments {
                event_store: None,
                config: EventBusConfig::default(),
            },
        )
        .await
        .unwrap();

        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({"task_id":"t-rebind"}),
                actor_id: "application_supervisor".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (relay_ref, _relay_handle) = Actor::spawn(
            None,
            EventRelayActor,
            EventRelayArguments {
                event_store: store_ref.clone(),
                event_bus: bus_ref_1.clone(),
                poll_interval_ms: 10_000,
            },
        )
        .await
        .unwrap();

        // Outage on first bus: tick should fail and keep cursor at 0.
        bus_ref_1.stop(None);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = relay_ref.cast(EventRelayMsg::Tick);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cursor = ractor::call!(relay_ref, |reply| EventRelayMsg::GetCursor { reply }).unwrap();
        assert_eq!(cursor, 0, "cursor must remain at 0 during bus outage");

        // Start replacement bus and subscribe collector.
        let (bus_ref_2, _bus_handle_2) = Actor::spawn(
            None,
            EventBusActor,
            EventBusArguments {
                event_store: None,
                config: EventBusConfig::default(),
            },
        )
        .await
        .unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
        let (collector_ref, _collector_handle) =
            Actor::spawn(None, CollectorActor, tx).await.unwrap();

        #[derive(Debug, Default)]
        struct EventBridge;
        #[async_trait]
        impl Actor for EventBridge {
            type Msg = Event;
            type State = ActorRef<CollectorMsg>;
            type Arguments = ActorRef<CollectorMsg>;

            async fn pre_start(
                &self,
                _myself: ActorRef<Self::Msg>,
                args: Self::Arguments,
            ) -> Result<Self::State, ActorProcessingErr> {
                Ok(args)
            }

            async fn handle(
                &self,
                _myself: ActorRef<Self::Msg>,
                message: Self::Msg,
                state: &mut Self::State,
            ) -> Result<(), ActorProcessingErr> {
                let _ = state.cast(CollectorMsg::Event(message));
                Ok(())
            }
        }

        let (bridge_ref, _bridge_handle) = Actor::spawn(None, EventBridge, collector_ref)
            .await
            .unwrap();
        ractor::cast!(
            bus_ref_2,
            EventBusMsg::Subscribe {
                topic: "worker.task.*".to_string(),
                subscriber: bridge_ref,
            }
        )
        .unwrap();

        let _ = relay_ref.cast(EventRelayMsg::SetEventBus {
            event_bus: bus_ref_2.clone(),
        });
        let _ = relay_ref.cast(EventRelayMsg::Tick);

        let relayed = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(relayed.topic, "worker.task.started");
        assert_eq!(
            relayed.event_type,
            EventType::Custom("worker.task.started".to_string())
        );
        assert!(relayed.payload.get("committed_event").is_some());

        let cursor = ractor::call!(relay_ref, |reply| EventRelayMsg::GetCursor { reply }).unwrap();
        assert!(
            cursor > 0,
            "cursor should advance after successful rebind publish"
        );
    }
}
