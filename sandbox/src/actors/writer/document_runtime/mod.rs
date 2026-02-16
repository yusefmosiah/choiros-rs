//! Run-document runtime used by WriterActor.
//!
//! This replaces the former per-run actor process with an in-process
//! runtime object owned by WriterActor.

mod messages;
mod state;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

pub use messages::{ApplyPatchResult, PatchOp, PatchOpKind, SectionState, WriterDocumentError};
pub use state::{
    DocumentVersion, Overlay, OverlayAuthor, OverlayKind, OverlayStatus, RunDocument,
    VersionSource, WriterDocumentState,
};

use crate::actors::event_store::{AppendEvent, EventStoreMsg};

const BASE_RUNS_DIR: &str = "conductor/runs";

#[derive(Debug, Clone)]
pub struct WriterDocumentArguments {
    pub run_id: String,
    pub desktop_id: String,
    pub objective: String,
    pub session_id: String,
    pub thread_id: String,
    pub root_dir: Option<String>,
    pub event_store: ractor::ActorRef<EventStoreMsg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedWriterDocumentSnapshot {
    revision: u64,
    document: RunDocument,
}

pub struct WriterDocumentRuntime {
    state: WriterDocumentState,
}

impl WriterDocumentRuntime {
    pub async fn load(args: WriterDocumentArguments) -> Result<Self, WriterDocumentError> {
        let run_dir_relative = PathBuf::from(BASE_RUNS_DIR).join(&args.run_id);
        let document_path_relative = run_dir_relative.join("draft.md");
        let document_meta_path_relative = run_dir_relative.join("draft.writer-state.json");

        let root_dir = args
            .root_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let document_path = root_dir.join(&document_path_relative);
        let document_meta_path = root_dir.join(&document_meta_path_relative);

        let (mut document, revision) =
            Self::load_or_create_document(&document_path, &document_meta_path, &args.objective)
                .await
                .map_err(|e| WriterDocumentError::WriteFailed(e.to_string()))?;
        if document.objective.trim().is_empty() {
            document.objective = args.objective.clone();
        }

        let mut runtime = Self {
            state: WriterDocumentState {
                run_id: args.run_id,
                desktop_id: args.desktop_id,
                session_id: args.session_id,
                thread_id: args.thread_id,
                objective: args.objective,
                event_store: args.event_store,
                document_path,
                document_meta_path,
                document_path_relative: document_path_relative.to_string_lossy().to_string(),
                revision,
                document,
            },
        };

        runtime.emit_started_event().await;
        Ok(runtime)
    }

    pub fn run_id(&self) -> &str {
        self.state.run_id.as_str()
    }

    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    pub fn document_markdown(&self) -> String {
        self.state.document.to_markdown()
    }

    pub fn head_version(&self) -> Result<DocumentVersion, WriterDocumentError> {
        self.state
            .document
            .head_version()
            .cloned()
            .ok_or(WriterDocumentError::VersionNotFound(
                self.state.document.head_version_id,
            ))
    }

    pub fn get_version(&self, version_id: u64) -> Result<DocumentVersion, WriterDocumentError> {
        self.state
            .document
            .get_version(version_id)
            .cloned()
            .ok_or(WriterDocumentError::VersionNotFound(version_id))
    }

    pub fn list_versions(&self) -> Vec<DocumentVersion> {
        let mut versions = self.state.document.versions.clone();
        versions.sort_by_key(|version| version.version_id);
        versions
    }

    pub fn list_overlays(
        &self,
        base_version_id: Option<u64>,
        status: Option<OverlayStatus>,
    ) -> Vec<Overlay> {
        let mut overlays: Vec<Overlay> = self
            .state
            .document
            .overlays
            .iter()
            .filter(|overlay| {
                base_version_id
                    .map(|id| overlay.base_version_id == id)
                    .unwrap_or(true)
                    && status
                        .as_ref()
                        .map(|s| &overlay.status == s)
                        .unwrap_or(true)
            })
            .cloned()
            .collect();
        overlays.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        overlays
    }

    pub async fn create_version(
        &mut self,
        run_id: &str,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
    ) -> Result<DocumentVersion, WriterDocumentError> {
        self.ensure_run_id(run_id)?;
        let version = self
            .create_version_internal(parent_version_id, content, source, "writer", None)
            .await?;
        self.emit_progress_event(
            "version_created",
            format!("Created version {}", version.version_id),
        )
        .await;
        Ok(version)
    }

    pub async fn create_overlay(
        &mut self,
        run_id: &str,
        base_version_id: u64,
        author: OverlayAuthor,
        kind: OverlayKind,
        diff_ops: Vec<shared_types::PatchOp>,
    ) -> Result<Overlay, WriterDocumentError> {
        self.ensure_run_id(run_id)?;
        let overlay = self
            .create_overlay_internal(
                base_version_id,
                author,
                kind,
                diff_ops,
                "writer",
                None,
                None,
            )
            .await?;
        self.emit_progress_event(
            "overlay_created",
            format!("Created overlay {}", overlay.overlay_id),
        )
        .await;
        Ok(overlay)
    }

    pub async fn apply_patch(
        &mut self,
        run_id: &str,
        source: &str,
        section_id: &str,
        ops: Vec<PatchOp>,
        proposal: bool,
    ) -> Result<ApplyPatchResult, WriterDocumentError> {
        self.ensure_run_id(run_id)?;

        let base_version_id = self.state.document.head_version_id;
        let base_content = self
            .state
            .document
            .head_version()
            .map(|version| version.content.clone())
            .unwrap_or_default();
        let (next_content, lines_modified) = Self::apply_legacy_line_patch_ops(&base_content, &ops);

        if proposal {
            let diff_ops = Self::diff_full_replace(&base_content, &next_content);
            let overlay = self
                .create_overlay_internal(
                    base_version_id,
                    Self::source_to_overlay_author(source),
                    OverlayKind::Proposal,
                    diff_ops,
                    source,
                    Some(section_id),
                    Some(next_content),
                )
                .await?;
            self.emit_progress_event(
                "patch_applied",
                format!("Created proposal overlay for {section_id} via {source}"),
            )
            .await;
            return Ok(ApplyPatchResult {
                revision: self.state.revision,
                lines_modified,
                base_version_id,
                target_version_id: None,
                overlay_id: Some(overlay.overlay_id),
            });
        }

        let version = self
            .create_version_internal(
                Some(base_version_id),
                next_content,
                Self::source_to_version_source(source),
                source,
                Some(section_id),
            )
            .await?;
        self.emit_progress_event(
            "patch_applied",
            format!("Updated canonical version via {source}"),
        )
        .await;

        Ok(ApplyPatchResult {
            revision: self.state.revision,
            lines_modified,
            base_version_id,
            target_version_id: Some(version.version_id),
            overlay_id: None,
        })
    }

    pub async fn report_section_progress(
        &mut self,
        run_id: &str,
        source: &str,
        section_id: &str,
        phase: &str,
        message: &str,
    ) -> Result<u64, WriterDocumentError> {
        self.ensure_run_id(run_id)?;
        self.emit_progress_event(
            format!("{source}:{section_id}:{phase}"),
            message.to_string(),
        )
        .await;
        Ok(self.state.revision)
    }

    pub async fn mark_section_state(
        &mut self,
        run_id: &str,
        section_id: &str,
        section_state: SectionState,
    ) -> Result<(), WriterDocumentError> {
        self.ensure_run_id(run_id)?;

        let status_message = format!("{section_id} -> {:?}", section_state);
        let status = match section_state {
            SectionState::Pending => shared_types::WriterRunStatusKind::WaitingForWorker,
            SectionState::Running => shared_types::WriterRunStatusKind::Running,
            SectionState::Complete => shared_types::WriterRunStatusKind::Completed,
            SectionState::Failed => shared_types::WriterRunStatusKind::Failed,
        };

        self.emit_status_event(status, Some(status_message)).await;
        Ok(())
    }

    async fn load_or_create_document(
        path: &PathBuf,
        meta_path: &PathBuf,
        objective: &str,
    ) -> Result<(RunDocument, u64), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if meta_path.exists() {
            match fs::read_to_string(meta_path).await {
                Ok(raw) => match serde_json::from_str::<PersistedWriterDocumentSnapshot>(&raw) {
                    Ok(snapshot) => return Ok((snapshot.document, snapshot.revision)),
                    Err(e) => {
                        tracing::warn!(
                            path = %meta_path.display(),
                            error = %e,
                            "Failed to parse writer state sidecar, falling back to markdown"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %meta_path.display(),
                        error = %e,
                        "Failed to read writer state sidecar, falling back to markdown"
                    );
                }
            }
        }

        if path.exists() {
            match fs::read_to_string(path).await {
                Ok(content) => {
                    let revision = Self::extract_revision_from_content(&content);
                    match RunDocument::from_legacy_markdown(&content) {
                        Ok(mut doc) => {
                            if doc.objective.trim().is_empty() {
                                doc.objective = objective.to_string();
                            }
                            return Ok((doc, revision));
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to parse existing document, creating new"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to read existing document, creating new"
                    );
                }
            }
        }

        Ok((RunDocument::new(objective), 0))
    }

    fn extract_revision_from_content(content: &str) -> u64 {
        for line in content.lines() {
            if line.starts_with("<!-- revision:") && line.ends_with(" -->") {
                if let Some(rev_str) = line
                    .strip_prefix("<!-- revision:")
                    .and_then(|s| s.strip_suffix(" -->"))
                {
                    if let Ok(rev) = rev_str.trim().parse::<u64>() {
                        return rev;
                    }
                }
            }
        }
        0
    }

    async fn persist_document(&mut self) -> Result<(), WriterDocumentError> {
        self.state.revision += 1;

        let content = format!(
            "<!-- revision:{} -->\n{}",
            self.state.revision,
            self.state.document.to_markdown()
        );

        let temp_path = self.state.document_path.with_extension("md.tmp");
        fs::write(&temp_path, &content).await.map_err(|e| {
            WriterDocumentError::WriteFailed(format!("Failed to write temp file: {e}"))
        })?;
        fs::rename(&temp_path, &self.state.document_path)
            .await
            .map_err(|e| {
                WriterDocumentError::WriteFailed(format!("Failed to rename temp file: {e}"))
            })?;

        let sidecar = PersistedWriterDocumentSnapshot {
            revision: self.state.revision,
            document: self.state.document.clone(),
        };
        let sidecar_raw = serde_json::to_string_pretty(&sidecar).map_err(|e| {
            WriterDocumentError::WriteFailed(format!("Failed to encode sidecar: {e}"))
        })?;
        let temp_meta = self.state.document_meta_path.with_extension("json.tmp");
        fs::write(&temp_meta, sidecar_raw).await.map_err(|e| {
            WriterDocumentError::WriteFailed(format!("Failed to write sidecar: {e}"))
        })?;
        fs::rename(&temp_meta, &self.state.document_meta_path)
            .await
            .map_err(|e| {
                WriterDocumentError::WriteFailed(format!("Failed to rename sidecar: {e}"))
            })?;

        Ok(())
    }

    fn source_to_patch_source(source: &str) -> shared_types::PatchSource {
        match source.to_ascii_lowercase().as_str() {
            "user" => shared_types::PatchSource::User,
            "system" | "conductor" => shared_types::PatchSource::System,
            _ => shared_types::PatchSource::Agent,
        }
    }

    fn source_to_version_source(source: &str) -> VersionSource {
        match source.to_ascii_lowercase().as_str() {
            "user" => VersionSource::UserSave,
            "writer" => VersionSource::Writer,
            _ => VersionSource::System,
        }
    }

    fn source_to_overlay_author(source: &str) -> OverlayAuthor {
        match source.to_ascii_lowercase().as_str() {
            "user" => OverlayAuthor::User,
            "researcher" => OverlayAuthor::Researcher,
            "terminal" => OverlayAuthor::Terminal,
            _ => OverlayAuthor::Writer,
        }
    }

    fn full_document_ops(content: &str) -> Vec<shared_types::PatchOp> {
        vec![
            shared_types::PatchOp::Delete {
                pos: 0,
                len: u64::MAX,
            },
            shared_types::PatchOp::Insert {
                pos: 0,
                text: content.to_string(),
            },
        ]
    }

    fn diff_full_replace(base: &str, target: &str) -> Vec<shared_types::PatchOp> {
        if base == target {
            return Vec::new();
        }
        vec![
            shared_types::PatchOp::Delete {
                pos: 0,
                len: base.chars().count() as u64,
            },
            shared_types::PatchOp::Insert {
                pos: 0,
                text: target.to_string(),
            },
        ]
    }

    fn apply_legacy_line_patch_ops(content: &str, ops: &[PatchOp]) -> (String, usize) {
        let mut target = content.to_string();
        let mut lines_modified = 0usize;

        for op in ops {
            match op.kind {
                PatchOpKind::Append => {
                    if let Some(text) = &op.text {
                        if !target.is_empty() && !target.ends_with('\n') {
                            target.push('\n');
                        }
                        target.push_str(text);
                        lines_modified += text.lines().count().max(1);
                    }
                }
                PatchOpKind::Insert => {
                    if let (Some(text), Some(pos)) = (&op.text, op.position) {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos <= lines.len() {
                            let mut new_lines = lines;
                            new_lines.insert(pos, text);
                            target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
                PatchOpKind::Delete => {
                    if let Some(pos) = op.position {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos < lines.len() {
                            let mut new_lines = lines;
                            new_lines.remove(pos);
                            target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
                PatchOpKind::Replace => {
                    if let (Some(text), Some(pos)) = (&op.text, op.position) {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos < lines.len() {
                            let mut new_lines = lines;
                            new_lines[pos] = text;
                            target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
            }
        }

        (target, lines_modified)
    }

    fn ensure_run_id(&self, run_id: &str) -> Result<(), WriterDocumentError> {
        if run_id != self.state.run_id {
            return Err(WriterDocumentError::RunIdMismatch {
                expected: self.state.run_id.clone(),
                actual: run_id.to_string(),
            });
        }
        Ok(())
    }

    async fn create_version_internal(
        &mut self,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
        event_source: &str,
        section_id: Option<&str>,
    ) -> Result<DocumentVersion, WriterDocumentError> {
        let parent = parent_version_id.unwrap_or(self.state.document.head_version_id);
        if self.state.document.get_version(parent).is_none() {
            return Err(WriterDocumentError::VersionNotFound(parent));
        }

        let version = DocumentVersion {
            version_id: self.state.document.next_version_id(),
            created_at: Utc::now(),
            source,
            content,
            parent_version_id: Some(parent),
        };
        self.state.document.versions.push(version.clone());
        self.state.document.head_version_id = version.version_id;

        for overlay in &mut self.state.document.overlays {
            if overlay.base_version_id == parent && overlay.status == OverlayStatus::Pending {
                overlay.status = OverlayStatus::Superseded;
            }
        }

        self.persist_document().await?;

        let full_doc = self.state.document.to_markdown();
        self.emit_patch_event(
            event_source,
            section_id,
            Self::full_document_ops(&full_doc),
            None,
            Some(parent),
            Some(version.version_id),
            None,
        )
        .await;

        Ok(version)
    }

    async fn create_overlay_internal(
        &mut self,
        base_version_id: u64,
        author: OverlayAuthor,
        kind: OverlayKind,
        diff_ops: Vec<shared_types::PatchOp>,
        event_source: &str,
        section_id: Option<&str>,
        proposal: Option<String>,
    ) -> Result<Overlay, WriterDocumentError> {
        if self.state.document.get_version(base_version_id).is_none() {
            return Err(WriterDocumentError::VersionNotFound(base_version_id));
        }
        if diff_ops.is_empty() {
            return Err(WriterDocumentError::InvalidPatch(
                "overlay diff_ops cannot be empty".to_string(),
            ));
        }

        let overlay = Overlay {
            overlay_id: ulid::Ulid::new().to_string(),
            base_version_id,
            author,
            kind,
            diff_ops: diff_ops.clone(),
            status: OverlayStatus::Pending,
            created_at: Utc::now(),
        };
        self.state.document.overlays.push(overlay.clone());
        self.persist_document().await?;

        self.emit_patch_event(
            event_source,
            section_id,
            diff_ops,
            proposal,
            Some(base_version_id),
            None,
            Some(&overlay.overlay_id),
        )
        .await;

        Ok(overlay)
    }

    fn base_event_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "desktop_id": self.state.desktop_id,
            "session_id": self.state.session_id,
            "thread_id": self.state.thread_id,
            "run_id": self.state.run_id,
            "document_path": self.state.document_path_relative,
            "revision": self.state.revision,
            "head_version_id": self.state.document.head_version_id,
            "timestamp": Utc::now().to_rfc3339(),
        })
    }

    async fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: format!("writer:{}", self.state.run_id),
            user_id: "system".to_string(),
        };
        let _ = self
            .state
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    async fn emit_started_event(&mut self) {
        let mut payload = self.base_event_payload();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "objective".to_string(),
                serde_json::Value::String(self.state.objective.clone()),
            );
        }
        self.emit_event("writer.run.started", payload).await;
    }

    async fn emit_patch_event(
        &self,
        source: &str,
        section_id: Option<&str>,
        ops: Vec<shared_types::PatchOp>,
        proposal: Option<String>,
        base_version_id: Option<u64>,
        target_version_id: Option<u64>,
        overlay_id: Option<&str>,
    ) {
        let mut payload = self.base_event_payload();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "patch_id".to_string(),
                serde_json::Value::String(ulid::Ulid::new().to_string()),
            );
            object.insert(
                "source".to_string(),
                serde_json::to_value(Self::source_to_patch_source(source))
                    .unwrap_or_else(|_| serde_json::Value::String("system".to_string())),
            );
            object.insert(
                "source_actor".to_string(),
                serde_json::Value::String(source.to_string()),
            );
            object.insert(
                "section_id".to_string(),
                section_id
                    .map(|value| serde_json::Value::String(value.to_string()))
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "ops".to_string(),
                serde_json::to_value(&ops).unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "proposal".to_string(),
                proposal
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "base_version_id".to_string(),
                base_version_id
                    .map(serde_json::Value::from)
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "target_version_id".to_string(),
                target_version_id
                    .map(serde_json::Value::from)
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "overlay_id".to_string(),
                overlay_id
                    .map(|value| serde_json::Value::String(value.to_string()))
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        self.emit_event("writer.run.patch", payload).await;
    }

    async fn emit_progress_event(&self, phase: impl Into<String>, message: impl Into<String>) {
        let mut payload = self.base_event_payload();
        if let Some(object) = payload.as_object_mut() {
            object.insert("phase".to_string(), serde_json::Value::String(phase.into()));
            object.insert(
                "message".to_string(),
                serde_json::Value::String(message.into()),
            );
        }
        self.emit_event("writer.run.progress", payload).await;
    }

    async fn emit_status_event(
        &self,
        status: shared_types::WriterRunStatusKind,
        message: Option<String>,
    ) {
        let mut payload = self.base_event_payload();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "status".to_string(),
                serde_json::to_value(status)
                    .unwrap_or_else(|_| serde_json::Value::String("running".to_string())),
            );
            object.insert(
                "message".to_string(),
                message
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        self.emit_event("writer.run.status", payload).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{PatchOp, PatchOpKind, WriterDocumentRuntime};

    #[test]
    fn extract_revision_from_content_parses_marker() {
        let content = "<!-- revision:42 -->\n# Test\nBody";
        assert_eq!(
            WriterDocumentRuntime::extract_revision_from_content(content),
            42
        );
        assert_eq!(
            WriterDocumentRuntime::extract_revision_from_content("# Test"),
            0
        );
    }

    #[test]
    fn apply_legacy_line_patch_ops_append() {
        let ops = vec![PatchOp {
            kind: PatchOpKind::Append,
            position: None,
            text: Some("world".to_string()),
        }];

        let (content, lines_modified) =
            WriterDocumentRuntime::apply_legacy_line_patch_ops("hello", &ops);
        assert_eq!(content, "hello\nworld");
        assert_eq!(lines_modified, 1);
    }
}
