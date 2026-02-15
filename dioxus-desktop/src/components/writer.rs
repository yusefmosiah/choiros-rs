//! Writer App Component
//!
//! Document editor with revision-based optimistic concurrency control.
//! Supports both edit and preview modes for markdown files.
//! Supports live patch apply from writer.run.* websocket events.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::files_api::{list_directory, DirectoryEntry};
use crate::api::{
    writer_open, writer_preview, writer_prompt, writer_save, writer_save_version, writer_version,
    writer_versions, WriterOverlay,
};
use crate::desktop::state::ACTIVE_WRITER_RUNS;
use shared_types::{PatchOp, WriterRunStatusKind};

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

/// View mode for markdown files
#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    Edit,
    Preview,
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

fn build_diff_ops(base: &str, edited: &str) -> Vec<PatchOp> {
    if base == edited {
        return Vec::new();
    }
    vec![
        PatchOp::Delete {
            pos: 0,
            len: base.chars().count() as u64,
        },
        PatchOp::Insert {
            pos: 0,
            text: edited.to_string(),
        },
    ]
}

fn extract_run_id_from_document_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("conductor"), Some("runs"), Some(run_id), Some("draft.md"), None) => {
            Some(run_id.to_string())
        }
        _ => None,
    }
}

fn overlay_note_lines(overlay: &WriterOverlay) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("> [{}/{}]", overlay.author, overlay.kind));
    for op in &overlay.diff_ops {
        match op {
            PatchOp::Insert { text, .. } => {
                if !text.trim().is_empty() {
                    lines.push(format!("> + {}", text.trim()));
                }
            }
            PatchOp::Delete { len, .. } => {
                lines.push(format!("> - remove {} chars", len));
            }
            PatchOp::Replace { len, text, .. } => {
                if text.trim().is_empty() {
                    lines.push(format!("> ~ rewrite {} chars", len));
                } else {
                    lines.push(format!("> ~ rewrite {} chars -> {}", len, text.trim()));
                }
            }
            PatchOp::Retain { .. } => {}
        }
    }
    lines
}

const INLINE_OVERLAY_MARKER: &str = "\n\n--- pending suggestions ---\n";

fn compose_editor_text(base: &str, overlays: &[WriterOverlay]) -> String {
    if overlays.is_empty() {
        return base.to_string();
    }
    let mut out = String::from(base);
    out.push_str(INLINE_OVERLAY_MARKER);
    for overlay in overlays {
        for line in overlay_note_lines(overlay) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn strip_inline_overlay_block(text: &str) -> String {
    if let Some(index) = text.find(INLINE_OVERLAY_MARKER) {
        return text[..index].to_string();
    }
    text.to_string()
}

fn normalize_version_ids(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn selected_version_index(ids: &[u64], selected_version_id: Option<u64>) -> Option<usize> {
    if ids.is_empty() {
        return None;
    }
    selected_version_id
        .and_then(|id| ids.iter().position(|v| *v == id))
        .or(Some(ids.len() - 1))
}

fn reconcile_selected_version_id(ids: &[u64], selected_version_id: Option<u64>) -> Option<u64> {
    selected_version_index(ids, selected_version_id).and_then(|idx| ids.get(idx).copied())
}

/// Apply patch operations to content
fn apply_patch_ops(content: &str, ops: &[PatchOp]) -> String {
    let mut chars: Vec<char> = content.chars().collect();

    for op in ops {
        match op {
            PatchOp::Insert { pos, text } => {
                let pos = (*pos as usize).min(chars.len());
                let insert_chars: Vec<char> = text.chars().collect();
                chars.splice(pos..pos, insert_chars);
            }
            PatchOp::Delete { pos, len } => {
                let pos = (*pos as usize).min(chars.len());
                let end = (pos + *len as usize).min(chars.len());
                chars.drain(pos..end);
            }
            PatchOp::Replace { pos, len, text } => {
                let pos = (*pos as usize).min(chars.len());
                let end = (pos + *len as usize).min(chars.len());
                let replace_chars: Vec<char> = text.chars().collect();
                chars.splice(pos..end, replace_chars);
            }
            PatchOp::Retain { .. } => {}
        }
    }

    chars.into_iter().collect()
}

/// Check if there's a revision gap (missed patches)
fn has_revision_gap(current_revision: u64, patch_revision: u64) -> bool {
    patch_revision > current_revision + 1
}

/// Writer component props
#[derive(Debug, Clone, PartialEq)]
pub struct WriterProps {
    pub desktop_id: String,
    pub window_id: String,
    pub initial_path: String,
}

/// Check if file is markdown based on mime type or extension
fn is_markdown(mime: &str, path: &str) -> bool {
    mime == "text/markdown" || path.ends_with(".md") || path.ends_with(".markdown")
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

#[component]
pub fn WriterView(desktop_id: String, window_id: String, initial_path: String) -> Element {
    let _ = (&desktop_id, &window_id);

    // Core state
    let mut content = use_signal(|| String::new());
    let mut revision = use_signal(|| 0u64);
    let mut mime = use_signal(|| "text/plain".to_string());
    let mut path = use_signal(|| initial_path.clone());
    let mut readonly = use_signal(|| false);
    let mut save_state = use_signal(|| SaveState::Clean);
    let mut view_mode = use_signal(|| ViewMode::Edit);

    // UI state
    let mut loading = use_signal(|| !initial_path.is_empty());
    let mut preview_html = use_signal(|| String::new());
    let mut dialog = use_signal(|| DialogState::None);
    let mut loaded_path = use_signal(|| None::<String>);
    let mut prompt_submitting = use_signal(|| false);
    let mut prompt_base_content = use_signal(String::new);
    let mut version_ids = use_signal(Vec::<u64>::new);
    let mut selected_version_id = use_signal(|| None::<u64>);
    let mut selected_version_content = use_signal(String::new);
    let mut selected_overlays = use_signal(|| Vec::<WriterOverlay>::new());
    let mut typing_locked = use_signal(|| false);
    let mut new_version_available = use_signal(|| false);

    // Live patch state
    let active_run_id = use_signal(|| None::<String>);
    let _pending_patches = use_signal(|| Vec::<(u64, Vec<PatchOp>)>::new());
    let mut last_applied_revision = use_signal(|| 0u64);
    let run_status = use_signal(|| None::<WriterRunStatusKind>);

    // Load document on mount
    use_effect(move || {
        let path_str = path();
        if path_str.is_empty() {
            loading.set(false);
            loaded_path.set(None);
            return;
        }

        if loaded_path().as_deref() == Some(path_str.as_str()) {
            return;
        }
        loaded_path.set(Some(path_str.clone()));

        spawn(async move {
            loading.set(true);
            match writer_open(&path_str).await {
                Ok(response) => {
                    let is_md = is_markdown(&response.mime, &response.path);
                    let opened_path = response.path.clone();
                    let opened_content = response.content.clone();

                    path.set(opened_path.clone());
                    content.set(opened_content.clone());
                    prompt_base_content.set(opened_content.clone());
                    selected_version_content.set(opened_content);
                    revision.set(response.revision);
                    last_applied_revision.set(response.revision);
                    mime.set(response.mime);
                    readonly.set(response.readonly);
                    save_state.set(SaveState::Clean);
                    view_mode.set(ViewMode::Edit);
                    typing_locked.set(false);
                    new_version_available.set(false);
                    loading.set(false);

                    if extract_run_id_from_document_path(&opened_path).is_some() {
                        match writer_versions(&opened_path).await {
                            Ok(versions_response) => {
                                let ids = normalize_version_ids(
                                    versions_response
                                        .versions
                                        .iter()
                                        .map(|version| version.version_id)
                                        .collect(),
                                );
                                let selected = reconcile_selected_version_id(
                                    &ids,
                                    Some(versions_response.head_version_id),
                                );
                                version_ids.set(ids);
                                selected_version_id.set(selected);

                                match writer_version(
                                    &opened_path,
                                    versions_response.head_version_id,
                                )
                                .await
                                {
                                    Ok(version_response) => {
                                        content.set(version_response.version.content.clone());
                                        prompt_base_content
                                            .set(version_response.version.content.clone());
                                        selected_version_content
                                            .set(version_response.version.content.clone());
                                        selected_overlays.set(version_response.overlays);
                                    }
                                    Err(_) => {
                                        selected_overlays.set(Vec::new());
                                    }
                                }
                            }
                            Err(_) => {
                                version_ids.set(Vec::new());
                                selected_version_id.set(None);
                                selected_overlays.set(Vec::new());
                            }
                        }
                    } else {
                        version_ids.set(Vec::new());
                        selected_version_id.set(None);
                        selected_overlays.set(Vec::new());
                    }

                    // Preload preview for markdown
                    if is_md {
                        spawn(async move {
                            let _ = update_preview(content(), &mime(), &path(), &mut preview_html)
                                .await;
                        });
                    }
                }
                Err(e) => {
                    save_state.set(SaveState::Error(e));
                    loaded_path.set(None);
                    loading.set(false);
                }
            }
        });
    });

    // Watch for writer run events and apply patches
    {
        let path_for_effect = path;
        let mut content_for_effect = content;
        let mut revision_for_effect = revision;
        let mut last_applied_revision_for_effect = last_applied_revision;
        let mut active_run_id_for_effect = active_run_id;
        let mut run_status_for_effect = run_status;
        let mut save_state_for_effect = save_state;
        let typing_locked_for_effect = typing_locked;
        let mut new_version_available_for_effect = new_version_available;
        let mut version_ids_for_effect = version_ids;
        let mut selected_version_id_for_effect = selected_version_id;
        let mut selected_version_content_for_effect = selected_version_content;
        let mut selected_overlays_for_effect = selected_overlays;

        use_effect(move || {
            let current_path = path_for_effect();
            let runs = ACTIVE_WRITER_RUNS.read();

            if let Some(run_state) = runs.get(&current_path) {
                let run_id = run_state.run_id.clone();
                let new_revision = run_state.revision;
                let status = run_state.status;
                let pending_patches = run_state.pending_patches.clone();
                let run_last_applied = run_state.last_applied_revision;

                drop(runs);

                // Check for new run starting
                if active_run_id_for_effect.read().as_ref() != Some(&run_id) {
                    active_run_id_for_effect.set(Some(run_id.clone()));
                    last_applied_revision_for_effect.set(run_last_applied);
                }

                run_status_for_effect.set(Some(status));
                if matches!(
                    status,
                    WriterRunStatusKind::Completed
                        | WriterRunStatusKind::Failed
                        | WriterRunStatusKind::Blocked
                ) {
                    selected_overlays_for_effect.set(Vec::new());
                }

                // Check for revision gap - need to fetch latest document
                let current_last_rev = last_applied_revision_for_effect();
                if has_revision_gap(current_last_rev, new_revision) && pending_patches.is_empty() {
                    dioxus_logger::tracing::warn!(
                        "Revision gap detected without patches: {} -> {}",
                        current_last_rev,
                        new_revision
                    );
                    save_state_for_effect.set(SaveState::Error(
                        "Live patch stream lost continuity; missing patch event".to_string(),
                    ));
                } else if !pending_patches.is_empty() {
                    // Apply pending patches in revision order
                    let mut patches_to_apply: Vec<_> = pending_patches
                        .into_iter()
                        .filter(|p| !p.applied && p.revision > current_last_rev)
                        .collect();
                    patches_to_apply.sort_by_key(|p| p.revision);

                    if !patches_to_apply.is_empty() {
                        if typing_locked_for_effect() {
                            new_version_available_for_effect.set(true);
                            return;
                        }

                        let mut current_content = content_for_effect();
                        let mut highest_revision = current_last_rev;
                        let mut latest_target_version = None::<u64>;
                        let selected_base = selected_version_id_for_effect();
                        let mut overlay_updates = Vec::<WriterOverlay>::new();

                        for patch in patches_to_apply {
                            dioxus_logger::tracing::debug!(
                                "Applying patch {} at revision {}",
                                patch.patch_id,
                                patch.revision
                            );

                            if patch.target_version_id.is_some() || patch.overlay_id.is_none() {
                                // Canonical version patch.
                                current_content = apply_patch_ops(&current_content, &patch.ops);
                                selected_overlays_for_effect.set(Vec::new());
                            } else if let Some(overlay_id) = patch.overlay_id.clone() {
                                if selected_base == patch.base_version_id {
                                    overlay_updates.push(WriterOverlay {
                                        overlay_id,
                                        base_version_id: patch.base_version_id.unwrap_or(0),
                                        author: "unknown".to_string(),
                                        kind: "proposal".to_string(),
                                        diff_ops: patch.ops.clone(),
                                        status: "pending".to_string(),
                                        created_at: chrono::Utc::now(),
                                    });
                                }
                            }
                            highest_revision = patch.revision;
                            if let Some(target_version_id) = patch.target_version_id {
                                latest_target_version = Some(target_version_id);
                            }

                            // Mark as applied in global state
                            if let Some(run) = ACTIVE_WRITER_RUNS.write().get_mut(&current_path) {
                                if let Some(p) = run
                                    .pending_patches
                                    .iter_mut()
                                    .find(|p| p.patch_id == patch.patch_id)
                                {
                                    p.applied = true;
                                }
                            }
                        }

                        content_for_effect.set(current_content);
                        selected_version_content_for_effect.set(content_for_effect());
                        revision_for_effect.set(highest_revision);
                        last_applied_revision_for_effect.set(highest_revision);
                        if let Some(target_version_id) = latest_target_version {
                            let mut ids = version_ids_for_effect();
                            ids.push(target_version_id);
                            let ids = normalize_version_ids(ids);
                            let selected =
                                reconcile_selected_version_id(&ids, Some(target_version_id));
                            version_ids_for_effect.set(ids);
                            selected_version_id_for_effect.set(selected);
                        }
                        if !overlay_updates.is_empty() {
                            selected_overlays_for_effect.set(overlay_updates);
                        }
                        new_version_available_for_effect.set(false);

                        // Update last_applied_revision in global state
                        if let Some(run) = ACTIVE_WRITER_RUNS.write().get_mut(&current_path) {
                            run.last_applied_revision = highest_revision;
                        }

                        // Update preview if in preview mode
                        if view_mode() == ViewMode::Preview {
                            let content_clone = content_for_effect();
                            let current_mime = mime();
                            let current_path_clone = current_path.clone();
                            spawn(async move {
                                let mut preview_signal = preview_html;
                                let _ = update_preview(
                                    content_clone,
                                    &current_mime,
                                    &current_path_clone,
                                    &mut preview_signal,
                                )
                                .await;
                            });
                        }
                    }
                }
            } else {
                // No active run for this document
                if active_run_id_for_effect.read().is_some() {
                    active_run_id_for_effect.set(None);
                    run_status_for_effect.set(None);
                }
                new_version_available_for_effect.set(false);
                selected_overlays_for_effect.set(Vec::new());
            }
        });
    }

    // Update preview when switching to preview mode
    use_effect(move || {
        let current_mode = view_mode();
        if current_mode != ViewMode::Preview {
            return;
        }

        let current_content = content();
        let current_mime = mime();
        let current_path = path();

        spawn(async move {
            let _ = update_preview(
                current_content,
                &current_mime,
                &current_path,
                &mut preview_html,
            )
            .await;
        });
    });

    // Keep selected version and version IDs reconciled after live updates.
    use_effect(move || {
        let ids = version_ids();
        if ids.is_empty() {
            if selected_version_id().is_some() {
                selected_version_id.set(None);
            }
            return;
        }

        let normalized = normalize_version_ids(ids.clone());
        if normalized != ids {
            version_ids.set(normalized.clone());
        }

        let selected = reconcile_selected_version_id(&normalized, selected_version_id());
        if selected != selected_version_id() {
            selected_version_id.set(selected);
        }
    });

    // Handle save
    let handle_save = use_callback(move |_| {
        if readonly() || matches!(save_state(), SaveState::Saving) {
            return;
        }

        let current_path = path();
        let current_content = content();
        let current_revision = revision();
        let current_parent_version_id = selected_version_id();
        let is_run_document = extract_run_id_from_document_path(&current_path).is_some();

        // Mark as saving
        save_state.set(SaveState::Saving);

        spawn(async move {
            let result = if is_run_document {
                writer_save_version(&current_path, &current_content, current_parent_version_id)
                    .await
                    .map(|saved| {
                        selected_version_id.set(Some(saved.version.version_id));
                        selected_version_content.set(saved.version.content.clone());
                        prompt_base_content.set(saved.version.content.clone());
                        let mut ids = version_ids();
                        if !ids.contains(&saved.version.version_id) {
                            ids.push(saved.version.version_id);
                            ids.sort_unstable();
                            version_ids.set(ids);
                        }
                        revision.set(current_revision.saturating_add(1));
                    })
            } else {
                writer_save(&current_path, current_revision, &current_content)
                    .await
                    .map(|response| {
                        revision.set(response.revision);
                    })
            };

            match result {
                Ok(response) => {
                    let _ = response;
                    save_state.set(SaveState::Saved);
                    typing_locked.set(false);
                    new_version_available.set(false);

                    // Clear "Saved" state after 2 seconds
                    spawn(async move {
                        TimeoutFuture::new(2000).await;
                        // Only clear if still in Saved state
                        save_state.set(SaveState::Clean);
                    });
                }
                Err(e) => {
                    if e.starts_with("CONFLICT:") {
                        // Parse conflict response
                        let parts: Vec<&str> = e.splitn(3, ':').collect();
                        if parts.len() >= 3 {
                            if let Ok(rev) = parts[1].parse::<u64>() {
                                save_state.set(SaveState::Conflict {
                                    current_revision: rev,
                                    current_content: parts[2].to_string(),
                                });
                                return;
                            }
                        }
                    }
                    save_state.set(SaveState::Error(e));
                }
            }
        });
    });

    // Handle writer prompt submit
    let handle_prompt_submit = use_callback(move |_| {
        if readonly() || prompt_submitting() {
            return;
        }
        let current_content = content();
        let prompt_payload = build_diff_ops(&selected_version_content(), &current_content);
        if prompt_payload.is_empty() {
            return;
        }
        let current_path = path();
        let base_version_id = selected_version_id().unwrap_or_else(|| revision());
        prompt_submitting.set(true);
        spawn(async move {
            match writer_prompt(&current_path, &prompt_payload, base_version_id).await {
                Ok(_response) => {
                    prompt_base_content.set(current_content);
                    typing_locked.set(false);
                }
                Err(e) => {
                    save_state.set(SaveState::Error(format!("Prompt failed: {e}")));
                }
            }
            prompt_submitting.set(false);
        });
    });

    // Handle content changes
    let on_content_change = use_callback(move |new_content: String| {
        // Update preview first if needed (before moving new_content)
        if view_mode() == ViewMode::Preview {
            let content_clone = new_content.clone();
            let current_mime = mime();
            let current_path = path();
            spawn(async move {
                let _ = update_preview(
                    content_clone,
                    &current_mime,
                    &current_path,
                    &mut preview_html,
                )
                .await;
            });
        }

        content.set(new_content);
        typing_locked.set(true);
        if !matches!(save_state(), SaveState::Dirty) {
            save_state.set(SaveState::Dirty);
        }
    });

    // Handle keyboard shortcuts
    let on_keydown = use_callback(move |e: KeyboardEvent| {
        if e.key() == Key::Character("s".to_string()) && e.modifiers().ctrl() {
            e.prevent_default();
            handle_save.call(());
        }
    });

    // Toggle view mode
    let set_view_mode = use_callback(move |mode: ViewMode| {
        view_mode.set(mode);
    });

    // Handle conflict resolution - reload latest
    let handle_reload_latest = use_callback(move |(new_content, new_revision): (String, u64)| {
        content.set(new_content.clone());
        revision.set(new_revision);
        save_state.set(SaveState::Clean);

        // Update preview if needed
        if view_mode() == ViewMode::Preview {
            let current_mime = mime();
            let current_path = path();
            spawn(async move {
                let _ =
                    update_preview(new_content, &current_mime, &current_path, &mut preview_html)
                        .await;
            });
        }
    });

    // Handle conflict resolution - overwrite
    let handle_overwrite = use_callback(move |_| {
        // Mark as dirty to trigger save
        save_state.set(SaveState::Dirty);
        handle_save.call(());
    });

    // Clear error state
    let clear_error = use_callback(move |_| {
        save_state.set(SaveState::Clean);
    });

    // Dismiss saved state
    let dismiss_saved = use_callback(move |_| {
        save_state.set(SaveState::Clean);
    });

    // Show Open File dialog
    let show_open_dialog = use_callback(move |_| {
        let initial_path = path();
        let base_path = initial_path
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
        spawn(async move {
            match list_directory(&base_path).await {
                Ok(response) => {
                    dialog.set(DialogState::OpenFile {
                        current_path: response.path,
                        entries: response.entries,
                    });
                }
                Err(e) => {
                    save_state.set(SaveState::Error(format!("Failed to open directory: {}", e)));
                }
            }
        });
    });

    // Show Save As dialog
    let show_save_as_dialog = use_callback(move |_| {
        let current_path = path();
        let (base_path, filename) = current_path
            .rsplit_once('/')
            .map(|(p, f)| (p.to_string(), f.to_string()))
            .unwrap_or_else(|| (String::new(), current_path.clone()));
        spawn(async move {
            match list_directory(&base_path).await {
                Ok(response) => {
                    dialog.set(DialogState::SaveAs {
                        current_path: response.path,
                        entries: response.entries,
                        filename,
                    });
                }
                Err(e) => {
                    save_state.set(SaveState::Error(format!("Failed to open directory: {}", e)));
                }
            }
        });
    });

    let handle_load_latest = use_callback(move |_| {
        let current_path = path();
        spawn(async move {
            match writer_open(&current_path).await {
                Ok(response) => {
                    content.set(response.content.clone());
                    selected_version_content.set(response.content.clone());
                    prompt_base_content.set(response.content);
                    revision.set(response.revision);
                    save_state.set(SaveState::Clean);
                    typing_locked.set(false);
                    new_version_available.set(false);
                    if extract_run_id_from_document_path(&current_path).is_some() {
                        if let Ok(versions_response) = writer_versions(&current_path).await {
                            let ids = normalize_version_ids(
                                versions_response
                                    .versions
                                    .iter()
                                    .map(|version| version.version_id)
                                    .collect(),
                            );
                            let selected = reconcile_selected_version_id(
                                &ids,
                                Some(versions_response.head_version_id),
                            );
                            version_ids.set(ids);
                            selected_version_id.set(selected);
                            if let Ok(version_response) =
                                writer_version(&current_path, versions_response.head_version_id)
                                    .await
                            {
                                content.set(version_response.version.content.clone());
                                selected_version_content
                                    .set(version_response.version.content.clone());
                                prompt_base_content.set(version_response.version.content.clone());
                                selected_overlays.set(version_response.overlays);
                            }
                        }
                    }
                }
                Err(e) => save_state.set(SaveState::Error(e)),
            }
        });
    });

    let handle_prev_version = use_callback(move |_| {
        let ids = version_ids();
        let Some(index) = selected_version_index(&ids, selected_version_id()) else {
            return;
        };
        if index == 0 {
            return;
        }
        let target = ids[index - 1];
        let current_path = path();
        spawn(async move {
            match writer_version(&current_path, target).await {
                Ok(response) => {
                    content.set(response.version.content.clone());
                    selected_version_content.set(response.version.content.clone());
                    prompt_base_content.set(response.version.content.clone());
                    selected_version_id.set(Some(target));
                    selected_overlays.set(response.overlays);
                    typing_locked.set(false);
                    save_state.set(SaveState::Clean);
                }
                Err(e) => save_state.set(SaveState::Error(e)),
            }
        });
    });

    let handle_next_version = use_callback(move |_| {
        let ids = version_ids();
        let Some(index) = selected_version_index(&ids, selected_version_id()) else {
            return;
        };
        if index + 1 >= ids.len() {
            return;
        }
        let target = ids[index + 1];
        let current_path = path();
        spawn(async move {
            match writer_version(&current_path, target).await {
                Ok(response) => {
                    content.set(response.version.content.clone());
                    selected_version_content.set(response.version.content.clone());
                    prompt_base_content.set(response.version.content.clone());
                    selected_version_id.set(Some(target));
                    selected_overlays.set(response.overlays);
                    typing_locked.set(false);
                    save_state.set(SaveState::Clean);
                }
                Err(e) => save_state.set(SaveState::Error(e)),
            }
        });
    });

    let current_path = path();
    let current_content = content();
    let current_mime = mime();
    let current_readonly = readonly();
    let current_save_state = save_state();
    let current_view_mode = view_mode();
    let is_markdown_file = is_markdown(&current_mime, &current_path);
    let is_loading = loading();
    let current_preview_html = preview_html();
    let current_run_status = run_status();
    let current_run_message = {
        let runs = ACTIVE_WRITER_RUNS.read();
        runs.get(&current_path).and_then(|r| r.message.clone())
    };
    let current_prompt_submitting = prompt_submitting();
    let has_prompt_diff = !build_diff_ops(&selected_version_content(), &current_content).is_empty();
    let current_version_ids = version_ids();
    let current_selected_version_id = selected_version_id();
    let current_selected_version_index =
        selected_version_index(&current_version_ids, current_selected_version_id);
    let current_total_versions = current_version_ids.len();
    let can_go_prev = current_selected_version_index.is_some_and(|idx| idx > 0);
    let can_go_next =
        current_selected_version_index.is_some_and(|idx| idx + 1 < current_total_versions);
    let current_selected_overlays = selected_overlays();
    let current_new_version_available = new_version_available();
    let current_editor_text = compose_editor_text(&current_content, &current_selected_overlays);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; background: var(--window-bg); color: var(--text-primary); overflow: hidden;",

            // Toolbar
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); flex-shrink: 0;",

                // Left: File info
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span { style: "font-size: 0.875rem; color: var(--text-secondary); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{current_path}"
                    }
                    if current_readonly {
                        span { style: "font-size: 0.75rem; color: var(--warning-bg); padding: 0.125rem 0.375rem; background: rgba(245, 158, 11, 0.1); border-radius: 0.25rem;",
                            "Read-only"
                        }
                    }
                    // Live run status indicator
                    if let Some(status) = current_run_status {
                        {
                            let (status_text, status_color) = match status {
                                WriterRunStatusKind::Initializing => ("Initializing...", "#94a3b8"),
                                WriterRunStatusKind::Running => ("Running...", "#3b82f6"),
                                WriterRunStatusKind::WaitingForWorker => ("Waiting...", "#f59e0b"),
                                WriterRunStatusKind::Completing => ("Completing...", "#10b981"),
                                WriterRunStatusKind::Completed => ("Completed", "#10b981"),
                                WriterRunStatusKind::Failed => ("Failed", "#ef4444"),
                                WriterRunStatusKind::Blocked => ("Blocked", "#ef4444"),
                            };
                            rsx! {
                                span {
                                    style: "font-size: 0.75rem; color: {status_color}; padding: 0.125rem 0.375rem; background: rgba(255, 255, 255, 0.05); border-radius: 0.25rem; display: flex; align-items: center; gap: 0.25rem;",
                                    if status != WriterRunStatusKind::Completed
                                        && status != WriterRunStatusKind::Failed
                                        && status != WriterRunStatusKind::Blocked
                                    {
                                        span { style: "animation: spin 1s linear infinite; display: inline-block;", "◐" }
                                    }
                                    "{status_text}"
                                }
                            }
                        }
                    }
                }
                if let Some(run_message) = current_run_message {
                    span {
                        style: "font-size: 0.75rem; color: var(--text-secondary); max-width: 360px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{run_message}"
                    }
                }

                // Center: View mode toggle (markdown only)
                div {
                    style: "display: flex; align-items: center; gap: 0.25rem;",
                    if !current_version_ids.is_empty() {
                        button {
                            style: if can_go_prev {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            } else {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: not-allowed; padding: 0.25rem 0.5rem; border-radius: 0.375rem; font-size: 0.875rem; opacity: 0.6;"
                            },
                            disabled: !can_go_prev,
                            onclick: move |_| handle_prev_version.call(()),
                            "<"
                        }
                        span {
                            style: "font-size: 0.8rem; color: var(--text-secondary); min-width: 90px; text-align: center;",
                            {
                                let selected = current_selected_version_index
                                    .map(|idx| idx + 1)
                                    .unwrap_or(current_version_ids.len());
                                let total = current_version_ids.len();
                                format!("v{} of {}", selected, total)
                            }
                        }
                        button {
                            style: if can_go_next {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            } else {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: not-allowed; padding: 0.25rem 0.5rem; border-radius: 0.375rem; font-size: 0.875rem; opacity: 0.6;"
                            },
                            disabled: !can_go_next,
                            onclick: move |_| handle_next_version.call(()),
                            ">"
                        }
                    }
                    if is_markdown_file {
                        button {
                            style: if current_view_mode == ViewMode::Edit {
                                "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            } else {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            },
                            onclick: move |_| set_view_mode.call(ViewMode::Edit),
                            "Edit"
                        }
                        button {
                            style: if current_view_mode == ViewMode::Preview {
                                "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            } else {
                                "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                            },
                            onclick: move |_| set_view_mode.call(ViewMode::Preview),
                            "Preview"
                        }
                    }
                }

                // Right: File operations, Save button and status
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| show_open_dialog.call(()),
                        "Open..."
                    }
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| show_save_as_dialog.call(()),
                        "Save As..."
                    }
                    {render_save_status(&current_save_state,
                        dismiss_saved.clone(),
                    )}
                    button {
                        style: if current_prompt_submitting || current_readonly || !has_prompt_diff {
                            "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: not-allowed; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem; opacity: 0.6;"
                        } else {
                            "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                        },
                        disabled: current_prompt_submitting || current_readonly || !has_prompt_diff,
                        onclick: move |_| handle_prompt_submit.call(()),
                        if current_prompt_submitting {
                            "Prompting..."
                        } else {
                            "Prompt"
                        }
                    }
                    button {
                        style: match current_save_state {
                            SaveState::Clean | SaveState::Saving | SaveState::Saved => {
                                "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: not-allowed; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem; opacity: 0.5;"
                            }
                            _ => "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                        },
                        disabled: matches!(current_save_state, SaveState::Clean | SaveState::Saving | SaveState::Saved) || current_readonly,
                        onclick: move |_| handle_save.call(()),
                        if matches!(current_save_state, SaveState::Saving) {
                            "Saving..."
                        } else {
                            "Save"
                        }
                    }
                }
            }

            // Loading indicator
            if is_loading {
                div {
                    style: "padding: 0.5rem 1rem; background: var(--accent-bg); color: var(--accent-text); font-size: 0.875rem;",
                    "Loading document..."
                }
            }

            if current_new_version_available {
                div {
                    style: "padding: 0.6rem 1rem; background: rgba(59, 130, 246, 0.12); color: var(--text-primary); font-size: 0.85rem; border-bottom: 1px solid var(--border-color); display: flex; align-items: center; justify-content: space-between;",
                    span { "New version available while you were editing." }
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-primary); cursor: pointer; padding: 0.25rem 0.6rem; border-radius: 0.3rem; font-size: 0.8rem;",
                        onclick: move |_| handle_load_latest.call(()),
                        "Load Latest"
                    }
                }
            }

            // Error banner
            if let SaveState::Error(ref msg) = current_save_state {
                div {
                    style: "padding: 0.75rem 1rem; background: var(--danger-bg); color: var(--danger-text); font-size: 0.875rem; border-bottom: 1px solid var(--danger-bg); display: flex; justify-content: space-between; align-items: center;",
                    div { "Error: {msg}" }
                    button {
                        style: "background: transparent; border: 1px solid var(--danger-text); color: var(--danger-text); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem;",
                        onclick: move |_| clear_error.call(()),
                        "Dismiss"
                    }
                }
            }

            // Conflict resolution UI
            if let SaveState::Conflict { current_revision, current_content } = &current_save_state {
                div {
                    style: "padding: 0.75rem 1rem; background: var(--warning-bg); color: var(--warning-bg); font-size: 0.875rem; border-bottom: 1px solid var(--border-color);",
                    div { style: "margin-bottom: 0.5rem;", "Conflict detected: Document was modified (revision {current_revision})" }
                    div {
                        style: "display: flex; gap: 0.5rem;",
                        button {
                            style: "background: transparent; border: 1px solid var(--warning-bg); color: var(--warning-bg); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                            onclick: {
                                let content = current_content.clone();
                                let rev = *current_revision;
                                move |_| handle_reload_latest.call((content.clone(), rev))
                            },
                            "Reload Latest"
                        }
                        button {
                            style: "background: var(--warning-bg); border: none; color: var(--warning-bg); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                            onclick: move |_| handle_overwrite.call(()),
                            "Overwrite"
                        }
                    }
                }
            }

            // Main content area
            div {
                style: "flex: 1; overflow: hidden; display: flex;",

                match current_view_mode {
                    ViewMode::Edit => rsx! {
                        textarea {
                            style: "flex: 1; width: calc(100% - 1rem); height: 100%; padding: 1rem; background: var(--input-bg, var(--window-bg)); color: var(--text-primary); border: 1px solid var(--border-color); resize: none; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.875rem; line-height: 1.6; outline: none; border-radius: 0.25rem; margin: 0.5rem;",
                            value: "{current_editor_text}",
                            readonly: current_readonly,
                            oninput: move |e: FormEvent| {
                                let stripped = strip_inline_overlay_block(&e.value());
                                on_content_change.call(stripped);
                            },
                            onkeydown: move |e: KeyboardEvent| on_keydown.call(e),
                        }
                    },
                    ViewMode::Preview => rsx! {
                        div {
                            style: "flex: 1; overflow: auto; padding: 1.5rem; background: var(--window-bg); font-family: system-ui, -apple-system, sans-serif;",
                            div {
                                style: "max-width: 800px; margin: 0 auto; color: var(--text-primary);",
                                dangerous_inner_html: "{current_preview_html}"
                            }
                        }
                    }
                }
            }

            // Dialog overlays
            {render_dialog(
                dialog,
                path,
                content,
                revision,
                mime,
                readonly,
                save_state,
                view_mode,
                preview_html,
                loading,
                loaded_path,
            )}
        }
    }
}

/// Render dialog overlays
fn render_dialog(
    mut dialog: Signal<DialogState>,
    mut path: Signal<String>,
    mut content: Signal<String>,
    mut revision: Signal<u64>,
    mut mime: Signal<String>,
    mut readonly: Signal<bool>,
    mut save_state: Signal<SaveState>,
    mut view_mode: Signal<ViewMode>,
    mut preview_html: Signal<String>,
    mut loading: Signal<bool>,
    mut loaded_path: Signal<Option<String>>,
) -> Element {
    let current_dialog = dialog();

    match current_dialog {
        DialogState::None => rsx! {},
        DialogState::OpenFile {
            current_path,
            entries,
        } => rsx! {
            FileBrowserDialog {
                title: "Open File",
                current_path,
                entries,
                show_filename_input: false,
                filename: String::new(),
                on_navigate: move |new_path: String| {
                    spawn(async move {
                        match list_directory(&new_path).await {
                            Ok(response) => {
                                dialog.set(DialogState::OpenFile {
                                    current_path: response.path,
                                    entries: response.entries,
                                });
                            }
                            Err(_) => {}
                        }
                    });
                },
                on_select: move |selected_path: String| {
                    dialog.set(DialogState::None);
                    spawn(async move {
                        loading.set(true);
                        match writer_open(&selected_path).await {
                            Ok(response) => {
                                let is_md = is_markdown(&response.mime, &response.path);
                                path.set(response.path);
                                content.set(response.content);
                                revision.set(response.revision);
                                mime.set(response.mime);
                                readonly.set(response.readonly);
                                save_state.set(SaveState::Clean);
                                view_mode.set(ViewMode::Edit);
                                loading.set(false);

                                if is_md {
                                    let content_clone = content();
                                    let _ = update_preview(content_clone, &mime(), &path(), &mut preview_html).await;
                                }
                                loaded_path.set(Some(path()));
                            }
                            Err(e) => {
                                save_state.set(SaveState::Error(e));
                                loading.set(false);
                            }
                        }
                    });
                },
                on_cancel: move || dialog.set(DialogState::None),
            }
        },
        DialogState::SaveAs {
            current_path,
            entries,
            filename,
        } => rsx! {
            FileBrowserDialog {
                title: "Save As",
                current_path,
                entries,
                show_filename_input: true,
                filename: filename.clone(),
                on_navigate: move |new_path: String| {
                    let current_filename = filename.clone();
                    spawn(async move {
                        match list_directory(&new_path).await {
                            Ok(response) => {
                                dialog.set(DialogState::SaveAs {
                                    current_path: response.path,
                                    entries: response.entries,
                                    filename: current_filename,
                                });
                            }
                            Err(_) => {}
                        }
                    });
                },
                on_select: move |selected_path: String| {
                    dialog.set(DialogState::None);
                    spawn(async move {
                        save_state.set(SaveState::Saving);
                        match writer_save(&selected_path, 0, &content()).await {
                            Ok(response) => {
                                path.set(selected_path);
                                revision.set(response.revision);
                                save_state.set(SaveState::Saved);
                                TimeoutFuture::new(2000).await;
                                save_state.set(SaveState::Clean);
                            }
                            Err(e) => {
                                save_state.set(SaveState::Error(e));
                            }
                        }
                    });
                },
                on_cancel: move || dialog.set(DialogState::None),
            }
        },
    }
}

/// File browser dialog component
#[component]
fn FileBrowserDialog(
    title: String,
    current_path: String,
    entries: Vec<DirectoryEntry>,
    show_filename_input: bool,
    filename: String,
    on_navigate: Callback<String>,
    on_select: Callback<String>,
    on_cancel: Callback<()>,
) -> Element {
    let mut current_filename = use_signal(|| filename);

    let get_file_icon = |entry: &DirectoryEntry| -> &'static str {
        if entry.is_dir {
            "📁"
        } else if entry.name.ends_with(".rs") {
            "🦀"
        } else if entry.name.ends_with(".md") {
            "📝"
        } else if entry.name.ends_with(".toml")
            || entry.name.ends_with(".yaml")
            || entry.name.ends_with(".yml")
            || entry.name.ends_with(".json")
        {
            "⚙️"
        } else if entry.name.ends_with(".txt") {
            "📄"
        } else if entry.name.ends_with(".sh") {
            "🖥️"
        } else {
            "📃"
        }
    };

    let navigate_up = {
        let current = current_path.clone();
        let on_navigate = on_navigate.clone();
        move |_| {
            if current.is_empty() {
                return;
            }
            let parent = current
                .rsplit_once('/')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            on_navigate.call(parent);
        }
    };

    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0, 0, 0, 0.7); display: flex; align-items: center; justify-content: center; z-index: 1000;",
            onclick: move |_| on_cancel.call(()),
            div {
                style: "background: var(--window-bg); border: 1px solid var(--border-color); border-radius: 0.5rem; padding: 1.5rem; min-width: 480px; max-width: 90vw; max-height: 80vh; display: flex; flex-direction: column;",
                onclick: move |e| e.stop_propagation(),
                h3 { style: "margin: 0 0 1rem 0; font-size: 1.125rem; color: var(--text-primary);", "{title}" }

                // Current path and navigation
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.75rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem;",
                        onclick: navigate_up,
                        "↑ Up"
                    }
                    span { style: "font-size: 0.875rem; color: var(--text-secondary); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        if current_path.is_empty() { "Home" } else { "{current_path}" }
                    }
                }

                // File list
                div {
                    style: "flex: 1; overflow: auto; max-height: 300px; border: 1px solid var(--border-color); border-radius: 0.375rem; margin-bottom: 1rem;",
                    if entries.is_empty() {
                        div {
                            style: "padding: 1rem; text-align: center; color: var(--text-muted); font-size: 0.875rem;",
                            "This folder is empty"
                        }
                    } else {
                        for entry in entries.clone() {
                            div {
                                key: "{entry.path}",
                                style: if entry.is_dir {
                                    "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem; cursor: pointer; hover:background: var(--hover-bg); font-size: 0.875rem; color: var(--text-primary); border-bottom: 1px solid var(--border-color);"
                                } else {
                                    "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem; cursor: pointer; hover:background: var(--hover-bg); font-size: 0.875rem; color: var(--text-primary); border-bottom: 1px solid var(--border-color); opacity: 0.7;"
                                },
                                onclick: move |_| {
                                    if entry.is_dir {
                                        on_navigate.call(entry.path.clone());
                                    } else {
                                        on_select.call(entry.path.clone());
                                    }
                                },
                                div { "{get_file_icon(&entry)}" }
                                div { "{entry.name}" }
                            }
                        }
                    }
                }

                // Filename input (for Save As)
                if show_filename_input {
                    div {
                        style: "margin-bottom: 1rem;",
                        input {
                            style: "width: 100%; padding: 0.5rem 0.75rem; background: var(--input-bg); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: 0.375rem; font-size: 0.875rem; box-sizing: border-box;",
                            value: "{current_filename()}",
                            placeholder: "Filename",
                            oninput: move |e: FormEvent| {
                                current_filename.set(e.value());
                            }
                        }
                    }
                }

                // Buttons
                div {
                    style: "display: flex; justify-content: flex-end; gap: 0.5rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    if show_filename_input {
                        button {
                            style: "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                            onclick: move |_| {
                                let full_path = if current_path.is_empty() {
                                    current_filename()
                                } else {
                                    format!("{}/{}", current_path, current_filename())
                                };
                                on_select.call(full_path);
                            },
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

/// Update the preview HTML
async fn update_preview(
    content: String,
    mime: &str,
    path: &str,
    preview_html: &mut Signal<String>,
) {
    if !is_markdown(mime, path) {
        preview_html.set(format!("<pre>{}</pre>", html_escape(&content)));
        return;
    }

    match writer_preview(Some(&content), Some(path)).await {
        Ok(response) => {
            preview_html.set(response.html);
        }
        Err(e) => {
            dioxus_logger::tracing::error!("Preview failed: {}", e);
            preview_html.set(format!("<pre>Error rendering preview: {}</pre>", e));
        }
    }
}

/// Simple HTML escape for non-markdown files
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render save status indicator
fn render_save_status(save_state: &SaveState, on_dismiss_saved: EventHandler<()>) -> Element {
    match save_state {
        SaveState::Clean => rsx! {
            span { style: "font-size: 0.875rem; color: var(--text-secondary);", "" }
        },
        SaveState::Dirty => rsx! {
            span { style: "font-size: 0.875rem; color: var(--warning-bg);", "Modified" }
        },
        SaveState::Saving => rsx! {
            span { style: "font-size: 0.875rem; color: var(--accent-bg);", "Saving..." }
        },
        SaveState::Saved => rsx! {
            div {
                style: "display: flex; align-items: center; gap: 0.5rem;",
                span { style: "font-size: 0.875rem; color: var(--success-bg);", "Saved" }
                button {
                    style: "background: transparent; border: none; color: var(--text-secondary); cursor: pointer; font-size: 0.75rem;",
                    onclick: move |_| on_dismiss_saved.call(()),
                    "Dismiss"
                }
            }
        },
        SaveState::Conflict { .. } => rsx! {
            span { style: "font-size: 0.875rem; color: var(--danger-text); font-weight: bold;", "CONFLICT!" }
        },
        SaveState::Error(_) => rsx! {
            span { style: "font-size: 0.875rem; color: var(--danger-text);", "Error" }
        },
    }
}
