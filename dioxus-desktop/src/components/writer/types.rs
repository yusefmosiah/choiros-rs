//! Writer data types

use crate::api::files_api::DirectoryEntry;

/// Save state for the document
#[derive(Debug, Clone, PartialEq)]
pub enum SaveState {
    /// No unsaved changes
    Clean,
    /// Has unsaved changes
    Dirty,
    /// Save in progress
    Saving,
    /// Recently saved (show success briefly)
    Saved,
    /// Conflict detected with server state
    Conflict {
        current_revision: u64,
        current_content: String,
    },
    /// Error state with message
    Error(String),
}

/// Top-level navigation mode for the Writer component
#[derive(Debug, Clone, PartialEq)]
pub enum WriterViewMode {
    /// Root — document list grid
    Overview,
    /// Drill-down — full editor for one document
    Editor,
}

/// Dialog state for file operations
#[derive(Debug, Clone)]
pub enum DialogState {
    None,
    OpenFile {
        current_path: String,
        entries: Vec<DirectoryEntry>,
    },
    SaveAs {
        current_path: String,
        entries: Vec<DirectoryEntry>,
        filename: String,
    },
}

/// Writer component props
#[derive(Debug, Clone, PartialEq)]
pub struct WriterProps {
    pub desktop_id: String,
    pub window_id: String,
    pub initial_path: String,
}

impl WriterProps {
    pub fn new(desktop_id: String, window_id: String, initial_path: String) -> Self {
        Self {
            desktop_id,
            window_id,
            initial_path,
        }
    }
}
