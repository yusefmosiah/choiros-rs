use dioxus::prelude::*;

use crate::api::files_api::{
    create_directory, create_file, delete_file, list_directory, read_file_content, rename_file,
    DirectoryEntry,
};

/// View mode for the Files app
#[derive(Debug, Clone, PartialEq)]
enum ViewMode {
    Browser,
    Editor { path: String, content: String },
}

/// Dialog state
#[derive(Debug, Clone, PartialEq)]
enum DialogState {
    None,
    CreateFolder,
    CreateFile,
    Rename { path: String, name: String },
    Delete { path: String, name: String, is_dir: bool },
}

#[component]
pub fn FilesView(
    desktop_id: String,
    window_id: String,
    #[props(default)] initial_path: String,
) -> Element {
    let _ = (&desktop_id, &window_id);
    let mut current_path = use_signal(|| initial_path.clone());
    let mut entries = use_signal(|| Vec::new());
    let mut selected_entry = use_signal(|| None::<DirectoryEntry>);
    let mut view_mode = use_signal(|| ViewMode::Browser);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut dialog = use_signal(|| DialogState::None);
    let mut dialog_input = use_signal(|| String::new());
    let mut initial_load_done = use_signal(|| false);

    // Clone for effects
    let desktop_id_for_effect = desktop_id.clone();
    let window_id_for_effect = window_id.clone();

    // Load directory contents
    let load_directory = use_callback(move |path: String| {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match list_directory(&path).await {
                Ok(response) => {
                    entries.set(response.entries);
                    current_path.set(response.path);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Initial load - only run once
    use_effect(move || {
        if initial_load_done() {
            return;
        }
        initial_load_done.set(true);
        let path = current_path();
        load_directory.call(path);
    });

    // Persist path to window props when it changes
    use_effect(move || {
        let path = current_path();
        // Only persist after initial load to avoid overwriting on first render
        if !initial_load_done() {
            return;
        }
        let desktop_id = desktop_id_for_effect.clone();
        let window_id = window_id_for_effect.clone();
        spawn(async move {
            // Use the desktop API to update window props
            let _ = persist_files_path(&desktop_id, &window_id, &path).await;
        });
    });

    // Refresh current directory
    let refresh = move |_| {
        let path = current_path();
        load_directory.call(path);
    };

    // Navigate up to parent directory
    let navigate_up = move |_| {
        let current = current_path();
        if current.is_empty() {
            return;
        }
        let parent = current
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
        load_directory.call(parent);
        selected_entry.set(None);
    };

    // Navigate to a directory
    let mut navigate_to = move |path: String| {
        load_directory.call(path);
        selected_entry.set(None);
    };

    // Open a file for editing
    let open_file = move |path: String| {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match read_file_content(&path).await {
                Ok(response) => {
                    view_mode.set(ViewMode::Editor {
                        path: response.path,
                        content: response.content,
                    });
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    // Handle entry click (select)
    let mut select_entry = move |entry: DirectoryEntry| {
        selected_entry.set(Some(entry));
    };

    // Handle entry double-click (open/navigate)
    let mut activate_entry = move |entry: DirectoryEntry| {
        if entry.is_dir {
            navigate_to(entry.path);
        } else {
            open_file(entry.path);
        }
    };

    // Go back to browser from editor
    let back_to_browser = move |_| {
        view_mode.set(ViewMode::Browser);
        selected_entry.set(None);
    };

    // Breadcrumb navigation
    let breadcrumb_navigate = move |index: usize| {
        let path = current_path();
        if path.is_empty() || index == 0 {
            load_directory.call("".to_string());
            return;
        }
        let parts: Vec<&str> = path.split('/').collect();
        let new_path = parts[..index].join("/");
        load_directory.call(new_path);
    };

    // Create folder dialog
    let show_create_folder = move |_| {
        dialog.set(DialogState::CreateFolder);
        dialog_input.set(String::new());
    };

    // Create file dialog
    let show_create_file = move |_| {
        dialog.set(DialogState::CreateFile);
        dialog_input.set(String::new());
    };

    // Rename dialog
    let mut show_rename = move |entry: DirectoryEntry| {
        dialog_input.set(entry.name.clone());
        dialog.set(DialogState::Rename {
            path: entry.path,
            name: entry.name,
        });
    };

    // Delete dialog
    let mut show_delete = move |entry: DirectoryEntry| {
        dialog.set(DialogState::Delete {
            path: entry.path.clone(),
            name: entry.name.clone(),
            is_dir: entry.is_dir,
        });
    };

    // Confirm create folder
    let confirm_create_folder = move |_| {
        let name = dialog_input();
        if name.is_empty() {
            return;
        }
        let path = if current_path().is_empty() {
            name.clone()
        } else {
            format!("{}/{}", current_path(), name)
        };
        let current = current_path();
        spawn(async move {
            match create_directory(&path).await {
                Ok(_) => {
                    dialog.set(DialogState::None);
                    load_directory.call(current);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Confirm create file
    let confirm_create_file = move |_| {
        let name = dialog_input();
        if name.is_empty() {
            return;
        }
        let path = if current_path().is_empty() {
            name.clone()
        } else {
            format!("{}/{}", current_path(), name)
        };
        let current = current_path();
        spawn(async move {
            match create_file(&path, None).await {
                Ok(_) => {
                    dialog.set(DialogState::None);
                    load_directory.call(current);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Confirm rename
    let confirm_rename = move |old_path: String| {
        let new_name = dialog_input();
        if new_name.is_empty() {
            return;
        }
        let new_path = if current_path().is_empty() {
            new_name.clone()
        } else {
            format!("{}/{}", current_path(), new_name)
        };
        let current = current_path();
        spawn(async move {
            match rename_file(&old_path, &new_path).await {
                Ok(_) => {
                    dialog.set(DialogState::None);
                    load_directory.call(current);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Confirm delete
    let confirm_delete = move |path: String, is_dir: bool| {
        let current = current_path();
        spawn(async move {
            match delete_file(&path, is_dir).await {
                Ok(_) => {
                    dialog.set(DialogState::None);
                    selected_entry.set(None);
                    load_directory.call(current);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Cancel dialog
    let cancel_dialog = move |_| {
        dialog.set(DialogState::None);
        dialog_input.set(String::new());
    };

    // Render breadcrumb
    let render_breadcrumb = move || {
        let path = current_path();
        let parts: Vec<&str> = if path.is_empty() {
            vec![]
        } else {
            path.split('/').collect()
        };

        rsx! {
            div {
                style: "display: flex; align-items: center; gap: 0.25rem; font-size: 0.875rem; color: var(--text-secondary, #94a3b8);",
                button {
                    style: "background: transparent; border: none; color: var(--accent-bg, #3b82f6); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.25rem;",
                    onclick: move |_| breadcrumb_navigate(0),
                    "Home"
                }
                for (i, part) in parts.iter().enumerate() {
                    span { ">" }
                    button {
                        style: "background: transparent; border: none; color: var(--accent-bg, #3b82f6); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.25rem;",
                        onclick: move |_| breadcrumb_navigate(i + 1),
                        "{part}"
                    }
                }
            }
        }
    };

    // Format file size
    let format_size = |size: u64| -> String {
        if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        }
    };

    // Get icon for file type
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

    // Main render
    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; background: var(--window-bg, #1f2937); color: var(--text-primary, #f8fafc); overflow: hidden;",

            // Toolbar
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); flex-shrink: 0;",

                // Left: Navigation
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        disabled: current_path().is_empty(),
                        onclick: navigate_up,
                        "↑ Up"
                    }
                    {render_breadcrumb()}
                }

                // Right: Actions
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: refresh,
                        if loading() {
                            "⟳ Refresh"
                        } else {
                            "🔄 Refresh"
                        }
                    }
                    button {
                        style: "background: var(--accent-bg, #3b82f6); border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: show_create_folder,
                        "+ Folder"
                    }
                    button {
                        style: "background: var(--accent-bg, #3b82f6); border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: show_create_file,
                        "+ File"
                    }
                }
            }

            // Error banner
            if let Some(err) = error() {
                div {
                    style: "padding: 0.75rem 1rem; background: #7f1d1d; color: #fecaca; font-size: 0.875rem; border-bottom: 1px solid #991b1b;",
                    "Error: {err}"
                }
            }

            // Main content
            match view_mode() {
                ViewMode::Browser => rsx! {
                    div {
                        style: "flex: 1; overflow: auto; padding: 0.5rem;",

                        // File list header
                        div {
                            style: "display: grid; grid-template-columns: 2rem 1fr 6rem 8rem; gap: 0.75rem; padding: 0.5rem 0.75rem; font-size: 0.75rem; font-weight: 600; color: var(--text-secondary, #94a3b8); border-bottom: 1px solid var(--border-color, #374151); position: sticky; top: 0; background: var(--window-bg, #1f2937);",
                            div { "" }
                            div { "Name" }
                            div { "Size" }
                            div { "Modified" }
                        }

                        // File list
                        if entries().is_empty() {
                            div {
                                style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem; color: var(--text-muted, #6b7280);",
                                div { style: "font-size: 3rem; margin-bottom: 1rem;", "📂" }
                                "This folder is empty"
                            }
                        } else {
                            for entry in entries() {
                                div {
                                    key: "{entry.path}",
                                    style: if selected_entry().as_ref().map(|e| e.path == entry.path).unwrap_or(false) {
                                        "display: grid; grid-template-columns: 2rem 1fr 6rem 8rem; gap: 0.75rem; padding: 0.5rem 0.75rem; font-size: 0.875rem; cursor: pointer; background: var(--accent-bg, #3b82f6); color: white; border-radius: 0.375rem; margin-bottom: 0.125rem;"
                                    } else {
                                        "display: grid; grid-template-columns: 2rem 1fr 6rem 8rem; gap: 0.75rem; padding: 0.5rem 0.75rem; font-size: 0.875rem; cursor: pointer; hover:background: var(--bg-secondary, #374151); border-radius: 0.375rem; margin-bottom: 0.125rem;"
                                    },
                                    onclick: {
                                        let entry_clone = entry.clone();
                                        move |_| select_entry(entry_clone.clone())
                                    },
                                    ondoubleclick: {
                                        let entry_clone = entry.clone();
                                        move |_| activate_entry(entry_clone.clone())
                                    },
                                    div { "{get_file_icon(&entry)}" }
                                    div { "{entry.name}" }
                                    div { style: "color: var(--text-secondary, #94a3b8);",
                                        if entry.is_dir { "—" } else { "{format_size(entry.size)}" }
                                    }
                                    div { style: "color: var(--text-secondary, #94a3b8); font-size: 0.75rem;",
                                        {format_timestamp(&entry.modified_at)}
                                    }
                                }
                            }
                        }
                    }

                    // Context panel for selected item
                    if let Some(entry) = selected_entry() {
                        div {
                            style: "padding: 0.75rem 1rem; background: var(--titlebar-bg, #111827); border-top: 1px solid var(--border-color, #374151); display: flex; align-items: center; justify-content: space-between; flex-shrink: 0;",
                            div {
                                style: "display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem;",
                                "{get_file_icon(&entry)}"
                                span { style: "font-weight: 500;", "{entry.name}" }
                                if entry.is_dir {
                                    span { style: "color: var(--text-secondary, #94a3b8);", "(folder)" }
                                } else {
                                    span { style: "color: var(--text-secondary, #94a3b8);", "({format_size(entry.size)})" }
                                }
                            }
                            div {
                                style: "display: flex; align-items: center; gap: 0.5rem;",
                                button {
                                    style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                                    onclick: {
                                        let entry_clone = entry.clone();
                                        move |_| show_rename(entry_clone.clone())
                                    },
                                    "Rename"
                                }
                                button {
                                    style: "background: transparent; border: 1px solid #991b1b; color: #fca5a5; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                                    onclick: {
                                        let entry_clone = entry.clone();
                                        move |_| show_delete(entry_clone.clone())
                                    },
                                    "Delete"
                                }
                                if entry.is_file {
                                    button {
                                        style: "background: var(--accent-bg, #3b82f6); border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                                        onclick: {
                                            let path = entry.path.clone();
                                            move |_| open_file(path.clone())
                                        },
                                        "Open"
                                    }
                                }
                            }
                        }
                    }
                },
                ViewMode::Editor { path, content } => rsx! {
                    div {
                        style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

                        // Editor toolbar
                        div {
                            style: "display: flex; align-items: center; justify-content: space-between; padding: 0.5rem 1rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); flex-shrink: 0;",
                            div {
                                style: "display: flex; align-items: center; gap: 0.5rem;",
                                button {
                                    style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                                    onclick: back_to_browser,
                                    "← Back"
                                }
                                span { style: "font-size: 0.875rem; color: var(--text-secondary, #94a3b8);", "{path}" }
                            }
                        }

                        // Editor content
                        div {
                            style: "flex: 1; overflow: auto; padding: 1rem;",
                            pre {
                                style: "margin: 0; white-space: pre-wrap; word-break: break-word; font-family: ui-monospace, monospace; font-size: 0.875rem; line-height: 1.5; color: var(--text-primary, #f8fafc);",
                                "{content}"
                            }
                        }
                    }
                }
            }

            // Dialog overlay
            match dialog() {
                DialogState::None => rsx! {},
                DialogState::CreateFolder => rsx! {
                    DialogOverlay {
                        title: "Create Folder",
                        input_value: dialog_input(),
                        on_input: move |v: String| dialog_input.set(v),
                        on_confirm: confirm_create_folder,
                        on_cancel: cancel_dialog,
                        confirm_text: "Create",
                        placeholder: "Folder name",
                    }
                },
                DialogState::CreateFile => rsx! {
                    DialogOverlay {
                        title: "Create File",
                        input_value: dialog_input(),
                        on_input: move |v: String| dialog_input.set(v),
                        on_confirm: confirm_create_file,
                        on_cancel: cancel_dialog,
                        confirm_text: "Create",
                        placeholder: "File name",
                    }
                },
                DialogState::Rename { path, .. } => rsx! {
                    DialogOverlay {
                        title: "Rename",
                        input_value: dialog_input(),
                        on_input: move |v: String| dialog_input.set(v),
                        on_confirm: move |_| confirm_rename(path.clone()),
                        on_cancel: cancel_dialog,
                        confirm_text: "Rename",
                        placeholder: "New name",
                    }
                },
                DialogState::Delete { path, name, is_dir } => rsx! {
                    ConfirmDialog {
                        title: if is_dir { "Delete Folder" } else { "Delete File" },
                        message: format!("Are you sure you want to delete '{}'?", name),
                        on_confirm: move |_| confirm_delete(path.clone(), is_dir),
                        on_cancel: cancel_dialog,
                        confirm_text: "Delete",
                        is_dangerous: true,
                    }
                },
            }
        }
    }
}

/// Dialog overlay component
#[component]
fn DialogOverlay(
    title: String,
    input_value: String,
    on_input: Callback<String>,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
    confirm_text: String,
    placeholder: String,
) -> Element {
    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0, 0, 0, 0.7); display: flex; align-items: center; justify-content: center; z-index: 1000;",
            onclick: move |_| on_cancel.call(()),
            div {
                style: "background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: 0.5rem; padding: 1.5rem; min-width: 320px; max-width: 90vw;",
                onclick: move |e| e.stop_propagation(),
                h3 { style: "margin: 0 0 1rem 0; font-size: 1.125rem;", "{title}" }
                input {
                    style: "width: 100%; padding: 0.5rem 0.75rem; background: var(--input-bg, #0f172a); color: var(--text-primary, #f8fafc); border: 1px solid var(--border-color, #374151); border-radius: 0.375rem; font-size: 0.875rem; box-sizing: border-box;",
                    value: "{input_value}",
                    placeholder: "{placeholder}",
                    oninput: move |e| on_input.call(e.value()),
                    autofocus: true,
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            on_confirm.call(());
                        } else if e.key() == Key::Escape {
                            on_cancel.call(());
                        }
                    }
                }
                div {
                    style: "display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 1rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        style: "background: var(--accent-bg, #3b82f6); border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_text}"
                    }
                }
            }
        }
    }
}

/// Confirmation dialog component
#[component]
fn ConfirmDialog(
    title: String,
    message: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
    confirm_text: String,
    is_dangerous: bool,
) -> Element {
    let confirm_style = if is_dangerous {
        "background: #dc2626; border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
    } else {
        "background: var(--accent-bg, #3b82f6); border: none; color: white; cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
    };

    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0, 0, 0, 0.7); display: flex; align-items: center; justify-content: center; z-index: 1000;",
            onclick: move |_| on_cancel.call(()),
            div {
                style: "background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: 0.5rem; padding: 1.5rem; min-width: 320px; max-width: 90vw;",
                onclick: move |e| e.stop_propagation(),
                h3 { style: "margin: 0 0 0.5rem 0; font-size: 1.125rem;", "{title}" }
                p { style: "margin: 0 0 1rem 0; font-size: 0.875rem; color: var(--text-secondary, #94a3b8);", "{message}" }
                div {
                    style: "display: flex; justify-content: flex-end; gap: 0.5rem;",
                    button {
                        style: "background: transparent; border: 1px solid var(--border-color, #374151); color: var(--text-secondary, #94a3b8); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        style: "{confirm_style}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_text}"
                    }
                }
            }
        }
    }
}

/// Format ISO timestamp to readable format
fn format_timestamp(iso: &str) -> String {
    // Try to parse and format the timestamp
    if let Some(date_part) = iso.split('T').next() {
        date_part.to_string()
    } else {
        iso.to_string()
    }
}

/// Persist the current files path to window props
async fn persist_files_path(desktop_id: &str, window_id: &str, path: &str) -> Result<(), String> {
    // For now, use localStorage as a simple persistence mechanism
    // This could be upgraded to use a proper window state API in the future
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("LocalStorage not available")?;

    let key = format!("choiros.files.path.{}.{}", desktop_id, window_id);
    storage
        .set_item(&key, path)
        .map_err(|_| "Failed to set item in localStorage")?;

    Ok(())
}

/// Load the persisted files path from window props
pub fn load_files_path(desktop_id: &str, window_id: &str) -> String {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return String::new();
    };

    let key = format!("choiros.files.path.{}.{}", desktop_id, window_id);
    storage.get_item(&key).ok().flatten().unwrap_or_default()
}