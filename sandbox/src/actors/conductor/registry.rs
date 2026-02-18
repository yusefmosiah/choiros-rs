use ractor::ActorRef;

use crate::actors::researcher::ResearcherMsg;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::WriterMsg;

pub const DEFAULT_CONDUCTOR_RESEARCHER_ID: &str = "conductor-researcher";
pub const DEFAULT_CONDUCTOR_TERMINAL_ID: &str = "conductor-terminal";
pub const DEFAULT_CONDUCTOR_WRITER_ID: &str = "conductor-writer";
pub const RUN_WRITER_ID_PREFIX: &str = "conductor-run-writer";
pub const CALL_RESEARCHER_ID_PREFIX: &str = "conductor-call-researcher";

fn lookup_actor<T>(registry_name: String) -> Option<ActorRef<T>> {
    ractor::registry::where_is(registry_name).map(Into::into)
}

pub fn lookup_researcher_actor() -> Option<ActorRef<ResearcherMsg>> {
    lookup_actor(format!("researcher:{DEFAULT_CONDUCTOR_RESEARCHER_ID}"))
}

pub fn lookup_terminal_actor() -> Option<ActorRef<TerminalMsg>> {
    lookup_actor(format!("terminal:{DEFAULT_CONDUCTOR_TERMINAL_ID}"))
}

pub fn lookup_writer_actor() -> Option<ActorRef<WriterMsg>> {
    lookup_actor(format!("writer:{DEFAULT_CONDUCTOR_WRITER_ID}"))
}

pub fn run_writer_id(run_id: &str) -> String {
    format!("{RUN_WRITER_ID_PREFIX}-{run_id}")
}

pub fn call_researcher_id(run_id: &str, call_id: &str) -> String {
    format!("{CALL_RESEARCHER_ID_PREFIX}-{run_id}-{call_id}")
}

pub fn lookup_writer_actor_for_run(run_id: &str) -> Option<ActorRef<WriterMsg>> {
    let writer_id = run_writer_id(run_id);
    lookup_actor(format!("writer:{writer_id}"))
}
