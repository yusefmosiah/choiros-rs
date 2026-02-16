//! ConductorActor internal message protocol
//!
//! Defines the messages that can be sent to the ConductorActor and
//! the error types used throughout the conductor system.

use crate::actors::researcher::ResearcherResult;
use crate::actors::terminal::TerminalAgentResult;
use crate::actors::terminal::TerminalMsg;
use crate::actors::writer::WriterQueueAck;
use crate::actors::writer::{DocumentVersion, Overlay, OverlayStatus, VersionSource};
use ractor::RpcReplyPort;
use shared_types::{ConductorExecuteRequest, ConductorRunState, EventMetadata};

/// Messages handled by ConductorActor
#[derive(Debug)]
pub enum ConductorMsg {
    /// Execute a new run.
    ExecuteTask {
        request: ConductorExecuteRequest,
        reply: RpcReplyPort<Result<ConductorRunState, ConductorError>>,
    },
    /// Perform initial conduct + worker dispatch asynchronously after run acceptance.
    StartRun {
        run_id: String,
        request: ConductorExecuteRequest,
    },
    /// Refresh run-time actor dependencies for an existing conductor instance.
    SyncDependencies {
        researcher_actor: Option<ractor::ActorRef<crate::actors::researcher::ResearcherMsg>>,
        terminal_actor: Option<ractor::ActorRef<TerminalMsg>>,
        writer_actor: Option<ractor::ActorRef<crate::actors::writer::WriterMsg>>,
    },
    /// Get the current state of a run.
    GetRunState {
        run_id: String,
        reply: RpcReplyPort<Option<ConductorRunState>>,
    },
    /// Receive a result from a run-scoped capability call
    CapabilityCallFinished {
        run_id: String,
        call_id: String,
        agenda_item_id: String,
        capability: String,
        result: Result<CapabilityWorkerOutput, ConductorError>,
    },

    /// Process an event with lane metadata
    ProcessEvent {
        run_id: String,
        event_type: String,
        payload: serde_json::Value,
        metadata: EventMetadata,
    },
    /// Submit a human prompt to the run-scoped writer inbox.
    SubmitUserPrompt {
        run_id: String,
        prompt_diff: Vec<shared_types::PatchOp>,
        base_version_id: u64,
        reply: RpcReplyPort<Result<WriterQueueAck, ConductorError>>,
    },
    /// List run document versions from writer document runtime.
    ListWriterDocumentVersions {
        run_id: String,
        reply: RpcReplyPort<Result<Vec<DocumentVersion>, ConductorError>>,
    },
    /// Fetch a specific run document version from writer document runtime.
    GetWriterDocumentVersion {
        run_id: String,
        version_id: u64,
        reply: RpcReplyPort<Result<DocumentVersion, ConductorError>>,
    },
    /// List overlays for a run document.
    ListWriterDocumentOverlays {
        run_id: String,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
        reply: RpcReplyPort<Result<Vec<Overlay>, ConductorError>>,
    },
    /// Create a canonical version for a run document.
    CreateWriterDocumentVersion {
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
        reply: RpcReplyPort<Result<DocumentVersion, ConductorError>>,
    },
}

/// Output from a worker task
#[derive(Debug, Clone)]
pub struct WorkerOutput {
    /// The report content in markdown format
    pub report_content: String,
    /// Citations from research (using ResearcherActor's ResearchCitation)
    pub citations: Vec<crate::actors::researcher::ResearchCitation>,
}

/// Typed output from a run-scoped capability call.
#[derive(Debug, Clone)]
pub enum CapabilityWorkerOutput {
    Researcher(ResearcherResult),
    Terminal(TerminalAgentResult),
}

/// Errors that can occur in ConductorActor
#[derive(Debug, thiserror::Error, Clone)]
pub enum ConductorError {
    /// Run not found
    #[error("run not found: {0}")]
    NotFound(String),
    /// Required actor/capability is unavailable
    #[error("actor not available: {0}")]
    ActorUnavailable(String),
    /// Invalid request parameters
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// Worker failed to complete task
    #[error("worker failed: {0}")]
    WorkerFailed(String),
    /// Worker could not proceed and returned a blocked state
    #[error("worker blocked: {0}")]
    WorkerBlocked(String),
    /// Report write failed
    #[error("report write failed: {0}")]
    ReportWriteFailed(String),
    /// Run already exists
    #[error("run already exists: {0}")]
    DuplicateRun(String),
    /// Model gateway decision failed
    #[error("model gateway error: {0}")]
    ModelGatewayError(String),
    /// File operation error
    #[error("file error: {0}")]
    FileError(String),
}

impl From<ConductorError> for shared_types::ConductorError {
    fn from(err: ConductorError) -> Self {
        shared_types::ConductorError {
            code: match &err {
                ConductorError::NotFound(_) => "NOT_FOUND",
                ConductorError::ActorUnavailable(_) => "ACTOR_NOT_AVAILABLE",
                ConductorError::InvalidRequest(_) => "INVALID_REQUEST",
                ConductorError::WorkerFailed(_) => "WORKER_FAILED",
                ConductorError::WorkerBlocked(_) => "WORKER_BLOCKED",
                ConductorError::ReportWriteFailed(_) => "REPORT_WRITE_FAILED",
                ConductorError::DuplicateRun(_) => "DUPLICATE_RUN",
                ConductorError::ModelGatewayError(_) => "MODEL_GATEWAY_ERROR",
                ConductorError::FileError(_) => "FILE_ERROR",
            }
            .to_string(),
            message: err.to_string(),
            failure_kind: Some(match err {
                ConductorError::NotFound(_) => shared_types::FailureKind::Unknown,
                ConductorError::ActorUnavailable(_) => shared_types::FailureKind::Unknown,
                ConductorError::InvalidRequest(_) => shared_types::FailureKind::Validation,
                ConductorError::WorkerFailed(_) => shared_types::FailureKind::Provider,
                ConductorError::WorkerBlocked(_) => shared_types::FailureKind::Provider,
                ConductorError::ReportWriteFailed(_) => shared_types::FailureKind::Unknown,
                ConductorError::DuplicateRun(_) => shared_types::FailureKind::Validation,
                ConductorError::ModelGatewayError(_) => shared_types::FailureKind::Provider,
                ConductorError::FileError(_) => shared_types::FailureKind::Unknown,
            }),
        }
    }
}
