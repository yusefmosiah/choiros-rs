//! Run document mutation/value types used by WriterActor.

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
pub enum WriterDocumentError {
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

impl From<std::io::Error> for WriterDocumentError {
    fn from(e: std::io::Error) -> Self {
        WriterDocumentError::Io(e.to_string())
    }
}
