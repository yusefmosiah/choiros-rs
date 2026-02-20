//! Main WriterView component

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::files_api::list_directory;
use crate::api::{
    writer_open, writer_prompt, writer_save, writer_save_version, writer_version, writer_versions,
    WriterOverlay,
};
use crate::desktop::state::ACTIVE_WRITER_RUNS;
use shared_types::{ChangesetImpact, PatchOp, WriterRunStatusKind};

use super::dialogs::render_dialog;
use super::logic::*;
use super::styles::*;
use super::types::*;

#[component]
pub fn WriterView(desktop_id: String, window_id: String, initial_path: String) -> Element {
    let _ = (&desktop_id, &window_id);

    // Top-level navigation mode
    let mut writer_view_mode = use_signal(|| {
        if initial_path.is_empty() {
            WriterViewMode::Overview
        } else {
            WriterViewMode::Editor
        }
    });

    // Overview state
    let mut overview_entries = use_signal(Vec::<crate::api::files_api::DirectoryEntry>::new);
    let mut overview_loaded = use_signal(|| false);

    // Load overview entries when in Overview mode
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
                match list_directory("").await {
                    Ok(response) => {
                        let filtered: Vec<_> = response
                            .entries
                            .into_iter()
                            .filter(|e| {
                                e.is_dir || (e.is_file && e.name.ends_with(".md"))
                            })
                            .collect();
                        overview_entries.set(filtered);
                    }
                    Err(_) => {
                        overview_entries.set(Vec::new());
                    }
                }
            });
        });
    }

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
    let mut selected_version_source = use_signal(|| String::new()); // VersionSource provenance
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
        let loading_for_effect = loading;
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
            // Don't process run events until the document has finished loading.
            // Effects fire immediately on mount; if ACTIVE_WRITER_RUNS already has a
            // run at revision N while the document open call is still in-flight, the
            // revision gap check would falsely fire (0 -> N) before we know the real
            // document revision.
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

                // Check for new run starting
                if active_run_id_for_effect.read().as_ref() != Some(&run_id) {
                    active_run_id_for_effect.set(Some(run_id.clone()));
                    // Use the higher of: the revision we loaded the document at, or
                    // the run's tracked last_applied_revision. This prevents the component
                    // from treating a freshly-opened document (e.g. revision 9) as a gap
                    // when the global run state still shows last_applied_revision=0 because
                    // progress events arrived before the document was opened.
                    let local_rev = last_applied_revision_for_effect();
                    if run_last_applied > local_rev {
                        last_applied_revision_for_effect.set(run_last_applied);
                    }
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
    let current_changesets = {
        let runs = ACTIVE_WRITER_RUNS.read();
        runs.get(&current_path)
            .map(|r| r.recent_changesets.clone())
            .unwrap_or_default()
    };
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
    let current_editor_text = compose_editor_text(&current_content, &current_selected_overlays);

    rsx! {
        style { {WRITER_STYLES} }
        div {
            style: "display: flex; flex-direction: column; height: 100%; background: var(--window-bg); color: var(--text-primary); overflow: hidden;",

            // Overview mode: document list grid
            if writer_view_mode() == WriterViewMode::Overview {
                // Overview toolbar
                div {
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); flex-shrink: 0;",
                    span { style: "font-size: 0.875rem; color: var(--text-secondary); font-weight: 500;", "Documents" }
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| show_save_as_dialog.call(()),
                        "New Document"
                    }
                }

                // Overview grid
                div {
                    class: "writer-overview-grid",
                    {
                        // Cards from ACTIVE_WRITER_RUNS (paths not already in directory listing)
                        let active_paths: Vec<String> = {
                            let runs = ACTIVE_WRITER_RUNS.read();
                            runs.keys().cloned().collect()
                        };
                        let dir_paths: std::collections::HashSet<String> = overview_entries
                            .read()
                            .iter()
                            .map(|e| e.path.clone())
                            .collect();
                        let extra_paths: Vec<String> = active_paths
                            .into_iter()
                            .filter(|p| !dir_paths.contains(p))
                            .collect();

                        rsx! {
                            // Cards from directory listing
                            for entry in overview_entries.read().iter().cloned() {
                                {
                                    let entry_path = entry.path.clone();
                                    let entry_name = entry.name.clone();
                                    let run_status: Option<String> = {
                                        let runs = ACTIVE_WRITER_RUNS.read();
                                        runs.get(&entry_path).map(|r| format!("{:?}", r.status).to_lowercase())
                                    };
                                    rsx! {
                                        div {
                                            class: "writer-doc-card",
                                            onclick: {
                                                let ep = entry_path.clone();
                                                move |_| {
                                                    path.set(ep.clone());
                                                    loaded_path.set(None);
                                                    writer_view_mode.set(WriterViewMode::Editor);
                                                }
                                            },
                                            div { class: "writer-doc-card-title", "{entry_name}" }
                                            div { class: "writer-doc-card-path", "{entry_path}" }
                                            div { class: "writer-doc-card-footer",
                                                if let Some(status) = run_status {
                                                    span {
                                                        class: "writer-status-chip writer-status--running",
                                                        style: "font-size: 0.65rem;",
                                                        "{status}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Extra cards from ACTIVE_WRITER_RUNS not in directory
                            for ep in extra_paths {
                                {
                                    let ep2 = ep.clone();
                                    let display_name = ep.rsplit('/').next().unwrap_or(&ep).to_string();
                                    let run_status: Option<String> = {
                                        let runs = ACTIVE_WRITER_RUNS.read();
                                        runs.get(&ep).map(|r| format!("{:?}", r.status).to_lowercase())
                                    };
                                    rsx! {
                                        div {
                                            class: "writer-doc-card",
                                            onclick: move |_| {
                                                path.set(ep2.clone());
                                                loaded_path.set(None);
                                                writer_view_mode.set(WriterViewMode::Editor);
                                            },
                                            div { class: "writer-doc-card-title", "{display_name}" }
                                            div { class: "writer-doc-card-path", "{ep}" }
                                            div { class: "writer-doc-card-footer",
                                                if let Some(status) = run_status {
                                                    span {
                                                        class: "writer-status-chip writer-status--running",
                                                        style: "font-size: 0.65rem;",
                                                        "{status}"
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

                // Dialog overlays (Save As / New Document)
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
            } else {

            // ── Editor mode ──────────────────────────────────────────────────

            // Toolbar
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); flex-shrink: 0;",

                // Left: Back button + File info
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    button {
                        class: "writer-back-btn",
                        onclick: move |_| {
                            overview_loaded.set(false);
                            writer_view_mode.set(WriterViewMode::Overview);
                        },
                        "← Documents"
                    }
                    span { style: "font-size: 0.875rem; color: var(--text-secondary); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{current_path}"
                    }
                    if current_readonly {
                        span { class: "writer-readonly-badge",
                            "Read-only"
                        }
                    }
                    // Live run status indicator
                    if let Some(status) = current_run_status {
                        {
                            let (status_text, status_class) = match status {
                                WriterRunStatusKind::Initializing => ("Initializing...", "writer-status--initializing"),
                                WriterRunStatusKind::Running => ("Running...", "writer-status--running"),
                                WriterRunStatusKind::WaitingForWorker => ("Waiting...", "writer-status--waiting"),
                                WriterRunStatusKind::Completing => ("Completing...", "writer-status--completing"),
                                WriterRunStatusKind::Completed => ("Completed", "writer-status--completed"),
                                WriterRunStatusKind::Failed => ("Failed", "writer-status--failed"),
                                WriterRunStatusKind::Blocked => ("Blocked", "writer-status--blocked"),
                            };
                            rsx! {
                                span {
                                    class: "writer-status-chip {status_class}",
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
                        // Provenance badge: show VersionSource of the currently displayed version
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
                                    _ => "System",
                                };
                                rsx! {
                                    span { class: "writer-provenance-badge {provenance_class}",
                                        "{provenance_label}"
                                    }
                                }
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
                div { class: "writer-new-version-banner",
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

            // Changeset summary panel (1.4 patch stream live view + Marginalia v1 observation)
            if !current_changesets.is_empty() {
                div { class: "writer-changeset-panel",
                    for changeset in &current_changesets {
                        div {
                            key: "{changeset.patch_id}",
                            style: "display: flex; gap: 0.5rem; align-items: baseline; padding: 0.1rem 0;",
                            {
                                let impact_class = match changeset.impact {
                                    ChangesetImpact::High => "writer-impact--high",
                                    ChangesetImpact::Medium => "writer-impact--medium",
                                    ChangesetImpact::Low => "writer-impact--low",
                                };
                                let impact_label = match changeset.impact {
                                    ChangesetImpact::High => "HIGH",
                                    ChangesetImpact::Medium => "MED",
                                    ChangesetImpact::Low => "LOW",
                                };
                                rsx! {
                                    span { class: "writer-impact-badge {impact_class}",
                                        "{impact_label}"
                                    }
                                }
                            }
                            span { style: "flex: 1; line-height: 1.4;", "{changeset.summary}" }
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
                            style: "flex: 1; width: calc(100% - 1rem); height: 100%; padding: 1rem; background: var(--input-bg, var(--window-bg)); color: var(--text-primary); border: 1px solid var(--border-color); resize: none; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.875rem; line-height: 1.6; font-kerning: none; font-variant-ligatures: none; text-rendering: optimizeSpeed; outline: none; border-radius: 0.25rem; margin: 0.5rem; overflow-y: scroll; scrollbar-gutter: stable;",
                            value: "{current_editor_text}",
                            wrap: "soft",
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

            } // end else (Editor mode)
        }
    }
}
