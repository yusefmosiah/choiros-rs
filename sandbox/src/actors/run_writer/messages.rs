//! RunWriterActor message types.
//!
//! Commands for mutating run documents with single-writer authority.

use ractor::RpcReplyPort;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchOp {
    pub kind: PatchOpKind,
    pub position: Option<usize>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchOpKind {
    Insert,
    Delete,
    Replace,
    Append,
}

#[derive(Debug)]
pub enum RunWriterMsg {
    ApplyPatch {
        run_id: String,
        source: String,
        section_id: String,
        ops: Vec<PatchOp>,
        proposal: bool,
        reply: RpcReplyPort<Result<ApplyPatchResult, RunWriterError>>,
    },
    AppendLogLine {
        run_id: String,
        source: String,
        section_id: String,
        text: String,
        proposal: bool,
        reply: RpcReplyPort<Result<u64, RunWriterError>>,
    },
    MarkSectionState {
        run_id: String,
        section_id: String,
        state: SectionState,
        reply: RpcReplyPort<Result<(), RunWriterError>>,
    },
    GetDocument {
        reply: RpcReplyPort<Result<String, RunWriterError>>,
    },
    GetRevision {
        reply: RpcReplyPort<u64>,
    },
    CommitProposal {
        section_id: String,
        reply: RpcReplyPort<Result<u64, RunWriterError>>,
    },
    DiscardProposal {
        section_id: String,
        reply: RpcReplyPort<Result<(), RunWriterError>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchResult {
    pub revision: u64,
    pub lines_modified: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SectionState {
    #[default]
    Pending,
    Running,
    Complete,
    Failed,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum RunWriterError {
    #[error("Section not found: {0}")]
    SectionNotFound(String),

    #[error("Invalid patch operation: {0}")]
    InvalidPatch(String),

    #[error("Document write failed: {0}")]
    WriteFailed(String),

    #[error("Run ID mismatch: expected {expected}, got {actual}")]
    RunIdMismatch { expected: String, actual: String },

    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<std::io::Error> for RunWriterError {
    fn from(e: std::io::Error) -> Self {
        RunWriterError::Io(e.to_string())
    }
}
