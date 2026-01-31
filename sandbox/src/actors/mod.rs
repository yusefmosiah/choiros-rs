pub mod event_store;
pub mod chat;
pub mod desktop;

pub use event_store::{EventStoreActor, AppendEvent};
pub use chat::{ChatActor};
pub use desktop::{DesktopActor, DesktopError};
