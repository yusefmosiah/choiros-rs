use dioxus::prelude::*;

use super::styles::CHAT_STYLES;

#[component]
pub fn SettingsView(desktop_id: String, window_id: String) -> Element {
    let _ = (&desktop_id, &window_id);
    let mut active_tab = use_signal(|| "model_policy".to_string());
    let tab_label = match active_tab().as_str() {
        "model_policy" => "Model Policy",
        "runtime" => "Runtime",
        "ui" => "UI",
        _ => "Settings",
    };
    let doc_path = match active_tab().as_str() {
        "model_policy" => "config/model-policy.toml",
        "runtime" => "config/runtime-policy.toml",
        "ui" => "config/ui-policy.toml",
        _ => "config/settings.txt",
    };
    let doc_body = match active_tab().as_str() {
        "model_policy" => {
            r#"default_model = "ClaudeBedrockSonnet45"
chat_default_model = "ClaudeBedrockSonnet45"
terminal_default_model = "KimiK25"
conductor_default_model = "ClaudeBedrockOpus46"
summarizer_default_model = "ZaiGLM47Flash"
allow_request_override = true

allowed_models = ["ClaudeBedrockOpus46", "ClaudeBedrockSonnet45", "ClaudeBedrockHaiku45", "KimiK25", "ZaiGLM47", "ZaiGLM47Flash", "ZaiGLM47Air"]
chat_allowed_models = ["ClaudeBedrockSonnet45", "ClaudeBedrockHaiku45", "ZaiGLM47Flash", "ZaiGLM47", "KimiK25"]
terminal_allowed_models = ["KimiK25", "ZaiGLM47", "ZaiGLM47Flash"]
conductor_allowed_models = ["ClaudeBedrockOpus46", "ClaudeBedrockSonnet45", "ZaiGLM47"]
summarizer_allowed_models = ["ZaiGLM47Flash", "ClaudeBedrockHaiku45"]"#
        }
        "runtime" => {
            r#"routing_mode = "conductor_default"
active_context_capsule_enabled = true
logs_stream_mode = "raw_then_summary""#
        }
        "ui" => {
            r#"logs_compact_mode = true
show_raw_json_default = false
summary_typography = "italic""#
        }
        _ => "",
    };

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
                    style: if active_tab() == "model_policy" { "text-align:left; width:100%; font-weight:700;" } else { "text-align:left; width:100%;" },
                    onclick: move |_| active_tab.set("model_policy".to_string()),
                    "Model Policy"
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
                }
                pre {
                    class: "tool-pre",
                    style: "flex: 1; min-height: 320px; max-height: none;",
                    "{doc_body}"
                }
                if active_tab() == "model_policy" {
                    details {
                        class: "tool-details",
                        summary {
                            class: "tool-summary",
                            "Available models catalog"
                        }
                        div {
                            class: "tool-body",
                            p { class: "tool-meta", "Catalog only. Keep policy document minimal and role-focused." }
                            ul {
                                style: "margin: 0.25rem 0 0.25rem 1rem;",
                                li { "ClaudeBedrockOpus46" }
                                li { "ClaudeBedrockSonnet45" }
                                li { "ClaudeBedrockHaiku45" }
                                li { "ZaiGLM47" }
                                li { "ZaiGLM47Flash" }
                                li { "ZaiGLM47Air" }
                                li { "KimiK25" }
                                li { "KimiK25Fallback" }
                            }
                        }
                    }
                }
                p { class: "tool-meta", "Settings are document-first: each tab is one config document view; global settings are the conceptual concatenation of these documents." }
            }
        }
    }
}
