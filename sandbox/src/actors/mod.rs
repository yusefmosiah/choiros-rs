pub mod event_store;

pub use event_store::{EventStoreActor, AppendEvent, GetEventsForActor, GetEventBySeq, EventStoreError};
