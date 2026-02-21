//! Writer dialog functions and components

use super::types::{DialogState, SaveState};
use crate::api::files_api::{list_directory, DirectoryEntry};
use crate::api::{writer_open, writer_save};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

/// Render dialog overlays
pub fn render_dialog(
    mut dialog: Signal<DialogState>,
    mut path: Signal<String>,
    mut content: Signal<String>,
    mut revision: Signal<u64>,
    mut mime: Signal<String>,
    mut readonly: Signal<bool>,
    mut save_state: Signal<SaveState>,
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
                                path.set(response.path);
                                content.set(response.content);
                                revision.set(response.revision);
                                mime.set(response.mime);
                                readonly.set(response.readonly);
                                save_state.set(SaveState::Clean);
                                loading.set(false);
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
pub fn FileBrowserDialog(
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
