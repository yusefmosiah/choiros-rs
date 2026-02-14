//! WriterActor - event-driven writing authority.
//!
//! Unlike researcher/terminal, WriterActor does not run a planning loop.
//! It reacts to typed actor messages from workers/humans and mutates run
//! documents through RunWriterActor. When multi-step work is needed, it can
//! delegate to researcher/terminal actors via typed actor messages.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress, ResearcherResult};
use crate::actors::run_writer::{PatchOp, PatchOpKind, RunWriterMsg, SectionState};
use crate::actors::terminal::{TerminalAgentProgress, TerminalAgentResult, TerminalMsg};

#[derive(Debug, Default)]
pub struct WriterActor;

#[derive(Debug, Clone)]
pub struct WriterArguments {
    pub writer_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
}

pub struct WriterState {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
    researcher_actor: Option<ActorRef<ResearcherMsg>>,
    terminal_actor: Option<ActorRef<TerminalMsg>>,
}

#[derive(Debug)]
pub enum WriterMsg {
    /// Apply text to a run section via RunWriterActor.
    ApplyText {
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
        proposal: bool,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Emit non-mutating progress for a run section.
    ReportProgress {
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        phase: String,
        message: String,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Update section state for writer UX.
    SetSectionState {
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        state: SectionState,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// Append a human comment into `user` proposal context.
    ApplyHumanComment {
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        comment: String,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Delegate multi-step work to a worker actor.
    DelegateTask {
        capability: WriterDelegateCapability,
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        run_id: Option<String>,
        call_id: Option<String>,
        reply: RpcReplyPort<Result<WriterDelegateResult, WriterError>>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriterSource {
    Writer,
    Researcher,
    Terminal,
    User,
    Conductor,
}

impl WriterSource {
    fn as_str(self) -> &'static str {
        match self {
            WriterSource::Writer => "writer",
            WriterSource::Researcher => "researcher",
            WriterSource::Terminal => "terminal",
            WriterSource::User => "user",
            WriterSource::Conductor => "conductor",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriterDelegateCapability {
    Researcher,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterDelegateResult {
    pub capability: WriterDelegateCapability,
    pub success: bool,
    pub summary: String,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum WriterError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("actor unavailable: {0}")]
    ActorUnavailable(String),
    #[error("worker failed: {0}")]
    WorkerFailed(String),
    #[error("run writer failed: {0}")]
    RunWriterFailed(String),
}

#[async_trait]
impl Actor for WriterActor {
    type Msg = WriterMsg;
    type State = WriterState;
    type Arguments = WriterArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(WriterState {
            writer_id: args.writer_id,
            user_id: args.user_id,
            event_store: args.event_store,
            researcher_actor: args.researcher_actor,
            terminal_actor: args.terminal_actor,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            WriterMsg::ApplyText {
                run_writer_actor,
                run_id,
                section_id,
                source,
                content,
                proposal,
                reply,
            } => {
                let result = Self::apply_text(
                    state,
                    run_writer_actor,
                    run_id,
                    section_id,
                    source,
                    content,
                    proposal,
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::ReportProgress {
                run_writer_actor,
                run_id,
                section_id,
                source,
                phase,
                message,
                reply,
            } => {
                let result = Self::report_progress(
                    state,
                    run_writer_actor,
                    run_id,
                    section_id,
                    source,
                    phase,
                    message,
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::SetSectionState {
                run_writer_actor,
                run_id,
                section_id,
                state: section_state,
                reply,
            } => {
                let result =
                    Self::set_section_state(run_writer_actor, run_id, section_id, section_state)
                        .await;
                let _ = reply.send(result);
            }
            WriterMsg::ApplyHumanComment {
                run_writer_actor,
                run_id,
                comment,
                reply,
            } => {
                let result = Self::apply_text(
                    state,
                    run_writer_actor,
                    run_id,
                    "user".to_string(),
                    WriterSource::User,
                    comment,
                    true,
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::DelegateTask {
                capability,
                objective,
                timeout_ms,
                max_steps,
                run_id,
                call_id,
                reply,
            } => {
                let result = Self::delegate_task(
                    state, capability, objective, timeout_ms, max_steps, run_id, call_id,
                )
                .await;
                let _ = reply.send(result);
            }
        }
        Ok(())
    }
}

impl WriterActor {
    fn emit_event(state: &WriterState, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: state.writer_id.clone(),
            user_id: state.user_id.clone(),
        };
        let _ = state
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    async fn apply_text(
        state: &WriterState,
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
        proposal: bool,
    ) -> Result<u64, WriterError> {
        if content.trim().is_empty() {
            return Err(WriterError::Validation(
                "content cannot be empty".to_string(),
            ));
        }
        let ops = vec![PatchOp {
            kind: PatchOpKind::Append,
            position: None,
            text: Some(content.clone()),
        }];
        match ractor::call!(run_writer_actor, |reply| RunWriterMsg::ApplyPatch {
            run_id: run_id.clone(),
            source: source.as_str().to_string(),
            section_id: section_id.clone(),
            ops,
            proposal,
            reply,
        }) {
            Ok(Ok(result)) => {
                Self::emit_event(
                    state,
                    "writer.actor.apply_text",
                    serde_json::json!({
                        "run_id": run_id,
                        "section_id": section_id,
                        "source": source.as_str(),
                        "proposal": proposal,
                        "revision": result.revision,
                        "lines_modified": result.lines_modified,
                    }),
                );
                Ok(result.revision)
            }
            Ok(Err(e)) => Err(WriterError::RunWriterFailed(e.to_string())),
            Err(e) => Err(WriterError::RunWriterFailed(e.to_string())),
        }
    }

    async fn report_progress(
        state: &WriterState,
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        phase: String,
        message: String,
    ) -> Result<u64, WriterError> {
        if message.trim().is_empty() {
            return Err(WriterError::Validation(
                "message cannot be empty".to_string(),
            ));
        }
        match ractor::call!(run_writer_actor, |reply| {
            RunWriterMsg::ReportSectionProgress {
                run_id: run_id.clone(),
                source: source.as_str().to_string(),
                section_id: section_id.clone(),
                phase: phase.clone(),
                message: message.clone(),
                reply,
            }
        }) {
            Ok(Ok(revision)) => {
                Self::emit_event(
                    state,
                    "writer.actor.progress",
                    serde_json::json!({
                        "run_id": run_id,
                        "section_id": section_id,
                        "source": source.as_str(),
                        "phase": phase,
                        "message": message,
                        "revision": revision,
                    }),
                );
                Ok(revision)
            }
            Ok(Err(e)) => Err(WriterError::RunWriterFailed(e.to_string())),
            Err(e) => Err(WriterError::RunWriterFailed(e.to_string())),
        }
    }

    async fn set_section_state(
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        section_state: SectionState,
    ) -> Result<(), WriterError> {
        match ractor::call!(run_writer_actor, |reply| RunWriterMsg::MarkSectionState {
            run_id,
            section_id,
            state: section_state,
            reply,
        }) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(WriterError::RunWriterFailed(e.to_string())),
            Err(e) => Err(WriterError::RunWriterFailed(e.to_string())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn delegate_task(
        state: &WriterState,
        capability: WriterDelegateCapability,
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        run_id: Option<String>,
        call_id: Option<String>,
    ) -> Result<WriterDelegateResult, WriterError> {
        match capability {
            WriterDelegateCapability::Researcher => {
                let researcher_actor = state.researcher_actor.as_ref().ok_or_else(|| {
                    WriterError::ActorUnavailable("researcher actor unavailable".to_string())
                })?;
                let (tx, _rx) = mpsc::unbounded_channel::<ResearcherProgress>();
                let result =
                    ractor::call!(researcher_actor, |reply| ResearcherMsg::RunAgenticTask {
                        objective: objective.clone(),
                        timeout_ms,
                        max_results: Some(8),
                        max_rounds: max_steps,
                        model_override: None,
                        progress_tx: Some(tx),
                        writer_actor: None,
                        run_writer_actor: None,
                        run_id,
                        call_id,
                        reply,
                    })
                    .map_err(|e| WriterError::WorkerFailed(e.to_string()))?
                    .map_err(|e| WriterError::WorkerFailed(e.to_string()))?;
                Ok(Self::from_researcher_result(result))
            }
            WriterDelegateCapability::Terminal => {
                let terminal_actor = state.terminal_actor.as_ref().ok_or_else(|| {
                    WriterError::ActorUnavailable("terminal actor unavailable".to_string())
                })?;
                let (tx, _rx) = mpsc::unbounded_channel::<TerminalAgentProgress>();
                let result = ractor::call!(terminal_actor, |reply| TerminalMsg::RunAgenticTask {
                    objective: objective.clone(),
                    timeout_ms,
                    max_steps,
                    model_override: None,
                    progress_tx: Some(tx),
                    run_id,
                    call_id,
                    reply,
                })
                .map_err(|e| WriterError::WorkerFailed(e.to_string()))?
                .map_err(|e| WriterError::WorkerFailed(e.to_string()))?;
                Ok(Self::from_terminal_result(result))
            }
        }
    }

    fn from_researcher_result(result: ResearcherResult) -> WriterDelegateResult {
        WriterDelegateResult {
            capability: WriterDelegateCapability::Researcher,
            success: result.success,
            summary: result.summary,
        }
    }

    fn from_terminal_result(result: TerminalAgentResult) -> WriterDelegateResult {
        WriterDelegateResult {
            capability: WriterDelegateCapability::Terminal,
            success: result.success,
            summary: result.summary,
        }
    }
}
