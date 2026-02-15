//! RunWriterActor - Single mutation authority for run documents.
//!
//! One actor per run, serializes all document writes with atomic persistence
//! (temp + rename) and monotonic revision increment.

mod messages;
mod state;

use async_trait::async_trait;
use chrono::Utc;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

pub use messages::{
    ApplyPatchResult, PatchOp, PatchOpKind, RunWriterError, RunWriterMsg, SectionState,
};
pub use state::{
    DocumentVersion, Overlay, OverlayAuthor, OverlayKind, OverlayStatus, RunDocument,
    RunWriterState, VersionSource,
};

use crate::actors::event_store::{AppendEvent, EventStoreMsg};

const BASE_RUNS_DIR: &str = "conductor/runs";

#[derive(Debug, Default)]
pub struct RunWriterActor;

#[derive(Debug, Clone)]
pub struct RunWriterArguments {
    pub run_id: String,
    pub desktop_id: String,
    pub objective: String,
    pub session_id: String,
    pub thread_id: String,
    pub root_dir: Option<String>,
    pub event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRunWriterSnapshot {
    revision: u64,
    document: RunDocument,
}

#[async_trait]
impl Actor for RunWriterActor {
    type Msg = RunWriterMsg;
    type State = RunWriterState;
    type Arguments = RunWriterArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            run_id = %args.run_id,
            "RunWriterActor starting"
        );

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
                .await?;
        if document.objective.trim().is_empty() {
            document.objective = args.objective.clone();
        }

        tracing::info!(
            actor_id = %myself.get_id(),
            run_id = %args.run_id,
            revision = revision,
            head_version_id = document.head_version_id,
            "RunWriterActor loaded document"
        );

        let mut state = RunWriterState {
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
        };

        Self::emit_started_event(&mut state).await;
        Ok(state)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            RunWriterMsg::ApplyPatch {
                run_id,
                source,
                section_id,
                ops,
                proposal,
                reply,
            } => {
                let result = self
                    .handle_apply_patch(&myself, state, run_id, source, section_id, ops, proposal)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::AppendLogLine {
                run_id,
                source,
                section_id,
                text,
                proposal,
                reply,
            } => {
                let result = self
                    .handle_append_log_line(
                        &myself, state, run_id, source, section_id, text, proposal,
                    )
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::ReportSectionProgress {
                run_id,
                source,
                section_id,
                phase,
                message,
                reply,
            } => {
                let result = self
                    .handle_report_section_progress(
                        &myself, state, run_id, source, section_id, phase, message,
                    )
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::MarkSectionState {
                run_id,
                section_id,
                state: section_state,
                reply,
            } => {
                let result = self
                    .handle_mark_section_state(&myself, state, run_id, section_id, section_state)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::GetDocument { reply } => {
                let _ = reply.send(Ok(state.document.to_markdown()));
            }
            RunWriterMsg::GetRevision { reply } => {
                let _ = reply.send(state.revision);
            }
            RunWriterMsg::GetHeadVersion { reply } => {
                let result =
                    state
                        .document
                        .head_version()
                        .cloned()
                        .ok_or(RunWriterError::VersionNotFound(
                            state.document.head_version_id,
                        ));
                let _ = reply.send(result);
            }
            RunWriterMsg::GetVersion { version_id, reply } => {
                let result = state
                    .document
                    .get_version(version_id)
                    .cloned()
                    .ok_or(RunWriterError::VersionNotFound(version_id));
                let _ = reply.send(result);
            }
            RunWriterMsg::ListVersions { reply } => {
                let mut versions = state.document.versions.clone();
                versions.sort_by_key(|version| version.version_id);
                let _ = reply.send(Ok(versions));
            }
            RunWriterMsg::ListOverlays {
                base_version_id,
                status,
                reply,
            } => {
                let mut overlays: Vec<Overlay> = state
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
                let _ = reply.send(Ok(overlays));
            }
            RunWriterMsg::CreateVersion {
                run_id,
                parent_version_id,
                content,
                source,
                reply,
            } => {
                let result = self
                    .handle_create_version(state, run_id, parent_version_id, content, source)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::CreateOverlay {
                run_id,
                base_version_id,
                author,
                kind,
                diff_ops,
                reply,
            } => {
                let result = self
                    .handle_create_overlay(state, run_id, base_version_id, author, kind, diff_ops)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::ResolveOverlay {
                run_id,
                overlay_id,
                status,
                reply,
            } => {
                let result = self
                    .handle_resolve_overlay(state, run_id, overlay_id, status)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::CommitProposal { section_id, reply } => {
                let result = self
                    .handle_commit_proposal(&myself, state, section_id)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::DiscardProposal { section_id, reply } => {
                let result = self
                    .handle_discard_proposal(&myself, state, section_id)
                    .await;
                let _ = reply.send(result);
            }
            RunWriterMsg::SetSectionContent {
                run_id,
                source,
                section_id,
                content,
                reply,
            } => {
                let result = self
                    .handle_set_section_content(&myself, state, run_id, source, section_id, content)
                    .await;
                let _ = reply.send(result);
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(actor_id = %myself.get_id(), "RunWriterActor stopped");
        Ok(())
    }
}

impl RunWriterActor {
    async fn load_or_create_document(
        path: &PathBuf,
        meta_path: &PathBuf,
        objective: &str,
    ) -> Result<(RunDocument, u64), ActorProcessingErr> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ActorProcessingErr::from(e.to_string()))?;
        }

        if meta_path.exists() {
            match fs::read_to_string(meta_path).await {
                Ok(raw) => match serde_json::from_str::<PersistedRunWriterSnapshot>(&raw) {
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

    async fn persist_document(state: &mut RunWriterState) -> Result<(), RunWriterError> {
        state.revision += 1;

        let content = format!(
            "<!-- revision:{} -->\n{}",
            state.revision,
            state.document.to_markdown()
        );

        let temp_path = state.document_path.with_extension("md.tmp");
        fs::write(&temp_path, &content)
            .await
            .map_err(|e| RunWriterError::WriteFailed(format!("Failed to write temp file: {e}")))?;
        fs::rename(&temp_path, &state.document_path)
            .await
            .map_err(|e| RunWriterError::WriteFailed(format!("Failed to rename temp file: {e}")))?;

        let sidecar = PersistedRunWriterSnapshot {
            revision: state.revision,
            document: state.document.clone(),
        };
        let sidecar_raw = serde_json::to_string_pretty(&sidecar)
            .map_err(|e| RunWriterError::WriteFailed(format!("Failed to encode sidecar: {e}")))?;
        let temp_meta = state.document_meta_path.with_extension("json.tmp");
        fs::write(&temp_meta, sidecar_raw)
            .await
            .map_err(|e| RunWriterError::WriteFailed(format!("Failed to write sidecar: {e}")))?;
        fs::rename(&temp_meta, &state.document_meta_path)
            .await
            .map_err(|e| RunWriterError::WriteFailed(format!("Failed to rename sidecar: {e}")))?;

        Ok(())
    }

    fn base_event_payload(state: &RunWriterState) -> serde_json::Value {
        serde_json::json!({
            "desktop_id": state.desktop_id,
            "session_id": state.session_id,
            "thread_id": state.thread_id,
            "run_id": state.run_id,
            "document_path": state.document_path_relative,
            "revision": state.revision,
            "head_version_id": state.document.head_version_id,
            "timestamp": Utc::now().to_rfc3339(),
        })
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

    fn apply_shared_patch_ops(
        content: &str,
        ops: &[shared_types::PatchOp],
    ) -> Result<String, RunWriterError> {
        let mut chars: Vec<char> = content.chars().collect();
        for op in ops {
            match op {
                shared_types::PatchOp::Insert { pos, text } => {
                    let pos = (*pos as usize).min(chars.len());
                    let insert_chars: Vec<char> = text.chars().collect();
                    chars.splice(pos..pos, insert_chars);
                }
                shared_types::PatchOp::Delete { pos, len } => {
                    let pos = (*pos as usize).min(chars.len());
                    let end = pos.saturating_add(*len as usize).min(chars.len());
                    if end < pos {
                        return Err(RunWriterError::InvalidPatch(
                            "delete range underflow".to_string(),
                        ));
                    }
                    chars.drain(pos..end);
                }
                shared_types::PatchOp::Replace { pos, len, text } => {
                    let pos = (*pos as usize).min(chars.len());
                    let end = pos.saturating_add(*len as usize).min(chars.len());
                    let replace_chars: Vec<char> = text.chars().collect();
                    chars.splice(pos..end, replace_chars);
                }
                shared_types::PatchOp::Retain { .. } => {}
            }
        }
        Ok(chars.into_iter().collect())
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

    async fn emit_event(state: &RunWriterState, event_type: &str, payload: serde_json::Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: format!("run_writer:{}", state.run_id),
            user_id: "system".to_string(),
        };
        if let Err(err) = state
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event })
        {
            tracing::warn!(
                run_id = %state.run_id,
                event_type = event_type,
                error = %err,
                "Failed to append writer event"
            );
        }
    }

    async fn emit_started_event(state: &mut RunWriterState) {
        let mut payload = Self::base_event_payload(state);
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "objective".to_string(),
                serde_json::Value::String(state.objective.clone()),
            );
        }
        Self::emit_event(state, "writer.run.started", payload).await;
    }

    async fn emit_patch_event(
        state: &RunWriterState,
        source: &str,
        section_id: Option<&str>,
        ops: Vec<shared_types::PatchOp>,
        proposal: Option<String>,
        base_version_id: Option<u64>,
        target_version_id: Option<u64>,
        overlay_id: Option<&str>,
    ) {
        let mut payload = Self::base_event_payload(state);
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
                "section_id".to_string(),
                section_id
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert(
                "ops".to_string(),
                serde_json::to_value(ops).unwrap_or_else(|_| serde_json::Value::Array(vec![])),
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
                    .map(|id| serde_json::Value::String(id.to_string()))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        Self::emit_event(state, "writer.run.patch", payload).await;
    }

    async fn emit_progress_event(
        state: &RunWriterState,
        phase: impl Into<String>,
        message: impl Into<String>,
    ) {
        let mut payload = Self::base_event_payload(state);
        if let Some(object) = payload.as_object_mut() {
            object.insert("phase".to_string(), serde_json::Value::String(phase.into()));
            object.insert(
                "message".to_string(),
                serde_json::Value::String(message.into()),
            );
            object.insert("progress_pct".to_string(), serde_json::Value::Null);
        }
        Self::emit_event(state, "writer.run.progress", payload).await;
    }

    async fn emit_status_event(
        state: &RunWriterState,
        status: shared_types::WriterRunStatusKind,
        message: Option<String>,
    ) {
        let mut payload = Self::base_event_payload(state);
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
        Self::emit_event(state, "writer.run.status", payload).await;
    }

    fn ensure_run_id(state: &RunWriterState, run_id: &str) -> Result<(), RunWriterError> {
        if run_id != state.run_id {
            return Err(RunWriterError::RunIdMismatch {
                expected: state.run_id.clone(),
                actual: run_id.to_string(),
            });
        }
        Ok(())
    }

    async fn create_version_internal(
        state: &mut RunWriterState,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
        event_source: &str,
        section_id: Option<&str>,
    ) -> Result<DocumentVersion, RunWriterError> {
        let parent = parent_version_id.unwrap_or(state.document.head_version_id);
        if state.document.get_version(parent).is_none() {
            return Err(RunWriterError::VersionNotFound(parent));
        }

        let version = DocumentVersion {
            version_id: state.document.next_version_id(),
            created_at: Utc::now(),
            source,
            content,
            parent_version_id: Some(parent),
        };
        state.document.versions.push(version.clone());
        state.document.head_version_id = version.version_id;

        for overlay in state.document.overlays.iter_mut() {
            if overlay.base_version_id == parent && overlay.status == OverlayStatus::Pending {
                overlay.status = OverlayStatus::Superseded;
            }
        }

        Self::persist_document(state).await?;

        let full_doc = state.document.to_markdown();
        Self::emit_patch_event(
            state,
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
        state: &mut RunWriterState,
        base_version_id: u64,
        author: OverlayAuthor,
        kind: OverlayKind,
        diff_ops: Vec<shared_types::PatchOp>,
        event_source: &str,
        section_id: Option<&str>,
        proposal: Option<String>,
    ) -> Result<Overlay, RunWriterError> {
        if state.document.get_version(base_version_id).is_none() {
            return Err(RunWriterError::VersionNotFound(base_version_id));
        }
        if diff_ops.is_empty() {
            return Err(RunWriterError::InvalidPatch(
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
        state.document.overlays.push(overlay.clone());
        Self::persist_document(state).await?;

        Self::emit_patch_event(
            state,
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

    async fn resolve_overlay_internal(
        state: &mut RunWriterState,
        overlay_id: String,
        status: OverlayStatus,
    ) -> Result<Overlay, RunWriterError> {
        let overlay = state
            .document
            .get_overlay_mut(&overlay_id)
            .ok_or_else(|| RunWriterError::OverlayNotFound(overlay_id.clone()))?;
        overlay.status = status;
        let updated = overlay.clone();
        Self::persist_document(state).await?;
        Ok(updated)
    }
}

impl RunWriterActor {
    async fn handle_create_version(
        &self,
        state: &mut RunWriterState,
        run_id: String,
        parent_version_id: Option<u64>,
        content: String,
        source: VersionSource,
    ) -> Result<DocumentVersion, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;
        let version = Self::create_version_internal(
            state,
            parent_version_id,
            content,
            source,
            "writer",
            None,
        )
        .await?;
        Self::emit_progress_event(
            state,
            "version_created",
            format!("Created version {}", version.version_id),
        )
        .await;
        Ok(version)
    }

    async fn handle_create_overlay(
        &self,
        state: &mut RunWriterState,
        run_id: String,
        base_version_id: u64,
        author: OverlayAuthor,
        kind: OverlayKind,
        diff_ops: Vec<shared_types::PatchOp>,
    ) -> Result<Overlay, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;
        let overlay = Self::create_overlay_internal(
            state,
            base_version_id,
            author,
            kind,
            diff_ops,
            "writer",
            None,
            None,
        )
        .await?;
        Self::emit_progress_event(
            state,
            "overlay_created",
            format!("Created overlay {}", overlay.overlay_id),
        )
        .await;
        Ok(overlay)
    }

    async fn handle_resolve_overlay(
        &self,
        state: &mut RunWriterState,
        run_id: String,
        overlay_id: String,
        status: OverlayStatus,
    ) -> Result<Overlay, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;
        let overlay =
            Self::resolve_overlay_internal(state, overlay_id.clone(), status.clone()).await?;
        Self::emit_progress_event(
            state,
            "overlay_resolved",
            format!("Overlay {overlay_id} -> {:?}", status),
        )
        .await;
        Ok(overlay)
    }

    async fn handle_apply_patch(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        run_id: String,
        source: String,
        section_id: String,
        ops: Vec<PatchOp>,
        proposal: bool,
    ) -> Result<ApplyPatchResult, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;

        let base_version_id = state.document.head_version_id;
        let base_content = state
            .document
            .head_version()
            .map(|version| version.content.clone())
            .unwrap_or_default();
        let (next_content, lines_modified) = Self::apply_legacy_line_patch_ops(&base_content, &ops);

        if proposal {
            let diff_ops = Self::diff_full_replace(&base_content, &next_content);
            let overlay = Self::create_overlay_internal(
                state,
                base_version_id,
                Self::source_to_overlay_author(&source),
                OverlayKind::Proposal,
                diff_ops,
                &source,
                Some(&section_id),
                Some(next_content),
            )
            .await?;
            Self::emit_progress_event(
                state,
                "patch_applied",
                format!("Created proposal overlay for {section_id} via {source}"),
            )
            .await;
            return Ok(ApplyPatchResult {
                revision: state.revision,
                lines_modified,
                base_version_id,
                target_version_id: None,
                overlay_id: Some(overlay.overlay_id),
            });
        }

        let version = Self::create_version_internal(
            state,
            Some(base_version_id),
            next_content,
            Self::source_to_version_source(&source),
            &source,
            Some(&section_id),
        )
        .await?;
        Self::emit_progress_event(
            state,
            "patch_applied",
            format!("Updated canonical version via {source}"),
        )
        .await;

        Ok(ApplyPatchResult {
            revision: state.revision,
            lines_modified,
            base_version_id,
            target_version_id: Some(version.version_id),
            overlay_id: None,
        })
    }

    async fn handle_append_log_line(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        run_id: String,
        source: String,
        section_id: String,
        text: String,
        proposal: bool,
    ) -> Result<u64, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;

        let base_version_id = state.document.head_version_id;
        let base_content = state
            .document
            .head_version()
            .map(|version| version.content.clone())
            .unwrap_or_default();
        let timestamp = Utc::now().format("%H:%M:%S");
        let log_line = format!("[{timestamp}] {text}");

        let mut next_content = base_content.clone();
        if !next_content.is_empty() && !next_content.ends_with('\n') {
            next_content.push('\n');
        }
        next_content.push_str(&log_line);

        if proposal {
            let diff_ops = Self::diff_full_replace(&base_content, &next_content);
            let _ = Self::create_overlay_internal(
                state,
                base_version_id,
                Self::source_to_overlay_author(&source),
                OverlayKind::Comment,
                diff_ops,
                &source,
                Some(&section_id),
                Some(log_line.clone()),
            )
            .await?;
        } else {
            let _ = Self::create_version_internal(
                state,
                Some(base_version_id),
                next_content,
                Self::source_to_version_source(&source),
                &source,
                Some(&section_id),
            )
            .await?;
        }

        Self::emit_progress_event(state, format!("{}:{}", source, section_id), log_line).await;
        Ok(state.revision)
    }

    async fn handle_report_section_progress(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        run_id: String,
        source: String,
        section_id: String,
        phase: String,
        message: String,
    ) -> Result<u64, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;
        Self::emit_progress_event(state, format!("{source}:{section_id}:{phase}"), message).await;
        Ok(state.revision)
    }

    async fn handle_mark_section_state(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        run_id: String,
        section_id: String,
        section_state: SectionState,
    ) -> Result<(), RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;

        let status_message = format!("{section_id} -> {:?}", section_state);
        let status = match section_state {
            SectionState::Pending => shared_types::WriterRunStatusKind::WaitingForWorker,
            SectionState::Running => shared_types::WriterRunStatusKind::Running,
            SectionState::Complete => shared_types::WriterRunStatusKind::Completed,
            SectionState::Failed => shared_types::WriterRunStatusKind::Failed,
        };
        Self::persist_document(state).await?;
        Self::emit_status_event(state, status, Some(status_message)).await;

        Ok(())
    }

    async fn handle_commit_proposal(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        _section_id: String,
    ) -> Result<u64, RunWriterError> {
        let pending = state
            .document
            .overlays
            .iter()
            .rev()
            .find(|overlay| overlay.status == OverlayStatus::Pending)
            .cloned()
            .ok_or_else(|| RunWriterError::InvalidPatch("No proposal to commit".to_string()))?;

        let base = state
            .document
            .get_version(pending.base_version_id)
            .cloned()
            .ok_or(RunWriterError::VersionNotFound(pending.base_version_id))?;

        let content = Self::apply_shared_patch_ops(&base.content, &pending.diff_ops)?;

        if let Some(overlay) = state.document.get_overlay_mut(&pending.overlay_id) {
            overlay.status = OverlayStatus::Applied;
        }

        let _ = Self::create_version_internal(
            state,
            Some(state.document.head_version_id),
            content,
            VersionSource::System,
            "system",
            None,
        )
        .await?;

        Self::emit_status_event(
            state,
            shared_types::WriterRunStatusKind::Running,
            Some(format!("Committed overlay {}", pending.overlay_id)),
        )
        .await;

        Ok(state.revision)
    }

    async fn handle_discard_proposal(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        _section_id: String,
    ) -> Result<(), RunWriterError> {
        let overlay = state
            .document
            .overlays
            .iter_mut()
            .rev()
            .find(|overlay| overlay.status == OverlayStatus::Pending)
            .ok_or_else(|| RunWriterError::InvalidPatch("No proposal to discard".to_string()))?;

        let discarded_id = overlay.overlay_id.clone();
        overlay.status = OverlayStatus::Discarded;

        Self::persist_document(state).await?;
        Self::emit_status_event(
            state,
            shared_types::WriterRunStatusKind::Running,
            Some(format!("Discarded overlay {discarded_id}")),
        )
        .await;

        Ok(())
    }

    async fn handle_set_section_content(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        run_id: String,
        source: String,
        section_id: String,
        content: String,
    ) -> Result<u64, RunWriterError> {
        Self::ensure_run_id(state, &run_id)?;

        let parent = state.document.head_version_id;
        let _ = Self::create_version_internal(
            state,
            Some(parent),
            content,
            Self::source_to_version_source(&source),
            &source,
            Some(&section_id),
        )
        .await?;

        Self::emit_progress_event(
            state,
            "section_rewritten",
            format!("Rewrote canonical content for {section_id}"),
        )
        .await;

        Ok(state.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_revision_from_content() {
        let content = "<!-- revision:42 -->\n# Test\n\nBody";
        assert_eq!(RunWriterActor::extract_revision_from_content(content), 42);
        assert_eq!(RunWriterActor::extract_revision_from_content("# Test"), 0);
    }

    #[test]
    fn test_run_document_default_head_version() {
        let doc = RunDocument::new("Objective");
        assert_eq!(doc.head_version_id, 0);
        assert_eq!(doc.versions.len(), 1);
        assert_eq!(doc.head_version().map(|v| v.version_id), Some(0));
    }

    #[test]
    fn test_legacy_markdown_migration_creates_overlay() {
        let md = "# Obj\n\nCanon\n\n<!-- proposal -->\nMaybe better\n<!-- /proposal -->\n";
        let doc = RunDocument::from_legacy_markdown(md).expect("legacy parse");
        assert_eq!(doc.head_version_id, 1);
        assert_eq!(doc.versions.len(), 1);
        assert_eq!(doc.overlays.len(), 1);
        assert_eq!(doc.overlays[0].status, OverlayStatus::Pending);
    }

    #[test]
    fn test_apply_shared_patch_ops_insert_delete_replace() {
        let base = "abcd";
        let ops = vec![
            shared_types::PatchOp::Delete { pos: 1, len: 2 },
            shared_types::PatchOp::Insert {
                pos: 1,
                text: "XY".to_string(),
            },
            shared_types::PatchOp::Replace {
                pos: 0,
                len: 1,
                text: "Q".to_string(),
            },
        ];
        let out = RunWriterActor::apply_shared_patch_ops(base, &ops).expect("patch ops");
        assert_eq!(out, "QXYd");
    }

    #[test]
    fn test_diff_full_replace_includes_delete_and_insert() {
        let ops = RunWriterActor::diff_full_replace("hello", "bye");
        assert_eq!(ops.len(), 2);
        match &ops[0] {
            shared_types::PatchOp::Delete { pos, len } => {
                assert_eq!(*pos, 0);
                assert_eq!(*len, 5);
            }
            _ => panic!("expected delete"),
        }
        match &ops[1] {
            shared_types::PatchOp::Insert { pos, text } => {
                assert_eq!(*pos, 0);
                assert_eq!(text, "bye");
            }
            _ => panic!("expected insert"),
        }
    }
}
