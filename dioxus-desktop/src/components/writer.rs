//! Writer App Component
//!
//! Document editor with revision-based optimistic concurrency control.
//! Supports both edit and preview modes for markdown files.
//! Supports live patch apply from writer.run.* websocket events.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::files_api::{list_directory, DirectoryEntry};
use crate::api::{writer_open, writer_preview, writer_prompt, writer_save};
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

fn build_diff_prompt(base: &str, edited: &str) -> Option<String> {
    if base == edited {
        return None;
    }

    let base_lines: Vec<&str> = base.lines().collect();
    let edited_lines: Vec<&str> = edited.lines().collect();
    let max_len = base_lines.len().max(edited_lines.len());

    let mut removed = Vec::new();
    let mut added = Vec::new();

    for idx in 0..max_len {
        match (base_lines.get(idx), edited_lines.get(idx)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                removed.push(format!("- {a}"));
                added.push(format!("+ {b}"));
            }
            (Some(a), None) => removed.push(format!("- {a}")),
            (None, Some(b)) => added.push(format!("+ {b}")),
            (None, None) => {}
        }
    }

    let mut body = String::from(
        "Apply the following user-authored document edits as intent for the next revision.\n\
         Treat this diff as the human prompt and update the narrative accordingly.\n\n",
    );
    if !removed.is_empty() {
        body.push_str("Removed:\n");
        body.push_str(&removed.join("\n"));
        body.push_str("\n\n");
    }
    if !added.is_empty() {
        body.push_str("Added:\n");
        body.push_str(&added.join("\n"));
        body.push('\n');
    }
    Some(body)
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

                    path.set(response.path);
                    content.set(response.content.clone());
                    prompt_base_content.set(response.content);
                    revision.set(response.revision);
                    last_applied_revision.set(response.revision);
                    mime.set(response.mime);
                    readonly.set(response.readonly);
                    save_state.set(SaveState::Clean);
                    view_mode.set(ViewMode::Edit);
                    loading.set(false);

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
                        let mut current_content = content_for_effect();
                        let mut highest_revision = current_last_rev;

                        for patch in patches_to_apply {
                            dioxus_logger::tracing::debug!(
                                "Applying patch {} at revision {}",
                                patch.patch_id,
                                patch.revision
                            );

                            // Apply the patch operations
                            current_content = apply_patch_ops(&current_content, &patch.ops);
                            highest_revision = patch.revision;

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
                        revision_for_effect.set(highest_revision);
                        last_applied_revision_for_effect.set(highest_revision);

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

    // Handle save
    let handle_save = use_callback(move |_| {
        if readonly() || matches!(save_state(), SaveState::Saving) {
            return;
        }

        let current_path = path();
        let current_content = content();
        let current_revision = revision();

        // Mark as saving
        save_state.set(SaveState::Saving);

        spawn(async move {
            match writer_save(&current_path, current_revision, &current_content).await {
                Ok(response) => {
                    revision.set(response.revision);
                    save_state.set(SaveState::Saved);

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
        let Some(prompt_payload) = build_diff_prompt(&prompt_base_content(), &current_content) else {
            return;
        };
        let current_path = path();
        prompt_submitting.set(true);
        spawn(async move {
            match writer_prompt(&current_path, &prompt_payload).await {
                Ok(_response) => {
                    prompt_base_content.set(current_content);
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
    let has_prompt_diff = build_diff_prompt(&prompt_base_content(), &current_content).is_some();

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
                            style: "flex: 1; width: 100%; height: 100%; padding: 1rem; background: var(--input-bg, var(--window-bg)); color: var(--text-primary); border: 1px solid var(--border-color); resize: none; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.875rem; line-height: 1.6; outline: none; border-radius: 0.25rem; margin: 0.5rem;",
                            value: "{current_content}",
                            readonly: current_readonly,
                            oninput: move |e: FormEvent| on_content_change.call(e.value()),
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
