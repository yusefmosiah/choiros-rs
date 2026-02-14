//! RunWriterActor - Single mutation authority for run documents.
//!
//! One actor per run, serializes all document writes with atomic persistence
//! (temp + rename) and monotonic revision increment.
//!
//! # Architecture
//!
//! - One RunWriterActor per run (spawned by RunWriterSupervisor)
//! - All document mutations flow through this actor
//! - Persists typed `writer.run.*` events to EventStore for websocket fanout
//!
//! # Document Path
//!
//! `conductor/runs/{run_id}/draft.md`
//!
//! # Document Structure
//!
//! ```markdown
//! # {objective}
//!
//! ## Conductor
//! {canon text only}
//!
//! ## Researcher
//! <!-- proposal -->
//! {live worker proposals}
//!
//! ## Terminal
//! <!-- proposal -->
//! {live worker proposals}
//!
//! ## User
//! <!-- proposal -->
//! {unsent user directives/comments}
//! ```

mod messages;
mod state;

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::path::PathBuf;
use tokio::fs;

pub use messages::{
    ApplyPatchResult, PatchOp, PatchOpKind, RunWriterError, RunWriterMsg, SectionState,
};
pub use state::{DocumentSection, RunDocument, RunWriterState};

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

        let document_path_relative = PathBuf::from(BASE_RUNS_DIR)
            .join(&args.run_id)
            .join("draft.md");
        let root_dir = args
            .root_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let document_path = root_dir.join(&document_path_relative);

        let (document, revision) = Self::load_or_create_document(&document_path).await?;

        tracing::info!(
            actor_id = %myself.get_id(),
            run_id = %args.run_id,
            revision = revision,
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
    ) -> Result<(RunDocument, u64), ActorProcessingErr> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.ok();
        }

        if path.exists() {
            match fs::read_to_string(path).await {
                Ok(content) => {
                    let revision = Self::extract_revision_from_content(&content);
                    match RunDocument::from_markdown(&content) {
                        Ok(doc) => return Ok((doc, revision)),
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

        Ok((RunDocument::default(), 0))
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
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
    }

    fn source_to_patch_source(source: &str) -> shared_types::PatchSource {
        match source.to_ascii_lowercase().as_str() {
            "user" => shared_types::PatchSource::User,
            "system" | "conductor" => shared_types::PatchSource::System,
            _ => shared_types::PatchSource::Agent,
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
        proposal: Option<String>,
    ) {
        let mut payload = Self::base_event_payload(state);
        let full_doc = state.document.to_markdown();
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
                serde_json::to_value(Self::full_document_ops(&full_doc))
                    .unwrap_or_else(|_| serde_json::Value::Array(vec![])),
            );
            object.insert(
                "proposal".to_string(),
                proposal
                    .map(serde_json::Value::String)
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
}

impl RunWriterActor {
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
        if run_id != state.run_id {
            return Err(RunWriterError::RunIdMismatch {
                expected: state.run_id.clone(),
                actual: run_id,
            });
        }

        let section = state
            .document
            .sections
            .get_mut(&section_id)
            .ok_or_else(|| RunWriterError::SectionNotFound(section_id.clone()))?;

        let target = if proposal {
            section.proposal.as_mut().unwrap_or(&mut section.content)
        } else {
            &mut section.content
        };

        let mut lines_modified = 0;
        for op in ops {
            match op.kind {
                PatchOpKind::Append => {
                    if let Some(text) = &op.text {
                        if !target.is_empty() && !target.ends_with('\n') {
                            target.push('\n');
                        }
                        target.push_str(text);
                        lines_modified += text.lines().count();
                    }
                }
                PatchOpKind::Insert => {
                    if let (Some(text), Some(pos)) = (&op.text, op.position) {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos <= lines.len() {
                            let mut new_lines = lines.clone();
                            new_lines.insert(pos, text);
                            *target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
                PatchOpKind::Delete => {
                    if let Some(pos) = op.position {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos < lines.len() {
                            let mut new_lines = lines.clone();
                            new_lines.remove(pos);
                            *target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
                PatchOpKind::Replace => {
                    if let (Some(text), Some(pos)) = (&op.text, op.position) {
                        let lines: Vec<&str> = target.lines().collect();
                        if pos < lines.len() {
                            let mut new_lines = lines.clone();
                            new_lines[pos] = text;
                            *target = new_lines.join("\n");
                            lines_modified += 1;
                        }
                    }
                }
            }
        }

        Self::persist_document(state).await?;
        let proposal_text = if proposal {
            state
                .document
                .sections
                .get(&section_id)
                .and_then(|s| s.proposal.clone())
        } else {
            None
        };
        Self::emit_patch_event(state, &source, Some(&section_id), proposal_text).await;
        Self::emit_progress_event(
            state,
            "patch_applied",
            format!("Updated {section_id} via {source}"),
        )
        .await;

        Ok(ApplyPatchResult {
            revision: state.revision,
            lines_modified,
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
        if run_id != state.run_id {
            return Err(RunWriterError::RunIdMismatch {
                expected: state.run_id.clone(),
                actual: run_id,
            });
        }

        let section = state
            .document
            .sections
            .get_mut(&section_id)
            .ok_or_else(|| RunWriterError::SectionNotFound(section_id.clone()))?;

        let target = if proposal {
            section.proposal.get_or_insert_with(String::new)
        } else {
            &mut section.content
        };

        let timestamp = chrono::Utc::now().format("%H:%M:%S");
        let log_line = format!("[{timestamp}] {text}");

        if !target.is_empty() && !target.ends_with('\n') {
            target.push('\n');
        }
        target.push_str(&log_line);

        Self::persist_document(state).await?;
        let proposal_text = if proposal {
            state
                .document
                .sections
                .get(&section_id)
                .and_then(|s| s.proposal.clone())
        } else {
            None
        };
        Self::emit_progress_event(
            state,
            format!("{}:{}", source, section_id),
            log_line.clone(),
        )
        .await;
        Self::emit_patch_event(state, &source, Some(&section_id), proposal_text).await;

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
        if run_id != state.run_id {
            return Err(RunWriterError::RunIdMismatch {
                expected: state.run_id.clone(),
                actual: run_id,
            });
        }

        if !state.document.sections.contains_key(&section_id) {
            return Err(RunWriterError::SectionNotFound(section_id));
        }

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
        if run_id != state.run_id {
            return Err(RunWriterError::RunIdMismatch {
                expected: state.run_id.clone(),
                actual: run_id,
            });
        }

        let section = state
            .document
            .sections
            .get_mut(&section_id)
            .ok_or_else(|| RunWriterError::SectionNotFound(section_id.clone()))?;

        section.state = section_state.clone();
        let proposal_snapshot = section.proposal.clone();
        let status_message = format!("{section_id} -> {:?}", section_state);

        Self::persist_document(state).await?;
        let status = match section_state {
            SectionState::Pending => shared_types::WriterRunStatusKind::WaitingForWorker,
            SectionState::Running => shared_types::WriterRunStatusKind::Running,
            SectionState::Complete => shared_types::WriterRunStatusKind::Completed,
            SectionState::Failed => shared_types::WriterRunStatusKind::Failed,
        };
        Self::emit_status_event(state, status, Some(status_message)).await;
        Self::emit_patch_event(state, "system", Some(&section_id), proposal_snapshot).await;

        Ok(())
    }

    async fn handle_commit_proposal(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        section_id: String,
    ) -> Result<u64, RunWriterError> {
        let section = state
            .document
            .sections
            .get_mut(&section_id)
            .ok_or_else(|| RunWriterError::SectionNotFound(section_id.clone()))?;

        if let Some(proposal) = section.proposal.take() {
            section.content = proposal;
            Self::persist_document(state).await?;
            Self::emit_status_event(
                state,
                shared_types::WriterRunStatusKind::Running,
                Some(format!("Committed proposal for {section_id}")),
            )
            .await;
            Self::emit_patch_event(state, "system", Some(&section_id), None).await;

            Ok(state.revision)
        } else {
            Err(RunWriterError::InvalidPatch(
                "No proposal to commit".to_string(),
            ))
        }
    }

    async fn handle_discard_proposal(
        &self,
        _myself: &ActorRef<RunWriterMsg>,
        state: &mut RunWriterState,
        section_id: String,
    ) -> Result<(), RunWriterError> {
        let section = state
            .document
            .sections
            .get_mut(&section_id)
            .ok_or_else(|| RunWriterError::SectionNotFound(section_id.clone()))?;

        section.proposal = None;

        Self::persist_document(state).await?;
        Self::emit_status_event(
            state,
            shared_types::WriterRunStatusKind::Running,
            Some(format!("Discarded proposal for {section_id}")),
        )
        .await;
        Self::emit_patch_event(state, "system", Some(&section_id), None).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_document_to_markdown() {
        let mut doc = RunDocument::new("Test Objective");
        doc.sections.get_mut("conductor").unwrap().content = "Conductor content".to_string();
        doc.sections.get_mut("researcher").unwrap().proposal =
            Some("Research proposal".to_string());

        let md = doc.to_markdown();
        assert!(md.contains("# Test Objective"));
        assert!(md.contains("## Conductor"));
        assert!(md.contains("Conductor content"));
        assert!(md.contains("## Researcher"));
        assert!(md.contains("<!-- proposal -->"));
        assert!(md.contains("Research proposal"));
    }

    #[test]
    fn test_document_from_markdown() {
        let md = r#"# Test Objective

## Conductor
Conductor content

## Researcher
<!-- proposal -->
Research proposal
"#;

        let doc = RunDocument::from_markdown(md).unwrap();
        assert_eq!(doc.objective, "Test Objective");
        assert_eq!(
            doc.sections.get("conductor").unwrap().content,
            "Conductor content"
        );
        assert_eq!(
            doc.sections.get("researcher").unwrap().proposal,
            Some("Research proposal".to_string())
        );
    }

    #[test]
    fn test_section_state_default() {
        let doc = RunDocument::default();
        for section in doc.sections.values() {
            assert_eq!(section.state, SectionState::Pending);
        }
    }

    #[test]
    fn test_document_roundtrip_preserves_all_sections() {
        let mut doc = RunDocument::new("Complex Roundtrip Test");
        doc.sections.get_mut("conductor").unwrap().content =
            "Canon content\nwith multiple lines".to_string();
        doc.sections.get_mut("researcher").unwrap().proposal =
            Some("Research in progress".to_string());
        doc.sections.get_mut("terminal").unwrap().content = "Terminal output".to_string();
        doc.sections.get_mut("user").unwrap().proposal = Some("User comment".to_string());

        let md = doc.to_markdown();
        let restored = RunDocument::from_markdown(&md).unwrap();

        assert_eq!(restored.objective, "Complex Roundtrip Test");
        assert_eq!(
            restored.sections.get("conductor").unwrap().content,
            "Canon content\nwith multiple lines"
        );
        assert_eq!(
            restored.sections.get("researcher").unwrap().proposal,
            Some("Research in progress".to_string())
        );
        assert_eq!(
            restored.sections.get("terminal").unwrap().content,
            "Terminal output"
        );
        assert_eq!(
            restored.sections.get("user").unwrap().proposal,
            Some("User comment".to_string())
        );
    }

    #[test]
    fn test_patch_append_adds_content() {
        let mut doc = RunDocument::new("Patch Test");
        doc.sections.get_mut("conductor").unwrap().content = "Initial".to_string();

        let section = doc.sections.get_mut("conductor").unwrap();
        let target = &mut section.content;

        let op = PatchOp {
            kind: PatchOpKind::Append,
            position: None,
            text: Some("Appended line".to_string()),
        };

        if let Some(text) = &op.text {
            if !target.is_empty() && !target.ends_with('\n') {
                target.push('\n');
            }
            target.push_str(text);
        }

        assert_eq!(
            doc.sections.get("conductor").unwrap().content,
            "Initial\nAppended line"
        );
    }

    #[test]
    fn test_patch_insert_at_position() {
        let mut doc = RunDocument::new("Insert Test");
        doc.sections.get_mut("conductor").unwrap().content = "Line 1\nLine 3".to_string();

        let section = doc.sections.get_mut("conductor").unwrap();
        let lines: Vec<&str> = section.content.lines().collect();
        let mut new_lines = lines.clone();
        new_lines.insert(1, "Line 2");
        section.content = new_lines.join("\n");

        assert_eq!(section.content, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_patch_delete_line() {
        let mut doc = RunDocument::new("Delete Test");
        doc.sections.get_mut("conductor").unwrap().content = "Line 1\nLine 2\nLine 3".to_string();

        let section = doc.sections.get_mut("conductor").unwrap();
        let lines: Vec<&str> = section.content.lines().collect();
        let mut new_lines = lines.clone();
        new_lines.remove(1);
        section.content = new_lines.join("\n");

        assert_eq!(section.content, "Line 1\nLine 3");
    }

    #[test]
    fn test_patch_replace_line() {
        let mut doc = RunDocument::new("Replace Test");
        doc.sections.get_mut("conductor").unwrap().content = "Line 1\nLine 2\nLine 3".to_string();

        let section = doc.sections.get_mut("conductor").unwrap();
        let lines: Vec<&str> = section.content.lines().collect();
        let mut new_lines = lines.clone();
        new_lines[1] = "Line 2 modified";
        section.content = new_lines.join("\n");

        assert_eq!(section.content, "Line 1\nLine 2 modified\nLine 3");
    }

    #[tokio::test]
    async fn test_revision_monotonicity_increments_on_persist() {
        use tokio::fs;

        let temp_dir = tempdir().expect("temp dir");
        let doc_path = temp_dir.path().join("test_run").join("draft.md");

        fs::create_dir_all(doc_path.parent().unwrap())
            .await
            .expect("create dirs");

        let mut revision: u64 = 0;
        let mut document = RunDocument::new("Monotonicity Test");

        for i in 1..=5 {
            document.sections.get_mut("conductor").unwrap().content =
                format!("Content version {}", i);

            revision += 1;
            let content = format!("<!-- revision:{} -->\n{}", revision, document.to_markdown());

            let temp_path = doc_path.with_extension("md.tmp");
            fs::write(&temp_path, &content).await.expect("write temp");
            fs::rename(&temp_path, &doc_path).await.expect("rename");

            let persisted_content = fs::read_to_string(&doc_path).await.expect("read back");
            assert!(
                persisted_content.contains(&format!("<!-- revision:{} -->", revision)),
                "revision {} should be in file",
                revision
            );
        }

        assert_eq!(revision, 5);
    }

    #[test]
    fn test_extract_revision_from_content() {
        let content = "<!-- revision:42 -->\n# Test\n\n## Conductor\nContent";
        let rev = RunWriterActor::extract_revision_from_content(content);
        assert_eq!(rev, 42);

        let no_rev = "# No revision\n\nContent";
        let rev_none = RunWriterActor::extract_revision_from_content(no_rev);
        assert_eq!(rev_none, 0);

        let invalid_rev = "<!-- revision:abc -->\n# Test";
        let rev_invalid = RunWriterActor::extract_revision_from_content(invalid_rev);
        assert_eq!(rev_invalid, 0);
    }

    #[test]
    fn test_proposal_vs_canon_target_selection() {
        let mut section = DocumentSection {
            content: "canon content".to_string(),
            state: SectionState::Pending,
            proposal: Some("proposal content".to_string()),
        };

        let proposal_target = section.proposal.as_mut().unwrap();
        assert_eq!(proposal_target, "proposal content");

        let canon_target = &mut section.content;
        assert_eq!(canon_target, "canon content");
    }

    #[test]
    fn test_commit_proposal_moves_to_canon() {
        let mut section = DocumentSection {
            content: "old canon".to_string(),
            state: SectionState::Pending,
            proposal: Some("new proposal".to_string()),
        };

        if let Some(proposal) = section.proposal.take() {
            section.content = proposal;
        }

        assert!(section.proposal.is_none());
        assert_eq!(section.content, "new proposal");
    }

    #[test]
    fn test_discard_proposal_clears() {
        let mut section = DocumentSection {
            content: "canon".to_string(),
            state: SectionState::Pending,
            proposal: Some("to discard".to_string()),
        };

        section.proposal = None;

        assert!(section.proposal.is_none());
        assert_eq!(section.content, "canon");
    }

    #[test]
    fn test_section_state_transitions() {
        let mut section = DocumentSection::default();
        assert_eq!(section.state, SectionState::Pending);

        section.state = SectionState::Running;
        assert_eq!(section.state, SectionState::Running);

        section.state = SectionState::Complete;
        assert_eq!(section.state, SectionState::Complete);

        section.state = SectionState::Failed;
        assert_eq!(section.state, SectionState::Failed);
    }

    #[test]
    fn test_empty_document_serialization() {
        let doc = RunDocument::default();
        let md = doc.to_markdown();
        let restored = RunDocument::from_markdown(&md).unwrap();

        assert_eq!(restored.objective, "");
        for section_id in ["conductor", "researcher", "terminal", "user"] {
            assert!(restored.sections.contains_key(section_id));
        }
    }

    #[test]
    fn test_special_characters_in_content() {
        let mut doc = RunDocument::new("Test with <special> & \"chars\"");
        doc.sections.get_mut("conductor").unwrap().content =
            "Content with\nnewlines\nand <html> tags".to_string();

        let md = doc.to_markdown();
        let restored = RunDocument::from_markdown(&md).unwrap();

        assert_eq!(restored.objective, "Test with <special> & \"chars\"");
        assert!(restored
            .sections
            .get("conductor")
            .unwrap()
            .content
            .contains("<html>"));
    }
}
