use dioxus::prelude::*;

use crate::api::files_api::read_file_content;

use super::styles::CHAT_STYLES;

fn settings_doc_candidates(tab: &str) -> &'static [&'static str] {
    match tab {
        "model_config" => &["config/model-catalog.toml"],
        "runtime" => &["config/runtime-policy.toml"],
        "ui" => &["config/ui-policy.toml"],
        _ => &["config/settings.txt"],
    }
}

#[component]
pub fn SettingsView(desktop_id: String, window_id: String) -> Element {
    let _ = (&desktop_id, &window_id);
    let mut active_tab = use_signal(|| "model_config".to_string());
    let mut doc_content = use_signal(String::new);
    let mut doc_error = use_signal(|| None::<String>);
    let mut doc_loading = use_signal(|| false);
    let mut resolved_doc_path =
        use_signal(|| settings_doc_candidates("model_config")[0].to_string());
    let mut refresh_counter = use_signal(|| 0u64);
    let tab_label = match active_tab().as_str() {
        "model_config" => "Model Config",
        "runtime" => "Runtime",
        "ui" => "UI",
        _ => "Settings",
    };
    let doc_path = resolved_doc_path();

    use_effect(move || {
        let tab = active_tab();
        let _refresh = refresh_counter();
        let candidates = settings_doc_candidates(tab.as_str())
            .iter()
            .map(|v| (*v).to_string())
            .collect::<Vec<_>>();
        spawn(async move {
            doc_loading.set(true);
            doc_error.set(None);
            let mut last_error = None::<String>;

            for path in candidates {
                resolved_doc_path.set(path.clone());
                match read_file_content(&path).await {
                    Ok(response) => {
                        doc_content.set(response.content);
                        doc_error.set(None);
                        doc_loading.set(false);
                        return;
                    }
                    Err(err) => {
                        last_error = Some(err);
                    }
                }
            }

            doc_content.set(String::new());
            doc_error.set(Some(
                last_error.unwrap_or_else(|| "No candidate settings path matched".to_string()),
            ));
            doc_loading.set(false);
        });
    });

    rsx! {
        style { {CHAT_STYLES} }
        div {
            class: "chat-container",
            style: "display: flex; height: 100%; overflow: hidden;",
            div {
                style: "width: 210px; border-right: 1px solid var(--border-color, #334155); padding: 0.6rem; display: flex; flex-direction: column; gap: 0.45rem; background: color-mix(in srgb, var(--window-bg, #111827) 92%, #0b1220 8%);",
                h3 { style: "margin: 0 0 0.25rem 0;", "Settings" }
                button {
                    class: "tool-summary",
                    style: if active_tab() == "model_config" { "text-align:left; width:100%; font-weight:700;" } else { "text-align:left; width:100%;" },
                    onclick: move |_| active_tab.set("model_config".to_string()),
                    "Model Config"
                }
                button {
                    class: "tool-summary",
                    style: if active_tab() == "runtime" { "text-align:left; width:100%; font-weight:700;" } else { "text-align:left; width:100%;" },
                    onclick: move |_| active_tab.set("runtime".to_string()),
                    "Runtime"
                }
                button {
                    class: "tool-summary",
                    style: if active_tab() == "ui" { "text-align:left; width:100%; font-weight:700;" } else { "text-align:left; width:100%;" },
                    onclick: move |_| active_tab.set("ui".to_string()),
                    "UI"
                }
            }
            div {
                style: "flex: 1; padding: 0.75rem; overflow: auto; display:flex; flex-direction:column; gap:0.5rem;",
                div { class: "chat-header",
                    h3 { "{tab_label}" }
                    span { class: "chat-status", "Document: {doc_path}" }
                    button {
                        class: "tool-summary",
                        style: "margin-left: auto; width: auto; padding: 0.2rem 0.55rem;",
                        onclick: move |_| refresh_counter += 1,
                        "Refresh"
                    }
                }
                pre {
                    class: "tool-pre",
                    style: "flex: 1; min-height: 320px; max-height: none;",
                    if doc_loading() {
                        "Loading {doc_path}..."
                    } else if let Some(err) = doc_error() {
                        "Failed to load {doc_path}:\n{err}"
                    } else {
                        "{doc_content}"
                    }
                }
                p { class: "tool-meta", "Displays live file contents. Use Refresh after external edits." }
            }
        }
    }
}
