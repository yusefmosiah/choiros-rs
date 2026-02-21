//! Main WriterView component

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::Cell;

use crate::api::files_api::list_directory;
use crate::api::{
    conductor_get_run_status, conductor_list_runs, writer_dismiss_overlay, writer_open,
    writer_prompt, writer_save, writer_save_version, writer_version, writer_versions,
    WriterOverlay,
};
use crate::desktop::state::{ActiveWriterRun, ACTIVE_WRITER_RUNS};
use shared_types::{
    ChangesetImpact, ConductorRunStatus, ConductorRunStatusResponse, PatchOp, WriterRunStatusKind,
};

use super::dialogs::render_dialog;
use super::logic::*;
use super::styles::*;
use super::types::*;

#[derive(Clone)]
struct MarginNote {
    id: String,
    impact: Option<ChangesetImpact>,
    title: String,
    lines: Vec<String>,
    overlay_id: Option<String>,
}

fn is_terminal_status(status: WriterRunStatusKind) -> bool {
    matches!(
        status,
        WriterRunStatusKind::Completed | WriterRunStatusKind::Failed | WriterRunStatusKind::Blocked
    )
}

fn map_conductor_to_writer_status(status: ConductorRunStatus) -> WriterRunStatusKind {
    match status {
        ConductorRunStatus::Initializing => WriterRunStatusKind::Initializing,
        ConductorRunStatus::Running => WriterRunStatusKind::Running,
        ConductorRunStatus::WaitingForCalls => WriterRunStatusKind::WaitingForWorker,
        ConductorRunStatus::Completing => WriterRunStatusKind::Completing,
        ConductorRunStatus::Completed => WriterRunStatusKind::Completed,
        ConductorRunStatus::Failed => WriterRunStatusKind::Failed,
        ConductorRunStatus::Blocked => WriterRunStatusKind::Blocked,
    }
}

fn sanitize_dom_id(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

thread_local! {
    static PROSE_INTEROP_LOADED: Cell<bool> = const { Cell::new(false) };
}

fn ensure_prose_interop_loaded() {
    PROSE_INTEROP_LOADED.with(|loaded| {
        if loaded.get() {
            return;
        }
        let script = include_str!("prose_interop.js");
        if js_sys::eval(script).is_ok() {
            loaded.set(true);
        }
    });
}

fn prose_set_markdown(editor_id: &str, markdown: &str) {
    ensure_prose_interop_loaded();
    let id = serde_json::to_string(editor_id).unwrap_or_else(|_| "\"\"".to_string());
    let md = serde_json::to_string(markdown).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        "window.__writerProseInterop && window.__writerProseInterop.setMarkdown({id}, {md});"
    );
    let _ = js_sys::eval(&script);
}

fn prose_get_markdown(editor_id: &str) -> String {
    ensure_prose_interop_loaded();
    let id = serde_json::to_string(editor_id).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        "(window.__writerProseInterop && window.__writerProseInterop.getMarkdown({id})) || ''"
    );
    js_sys::eval(&script)
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

fn prose_apply_shortcuts(editor_id: &str) {
    ensure_prose_interop_loaded();
    let id = serde_json::to_string(editor_id).unwrap_or_else(|_| "\"\"".to_string());
    let script =
        format!("window.__writerProseInterop && window.__writerProseInterop.applyShortcuts({id});");
    let _ = js_sys::eval(&script);
}

fn prose_compute_bubble_tops(editor_id: &str, count: usize) -> Vec<i32> {
    if count == 0 {
        return Vec::new();
    }
    ensure_prose_interop_loaded();
    let id = serde_json::to_string(editor_id).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        "JSON.stringify((window.__writerProseInterop && window.__writerProseInterop.computeBubbleTops({id}, {})) || [])",
        count
    );
    let raw = js_sys::eval(&script)
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| "[]".to_string());
    serde_json::from_str::<Vec<i32>>(&raw).unwrap_or_default()
}

async fn reconcile_run_state_on_open(
    opened_path: &str,
    run_id: &str,
    revision: u64,
) -> Option<WriterRunStatusKind> {
    let status_response = conductor_get_run_status(run_id).await.ok()?;
    let writer_status = map_conductor_to_writer_status(status_response.status);

    let mut runs = ACTIVE_WRITER_RUNS.write();

    if let Some(existing) = runs.get(opened_path) {
        if is_terminal_status(existing.status) && existing.last_applied_revision == revision {
            runs.remove(opened_path);
        }
    }

    if is_terminal_status(writer_status) {
        runs.insert(
            opened_path.to_string(),
            ActiveWriterRun {
                run_id: run_id.to_string(),
                document_path: opened_path.to_string(),
                revision,
                status: writer_status,
                objective: Some(status_response.objective),
                phase: None,
                message: None,
                progress_pct: Some(100),
                proposal: None,
                pending_patches: Vec::new(),
                last_applied_revision: revision,
                recent_changesets: Vec::new(),
            },
        );
    } else if let Some(existing) = runs.get_mut(opened_path) {
        existing.run_id = run_id.to_string();
        existing.status = writer_status;
        existing.objective = Some(status_response.objective);
        existing.revision = existing.revision.max(revision);
        existing.last_applied_revision = existing.last_applied_revision.max(revision);
    }

    Some(writer_status)
}

#[component]
pub fn WriterView(desktop_id: String, window_id: String, initial_path: String) -> Element {
    let prose_editor_id = format!(
        "writer-prose-{}-{}",
        sanitize_dom_id(&desktop_id),
        sanitize_dom_id(&window_id)
    );

    let mut writer_view_mode = use_signal(|| {
        if initial_path.is_empty() {
            WriterViewMode::Overview
        } else {
            WriterViewMode::Editor
        }
    });

    let mut overview_entries = use_signal(Vec::<ConductorRunStatusResponse>::new);
    let mut overview_loaded = use_signal(|| false);

    {
        use_effect(move || {
            if writer_view_mode() != WriterViewMode::Overview {
                return;
            }
            if overview_loaded() {
                return;
            }
            overview_loaded.set(true);
            spawn(async move {
                match conductor_list_runs().await {
                    Ok(runs) => overview_entries.set(runs),
                    Err(_) => overview_entries.set(Vec::new()),
                }
            });
        });
    }

    let mut content = use_signal(String::new);
    let mut revision = use_signal(|| 0u64);
    let mut mime = use_signal(|| "text/plain".to_string());
    let mut path = use_signal(|| initial_path.clone());
    let mut readonly = use_signal(|| false);
    let mut save_state = use_signal(|| SaveState::Clean);

    let mut loading = use_signal(|| !initial_path.is_empty());
    let mut dialog = use_signal(|| DialogState::None);
    let mut loaded_path = use_signal(|| None::<String>);
    let mut prompt_submitting = use_signal(|| false);
    let mut prompt_base_content = use_signal(String::new);
    let mut version_ids = use_signal(Vec::<u64>::new);
    let mut selected_version_id = use_signal(|| None::<u64>);
    let mut selected_version_content = use_signal(String::new);
    let mut selected_version_source = use_signal(String::new);
    let mut selected_overlays = use_signal(|| Vec::<WriterOverlay>::new());
    let mut typing_locked = use_signal(|| false);
    let mut new_version_available = use_signal(|| false);
    let mut right_margin_open = use_signal(|| false);
    let mut mobile_sheet_note_idx = use_signal(|| None::<usize>);

    let active_run_id = use_signal(|| None::<String>);
    let _pending_patches = use_signal(|| Vec::<(u64, Vec<PatchOp>)>::new());
    let mut last_applied_revision = use_signal(|| 0u64);
    let mut run_status = use_signal(|| None::<WriterRunStatusKind>);

    use_effect(move || {
        let path_str = path();
        if path_str.is_empty() {
            loading.set(false);
            loaded_path.set(None);
            run_status.set(None);
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
                    typing_locked.set(false);
                    new_version_available.set(false);
                    right_margin_open.set(false);
                    mobile_sheet_note_idx.set(None);

                    if let Some(run_id) = extract_run_id_from_document_path(&opened_path) {
                        if let Some(status) =
                            reconcile_run_state_on_open(&opened_path, &run_id, response.revision)
                                .await
                        {
                            run_status.set(Some(status));
                        }

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
                                        selected_version_source
                                            .set(version_response.version.source.clone());
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
                        run_status.set(None);
                    }
                    loading.set(false);
                }
                Err(e) => {
                    save_state.set(SaveState::Error(e));
                    loaded_path.set(None);
                    loading.set(false);
                }
            }
        });
    });

    {
        let path_for_effect = path;
        let loading_for_effect = loading;
        let mut content_for_effect = content;
        let mut revision_for_effect = revision;
        let mut last_applied_revision_for_effect = last_applied_revision;
        let mut active_run_id_for_effect = active_run_id;
        let mut run_status_for_effect = run_status;
        let typing_locked_for_effect = typing_locked;
        let mut new_version_available_for_effect = new_version_available;
        let mut version_ids_for_effect = version_ids;
        let mut selected_version_id_for_effect = selected_version_id;
        let mut selected_version_content_for_effect = selected_version_content;
        let mut selected_overlays_for_effect = selected_overlays;

        use_effect(move || {
            let current_path = path_for_effect();
            if loading_for_effect() {
                return;
            }
            let runs = ACTIVE_WRITER_RUNS.read();

            if let Some(run_state) = runs.get(&current_path) {
                let run_id = run_state.run_id.clone();
                let new_revision = run_state.revision;
                let status = run_state.status;
                let pending_patches = run_state.pending_patches.clone();
                let run_last_applied = run_state.last_applied_revision;

                drop(runs);

                if active_run_id_for_effect.read().as_ref() != Some(&run_id) {
                    active_run_id_for_effect.set(Some(run_id));
                    let local_rev = last_applied_revision_for_effect();
                    if run_last_applied > local_rev {
                        last_applied_revision_for_effect.set(run_last_applied);
                    }
                }

                run_status_for_effect.set(Some(status));

                let is_terminal = is_terminal_status(status);
                let current_last_rev = last_applied_revision_for_effect();
                if !is_terminal
                    && has_revision_gap(current_last_rev, new_revision)
                    && pending_patches.is_empty()
                {
                    dioxus_logger::tracing::warn!(
                        "Revision gap detected without patches: {} -> {}",
                        current_last_rev,
                        new_revision
                    );
                    new_version_available_for_effect.set(true);
                } else if !pending_patches.is_empty() {
                    let mut patches_to_apply: Vec<_> = pending_patches
                        .iter()
                        .filter(|p| !p.applied && p.revision > current_last_rev)
                        .cloned()
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
                            if patch.target_version_id.is_some() || patch.overlay_id.is_none() {
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

                        if let Some(run) = ACTIVE_WRITER_RUNS.write().get_mut(&current_path) {
                            run.last_applied_revision = highest_revision;
                        }
                    }
                }

                if is_terminal && pending_patches.iter().all(|patch| patch.applied) {
                    let should_remove = {
                        let runs = ACTIVE_WRITER_RUNS.read();
                        runs.get(&current_path)
                            .map(|run| run.last_applied_revision >= run.revision)
                            .unwrap_or(false)
                    };
                    if should_remove {
                        ACTIVE_WRITER_RUNS.write().remove(&current_path);
                    }
                }
            } else {
                if active_run_id_for_effect.read().is_some() {
                    active_run_id_for_effect.set(None);
                }
                new_version_available_for_effect.set(false);
            }
        });
    }

    {
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
    }

    {
        let prose_editor_id = prose_editor_id.clone();
        use_effect(move || {
            ensure_prose_interop_loaded();
            if writer_view_mode() != WriterViewMode::Editor {
                return;
            }
            if loading() || typing_locked() {
                return;
            }
            prose_set_markdown(&prose_editor_id, &content());
        });
    }

    let handle_save = use_callback(move |_| {
        if readonly() || matches!(save_state(), SaveState::Saving) {
            return;
        }

        let current_path = path();
        let current_content = content();
        let current_revision = revision();
        let current_parent_version_id = selected_version_id();
        let is_run_document = extract_run_id_from_document_path(&current_path).is_some();

        save_state.set(SaveState::Saving);

        spawn(async move {
            let result = if is_run_document {
                writer_save_version(&current_path, &current_content, current_parent_version_id)
                    .await
                    .map(|saved| {
                        selected_version_id.set(Some(saved.version.version_id));
                        selected_version_content.set(saved.version.content.clone());
                        prompt_base_content.set(saved.version.content.clone());
                        selected_version_source.set(saved.version.source.clone());
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
                Ok(()) => {
                    save_state.set(SaveState::Saved);
                    typing_locked.set(false);
                    new_version_available.set(false);
                    spawn(async move {
                        TimeoutFuture::new(2000).await;
                        save_state.set(SaveState::Clean);
                    });
                }
                Err(e) => {
                    if e.starts_with("CONFLICT:") {
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

    let on_content_change = use_callback(move |new_content: String| {
        content.set(new_content);
        typing_locked.set(true);
        if !matches!(save_state(), SaveState::Dirty) {
            save_state.set(SaveState::Dirty);
        }
    });

    {
        use_effect(move || {
            if !matches!(save_state(), SaveState::Dirty) {
                return;
            }
            if readonly() {
                return;
            }
            spawn(async move {
                TimeoutFuture::new(2000).await;
                if matches!(save_state(), SaveState::Dirty) {
                    handle_save.call(());
                }
            });
        });
    }

    let on_keydown = use_callback(move |e: KeyboardEvent| {
        if e.key() == Key::Character("s".to_string()) && e.modifiers().ctrl() {
            e.prevent_default();
            handle_save.call(());
        }
    });

    let handle_reload_latest = use_callback(move |(new_content, new_revision): (String, u64)| {
        content.set(new_content.clone());
        selected_version_content.set(new_content.clone());
        prompt_base_content.set(new_content);
        revision.set(new_revision);
        save_state.set(SaveState::Clean);
        typing_locked.set(false);
    });

    let handle_overwrite = use_callback(move |_| {
        save_state.set(SaveState::Dirty);
        handle_save.call(());
    });

    let clear_error = use_callback(move |_| {
        save_state.set(SaveState::Clean);
    });

    let dismiss_saved = use_callback(move |_| {
        save_state.set(SaveState::Clean);
    });

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
                    save_state.set(SaveState::Error(format!("Failed to open directory: {e}")));
                }
            }
        });
    });

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
                    save_state.set(SaveState::Error(format!("Failed to open directory: {e}")));
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
                                selected_version_source
                                    .set(version_response.version.source.clone());
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
                    selected_version_source.set(response.version.source.clone());
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
                    selected_version_source.set(response.version.source.clone());
                    selected_overlays.set(response.overlays);
                    typing_locked.set(false);
                    save_state.set(SaveState::Clean);
                }
                Err(e) => save_state.set(SaveState::Error(e)),
            }
        });
    });

    let handle_accept_overlay = use_callback(move |overlay_id: String| {
        let overlays = selected_overlays();
        let Some(overlay) = overlays
            .iter()
            .find(|item| item.overlay_id == overlay_id)
            .cloned()
        else {
            return;
        };

        if selected_version_id() != Some(overlay.base_version_id) {
            save_state.set(SaveState::Error(
                "Cannot accept suggestion on a different base version".to_string(),
            ));
            return;
        }

        let merged_content = apply_patch_ops(&selected_version_content(), &overlay.diff_ops);
        let current_path = path();
        save_state.set(SaveState::Saving);

        spawn(async move {
            match writer_save_version(
                &current_path,
                &merged_content,
                Some(overlay.base_version_id),
            )
            .await
            {
                Ok(saved) => {
                    content.set(saved.version.content.clone());
                    selected_version_content.set(saved.version.content.clone());
                    prompt_base_content.set(saved.version.content.clone());
                    selected_version_source.set(saved.version.source.clone());
                    selected_version_id.set(Some(saved.version.version_id));
                    let mut ids = version_ids();
                    if !ids.contains(&saved.version.version_id) {
                        ids.push(saved.version.version_id);
                        ids.sort_unstable();
                        version_ids.set(ids);
                    }
                    selected_overlays.set(
                        selected_overlays()
                            .into_iter()
                            .filter(|item| item.overlay_id != overlay.overlay_id)
                            .collect(),
                    );
                    typing_locked.set(false);
                    save_state.set(SaveState::Saved);
                    spawn(async move {
                        TimeoutFuture::new(1200).await;
                        save_state.set(SaveState::Clean);
                    });
                }
                Err(e) => save_state.set(SaveState::Error(format!("Accept failed: {e}"))),
            }
        });
    });

    let handle_dismiss_overlay = use_callback(move |overlay_id: String| {
        let current_path = path();
        spawn(async move {
            match writer_dismiss_overlay(&current_path, &overlay_id).await {
                Ok(_response) => {
                    selected_overlays.set(
                        selected_overlays()
                            .into_iter()
                            .filter(|item| item.overlay_id != overlay_id)
                            .collect(),
                    );
                }
                Err(e) => save_state.set(SaveState::Error(format!("Dismiss failed: {e}"))),
            }
        });
    });

    let prose_editor_id_for_input = prose_editor_id.clone();
    let on_prose_input = use_callback(move |_| {
        prose_apply_shortcuts(&prose_editor_id_for_input);
        let markdown = prose_get_markdown(&prose_editor_id_for_input);
        on_content_change.call(markdown);
    });

    let current_path = path();
    let current_content = content();
    let current_readonly = readonly();
    let current_save_state = save_state();
    let is_loading = loading();
    let current_run_status = run_status();
    let current_version_source = selected_version_source();
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

    let current_changesets = {
        let runs = ACTIVE_WRITER_RUNS.read();
        runs.get(&current_path)
            .map(|r| r.recent_changesets.clone())
            .unwrap_or_default()
    };

    let mut current_notes = Vec::<MarginNote>::new();
    for changeset in &current_changesets {
        current_notes.push(MarginNote {
            id: format!("changeset-{}", changeset.patch_id),
            impact: Some(changeset.impact.clone()),
            title: "Changeset".to_string(),
            lines: vec![changeset.summary.clone()],
            overlay_id: None,
        });
    }
    for overlay in &current_selected_overlays {
        let mut lines = overlay_note_lines(overlay);
        for line in &mut lines {
            *line = line.trim_start_matches('>').trim_start().to_string();
        }
        current_notes.push(MarginNote {
            id: format!("overlay-{}", overlay.overlay_id),
            impact: None,
            title: format!("{} suggestion", overlay.author),
            lines,
            overlay_id: Some(overlay.overlay_id.clone()),
        });
    }

    let bubble_tops = prose_compute_bubble_tops(&prose_editor_id, current_notes.len());

    rsx! {
        style { {WRITER_STYLES} }
        div {
            style: "display: flex; flex-direction: column; height: 100%; background: var(--window-bg); color: var(--text-primary); overflow: hidden;",

            if writer_view_mode() == WriterViewMode::Overview {
                div {
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 0.3rem 0.75rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); flex-shrink: 0;",
                    span { style: "font-size: 0.8rem; color: var(--text-secondary); font-weight: 500;", "Recent Documents" }
                    button {
                        class: "writer-toolbar-btn",
                        onclick: move |_| {
                            overview_loaded.set(false);
                            writer_view_mode.set(WriterViewMode::Overview);
                        },
                        "Refresh"
                    }
                }

                div {
                    class: "writer-overview-grid",
                    {
                        let entries = overview_entries.read();
                        if entries.is_empty() {
                            rsx! {
                                div {
                                    style: "grid-column: 1 / -1; padding: 2rem; text-align: center; color: var(--text-secondary); font-size: 0.85rem;",
                                    "No recent runs found."
                                }
                            }
                        } else {
                            rsx! {
                                for run in entries.iter().cloned() {
                                    {
                                        let doc_path = if run.document_path.is_empty() {
                                            format!("conductor/runs/{}/draft.md", run.run_id)
                                        } else {
                                            run.document_path.clone()
                                        };
                                        let doc_path_click = doc_path.clone();
                                        let run_id = run.run_id.clone();

                                        let (status_text, status_class, is_active) = {
                                            let runs = ACTIVE_WRITER_RUNS.read();
                                            if let Some(live) = runs.get(&doc_path) {
                                                let (t, c) = match live.status {
                                                    WriterRunStatusKind::Initializing => ("init", "writer-status--initializing"),
                                                    WriterRunStatusKind::Running => ("running", "writer-status--running"),
                                                    WriterRunStatusKind::WaitingForWorker => ("waiting", "writer-status--waiting"),
                                                    WriterRunStatusKind::Completing => ("completing", "writer-status--completing"),
                                                    WriterRunStatusKind::Completed => ("done", "writer-status--completed"),
                                                    WriterRunStatusKind::Failed => ("failed", "writer-status--failed"),
                                                    WriterRunStatusKind::Blocked => ("blocked", "writer-status--blocked"),
                                                };
                                                (t, c, !is_terminal_status(live.status))
                                            } else {
                                                let (t, c) = match run.status {
                                                    ConductorRunStatus::Initializing => ("init", "writer-status--initializing"),
                                                    ConductorRunStatus::Running => ("running", "writer-status--running"),
                                                    ConductorRunStatus::WaitingForCalls => ("waiting", "writer-status--waiting"),
                                                    ConductorRunStatus::Completing => ("completing", "writer-status--completing"),
                                                    ConductorRunStatus::Completed => ("done", "writer-status--completed"),
                                                    ConductorRunStatus::Failed => ("failed", "writer-status--failed"),
                                                    ConductorRunStatus::Blocked => ("blocked", "writer-status--blocked"),
                                                };
                                                (t, c, false)
                                            }
                                        };

                                        let date_display = run.created_at.format("%Y-%m-%d %H:%M").to_string();
                                        let title = if run.objective.len() > 80 {
                                            format!("{}...", &run.objective[..80])
                                        } else {
                                            run.objective.clone()
                                        };

                                        rsx! {
                                            div {
                                                key: "{run_id}",
                                                class: "writer-doc-card",
                                                onclick: move |_| {
                                                    path.set(doc_path_click.clone());
                                                    loaded_path.set(None);
                                                    writer_view_mode.set(WriterViewMode::Editor);
                                                },
                                                div { class: "writer-doc-card-title", "{title}" }
                                                div { class: "writer-doc-card-footer",
                                                    span { class: "writer-doc-card-meta", "{date_display}" }
                                                    span {
                                                        class: "writer-status-chip {status_class}",
                                                        style: "font-size: 0.65rem;",
                                                        if is_active {
                                                            span { style: "animation: spin 1s linear infinite; display: inline-block;", "◐" }
                                                        }
                                                        "{status_text}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                {render_dialog(
                    dialog,
                    path,
                    content,
                    revision,
                    mime,
                    readonly,
                    save_state,
                    loading,
                    loaded_path,
                )}
            } else {
                div {
                    class: "writer-toolbar",

                    div {
                        class: "writer-toolbar-left",
                        button {
                            class: "writer-back-btn",
                            onclick: move |_| {
                                overview_loaded.set(false);
                                writer_view_mode.set(WriterViewMode::Overview);
                            },
                            "← Docs"
                        }
                        span { class: "writer-path-label", "{current_path}" }
                        if current_readonly {
                            span { class: "writer-readonly-badge", "Read-only" }
                        }
                        if let Some(status) = current_run_status {
                            {
                                let (status_text, status_class) = match status {
                                    WriterRunStatusKind::Initializing => ("Init...", "writer-status--initializing"),
                                    WriterRunStatusKind::Running => ("Running", "writer-status--running"),
                                    WriterRunStatusKind::WaitingForWorker => ("Waiting", "writer-status--waiting"),
                                    WriterRunStatusKind::Completing => ("Completing", "writer-status--completing"),
                                    WriterRunStatusKind::Completed => ("Done", "writer-status--completed"),
                                    WriterRunStatusKind::Failed => ("Failed", "writer-status--failed"),
                                    WriterRunStatusKind::Blocked => ("Blocked", "writer-status--blocked"),
                                };
                                rsx! {
                                    span {
                                        class: "writer-status-chip {status_class}",
                                        if !is_terminal_status(status) {
                                            span { style: "animation: spin 1s linear infinite; display: inline-block;", "◐" }
                                        }
                                        "{status_text}"
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "writer-toolbar-center",
                        if !current_version_ids.is_empty() {
                            button {
                                class: "writer-toolbar-btn",
                                disabled: !can_go_prev,
                                onclick: move |_| handle_prev_version.call(()),
                                "<"
                            }
                            span {
                                style: "font-size: 0.75rem; color: var(--text-secondary); min-width: 72px; text-align: center;",
                                {
                                    let selected = current_selected_version_index
                                        .map(|idx| idx + 1)
                                        .unwrap_or(current_version_ids.len());
                                    let total = current_version_ids.len();
                                    format!("v{} of {}", selected, total)
                                }
                            }
                            if !current_version_source.is_empty() {
                                {
                                    let provenance_class = match current_version_source.as_str() {
                                        "writer" => "writer-provenance--ai",
                                        "user_save" => "writer-provenance--user",
                                        _ => "writer-provenance--system",
                                    };
                                    let provenance_label = match current_version_source.as_str() {
                                        "writer" => "AI",
                                        "user_save" => "User",
                                        _ => "Sys",
                                    };
                                    rsx! {
                                        span { class: "writer-provenance-badge {provenance_class}",
                                            "{provenance_label}"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "writer-toolbar-btn",
                                disabled: !can_go_next,
                                onclick: move |_| handle_next_version.call(()),
                                ">"
                            }
                        }
                    }

                    div { class: "writer-toolbar-spacer" }

                    div {
                        class: "writer-toolbar-secondary",
                        button {
                            class: "writer-toolbar-btn",
                            onclick: move |_| show_open_dialog.call(()),
                            "Open..."
                        }
                        button {
                            class: "writer-toolbar-btn",
                            onclick: move |_| show_save_as_dialog.call(()),
                            "Save As..."
                        }
                        {render_save_status(&current_save_state, dismiss_saved.clone())}
                    }

                    div {
                        class: "writer-toolbar-right",
                        button {
                            class: "writer-toolbar-btn-accent",
                            disabled: current_prompt_submitting || current_readonly || !has_prompt_diff,
                            onclick: move |_| handle_prompt_submit.call(()),
                            if current_prompt_submitting { "..." } else { "Prompt" }
                        }
                        button {
                            class: match current_save_state {
                                SaveState::Clean | SaveState::Saving | SaveState::Saved => "writer-toolbar-btn",
                                _ => "writer-toolbar-btn-accent",
                            },
                            disabled: matches!(current_save_state, SaveState::Clean | SaveState::Saving | SaveState::Saved) || current_readonly,
                            onclick: move |_| handle_save.call(()),
                            if matches!(current_save_state, SaveState::Saving) { "Saving..." } else { "Save" }
                        }
                    }
                }

                if is_loading {
                    div {
                        style: "padding: 0.5rem 1rem; background: var(--accent-bg); color: var(--accent-text); font-size: 0.875rem;",
                        "Loading document..."
                    }
                }

                if current_new_version_available {
                    div { class: "writer-new-version-banner",
                        span { "New version available while you were editing." }
                        button {
                            style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-primary); cursor: pointer; padding: 0.25rem 0.6rem; border-radius: 0.3rem; font-size: 0.8rem;",
                            onclick: move |_| handle_load_latest.call(()),
                            "Load Latest"
                        }
                    }
                }

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

                div {
                    class: "writer-layout",

                    div {
                        class: "writer-margin writer-margin-left",
                        if current_notes.is_empty() {
                            div { class: "writer-margin-empty", "No active marginalia." }
                        } else {
                            for note in current_notes.iter().cloned() {
                                div {
                                    key: "{note.id}",
                                    class: "writer-margin-card",
                                    if let Some(impact) = note.impact {
                                        {
                                            let impact_class = match impact {
                                                ChangesetImpact::High => "writer-impact--high",
                                                ChangesetImpact::Medium => "writer-impact--medium",
                                                ChangesetImpact::Low => "writer-impact--low",
                                            };
                                            let impact_label = match impact {
                                                ChangesetImpact::High => "HIGH",
                                                ChangesetImpact::Medium => "MED",
                                                ChangesetImpact::Low => "LOW",
                                            };
                                            rsx! {
                                                span { class: "writer-impact-badge {impact_class}", "{impact_label}" }
                                            }
                                        }
                                    }
                                    div { style: "font-weight: 600; margin-bottom: 0.2rem;", "{note.title}" }
                                    for line in note.lines.iter().take(4) {
                                        div { style: "line-height: 1.35;", "{line}" }
                                    }
                                    if let Some(overlay_id) = note.overlay_id.clone() {
                                        div { class: "writer-margin-card-actions",
                                            button {
                                                class: "writer-margin-card-btn",
                                                onclick: {
                                                    let overlay_id = overlay_id.clone();
                                                    move |_| handle_accept_overlay.call(overlay_id.clone())
                                                },
                                                "Accept"
                                            }
                                            button {
                                                class: "writer-margin-card-btn",
                                                onclick: {
                                                    let overlay_id = overlay_id.clone();
                                                    move |_| handle_dismiss_overlay.call(overlay_id.clone())
                                                },
                                                "Dismiss"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "writer-prose-column",
                        button {
                            class: "writer-note-toggle",
                            onclick: move |_| right_margin_open.set(!right_margin_open()),
                            "Notes {current_notes.len()}"
                        }
                        div {
                            class: "writer-prose-container",
                            div {
                                id: "{prose_editor_id}",
                                class: "writer-prose-body",
                                contenteditable: if current_readonly { "false" } else { "true" },
                                spellcheck: "true",
                                oninput: move |_| on_prose_input.call(()),
                                onkeydown: move |e: KeyboardEvent| on_keydown.call(e),
                            }

                            for (idx, note) in current_notes.iter().enumerate() {
                                button {
                                    key: "bubble-{note.id}",
                                    class: "writer-bubble",
                                    style: {
                                        let top = bubble_tops
                                            .get(idx)
                                            .copied()
                                            .unwrap_or(((idx + 1) * 28) as i32);
                                        format!("top: {}px;", top)
                                    },
                                    onclick: {
                                        let idx = idx;
                                        move |_| mobile_sheet_note_idx.set(Some(idx))
                                    },
                                    aria_label: "Note",
                                    "◉"
                                }
                            }
                        }
                    }

                    div {
                        class: if right_margin_open() {
                            "writer-margin writer-margin-right is-open"
                        } else {
                            "writer-margin writer-margin-right"
                        },
                        div { class: "writer-margin-empty", "User notes are coming in Phase 8." }
                    }
                }

                if let Some(note_idx) = mobile_sheet_note_idx() {
                    {
                        let note = current_notes.get(note_idx).cloned();
                        rsx! {
                            if let Some(note) = note {
                                div {
                                    class: "writer-bottom-sheet-backdrop",
                                    onclick: move |_| mobile_sheet_note_idx.set(None),
                                }
                                div {
                                    class: "writer-bottom-sheet",
                                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;",
                                        strong { "{note.title}" }
                                        button {
                                            class: "writer-margin-card-btn",
                                            onclick: move |_| mobile_sheet_note_idx.set(None),
                                            "Close"
                                        }
                                    }
                                    for line in note.lines {
                                        p { style: "margin: 0.25rem 0; font-size: 0.82rem;", "{line}" }
                                    }
                                    if let Some(overlay_id) = note.overlay_id {
                                        div { class: "writer-margin-card-actions",
                                            button {
                                                class: "writer-margin-card-btn",
                                                onclick: {
                                                    let overlay_id = overlay_id.clone();
                                                    move |_| handle_accept_overlay.call(overlay_id.clone())
                                                },
                                                "Accept"
                                            }
                                            button {
                                                class: "writer-margin-card-btn",
                                                onclick: {
                                                    let overlay_id = overlay_id.clone();
                                                    move |_| handle_dismiss_overlay.call(overlay_id.clone())
                                                },
                                                "Dismiss"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                {render_dialog(
                    dialog,
                    path,
                    content,
                    revision,
                    mime,
                    readonly,
                    save_state,
                    loading,
                    loaded_path,
                )}
            }
        }
    }
}
