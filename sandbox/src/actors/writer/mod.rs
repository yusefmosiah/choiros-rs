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
use crate::actors::run_writer::{
    OverlayAuthor, OverlayKind, PatchOp, PatchOpKind, RunWriterMsg, SectionState, VersionSource,
};
use crate::actors::terminal::{TerminalAgentProgress, TerminalAgentResult, TerminalMsg};
use crate::baml_client::{new_collector, B};
use crate::observability::llm_trace::{token_usage_from_collector, LlmCallScope, LlmTraceEmitter};

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
    envelope: WriterInboundEnvelope,
    run_writer_actor: ActorRef<RunWriterMsg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterInboundEnvelope {
    pub message_id: String,
    pub correlation_id: String,
    pub kind: String,
    pub run_id: String,
    pub section_id: String,
    pub source: WriterSource,
    pub content: String,
    pub base_version_id: Option<u64>,
    pub prompt_diff: Option<Vec<shared_types::PatchOp>>,
    pub overlay_id: Option<String>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub call_id: Option<String>,
    pub origin_actor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriterDelegationPlan {
    capability: Option<String>,
    objective: Option<String>,
    max_steps: Option<u8>,
}

#[derive(Debug, Clone)]
struct WriterDelegationDispatch {
    capability: WriterDelegateCapability,
    objective: String,
    max_steps: Option<u8>,
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
        envelope: WriterInboundEnvelope,
        run_writer_actor: ActorRef<RunWriterMsg>,
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
                envelope,
                run_writer_actor,
                reply,
            } => {
                let result = Self::enqueue_inbound(
                    &myself,
                    state,
                    WriterInboxMessage {
                        envelope,
                        run_writer_actor,
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
            source = inbound.envelope.source.as_str(),
            kind = inbound.envelope.kind.as_str(),
            id = inbound.envelope.message_id.as_str(),
            content = inbound.envelope.content.as_str()
        )
    }

    async fn enqueue_inbound(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        mut inbound: WriterInboxMessage,
    ) -> Result<WriterQueueAck, WriterError> {
        let has_prompt_diff = inbound
            .envelope
            .prompt_diff
            .as_ref()
            .map(|ops| !ops.is_empty())
            .unwrap_or(false);
        if inbound.envelope.content.trim().is_empty() && !has_prompt_diff {
            return Err(WriterError::Validation(
                "inbound content cannot be empty when prompt_diff is absent".to_string(),
            ));
        }

        if state
            .seen_message_ids
            .contains(&inbound.envelope.message_id)
        {
            return Ok(WriterQueueAck {
                message_id: inbound.envelope.message_id,
                accepted: true,
                duplicate: true,
                queue_len: state.inbox_queue.len(),
                revision: 0,
            });
        }

        let initial_revision = if inbound.envelope.source == WriterSource::User {
            let base_version_id = inbound.envelope.base_version_id.ok_or_else(|| {
                WriterError::Validation("base_version_id is required for user prompt".to_string())
            })?;
            let prompt_diff = inbound.envelope.prompt_diff.clone().ok_or_else(|| {
                WriterError::Validation("prompt_diff is required for user prompt".to_string())
            })?;
            if prompt_diff.is_empty() {
                return Err(WriterError::Validation(
                    "prompt_diff cannot be empty for user prompt".to_string(),
                ));
            }

            let overlay = ractor::call!(inbound.run_writer_actor.clone(), |reply| {
                RunWriterMsg::CreateOverlay {
                    run_id: inbound.envelope.run_id.clone(),
                    base_version_id,
                    author: OverlayAuthor::User,
                    kind: OverlayKind::Proposal,
                    diff_ops: prompt_diff,
                    reply,
                }
            })
            .map_err(|e| WriterError::RunWriterFailed(e.to_string()))?
            .map_err(|e| WriterError::RunWriterFailed(e.to_string()))?;
            inbound.envelope.overlay_id = Some(overlay.overlay_id);

            ractor::call!(inbound.run_writer_actor.clone(), |reply| {
                RunWriterMsg::GetRevision { reply }
            })
            .map_err(|e| WriterError::RunWriterFailed(e.to_string()))?
        } else {
            Self::apply_text(
                state,
                inbound.run_writer_actor.clone(),
                inbound.envelope.run_id.clone(),
                inbound.envelope.section_id.clone(),
                inbound.envelope.source,
                Self::format_inbox_note(&inbound),
                true,
            )
            .await?
        };

        Self::remember_message_id(state, &inbound.envelope.message_id);
        state.inbox_queue.push_back(inbound.clone());
        Self::emit_event(
            state,
            "writer.actor.inbox.enqueued",
            serde_json::json!({
                "run_id": inbound.envelope.run_id.clone(),
                "section_id": inbound.envelope.section_id.clone(),
                "source": inbound.envelope.source.as_str(),
                "kind": inbound.envelope.kind.clone(),
                "message_id": inbound.envelope.message_id.clone(),
                "queue_len": state.inbox_queue.len(),
                "revision": initial_revision,
                "correlation_id": inbound.envelope.correlation_id.clone(),
                "base_version_id": inbound.envelope.base_version_id,
                "overlay_id": inbound.envelope.overlay_id.clone(),
                "session_id": inbound.envelope.session_id.clone(),
                "thread_id": inbound.envelope.thread_id.clone(),
                "call_id": inbound.envelope.call_id.clone(),
                "origin_actor": inbound.envelope.origin_actor.clone(),
            }),
        );

        if !state.inbox_processing {
            let _ = myself.send_message(WriterMsg::ProcessInbox);
        }

        Ok(WriterQueueAck {
            message_id: inbound.envelope.message_id,
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

        let delegation_context = if inbound.envelope.source == WriterSource::User {
            Self::dispatch_user_prompt_delegation(myself, state, &inbound)
                .await
                .unwrap_or_default()
        } else {
            String::new()
        };

        let synthesis = Self::synthesize_with_llm(state, &inbound, delegation_context).await;
        match synthesis {
            Ok(Some(markdown)) => {
                let _ = Self::set_section_content(
                    state,
                    inbound.run_writer_actor.clone(),
                    inbound.envelope.run_id.clone(),
                    "conductor".to_string(),
                    WriterSource::Writer,
                    markdown,
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => {
                Self::emit_event(
                    state,
                    "writer.actor.inbox.synthesis_failed",
                    serde_json::json!({
                        "run_id": inbound.envelope.run_id.clone(),
                        "section_id": inbound.envelope.section_id.clone(),
                        "message_id": inbound.envelope.message_id.clone(),
                        "correlation_id": inbound.envelope.correlation_id.clone(),
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
        delegation_context: String,
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

        let message_content = if let Some(diff_ops) = inbound.envelope.prompt_diff.as_ref() {
            let diff_json = serde_json::to_string_pretty(diff_ops).unwrap_or_default();
            format!(
                "base_version_id: {}\noverlay_id: {}\nTyped diff intent (insert/delete/replace):\n{}",
                inbound
                    .envelope
                    .base_version_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                inbound
                    .envelope
                    .overlay_id
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "none".to_string()),
                diff_json
            )
        } else {
            inbound.envelope.content.clone()
        };

        let prompt = format!(
            "You are WriterActor.\n\
             Produce a full revised document body (single coherent narrative) for this run.\n\
             Use the new inbox message and current document context.\n\
             Requirements:\n\
             - prioritize readability for humans\n\
             - prefer concise paragraphs/bullets over rigid report templates\n\
             - preserve factual claims from the inbox message, but reconcile contradictions with existing document context\n\
             - if a new claim conflicts with earlier text, explicitly correct/supersede the earlier claim\n\
             - avoid duplicating stale or disproven claims\n\
             - produce a self-contained best-integrated revision (not just delta snippets)\n\
             - do not include section headers like 'Conductor/Researcher/Terminal/User'\n\
             - do not include markdown title '# ...' (title is handled separately)\n\
             - do not include proposal metadata or actor message wrappers\n\
             - output markdown only\n\n\
             Inbox message id: {message_id}\n\
             Message kind: {kind}\n\
             Message source: {source}\n\
             Message content:\n{content}",
            message_id = inbound.envelope.message_id.as_str(),
            kind = inbound.envelope.kind.as_str(),
            source = inbound.envelope.source.as_str(),
            content = message_content
        );
        let prompt = if delegation_context.trim().is_empty() {
            prompt
        } else {
            format!("{prompt}\n\nDelegation context:\n{delegation_context}")
        };
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
                "run_id": inbound.envelope.run_id.clone(),
                "section_id": inbound.envelope.section_id.clone(),
                "message_id": inbound.envelope.message_id.clone(),
                "kind": inbound.envelope.kind.clone(),
                "source": inbound.envelope.source.as_str(),
                "correlation_id": inbound.envelope.correlation_id.clone(),
                "session_id": inbound.envelope.session_id.clone(),
                "thread_id": inbound.envelope.thread_id.clone(),
                "call_id": inbound.envelope.call_id.clone(),
                "message_content": message_content,
                "history_excerpt": history,
            }),
            "Writer synthesizes queued inbound message",
            Some(LlmCallScope {
                run_id: Some(inbound.envelope.run_id.clone()),
                task_id: Some(inbound.envelope.message_id.clone()),
                call_id: inbound.envelope.call_id.clone(),
                session_id: inbound.envelope.session_id.clone(),
                thread_id: inbound.envelope.thread_id.clone(),
            }),
        );

        let collector = new_collector("writer.quick_response");
        let result = B
            .QuickResponse
            .with_client_registry(&client_registry)
            .with_collector(&collector)
            .call(prompt, history)
            .await;
        let usage = token_usage_from_collector(&collector);

        match result {
            Ok(output) => {
                trace.complete_call_with_usage(
                    &trace_ctx,
                    &model_id,
                    None,
                    &serde_json::json!({ "output": output }),
                    "writer synthesis complete",
                    usage.clone(),
                );
                let trimmed = output.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(error) => {
                trace.fail_call_with_usage(
                    &trace_ctx,
                    &model_id,
                    None,
                    None,
                    &error.to_string(),
                    Some(shared_types::FailureKind::Unknown),
                    usage,
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

    fn parse_json_object(raw: &str) -> Option<serde_json::Value> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            return Some(value);
        }
        let start = raw.find('{')?;
        let end = raw.rfind('}')?;
        if end <= start {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(&raw[start..=end]).ok()
    }

    async fn dispatch_user_prompt_delegation(
        myself: &ActorRef<WriterMsg>,
        state: &WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<String, WriterError> {
        let Some(dispatch) = Self::plan_user_prompt_delegation(state, inbound).await? else {
            return Ok(String::new());
        };
        let call_id = format!("writer-delegate:{}", ulid::Ulid::new());
        let capability_name = match dispatch.capability {
            WriterDelegateCapability::Researcher => "researcher",
            WriterDelegateCapability::Terminal => "terminal",
        };
        let section_id = capability_name.to_string();
        let queued_note = format!(
            "> [writer] delegated_{capability_name}_queued ({call_id})\n\
             Objective: {}\n\
             Status: queued\n",
            dispatch.objective
        );
        let _ = Self::apply_text(
            state,
            inbound.run_writer_actor.clone(),
            inbound.envelope.run_id.clone(),
            section_id.clone(),
            WriterSource::Writer,
            queued_note.clone(),
            true,
        )
        .await?;

        let writer_actor = myself.clone();
        let run_writer_actor = inbound.run_writer_actor.clone();
        let run_id = inbound.envelope.run_id.clone();
        let thread_id = inbound.envelope.thread_id.clone();
        let session_id = inbound.envelope.session_id.clone();
        let objective = dispatch.objective.clone();
        let max_steps = dispatch.max_steps;
        let capability = dispatch.capability;
        let correlation_id = inbound.envelope.correlation_id.clone();
        let call_id_for_task = call_id.clone();
        tokio::spawn(async move {
            let result = ractor::call!(writer_actor, |reply| WriterMsg::DelegateTask {
                capability,
                objective: objective.clone(),
                timeout_ms: Some(180_000),
                max_steps,
                run_id: Some(run_id.clone()),
                call_id: Some(call_id_for_task.clone()),
                reply,
            });

            let completion_content = match result {
                Ok(Ok(delegate_result)) => format!(
                    "Objective: {objective}\nSuccess: {}\nSummary: {}\n",
                    delegate_result.success, delegate_result.summary
                ),
                Ok(Err(error)) => {
                    format!("Objective: {objective}\nSuccess: false\nError: {error}\n")
                }
                Err(error) => format!("Objective: {objective}\nSuccess: false\nError: {error}\n"),
            };

            let completion_message = format!(
                "{prefix}\n{completion}",
                prefix = format!("> [writer] delegated_{capability_name}_completed ({call_id})"),
                completion = completion_content
            );
            let envelope = WriterInboundEnvelope {
                message_id: format!("{run_id}:{capability_name}:delegate_completion:{call_id}"),
                correlation_id,
                kind: format!("delegated_{capability_name}_completion"),
                run_id,
                section_id,
                source: WriterSource::Writer,
                content: completion_message,
                base_version_id: None,
                prompt_diff: None,
                overlay_id: None,
                session_id,
                thread_id,
                call_id: Some(call_id.clone()),
                origin_actor: Some("writer".to_string()),
            };
            let _ = ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
                envelope,
                run_writer_actor,
                reply,
            });
        });

        Ok(format!(
            "Delegation dispatched asynchronously:\n{queued_note}"
        ))
    }

    async fn plan_user_prompt_delegation(
        state: &WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<Option<WriterDelegationDispatch>, WriterError> {
        let resolved = state
            .model_registry
            .resolve_for_role("writer", &ModelResolutionContext::default())
            .map_err(|e| WriterError::ModelResolution(e.to_string()))?;
        let model_id = resolved.config.id.clone();
        let client_registry = state
            .model_registry
            .create_runtime_client_registry_for_model(&model_id)
            .map_err(|e| WriterError::ModelResolution(e.to_string()))?;

        let planner_input = if let Some(diff_ops) = inbound.envelope.prompt_diff.as_ref() {
            serde_json::to_string_pretty(diff_ops)
                .unwrap_or_else(|_| inbound.envelope.content.clone())
        } else {
            inbound.envelope.content.clone()
        };

        let planner_prompt = format!(
            "You are WriterActor planning whether to delegate work.\n\
             Decide if this human prompt requires new worker execution before revising the document.\n\
             Output strict JSON only with keys:\n\
             - capability: \"researcher\" | \"terminal\" | null\n\
             - objective: string | null\n\
             - max_steps: integer 1-100 | null\n\
             Rules:\n\
             - choose researcher when the prompt asks for new facts, links, topics, or verification\n\
             - choose terminal when the prompt asks for repo/code inspection or command execution\n\
             - choose null when the prompt is purely editorial (style, clarity, prose, structure)\n\
             - objective must be concise and actionable when capability is not null\n\
             Human prompt:\n{}",
            planner_input
        );

        let trace = LlmTraceEmitter::new(state.event_store.clone());
        let planner_task_id = format!("{}:planner", inbound.envelope.message_id);
        let trace_ctx = trace.start_call(
            "writer",
            "DelegationPlanner",
            &state.writer_id,
            &model_id,
            None,
            "Writer delegation planner",
            &serde_json::json!({
                "run_id": inbound.envelope.run_id.clone(),
                "section_id": inbound.envelope.section_id.clone(),
                "message_id": inbound.envelope.message_id.clone(),
                "kind": inbound.envelope.kind.clone(),
                "source": inbound.envelope.source.as_str(),
                "correlation_id": inbound.envelope.correlation_id.clone(),
                "planner_input": planner_input,
            }),
            "Writer decides whether to delegate researcher/terminal work",
            Some(LlmCallScope {
                run_id: Some(inbound.envelope.run_id.clone()),
                task_id: Some(planner_task_id),
                call_id: inbound.envelope.call_id.clone(),
                session_id: inbound.envelope.session_id.clone(),
                thread_id: inbound.envelope.thread_id.clone(),
            }),
        );

        let collector = new_collector("writer.delegation_planner");
        let planner_result = B
            .QuickResponse
            .with_client_registry(&client_registry)
            .with_collector(&collector)
            .call(planner_prompt, String::new())
            .await;
        let usage = token_usage_from_collector(&collector);

        let planner_output = match planner_result {
            Ok(output) => {
                trace.complete_call_with_usage(
                    &trace_ctx,
                    &model_id,
                    None,
                    &serde_json::json!({ "output": output }),
                    "writer delegation planner completed",
                    usage,
                );
                output
            }
            Err(e) => {
                trace.fail_call_with_usage(
                    &trace_ctx,
                    &model_id,
                    None,
                    None,
                    &e.to_string(),
                    Some(shared_types::FailureKind::Unknown),
                    usage,
                );
                return Err(WriterError::WriterLlmFailed(e.to_string()));
            }
        };
        let plan_value = Self::parse_json_object(&planner_output).ok_or_else(|| {
            WriterError::WriterLlmFailed("writer delegation planner returned invalid JSON".into())
        })?;
        let plan: WriterDelegationPlan = serde_json::from_value(plan_value)
            .map_err(|e| WriterError::WriterLlmFailed(e.to_string()))?;

        let Some(capability_raw) = plan
            .capability
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty() && v != "null" && v != "none")
        else {
            return Ok(None);
        };
        let objective = plan
            .objective
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                WriterError::Validation(
                    "writer delegation plan missing objective for non-null capability".to_string(),
                )
            })?;

        let capability = match capability_raw.as_str() {
            "researcher" => WriterDelegateCapability::Researcher,
            "terminal" => WriterDelegateCapability::Terminal,
            _ => return Ok(None),
        };
        let max_steps = plan.max_steps.map(|s| s.clamp(1, 100));
        Ok(Some(WriterDelegationDispatch {
            capability,
            objective,
            max_steps,
        }))
    }

    async fn set_section_content(
        state: &WriterState,
        run_writer_actor: ActorRef<RunWriterMsg>,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
    ) -> Result<u64, WriterError> {
        if content.trim().is_empty() {
            return Err(WriterError::Validation(
                "content cannot be empty".to_string(),
            ));
        }
        let parent_version_id = match ractor::call!(run_writer_actor.clone(), |reply| {
            RunWriterMsg::GetHeadVersion { reply }
        }) {
            Ok(Ok(version)) => version.version_id,
            Ok(Err(e)) => return Err(WriterError::RunWriterFailed(e.to_string())),
            Err(e) => return Err(WriterError::RunWriterFailed(e.to_string())),
        };

        let version_source = match source {
            WriterSource::Writer => VersionSource::Writer,
            WriterSource::User => VersionSource::UserSave,
            WriterSource::Researcher | WriterSource::Terminal | WriterSource::Conductor => {
                VersionSource::System
            }
        };

        match ractor::call!(run_writer_actor.clone(), |reply| {
            RunWriterMsg::CreateVersion {
                run_id: run_id.clone(),
                parent_version_id: Some(parent_version_id),
                content: content.clone(),
                source: version_source,
                reply,
            }
        }) {
            Ok(Ok(version)) => {
                let revision = match ractor::call!(run_writer_actor, |reply| {
                    RunWriterMsg::GetRevision { reply }
                }) {
                    Ok(revision) => revision,
                    Err(e) => return Err(WriterError::RunWriterFailed(e.to_string())),
                };
                Self::emit_event(
                    state,
                    "writer.actor.set_section_content",
                    serde_json::json!({
                        "run_id": run_id,
                        "section_id": section_id,
                        "source": source.as_str(),
                        "version_id": version.version_id,
                        "revision": revision,
                    }),
                );
                Ok(revision)
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
        let objective = format!(
            "{objective}\n\nWriter output contract:\n- Return concise diff intent only.\n- Prefer short additions/removals.\n- If broad changes are needed, return rewrite instructions for Writer (not a full rewritten draft)."
        );
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
