use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{window, MouseEvent};

/// Get the browser viewport dimensions
pub fn get_viewport_size() -> (u32, u32) {
    let window = window().expect("no global `window` exists");
    let width = window
        .inner_width()
        .expect("failed to get inner width")
        .as_f64()
        .unwrap_or(0.0) as u32;
    let height = window
        .inner_height()
        .expect("failed to get inner height")
        .as_f64()
        .unwrap_or(0.0) as u32;
    (width, height)
}

/// Get the current workspace canvas size in CSS pixels.
pub fn get_window_canvas_size() -> Option<(i32, i32)> {
    let window = window()?;
    let document = window.document()?;
    let canvas = document.query_selector(".window-canvas").ok()??;
    let width = canvas.client_width();
    let height = canvas.client_height();
    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

/// Start dragging a window. Calls `on_move` with delta (dx, dy) on each mouse move.
pub fn start_window_drag(window_id: String, on_move: Callback<(i32, i32)>) {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("no document on window");

    let initial_x = std::cell::Cell::new(0i32);
    let initial_y = std::cell::Cell::new(0i32);
    let is_dragging = std::cell::Cell::new(false);

    // Mouse down handler
    let window_id_clone = window_id.clone();
    let mousedown_closure = Closure::wrap(Box::new(move |e: MouseEvent| {
        let target = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(target) = target {
            if target.id() == window_id_clone {
                initial_x.set(e.client_x());
                initial_y.set(e.client_y());
                is_dragging.set(true);
            }
        }
    }) as Box<dyn FnMut(MouseEvent)>);

    // Mouse move handler
    let on_move_clone = on_move;
    let window_id_clone = window_id.clone();
    let mousemove_closure = Closure::wrap(Box::new(move |e: MouseEvent| {
        let target = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(target) = target {
            if target.id() == window_id_clone {
                on_move_clone.call((e.client_x(), e.client_y()));
            }
        }
    }) as Box<dyn FnMut(MouseEvent)>);

    // Mouse up handler
    let mouseup_closure = Closure::wrap(Box::new(move |_e: MouseEvent| {
        // Dragging ends on mouse up
    }) as Box<dyn FnMut(MouseEvent)>);

    document
        .add_event_listener_with_callback("mousedown", mousedown_closure.as_ref().unchecked_ref())
        .expect("failed to add mousedown listener");
    document
        .add_event_listener_with_callback("mousemove", mousemove_closure.as_ref().unchecked_ref())
        .expect("failed to add mousemove listener");
    document
        .add_event_listener_with_callback("mouseup", mouseup_closure.as_ref().unchecked_ref())
        .expect("failed to add mouseup listener");

    // Leak the closures to keep them alive (they will be cleaned up when the page unloads)
    mousedown_closure.forget();
    mousemove_closure.forget();
    mouseup_closure.forget();
}

/// Start resizing a window. Calls `on_resize` with new (width, height) on each mouse move.
pub fn start_window_resize(window_id: String, on_resize: Callback<(i32, i32)>) {
    // TODO: Implement resize via JS interop
    let _ = (window_id, on_resize);
}

/// Connect to a WebSocket and forward messages to the callback
pub async fn connect_websocket(_url: &str, _on_message: Callback<String>) -> Result<(), JsValue> {
    // TODO: Implement WebSocket connection
    // Stub for now
    Ok(())
}
