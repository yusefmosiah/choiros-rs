//! ConductorActor - orchestrates task execution across worker actors
//!
//! The ConductorActor is the central orchestration component that:
//! - Receives task execution requests via `ConductorMsg::ExecuteTask`
//! - Routes tasks to appropriate worker actors (ResearcherActor, TerminalActor, etc.)
//! - Manages task lifecycle with typed state transitions
//! - Writes reports to sandbox-safe paths
//! - Emits events for observability
//!
//! ## State Machine
//!
//! ```text
//! Queued → Running → WaitingWorker → Completed
//!                                      |
//!                                      v
//!                                    Failed
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ractor::Actor;
//! use crate::actors::conductor::{ConductorActor, ConductorArguments};
//!
//! let args = ConductorArguments {
//!     event_store: event_store_ref,
//!     researcher_actor: Some(researcher_ref),
//!     terminal_actor: Some(terminal_ref),
//! };
//!
//! let (conductor_ref, _handle) = Actor::spawn(None, ConductorActor, args).await?;
//! ```

pub mod actor;
pub mod events;
pub mod model_gateway;
pub mod output;
pub mod protocol;
mod runtime;
pub mod state;
pub mod workers;

#[cfg(test)]
mod tests;

pub use actor::{ConductorActor, ConductorArguments, ConductorState};
pub use protocol::{ConductorError, ConductorMsg, WorkerOutput};
