use dioxus::prelude::*;
use futures::stream::StreamExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message as WsMessage;
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;
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
        let target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(target) = target {
            if target.id() == window_id_clone {
                initial_x.set(e.client_x());
                initial_y.set(e.client_y());
                is_dragging.set(true);
            }
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    
    // Mouse move handler
    let on_move_clone = on_move.clone();
    let window_id_clone = window_id.clone();
    let mousemove_closure = Closure::wrap(Box::new(move |e: MouseEvent| {
        if is_dragging.get() {
            let current_x = e.client_x();
            let current_y = e.client_y();
            let dx = current_x - initial_x.get();
            let dy = current_y - initial_y.get();
            
            on_move_clone.call((dx, dy));
            
            initial_x.set(current_x);
            initial_y.set(current_y);
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    
    // Mouse up handler
    let is_dragging_clone = is_dragging.clone();
    let mouseup_closure = Closure::wrap(Box::new(move |_e: MouseEvent| {
        is_dragging_clone.set(false);
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
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("no document on window");
    
    let initial_x = std::cell::Cell::new(0i32);
    let initial_y = std::cell::Cell::new(0i32);
    let initial_width = std::cell::Cell::new(0i32);
    let initial_height = std::cell::Cell::new(0i32);
    let is_resizing = std::cell::Cell::new(false);
    
    // Mouse down handler - start resize on resize handle
    let window_id_clone = window_id.clone();
    let mousedown_closure = Closure::wrap(Box::new(move |e: MouseEvent| {
        let target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(target) = target {
            // Check if the target is a resize handle for this window
            let resize_handle_id = format!("{}-resize", window_id_clone);
            if target.id() == resize_handle_id {
                initial_x.set(e.client_x());
                initial_y.set(e.client_y());
                
                // Get current window dimensions
                if let Some(window_element) = document.get_element_by_id(&window_id_clone) {
                    let rect = window_element.get_bounding_client_rect();
                    initial_width.set(rect.width() as i32);
                    initial_height.set(rect.height() as i32);
                }
                
                is_resizing.set(true);
            }
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    
    // Mouse move handler
    let on_resize_clone = on_resize.clone();
    let initial_x_clone = initial_x.clone();
    let initial_y_clone = initial_y.clone();
    let initial_width_clone = initial_width.clone();
    let initial_height_clone = initial_height.clone();
    let is_resizing_clone = is_resizing.clone();
    
    let mousemove_closure = Closure::wrap(Box::new(move |e: MouseEvent| {
        if is_resizing_clone.get() {
            let dx = e.client_x() - initial_x_clone.get();
            let dy = e.client_y() - initial_y_clone.get();
            
            let new_width = initial_width_clone.get() + dx;
            let new_height = initial_height_clone.get() + dy;
            
            // Ensure minimum dimensions
            let new_width = new_width.max(100);
            let new_height = new_height.max(100);
            
            on_resize_clone.call((new_width, new_height));
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    
    // Mouse up handler
    let is_resizing_clone = is_resizing.clone();
    let mouseup_closure = Closure::wrap(Box::new(move |_e: MouseEvent| {
        is_resizing_clone.set(false);
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
    
    // Leak the closures to keep them alive
    mousedown_closure.forget();
    mousemove_closure.forget();
    mouseup_closure.forget();
}

/// Connect to a WebSocket and forward messages to the callback
pub async fn connect_websocket(url: &str, on_message: Callback<String>) -> Result<(), JsValue> {
    let ws = WebSocket::open(url).map_err(|e| JsValue::from_str(&format!("WebSocket error: {:?}", e)))?;
    
    let (mut _write, mut read) = ws.split();
    
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(result) = read.next().await {
            match result {
                Ok(WsMessage::Text(text)) => {
                    on_message.call(text);
                }
                Ok(WsMessage::Bytes(_)) => {
                    // Handle binary messages if needed
                }
                Err(e) => {
                    log::error!("WebSocket error: {:?}", e);
                    break;
                }
            }
        }
    });
    
    Ok(())
}
