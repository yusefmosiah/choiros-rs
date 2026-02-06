use dioxus::prelude::*;
use shared_types::WindowState;

use crate::components::ChatView;
use crate::terminal::TerminalView;
use crate::viewers::{parse_viewer_window_props, ViewerShell};

#[component]
pub fn FloatingWindow(
    window: WindowState,
    desktop_id: String,
    is_active: bool,
    viewport: (u32, u32),
    on_close: Callback<String>,
    on_focus: Callback<String>,
    on_move: Callback<(String, i32, i32)>,
    on_resize: Callback<(String, i32, i32)>,
) -> Element {
    let window_id = window.id.clone();
    let (vw, vh) = viewport;
    let is_mobile = vw <= 1024;

    let (width, height, x, y) = if is_mobile {
        (vw as i32 - 20, vh as i32 - 100, 10i32, 10i32)
    } else {
        let w = window.width.min(vw as i32 - 40);
        let h = window.height.min(vh as i32 - 120);
        let x = window.x.max(10).min(vw as i32 - w - 10);
        let y = window.y.max(10).min(vh as i32 - h - 60);
        (w, h, x, y)
    };

    let z_index = window.z_index;
    let window_id_for_focus = window_id.clone();
    let window_id_for_drag = window_id.clone();
    let window_id_for_close = window_id.clone();
    let window_id_for_resize = window_id.clone();
    let on_move_drag = on_move;
    let viewer_props = parse_viewer_window_props(&window.props).ok();

    rsx! {
        div {
            class: if is_active { "floating-window active" } else { "floating-window" },
            style: "position: absolute; left: {x}px; top: {y}px; width: {width}px; height: {height}px; z-index: {z_index}; display: flex; flex-direction: column; background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-lg, 12px); overflow: hidden; box-shadow: var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5));",
            onclick: move |_| on_focus.call(window_id_for_focus.clone()),

            div {
                class: "window-titlebar",
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); cursor: grab; user-select: none;",
                onmousedown: move |e| {
                    if !is_mobile {
                        start_drag(e, window_id_for_drag.clone(), on_move_drag);
                    }
                },

                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span { style: "font-size: 1rem;", {get_app_icon(&window.app_id)} }
                    span { style: "font-weight: 500; color: var(--text-primary, white);", "{window.title}" }
                }

                button {
                    class: "window-close",
                    style: "width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; background: transparent; color: var(--text-secondary, #9ca3af); border: none; border-radius: var(--radius-sm, 4px); cursor: pointer; font-size: 1.25rem; line-height: 1;",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_close.call(window_id_for_close.clone());
                    },
                    "×"
                }
            }

            div {
                class: "window-content",
                style: "flex: 1; overflow: hidden;",

                if let Some(viewer_props) = viewer_props.clone() {
                    ViewerShell {
                        window_id: window.id.clone(),
                        desktop_id: desktop_id.clone(),
                        descriptor: viewer_props.descriptor,
                    }
                } else {
                    match window.app_id.as_str() {
                    "chat" => rsx! {
                        ChatView { actor_id: window.id.clone() }
                    },
                    "terminal" => rsx! {
                        TerminalView {
                            terminal_id: window.id.clone(),
                            width: width,
                            height: height,
                        }
                    },
                    _ => rsx! {
                        div {
                            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280); padding: 1rem;",
                            "App not yet implemented"
                        }
                    }
                }
                }
            }

            if !is_mobile {
                div {
                    class: "resize-handle",
                    style: "position: absolute; right: 0; bottom: 0; width: 16px; height: 16px; cursor: se-resize;",
                    onmousedown: move |e| {
                        start_resize(e, window_id_for_resize.clone(), on_resize);
                    },
                }
            }
        }
    }
}

fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "chat" => "💬",
        "writer" => "📝",
        "terminal" => "🖥️",
        "files" => "📁",
        _ => "📱",
    }
}

fn start_drag(e: Event<MouseData>, window_id: String, on_move: Callback<(String, i32, i32)>) {
    let _ = (e, window_id, on_move);
}

fn start_resize(e: Event<MouseData>, window_id: String, on_resize: Callback<(String, i32, i32)>) {
    let _ = (e, window_id, on_resize);
}
