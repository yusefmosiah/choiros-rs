use dioxus::prelude::*;
use shared_types::WindowState;

use crate::desktop::apps::get_app_icon;

#[component]
pub fn PromptBar(
    connected: bool,
    is_mobile: bool,
    windows: Vec<WindowState>,
    active_window: Option<String>,
    on_submit: Callback<String>,
    on_focus_window: Callback<String>,
    current_theme: String,
    on_toggle_theme: Callback<()>,
) -> Element {
    let mut input_value = use_signal(String::new);
    let mut mobile_dock_expanded = use_signal(|| false);
    let visible_mobile_icons = 2usize;

    rsx! {
        div {
            class: "prompt-bar",
            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.75rem 1rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151); position: relative;",

            button {
                class: "prompt-help-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-weight: 600; flex-shrink: 0;",
                onclick: move |_| {},
                "?"
            }

            button {
                class: "prompt-theme-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; flex-shrink: 0;",
                onclick: move |_| on_toggle_theme.call(()),
                title: "Toggle theme",
                if current_theme == "dark" {
                    "☀️"
                } else {
                    "🌙"
                }
            }

            input {
                class: "prompt-input",
                style: "flex: 1; padding: 0.5rem 1rem; background: var(--input-bg, #1f2937); color: var(--text-primary, white); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); font-size: 0.875rem; outline: none; min-width: 0;",
                placeholder: "Ask anything, paste URL, or type ? for commands...",
                value: "{input_value}",
                oninput: move |e| input_value.set(e.value().clone()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        let text = input_value.to_string();
                        if !text.is_empty() {
                            on_submit.call(text);
                            input_value.set(String::new());
                        }
                    }
                }
            }

            if !windows.is_empty() && !is_mobile {
                div {
                    class: "running-apps",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0;",

                    for window in windows.iter() {
                        RunningAppIndicator {
                            key: "{window.id}",
                            window: window.clone(),
                            is_active: active_window.as_ref() == Some(&window.id),
                            on_focus: on_focus_window,
                        }
                    }
                }
            }

            if is_mobile {
                div {
                    class: "mobile-dock",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0; margin-left: auto;",

                    for window in windows.iter().take(visible_mobile_icons) {
                        RunningAppIndicator {
                            key: "{window.id}",
                            window: window.clone(),
                            is_active: active_window.as_ref() == Some(&window.id),
                            on_focus: on_focus_window,
                        }
                    }

                    if windows.len() > visible_mobile_icons {
                        button {
                            class: "mobile-dock-more",
                            style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 0.75rem; font-weight: 600;",
                            onclick: move |_| mobile_dock_expanded.set(!mobile_dock_expanded()),
                            title: "Show all open windows",
                            "+{windows.len() - visible_mobile_icons}"
                        }
                    }

                    div {
                        style: if connected {
                            "display: flex; align-items: center; justify-content: center; width: 18px; height: 18px; color: #10b981; font-size: 0.8rem;"
                        } else {
                            "display: flex; align-items: center; justify-content: center; width: 18px; height: 18px; color: #f59e0b; font-size: 0.8rem;"
                        },
                        if connected { "●" } else { "◐" }
                    }
                }

                if mobile_dock_expanded() && !windows.is_empty() {
                    div {
                        class: "mobile-dock-panel",
                        style: "position: absolute; right: 0.75rem; bottom: 3.5rem; z-index: 30; display: flex; flex-wrap: wrap; gap: 0.35rem; max-width: min(80vw, 320px); padding: 0.5rem; background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); box-shadow: var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5));",
                        for window in windows.iter() {
                            RunningAppIndicator {
                                key: "mobile-{window.id}",
                                window: window.clone(),
                                is_active: active_window.as_ref() == Some(&window.id),
                                on_focus: on_focus_window,
                            }
                        }
                    }
                }
            } else {
                div {
                    class: if connected { "ws-status connected" } else { "ws-status" },
                    style: if connected {
                        "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                    } else {
                        "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--warning-bg, #f59e0b); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                    },

                    span { if connected { "●" } else { "◐" } }
                    span { if connected { "Connected" } else { "Connecting..." } }
                }
            }
        }
    }
}

#[component]
pub fn RunningAppIndicator(
    window: WindowState,
    is_active: bool,
    on_focus: Callback<String>,
) -> Element {
    let icon = get_app_icon(&window.app_id);
    let window_id = window.id.clone();

    rsx! {
        button {
            class: if is_active { "running-app active" } else { "running-app" },
            style: if is_active {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            } else {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            },
            onclick: move |_| on_focus.call(window_id.clone()),
            title: "{window.title}",
            "{icon}"
        }
    }
}
