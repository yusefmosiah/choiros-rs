use dioxus::prelude::*;
use shared_types::{ViewerDescriptor, ViewerKind, ViewerRevision};

use crate::api::{fetch_viewer_content, patch_viewer_content, PatchViewerContentError};
use crate::viewers::image::ImageViewer;
use crate::viewers::markdown::MarkdownViewer;
use crate::viewers::text::TextViewer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerShellState {
    Loading,
    Ready,
    Dirty,
    Saving,
    Error,
}

#[component]
pub fn ViewerShell(window_id: String, desktop_id: String, descriptor: ViewerDescriptor) -> Element {
    let mut shell_state = use_signal(|| ViewerShellState::Loading);
    let mut content = use_signal(String::new);
    let mut rendered_html = use_signal(|| None::<String>);
    let mut saved_content = use_signal(String::new);
    let mut revision = use_signal(|| ViewerRevision {
        rev: 0,
        updated_at: String::new(),
    });
    let mut error = use_signal(|| None::<String>);
    let mut markdown_preview = use_signal(|| true);

    let uri = descriptor.resource.uri.clone();
    let mime = descriptor.resource.mime.clone();
    let readonly = descriptor.capabilities.readonly;
    let text_mode = matches!(descriptor.kind, ViewerKind::Text);
    let markdown_mode = text_mode && mime == "text/markdown";
    let effective_readonly = readonly;

    let uri_for_initial = uri.clone();
    use_effect(move || {
        let uri_inner = uri_for_initial.clone();
        spawn(async move {
            shell_state.set(ViewerShellState::Loading);
            error.set(None);
            match fetch_viewer_content(&uri_inner).await {
                Ok(resp) => {
                    content.set(resp.content.clone());
                    rendered_html.set(resp.rendered_html.clone());
                    saved_content.set(resp.content);
                    revision.set(resp.revision);
                    if markdown_mode {
                        markdown_preview.set(true);
                    }
                    shell_state.set(ViewerShellState::Ready);
                }
                Err(e) => {
                    error.set(Some(e));
                    shell_state.set(ViewerShellState::Error);
                }
            }
        });
    });

    let on_text_change = move |next: String| {
        content.set(next.clone());
        rendered_html.set(None);
        if next != saved_content() {
            shell_state.set(ViewerShellState::Dirty);
        } else {
            shell_state.set(ViewerShellState::Ready);
        }
    };

    let save_enabled = matches!(shell_state(), ViewerShellState::Dirty) && !effective_readonly;
    let uri_for_save = uri.clone();
    let markdown_mode_for_save = markdown_mode;
    let on_save = move |_| {
        if !save_enabled {
            return;
        }
        let uri_inner = uri_for_save.clone();
        let window_id_inner = window_id.clone();
        let base_rev = revision().rev;
        let content_to_save = content();

        spawn(async move {
            shell_state.set(ViewerShellState::Saving);
            match patch_viewer_content(&uri_inner, base_rev, &content_to_save, &window_id_inner)
                .await
            {
                Ok(next_revision) => {
                    revision.set(next_revision);
                    saved_content.set(content_to_save);
                    if markdown_mode_for_save {
                        match fetch_viewer_content(&uri_inner).await {
                            Ok(resp) => {
                                rendered_html.set(resp.rendered_html.clone());
                                content.set(resp.content.clone());
                                saved_content.set(resp.content);
                                revision.set(resp.revision);
                                markdown_preview.set(true);
                            }
                            Err(e) => {
                                error.set(Some(e));
                            }
                        }
                    }
                    shell_state.set(ViewerShellState::Ready);
                    error.set(None);
                }
                Err(PatchViewerContentError::Conflict {
                    latest_content,
                    latest_revision,
                }) => {
                    content.set(latest_content.clone());
                    saved_content.set(latest_content);
                    revision.set(latest_revision);
                    error.set(Some("revision_conflict".to_string()));
                    shell_state.set(ViewerShellState::Error);
                }
                Err(PatchViewerContentError::Message(message)) => {
                    error.set(Some(message));
                    shell_state.set(ViewerShellState::Error);
                }
            }
        });
    };

    let status_text = shell_status_text(&shell_state());
    let title = descriptor
        .resource
        .uri
        .split('/')
        .next_back()
        .unwrap_or("Viewer");

    let uri_for_reload = uri.clone();
    rsx! {
        div {
            class: "viewer-shell",
            style: "height: 100%; display: flex; flex-direction: column; background: #0f172a;",
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; border-bottom: 1px solid #1f2937;",
                div {
                    style: "display: flex; flex-direction: column; gap: 2px;",
                    strong { style: "font-size: 0.9rem;", "{title}" }
                    span { style: "font-size: 0.75rem; color: #94a3b8;", "{uri}" }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    if markdown_mode {
                        button {
                            onclick: move |_| markdown_preview.set(true),
                            disabled: markdown_preview(),
                            "Preview"
                        }
                        button {
                            onclick: move |_| markdown_preview.set(false),
                            disabled: !markdown_preview(),
                            "Source"
                        }
                        button {
                            onclick: move |_| set_markdown_details_open(true),
                            disabled: !markdown_preview(),
                            "Expand all"
                        }
                        button {
                            onclick: move |_| set_markdown_details_open(false),
                            disabled: !markdown_preview(),
                            "Collapse all"
                        }
                        button {
                            onclick: {
                                let content_snapshot = content();
                                move |_| copy_text_to_clipboard(&content_snapshot)
                            },
                            "Copy all"
                        }
                    }
                    button {
                        onclick: move |_| {
                            let uri_inner = uri_for_reload.clone();
                            spawn(async move {
                                shell_state.set(ViewerShellState::Loading);
                                error.set(None);
                                match fetch_viewer_content(&uri_inner).await {
                                    Ok(resp) => {
                                        content.set(resp.content.clone());
                                        rendered_html.set(resp.rendered_html.clone());
                                        saved_content.set(resp.content);
                                        revision.set(resp.revision);
                                        shell_state.set(ViewerShellState::Ready);
                                    }
                                    Err(e) => {
                                        error.set(Some(e));
                                        shell_state.set(ViewerShellState::Error);
                                    }
                                }
                            });
                        },
                        "Reload"
                    }
                    if !effective_readonly {
                        button {
                            disabled: !save_enabled,
                            onclick: on_save,
                            "Save"
                        }
                    }
                }
            }

            div {
                style: "flex: 1; overflow: hidden;",
                if matches!(shell_state(), ViewerShellState::Loading) {
                    div { style: "padding: 12px; color: #94a3b8;", "Loading..." }
                } else {
                    match descriptor.kind {
                        ViewerKind::Text => rsx! {
                            if markdown_mode {
                                if markdown_preview() {
                                    if let Some(html) = rendered_html() {
                                        MarkdownViewer {
                                            html: html.clone(),
                                        }
                                    } else {
                                        div {
                                            style: "padding: 12px; color: #94a3b8;",
                                            "Preview unavailable until content is reloaded or saved."
                                        }
                                    }
                                } else {
                                    TextViewer {
                                        content: content(),
                                        readonly: effective_readonly,
                                        on_change: on_text_change,
                                    }
                                }
                            } else {
                                TextViewer {
                                    content: content(),
                                    readonly: effective_readonly,
                                    on_change: on_text_change,
                                }
                            }
                        },
                        ViewerKind::Image => rsx! {
                            ImageViewer {
                                content: content(),
                                fallback_uri: uri.clone(),
                            }
                        },
                    }
                }
            }

            div {
                style: "display: flex; justify-content: space-between; gap: 8px; padding: 8px 12px; border-top: 1px solid #1f2937; font-size: 0.75rem; color: #94a3b8;",
                span { "{status_text}" }
                span { "rev {revision().rev} ({mime}) · {desktop_id}" }
            }
            if let Some(message) = error() {
                div {
                    style: "padding: 8px 12px; font-size: 0.75rem; color: #fca5a5;",
                    "{message}"
                }
            }
        }
    }
}

fn set_markdown_details_open(open: bool) {
    let js = if open {
        "document.querySelectorAll('.viewer-shell details').forEach((el)=>el.setAttribute('open',''));"
    } else {
        "document.querySelectorAll('.viewer-shell details').forEach((el)=>el.removeAttribute('open'));"
    };
    let _ = js_sys::eval(js);
}

fn copy_text_to_clipboard(text: &str) {
    let Ok(payload) = serde_json::to_string(text) else {
        return;
    };
    let js = format!(
        "if (navigator && navigator.clipboard) {{ navigator.clipboard.writeText({payload}); }}"
    );
    let _ = js_sys::eval(&js);
}

pub fn shell_status_text(state: &ViewerShellState) -> &'static str {
    match state {
        ViewerShellState::Loading => "Loading",
        ViewerShellState::Ready => "Saved",
        ViewerShellState::Dirty => "Unsaved changes",
        ViewerShellState::Saving => "Saving",
        ViewerShellState::Error => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_state_labels_match_contract() {
        assert_eq!(shell_status_text(&ViewerShellState::Loading), "Loading");
        assert_eq!(shell_status_text(&ViewerShellState::Ready), "Saved");
        assert_eq!(
            shell_status_text(&ViewerShellState::Dirty),
            "Unsaved changes"
        );
        assert_eq!(shell_status_text(&ViewerShellState::Saving), "Saving");
        assert_eq!(shell_status_text(&ViewerShellState::Error), "Error");
    }
}
