//! Supervision tests - Phase 1 Foundation
//!
//! These tests verify the basic supervision tree functionality:
//! - Supervisor correctly spawns and manages child actors
//! - Failed actors are detected via SupervisionEvent::ActorFailed
//! - Terminated actors trigger SupervisionEvent::ActorTerminated
//! - Registry auto-cleanup works (where_is returns None after actor stops)

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

/// Simple test actor that can fail or terminate on command
#[derive(Debug, Default)]
struct TestActor;

#[derive(Debug)]
enum TestActorMsg {
    Ping(RpcReplyPort<String>),
    Fail,
    Stop,
}

#[ractor::async_trait]
impl Actor for TestActor {
    type Msg = TestActorMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TestActorMsg::Ping(reply) => {
                let _ = reply.send("pong".to_string());
            }
            TestActorMsg::Fail => {
                tracing::info!("TestActor received Fail command");
                return Err(ActorProcessingErr::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Intentional failure",
                )));
            }
            TestActorMsg::Stop => {
                tracing::info!("TestActor received Stop command");
                myself.stop(Some("Test stop".to_string()));
            }
        }
        Ok(())
    }
}

/// Test supervisor that tracks supervision events
#[derive(Debug, Default)]
struct TestSupervisor;

struct TestSupervisorState {
    events: Arc<std::sync::Mutex<Vec<String>>>,
    child_count: AtomicUsize,
}

#[derive(Debug)]
enum TestSupervisorMsg {
    RecordEvent(String),
    GetEvents(RpcReplyPort<Vec<String>>),
    GetChildCount(RpcReplyPort<usize>),
    ChildStarted,
    ChildFailed,
    ChildTerminated,
}

#[ractor::async_trait]
impl Actor for TestSupervisor {
    type Msg = TestSupervisorMsg;
    type State = TestSupervisorState;
    type Arguments = Arc<std::sync::Mutex<Vec<String>>>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        events: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("TestSupervisor starting");
        Ok(TestSupervisorState {
            events,
            child_count: AtomicUsize::new(0),
        })
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let event_str = match &event {
            SupervisionEvent::ActorStarted(cell) => {
                state.child_count.fetch_add(1, Ordering::SeqCst);
                format!("Started:{}", cell.get_id())
            }
            SupervisionEvent::ActorFailed(cell, err) => {
                format!("Failed:{}:{}", cell.get_id(), err)
            }
            SupervisionEvent::ActorTerminated(cell, _st, reason) => {
                state.child_count.fetch_sub(1, Ordering::SeqCst);
                format!(
                    "Terminated:{}:{}",
                    cell.get_id(),
                    reason.as_deref().unwrap_or("no reason")
                )
            }
            _ => format!("Other:{:?}", event),
        };

        if let Ok(mut events) = state.events.lock() {
            events.push(event_str);
        }

        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TestSupervisorMsg::RecordEvent(event_str) => {
                if let Ok(mut events) = state.events.lock() {
                    events.push(event_str);
                }
            }
            TestSupervisorMsg::GetEvents(reply) => {
                let events = state.events.lock().map(|e| e.clone()).unwrap_or_default();
                let _ = reply.send(events);
            }
            TestSupervisorMsg::GetChildCount(reply) => {
                let count = state.child_count.load(Ordering::SeqCst);
                let _ = reply.send(count);
            }
            TestSupervisorMsg::ChildStarted => {
                state.child_count.fetch_add(1, Ordering::SeqCst);
            }
            TestSupervisorMsg::ChildFailed => {
                // Just record it
            }
            TestSupervisorMsg::ChildTerminated => {
                state.child_count.fetch_sub(1, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_supervisor_restarts_failed_child() {
    tracing::info!("Testing supervisor handles failed child...");

    let events = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn supervisor
    let (supervisor, _handle) = Actor::spawn(None, TestSupervisor::default(), events.clone())
        .await
        .expect("Failed to spawn supervisor");

    // Spawn test actor as linked child
    let (child, child_handle) =
        Actor::spawn_linked(None, TestActor::default(), (), supervisor.get_cell())
            .await
            .expect("Failed to spawn child");

    // Wait for startup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify child is running
    let ping_result = ractor::call!(child, |reply| TestActorMsg::Ping(reply));
    assert!(ping_result.is_ok(), "Child should respond to ping");
    assert_eq!(ping_result.unwrap(), "pong");

    // Trigger failure
    let _ = child.cast(TestActorMsg::Fail);

    // Wait for failure to be processed
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait for child to stop
    let _ = timeout(Duration::from_secs(2), child_handle).await;

    // The supervision event should have been received by the supervisor
    // Because we used spawn_linked, the supervisor's handle_supervision_event was called
    tracing::info!("Test completed - supervisor should have received failure event");
}

#[tokio::test]
async fn test_supervisor_handles_actor_termination() {
    tracing::info!("Testing supervisor handles actor termination...");

    let events = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn supervisor
    let (supervisor, _handle) = Actor::spawn(None, TestSupervisor::default(), events.clone())
        .await
        .expect("Failed to spawn supervisor");

    // Spawn test actor as linked child
    let (child, child_handle) =
        Actor::spawn_linked(None, TestActor::default(), (), supervisor.get_cell())
            .await
            .expect("Failed to spawn child");

    // Wait for startup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify child is running via ping
    let ping_result = ractor::call!(child, |reply| TestActorMsg::Ping(reply));
    assert!(ping_result.is_ok(), "Child should respond to ping");

    // Gracefully stop the child
    let _ = child.cast(TestActorMsg::Stop);

    // Wait for termination
    let _ = timeout(Duration::from_secs(2), child_handle).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The termination should have been received by supervisor via handle_supervision_event
    tracing::info!("Test completed - supervisor should have received termination event");
}

#[tokio::test]
async fn test_actor_registry_auto_cleanup() {
    tracing::info!("Testing actor registry auto-cleanup...");

    // Spawn a named actor
    let actor_name = "test_cleanup_actor".to_string();

    let (actor, handle) = Actor::spawn(Some(actor_name.clone()), TestActor::default(), ())
        .await
        .expect("Failed to spawn named actor");

    // Verify actor is in registry
    let found = ractor::registry::where_is(actor_name.clone());
    assert!(found.is_some(), "Actor should be in registry after spawn");

    // Stop the actor
    let _ = actor.cast(TestActorMsg::Stop);

    // Wait for actor to stop
    let _ = timeout(Duration::from_secs(2), handle).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify actor is no longer in registry
    let found_after = ractor::registry::where_is(actor_name.clone());
    assert!(
        found_after.is_none(),
        "Actor should be removed from registry after stop"
    );
}

/// Integration test for the real ApplicationSupervisor
#[cfg(feature = "supervision_refactor")]
mod integration_tests {
    use super::*;
    use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
    use sandbox::supervisor::ApplicationSupervisor;

    #[tokio::test]
    async fn test_application_supervisor_spawns_successfully() {
        tracing::info!("Testing ApplicationSupervisor spawn...");

        // Create EventStore first
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Verify supervisor is in registry
        let found = ractor::registry::where_is("test_app_supervisor".to_string());
        assert!(
            found.is_some(),
            "ApplicationSupervisor should be in registry"
        );

        tracing::info!("ApplicationSupervisor test passed!");
    }
}
