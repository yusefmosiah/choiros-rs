//! WriterActor - event-driven writing authority.
//!
//! Unlike researcher/terminal, WriterActor does not run a planning loop.
//! It reacts to typed actor messages from workers/humans and mutates run
//! documents through RunWriterActor. When multi-step work is needed, it can
//! delegate to researcher/terminal actors via typed actor messages.

mod adapter;

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::actors::agent_harness::{AgentHarness, HarnessConfig, ObjectiveStatus, ToolExecution};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress, ResearcherResult};
use crate::actors::run_writer::{
    DocumentVersion, Overlay, OverlayAuthor, OverlayKind, OverlayStatus, PatchOp, PatchOpKind,
    RunWriterActor, RunWriterArguments, RunWriterMsg, SectionState, VersionSource,
};
use crate::actors::terminal::{TerminalAgentProgress, TerminalAgentResult, TerminalMsg};
use crate::observability::llm_trace::LlmTraceEmitter;
use adapter::{WriterDelegationAdapter, WriterSynthesisAdapter};

#[derive(Debug, Default)]
pub struct WriterActor;

#[derive(Debug, Clone)]
pub struct WriterArguments {
    pub writer_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_actor: Option<ActorRef<ResearcherMsg>>,
    pub terminal_actor: Option<ActorRef<TerminalMsg>>,
    pub root_dir: Option<String>,
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
    run_writers_by_run_id: HashMap<String, ActorRef<RunWriterMsg>>,
    root_dir: String,
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
pub struct WriterQueueAck {
    pub message_id: String,
    pub accepted: bool,
    pub duplicate: bool,
    pub queue_len: usize,
    pub revision: u64,
}

#[derive(Debug)]
pub enum WriterMsg {
    /// Register a run_id -> RunWriterActor binding.
    RegisterRunWriter {
        run_id: String,
        run_writer_actor: ActorRef<RunWriterMsg>,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// List run document versions for a registered run.
    ListRunWriterVersions {
        run_id: String,
        reply: RpcReplyPort<Result<Vec<DocumentVersion>, WriterError>>,
    },
    /// Fetch a single run document version for a registered run.
    GetRunWriterVersion {
        run_id: String,
        version_id: u64,
        reply: RpcReplyPort<Result<DocumentVersion, WriterError>>,
    },
    /// List overlays for a registered run.
    ListRunWriterOverlays {
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
        reply: RpcReplyPort<Result<Vec<Overlay>, WriterError>>,
    },
    /// Create a canonical document version for a registered run.
    CreateRunWriterVersion {
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
        reply: RpcReplyPort<Result<DocumentVersion, WriterError>>,
    },
    /// Submit a user prompt diff into writer ingress for a run.
    SubmitUserPrompt {
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
        reply: RpcReplyPort<Result<WriterQueueAck, WriterError>>,
    },
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
            run_writers_by_run_id: HashMap::new(),
            root_dir: args
                .root_dir
                .unwrap_or_else(|| env!("CARGO_MANIFEST_DIR").to_string()),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            WriterMsg::RegisterRunWriter {
                run_id,
                run_writer_actor,
                reply,
            } => {
                state.run_writers_by_run_id.insert(run_id, run_writer_actor);
                let _ = reply.send(Ok(()));
            }
            WriterMsg::ListRunWriterVersions { run_id, reply } => {
                let result = Self::list_run_writer_versions(state, run_id).await;
                let _ = reply.send(result);
            }
            WriterMsg::GetRunWriterVersion {
                run_id,
                version_id,
                reply,
            } => {
                let result = Self::get_run_writer_version(state, run_id, version_id).await;
                let _ = reply.send(result);
            }
            WriterMsg::ListRunWriterOverlays {
                run_id,
                base_version_id,
                status,
                reply,
            } => {
                let result =
                    Self::list_run_writer_overlays(state, run_id, base_version_id, status).await;
                let _ = reply.send(result);
            }
            WriterMsg::CreateRunWriterVersion {
                run_id,
                parent_version_id,
                content,
                source,
                reply,
            } => {
                let result = Self::create_run_writer_version(
                    state,
                    run_id,
                    parent_version_id,
                    content,
                    source,
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::SubmitUserPrompt {
                run_id,
                prompt_diff,
                base_version_id,
                reply,
            } => {
                let result =
                    Self::submit_user_prompt(&myself, state, run_id, prompt_diff, base_version_id)
                        .await;
                let _ = reply.send(result);
            }
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
                    &myself, state, capability, objective, timeout_ms, max_steps, run_id, call_id,
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

    fn resolve_run_writer(
        state: &WriterState,
        run_id: &str,
    ) -> Result<ActorRef<RunWriterMsg>, WriterError> {
        state
            .run_writers_by_run_id
            .get(run_id)
            .cloned()
            .ok_or_else(|| {
                WriterError::Validation(format!("run writer not found for run_id={run_id}"))
            })
    }

    fn run_writer_state_exists(state: &WriterState, run_id: &str) -> bool {
        let run_dir = PathBuf::from(&state.root_dir)
            .join("conductor")
            .join("runs")
            .join(run_id);
        run_dir.join("draft.writer-state.json").exists() || run_dir.join("draft.md").exists()
    }

    async fn ensure_run_writer(
        state: &mut WriterState,
        run_id: &str,
    ) -> Result<ActorRef<RunWriterMsg>, WriterError> {
        if let Some(existing) = state.run_writers_by_run_id.get(run_id).cloned() {
            return Ok(existing);
        }

        let actor_name = format!("run-writer-{run_id}");
        if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
            let actor_ref: ActorRef<RunWriterMsg> = cell.into();
            state
                .run_writers_by_run_id
                .insert(run_id.to_string(), actor_ref.clone());
            return Ok(actor_ref);
        }

        if !Self::run_writer_state_exists(state, run_id) {
            return Err(WriterError::Validation(format!(
                "run writer not found for run_id={run_id}"
            )));
        }

        let (actor_ref, _handle) = ractor::Actor::spawn(
            Some(actor_name),
            RunWriterActor,
            RunWriterArguments {
                run_id: run_id.to_string(),
                desktop_id: run_id.to_string(),
                objective: String::new(),
                session_id: run_id.to_string(),
                thread_id: run_id.to_string(),
                root_dir: Some(state.root_dir.clone()),
                event_store: state.event_store.clone(),
            },
        )
        .await
        .map_err(|e| {
            WriterError::ActorUnavailable(format!(
                "failed to restore run writer for run_id={run_id}: {e}"
            ))
        })?;

        state
            .run_writers_by_run_id
            .insert(run_id.to_string(), actor_ref.clone());
        Self::emit_event(
            state,
            "writer.actor.run_writer.restored",
            serde_json::json!({
                "run_id": run_id,
                "actor_name": format!("run-writer-{run_id}"),
            }),
        );

        Ok(actor_ref)
    }

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
        state.run_writers_by_run_id.insert(
            inbound.envelope.run_id.clone(),
            inbound.run_writer_actor.clone(),
        );

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

    fn build_synthesis_objective(
        inbound: &WriterInboxMessage,
        message_content: &str,
        history: &str,
        delegation_context: &str,
    ) -> String {
        let mut objective = format!(
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
             - output markdown only\n\
             - do not call tools; return the revised markdown in the final message\n\
             \n\
             Inbox message id: {message_id}\n\
             Message kind: {kind}\n\
             Message source: {source}\n\
             Message content:\n{message_content}\n\
             \n\
             Current document excerpt:\n{history}",
            message_id = inbound.envelope.message_id,
            kind = inbound.envelope.kind,
            source = inbound.envelope.source.as_str(),
            message_content = message_content,
            history = history
        );
        if !delegation_context.trim().is_empty() {
            objective.push_str("\n\nDelegation context:\n");
            objective.push_str(delegation_context);
        }
        objective
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
        let objective = Self::build_synthesis_objective(
            inbound,
            &message_content,
            &history,
            &delegation_context,
        );

        let adapter = WriterSynthesisAdapter::new(
            state.writer_id.clone(),
            state.user_id.clone(),
            state.event_store.clone(),
        );
        let harness = AgentHarness::with_config(
            adapter,
            state.model_registry.clone(),
            HarnessConfig {
                timeout_budget_ms: 90_000,
                max_steps: 3,
                emit_progress: true,
                emit_worker_report: true,
            },
            LlmTraceEmitter::new(state.event_store.clone()),
        );

        let result = harness
            .run(
                format!(
                    "{}:synthesis:{}",
                    state.writer_id, inbound.envelope.message_id
                ),
                state.user_id.clone(),
                objective,
                None,
                None,
                Some(inbound.envelope.run_id.clone()),
                inbound.envelope.call_id.clone(),
            )
            .await;

        match result {
            Ok(agent_result) => {
                if agent_result.objective_status == ObjectiveStatus::Blocked {
                    return Err(WriterError::WriterLlmFailed(agent_result.completion_reason));
                }
                let trimmed = agent_result.summary.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(error) => Err(WriterError::WriterLlmFailed(error.to_string())),
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

    async fn list_run_writer_versions(
        state: &mut WriterState,
        run_id: String,
    ) -> Result<Vec<DocumentVersion>, WriterError> {
        let run_writer = Self::ensure_run_writer(state, &run_id).await?;
        match ractor::call!(run_writer, |reply| RunWriterMsg::ListVersions { reply }) {
            Ok(Ok(versions)) => Ok(versions),
            Ok(Err(error)) => Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => Err(WriterError::RunWriterFailed(error.to_string())),
        }
    }

    async fn get_run_writer_version(
        state: &mut WriterState,
        run_id: String,
        version_id: u64,
    ) -> Result<DocumentVersion, WriterError> {
        let run_writer = Self::ensure_run_writer(state, &run_id).await?;
        match ractor::call!(run_writer, |reply| RunWriterMsg::GetVersion {
            version_id,
            reply
        }) {
            Ok(Ok(version)) => Ok(version),
            Ok(Err(error)) => Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => Err(WriterError::RunWriterFailed(error.to_string())),
        }
    }

    async fn list_run_writer_overlays(
        state: &mut WriterState,
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
    ) -> Result<Vec<Overlay>, WriterError> {
        let run_writer = Self::ensure_run_writer(state, &run_id).await?;
        match ractor::call!(run_writer, |reply| RunWriterMsg::ListOverlays {
            base_version_id,
            status,
            reply
        }) {
            Ok(Ok(overlays)) => Ok(overlays),
            Ok(Err(error)) => Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => Err(WriterError::RunWriterFailed(error.to_string())),
        }
    }

    async fn create_run_writer_version(
        state: &mut WriterState,
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
    ) -> Result<DocumentVersion, WriterError> {
        let run_writer = Self::ensure_run_writer(state, &run_id).await?;
        match ractor::call!(run_writer, |reply| RunWriterMsg::CreateVersion {
            run_id,
            parent_version_id,
            content,
            source,
            reply
        }) {
            Ok(Ok(version)) => Ok(version),
            Ok(Err(error)) => Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => Err(WriterError::RunWriterFailed(error.to_string())),
        }
    }

    async fn submit_user_prompt(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
    ) -> Result<WriterQueueAck, WriterError> {
        if prompt_diff.is_empty() {
            return Err(WriterError::Validation(
                "prompt_diff cannot be empty".to_string(),
            ));
        }
        let run_writer_actor = Self::ensure_run_writer(state, &run_id).await?;

        let head = match ractor::call!(run_writer_actor.clone(), |reply| {
            RunWriterMsg::GetHeadVersion { reply }
        }) {
            Ok(Ok(head)) => head,
            Ok(Err(error)) => return Err(WriterError::RunWriterFailed(error.to_string())),
            Err(error) => return Err(WriterError::RunWriterFailed(error.to_string())),
        };
        if base_version_id != head.version_id {
            return Err(WriterError::Validation(format!(
                "stale base_version_id: expected {}, got {}",
                head.version_id, base_version_id
            )));
        }

        let envelope = WriterInboundEnvelope {
            message_id: format!("{run_id}:user:prompt:{}", ulid::Ulid::new()),
            correlation_id: format!("{run_id}:{}", ulid::Ulid::new()),
            kind: "human_prompt".to_string(),
            run_id,
            section_id: "user".to_string(),
            source: WriterSource::User,
            content: "user_prompt_diff".to_string(),
            base_version_id: Some(base_version_id),
            prompt_diff: Some(prompt_diff),
            overlay_id: None,
            session_id: None,
            thread_id: None,
            call_id: None,
            origin_actor: Some("conductor".to_string()),
        };
        Self::enqueue_inbound(
            myself,
            state,
            WriterInboxMessage {
                envelope,
                run_writer_actor,
            },
        )
        .await
    }

    fn build_delegation_objective(inbound: &WriterInboxMessage) -> String {
        let prompt_payload = if let Some(diff_ops) = inbound.envelope.prompt_diff.as_ref() {
            serde_json::to_string_pretty(diff_ops)
                .unwrap_or_else(|_| inbound.envelope.content.clone())
        } else {
            inbound.envelope.content.clone()
        };

        format!(
            "Determine whether Writer should delegate before revising this run document.\n\
             Delegate only if additional execution is required.\n\
             Use message_writer tool with one of:\n\
             - mode: \"delegate_researcher\" for facts, links, verification, or web research\n\
             - mode: \"delegate_terminal\" for repository inspection, shell commands, or local execution\n\
             In both cases, set content to a concise objective for the delegated worker.\n\
             If no delegation is needed, return no tool calls and explain why in message.\n\
             \n\
             Run ID: {}\n\
             Inbox Message ID: {}\n\
             Prompt Payload:\n{}",
            inbound.envelope.run_id, inbound.envelope.message_id, prompt_payload
        )
    }

    fn extract_capability_from_tool_output(output: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(output).ok()?;
        value
            .get("capability")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
    }

    fn summarize_delegation_calls(tool_executions: &[ToolExecution]) -> String {
        let mut lines = Vec::new();
        for execution in tool_executions
            .iter()
            .filter(|exec| exec.tool_name == "message_writer")
        {
            let capability = Self::extract_capability_from_tool_output(&execution.output)
                .unwrap_or_else(|| "unknown".to_string());
            let summary = serde_json::from_str::<serde_json::Value>(&execution.output)
                .ok()
                .and_then(|value| {
                    value
                        .get("summary")
                        .and_then(|summary| summary.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| execution.output.clone());
            lines.push(format!(
                "- capability={capability} success={} summary={summary}",
                execution.success
            ));
        }
        if lines.is_empty() {
            "none".to_string()
        } else {
            lines.join("\n")
        }
    }

    fn delegation_section_for_result(tool_executions: &[ToolExecution]) -> String {
        for execution in tool_executions
            .iter()
            .filter(|exec| exec.tool_name == "message_writer" && exec.success)
        {
            if let Some(capability) = Self::extract_capability_from_tool_output(&execution.output) {
                return capability;
            }
        }
        "conductor".to_string()
    }

    async fn dispatch_user_prompt_delegation(
        myself: &ActorRef<WriterMsg>,
        state: &WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<String, WriterError> {
        let delegation_call_id = format!("writer-delegation:{}", ulid::Ulid::new());
        let objective = Self::build_delegation_objective(inbound);
        let writer_actor = myself.clone();
        let run_writer_actor = inbound.run_writer_actor.clone();
        let seed_envelope = inbound.envelope.clone();
        let writer_id = state.writer_id.clone();
        let user_id = state.user_id.clone();
        let event_store = state.event_store.clone();
        let model_registry = state.model_registry.clone();
        let adapter = WriterDelegationAdapter::new(
            writer_id.clone(),
            user_id.clone(),
            event_store.clone(),
            writer_actor.clone(),
            state.researcher_actor.is_some(),
            state.terminal_actor.is_some(),
        );
        let trace_emitter = LlmTraceEmitter::new(event_store.clone());
        let harness = AgentHarness::with_config(
            adapter,
            model_registry,
            HarnessConfig {
                timeout_budget_ms: 180_000,
                max_steps: 100,
                emit_progress: true,
                emit_worker_report: true,
            },
            trace_emitter,
        );

        Self::emit_event(
            state,
            "writer.actor.delegation_harness.dispatched",
            serde_json::json!({
                "run_id": seed_envelope.run_id.clone(),
                "message_id": seed_envelope.message_id.clone(),
                "correlation_id": seed_envelope.correlation_id.clone(),
                "delegation_call_id": delegation_call_id.clone(),
            }),
        );

        tokio::spawn(async move {
            let harness_result = harness
                .run(
                    format!("{writer_id}:{delegation_call_id}"),
                    user_id,
                    objective.clone(),
                    None,
                    None,
                    Some(seed_envelope.run_id.clone()),
                    Some(delegation_call_id.clone()),
                )
                .await;

            let (summary, status, tool_executions, success, should_enqueue) = match harness_result {
                Ok(result) => {
                    let tool_executions = result.tool_executions.clone();
                    let should_enqueue = tool_executions
                        .iter()
                        .any(|exec| exec.tool_name == "message_writer");
                    (
                        result.summary,
                        result.objective_status,
                        tool_executions,
                        result.success,
                        should_enqueue,
                    )
                }
                Err(error) => (
                    format!("Writer delegation harness failed: {error}"),
                    ObjectiveStatus::Blocked,
                    Vec::new(),
                    false,
                    true,
                ),
            };

            if !should_enqueue {
                return;
            }

            let completion_message = format!(
                "> [writer] delegation_harness_completed ({delegation_call_id})\n\
                 Objective: {objective}\n\
                 Success: {success}\n\
                 Status: {status:?}\n\
                 Summary: {summary}\n\
                 Delegation Calls:\n{}\n",
                Self::summarize_delegation_calls(&tool_executions)
            );
            let section_id = Self::delegation_section_for_result(&tool_executions);
            let completion_envelope = WriterInboundEnvelope {
                message_id: format!(
                    "{}:writer:delegation_harness_completion:{}",
                    seed_envelope.run_id.clone(),
                    delegation_call_id
                ),
                correlation_id: seed_envelope.correlation_id.clone(),
                kind: "delegation_harness_completion".to_string(),
                run_id: seed_envelope.run_id.clone(),
                section_id,
                source: WriterSource::Writer,
                content: completion_message,
                base_version_id: None,
                prompt_diff: None,
                overlay_id: None,
                session_id: seed_envelope.session_id.clone(),
                thread_id: seed_envelope.thread_id.clone(),
                call_id: Some(delegation_call_id),
                origin_actor: Some("writer".to_string()),
            };
            let _ = ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
                envelope: completion_envelope,
                run_writer_actor,
                reply,
            });
        });

        Ok("Delegation harness dispatched asynchronously.".to_string())
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
        myself: &ActorRef<WriterMsg>,
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
                let run_writer_actor = run_id
                    .as_ref()
                    .and_then(|rid| state.run_writers_by_run_id.get(rid).cloned());
                let (tx, _rx) = mpsc::unbounded_channel::<ResearcherProgress>();
                let result =
                    ractor::call!(researcher_actor, |reply| ResearcherMsg::RunAgenticTask {
                        objective: objective.clone(),
                        timeout_ms,
                        max_results: Some(8),
                        max_rounds: max_steps,
                        model_override: None,
                        progress_tx: Some(tx),
                        writer_actor: Some(myself.clone()),
                        run_writer_actor,
                        run_id: run_id.clone(),
                        call_id: call_id.clone(),
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
                    run_id: run_id.clone(),
                    call_id: call_id.clone(),
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
