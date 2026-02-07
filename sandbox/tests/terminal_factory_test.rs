//! Terminal supervision tests

use ractor::{Actor, ActorRef};

use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::supervisor::terminal::{
    TerminalSupervisor, TerminalSupervisorArgs, TerminalSupervisorMsg,
};
use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

async fn create_event_store() -> ActorRef<sandbox::actors::event_store::EventStoreMsg> {
    let (event_store, _) = Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
        .await
        .expect("failed to spawn event store");
    event_store
}

#[tokio::test]
async fn test_terminal_supervisor_create_and_reuse_terminal() {
    let event_store = create_event_store().await;
    let (supervisor, _) = Actor::spawn(
        None,
        TerminalSupervisor,
        TerminalSupervisorArgs { event_store },
    )
    .await
    .expect("failed to spawn terminal supervisor");

    let first = ractor::call!(&supervisor, |reply| {
        TerminalSupervisorMsg::GetOrCreateTerminal {
            terminal_id: "term-1".to_string(),
            user_id: "user-1".to_string(),
            shell: "/bin/bash".to_string(),
            working_dir: "/tmp".to_string(),
            reply,
        }
    })
    .expect("rpc failed")
    .expect("terminal create failed");

    let second = ractor::call!(&supervisor, |reply| {
        TerminalSupervisorMsg::GetOrCreateTerminal {
            terminal_id: "term-1".to_string(),
            user_id: "user-2".to_string(),
            shell: "/bin/zsh".to_string(),
            working_dir: "/".to_string(),
            reply,
        }
    })
    .expect("rpc failed")
    .expect("terminal create failed");

    assert_eq!(first.get_id(), second.get_id());
}

#[tokio::test]
async fn test_application_supervisor_routes_terminal() {
    let event_store = create_event_store().await;
    let (supervisor, _) = Actor::spawn(None, ApplicationSupervisor, event_store)
        .await
        .expect("failed to spawn application supervisor");

    let terminal = ractor::call!(&supervisor, |reply| {
        ApplicationSupervisorMsg::GetOrCreateTerminal {
            terminal_id: "routed-term".to_string(),
            user_id: "user-1".to_string(),
            shell: "/bin/bash".to_string(),
            working_dir: "/tmp".to_string(),
            reply,
        }
    })
    .expect("rpc failed");

    let info = ractor::call!(terminal, |reply| {
        sandbox::actors::terminal::TerminalMsg::GetInfo { reply }
    })
    .expect("failed to get terminal info");

    assert_eq!(info.terminal_id, "routed-term");
}
