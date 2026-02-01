pub mod event_store;
pub mod chat;
pub mod chat_agent;
pub mod desktop;

pub use event_store::{EventStoreActor, AppendEvent};
pub use chat::{ChatActor};
pub use chat_agent::{ChatAgent, ProcessMessage};
pub use desktop::DesktopActor;
