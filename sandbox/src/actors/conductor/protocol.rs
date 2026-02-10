//! ConductorActor internal message protocol
//!
//! Defines the messages that can be sent to the ConductorActor and
//! the error types used throughout the conductor system.

use ractor::RpcReplyPort;
use shared_types::{ConductorExecuteRequest, ConductorTaskState};

/// Messages handled by ConductorActor
#[derive(Debug)]
pub enum ConductorMsg {
    /// Execute a new task
    ExecuteTask {
        request: ConductorExecuteRequest,
        reply: RpcReplyPort<Result<ConductorTaskState, ConductorError>>,
    },
    /// Get the current state of a task
    GetTaskState {
        task_id: String,
        reply: RpcReplyPort<Option<ConductorTaskState>>,
    },
    /// Receive a result from a worker
    WorkerResult {
        task_id: String,
        result: Result<WorkerOutput, ConductorError>,
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
    /// Report write failed
    #[error("report write failed: {0}")]
    ReportWriteFailed(String),
    /// Task already exists
    #[error("task already exists: {0}")]
    DuplicateTask(String),
}

impl From<ConductorError> for shared_types::ConductorError {
    fn from(err: ConductorError) -> Self {
        shared_types::ConductorError {
            code: match &err {
                ConductorError::NotFound(_) => "NOT_FOUND",
                ConductorError::InvalidRequest(_) => "INVALID_REQUEST",
                ConductorError::WorkerFailed(_) => "WORKER_FAILED",
                ConductorError::ReportWriteFailed(_) => "REPORT_WRITE_FAILED",
                ConductorError::DuplicateTask(_) => "DUPLICATE_TASK",
            }
            .to_string(),
            message: err.to_string(),
            failure_kind: Some(match err {
                ConductorError::NotFound(_) => shared_types::FailureKind::Unknown,
                ConductorError::InvalidRequest(_) => shared_types::FailureKind::Validation,
                ConductorError::WorkerFailed(_) => shared_types::FailureKind::Provider,
                ConductorError::ReportWriteFailed(_) => shared_types::FailureKind::Unknown,
                ConductorError::DuplicateTask(_) => shared_types::FailureKind::Validation,
            }),
        }
    }
}
