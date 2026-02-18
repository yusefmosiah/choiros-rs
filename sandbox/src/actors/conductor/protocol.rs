//! ConductorActor internal message protocol
//!
//! Defines the messages that can be sent to the ConductorActor and
//! the error types used throughout the conductor system.

use crate::actors::researcher::ResearcherResult;
use crate::actors::terminal::TerminalAgentResult;
use crate::actors::writer::{WriterOrchestrationResult, WriterQueueAck};
use ractor::{ActorRef, RpcReplyPort};
use shared_types::{CitationRecord, ConductorExecuteRequest, ConductorRunState, EventMetadata};

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

    // -----------------------------------------------------------------------
    // Phase 2.4 — SubharnessActor completion messages
    // -----------------------------------------------------------------------
    /// A SubharnessActor completed its objective successfully.
    SubharnessComplete {
        /// Opaque correlation handle supplied at spawn time.
        correlation_id: String,
        result: SubharnessResult,
    },
    /// A SubharnessActor failed (panicked or returned an error).
    SubharnessFailed {
        correlation_id: String,
        reason: String,
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
    Writer(WriterOrchestrationResult),
    ImmediateResponse(String),
    Subharness(SubharnessResult),
}

// ---------------------------------------------------------------------------
// Phase 2.4 — SubharnessActor types
// ---------------------------------------------------------------------------

/// Messages handled by SubharnessActor.
///
/// SubharnessActor is a one-shot actor: it receives a single `Execute`
/// message, runs to completion, sends a typed reply to conductor, then stops.
#[derive(Debug)]
pub enum SubharnessMsg {
    /// Execute a scoped objective.
    Execute {
        /// Plain-language objective for this subharness run.
        objective: String,
        /// Serialised context bundle (retrieved artifacts, prior state, etc.)
        context: serde_json::Value,
        /// Opaque correlation handle returned unchanged in the completion message.
        correlation_id: String,
        /// Conductor's mailbox — subharness sends completion here.
        reply_to: ActorRef<ConductorMsg>,
    },
}

/// Completion payload returned by a SubharnessActor.
#[derive(Debug, Clone)]
pub struct SubharnessResult {
    /// Final output text (markdown, JSON, or plain prose).
    pub output: String,
    /// Citations produced during the subharness run.
    pub citations: Vec<CitationRecord>,
    /// Whether the subharness considered the objective fully satisfied.
    pub objective_satisfied: bool,
    /// Optional human-readable reason (especially useful on partial completion).
    pub completion_reason: Option<String>,
    /// Number of harness steps taken.
    pub steps_taken: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

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
