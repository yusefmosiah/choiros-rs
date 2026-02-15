//! RunWriterActor message types.
//!
//! Commands for mutating run documents with single-writer authority.

use ractor::RpcReplyPort;
use serde::{Deserialize, Serialize};

use super::state::{
    DocumentVersion, Overlay, OverlayAuthor, OverlayKind, OverlayStatus, VersionSource,
};

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
    /// Legacy patch mutation path kept for compatibility during migration.
    ApplyPatch {
        run_id: String,
        source: String,
        section_id: String,
        ops: Vec<PatchOp>,
        proposal: bool,
        reply: RpcReplyPort<Result<ApplyPatchResult, RunWriterError>>,
    },
    /// Legacy path. Avoid for normal worker progress streaming.
    AppendLogLine {
        run_id: String,
        source: String,
        section_id: String,
        text: String,
        proposal: bool,
        reply: RpcReplyPort<Result<u64, RunWriterError>>,
    },
    /// Emit a concise worker status tick without mutating the document.
    ReportSectionProgress {
        run_id: String,
        source: String,
        section_id: String,
        phase: String,
        message: String,
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
    GetHeadVersion {
        reply: RpcReplyPort<Result<DocumentVersion, RunWriterError>>,
    },
    GetVersion {
        version_id: u64,
        reply: RpcReplyPort<Result<DocumentVersion, RunWriterError>>,
    },
    ListVersions {
        reply: RpcReplyPort<Result<Vec<DocumentVersion>, RunWriterError>>,
    },
    ListOverlays {
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
        reply: RpcReplyPort<Result<Vec<Overlay>, RunWriterError>>,
    },
    CreateVersion {
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
        reply: RpcReplyPort<Result<DocumentVersion, RunWriterError>>,
    },
    CreateOverlay {
        run_id: String,
        base_version_id: u64,
        author: OverlayAuthor,
        kind: OverlayKind,
        diff_ops: Vec<shared_types::PatchOp>,
        reply: RpcReplyPort<Result<Overlay, RunWriterError>>,
    },
    ResolveOverlay {
        run_id: String,
        overlay_id: String,
        status: OverlayStatus,
        reply: RpcReplyPort<Result<Overlay, RunWriterError>>,
    },
    /// Legacy commit path kept for compatibility.
    CommitProposal {
        section_id: String,
        reply: RpcReplyPort<Result<u64, RunWriterError>>,
    },
    /// Legacy discard path kept for compatibility.
    DiscardProposal {
        section_id: String,
        reply: RpcReplyPort<Result<(), RunWriterError>>,
    },
    /// Legacy canonical rewrite path kept for compatibility.
    SetSectionContent {
        run_id: String,
        source: String,
        section_id: String,
        content: String,
        reply: RpcReplyPort<Result<u64, RunWriterError>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchResult {
    pub revision: u64,
    pub lines_modified: usize,
    pub base_version_id: u64,
    pub target_version_id: Option<u64>,
    pub overlay_id: Option<String>,
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

    #[error("Version not found: {0}")]
    VersionNotFound(u64),

    #[error("Overlay not found: {0}")]
    OverlayNotFound(String),

    #[error("Invalid base version: {requested} (head is {head})")]
    InvalidBaseVersion { requested: u64, head: u64 },

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
