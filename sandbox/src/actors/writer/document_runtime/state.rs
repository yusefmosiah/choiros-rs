//! Run document runtime state types.

use chrono::{DateTime, Utc};
use ractor::ActorRef;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::actors::event_store::EventStoreMsg;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionSource {
    Writer,
    UserSave,
    #[default]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAuthor {
    User,
    Researcher,
    Terminal,
    #[default]
    Writer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayKind {
    Comment,
    #[default]
    Proposal,
    WorkerCompletion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayStatus {
    #[default]
    Pending,
    Superseded,
    Applied,
    Discarded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub version_id: u64,
    pub created_at: DateTime<Utc>,
    pub source: VersionSource,
    pub content: String,
    pub parent_version_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overlay {
    pub overlay_id: String,
    pub base_version_id: u64,
    pub author: OverlayAuthor,
    pub kind: OverlayKind,
    pub diff_ops: Vec<shared_types::PatchOp>,
    pub status: OverlayStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDocument {
    pub objective: String,
    pub versions: Vec<DocumentVersion>,
    pub overlays: Vec<Overlay>,
    pub head_version_id: u64,
}

impl Default for RunDocument {
    fn default() -> Self {
        let now = Utc::now();
        let base = DocumentVersion {
            version_id: 0,
            created_at: now,
            source: VersionSource::System,
            content: String::new(),
            parent_version_id: None,
        };
        Self {
            objective: String::new(),
            versions: vec![base],
            overlays: Vec::new(),
            head_version_id: 0,
        }
    }
}

impl RunDocument {
    pub fn new(objective: impl Into<String>) -> Self {
        Self {
            objective: objective.into(),
            ..Default::default()
        }
    }

    pub fn head_version(&self) -> Option<&DocumentVersion> {
        self.versions
            .iter()
            .find(|version| version.version_id == self.head_version_id)
    }

    pub fn get_version(&self, version_id: u64) -> Option<&DocumentVersion> {
        self.versions
            .iter()
            .find(|version| version.version_id == version_id)
    }

    pub fn get_overlay(&self, overlay_id: &str) -> Option<&Overlay> {
        self.overlays
            .iter()
            .find(|overlay| overlay.overlay_id == overlay_id)
    }

    pub fn get_overlay_mut(&mut self, overlay_id: &str) -> Option<&mut Overlay> {
        self.overlays
            .iter_mut()
            .find(|overlay| overlay.overlay_id == overlay_id)
    }

    pub fn next_version_id(&self) -> u64 {
        self.versions
            .iter()
            .map(|version| version.version_id)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("# {}\n\n", self.objective);
        if let Some(head) = self.head_version() {
            if !head.content.trim().is_empty() {
                md.push_str(head.content.trim());
                md.push('\n');
            }
        }
        md
    }

    pub fn from_legacy_markdown(md: &str) -> Result<Self, String> {
        let mut objective = String::new();
        let mut canonical_lines = Vec::<String>::new();
        let mut proposal_lines = Vec::<String>::new();
        let mut in_proposal = false;

        for raw in md.lines() {
            let line = raw.trim_end();
            if line.starts_with("<!-- revision:") {
                continue;
            }
            if let Some(rest) = line.strip_prefix("# ") {
                objective = rest.trim().to_string();
                continue;
            }
            if line.trim() == "<!-- proposal -->" {
                in_proposal = true;
                continue;
            }
            if line.trim() == "<!-- /proposal -->" {
                in_proposal = false;
                continue;
            }
            if line.starts_with("## ") {
                // Legacy section headers are formatting-only for migration.
                continue;
            }
            if in_proposal {
                proposal_lines.push(line.to_string());
            } else {
                canonical_lines.push(line.to_string());
            }
        }

        if objective.trim().is_empty() {
            return Err("missing document objective".to_string());
        }

        let canonical = canonical_lines.join("\n").trim().to_string();
        let now = Utc::now();
        let mut doc = Self {
            objective,
            versions: vec![DocumentVersion {
                version_id: 1,
                created_at: now,
                source: VersionSource::System,
                content: canonical.clone(),
                parent_version_id: None,
            }],
            overlays: Vec::new(),
            head_version_id: 1,
        };

        let proposal = proposal_lines.join("\n").trim().to_string();
        if !proposal.is_empty() {
            let insert_pos = canonical.chars().count() as u64;
            let prefix = if canonical.is_empty() { "" } else { "\n\n" };
            doc.overlays.push(Overlay {
                overlay_id: ulid::Ulid::new().to_string(),
                base_version_id: 1,
                author: OverlayAuthor::Researcher,
                kind: OverlayKind::Proposal,
                diff_ops: vec![shared_types::PatchOp::Insert {
                    pos: insert_pos,
                    text: format!("{prefix}{proposal}"),
                }],
                status: OverlayStatus::Pending,
                created_at: now,
            });
        }

        Ok(doc)
    }
}

pub struct WriterDocumentState {
    pub run_id: String,
    pub desktop_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub objective: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub document_path: PathBuf,
    pub document_meta_path: PathBuf,
    pub document_path_relative: String,
    pub revision: u64,
    pub document: RunDocument,
}
