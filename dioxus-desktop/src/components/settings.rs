use std::collections::HashMap;

use dioxus::prelude::*;

use crate::api::{fetch_model_catalog, fetch_model_config, update_model_config, ModelInfo};

use super::styles::CHAT_STYLES;

#[component]
pub fn SettingsView(desktop_id: String, window_id: String) -> Element {
    let _ = (&desktop_id, &window_id);
    let mut active_tab = use_signal(|| "model_config".to_string());

    rsx! {
        style { {CHAT_STYLES} }
        div {
            class: "panel-container",
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
            }
            div {
                style: "flex: 1; padding: 0.75rem; overflow: auto; display:flex; flex-direction:column; gap:0.5rem;",
                if active_tab() == "model_config" {
                    ModelConfigPanel {}
                }
            }
        }
    }
}

#[component]
fn ModelConfigPanel() -> Element {
    let mut models = use_signal(Vec::<ModelInfo>::new);
    let mut callsites = use_signal(Vec::<String>::new);
    let mut selections = use_signal(HashMap::<String, String>::new);
    let mut status_msg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);

    // Load catalog + current config on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            match fetch_model_catalog().await {
                Ok(catalog) => {
                    models.set(catalog.models);
                    callsites.set(catalog.callsites);
                    // Start with defaults
                    let mut sels = catalog.defaults;
                    // Override with user config
                    // TODO: get user_id from auth context
                    if let Ok(config) = fetch_model_config("default").await {
                        for (k, v) in config.callsite_models {
                            sels.insert(k, v);
                        }
                    }
                    selections.set(sels);
                }
                Err(e) => {
                    status_msg.set(Some(format!("Failed to load catalog: {e}")));
                }
            }
            loading.set(false);
        });
    });

    let on_save = move |_| {
        let sels = selections();
        spawn(async move {
            status_msg.set(Some("Saving...".to_string()));
            match update_model_config("default", &sels).await {
                Ok(_) => status_msg.set(Some("Saved".to_string())),
                Err(e) => status_msg.set(Some(format!("Error: {e}"))),
            }
        });
    };

    rsx! {
        div { class: "panel-header",
            h3 { "Model Config" }
            if let Some(msg) = status_msg() {
                span { class: "panel-status", "{msg}" }
            }
        }
        if loading() {
            p { "Loading model catalog..." }
        } else {
            div {
                style: "display: flex; flex-direction: column; gap: 0.75rem;",
                for callsite in callsites() {
                    {
                        let cs = callsite.clone();
                        let cs2 = callsite.clone();
                        let current = selections().get(&callsite).cloned().unwrap_or_default();
                        rsx! {
                            div {
                                style: "display: flex; align-items: center; gap: 0.75rem;",
                                label {
                                    style: "min-width: 100px; font-weight: 600; text-transform: capitalize;",
                                    "{cs}"
                                }
                                select {
                                    style: "flex: 1; padding: 0.35rem 0.5rem; background: var(--window-bg, #111827); color: var(--text-color, #e2e8f0); border: 1px solid var(--border-color, #334155); border-radius: 4px;",
                                    value: "{current}",
                                    onchange: move |evt: Event<FormData>| {
                                        let val = evt.value();
                                        let cs_key = cs2.clone();
                                        selections.write().insert(cs_key, val);
                                    },
                                    for model in models() {
                                        option {
                                            value: "{model.id}",
                                            selected: model.id == current,
                                            "{model.name}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    style: "margin-top: 0.5rem;",
                    button {
                        class: "tool-summary",
                        style: "width: auto; padding: 0.35rem 1rem; font-weight: 600;",
                        onclick: on_save,
                        "Save"
                    }
                }
            }
        }
    }
}
