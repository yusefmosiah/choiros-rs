use dioxus::prelude::*;
use dioxus_web::WebEventExt;
use gloo_timers::future::TimeoutFuture;
use shared_types::WindowState;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::components::{
    load_files_path, FilesView, LogsView, SettingsView, TraceView, WriterView,
};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MobileInteractionMode {
    Normal,
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct MobilePinchState {
    pointer_a_id: i32,
    pointer_b_id: i32,
    pointer_a_x: i32,
    pointer_a_y: i32,
    pointer_b_x: i32,
    pointer_b_y: i32,
    start_distance: f64,
    start_bounds: WindowBounds,
    committed_bounds: WindowBounds,
}

struct DocumentPointerListeners {
    document: web_sys::Document,
    pointer_move: Closure<dyn FnMut(web_sys::PointerEvent)>,
    pointer_up: Closure<dyn FnMut(web_sys::PointerEvent)>,
    pointer_cancel: Closure<dyn FnMut(web_sys::PointerEvent)>,
}

impl DocumentPointerListeners {
    fn detach(self) {
        let _ = self.document.remove_event_listener_with_callback(
            "pointermove",
            self.pointer_move.as_ref().unchecked_ref(),
        );
        let _ = self.document.remove_event_listener_with_callback(
            "pointerup",
            self.pointer_up.as_ref().unchecked_ref(),
        );
        let _ = self.document.remove_event_listener_with_callback(
            "pointercancel",
            self.pointer_cancel.as_ref().unchecked_ref(),
        );
    }
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

fn clamp_bounds(bounds: WindowBounds, viewport: (u32, u32), is_mobile: bool) -> WindowBounds {
    let (vw, vh) = viewport;
    if is_mobile {
        let min_mobile_width = 240;
        let min_mobile_height = 220;
        let max_mobile_width = (vw as i32 - 8).max(min_mobile_width);
        let max_mobile_height = (vh as i32 - 20).max(min_mobile_height);
        let width = bounds.width.max(min_mobile_width).min(max_mobile_width);
        let height = bounds.height.max(min_mobile_height).min(max_mobile_height);
        let min_x = 4;
        let max_x = (vw as i32 - width - 4).max(min_x);
        let min_y = 8;
        let max_y = (vh as i32 - height - 64).max(min_y);
        let x = bounds.x.max(min_x).min(max_x);
        let y = bounds.y.max(min_y).min(max_y);
        return WindowBounds {
            x,
            y,
            width,
            height,
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

fn pointer_ids_match(active_pointer_id: i32, event_pointer_id: i32) -> bool {
    active_pointer_id == event_pointer_id
}

fn pointer_distance(ax: i32, ay: i32, bx: i32, by: i32) -> f64 {
    let dx = (bx - ax) as f64;
    let dy = (by - ay) as f64;
    (dx * dx + dy * dy).sqrt()
}

fn event_pointer_id(e: &PointerEvent) -> i32 {
    e.data()
        .try_as_web_event()
        .and_then(|event| {
            event
                .dyn_ref::<web_sys::PointerEvent>()
                .map(web_sys::PointerEvent::pointer_id)
        })
        .unwrap_or_else(|| e.data().pointer_id())
}

fn event_element(e: &PointerEvent) -> Option<web_sys::Element> {
    e.data()
        .try_as_web_event()
        .and_then(|event| event.current_target().or_else(|| event.target()))
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
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
    if let Some(element) = event_element(e) {
        if let Some(window) = element.closest(".floating-window").ok().flatten() {
            if window.set_pointer_capture(pointer_id).is_ok() {
                return;
            }

            let _ = element.set_pointer_capture(pointer_id);
            return;
        }

        let _ = element.set_pointer_capture(pointer_id);
    }
}

fn release_window_pointer(e: &PointerEvent, pointer_id: i32) {
    if let Some(element) = event_element(e) {
        let _ = element.release_pointer_capture(pointer_id);
        let _ = element
            .closest(".floating-window")
            .ok()
            .flatten()
            .map(|window| window.release_pointer_capture(pointer_id));
    }
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
    let mut mobile_pinch_anchor = use_signal(|| None::<(i32, i32, i32)>);
    let mut mobile_pinch = use_signal(|| None::<MobilePinchState>);
    let mobile_interaction_mode = use_signal(|| MobileInteractionMode::Normal);
    let document_pointer_listeners = use_signal(|| None::<DocumentPointerListeners>);

    let bounds = live_bounds().unwrap_or(committed);

    let window_id_for_focus = window_id.clone();
    let window_id_for_minimize = window_id.clone();
    let window_id_for_keyboard = window_id.clone();
    let window_id_for_pointer_move = window_id.clone();
    let window_id_for_pointer_up = window_id.clone();
    let window_id_for_pointer_lost = window_id.clone();
    let window_id_for_doc_pointer_up = window_id.clone();
    let window_id_for_title_key = window_id.clone();
    let window_id_for_title_pointer = window_id.clone();
    let window_id_for_resize_pointer = window_id.clone();

    let z_index = window.z_index;
    let viewer_props = parse_viewer_window_props(&window.props).ok();
    let window_shadow = if window.maximized {
        "none"
    } else {
        "var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5))"
    };
    let window_style = if window.maximized {
        format!(
            "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: {z_index}; \
             display: flex; flex-direction: column; background: var(--window-bg, #1f2937); \
             border: none; border-radius: 0; overflow: hidden; box-shadow: {window_shadow}; \
             outline: none;"
        )
    } else {
        format!(
            "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; z-index: \
             {z_index}; display: flex; flex-direction: column; background: var(--window-bg, \
             #1f2937); border: 1px solid var(--border-color, #374151); border-radius: \
             var(--radius-lg, 12px); overflow: hidden; box-shadow: {window_shadow}; outline: \
             none;",
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

    let pointer_move_cb = use_callback(move |e: PointerEvent| {
        let pointer_id = event_pointer_id(&e);
        let (client_x, client_y) = pointer_point(&e);

        if is_mobile && mobile_interaction_mode() == MobileInteractionMode::Resize {
            if let Some(mut pinch) = mobile_pinch() {
                let mut matches = false;
                if pointer_ids_match(pinch.pointer_a_id, pointer_id) {
                    pinch.pointer_a_x = client_x;
                    pinch.pointer_a_y = client_y;
                    matches = true;
                } else if pointer_ids_match(pinch.pointer_b_id, pointer_id) {
                    pinch.pointer_b_x = client_x;
                    pinch.pointer_b_y = client_y;
                    matches = true;
                }

                if matches {
                    let distance = pointer_distance(
                        pinch.pointer_a_x,
                        pinch.pointer_a_y,
                        pinch.pointer_b_x,
                        pinch.pointer_b_y,
                    );
                    if pinch.start_distance >= 1.0 && distance >= 1.0 {
                        let scale = distance / pinch.start_distance;
                        let raw_width = (pinch.start_bounds.width as f64 * scale).round() as i32;
                        let raw_height = (pinch.start_bounds.height as f64 * scale).round() as i32;
                        let center_x = pinch.start_bounds.x + pinch.start_bounds.width / 2;
                        let center_y = pinch.start_bounds.y + pinch.start_bounds.height / 2;
                        let next = clamp_bounds(
                            WindowBounds {
                                x: center_x - raw_width / 2,
                                y: center_y - raw_height / 2,
                                width: raw_width,
                                height: raw_height,
                            },
                            viewport,
                            is_mobile,
                        );
                        live_bounds.set(Some(next));

                        queued_resize.set(Some((next.width, next.height)));
                        let elapsed = now_ms() - last_resize_sent_ms();
                        if elapsed >= PATCH_INTERVAL_MS {
                            let pending_resize = { queued_resize.write().take() };
                            if let Some((next_w, next_h)) = pending_resize {
                                on_resize.call((
                                    window_id_for_pointer_move.clone(),
                                    next_w,
                                    next_h,
                                ));
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
                                let pending_resize = queued_resize_clone
                                    .try_write()
                                    .ok()
                                    .and_then(|mut pending| pending.take());
                                if let Some((next_w, next_h)) = pending_resize {
                                    on_resize_clone.call((window_id_clone, next_w, next_h));
                                    if let Ok(mut last_sent) = last_resize_sent_ms_clone.try_write()
                                    {
                                        *last_sent = now_ms();
                                    }
                                }
                                if let Ok(mut scheduled) = resize_flush_scheduled_clone.try_write()
                                {
                                    *scheduled = false;
                                }
                            });
                        }
                    }
                    mobile_pinch.set(Some(pinch));
                    return;
                }
            }
        }

        let Some(active) = interaction() else {
            return;
        };

        if !pointer_ids_match(active.pointer_id, pointer_id) {
            return;
        }

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
                    let pending_move = { queued_move.write().take() };
                    if let Some((next_x, next_y)) = pending_move {
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
                        let pending_move = queued_move_clone
                            .try_write()
                            .ok()
                            .and_then(|mut pending| pending.take());
                        if let Some((next_x, next_y)) = pending_move {
                            on_move_clone.call((window_id_clone, next_x, next_y));
                            if let Ok(mut last_sent) = last_move_sent_ms_clone.try_write() {
                                *last_sent = now_ms();
                            }
                        }
                        if let Ok(mut scheduled) = move_flush_scheduled_clone.try_write() {
                            *scheduled = false;
                        }
                    });
                }
            }
            InteractionMode::Resize => {
                queued_resize.set(Some((next.width, next.height)));
                let elapsed = now_ms() - last_resize_sent_ms();
                if elapsed >= PATCH_INTERVAL_MS {
                    let pending_resize = { queued_resize.write().take() };
                    if let Some((next_w, next_h)) = pending_resize {
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
                        let pending_resize = queued_resize_clone
                            .try_write()
                            .ok()
                            .and_then(|mut pending| pending.take());
                        if let Some((next_w, next_h)) = pending_resize {
                            on_resize_clone.call((window_id_clone, next_w, next_h));
                            if let Ok(mut last_sent) = last_resize_sent_ms_clone.try_write() {
                                *last_sent = now_ms();
                            }
                        }
                        if let Ok(mut scheduled) = resize_flush_scheduled_clone.try_write() {
                            *scheduled = false;
                        }
                    });
                }
            }
        }
    });

    let pointer_up_cb = use_callback(move |e: PointerEvent| {
        let pointer_id = event_pointer_id(&e);

        if let Some(pinch) = mobile_pinch() {
            if pointer_ids_match(pinch.pointer_a_id, pointer_id)
                || pointer_ids_match(pinch.pointer_b_id, pointer_id)
            {
                release_window_pointer(&e, pointer_id);
                let final_bounds = live_bounds().unwrap_or(pinch.start_bounds);
                queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                let pending_resize = { queued_resize.write().take() };
                if let Some((next_w, next_h)) = pending_resize {
                    on_resize.call((window_id_for_pointer_up.clone(), next_w, next_h));
                    last_resize_sent_ms.set(now_ms());
                }
                resize_flush_scheduled.set(false);
                queued_resize.set(None);
                mobile_pinch_anchor.set(None);
                mobile_pinch.set(None);
                return;
            }
        }

        if let Some((anchor_id, _, _)) = mobile_pinch_anchor() {
            if pointer_ids_match(anchor_id, pointer_id) {
                mobile_pinch_anchor.set(None);
                release_window_pointer(&e, pointer_id);
                return;
            }
        }

        let Some(active) = interaction() else {
            return;
        };
        if !pointer_ids_match(active.pointer_id, pointer_id) {
            return;
        }
        release_window_pointer(&e, active.pointer_id);

        let final_bounds = live_bounds().unwrap_or(active.start_bounds);
        match active.mode {
            InteractionMode::Drag => {
                queued_move.set(Some((final_bounds.x, final_bounds.y)));
                let pending_move = { queued_move.write().take() };
                if let Some((next_x, next_y)) = pending_move {
                    on_move.call((window_id_for_pointer_up.clone(), next_x, next_y));
                    last_move_sent_ms.set(now_ms());
                }
            }
            InteractionMode::Resize => {
                queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                let pending_resize = { queued_resize.write().take() };
                if let Some((next_w, next_h)) = pending_resize {
                    on_resize.call((window_id_for_pointer_up.clone(), next_w, next_h));
                    last_resize_sent_ms.set(now_ms());
                }
            }
        }

        move_flush_scheduled.set(false);
        resize_flush_scheduled.set(false);
        queued_move.set(None);
        queued_resize.set(None);
        mobile_pinch_anchor.set(None);
        mobile_pinch.set(None);
        interaction.set(None);
    });

    let pointer_cancel_cb = use_callback(move |e: PointerEvent| {
        let pointer_id = event_pointer_id(&e);

        if let Some(pinch) = mobile_pinch() {
            if pointer_ids_match(pinch.pointer_a_id, pointer_id)
                || pointer_ids_match(pinch.pointer_b_id, pointer_id)
            {
                release_window_pointer(&e, pointer_id);
                move_flush_scheduled.set(false);
                resize_flush_scheduled.set(false);
                queued_move.set(None);
                queued_resize.set(None);
                live_bounds.set(Some(pinch.committed_bounds));
                mobile_pinch_anchor.set(None);
                mobile_pinch.set(None);
                return;
            }
        }

        if let Some((anchor_id, _, _)) = mobile_pinch_anchor() {
            if pointer_ids_match(anchor_id, pointer_id) {
                mobile_pinch_anchor.set(None);
                release_window_pointer(&e, pointer_id);
                return;
            }
        }

        let Some(active) = interaction() else {
            return;
        };
        if !pointer_ids_match(active.pointer_id, pointer_id) {
            return;
        }
        release_window_pointer(&e, active.pointer_id);

        move_flush_scheduled.set(false);
        resize_flush_scheduled.set(false);
        queued_move.set(None);
        queued_resize.set(None);
        mobile_pinch_anchor.set(None);
        mobile_pinch.set(None);
        live_bounds.set(Some(active.committed_bounds));
        interaction.set(None);
    });

    let pointer_lost_capture_cb = use_callback(move |e: PointerEvent| {
        let pointer_id = event_pointer_id(&e);

        if let Some(pinch) = mobile_pinch() {
            if pointer_ids_match(pinch.pointer_a_id, pointer_id)
                || pointer_ids_match(pinch.pointer_b_id, pointer_id)
            {
                let final_bounds = live_bounds().unwrap_or(pinch.start_bounds);
                queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                let pending_resize = { queued_resize.write().take() };
                if let Some((next_w, next_h)) = pending_resize {
                    on_resize.call((window_id_for_pointer_lost.clone(), next_w, next_h));
                    last_resize_sent_ms.set(now_ms());
                }
                move_flush_scheduled.set(false);
                resize_flush_scheduled.set(false);
                queued_move.set(None);
                queued_resize.set(None);
                mobile_pinch_anchor.set(None);
                mobile_pinch.set(None);
                return;
            }
        }

        if let Some((anchor_id, _, _)) = mobile_pinch_anchor() {
            if pointer_ids_match(anchor_id, pointer_id) {
                mobile_pinch_anchor.set(None);
                return;
            }
        }

        let Some(active) = interaction() else {
            return;
        };
        if !pointer_ids_match(active.pointer_id, pointer_id) {
            return;
        }

        let final_bounds = live_bounds().unwrap_or(active.start_bounds);
        match active.mode {
            InteractionMode::Drag => {
                queued_move.set(Some((final_bounds.x, final_bounds.y)));
                let pending_move = { queued_move.write().take() };
                if let Some((next_x, next_y)) = pending_move {
                    on_move.call((window_id_for_pointer_lost.clone(), next_x, next_y));
                    last_move_sent_ms.set(now_ms());
                }
            }
            InteractionMode::Resize => {
                queued_resize.set(Some((final_bounds.width, final_bounds.height)));
                let pending_resize = { queued_resize.write().take() };
                if let Some((next_w, next_h)) = pending_resize {
                    on_resize.call((window_id_for_pointer_lost.clone(), next_w, next_h));
                    last_resize_sent_ms.set(now_ms());
                }
            }
        }

        move_flush_scheduled.set(false);
        resize_flush_scheduled.set(false);
        queued_move.set(None);
        queued_resize.set(None);
        mobile_pinch_anchor.set(None);
        mobile_pinch.set(None);
        interaction.set(None);
    });

    let pointer_move_for_root = pointer_move_cb;
    let pointer_up_for_root = pointer_up_cb;
    let pointer_cancel_for_root = pointer_cancel_cb;
    let pointer_lost_capture_for_root = pointer_lost_capture_cb;
    let pointer_move_for_title = pointer_move_for_root.clone();
    let pointer_up_for_title = pointer_up_for_root.clone();
    let pointer_cancel_for_title = pointer_cancel_for_root.clone();
    let pointer_move_for_resize = pointer_move_for_root.clone();
    let pointer_up_for_resize = pointer_up_for_root.clone();
    let pointer_cancel_for_resize = pointer_cancel_for_root.clone();

    {
        let mut document_pointer_listeners = document_pointer_listeners;
        let mut interaction_doc = interaction;
        let mut live_bounds_doc = live_bounds;
        let mut queued_move_doc = queued_move;
        let mut queued_resize_doc = queued_resize;
        let mut move_flush_scheduled_doc = move_flush_scheduled;
        let mut resize_flush_scheduled_doc = resize_flush_scheduled;
        let mut mobile_pinch_anchor_doc = mobile_pinch_anchor;
        let mut mobile_pinch_doc = mobile_pinch;
        let mobile_mode_doc = mobile_interaction_mode;
        let on_move_doc = on_move;
        let on_resize_doc = on_resize;
        let window_id_for_pointer_up_doc = window_id_for_doc_pointer_up.clone();
        let viewport_doc = viewport;
        let is_mobile_doc = is_mobile;
        use_effect(move || {
            if document_pointer_listeners.peek().is_some() {
                return;
            }
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };
            let window_id_for_pointer_up_doc_for_move = window_id_for_pointer_up_doc.clone();
            let window_id_for_pointer_up_doc_for_resize = window_id_for_pointer_up_doc.clone();

            let pointer_move_closure =
                Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
                    if is_mobile_doc && mobile_mode_doc() == MobileInteractionMode::Resize {
                        if let Some(mut pinch) = mobile_pinch_doc() {
                            let mut matches = false;
                            if pointer_ids_match(pinch.pointer_a_id, event.pointer_id()) {
                                pinch.pointer_a_x = event.client_x();
                                pinch.pointer_a_y = event.client_y();
                                matches = true;
                            } else if pointer_ids_match(pinch.pointer_b_id, event.pointer_id()) {
                                pinch.pointer_b_x = event.client_x();
                                pinch.pointer_b_y = event.client_y();
                                matches = true;
                            }

                            if matches {
                                let distance = pointer_distance(
                                    pinch.pointer_a_x,
                                    pinch.pointer_a_y,
                                    pinch.pointer_b_x,
                                    pinch.pointer_b_y,
                                );
                                if pinch.start_distance >= 1.0 && distance >= 1.0 {
                                    let scale = distance / pinch.start_distance;
                                    let raw_width =
                                        (pinch.start_bounds.width as f64 * scale).round() as i32;
                                    let raw_height =
                                        (pinch.start_bounds.height as f64 * scale).round() as i32;
                                    let center_x =
                                        pinch.start_bounds.x + pinch.start_bounds.width / 2;
                                    let center_y =
                                        pinch.start_bounds.y + pinch.start_bounds.height / 2;
                                    let next = clamp_bounds(
                                        WindowBounds {
                                            x: center_x - raw_width / 2,
                                            y: center_y - raw_height / 2,
                                            width: raw_width,
                                            height: raw_height,
                                        },
                                        viewport_doc,
                                        is_mobile_doc,
                                    );
                                    live_bounds_doc.set(Some(next));
                                }

                                mobile_pinch_doc.set(Some(pinch));
                                return;
                            }
                        }
                    }

                    let Some(active) = interaction_doc() else {
                        return;
                    };
                    if !pointer_ids_match(active.pointer_id, event.pointer_id()) {
                        return;
                    }

                    let dx = event.client_x() - active.start_x;
                    let dy = event.client_y() - active.start_y;

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
                    let next = clamp_bounds(next, viewport_doc, is_mobile_doc);

                    live_bounds_doc.set(Some(next));
                }) as Box<dyn FnMut(web_sys::PointerEvent)>);

            let pointer_up_closure = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
                if let Some(pinch) = mobile_pinch_doc() {
                    if pointer_ids_match(pinch.pointer_a_id, event.pointer_id())
                        || pointer_ids_match(pinch.pointer_b_id, event.pointer_id())
                    {
                        let final_bounds = live_bounds_doc().unwrap_or(pinch.start_bounds);
                        on_resize_doc.call((
                            window_id_for_pointer_up_doc_for_resize.clone(),
                            final_bounds.width,
                            final_bounds.height,
                        ));
                        move_flush_scheduled_doc.set(false);
                        resize_flush_scheduled_doc.set(false);
                        queued_move_doc.set(None);
                        queued_resize_doc.set(None);
                        mobile_pinch_anchor_doc.set(None);
                        mobile_pinch_doc.set(None);
                        interaction_doc.set(None);
                        return;
                    }
                }

                if let Some((anchor_id, _, _)) = mobile_pinch_anchor_doc() {
                    if pointer_ids_match(anchor_id, event.pointer_id()) {
                        mobile_pinch_anchor_doc.set(None);
                        return;
                    }
                }

                let Some(active) = interaction_doc() else {
                    return;
                };
                if !pointer_ids_match(active.pointer_id, event.pointer_id()) {
                    return;
                }

                let final_bounds = live_bounds_doc().unwrap_or(active.start_bounds);
                match active.mode {
                    InteractionMode::Drag => {
                        on_move_doc.call((
                            window_id_for_pointer_up_doc_for_move.clone(),
                            final_bounds.x,
                            final_bounds.y,
                        ));
                    }
                    InteractionMode::Resize => {
                        on_resize_doc.call((
                            window_id_for_pointer_up_doc_for_resize.clone(),
                            final_bounds.width,
                            final_bounds.height,
                        ));
                    }
                }

                move_flush_scheduled_doc.set(false);
                resize_flush_scheduled_doc.set(false);
                queued_move_doc.set(None);
                queued_resize_doc.set(None);
                mobile_pinch_anchor_doc.set(None);
                mobile_pinch_doc.set(None);
                interaction_doc.set(None);
            })
                as Box<dyn FnMut(web_sys::PointerEvent)>);

            let pointer_cancel_closure =
                Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
                    if let Some(pinch) = mobile_pinch_doc() {
                        if pointer_ids_match(pinch.pointer_a_id, event.pointer_id())
                            || pointer_ids_match(pinch.pointer_b_id, event.pointer_id())
                        {
                            move_flush_scheduled_doc.set(false);
                            resize_flush_scheduled_doc.set(false);
                            queued_move_doc.set(None);
                            queued_resize_doc.set(None);
                            mobile_pinch_anchor_doc.set(None);
                            mobile_pinch_doc.set(None);
                            live_bounds_doc.set(Some(pinch.committed_bounds));
                            interaction_doc.set(None);
                            return;
                        }
                    }

                    if let Some((anchor_id, _, _)) = mobile_pinch_anchor_doc() {
                        if pointer_ids_match(anchor_id, event.pointer_id()) {
                            mobile_pinch_anchor_doc.set(None);
                            return;
                        }
                    }

                    let Some(active) = interaction_doc() else {
                        return;
                    };
                    if !pointer_ids_match(active.pointer_id, event.pointer_id()) {
                        return;
                    }

                    move_flush_scheduled_doc.set(false);
                    resize_flush_scheduled_doc.set(false);
                    queued_move_doc.set(None);
                    queued_resize_doc.set(None);
                    mobile_pinch_anchor_doc.set(None);
                    mobile_pinch_doc.set(None);
                    live_bounds_doc.set(Some(active.committed_bounds));
                    interaction_doc.set(None);
                }) as Box<dyn FnMut(web_sys::PointerEvent)>);

            let _ = document.add_event_listener_with_callback(
                "pointermove",
                pointer_move_closure.as_ref().unchecked_ref(),
            );
            let _ = document.add_event_listener_with_callback(
                "pointerup",
                pointer_up_closure.as_ref().unchecked_ref(),
            );
            let _ = document.add_event_listener_with_callback(
                "pointercancel",
                pointer_cancel_closure.as_ref().unchecked_ref(),
            );

            document_pointer_listeners.set(Some(DocumentPointerListeners {
                document,
                pointer_move: pointer_move_closure,
                pointer_up: pointer_up_closure,
                pointer_cancel: pointer_cancel_closure,
            }));
        });
    }

    {
        let mut document_pointer_listeners = document_pointer_listeners;
        use_drop(move || {
            if let Ok(mut listeners_slot) = document_pointer_listeners.try_write() {
                if let Some(listeners) = listeners_slot.take() {
                    listeners.detach();
                }
            }
        });
    }

    rsx! {
        div {
            class: "floating-window",
            role: "dialog",
            "aria-label": window.title.clone(),
            "aria-modal": if is_active { "false" } else { "true" },
            tabindex: "0",
            style: "{window_style}",
            onclick: move |_| {
                if !is_active {
                    on_focus.call(window_id_for_focus.clone())
                }
            },
            onkeydown: on_window_keydown,
            onpointermove: move |e| pointer_move_for_root.call(e),
            onpointerup: move |e| pointer_up_for_root.call(e),
            onpointercancel: move |e| pointer_cancel_for_root.call(e),
            onlostpointercapture: move |e| pointer_lost_capture_for_root.call(e),

            if !window.maximized {
                div {
                    class: "window-titlebar",
                    tabindex: "0",
                    style: if is_mobile {
                        "display: flex; align-items: center; justify-content: space-between; padding: 0.4rem 0.5rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); cursor: grab; user-select: none; touch-action: none;"
                    } else {
                        "display: flex; align-items: center; justify-content: space-between; padding: 0.35rem 0.75rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); cursor: grab; user-select: none; touch-action: none;"
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".to_string()) {
                            on_focus.call(window_id_for_title_key.clone());
                        }
                    },
                    onpointermove: move |e| pointer_move_for_title.call(e),
                    onpointerup: move |e| pointer_up_for_title.call(e),
                    onpointercancel: move |e| pointer_cancel_for_title.call(e),
                    oncontextmenu: move |e| {
                        if let Some(active) = interaction() {
                            e.prevent_default();
                            move_flush_scheduled.set(false);
                            resize_flush_scheduled.set(false);
                            queued_move.set(None);
                            queued_resize.set(None);
                            live_bounds.set(Some(active.committed_bounds));
                            interaction.set(None);
                        }
                    },
                    onpointerdown: move |e| {
                        if window.maximized {
                            return;
                        }
                        if pointer_target_is_window_control(&e) {
                            return;
                        }
                        if !is_active {
                            on_focus.call(window_id_for_title_pointer.clone());
                        }
                        e.prevent_default();
                        let pointer_id = event_pointer_id(&e);
                        capture_window_pointer(&e, pointer_id);

                        let (start_x, start_y) = pointer_point(&e);

                        interaction.set(Some(InteractionState {
                            mode: InteractionMode::Drag,
                            pointer_id,
                            start_x,
                            start_y,
                            start_bounds: bounds,
                            committed_bounds: committed,
                        }));
                    },

                    div {
                        style: if is_mobile {
                            "display: flex; align-items: center; gap: 0.35rem; min-width: 0;"
                        } else {
                            "display: flex; align-items: center; gap: 0.5rem;"
                        },
                        span { style: if is_mobile { "font-size: 0.9rem;" } else { "font-size: 1rem;" }, {get_app_icon(&window.app_id)} }
                        span { style: if is_mobile { "font-weight: 500; color: var(--text-primary, white); font-size: 0.92rem; max-width: 36vw; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" } else { "font-weight: 500; color: var(--text-primary, white);" }, "{window.title}" }
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
                    "trace" => rsx! {
                        TraceView {
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
                    _ => rsx! {
                        div {
                            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280); padding: 1rem;",
                            "App not yet implemented"
                        }
                    }
                }
                }
            }

            if !window.maximized {
                div {
                    class: "resize-handle",
                    style: "position: absolute; right: 0; bottom: 0; width: 24px; height: 24px; cursor: se-resize; touch-action: none; z-index: 30; background: linear-gradient(135deg, transparent 58%, color-mix(in srgb, var(--text-secondary, #94a3b8) 45%, transparent) 58%); border-bottom-right-radius: var(--radius-lg, 12px);",
                    onpointermove: move |e| pointer_move_for_resize.call(e),
                    onpointerup: move |e| pointer_up_for_resize.call(e),
                    onpointercancel: move |e| pointer_cancel_for_resize.call(e),
                    oncontextmenu: move |e| {
                        if let Some(active) = interaction() {
                            e.prevent_default();
                            move_flush_scheduled.set(false);
                            resize_flush_scheduled.set(false);
                            queued_move.set(None);
                            queued_resize.set(None);
                            live_bounds.set(Some(active.committed_bounds));
                            interaction.set(None);
                        }
                    },
                    onpointerdown: move |e| {
                        if !is_active {
                            on_focus.call(window_id_for_resize_pointer.clone());
                        }
                        e.prevent_default();
                        let pointer_id = event_pointer_id(&e);
                        capture_window_pointer(&e, pointer_id);

                        let (start_x, start_y) = pointer_point(&e);
                        interaction.set(Some(InteractionState {
                            mode: InteractionMode::Resize,
                            pointer_id,
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
    let container_style = if floating {
        "position: absolute; top: 0.5rem; left: 0.5rem; z-index: 10; display: flex; align-items: center; gap: 0.25rem;"
    } else if mobile {
        "display: flex; align-items: center; gap: 0.25rem; margin-left: auto; padding-right: 0.25rem;"
    } else {
        "display: flex; align-items: center; gap: 0.25rem;"
    };
    let action_row_style = if floating {
        "display: flex; align-items: center; gap: 0.2rem; background: color-mix(in srgb, var(--titlebar-bg, #111827) 35%, transparent); border: 1px solid var(--border-color, #374151); border-radius: 999px; padding: 0.15rem 0.25rem;"
    } else {
        "display: flex; align-items: center; gap: 0.25rem;"
    };
    let control_size = if mobile { "22px" } else { "24px" };
    let close_size = if mobile { "22px" } else { "24px" };

    rsx! {
        div {
            class: if floating { "window-controls window-controls-floating" } else { "window-controls" },
            style: "{container_style}",

            div {
                style: "{action_row_style}",
                button {
                    style: "width: {control_size}; height: {control_size}; display: flex; align-items: center; justify-content: center; background: transparent; color: #facc15; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer;",
                    onpointerdown: move |e| e.stop_propagation(),
                    "aria-label": "Minimize",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_minimize.call(window_id_for_minimize.clone());
                    },
                    "−"
                }
                button {
                    style: "width: {control_size}; height: {control_size}; display: flex; align-items: center; justify-content: center; background: transparent; color: #22c55e; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer;",
                    onpointerdown: move |e| e.stop_propagation(),
                    "aria-label": if maximized { "Restore" } else { "Maximize" },
                    onclick: move |e| {
                        e.stop_propagation();
                        if maximized {
                            on_restore.call(window_id_for_max_restore.clone());
                        } else {
                            on_maximize.call(window_id_for_max_restore.clone());
                        }
                    },
                    if maximized { "❐" } else { "□" }
                }
                button {
                    class: "window-close",
                    style: "width: {close_size}; height: {close_size}; display: flex; align-items: center; justify-content: center; background: transparent; color: #ef4444; border: none; border-radius: var(--radius-sm, 4px); cursor: pointer; font-size: 1.2rem; line-height: 1;",
                    onpointerdown: move |e| e.stop_propagation(),
                    "aria-label": "Close",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_close.call(window_id_for_close.clone());
                    },
                    "×"
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
        "trace" => "🔍",
        "settings" => "⚙️",
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

    #[test]
    fn clamp_mobile_preserves_resized_dimensions() {
        let resized = clamp_bounds(
            WindowBounds {
                x: 20,
                y: 40,
                width: 300,
                height: 520,
            },
            (390, 844),
            true,
        );

        assert_eq!(resized.width, 300);
        assert_eq!(resized.height, 520);
    }

    #[test]
    fn pointer_ids_match_is_strict() {
        assert!(pointer_ids_match(1, 1));
        assert!(!pointer_ids_match(1, 2));
        assert!(!pointer_ids_match(0, 1));
        assert!(!pointer_ids_match(1, 0));
        assert!(pointer_ids_match(0, 0));
        assert!(!pointer_ids_match(-1, 1));
    }
}
