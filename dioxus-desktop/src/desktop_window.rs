use dioxus::prelude::*;
use dioxus_web::WebEventExt;
use gloo_timers::future::TimeoutFuture;
use shared_types::WindowState;
use wasm_bindgen::JsCast;

use crate::components::{load_files_path, FilesView, LogsView, RunView, SettingsView, WriterView};
use crate::terminal::TerminalView;
use crate::viewers::{parse_viewer_window_props, ViewerShell};

const DRAG_THRESHOLD_PX: i32 = 4;
const KEYBOARD_STEP_PX: i32 = 10;
const MIN_WINDOW_WIDTH: i32 = 200;
const MIN_WINDOW_HEIGHT: i32 = 160;
const MIN_VISIBLE_X_PX: i32 = 64;
const PATCH_INTERVAL_MS: i64 = 50;

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InteractionMode {
    Drag,
    Resize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct InteractionState {
    mode: InteractionMode,
    pointer_id: i32,
    start_x: i32,
    start_y: i32,
    start_bounds: WindowBounds,
    committed_bounds: WindowBounds,
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

fn clamp_bounds(bounds: WindowBounds, viewport: (u32, u32), is_mobile: bool) -> WindowBounds {
    let (vw, vh) = viewport;
    if is_mobile {
        let mobile_width = ((vw as i32) - 20).max(280).min(vw as i32 - 8);
        let mobile_height = ((vh as i32) - 130).max(260).min(vh as i32 - 20);
        let min_x = 4;
        let max_x = (vw as i32 - mobile_width - 4).max(min_x);
        let min_y = 8;
        let max_y = (vh as i32 - mobile_height - 64).max(min_y);
        let x = bounds.x.max(min_x).min(max_x);
        let y = bounds.y.max(min_y).min(max_y);
        return WindowBounds {
            x,
            y,
            width: mobile_width,
            height: mobile_height,
        };
    }

    let width_cap = (vw as i32 - 40).max(MIN_WINDOW_WIDTH);
    let height_cap = (vh as i32 - 120).max(MIN_WINDOW_HEIGHT);
    let width = bounds.width.max(MIN_WINDOW_WIDTH).min(width_cap);
    let height = bounds.height.max(MIN_WINDOW_HEIGHT).min(height_cap);
    let min_x = -(width - MIN_VISIBLE_X_PX).max(0);
    let max_x = (vw as i32 - MIN_VISIBLE_X_PX).max(min_x);
    let x = bounds.x.max(min_x).min(max_x);
    let y = bounds.y.max(10).min(vh as i32 - height - 60);

    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

fn pointer_point(e: &PointerEvent) -> (i32, i32) {
    if let Some((x, y)) = e.data().try_as_web_event().and_then(|event| {
        event
            .dyn_ref::<web_sys::PointerEvent>()
            .map(|pointer| (pointer.client_x(), pointer.client_y()))
    }) {
        return (x, y);
    }

    let point = e.data().client_coordinates();
    (point.x as i32, point.y as i32)
}

fn pointer_buttons(e: &PointerEvent) -> u16 {
    e.data()
        .try_as_web_event()
        .and_then(|event| {
            event
                .dyn_ref::<web_sys::PointerEvent>()
                .map(|pointer| pointer.buttons())
        })
        .unwrap_or(1)
}

fn pointer_target_is_window_control(e: &PointerEvent) -> bool {
    e.data()
        .try_as_web_event()
        .and_then(|event| event.target())
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .map(|element| {
            element.closest("button").ok().flatten().is_some()
                || element.closest(".window-controls").ok().flatten().is_some()
        })
        .unwrap_or(false)
}

fn capture_window_pointer(e: &PointerEvent, pointer_id: i32) {
    let _ = e
        .data()
        .try_as_web_event()
        .and_then(|event| event.current_target())
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| element.closest(".floating-window").ok().flatten())
        .map(|window| window.set_pointer_capture(pointer_id));
}

fn release_window_pointer(e: &PointerEvent, pointer_id: i32) {
    let _ = e
        .data()
        .try_as_web_event()
        .and_then(|event| event.current_target())
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| element.closest(".floating-window").ok().flatten())
        .map(|window| window.release_pointer_capture(pointer_id));
}

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
    on_minimize: Callback<String>,
    on_maximize: Callback<String>,
    on_restore: Callback<String>,
) -> Element {
    let window_id = window.id.clone();
    let (vw, _vh) = viewport;
    let is_mobile = vw <= 1024;

    let committed = clamp_bounds(
        WindowBounds {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        },
        viewport,
        is_mobile,
    );

    let mut interaction = use_signal(|| None::<InteractionState>);
    let mut live_bounds = use_signal(|| None::<WindowBounds>);
    let mut queued_move = use_signal(|| None::<(i32, i32)>);
    let mut queued_resize = use_signal(|| None::<(i32, i32)>);
    let mut move_flush_scheduled = use_signal(|| false);
    let mut resize_flush_scheduled = use_signal(|| false);
    let mut last_move_sent_ms = use_signal(|| 0i64);
    let mut last_resize_sent_ms = use_signal(|| 0i64);

    let bounds = live_bounds().unwrap_or(committed);

    let window_id_for_focus = window_id.clone();
    let window_id_for_minimize = window_id.clone();
    let window_id_for_keyboard = window_id.clone();
    let window_id_for_pointer_move = window_id.clone();
    let window_id_for_pointer_up = window_id.clone();
    let window_id_for_title_key = window_id.clone();
    let window_id_for_title_pointer = window_id.clone();
    let window_id_for_resize_pointer = window_id.clone();

    let z_index = window.z_index;
    let viewer_props = parse_viewer_window_props(&window.props).ok();
    let active_outline = if is_active && !window.maximized {
        "2px solid var(--accent-bg, #3b82f6)"
    } else {
        "none"
    };
    let window_style = if window.maximized {
        format!(
            "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: {z_index}; \
             display: flex; flex-direction: column; background: var(--window-bg, #1f2937); \
             border: none; border-radius: 0; overflow: hidden; box-shadow: none; \
             outline: {active_outline};"
        )
    } else {
        format!(
            "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; z-index: \
             {z_index}; display: flex; flex-direction: column; background: var(--window-bg, \
             #1f2937); border: 1px solid var(--border-color, #374151); border-radius: \
             var(--radius-lg, 12px); overflow: hidden; box-shadow: var(--shadow-lg, 0 10px 40px \
             rgba(0,0,0,0.5)); outline: {active_outline};",
            bounds.x, bounds.y, bounds.width, bounds.height
        )
    };

    {
        let window_id_for_sync = window_id.clone();
        let on_move_sync = on_move;
        let on_resize_sync = on_resize;
        use_effect(move || {
            if interaction().is_some() || window.maximized {
                return;
            }

            if committed.x != window.x || committed.y != window.y {
                on_move_sync.call((window_id_for_sync.clone(), committed.x, committed.y));
            }

            if committed.width != window.width || committed.height != window.height {
                on_resize_sync.call((
                    window_id_for_sync.clone(),
                    committed.width,
                    committed.height,
                ));
            }
        });
    }

    let on_window_keydown = move |e: KeyboardEvent| {
        let key = e.key();
        let modifiers = e.modifiers();

        if key == Key::F4 && modifiers.alt() {
            e.prevent_default();
            on_close.call(window_id_for_keyboard.clone());
            return;
        }

        if key == Key::Escape {
            if let Some(active) = interaction() {
                e.prevent_default();
                live_bounds.set(Some(active.committed_bounds));
                interaction.set(None);
            }
            return;
        }

        if key == Key::Character("m".to_string()) && modifiers.ctrl() && !modifiers.shift() {
            e.prevent_default();
            on_minimize.call(window_id_for_keyboard.clone());
            return;
        }

        if key == Key::Character("m".to_string()) && modifiers.ctrl() && modifiers.shift() {
            e.prevent_default();
            if window.maximized {
                on_restore.call(window_id_for_keyboard.clone());
            } else {
                on_maximize.call(window_id_for_keyboard.clone());
            }
            return;
        }

        if modifiers.alt() {
            if modifiers.shift() {
                let mut next = bounds;
                match key {
                    Key::ArrowLeft => next.width -= KEYBOARD_STEP_PX,
                    Key::ArrowRight => next.width += KEYBOARD_STEP_PX,
                    Key::ArrowUp => next.height -= KEYBOARD_STEP_PX,
                    Key::ArrowDown => next.height += KEYBOARD_STEP_PX,
                    _ => return,
                }
                e.prevent_default();
                let next = clamp_bounds(next, viewport, is_mobile);
                live_bounds.set(Some(next));
                on_resize.call((window_id_for_keyboard.clone(), next.width, next.height));
                return;
            }

            let mut next = bounds;
            match key {
                Key::ArrowLeft => next.x -= KEYBOARD_STEP_PX,
                Key::ArrowRight => next.x += KEYBOARD_STEP_PX,
                Key::ArrowUp => next.y -= KEYBOARD_STEP_PX,
                Key::ArrowDown => next.y += KEYBOARD_STEP_PX,
                _ => return,
            }
            e.prevent_default();
            let next = clamp_bounds(next, viewport, is_mobile);
            live_bounds.set(Some(next));
            on_move.call((window_id_for_keyboard.clone(), next.x, next.y));
        }
    };

    rsx! {
        div {
            class: if is_active { "floating-window active" } else { "floating-window" },
            role: "dialog",
            "aria-label": window.title.clone(),
            "aria-modal": if is_active { "false" } else { "true" },
            tabindex: "0",
            style: "{window_style}",
            onclick: move |_| on_focus.call(window_id_for_focus.clone()),
            onkeydown: on_window_keydown,
            onpointermove: move |e| {
                let Some(active) = interaction() else {
                    return;
                };

                if e.data().pointer_id() != active.pointer_id {
                    return;
                }

                // Pointer capture can occasionally be lost across browser focus transitions.
                // If no buttons are held, end interaction immediately to avoid sticky drag mode.
                if pointer_buttons(&e) == 0 {
                    let final_bounds = live_bounds().unwrap_or(active.start_bounds);
                    match active.mode {
                        InteractionMode::Drag => {
                            queued_move.set(Some((final_bounds.x, final_bounds.y)));
                            if let Some((next_x, next_y)) = queued_move.write().take() {
                                on_move.call((window_id_for_pointer_move.clone(), next_x, next_y));
                                last_move_sent_ms.set(now_ms());
                            }
                        }
                        InteractionMode::Resize => {
                            queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                            if let Some((next_w, next_h)) = queued_resize.write().take() {
                                on_resize.call((window_id_for_pointer_move.clone(), next_w, next_h));
                                last_resize_sent_ms.set(now_ms());
                            }
                        }
                    }
                    interaction.set(None);
                    return;
                }

                let (client_x, client_y) = pointer_point(&e);
                let dx = client_x - active.start_x;
                let dy = client_y - active.start_y;

                if dx.abs() < DRAG_THRESHOLD_PX && dy.abs() < DRAG_THRESHOLD_PX {
                    return;
                }

                let next = match active.mode {
                    InteractionMode::Drag => WindowBounds {
                        x: active.start_bounds.x + dx,
                        y: active.start_bounds.y + dy,
                        width: active.start_bounds.width,
                        height: active.start_bounds.height,
                    },
                    InteractionMode::Resize => WindowBounds {
                        x: active.start_bounds.x,
                        y: active.start_bounds.y,
                        width: active.start_bounds.width + dx,
                        height: active.start_bounds.height + dy,
                    },
                };
                let next = clamp_bounds(next, viewport, is_mobile);

                live_bounds.set(Some(next));
                match active.mode {
                    InteractionMode::Drag => {
                        queued_move.set(Some((next.x, next.y)));
                        let elapsed = now_ms() - last_move_sent_ms();
                        if elapsed >= PATCH_INTERVAL_MS {
                            if let Some((next_x, next_y)) = queued_move.write().take() {
                                on_move.call((window_id_for_pointer_move.clone(), next_x, next_y));
                                last_move_sent_ms.set(now_ms());
                            }
                        } else if !move_flush_scheduled() {
                            move_flush_scheduled.set(true);
                            let wait_ms = (PATCH_INTERVAL_MS - elapsed).max(1) as u32;
                            let mut move_flush_scheduled_clone = move_flush_scheduled;
                            let mut queued_move_clone = queued_move;
                            let mut last_move_sent_ms_clone = last_move_sent_ms;
                            let on_move_clone = on_move;
                            let window_id_clone = window_id_for_pointer_move.clone();
                            spawn(async move {
                                TimeoutFuture::new(wait_ms).await;
                                if let Some((next_x, next_y)) = queued_move_clone.write().take() {
                                    on_move_clone.call((window_id_clone, next_x, next_y));
                                    last_move_sent_ms_clone.set(now_ms());
                                }
                                move_flush_scheduled_clone.set(false);
                            });
                        }
                    }
                    InteractionMode::Resize => {
                        queued_resize.set(Some((next.width, next.height)));
                        let elapsed = now_ms() - last_resize_sent_ms();
                        if elapsed >= PATCH_INTERVAL_MS {
                            if let Some((next_w, next_h)) = queued_resize.write().take() {
                                on_resize.call((window_id_for_pointer_move.clone(), next_w, next_h));
                                last_resize_sent_ms.set(now_ms());
                            }
                        } else if !resize_flush_scheduled() {
                            resize_flush_scheduled.set(true);
                            let wait_ms = (PATCH_INTERVAL_MS - elapsed).max(1) as u32;
                            let mut resize_flush_scheduled_clone = resize_flush_scheduled;
                            let mut queued_resize_clone = queued_resize;
                            let mut last_resize_sent_ms_clone = last_resize_sent_ms;
                            let on_resize_clone = on_resize;
                            let window_id_clone = window_id_for_pointer_move.clone();
                            spawn(async move {
                                TimeoutFuture::new(wait_ms).await;
                                if let Some((next_w, next_h)) = queued_resize_clone.write().take() {
                                    on_resize_clone.call((window_id_clone, next_w, next_h));
                                    last_resize_sent_ms_clone.set(now_ms());
                                }
                                resize_flush_scheduled_clone.set(false);
                            });
                        }
                    }
                }
            },
            onpointerup: move |e| {
                let Some(active) = interaction() else {
                    return;
                };
                if e.data().pointer_id() != active.pointer_id {
                    return;
                }
                release_window_pointer(&e, active.pointer_id);

                let final_bounds = live_bounds().unwrap_or(active.start_bounds);
                match active.mode {
                    InteractionMode::Drag => {
                        queued_move.set(Some((final_bounds.x, final_bounds.y)));
                        if let Some((next_x, next_y)) = queued_move.write().take() {
                            on_move.call((window_id_for_pointer_up.clone(), next_x, next_y));
                            last_move_sent_ms.set(now_ms());
                        }
                    }
                    InteractionMode::Resize => {
                        queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                        if let Some((next_w, next_h)) = queued_resize.write().take() {
                            on_resize.call((window_id_for_pointer_up.clone(), next_w, next_h));
                            last_resize_sent_ms.set(now_ms());
                        }
                    }
                }

                interaction.set(None);
            },
            onpointercancel: move |e| {
                let Some(active) = interaction() else {
                    return;
                };
                if e.data().pointer_id() != active.pointer_id {
                    return;
                }
                release_window_pointer(&e, active.pointer_id);

                live_bounds.set(Some(active.committed_bounds));
                interaction.set(None);
            },

            if !window.maximized {
                div {
                    class: "window-titlebar",
                    tabindex: "0",
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); cursor: grab; user-select: none; touch-action: none;",
                    onkeydown: move |e| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".to_string()) {
                            on_focus.call(window_id_for_title_key.clone());
                        }
                    },
                    onpointerdown: move |e| {
                        if window.maximized || is_mobile {
                            return;
                        }
                        if pointer_target_is_window_control(&e) {
                            return;
                        }
                        if !is_active {
                            on_focus.call(window_id_for_title_pointer.clone());
                        }
                        e.prevent_default();
                        capture_window_pointer(&e, e.data().pointer_id());

                        let (start_x, start_y) = pointer_point(&e);
                        interaction.set(Some(InteractionState {
                            mode: InteractionMode::Drag,
                            pointer_id: e.data().pointer_id(),
                            start_x,
                            start_y,
                            start_bounds: bounds,
                            committed_bounds: committed,
                        }));
                    },

                    div {
                        style: "display: flex; align-items: center; gap: 0.5rem;",
                        span { style: "font-size: 1rem;", {get_app_icon(&window.app_id)} }
                        span { style: "font-weight: 500; color: var(--text-primary, white);", "{window.title}" }
                    }

                    WindowControls {
                        maximized: false,
                        floating: false,
                        mobile: is_mobile,
                        window_id: window_id_for_minimize.clone(),
                        on_minimize,
                        on_maximize,
                        on_restore,
                        on_close,
                    }
                }
            } else {
                WindowControls {
                    maximized: true,
                    floating: true,
                    mobile: is_mobile,
                    window_id: window_id_for_minimize.clone(),
                    on_minimize,
                    on_maximize,
                    on_restore,
                    on_close,
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
                    "terminal" => rsx! {
                        TerminalView {
                            key: "{window.id}",
                            terminal_id: window.id.clone(),
                            width: bounds.width,
                            height: bounds.height,
                        }
                    },
                    "logs" => rsx! {
                        LogsView {
                            key: "{window.id}",
                            desktop_id: desktop_id.clone(),
                            window_id: window.id.clone(),
                        }
                    },
                    "files" => {
                        let initial_path = load_files_path(&desktop_id, &window.id);
                        rsx! {
                            FilesView {
                                key: "{window.id}",
                                desktop_id: desktop_id.clone(),
                                window_id: window.id.clone(),
                                initial_path,
                            }
                        }
                    }
                    "settings" => rsx! {
                        SettingsView {
                            key: "{window.id}",
                            desktop_id: desktop_id.clone(),
                            window_id: window.id.clone(),
                        }
                    },
                    "writer" => {
                        let initial_path = window
                            .props
                            .get("path")
                            .and_then(|v| v.as_str())
                            .or_else(|| window.props.get("file_path").and_then(|v| v.as_str()))
                            .unwrap_or("")
                            .to_string();
                        rsx! {
                            WriterView {
                                key: "{window.id}",
                                desktop_id: desktop_id.clone(),
                                window_id: window.id.clone(),
                                initial_path: initial_path,
                            }
                        }
                    },
                    "run" => {
                        let run_id = window
                            .props
                            .get("run_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let document_path = window
                            .props
                            .get("document_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        rsx! {
                            RunView {
                                key: "{window.id}",
                                desktop_id: desktop_id.clone(),
                                window_id: window.id.clone(),
                                run_id: run_id,
                                document_path: document_path,
                            }
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

            if !is_mobile && !window.maximized {
                div {
                    class: "resize-handle",
                    style: "position: absolute; right: 0; bottom: 0; width: 16px; height: 16px; cursor: se-resize;",
                    onpointerdown: move |e| {
                        if is_mobile {
                            return;
                        }
                        if !is_active {
                            on_focus.call(window_id_for_resize_pointer.clone());
                        }
                        e.prevent_default();
                        capture_window_pointer(&e, e.data().pointer_id());

                        let (start_x, start_y) = pointer_point(&e);
                        interaction.set(Some(InteractionState {
                            mode: InteractionMode::Resize,
                            pointer_id: e.data().pointer_id(),
                            start_x,
                            start_y,
                            start_bounds: bounds,
                            committed_bounds: committed,
                        }));
                    },
                }
            }
        }
    }
}

#[component]
fn WindowControls(
    maximized: bool,
    floating: bool,
    mobile: bool,
    window_id: String,
    on_minimize: Callback<String>,
    on_maximize: Callback<String>,
    on_restore: Callback<String>,
    on_close: Callback<String>,
) -> Element {
    let window_id_for_minimize = window_id.clone();
    let window_id_for_max_restore = window_id.clone();
    let window_id_for_close = window_id;
    let mut expanded = use_signal(|| false);
    let container_style = if floating {
        "position: absolute; top: 0.75rem; right: 0.75rem; z-index: 10; display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem; border: none; border-radius: 999px; background: transparent;"
    } else {
        "display: flex; align-items: center; gap: 0.25rem;"
    };
    let show_compact_toggle = floating && mobile;
    let action_row_style = if floating && !mobile {
        "display: flex; align-items: center; gap: 0.25rem; background: color-mix(in srgb, var(--titlebar-bg, #111827) 35%, transparent); border-radius: 999px; padding: 0.125rem 0.25rem;"
    } else if floating && mobile {
        "display: flex; align-items: center; gap: 0.25rem; background: color-mix(in srgb, var(--titlebar-bg, #111827) 28%, transparent); border-radius: 999px; padding: 0.125rem 0.25rem;"
    } else {
        "display: flex; align-items: center; gap: 0.25rem;"
    };

    rsx! {
        div {
            class: if floating { "window-controls window-controls-floating" } else { "window-controls" },
            style: "{container_style}",

            if show_compact_toggle {
                button {
                    style: "width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; background: transparent; color: #2563eb; border: none; border-radius: 999px; cursor: pointer; font-size: 1.1rem; font-weight: 700;",
                    onpointerdown: move |e| e.stop_propagation(),
                    "aria-label": if expanded() { "Hide window controls" } else { "Show window controls" },
                    onclick: move |e| {
                        e.stop_propagation();
                        expanded.set(!expanded());
                    },
                    "◎"
                }
            }

            if !show_compact_toggle || expanded() {
                div {
                    style: "{action_row_style}",
                    button {
                        style: "width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; background: transparent; color: #facc15; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer;",
                        onpointerdown: move |e| e.stop_propagation(),
                        "aria-label": "Minimize",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_minimize.call(window_id_for_minimize.clone());
                            expanded.set(false);
                        },
                        "−"
                    }
                    button {
                        style: "width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; background: transparent; color: #22c55e; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer;",
                        onpointerdown: move |e| e.stop_propagation(),
                        "aria-label": if maximized { "Restore" } else { "Maximize" },
                        onclick: move |e| {
                            e.stop_propagation();
                            if maximized {
                                on_restore.call(window_id_for_max_restore.clone());
                            } else {
                                on_maximize.call(window_id_for_max_restore.clone());
                            }
                            expanded.set(false);
                        },
                        if maximized { "❐" } else { "□" }
                    }
                    button {
                        class: "window-close",
                        style: "width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; background: transparent; color: #ef4444; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer; font-size: 1.25rem; line-height: 1;",
                        onpointerdown: move |e| e.stop_propagation(),
                        "aria-label": "Close",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_close.call(window_id_for_close.clone());
                            expanded.set(false);
                        },
                        "×"
                    }
                }
            }
        }
    }
}

fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "writer" => "📝",
        "terminal" => "🖥️",
        "files" => "📁",
        "logs" => "📡",
        "settings" => "⚙️",
        "run" => "🚀",
        _ => "📱",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_respects_minimums() {
        let clamped = clamp_bounds(
            WindowBounds {
                x: -100,
                y: -100,
                width: 50,
                height: 20,
            },
            (1280, 720),
            false,
        );

        assert_eq!(clamped.x, -100);
        assert_eq!(clamped.y, 10);
        assert_eq!(clamped.width, MIN_WINDOW_WIDTH);
        assert_eq!(clamped.height, MIN_WINDOW_HEIGHT);
    }

    #[test]
    fn clamp_allows_horizontal_overhang_but_keeps_strip_visible() {
        let clamped = clamp_bounds(
            WindowBounds {
                x: -999,
                y: 40,
                width: 500,
                height: 300,
            },
            (1280, 720),
            false,
        );
        assert_eq!(clamped.x, -(500 - MIN_VISIBLE_X_PX));

        let clamped_right = clamp_bounds(
            WindowBounds {
                x: 9999,
                y: 40,
                width: 500,
                height: 300,
            },
            (1280, 720),
            false,
        );
        assert_eq!(clamped_right.x, 1280 - MIN_VISIBLE_X_PX);
    }
}
