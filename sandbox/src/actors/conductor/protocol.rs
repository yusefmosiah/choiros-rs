//! ConductorActor internal message protocol
//!
//! Defines the messages that can be sent to the ConductorActor and
//! the error types used throughout the conductor system.

use crate::actors::researcher::ResearcherResult;
use crate::actors::terminal::TerminalAgentResult;
use ractor::RpcReplyPort;
use shared_types::{ConductorExecuteRequest, ConductorTaskState, EventMetadata};

/// Messages handled by ConductorActor
#[derive(Debug)]
pub enum ConductorMsg {
    /// Execute a new task (legacy, for compatibility)
    ExecuteTask {
        request: ConductorExecuteRequest,
        reply: RpcReplyPort<Result<ConductorTaskState, ConductorError>>,
    },
    /// Get the current state of a task (legacy)
    GetTaskState {
        task_id: String,
        reply: RpcReplyPort<Option<ConductorTaskState>>,
    },
    /// Receive a result from a run-scoped capability call
    CapabilityCallFinished {
        run_id: String,
        call_id: String,
        agenda_item_id: String,
        capability: String,
        result: Result<CapabilityWorkerOutput, ConductorError>,
    },

    /// Process an event with wake policy
    ProcessEvent {
        run_id: String,
        event_type: String,
        payload: serde_json::Value,
        metadata: EventMetadata,
    },
    /// Check and dispatch ready agenda items
    DispatchReady { run_id: String },
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
    /// Task not found
    #[error("task not found: {0}")]
    NotFound(String),
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
    /// Task already exists
    #[error("task already exists: {0}")]
    DuplicateTask(String),
    /// Policy decision failed
    #[error("policy error: {0}")]
    PolicyError(String),
    /// File operation error
    #[error("file error: {0}")]
    FileError(String),
}

impl From<ConductorError> for shared_types::ConductorError {
    fn from(err: ConductorError) -> Self {
        shared_types::ConductorError {
            code: match &err {
                ConductorError::NotFound(_) => "NOT_FOUND",
                ConductorError::InvalidRequest(_) => "INVALID_REQUEST",
                ConductorError::WorkerFailed(_) => "WORKER_FAILED",
                ConductorError::WorkerBlocked(_) => "WORKER_BLOCKED",
                ConductorError::ReportWriteFailed(_) => "REPORT_WRITE_FAILED",
                ConductorError::DuplicateTask(_) => "DUPLICATE_TASK",
                ConductorError::PolicyError(_) => "POLICY_ERROR",
                ConductorError::FileError(_) => "FILE_ERROR",
            }
            .to_string(),
            message: err.to_string(),
            failure_kind: Some(match err {
                ConductorError::NotFound(_) => shared_types::FailureKind::Unknown,
                ConductorError::InvalidRequest(_) => shared_types::FailureKind::Validation,
                ConductorError::WorkerFailed(_) => shared_types::FailureKind::Provider,
                ConductorError::WorkerBlocked(_) => shared_types::FailureKind::Provider,
                ConductorError::ReportWriteFailed(_) => shared_types::FailureKind::Unknown,
                ConductorError::DuplicateTask(_) => shared_types::FailureKind::Validation,
                ConductorError::PolicyError(_) => shared_types::FailureKind::Provider,
                ConductorError::FileError(_) => shared_types::FailureKind::Unknown,
            }),
        }
    }
}
