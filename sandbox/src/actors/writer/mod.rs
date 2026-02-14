//! WriterActor - event-driven writing authority.
//!
//! Unlike researcher/terminal, WriterActor does not run a planning loop.
//! It reacts to typed actor messages from workers/humans and mutates run
//! documents through RunWriterActor. When multi-step work is needed, it can
//! delegate to researcher/terminal actors via typed actor messages.

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use tokio::sync::mpsc;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::{ModelRegistry, ModelResolutionContext};
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress, ResearcherResult};
use crate::actors::run_writer::{PatchOp, PatchOpKind, RunWriterMsg, SectionState};
use crate::actors::terminal::{TerminalAgentProgress, TerminalAgentResult, TerminalMsg};
use crate::baml_client::B;
use crate::observability::llm_trace::{LlmCallScope, LlmTraceEmitter};

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
    model_registry: ModelRegistry,
    inbox_queue: VecDeque<WriterInboxMessage>,
    seen_message_ids: HashSet<String>,
    seen_order: VecDeque<String>,
    inbox_processing: bool,
}

#[derive(Debug, Clone)]
struct WriterInboxMessage {
    message_id: String,
    kind: String,
    run_writer_actor: ActorRef<RunWriterMsg>,
    run_id: String,
    section_id: String,
    source: WriterSource,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterQueueAck {
    pub message_id: String,
    pub accepted: bool,
    pub duplicate: bool,
    pub queue_len: usize,
    pub revision: u64,
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
    /// Queue an inbound worker/human message for writer-agent synthesis.
    ///
    /// Control flow uses this actor message path; EventStore remains trace-only.
    EnqueueInbound {
        message_id: String,
        kind: String,
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
        reply: RpcReplyPort<Result<WriterQueueAck, WriterError>>,
    },
    /// Internal wake to process the next queued inbox item.
    ProcessInbox,
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
    #[error("model resolution failed: {0}")]
    ModelResolution(String),
    #[error("writer llm failed: {0}")]
    WriterLlmFailed(String),
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
            model_registry: ModelRegistry::new(),
            inbox_queue: VecDeque::new(),
            seen_message_ids: HashSet::new(),
            seen_order: VecDeque::new(),
            inbox_processing: false,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
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
            WriterMsg::EnqueueInbound {
                message_id,
                kind,
                run_writer_actor,
                run_id,
                section_id,
                source,
                content,
                reply,
            } => {
                let result = Self::enqueue_inbound(
                    &myself,
                    state,
                    WriterInboxMessage {
                        message_id,
                        kind,
                        run_writer_actor,
                        run_id,
                        section_id,
                        source,
                        content,
                    },
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::ProcessInbox => {
                Self::process_inbox(&myself, state).await;
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
    const MAX_SEEN_IDS: usize = 4096;

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

    fn remember_message_id(state: &mut WriterState, message_id: &str) {
        if state.seen_message_ids.insert(message_id.to_string()) {
            state.seen_order.push_back(message_id.to_string());
        }
        while state.seen_order.len() > Self::MAX_SEEN_IDS {
            if let Some(evicted) = state.seen_order.pop_front() {
                state.seen_message_ids.remove(&evicted);
            }
        }
    }

    fn format_inbox_note(inbound: &WriterInboxMessage) -> String {
        format!(
            "> [{source}] {kind} ({id})\n{content}\n",
            source = inbound.source.as_str(),
            kind = inbound.kind,
            id = inbound.message_id,
            content = inbound.content
        )
    }

    async fn enqueue_inbound(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        inbound: WriterInboxMessage,
    ) -> Result<WriterQueueAck, WriterError> {
        if inbound.content.trim().is_empty() {
            return Err(WriterError::Validation(
                "inbound content cannot be empty".to_string(),
            ));
        }

        if state.seen_message_ids.contains(&inbound.message_id) {
            return Ok(WriterQueueAck {
                message_id: inbound.message_id,
                accepted: true,
                duplicate: true,
                queue_len: state.inbox_queue.len(),
                revision: 0,
            });
        }

        let initial_revision = Self::apply_text(
            state,
            inbound.run_writer_actor.clone(),
            inbound.run_id.clone(),
            inbound.section_id.clone(),
            inbound.source,
            Self::format_inbox_note(&inbound),
            true,
        )
        .await?;

        Self::remember_message_id(state, &inbound.message_id);
        state.inbox_queue.push_back(inbound.clone());
        Self::emit_event(
            state,
            "writer.actor.inbox.enqueued",
            serde_json::json!({
                "run_id": inbound.run_id,
                "section_id": inbound.section_id,
                "source": inbound.source.as_str(),
                "kind": inbound.kind,
                "message_id": inbound.message_id,
                "queue_len": state.inbox_queue.len(),
                "revision": initial_revision,
            }),
        );

        if !state.inbox_processing {
            let _ = myself.send_message(WriterMsg::ProcessInbox);
        }

        Ok(WriterQueueAck {
            message_id: inbound.message_id,
            accepted: true,
            duplicate: false,
            queue_len: state.inbox_queue.len(),
            revision: initial_revision,
        })
    }

    async fn process_inbox(myself: &ActorRef<WriterMsg>, state: &mut WriterState) {
        if state.inbox_processing {
            return;
        }

        let Some(inbound) = state.inbox_queue.pop_front() else {
            return;
        };
        state.inbox_processing = true;

        let synthesis = Self::synthesize_with_llm(state, &inbound).await;
        match synthesis {
            Ok(Some(markdown)) => {
                let _ = Self::apply_text(
                    state,
                    inbound.run_writer_actor.clone(),
                    inbound.run_id.clone(),
                    inbound.section_id.clone(),
                    WriterSource::Writer,
                    markdown,
                    false,
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => {
                Self::emit_event(
                    state,
                    "writer.actor.inbox.synthesis_failed",
                    serde_json::json!({
                        "run_id": inbound.run_id,
                        "section_id": inbound.section_id,
                        "message_id": inbound.message_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }

        state.inbox_processing = false;
        if !state.inbox_queue.is_empty() {
            let _ = myself.send_message(WriterMsg::ProcessInbox);
        }
    }

    async fn synthesize_with_llm(
        state: &WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<Option<String>, WriterError> {
        let doc = match ractor::call!(inbound.run_writer_actor, |reply| {
            RunWriterMsg::GetDocument { reply }
        }) {
            Ok(Ok(doc)) => doc,
            Ok(Err(error)) => return Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => return Err(WriterError::RunWriterFailed(error.to_string())),
        };

        let resolved = state
            .model_registry
            .resolve_for_role("writer", &ModelResolutionContext::default())
            .map_err(|e| WriterError::ModelResolution(e.to_string()))?;
        let model_id = resolved.config.id.clone();
        let client_registry = state
            .model_registry
            .create_runtime_client_registry_for_model(&model_id)
            .map_err(|e| WriterError::ModelResolution(e.to_string()))?;

        let prompt = format!(
            "You are WriterActor.\n\
             Produce a concise markdown update to append to section '{section}'.\n\
             Use the new inbox message and current document context.\n\
             Requirements:\n\
             - 3-8 bullet points or short paragraphs\n\
             - preserve factual claims from the inbox message\n\
             - do not repeat the entire document\n\
             - output markdown only\n\n\
             Inbox message id: {message_id}\n\
             Message kind: {kind}\n\
             Message source: {source}\n\
             Message content:\n{content}",
            section = inbound.section_id,
            message_id = inbound.message_id,
            kind = inbound.kind,
            source = inbound.source.as_str(),
            content = inbound.content
        );
        let history = if doc.len() > 12_000 {
            doc.chars()
                .rev()
                .take(12_000)
                .collect::<String>()
                .chars()
                .rev()
                .collect()
        } else {
            doc
        };

        let trace = LlmTraceEmitter::new(state.event_store.clone());
        let trace_ctx = trace.start_call(
            "writer",
            "QuickResponse",
            &state.writer_id,
            &model_id,
            None,
            "Writer inbox synthesis",
            &serde_json::json!({
                "run_id": inbound.run_id,
                "section_id": inbound.section_id,
                "message_id": inbound.message_id,
                "kind": inbound.kind,
            }),
            "Writer synthesizes queued inbound message",
            Some(LlmCallScope {
                run_id: Some(inbound.run_id.clone()),
                task_id: Some(inbound.message_id.clone()),
                call_id: None,
                session_id: None,
                thread_id: None,
            }),
        );

        let result = B
            .QuickResponse
            .with_client_registry(&client_registry)
            .call(prompt, history)
            .await;

        match result {
            Ok(output) => {
                trace.complete_call(
                    &trace_ctx,
                    &model_id,
                    None,
                    &serde_json::json!({ "output": output }),
                    "writer synthesis complete",
                );
                let trimmed = output.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(format!("\n<!-- writer_synthesis -->\n{trimmed}\n")))
                }
            }
            Err(error) => {
                trace.fail_call(
                    &trace_ctx,
                    &model_id,
                    None,
                    None,
                    &error.to_string(),
                    Some(shared_types::FailureKind::Unknown),
                );
                Err(WriterError::WriterLlmFailed(error.to_string()))
            }
        }
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
