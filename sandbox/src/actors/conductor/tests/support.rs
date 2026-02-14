use ractor::{Actor, ActorRef};

use crate::actors::conductor::{ConductorActor, ConductorArguments, ConductorMsg};
use crate::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;

pub(crate) async fn setup_test_conductor(
    researcher_actor: Option<ActorRef<ResearcherMsg>>,
    terminal_actor: Option<ActorRef<TerminalMsg>>,
) -> (ActorRef<ConductorMsg>, ActorRef<EventStoreMsg>) {
    let (store_ref, _store_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let args = ConductorArguments {
        event_store: store_ref.clone(),
        researcher_actor,
        terminal_actor,
    };

    let (conductor_ref, _conductor_handle) =
        Actor::spawn(None, ConductorActor, args).await.unwrap();
    (conductor_ref, store_ref)
}
