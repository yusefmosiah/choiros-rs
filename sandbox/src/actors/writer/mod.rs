//! WriterActor - event-driven writing authority.
//!
//! Unlike researcher/terminal, WriterActor does not run a planning loop.
//! It reacts to typed actor messages from workers/humans and mutates run
//! documents through in-process run-document runtime state. When multi-step work is needed, it can
//! delegate to researcher/terminal actors via typed actor messages.

mod adapter;
pub mod document_runtime;

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::actors::agent_harness::{AgentHarness, HarnessConfig, ObjectiveStatus, ToolExecution};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress};
use crate::actors::terminal::{ensure_terminal_started, TerminalAgentProgress, TerminalMsg};
use crate::observability::llm_trace::LlmTraceEmitter;
use crate::supervisor::researcher::ResearcherSupervisorMsg;
use crate::supervisor::terminal::TerminalSupervisorMsg;
use adapter::{WriterDelegationAdapter, WriterSynthesisAdapter};
pub use document_runtime::{
    ApplyPatchResult, DocumentVersion, Overlay, OverlayAuthor, OverlayKind, OverlayStatus, PatchOp,
    PatchOpKind, RunDocument, SectionState, VersionSource, WriterDocumentArguments,
    WriterDocumentError, WriterDocumentRuntime,
};

#[derive(Debug, Default)]
pub struct WriterActor;

#[derive(Debug, Clone)]
pub struct WriterArguments {
    pub writer_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    pub terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
}

pub struct WriterState {
    writer_id: String,
    user_id: String,
    event_store: ActorRef<EventStoreMsg>,
    researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
    model_registry: ModelRegistry,
    inbox_queue: VecDeque<WriterInboxMessage>,
    seen_message_ids: HashSet<String>,
    seen_order: VecDeque<String>,
    inbox_processing: bool,
    run_documents_by_run_id: HashMap<String, WriterDocumentRuntime>,
    /// Confirmed citation stubs per run_id — populated on DelegationWorkerCompleted,
    /// used to populate .qwy citation_registry on version save (Phase 3.5).
    confirmed_citations_by_run_id: HashMap<String, Vec<ProposedCitationStub>>,
}

#[derive(Debug, Clone)]
struct WriterInboxMessage {
    envelope: WriterInboundEnvelope,
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
    /// Ensure run-scoped writer document exists for run_id.
    EnsureRunDocument {
        run_id: String,
        desktop_id: String,
        objective: String,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// List run document versions for a registered run.
    ListWriterDocumentVersions {
        run_id: String,
        reply: RpcReplyPort<Result<Vec<DocumentVersion>, WriterError>>,
    },
    /// Fetch a single run document version for a registered run.
    GetWriterDocumentVersion {
        run_id: String,
        version_id: u64,
        reply: RpcReplyPort<Result<DocumentVersion, WriterError>>,
    },
    /// List overlays for a registered run.
    ListWriterDocumentOverlays {
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
        reply: RpcReplyPort<Result<Vec<Overlay>, WriterError>>,
    },
    /// Dismiss a pending overlay for a registered run.
    DismissWriterDocumentOverlay {
        run_id: String,
        overlay_id: String,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// Create a canonical document version for a registered run.
    CreateWriterDocumentVersion {
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
    /// Apply text to a run section in run-document state.
    ApplyText {
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
        proposal: bool,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Emit non-mutating progress for a run section.
    ReportProgress {
        run_id: String,
        section_id: String,
        source: WriterSource,
        phase: String,
        message: String,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Update section state for writer UX.
    SetSectionState {
        run_id: String,
        section_id: String,
        state: SectionState,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// Append a human comment into `user` proposal context.
    ApplyHumanComment {
        run_id: String,
        comment: String,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Queue an inbound worker/human message for writer-agent synthesis.
    ///
    /// Control flow uses this actor message path; EventStore remains trace-only.
    EnqueueInbound {
        envelope: WriterInboundEnvelope,
        reply: RpcReplyPort<Result<WriterQueueAck, WriterError>>,
    },
    /// Queue inbound without waiting for ack (used by background tasks).
    EnqueueInboundAsync { envelope: WriterInboundEnvelope },
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
    /// Internal completion signal from asynchronously dispatched worker delegations.
    DelegationWorkerCompleted {
        capability: WriterDelegateCapability,
        run_id: Option<String>,
        call_id: Option<String>,
        dispatch_id: String,
        result: Result<WriterDelegateResult, WriterError>,
    },
    /// Delegate an objective through Writer's own planner, which may call workers.
    OrchestrateObjective {
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        run_id: Option<String>,
        call_id: Option<String>,
        reply: RpcReplyPort<Result<WriterOrchestrationResult, WriterError>>,
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

/// Minimal citation stub passed from researcher to writer for confirmation events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedCitationStub {
    pub citation_id: String,
    /// "external_url" | "version_snapshot" | "qwy_block" etc.
    pub cited_kind: String,
    /// URL, artifact path, or block_id
    pub cited_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterDelegateResult {
    pub capability: WriterDelegateCapability,
    pub success: bool,
    pub summary: String,
    /// Citation stubs proposed during this delegation run (for writer confirmation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_citation_ids: Vec<String>,
    /// Full stubs for external content publish trigger (3.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_citation_stubs: Vec<ProposedCitationStub>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterOrchestrationResult {
    pub success: bool,
    pub summary: String,
    pub delegated_capabilities: Vec<String>,
    pub pending_delegations: usize,
}

/// Context bundle for [`WriterActor::spawn_changeset_summarization`].
/// Collects the arguments into a single struct to stay within clippy's
/// `too_many_arguments` limit (7).
struct ChangesetSummarizationCtx {
    event_store: ActorRef<EventStoreMsg>,
    model_registry: ModelRegistry,
    run_id: String,
    desktop_id: String,
    source: String,
    before_content: String,
    after_content: String,
    target_version_id: u64,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum WriterError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("actor unavailable: {0}")]
    ActorUnavailable(String),
    #[error("worker failed: {0}")]
    WorkerFailed(String),
    #[error("document runtime failed: {0}")]
    WriterDocumentFailed(String),
    #[error("model resolution failed: {0}")]
    ModelResolution(String),
    #[error("writer llm failed: {0}")]
    WriterLlmFailed(String),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_delegate_capability(
    writer_id: &str,
    user_id: &str,
    writer_actor: &ActorRef<WriterMsg>,
    researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
    terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
    capability: WriterDelegateCapability,
    objective: String,
    timeout_ms: Option<u64>,
    max_steps: Option<u8>,
    run_id: Option<String>,
    call_id: Option<String>,
) -> Result<WriterDelegateResult, WriterError> {
    let delegate_key = call_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| ulid::Ulid::new().to_string());
    let objective = format!(
        "{objective}\n\nWriter output contract:\n- Return concise diff intent only.\n- Prefer short additions/removals.\n- If broad changes are needed, return rewrite instructions for Writer (not a full rewritten draft)."
    );

    let worker_call_id = Some(match call_id.clone() {
        Some(base) if !base.trim().is_empty() => format!("{base}:{delegate_key}"),
        _ => delegate_key.clone(),
    });

    match capability {
        WriterDelegateCapability::Researcher => {
            let researcher_supervisor = researcher_supervisor.ok_or_else(|| {
                WriterError::ActorUnavailable("researcher supervisor unavailable".to_string())
            })?;
            let writer_actor = writer_actor.clone();
            let writer_id = writer_id.to_string();
            let user_id = user_id.to_string();
            let objective = objective.clone();
            let run_id_for_task = run_id.clone();
            let call_id_for_task = worker_call_id.clone();
            let dispatch_id = delegate_key.clone();

            tokio::spawn(async move {
                let researcher_id = format!("writer-researcher:{writer_id}:{dispatch_id}");
                let _dispatch_result = async {
                    let researcher_actor = ractor::call!(researcher_supervisor, |reply| {
                        ResearcherSupervisorMsg::GetOrCreateResearcher {
                            researcher_id: researcher_id.clone(),
                            user_id: user_id.clone(),
                            reply,
                        }
                    })
                    .map_err(|e| WriterError::ActorUnavailable(e.to_string()))?
                    .map_err(WriterError::ActorUnavailable)?;

                    let (tx, _rx) = mpsc::unbounded_channel::<ResearcherProgress>();
                    researcher_actor
                        .send_message(ResearcherMsg::RunAgenticTaskDetached {
                            objective: objective.clone(),
                            timeout_ms,
                            max_results: Some(8),
                            max_rounds: max_steps,
                            model_override: None,
                            progress_tx: Some(tx),
                            writer_actor: Some(writer_actor.clone()),
                            run_id: run_id_for_task.clone(),
                            call_id: call_id_for_task.clone(),
                        })
                        .map_err(|e| WriterError::WorkerFailed(e.to_string()))?;
                    Ok::<(), WriterError>(())
                }
                .await;

                if let Err(error) = _dispatch_result {
                    let _ = writer_actor.send_message(WriterMsg::DelegationWorkerCompleted {
                        capability: WriterDelegateCapability::Researcher,
                        run_id: run_id_for_task.clone(),
                        call_id: call_id_for_task.clone(),
                        dispatch_id: dispatch_id.clone(),
                        result: Err(error),
                    });
                }
            });

            Ok(WriterDelegateResult {
                capability: WriterDelegateCapability::Researcher,
                success: true,
                summary: format!(
                    "Researcher delegation dispatched asynchronously ({delegate_key})"
                ),
                proposed_citation_ids: vec![],
                proposed_citation_stubs: vec![],
            })
        }
        WriterDelegateCapability::Terminal => {
            let terminal_supervisor = terminal_supervisor.ok_or_else(|| {
                WriterError::ActorUnavailable("terminal supervisor unavailable".to_string())
            })?;
            let writer_actor = writer_actor.clone();
            let writer_id = writer_id.to_string();
            let user_id = user_id.to_string();
            let objective = objective.clone();
            let run_id_for_task = run_id.clone();
            let call_id_for_task = worker_call_id.clone();
            let dispatch_id = delegate_key.clone();

            tokio::spawn(async move {
                let terminal_id = format!("writer-terminal:{writer_id}:{dispatch_id}");
                let _dispatch_result = async {
                    let terminal_actor = ractor::call!(terminal_supervisor, |reply| {
                        TerminalSupervisorMsg::GetOrCreateTerminal {
                            terminal_id: terminal_id.clone(),
                            user_id: user_id.clone(),
                            shell: "/bin/zsh".to_string(),
                            working_dir: env!("CARGO_MANIFEST_DIR").to_string(),
                            reply,
                        }
                    })
                    .map_err(|e| WriterError::ActorUnavailable(e.to_string()))?
                    .map_err(WriterError::ActorUnavailable)?;
                    ensure_terminal_started(&terminal_actor)
                        .await
                        .map_err(WriterError::WorkerFailed)?;

                    let (tx, _rx) = mpsc::unbounded_channel::<TerminalAgentProgress>();
                    terminal_actor
                        .send_message(TerminalMsg::RunAgenticTaskDetached {
                            objective: objective.clone(),
                            timeout_ms,
                            max_steps,
                            model_override: None,
                            progress_tx: Some(tx),
                            writer_actor: Some(writer_actor.clone()),
                            run_id: run_id_for_task.clone(),
                            call_id: call_id_for_task.clone(),
                        })
                        .map_err(|e| WriterError::WorkerFailed(e.to_string()))?;
                    Ok::<(), WriterError>(())
                }
                .await;

                if let Err(error) = _dispatch_result {
                    let _ = writer_actor.send_message(WriterMsg::DelegationWorkerCompleted {
                        capability: WriterDelegateCapability::Terminal,
                        run_id: run_id_for_task.clone(),
                        call_id: call_id_for_task.clone(),
                        dispatch_id: dispatch_id.clone(),
                        result: Err(error),
                    });
                }
            });

            Ok(WriterDelegateResult {
                capability: WriterDelegateCapability::Terminal,
                success: true,
                summary: format!("Terminal delegation dispatched asynchronously ({delegate_key})"),
                proposed_citation_ids: vec![],
                proposed_citation_stubs: vec![],
            })
        }
    }
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
            researcher_supervisor: args.researcher_supervisor,
            terminal_supervisor: args.terminal_supervisor,
            model_registry: ModelRegistry::new(),
            inbox_queue: VecDeque::new(),
            seen_message_ids: HashSet::new(),
            seen_order: VecDeque::new(),
            inbox_processing: false,
            run_documents_by_run_id: HashMap::new(),
            confirmed_citations_by_run_id: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            WriterMsg::EnsureRunDocument {
                run_id,
                desktop_id,
                objective,
                reply,
            } => {
                let result = Self::ensure_run_document(state, run_id, desktop_id, objective).await;
                let _ = reply.send(result);
            }
            WriterMsg::ListWriterDocumentVersions { run_id, reply } => {
                let result = Self::list_writer_document_versions(state, run_id).await;
                let _ = reply.send(result);
            }
            WriterMsg::GetWriterDocumentVersion {
                run_id,
                version_id,
                reply,
            } => {
                let result = Self::get_writer_document_version(state, run_id, version_id).await;
                let _ = reply.send(result);
            }
            WriterMsg::ListWriterDocumentOverlays {
                run_id,
                base_version_id,
                status,
                reply,
            } => {
                let result =
                    Self::list_writer_document_overlays(state, run_id, base_version_id, status)
                        .await;
                let _ = reply.send(result);
            }
            WriterMsg::DismissWriterDocumentOverlay {
                run_id,
                overlay_id,
                reply,
            } => {
                let result = Self::dismiss_writer_document_overlay(state, run_id, overlay_id).await;
                let _ = reply.send(result);
            }
            WriterMsg::CreateWriterDocumentVersion {
                run_id,
                parent_version_id,
                content,
                source,
                reply,
            } => {
                let result = Self::create_writer_document_version(
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
                run_id,
                section_id,
                source,
                content,
                proposal,
                reply,
            } => {
                let result =
                    Self::apply_text(state, run_id, section_id, source, content, proposal).await;
                let _ = reply.send(result);
            }
            WriterMsg::ReportProgress {
                run_id,
                section_id,
                source,
                phase,
                message,
                reply,
            } => {
                let result =
                    Self::report_progress(state, run_id, section_id, source, phase, message).await;
                let _ = reply.send(result);
            }
            WriterMsg::SetSectionState {
                run_id,
                section_id,
                state: section_state,
                reply,
            } => {
                let result =
                    Self::set_section_state(state, run_id, section_id, section_state).await;
                let _ = reply.send(result);
            }
            WriterMsg::ApplyHumanComment {
                run_id,
                comment,
                reply,
            } => {
                let result = Self::apply_text(
                    state,
                    run_id,
                    "user".to_string(),
                    WriterSource::User,
                    comment,
                    true,
                )
                .await;
                let _ = reply.send(result);
            }
            WriterMsg::EnqueueInbound { envelope, reply } => {
                let result =
                    Self::enqueue_inbound(&myself, state, WriterInboxMessage { envelope }).await;
                let _ = reply.send(result);
            }
            WriterMsg::EnqueueInboundAsync { envelope } => {
                if let Err(error) =
                    Self::enqueue_inbound(&myself, state, WriterInboxMessage { envelope }).await
                {
                    Self::emit_event(
                        state,
                        "writer.actor.enqueue_inbound_async.failed",
                        serde_json::json!({
                            "error": error.to_string(),
                        }),
                    );
                }
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
            WriterMsg::DelegationWorkerCompleted {
                capability,
                run_id,
                call_id,
                dispatch_id,
                result,
            } => {
                Self::handle_delegation_worker_completed(
                    &myself,
                    state,
                    capability,
                    run_id,
                    call_id,
                    dispatch_id,
                    result,
                )
                .await;
            }
            WriterMsg::OrchestrateObjective {
                objective,
                timeout_ms,
                max_steps,
                run_id,
                call_id,
                reply,
            } => {
                let result = Self::orchestrate_objective(
                    &myself, state, objective, timeout_ms, max_steps, run_id, call_id,
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
    const RUN_DOCUMENTS_ROOT: &'static str = "conductor/runs";
    const RUN_DOCUMENT_FILE: &'static str = "draft.md";
    const RUN_DOCUMENT_STATE_FILE: &'static str = "draft.writer-state.json";
    const DEFAULT_RESTORED_DESKTOP_ID: &'static str = "default-desktop";

    fn run_document_dir(run_id: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(Self::RUN_DOCUMENTS_ROOT)
            .join(run_id)
    }

    fn persisted_run_document_exists(run_id: &str) -> bool {
        let run_dir = Self::run_document_dir(run_id);
        run_dir.join(Self::RUN_DOCUMENT_FILE).exists()
            || run_dir.join(Self::RUN_DOCUMENT_STATE_FILE).exists()
    }

    async fn ensure_run_document_loaded(
        state: &mut WriterState,
        run_id: &str,
    ) -> Result<(), WriterError> {
        if state.run_documents_by_run_id.contains_key(run_id) {
            return Ok(());
        }

        if !Self::persisted_run_document_exists(run_id) {
            return Err(WriterError::Validation(format!(
                "document runtime not found for run_id={run_id}"
            )));
        }

        let runtime = WriterDocumentRuntime::load(WriterDocumentArguments {
            run_id: run_id.to_string(),
            desktop_id: Self::DEFAULT_RESTORED_DESKTOP_ID.to_string(),
            objective: String::new(),
            session_id: Self::DEFAULT_RESTORED_DESKTOP_ID.to_string(),
            thread_id: run_id.to_string(),
            root_dir: Some(env!("CARGO_MANIFEST_DIR").to_string()),
            event_store: state.event_store.clone(),
        })
        .await
        .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;

        let revision = runtime.revision();
        state
            .run_documents_by_run_id
            .insert(run_id.to_string(), runtime);
        Self::emit_event(
            state,
            "writer.actor.run_document.hydrated",
            serde_json::json!({
                "run_id": run_id,
                "revision": revision,
                "source": "persisted_state",
            }),
        );
        Ok(())
    }

    async fn ensure_run_document(
        state: &mut WriterState,
        run_id: String,
        desktop_id: String,
        objective: String,
    ) -> Result<(), WriterError> {
        if state.run_documents_by_run_id.contains_key(&run_id) {
            return Ok(());
        }
        let runtime = WriterDocumentRuntime::load(WriterDocumentArguments {
            run_id: run_id.clone(),
            desktop_id: desktop_id.clone(),
            objective: objective.clone(),
            session_id: desktop_id,
            thread_id: run_id.clone(),
            root_dir: Some(env!("CARGO_MANIFEST_DIR").to_string()),
            event_store: state.event_store.clone(),
        })
        .await
        .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
        state.run_documents_by_run_id.insert(run_id, runtime);
        Ok(())
    }

    fn resolve_run_document_mut<'a>(
        state: &'a mut WriterState,
        run_id: &str,
    ) -> Result<&'a mut WriterDocumentRuntime, WriterError> {
        state
            .run_documents_by_run_id
            .get_mut(run_id)
            .ok_or_else(|| {
                WriterError::Validation(format!("document runtime not found for run_id={run_id}"))
            })
    }

    fn resolve_run_document<'a>(
        state: &'a WriterState,
        run_id: &str,
    ) -> Result<&'a WriterDocumentRuntime, WriterError> {
        state.run_documents_by_run_id.get(run_id).ok_or_else(|| {
            WriterError::Validation(format!("document runtime not found for run_id={run_id}"))
        })
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
        Self::ensure_run_document_loaded(state, &inbound.envelope.run_id).await?;

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

            let run_doc = Self::resolve_run_document_mut(state, &inbound.envelope.run_id)?;
            let overlay = run_doc
                .create_overlay(
                    &inbound.envelope.run_id,
                    base_version_id,
                    OverlayAuthor::User,
                    OverlayKind::Proposal,
                    prompt_diff,
                )
                .await
                .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
            inbound.envelope.overlay_id = Some(overlay.overlay_id);
            run_doc.revision()
        } else {
            Self::apply_text(
                state,
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
        let doc = Self::resolve_run_document(state, &inbound.envelope.run_id)?.document_markdown();

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
                max_steps: 100,
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
        state: &mut WriterState,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
        proposal: bool,
    ) -> Result<u64, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
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
        let result = {
            let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
            run_doc
                .apply_patch(&run_id, source.as_str(), &section_id, ops, proposal)
                .await
                .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?
        };
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

    async fn list_writer_document_versions(
        state: &mut WriterState,
        run_id: String,
    ) -> Result<Vec<DocumentVersion>, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        Ok(Self::resolve_run_document(state, &run_id)?.list_versions())
    }

    async fn get_writer_document_version(
        state: &mut WriterState,
        run_id: String,
        version_id: u64,
    ) -> Result<DocumentVersion, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        Self::resolve_run_document(state, &run_id)?
            .get_version(version_id)
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))
    }

    async fn list_writer_document_overlays(
        state: &mut WriterState,
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
    ) -> Result<Vec<Overlay>, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        Ok(Self::resolve_run_document(state, &run_id)?.list_overlays(base_version_id, status))
    }

    async fn dismiss_writer_document_overlay(
        state: &mut WriterState,
        run_id: String,
        overlay_id: String,
    ) -> Result<(), WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
        run_doc
            .dismiss_overlay(&run_id, &overlay_id)
            .await
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))
    }

    async fn create_writer_document_version(
        state: &mut WriterState,
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
    ) -> Result<DocumentVersion, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;

        // Capture before-content from the effective parent so changeset summarization
        // can diff the two versions.
        let effective_parent = parent_version_id
            .unwrap_or_else(|| run_doc.head_version().map(|v| v.version_id).unwrap_or(0));
        let before_content = run_doc
            .get_version(effective_parent)
            .map(|v| v.content)
            .unwrap_or_default();
        let desktop_id = run_doc.desktop_id().to_string();

        let source_str = match &source {
            VersionSource::Writer => "writer",
            VersionSource::UserSave => "user_save",
            VersionSource::System => "system",
        }
        .to_string();

        let version = run_doc
            .create_version(&run_id, parent_version_id, content.clone(), source)
            .await
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;

        // Fire-and-forget changeset summarization (never blocks the caller).
        Self::spawn_changeset_summarization(ChangesetSummarizationCtx {
            event_store: state.event_store.clone(),
            model_registry: state.model_registry.clone(),
            run_id: run_id.clone(),
            desktop_id,
            source: source_str,
            before_content,
            after_content: content,
            target_version_id: version.version_id,
        });

        // 3.5: Emit .qwy citation_registry event on writer loop completion
        if matches!(version.source, VersionSource::Writer) {
            if let Some(stubs) = state.confirmed_citations_by_run_id.get(&run_id) {
                if !stubs.is_empty() {
                    let entries: Vec<serde_json::Value> = stubs
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "citation_id": s.citation_id,
                                "cited_kind": s.cited_kind,
                                "cited_id": s.cited_id,
                            })
                        })
                        .collect();
                    let payload = serde_json::json!({
                        "run_id": run_id,
                        "version_id": version.version_id,
                        "citation_registry": entries,
                    });
                    let _ = state.event_store.cast(EventStoreMsg::AppendAsync {
                        event: AppendEvent {
                            event_type: shared_types::EVENT_TOPIC_QWY_CITATION_REGISTRY.to_string(),
                            payload,
                            actor_id: state.writer_id.clone(),
                            user_id: state.user_id.clone(),
                        },
                    });
                }
            }
        }

        Ok(version)
    }

    async fn submit_user_prompt(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
    ) -> Result<WriterQueueAck, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        if prompt_diff.is_empty() {
            return Err(WriterError::Validation(
                "prompt_diff cannot be empty".to_string(),
            ));
        }

        let head = Self::resolve_run_document(state, &run_id)?
            .head_version()
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
        if base_version_id != head.version_id {
            return Err(WriterError::Validation(format!(
                "stale base_version_id: expected {}, got {}",
                head.version_id, base_version_id
            )));
        }

        // 3.3: Emit UserInputRecord for writer surface
        let prompt_text: String = prompt_diff
            .iter()
            .filter_map(|op| match op {
                shared_types::PatchOp::Insert { text, .. } => Some(text.as_str()),
                shared_types::PatchOp::Replace { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let user_input_record = shared_types::UserInputRecord {
            input_id: ulid::Ulid::new().to_string(),
            content: prompt_text,
            surface: "writer".to_string(),
            desktop_id: String::new(),
            session_id: String::new(),
            thread_id: run_id.clone(),
            run_id: Some(run_id.clone()),
            document_path: None,
            base_version_id: Some(base_version_id),
            created_at: chrono::Utc::now(),
        };
        let _ = state.event_store.cast(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: shared_types::EVENT_TOPIC_USER_INPUT.to_string(),
                payload: serde_json::json!({
                    "surface": "writer.submit_user_prompt",
                    "run_id": &run_id,
                    "base_version_id": base_version_id,
                    "prompt_diff_len": prompt_diff.len(),
                    "record": user_input_record,
                }),
                actor_id: "writer".to_string(),
                user_id: state.user_id.clone(),
            },
        });

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
        Self::enqueue_inbound(myself, state, WriterInboxMessage { envelope }).await
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
             Use message_writer tool with one or both of:\n\
             - mode: \"delegate_researcher\" for facts, links, verification, or web research\n\
             - mode: \"delegate_terminal\" for repository inspection, architecture/codebase research, shell commands, or local execution\n\
             If the objective needs both local codebase evidence and external/web evidence, call both modes in the same run.\n\
             For research-oriented objectives delegated to terminal, prefer writing findings in docs; only ask for source-code edits when the objective explicitly requests implementation.\n\
             In both cases, set content to a concise objective for the delegated worker.\n\
             Tool discipline (strict): never call web_search/fetch_url/bash/file tools in this planner; only message_writer + finished are valid.\n\
             If no delegation is needed, call `finished` and explain why in message.\n\
             \n\
             Run ID: {}\n\
             Inbox Message ID: {}\n\
             Prompt Payload:\n{}",
            inbound.envelope.run_id, inbound.envelope.message_id, prompt_payload
        )
    }

    fn extract_prompt_text_from_diff_ops(diff_ops: &[shared_types::PatchOp]) -> String {
        let mut fragments = Vec::new();
        for op in diff_ops {
            match op {
                shared_types::PatchOp::Insert { text, .. }
                | shared_types::PatchOp::Replace { text, .. } => {
                    if !text.trim().is_empty() {
                        fragments.push(text.trim().to_string());
                    }
                }
                shared_types::PatchOp::Delete { .. } | shared_types::PatchOp::Retain { .. } => {}
            }
        }
        fragments.join("\n")
    }

    fn extract_user_prompt_text(inbound: &WriterInboxMessage) -> String {
        let prompt_text = inbound
            .envelope
            .prompt_diff
            .as_ref()
            .map(|ops| Self::extract_prompt_text_from_diff_ops(ops))
            .unwrap_or_default();
        if prompt_text.trim().is_empty() {
            inbound.envelope.content.clone()
        } else {
            prompt_text
        }
    }

    fn forced_delegate_capabilities(prompt_text: &str) -> Vec<WriterDelegateCapability> {
        let lower = prompt_text.to_ascii_lowercase();
        let mut capabilities = Vec::new();
        if lower.contains("delegate_researcher") {
            capabilities.push(WriterDelegateCapability::Researcher);
        }
        if lower.contains("delegate_terminal") {
            capabilities.push(WriterDelegateCapability::Terminal);
        }
        capabilities
    }

    fn strip_forced_delegate_tokens(prompt_text: &str) -> String {
        let filtered = prompt_text
            .split_whitespace()
            .filter(|raw| {
                let normalized = raw
                    .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .to_ascii_lowercase();
                normalized != "delegate_researcher" && normalized != "delegate_terminal"
            })
            .collect::<Vec<_>>();
        filtered.join(" ").trim().to_string()
    }

    fn capability_name(capability: WriterDelegateCapability) -> &'static str {
        match capability {
            WriterDelegateCapability::Researcher => "researcher",
            WriterDelegateCapability::Terminal => "terminal",
        }
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

    fn delegated_capabilities_from_tool_executions(
        tool_executions: &[ToolExecution],
    ) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        for execution in tool_executions
            .iter()
            .filter(|exec| exec.tool_name == "message_writer" && exec.success)
        {
            if let Some(capability) = Self::extract_capability_from_tool_output(&execution.output) {
                let normalized = capability.trim().to_ascii_lowercase();
                if !normalized.is_empty() && seen.insert(normalized.clone()) {
                    ordered.push(normalized);
                }
            }
        }
        ordered
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
        let prompt_text = Self::extract_user_prompt_text(inbound);
        let forced_capabilities = Self::forced_delegate_capabilities(&prompt_text);
        if !forced_capabilities.is_empty() {
            let writer_actor = myself.clone();
            let objective = {
                let stripped = Self::strip_forced_delegate_tokens(&prompt_text);
                if stripped.trim().is_empty() {
                    prompt_text.clone()
                } else {
                    stripped
                }
            };

            let mut dispatch_summaries = Vec::new();
            for capability in forced_capabilities {
                let forced_call_id = format!(
                    "writer-forced-delegation:{}:{}",
                    Self::capability_name(capability),
                    ulid::Ulid::new()
                );
                let result = dispatch_delegate_capability(
                    &state.writer_id,
                    &state.user_id,
                    &writer_actor,
                    state.researcher_supervisor.clone(),
                    state.terminal_supervisor.clone(),
                    capability,
                    objective.clone(),
                    Some(180_000),
                    Some(100),
                    Some(inbound.envelope.run_id.clone()),
                    Some(forced_call_id.clone()),
                )?;
                dispatch_summaries.push(format!(
                    "- capability={} call_id={} summary={}",
                    Self::capability_name(capability),
                    forced_call_id,
                    result.summary
                ));
            }

            let completion_envelope = WriterInboundEnvelope {
                message_id: format!(
                    "{}:writer:forced_delegation:{}",
                    inbound.envelope.run_id,
                    ulid::Ulid::new()
                ),
                correlation_id: inbound.envelope.correlation_id.clone(),
                kind: "delegation_forced_dispatch".to_string(),
                run_id: inbound.envelope.run_id.clone(),
                section_id: "conductor".to_string(),
                source: WriterSource::Writer,
                content: format!(
                    "> [writer] forced delegation from prompt directive\n{}\n",
                    dispatch_summaries.join("\n")
                ),
                base_version_id: None,
                prompt_diff: None,
                overlay_id: None,
                session_id: inbound.envelope.session_id.clone(),
                thread_id: inbound.envelope.thread_id.clone(),
                call_id: None,
                origin_actor: Some("writer".to_string()),
            };
            let _ = writer_actor.send_message(WriterMsg::EnqueueInboundAsync {
                envelope: completion_envelope,
            });

            return Ok(
                "Applied deterministic forced delegation from explicit prompt directive."
                    .to_string(),
            );
        }

        let delegation_call_id = format!("writer-delegation:{}", ulid::Ulid::new());
        let objective = Self::build_delegation_objective(inbound);
        let writer_actor = myself.clone();
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
            state.researcher_supervisor.clone(),
            state.terminal_supervisor.clone(),
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
            let _ = writer_actor.send_message(WriterMsg::EnqueueInboundAsync {
                envelope: completion_envelope,
            });
        });

        Ok("Delegation harness dispatched asynchronously.".to_string())
    }

    #[allow(clippy::too_many_arguments)]
    async fn orchestrate_objective(
        myself: &ActorRef<WriterMsg>,
        state: &WriterState,
        objective: String,
        timeout_ms: Option<u64>,
        max_steps: Option<u8>,
        run_id: Option<String>,
        call_id: Option<String>,
    ) -> Result<WriterOrchestrationResult, WriterError> {
        let objective_text = objective.trim();
        if objective_text.is_empty() {
            return Err(WriterError::Validation(
                "objective cannot be empty".to_string(),
            ));
        }

        let orchestration_call_id = call_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("writer-orchestrate:{}", ulid::Ulid::new()));
        let delegation_objective = format!(
            "Determine whether Writer should delegate workers before producing a revision-ready synthesis context.\n\
             Delegate only when the objective needs external verification or local execution.\n\
             Use message_writer with:\n\
             - mode \"delegate_researcher\" for external research, verification, or citations\n\
             - mode \"delegate_terminal\" for local execution or repository/system inspection\n\
             If both are needed, call both in the same run.\n\
             If no delegation is needed, call `finished` and explain why.\n\
             \n\
             Objective:\n{}",
            objective_text
        );

        Self::emit_event(
            state,
            "writer.actor.objective_orchestration.dispatched",
            serde_json::json!({
                "run_id": run_id,
                "call_id": orchestration_call_id,
                "objective": objective_text,
            }),
        );

        let adapter = WriterDelegationAdapter::new(
            state.writer_id.clone(),
            state.user_id.clone(),
            state.event_store.clone(),
            myself.clone(),
            state.researcher_supervisor.clone(),
            state.terminal_supervisor.clone(),
        );
        let harness = AgentHarness::with_config(
            adapter,
            state.model_registry.clone(),
            HarnessConfig {
                timeout_budget_ms: timeout_ms.unwrap_or(180_000),
                max_steps: usize::from(max_steps.unwrap_or(100)),
                emit_progress: true,
                emit_worker_report: true,
            },
            LlmTraceEmitter::new(state.event_store.clone()),
        );

        let result = harness
            .run(
                format!("{}:{orchestration_call_id}", state.writer_id),
                state.user_id.clone(),
                delegation_objective,
                None,
                None,
                run_id.clone(),
                Some(orchestration_call_id.clone()),
            )
            .await
            .map_err(|e| WriterError::WorkerFailed(format!("writer orchestration failed: {e}")))?;

        let delegated_capabilities =
            Self::delegated_capabilities_from_tool_executions(&result.tool_executions);
        let pending_delegations = delegated_capabilities.len();
        let summary = if delegated_capabilities.is_empty() {
            format!("No worker delegation executed. {}", result.summary.trim())
        } else {
            format!(
                "Delegated capabilities dispatched: {}. {}",
                delegated_capabilities.join(", "),
                result.summary.trim()
            )
        };

        Self::emit_event(
            state,
            "writer.actor.objective_orchestration.completed",
            serde_json::json!({
                "run_id": run_id,
                "call_id": orchestration_call_id,
                "success": result.success,
                "delegated_capabilities": delegated_capabilities,
                "pending_delegations": pending_delegations,
                "summary": summary,
            }),
        );

        Ok(WriterOrchestrationResult {
            success: result.success,
            summary,
            delegated_capabilities,
            pending_delegations,
        })
    }

    async fn set_section_content(
        state: &mut WriterState,
        run_id: String,
        section_id: String,
        source: WriterSource,
        content: String,
    ) -> Result<u64, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        if content.trim().is_empty() {
            return Err(WriterError::Validation(
                "content cannot be empty".to_string(),
            ));
        }
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
        let head = run_doc
            .head_version()
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
        let parent_version_id = head.version_id;
        let before_content = head.content.clone();
        let desktop_id = run_doc.desktop_id().to_string();

        let version_source = match source {
            WriterSource::Writer => VersionSource::Writer,
            WriterSource::User => VersionSource::UserSave,
            WriterSource::Researcher | WriterSource::Terminal | WriterSource::Conductor => {
                VersionSource::System
            }
        };
        let is_writer_source = matches!(version_source, VersionSource::Writer);

        let version = run_doc
            .create_version(
                &run_id,
                Some(parent_version_id),
                content.clone(),
                version_source,
            )
            .await
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
        let revision = run_doc.revision();
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

        // Spawn background changeset summarization (non-blocking).
        Self::spawn_changeset_summarization(ChangesetSummarizationCtx {
            event_store: state.event_store.clone(),
            model_registry: state.model_registry.clone(),
            run_id: run_id.clone(),
            desktop_id,
            source: source.as_str().to_string(),
            before_content,
            after_content: content,
            target_version_id: version.version_id,
        });

        // 3.5: Emit .qwy citation_registry event on writer synthesis completion
        if is_writer_source {
            if let Some(stubs) = state.confirmed_citations_by_run_id.get(&run_id) {
                if !stubs.is_empty() {
                    let entries: Vec<serde_json::Value> = stubs
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "citation_id": s.citation_id,
                                "cited_kind": s.cited_kind,
                                "cited_id": s.cited_id,
                            })
                        })
                        .collect();
                    let payload = serde_json::json!({
                        "run_id": run_id,
                        "version_id": version.version_id,
                        "citation_registry": entries,
                    });
                    let _ = state.event_store.cast(EventStoreMsg::AppendAsync {
                        event: AppendEvent {
                            event_type: shared_types::EVENT_TOPIC_QWY_CITATION_REGISTRY.to_string(),
                            payload,
                            actor_id: state.writer_id.clone(),
                            user_id: state.user_id.clone(),
                        },
                    });
                }
            }
        }

        Ok(revision)
    }

    /// Spawn a non-blocking background task that calls BAML to summarize a document
    /// changeset and emits a `writer.run.changeset` event.  Failures are logged but
    /// never propagate to the caller — this is pure observability enrichment.
    fn spawn_changeset_summarization(ctx: ChangesetSummarizationCtx) {
        use crate::actors::model_config::ModelResolutionContext;
        use crate::baml_client::types::{ChangesetInput, ImpactLevel};
        use crate::baml_client::{new_collector, B};

        let ChangesetSummarizationCtx {
            event_store,
            model_registry,
            run_id,
            desktop_id,
            source,
            before_content,
            after_content,
            target_version_id,
        } = ctx;

        tokio::spawn(async move {
            let patch_id = ulid::Ulid::new().to_string();
            let ops_json = serde_json::json!({
                "before_len": before_content.len(),
                "after_len": after_content.len(),
                "target_version_id": target_version_id,
            })
            .to_string();

            let resolved = match model_registry
                .resolve_for_callsite("writer", &ModelResolutionContext::default())
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %e,
                        "changeset summarization: model resolution failed"
                    );
                    return;
                }
            };
            let client_registry = match model_registry
                .create_runtime_client_registry_for_model(&resolved.config.id)
            {
                Ok(cr) => cr,
                Err(e) => {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %e,
                        "changeset summarization: client registry creation failed"
                    );
                    return;
                }
            };

            let input = ChangesetInput {
                patch_id: patch_id.clone(),
                loop_id: None,
                before_content: before_content.chars().take(2000).collect::<String>(),
                after_content: after_content.chars().take(2000).collect::<String>(),
                ops_json,
                source: source.clone(),
            };

            let collector = new_collector("writer.changeset_summarization");
            match B
                .SummarizeChangeset
                .with_client_registry(&client_registry)
                .with_collector(&collector)
                .call(&input)
                .await
            {
                Ok(summary) => {
                    let impact_str = match summary.impact {
                        ImpactLevel::Low => "low",
                        ImpactLevel::Medium => "medium",
                        ImpactLevel::High => "high",
                    };
                    let _ = event_store.cast(EventStoreMsg::AppendAsync {
                        event: AppendEvent {
                            event_type: shared_types::EVENT_TOPIC_WRITER_RUN_CHANGESET.to_string(),
                            payload: serde_json::json!({
                                "run_id": run_id,
                                "desktop_id": desktop_id,
                                "patch_id": patch_id,
                                "loop_id": null,
                                "target_version_id": target_version_id,
                                "source": source,
                                "summary": summary.summary,
                                "impact": impact_str,
                                "op_taxonomy": summary.op_taxonomy,
                            }),
                            actor_id: "writer".to_string(),
                            user_id: String::new(),
                        },
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        run_id = %run_id,
                        patch_id = %patch_id,
                        error = %e,
                        "changeset summarization: BAML call failed"
                    );
                }
            }
        });
    }

    async fn report_progress(
        state: &mut WriterState,
        run_id: String,
        section_id: String,
        source: WriterSource,
        phase: String,
        message: String,
    ) -> Result<u64, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        if message.trim().is_empty() {
            return Err(WriterError::Validation(
                "message cannot be empty".to_string(),
            ));
        }
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
        let revision = run_doc
            .report_section_progress(&run_id, source.as_str(), &section_id, &phase, &message)
            .await
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
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

    async fn set_section_state(
        state: &mut WriterState,
        run_id: String,
        section_id: String,
        section_state: SectionState,
    ) -> Result<(), WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
        run_doc
            .mark_section_state(&run_id, &section_id, section_state)
            .await
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))
    }

    fn source_for_delegated_capability(capability: WriterDelegateCapability) -> WriterSource {
        match capability {
            WriterDelegateCapability::Researcher => WriterSource::Researcher,
            WriterDelegateCapability::Terminal => WriterSource::Terminal,
        }
    }

    fn section_for_delegated_capability(capability: WriterDelegateCapability) -> String {
        match capability {
            WriterDelegateCapability::Researcher => "researcher".to_string(),
            WriterDelegateCapability::Terminal => "terminal".to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_delegation_worker_completed(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        capability: WriterDelegateCapability,
        run_id: Option<String>,
        call_id: Option<String>,
        dispatch_id: String,
        result: Result<WriterDelegateResult, WriterError>,
    ) {
        let Some(run_id) = run_id else {
            Self::emit_event(
                state,
                "writer.actor.delegation_worker.completed_ignored",
                serde_json::json!({
                    "dispatch_id": dispatch_id,
                    "capability": format!("{capability:?}"),
                    "reason": "missing_run_id",
                }),
            );
            return;
        };

        let section_id = Self::section_for_delegated_capability(capability);
        let source = Self::source_for_delegated_capability(capability);
        let correlation_id = call_id.unwrap_or_else(|| dispatch_id.clone());

        let (success, summary, proposed_citation_ids, proposed_citation_stubs) = match result {
            Ok(delegate_result) => {
                let summary = if delegate_result.summary.trim().is_empty() {
                    "Delegated worker completed without summary.".to_string()
                } else {
                    delegate_result.summary
                };
                (
                    delegate_result.success,
                    summary,
                    delegate_result.proposed_citation_ids,
                    delegate_result.proposed_citation_stubs,
                )
            }
            Err(error) => (false, error.to_string(), vec![], vec![]),
        };

        // 3.2: Emit citation.confirmed / citation.rejected for proposed citations
        Self::emit_citation_confirmation_events(state, &run_id, &proposed_citation_ids, success);

        // 3.4: Emit global_external_content.upsert for confirmed external citations
        if success {
            Self::emit_global_external_content_upsert(state, &run_id, &proposed_citation_stubs);
        }

        // 3.5: Accumulate confirmed stubs for .qwy citation_registry on version save
        if success && !proposed_citation_stubs.is_empty() {
            state
                .confirmed_citations_by_run_id
                .entry(run_id.clone())
                .or_default()
                .extend(proposed_citation_stubs);
        }

        let section_state = if success {
            SectionState::Complete
        } else {
            SectionState::Failed
        };
        if let Err(error) =
            Self::set_section_state(state, run_id.clone(), section_id.clone(), section_state).await
        {
            Self::emit_event(
                state,
                "writer.actor.delegation_worker.set_state_failed",
                serde_json::json!({
                    "dispatch_id": dispatch_id,
                    "run_id": run_id,
                    "section_id": section_id,
                    "error": error.to_string(),
                }),
            );
        }

        let kind = if success {
            "delegation_worker_completed"
        } else {
            "delegation_worker_failed"
        };
        let content = if success {
            format!(
                "Delegated {:?} worker completed.\nSummary: {}",
                capability, summary
            )
        } else {
            format!(
                "Delegated {:?} worker failed.\nError: {}",
                capability, summary
            )
        };
        let envelope = WriterInboundEnvelope {
            message_id: format!("{run_id}:writer:{kind}:{dispatch_id}"),
            correlation_id,
            kind: kind.to_string(),
            run_id,
            section_id,
            source,
            content,
            base_version_id: None,
            prompt_diff: None,
            overlay_id: None,
            session_id: None,
            thread_id: None,
            call_id: Some(dispatch_id.clone()),
            origin_actor: Some("writer".to_string()),
        };

        if let Err(error) =
            Self::enqueue_inbound(myself, state, WriterInboxMessage { envelope }).await
        {
            Self::emit_event(
                state,
                "writer.actor.delegation_worker.enqueue_failed",
                serde_json::json!({
                    "dispatch_id": dispatch_id,
                    "error": error.to_string(),
                }),
            );
        }

        match capability {
            WriterDelegateCapability::Researcher => {
                if let Some(researcher_supervisor) = &state.researcher_supervisor {
                    let researcher_id =
                        format!("writer-researcher:{}:{}", state.writer_id, dispatch_id);
                    let _ = researcher_supervisor
                        .cast(ResearcherSupervisorMsg::RemoveResearcher { researcher_id });
                }
            }
            WriterDelegateCapability::Terminal => {
                if let Some(terminal_supervisor) = &state.terminal_supervisor {
                    let terminal_id =
                        format!("writer-terminal:{}:{}", state.writer_id, dispatch_id);
                    let _ = terminal_supervisor
                        .cast(TerminalSupervisorMsg::RemoveTerminal { terminal_id });
                }
            }
        }
    }

    /// 3.2: Emit citation.confirmed or citation.rejected events based on synthesis success.
    /// Called when a delegated worker (researcher/terminal) completes with proposed citation IDs.
    fn emit_citation_confirmation_events(
        state: &WriterState,
        run_id: &str,
        proposed_citation_ids: &[String],
        confirmed: bool,
    ) {
        if proposed_citation_ids.is_empty() {
            return;
        }
        let topic = if confirmed {
            shared_types::EVENT_TOPIC_CITATION_CONFIRMED
        } else {
            shared_types::EVENT_TOPIC_CITATION_REJECTED
        };
        let status = if confirmed { "confirmed" } else { "rejected" };
        let confirmed_by: Option<String> = if confirmed {
            Some("writer".to_string())
        } else {
            None
        };
        let confirmed_at: Option<String> = if confirmed {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };
        for citation_id in proposed_citation_ids {
            let payload = serde_json::json!({
                "citation_id": citation_id,
                "citing_run_id": run_id,
                "status": status,
                "confirmed_by": confirmed_by,
                "confirmed_at": confirmed_at,
            });
            let _ = state.event_store.cast(EventStoreMsg::AppendAsync {
                event: AppendEvent {
                    event_type: topic.to_string(),
                    payload,
                    actor_id: state.writer_id.clone(),
                    user_id: state.user_id.clone(),
                },
            });
        }
    }

    /// 3.4: Emit global_external_content.upsert for confirmed external URL citations.
    /// This event signals the global store to create or increment the citation count for
    /// the cited URL. Downstream persistence is wired in Phase 5/6.
    fn emit_global_external_content_upsert(
        state: &WriterState,
        run_id: &str,
        stubs: &[ProposedCitationStub],
    ) {
        for stub in stubs {
            if stub.cited_kind != "external_url" {
                continue;
            }
            let payload = serde_json::json!({
                "citation_id": stub.citation_id,
                "citing_run_id": run_id,
                "cited_kind": stub.cited_kind,
                "cited_id": stub.cited_id,
                // content_id is the URL hash — computed downstream by the store
                "action": "upsert",
            });
            let _ = state.event_store.cast(EventStoreMsg::AppendAsync {
                event: AppendEvent {
                    event_type: shared_types::EVENT_TOPIC_GLOBAL_EXTERNAL_CONTENT_UPSERT
                        .to_string(),
                    payload,
                    actor_id: state.writer_id.clone(),
                    user_id: state.user_id.clone(),
                },
            });
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
        dispatch_delegate_capability(
            &state.writer_id,
            &state.user_id,
            myself,
            state.researcher_supervisor.clone(),
            state.terminal_supervisor.clone(),
            capability,
            objective,
            timeout_ms,
            max_steps,
            run_id,
            call_id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    fn run_dir(run_id: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(WriterActor::RUN_DOCUMENTS_ROOT)
            .join(run_id)
    }

    #[tokio::test]
    async fn list_versions_rehydrates_persisted_document_after_writer_restart() {
        let run_id = format!("run_writer_rehydrate_{}", ulid::Ulid::new());
        let run_dir = run_dir(&run_id);
        if run_dir.exists() {
            tokio::fs::remove_dir_all(&run_dir).await.unwrap();
        }

        let (event_store, _event_store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let writer_args = WriterArguments {
            writer_id: "writer-test".to_string(),
            user_id: "user-test".to_string(),
            event_store: event_store.clone(),
            researcher_supervisor: None,
            terminal_supervisor: None,
        };

        let (writer_first, _writer_first_handle) =
            Actor::spawn(None, WriterActor, writer_args.clone())
                .await
                .unwrap();

        let ensured = ractor::call!(writer_first, |reply| WriterMsg::EnsureRunDocument {
            run_id: run_id.clone(),
            desktop_id: "desktop-test".to_string(),
            objective: "Hydration test objective".to_string(),
            reply,
        })
        .unwrap();
        assert!(ensured.is_ok());

        let revision = ractor::call!(writer_first, |reply| WriterMsg::ApplyText {
            run_id: run_id.clone(),
            section_id: "conductor".to_string(),
            source: WriterSource::Conductor,
            content: "Persisted content across restart.".to_string(),
            proposal: false,
            reply,
        })
        .unwrap()
        .unwrap();
        assert!(revision > 0);

        let versions_before = ractor::call!(writer_first, |reply| {
            WriterMsg::ListWriterDocumentVersions {
                run_id: run_id.clone(),
                reply,
            }
        })
        .unwrap()
        .unwrap();
        assert!(versions_before.len() >= 2);
        let head_before = versions_before
            .iter()
            .max_by_key(|version| version.version_id)
            .cloned()
            .expect("head version should exist");

        writer_first.stop(None);

        let (writer_second, _writer_second_handle) =
            Actor::spawn(None, WriterActor, writer_args).await.unwrap();

        // No EnsureRunDocument call on purpose. The writer should hydrate lazily.
        let versions_after = ractor::call!(writer_second, |reply| {
            WriterMsg::ListWriterDocumentVersions {
                run_id: run_id.clone(),
                reply,
            }
        })
        .unwrap()
        .unwrap();

        let head_after = versions_after
            .iter()
            .max_by_key(|version| version.version_id)
            .cloned()
            .expect("head version should exist after rehydrate");

        assert_eq!(head_after.version_id, head_before.version_id);
        assert_eq!(head_after.content, head_before.content);
        assert_eq!(versions_after.len(), versions_before.len());

        writer_second.stop(None);
        event_store.stop(None);
        if run_dir.exists() {
            tokio::fs::remove_dir_all(&run_dir).await.unwrap();
        }
    }
}
