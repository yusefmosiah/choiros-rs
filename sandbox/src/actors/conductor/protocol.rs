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
    /// List all runs, sorted by created_at descending.
    ListRuns {
        reply: RpcReplyPort<Vec<ConductorRunState>>,
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
    // Phase 2.4 — HarnessActor completion messages
    // -----------------------------------------------------------------------
    /// A HarnessActor completed its objective successfully.
    HarnessComplete {
        /// Opaque correlation handle supplied at spawn time.
        correlation_id: String,
        result: HarnessResult,
    },
    /// A HarnessActor failed (panicked or returned an error).
    HarnessFailed {
        correlation_id: String,
        reason: String,
    },

    // -----------------------------------------------------------------------
    // Phase 4 — HarnessActor in-flight progress
    // -----------------------------------------------------------------------
    /// Intermediate progress report from a running HarnessActor.
    ///
    /// Sent via the `message_writer` tool reinterpreted as parent messaging.
    /// Non-blocking — conductor logs/persists but does not reply.
    HarnessProgress {
        correlation_id: String,
        /// Report kind: "progress", "status", "finding", etc.
        kind: String,
        /// Human-readable content of the report.
        content: String,
        /// Full structured metadata (superset of kind + content).
        metadata: serde_json::Value,
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
    Harness(HarnessResult),
}

// ---------------------------------------------------------------------------
// Phase 2.4 — HarnessActor types
// ---------------------------------------------------------------------------

/// Messages handled by HarnessActor.
///
/// HarnessActor is a one-shot actor: it receives a single `Execute`
/// message, runs to completion, sends a typed reply to conductor, then stops.
#[derive(Debug)]
pub enum HarnessMsg {
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

/// Completion payload returned by a HarnessActor.
#[derive(Debug, Clone)]
pub struct HarnessResult {
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
// Phase 4.2 — NextAction types (Rust-side mirror of ConductorAction BAML)
// ---------------------------------------------------------------------------

/// Strongly-typed next action for conductor decision steps.
///
/// Mirrors the `ConductorAction` BAML enum, extended with Phase 4 variants.
/// Used by the conductor ALM harness turn (Phase 4.3) rather than
/// the bootstrap path which still uses `ConductorBootstrapOutput`.
#[derive(Debug, Clone)]
pub enum NextAction {
    /// Dispatch a named worker capability.
    SpawnWorker {
        /// Capability name: "writer", "researcher", "terminal".
        capability: String,
        /// Objective to pass to the worker.
        objective: String,
    },
    /// Wait for in-flight capability calls to complete.
    AwaitWorker,
    /// Merge completed worker proposals into the canonical document.
    MergeCanon,
    /// Run is complete — no further steps needed.
    Complete { reason: String },
    /// Run is blocked and cannot proceed.
    Block { reason: String },
    /// Spawn a `HarnessActor` for a bounded scoped task.
    SpawnHarness {
        /// Plain-language task description for the sub-agent.
        task: String,
        /// Context bundle (JSON) passed to HarnessActor.
        context: serde_json::Value,
    },
    /// Delegate a task to a named worker kind.
    Delegate {
        worker_kind: WorkerKind,
        task: String,
    },
}

/// Worker kind for the `Delegate` action variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind {
    Researcher,
    Writer,
    Terminal,
    Harness,
}

impl std::fmt::Display for WorkerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerKind::Researcher => write!(f, "researcher"),
            WorkerKind::Writer => write!(f, "writer"),
            WorkerKind::Terminal => write!(f, "terminal"),
            WorkerKind::Harness => write!(f, "harness"),
        }
    }
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
