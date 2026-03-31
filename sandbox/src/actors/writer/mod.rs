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

use crate::actors::agent_harness::{AgentHarness, HarnessConfig, ToolExecution};
use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress};
use crate::actors::terminal::{ensure_terminal_started, TerminalAgentProgress, TerminalMsg};
use crate::observability::llm_trace::LlmTraceEmitter;
use crate::supervisor::researcher::ResearcherSupervisorMsg;
use crate::supervisor::terminal::TerminalSupervisorMsg;
use adapter::{WriterDelegationAdapter, WriterUserPromptAdapter};
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

#[derive(Debug)]
struct WriterInboxApplyOutcome {
    revision: u64,
    base_version_id: Option<u64>,
    target_version_id: Option<u64>,
    overlay_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriterMessageSourceKind {
    Web,
    File,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterMessageSource {
    pub id: String,
    pub kind: WriterMessageSourceKind,
    pub provider: Option<String>,
    pub url: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub publisher: Option<String>,
    pub published_at: Option<String>,
    pub line_start: Option<u64>,
    pub line_end: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterMessageCitation {
    pub source_id: String,
    pub anchor: Option<String>,
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
    pub source_refs: Vec<String>,
    pub sources: Vec<WriterMessageSource>,
    pub citations: Vec<WriterMessageCitation>,
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
        source_refs: Vec<String>,
        reply: RpcReplyPort<Result<u64, WriterError>>,
    },
    /// Update section state for writer UX.
    SetSectionState {
        run_id: String,
        section_id: String,
        state: SectionState,
        reply: RpcReplyPort<Result<(), WriterError>>,
    },
    /// Queue an inbound diff/event message for unified async inbox processing.
    EnqueueInbound {
        envelope: WriterInboundEnvelope,
        reply: RpcReplyPort<Result<WriterQueueAck, WriterError>>,
    },
    /// Internal wake to process the next queued inbox item.
    ProcessInbox,
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
    /// Background: writer LLM processes a user prompt diff and produces a revision.
    OrchestrateUserPrompt {
        run_id: String,
        call_id: String,
        objective: String,
        parent_version_id: u64,
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
    session_id: String,
    thread_id: String,
    document_path: String,
    revision: u64,
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
        "{objective}\n\nWriter output contract:\n- Return findings summary only.\n- Writer will incorporate your findings into the document.\n- Do not propose document mutations or full rewrites."
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
                            working_dir: crate::paths::sandbox_root().to_string_lossy().to_string(),
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
                source_refs,
                reply,
            } => {
                let result = Self::report_progress(
                    state,
                    run_id,
                    section_id,
                    source,
                    phase,
                    message,
                    source_refs,
                )
                .await;
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
            WriterMsg::EnqueueInbound { envelope, reply } => {
                let result =
                    Self::enqueue_inbound(&myself, state, WriterInboxMessage { envelope }).await;
                let _ = reply.send(result);
            }
            WriterMsg::ProcessInbox => {
                Self::process_inbox(&myself, state).await;
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
            WriterMsg::OrchestrateUserPrompt {
                run_id,
                call_id,
                objective,
                parent_version_id,
            } => {
                // IMPORTANT: Spawn as a background task instead of awaiting.
                // The orchestration adapter calls back into this actor via
                // CreateWriterDocumentVersion. Awaiting here would deadlock
                // because the actor can't process messages while handle() is blocked.
                let myself_clone = myself.clone();
                let writer_id = state.writer_id.clone();
                let user_id = state.user_id.clone();
                let event_store = state.event_store.clone();
                let researcher_supervisor = state.researcher_supervisor.clone();
                let terminal_supervisor = state.terminal_supervisor.clone();
                let model_registry = state.model_registry.clone();

                tokio::spawn(async move {
                    Self::orchestrate_user_prompt_bg(
                        &myself_clone,
                        writer_id,
                        user_id,
                        event_store,
                        researcher_supervisor,
                        terminal_supervisor,
                        model_registry,
                        run_id,
                        call_id,
                        objective,
                        parent_version_id,
                    )
                    .await;
                });
            }
        }
        Ok(())
    }
}

impl WriterActor {
    const MAX_SEEN_IDS: usize = 4096;
    const AUTO_ACCEPT_WORKER_DIFFS: bool = false;
    const RUN_DOCUMENTS_ROOT: &'static str = "conductor/runs";
    const RUN_DOCUMENT_FILE: &'static str = "draft.md";
    const RUN_DOCUMENT_STATE_FILE: &'static str = "draft.writer-state.json";
    const DEFAULT_RESTORED_DESKTOP_ID: &'static str = "default-desktop";

    fn run_document_dir(run_id: &str) -> PathBuf {
        crate::paths::writer_root()
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
            root_dir: Some(crate::paths::writer_root().to_string_lossy().to_string()),
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
            root_dir: Some(crate::paths::writer_root().to_string_lossy().to_string()),
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

    fn source_reference_strings(envelope: &WriterInboundEnvelope) -> Vec<String> {
        let mut refs = Vec::new();
        for source in &envelope.sources {
            if let Some(url) = source
                .url
                .as_ref()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            {
                if !refs.iter().any(|existing| existing == url) {
                    refs.push(url.to_string());
                }
                continue;
            }
            if let Some(path) = source
                .path
                .as_ref()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            {
                if !refs.iter().any(|existing| existing == path) {
                    refs.push(path.to_string());
                }
            }
        }
        refs
    }

    fn normalize_inbound_append_delta(
        state: &WriterState,
        run_id: &str,
        raw_content: &str,
    ) -> String {
        let trimmed = raw_content.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let has_existing_content = Self::resolve_run_document(state, run_id)
            .ok()
            .and_then(|doc| doc.head_version().ok())
            .map(|version| !version.content.trim().is_empty())
            .unwrap_or(false);
        if has_existing_content {
            format!("\n\n{trimmed}\n")
        } else {
            format!("{trimmed}\n")
        }
    }

    fn overlay_author_for_source(source: WriterSource) -> OverlayAuthor {
        match source {
            WriterSource::User => OverlayAuthor::User,
            WriterSource::Researcher => OverlayAuthor::Researcher,
            WriterSource::Terminal => OverlayAuthor::Terminal,
            WriterSource::Writer | WriterSource::Conductor => OverlayAuthor::Writer,
        }
    }

    fn version_source_for_source(source: WriterSource) -> VersionSource {
        match source {
            WriterSource::User => VersionSource::UserSave,
            WriterSource::Writer => VersionSource::Writer,
            WriterSource::Researcher | WriterSource::Terminal | WriterSource::Conductor => {
                VersionSource::System
            }
        }
    }

    fn apply_shared_patch_ops(
        base_content: &str,
        ops: &[shared_types::PatchOp],
    ) -> Result<String, WriterError> {
        let mut chars: Vec<char> = base_content.chars().collect();

        for op in ops {
            match op {
                shared_types::PatchOp::Insert { pos, text } => {
                    let idx = usize::try_from(*pos).map_err(|_| {
                        WriterError::Validation(format!(
                            "insert position out of bounds for usize: {pos}"
                        ))
                    })?;
                    if idx > chars.len() {
                        return Err(WriterError::Validation(format!(
                            "insert position {idx} exceeds content length {}",
                            chars.len()
                        )));
                    }
                    chars.splice(idx..idx, text.chars());
                }
                shared_types::PatchOp::Delete { pos, len } => {
                    let idx = usize::try_from(*pos).map_err(|_| {
                        WriterError::Validation(format!(
                            "delete position out of bounds for usize: {pos}"
                        ))
                    })?;
                    if idx > chars.len() {
                        return Err(WriterError::Validation(format!(
                            "delete position {idx} exceeds content length {}",
                            chars.len()
                        )));
                    }
                    let max_delete = chars.len().saturating_sub(idx);
                    let requested = if *len == u64::MAX {
                        max_delete
                    } else {
                        usize::try_from(*len).unwrap_or(max_delete)
                    };
                    let end = idx.saturating_add(requested).min(chars.len());
                    chars.drain(idx..end);
                }
                shared_types::PatchOp::Replace { pos, len, text } => {
                    let idx = usize::try_from(*pos).map_err(|_| {
                        WriterError::Validation(format!(
                            "replace position out of bounds for usize: {pos}"
                        ))
                    })?;
                    if idx > chars.len() {
                        return Err(WriterError::Validation(format!(
                            "replace position {idx} exceeds content length {}",
                            chars.len()
                        )));
                    }
                    let max_replace = chars.len().saturating_sub(idx);
                    let requested = if *len == u64::MAX {
                        max_replace
                    } else {
                        usize::try_from(*len).unwrap_or(max_replace)
                    };
                    let end = idx.saturating_add(requested).min(chars.len());
                    chars.splice(idx..end, text.chars());
                }
                shared_types::PatchOp::Retain { .. } => {}
            }
        }

        Ok(chars.into_iter().collect::<String>())
    }

    async fn apply_prompt_diff_inbox(
        state: &mut WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<WriterInboxApplyOutcome, WriterError> {
        let base_version_id = inbound.envelope.base_version_id.ok_or_else(|| {
            WriterError::Validation(
                "base_version_id is required when prompt_diff is set".to_string(),
            )
        })?;
        let prompt_diff = inbound.envelope.prompt_diff.clone().ok_or_else(|| {
            WriterError::Validation(
                "prompt_diff is required when base_version_id is set".to_string(),
            )
        })?;
        if prompt_diff.is_empty() {
            return Err(WriterError::Validation(
                "prompt_diff cannot be empty".to_string(),
            ));
        }

        let run_id = inbound.envelope.run_id.clone();
        let proposal =
            inbound.envelope.source != WriterSource::User && !Self::AUTO_ACCEPT_WORKER_DIFFS;
        Self::ensure_run_document_loaded(state, &run_id).await?;

        if !proposal {
            let head_version_id = Self::resolve_run_document(state, &run_id)?
                .head_version()
                .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?
                .version_id;
            if base_version_id != head_version_id {
                return Err(WriterError::Validation(format!(
                    "stale base_version_id: expected {head_version_id}, got {base_version_id}"
                )));
            }
        }

        if proposal {
            let (overlay_id, revision) = {
                let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
                let overlay = run_doc
                    .create_overlay(
                        &run_id,
                        base_version_id,
                        Self::overlay_author_for_source(inbound.envelope.source),
                        OverlayKind::Proposal,
                        prompt_diff,
                    )
                    .await
                    .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
                (overlay.overlay_id, run_doc.revision())
            };
            return Ok(WriterInboxApplyOutcome {
                revision,
                base_version_id: Some(base_version_id),
                target_version_id: None,
                overlay_id: Some(overlay_id),
            });
        }

        let base_content = Self::resolve_run_document(state, &run_id)?
            .get_version(base_version_id)
            .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?
            .content;
        let next_content = Self::apply_shared_patch_ops(&base_content, &prompt_diff)?;

        let (target_version_id, revision) = {
            let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
            let version = run_doc
                .create_version(
                    &run_id,
                    Some(base_version_id),
                    next_content,
                    Self::version_source_for_source(inbound.envelope.source),
                )
                .await
                .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?;
            (version.version_id, run_doc.revision())
        };

        Ok(WriterInboxApplyOutcome {
            revision,
            base_version_id: Some(base_version_id),
            target_version_id: Some(target_version_id),
            overlay_id: None,
        })
    }

    async fn apply_inbound_message(
        state: &mut WriterState,
        inbound: &WriterInboxMessage,
    ) -> Result<WriterInboxApplyOutcome, WriterError> {
        if inbound
            .envelope
            .prompt_diff
            .as_ref()
            .map(|ops| !ops.is_empty())
            .unwrap_or(false)
        {
            return Self::apply_prompt_diff_inbox(state, inbound).await;
        }

        if inbound.envelope.content.trim().is_empty() {
            return Err(WriterError::Validation(
                "inbound content cannot be empty when prompt_diff is absent".to_string(),
            ));
        }

        let is_worker_update = inbound.envelope.source != WriterSource::User;
        let proposal = is_worker_update && !Self::AUTO_ACCEPT_WORKER_DIFFS;
        let normalized_content = if is_worker_update {
            Self::normalize_inbound_append_delta(
                state,
                &inbound.envelope.run_id,
                &inbound.envelope.content,
            )
        } else {
            inbound.envelope.content.clone()
        };
        let revision = Self::apply_text(
            state,
            inbound.envelope.run_id.clone(),
            inbound.envelope.section_id.clone(),
            inbound.envelope.source,
            normalized_content,
            proposal,
        )
        .await?;

        let selected_source_refs = Self::source_reference_strings(&inbound.envelope);
        if !selected_source_refs.is_empty() {
            let _ = Self::report_progress(
                state,
                inbound.envelope.run_id.clone(),
                inbound.envelope.section_id.clone(),
                inbound.envelope.source,
                format!("{}:source_refs", inbound.envelope.kind),
                "Updated source references".to_string(),
                selected_source_refs,
            )
            .await?;
        }

        Ok(WriterInboxApplyOutcome {
            revision,
            base_version_id: inbound.envelope.base_version_id,
            target_version_id: None,
            overlay_id: None,
        })
    }

    async fn enqueue_inbound(
        myself: &ActorRef<WriterMsg>,
        state: &mut WriterState,
        inbound: WriterInboxMessage,
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
        if has_prompt_diff && inbound.envelope.base_version_id.is_none() {
            return Err(WriterError::Validation(
                "base_version_id is required when prompt_diff is set".to_string(),
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

        let revision = Self::resolve_run_document(state, &inbound.envelope.run_id)?.revision();
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
                "revision": revision,
                "correlation_id": inbound.envelope.correlation_id.clone(),
                "base_version_id": inbound.envelope.base_version_id,
                "overlay_id": inbound.envelope.overlay_id.clone(),
                "session_id": inbound.envelope.session_id.clone(),
                "thread_id": inbound.envelope.thread_id.clone(),
                "call_id": inbound.envelope.call_id.clone(),
                "origin_actor": inbound.envelope.origin_actor.clone(),
                "source_refs": inbound.envelope.source_refs.clone(),
                "sources": inbound.envelope.sources.clone(),
                "citations": inbound.envelope.citations.clone(),
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
            revision,
        })
    }

    async fn process_inbox(myself: &ActorRef<WriterMsg>, state: &mut WriterState) {
        if state.inbox_processing {
            return;
        }
        state.inbox_processing = true;

        while let Some(inbound) = state.inbox_queue.pop_front() {
            match Self::apply_inbound_message(state, &inbound).await {
                Ok(outcome) => {
                    Self::emit_event(
                        state,
                        "writer.actor.inbox.applied",
                        serde_json::json!({
                            "run_id": inbound.envelope.run_id.clone(),
                            "section_id": inbound.envelope.section_id.clone(),
                            "source": inbound.envelope.source.as_str(),
                            "kind": inbound.envelope.kind.clone(),
                            "message_id": inbound.envelope.message_id.clone(),
                            "queue_len": state.inbox_queue.len(),
                            "revision": outcome.revision,
                            "correlation_id": inbound.envelope.correlation_id.clone(),
                            "base_version_id": outcome.base_version_id,
                            "target_version_id": outcome.target_version_id,
                            "overlay_id": outcome.overlay_id.or_else(|| inbound.envelope.overlay_id.clone()),
                            "session_id": inbound.envelope.session_id.clone(),
                            "thread_id": inbound.envelope.thread_id.clone(),
                            "call_id": inbound.envelope.call_id.clone(),
                            "origin_actor": inbound.envelope.origin_actor.clone(),
                            "source_refs": inbound.envelope.source_refs.clone(),
                            "sources": inbound.envelope.sources.clone(),
                            "citations": inbound.envelope.citations.clone(),
                        }),
                    );
                }
                Err(error) => {
                    Self::emit_event(
                        state,
                        "writer.actor.inbox.apply_failed",
                        serde_json::json!({
                            "run_id": inbound.envelope.run_id.clone(),
                            "section_id": inbound.envelope.section_id.clone(),
                            "source": inbound.envelope.source.as_str(),
                            "kind": inbound.envelope.kind.clone(),
                            "message_id": inbound.envelope.message_id.clone(),
                            "queue_len": state.inbox_queue.len(),
                            "correlation_id": inbound.envelope.correlation_id.clone(),
                            "base_version_id": inbound.envelope.base_version_id,
                            "overlay_id": inbound.envelope.overlay_id.clone(),
                            "session_id": inbound.envelope.session_id.clone(),
                            "thread_id": inbound.envelope.thread_id.clone(),
                            "call_id": inbound.envelope.call_id.clone(),
                            "origin_actor": inbound.envelope.origin_actor.clone(),
                            "error": error.to_string(),
                        }),
                    );
                }
            }
        }

        state.inbox_processing = false;
        if !state.inbox_queue.is_empty() {
            let _ = myself.send_message(WriterMsg::ProcessInbox);
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
        let event_store = state.event_store.clone();
        let model_registry = state.model_registry.clone();
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

        let (session_id, thread_id, document_path, revision) = (
            run_doc.session_id().to_string(),
            run_doc.thread_id().to_string(),
            run_doc.document_path_relative().to_string(),
            run_doc.revision(),
        );

        // Fire-and-forget changeset summarization (never blocks the caller).
        Self::spawn_changeset_summarization(ChangesetSummarizationCtx {
            event_store,
            model_registry,
            run_id: run_id.clone(),
            desktop_id,
            session_id,
            thread_id,
            document_path,
            revision,
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

    /// Generate a unified diff string between two texts for LLM context.
    fn compute_unified_diff(base: &str, edited: &str) -> String {
        use similar::{ChangeTag, TextDiff};
        let diff = TextDiff::from_lines(base, edited);
        let mut out = String::new();
        for change in diff.iter_all_changes() {
            let prefix = match change.tag() {
                ChangeTag::Equal => " ",
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
            };
            out.push_str(prefix);
            out.push_str(change.value());
            if !change.value().ends_with('\n') {
                out.push('\n');
            }
        }
        out
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

        // Get the base content and compute the user's prompted version.
        let base_content = head.content.clone();
        let prompted_content = Self::apply_shared_patch_ops(&base_content, &prompt_diff)?;

        // Emit UserInputRecord for writer surface.
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

        // Save the user's edit as a UserSave version immediately.
        let user_version = {
            let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
            run_doc
                .create_version(
                    &run_id,
                    Some(base_version_id),
                    prompted_content.clone(),
                    VersionSource::UserSave,
                )
                .await
                .map_err(|e| WriterError::WriterDocumentFailed(e.to_string()))?
        };
        let revision = Self::resolve_run_document(state, &run_id)?.revision();

        Self::emit_event(
            state,
            "writer.actor.user_prompt.saved",
            serde_json::json!({
                "run_id": &run_id,
                "base_version_id": base_version_id,
                "user_version_id": user_version.version_id,
                "revision": revision,
            }),
        );

        // Compute unified diff for LLM context and spawn background orchestration.
        let unified_diff = Self::compute_unified_diff(&base_content, &prompted_content);
        let writer_ref = myself.clone();
        let run_id_for_task = run_id.clone();
        let orchestration_call_id = format!("writer-user-prompt:{}", ulid::Ulid::new());
        let user_version_id = user_version.version_id;

        // Build the objective that includes document + diff for the writer LLM.
        let objective = format!(
            "The user edited the document. Review their changes and produce a revised version \
             that incorporates the user's intent. The user's changes may include:\n\
             - Direct content edits (apply them)\n\
             - Inline instructions like [make this shorter] (interpret and execute)\n\
             - Deletions (honor them)\n\
             - Replacements (use the new content)\n\n\
             If the changes are purely editorial (typo fixes, reformatting), apply them as-is.\n\
             If the changes require research or code inspection, delegate to workers first.\n\
             After you submit a satisfactory write_revision, call finished instead of creating \
             another revision unless fresh worker results still require a change.\n\
             Do not use markdown footnote markers like [^s1] or [1] unless the document already \
             has a working citation system. Prefer inline source mentions or plain prose.\n\n\
             ## Current Document (before user edit)\n\n{base_content}\n\n\
             ## User's Changes (unified diff)\n\n```diff\n{unified_diff}```\n\n\
             ## User's Edited Version\n\n{prompted_content}"
        );

        // Spawn background orchestration — writer LLM processes the diff.
        let _ = writer_ref.send_message(WriterMsg::OrchestrateUserPrompt {
            run_id: run_id_for_task,
            call_id: orchestration_call_id,
            objective,
            parent_version_id: user_version_id,
        });

        Ok(WriterQueueAck {
            message_id: format!("{run_id}:user:prompt:{}", ulid::Ulid::new()),
            accepted: true,
            duplicate: false,
            queue_len: 0,
            revision,
        })
    }

    fn extract_capability_from_tool_output(output: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(output).ok()?;
        value
            .get("capability")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
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
            "Determine whether Writer should delegate workers or write directly.\n\
             Use message_writer with:\n\
             - mode \"delegate_researcher\" for external research, verification, or citations\n\
             - mode \"delegate_terminal\" for local execution or repository/system inspection\n\
             - mode \"write_revision\" to compose document content directly (for editorial tasks)\n\
             If both delegation types are needed, call both in the same run.\n\
             If no delegation is needed, compose the content using write_revision, then call finished.\n\
             \n\
             Objective:\n{objective_text}"
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

        // Resolve head version so the delegation adapter can write revisions.
        let parent_version_id = run_id
            .as_ref()
            .and_then(|rid| Self::resolve_run_document(state, rid).ok())
            .and_then(|doc| doc.head_version().ok())
            .map(|v| v.version_id);

        let adapter = WriterDelegationAdapter::new(
            state.writer_id.clone(),
            state.user_id.clone(),
            state.event_store.clone(),
            myself.clone(),
            state.researcher_supervisor.clone(),
            state.terminal_supervisor.clone(),
            run_id.clone(),
            parent_version_id,
        );
        let harness = AgentHarness::with_config(
            adapter,
            state.model_registry.clone(),
            HarnessConfig {
                timeout_budget_ms: timeout_ms.unwrap_or(180_000),
                // Keep writer orchestration shallow so workers are dispatched early.
                max_steps: usize::from(max_steps.unwrap_or(100)).min(2),
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

        // If no workers were dispatched, trigger OrchestrateUserPrompt immediately
        // so the Writer LLM composes the document content via write_revision.
        // When workers are dispatched, handle_delegation_worker_completed triggers
        // OrchestrateUserPrompt after each worker completes — same mechanism.
        if delegated_capabilities.is_empty() {
            if let Some(rid) = run_id.as_ref() {
                let parent_version_id = Self::resolve_run_document(state, rid)
                    .ok()
                    .and_then(|doc| doc.head_version().ok())
                    .map(|v| v.version_id)
                    .unwrap_or(0);
                let rewrite_call_id = format!("writer-compose-direct:{}", ulid::Ulid::new());

                Self::emit_event(
                    state,
                    "writer.actor.objective_orchestration.compose_triggered",
                    serde_json::json!({
                        "run_id": rid,
                        "call_id": &rewrite_call_id,
                        "parent_version_id": parent_version_id,
                        "reason": "no_delegation",
                    }),
                );

                let _ = myself.send_message(WriterMsg::OrchestrateUserPrompt {
                    run_id: rid.clone(),
                    call_id: rewrite_call_id,
                    objective: objective_text.to_string(),
                    parent_version_id,
                });
            }
        }

        Ok(WriterOrchestrationResult {
            success: result.success,
            summary,
            delegated_capabilities,
            pending_delegations,
        })
    }

    /// Process a user prompt diff through the writer LLM to produce a revision.
    ///
    /// The LLM sees the document, the user's diff, and the user's edited version,
    /// then either produces a revised version directly or delegates to workers first.
    /// Background orchestration for user prompts.
    /// Runs outside the actor's message handler to avoid deadlock — the adapter
    /// calls back into the writer actor via `CreateWriterDocumentVersion`.
    #[allow(clippy::too_many_arguments)]
    async fn orchestrate_user_prompt_bg(
        myself: &ActorRef<WriterMsg>,
        writer_id: String,
        user_id: String,
        event_store: ActorRef<EventStoreMsg>,
        researcher_supervisor: Option<ActorRef<ResearcherSupervisorMsg>>,
        terminal_supervisor: Option<ActorRef<TerminalSupervisorMsg>>,
        model_registry: ModelRegistry,
        run_id: String,
        call_id: String,
        objective: String,
        parent_version_id: u64,
    ) {
        let _ = event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "writer.actor.user_prompt_orchestration.dispatched".to_string(),
                payload: serde_json::json!({
                    "run_id": &run_id,
                    "call_id": &call_id,
                    "parent_version_id": parent_version_id,
                    "objective": "Review the user's edits and produce a revised document.",
                    "phase": "run_start",
                    "status": "running",
                    "message": "Writer reprompt orchestration started",
                }),
                actor_id: writer_id.clone(),
                user_id: user_id.clone(),
            },
        });

        let adapter = WriterUserPromptAdapter::new(
            writer_id.clone(),
            user_id.clone(),
            event_store.clone(),
            myself.clone(),
            researcher_supervisor,
            terminal_supervisor,
            run_id.clone(),
            parent_version_id,
        );
        let harness = AgentHarness::with_config(
            adapter,
            model_registry,
            HarnessConfig {
                timeout_budget_ms: 180_000,
                max_steps: 5,
                emit_progress: true,
                emit_worker_report: true,
            },
            LlmTraceEmitter::new(event_store.clone()),
        );

        let result = harness
            .run(
                format!("{writer_id}:{call_id}"),
                user_id.clone(),
                objective,
                None,
                None,
                Some(run_id.clone()),
                Some(call_id.clone()),
            )
            .await;

        let (event_type, payload) = match result {
            Ok(run_result) => {
                let summary = run_result.summary;
                (
                    "writer.actor.user_prompt_orchestration.completed",
                    serde_json::json!({
                        "run_id": &run_id,
                        "call_id": &call_id,
                        "success": run_result.success,
                        "phase": "completion",
                        "status": "completed",
                        "message": &summary,
                        "summary": summary,
                    }),
                )
            }
            Err(e) => {
                let error = e.to_string();
                (
                    "writer.actor.user_prompt_orchestration.failed",
                    serde_json::json!({
                        "run_id": &run_id,
                        "call_id": &call_id,
                        "phase": "failure",
                        "status": "failed",
                        "message": "Writer reprompt orchestration failed",
                        "error_message": &error,
                        "error": error,
                    }),
                )
            }
        };
        let _ = event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: event_type.to_string(),
                payload,
                actor_id: writer_id,
                user_id,
            },
        });
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
            session_id,
            thread_id,
            document_path,
            revision,
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
                    let impact = match summary.impact {
                        ImpactLevel::Low => shared_types::ChangesetImpact::Low,
                        ImpactLevel::Medium => shared_types::ChangesetImpact::Medium,
                        ImpactLevel::High => shared_types::ChangesetImpact::High,
                    };
                    let mut payload =
                        serde_json::to_value(shared_types::WriterRunEvent::Changeset {
                            base: shared_types::WriterRunEventBase {
                                desktop_id,
                                session_id,
                                thread_id,
                                run_id,
                                document_path,
                                revision,
                                head_version_id: Some(target_version_id),
                                timestamp: chrono::Utc::now(),
                            },
                            payload: shared_types::WriterRunChangesetPayload {
                                patch_id,
                                loop_id: None,
                                target_version_id: Some(target_version_id),
                                source: Some(source),
                                summary: summary.summary,
                                impact,
                                op_taxonomy: summary.op_taxonomy,
                            },
                        })
                        .unwrap_or(serde_json::Value::Null);
                    if let Some(object) = payload.as_object_mut() {
                        object.remove("event_type");
                    }
                    let _ = event_store.cast(EventStoreMsg::AppendAsync {
                        event: AppendEvent {
                            event_type: shared_types::EVENT_TOPIC_WRITER_RUN_CHANGESET.to_string(),
                            payload,
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
        source_refs: Vec<String>,
    ) -> Result<u64, WriterError> {
        Self::ensure_run_document_loaded(state, &run_id).await?;
        if message.trim().is_empty() {
            return Err(WriterError::Validation(
                "message cannot be empty".to_string(),
            ));
        }
        let run_doc = Self::resolve_run_document_mut(state, &run_id)?;
        let revision = run_doc
            .report_section_progress(
                &run_id,
                source.as_str(),
                &section_id,
                &phase,
                &message,
                source_refs.clone(),
            )
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
                "source_refs": source_refs,
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
        _myself: &ActorRef<WriterMsg>,
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
        let _ = call_id;

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

        // Delegation completion is control-plane metadata. Keep it in marginalia/progress
        // lane so the canonical document is reserved for actual worker content updates.
        let phase = if success {
            "delegation_worker_completed"
        } else {
            "delegation_worker_failed"
        };
        let message = if success {
            format!("Delegated {capability:?} worker completed. Summary: {summary}")
        } else {
            format!("Delegated {capability:?} worker failed. Error: {summary}")
        };
        if let Err(error) = Self::report_progress(
            state,
            run_id.clone(),
            section_id.clone(),
            source,
            phase.to_string(),
            message,
            Vec::new(),
        )
        .await
        {
            Self::emit_event(
                state,
                "writer.actor.delegation_worker.progress_failed",
                serde_json::json!({
                    "dispatch_id": dispatch_id,
                    "run_id": run_id,
                    "section_id": section_id,
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

        // Bug 4+5 fix: Re-invoke the Writer LLM after worker completion so
        // it can incorporate the worker's findings into a revised document.
        // Without this, the original harness has already finished and the
        // worker results sit as progress events but never become content.
        if success {
            if let Ok(run_doc) = Self::resolve_run_document(state, &run_id) {
                if let Ok(head) = run_doc.head_version() {
                    let parent_version_id = head.version_id;
                    let current_content = head.content.clone();
                    let rewrite_call_id =
                        format!("writer-rewrite-after-worker:{}", ulid::Ulid::new());

                    let objective = format!(
                        "A delegated {capability:?} worker has completed. \
                         Incorporate its findings into the document.\n\n\
                         ## Worker Summary\n\n{summary}\n\n\
                         ## Current Document\n\n{current_content}\n\n\
                         Review the worker's results and produce a revised version \
                         that integrates the findings naturally into the document. \
                         Always call write_revision with the updated content, \
                         then call finished."
                    );

                    Self::emit_event(
                        state,
                        "writer.actor.delegation_worker.rewrite_triggered",
                        serde_json::json!({
                            "run_id": &run_id,
                            "dispatch_id": &dispatch_id,
                            "capability": format!("{capability:?}"),
                            "parent_version_id": parent_version_id,
                            "rewrite_call_id": &rewrite_call_id,
                        }),
                    );

                    let _ = _myself.send_message(WriterMsg::OrchestrateUserPrompt {
                        run_id,
                        call_id: rewrite_call_id,
                        objective,
                        parent_version_id,
                    });
                }
            }
        }
    }

    /// 3.2: Emit citation.confirmed or citation.rejected events based on delegation success.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    fn run_dir(run_id: &str) -> PathBuf {
        crate::paths::writer_root()
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
