use std::cell::RefCell;
use std::rc::Rc;

use shared_types::{DesktopState, WindowState};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    Disconnected,
    DesktopStateUpdate(DesktopState),
    WindowOpened(WindowState),
    WindowClosed(String),
    WindowMoved {
        window_id: String,
        x: i32,
        y: i32,
    },
    WindowResized {
        window_id: String,
        width: i32,
        height: i32,
    },
    WindowFocused(String),
    WindowMinimized(String),
    WindowMaximized {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    WindowRestored {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        from: String,
    },
    Pong,
    Error(String),
}

pub fn http_to_ws_url(http_url: &str) -> String {
    if http_url.starts_with("http://") {
        http_url.replace("http://", "ws://")
    } else if http_url.starts_with("https://") {
        http_url.replace("https://", "wss://")
    } else if http_url.is_empty() {
        let protocol = web_sys::window()
            .and_then(|w| w.location().protocol().ok())
            .unwrap_or_else(|| "http:".to_string());
        let host = web_sys::window()
            .and_then(|w| w.location().host().ok())
            .unwrap_or_else(|| "localhost".to_string());

        if protocol == "https:" {
            format!("wss://{host}")
        } else {
            format!("ws://{host}")
        }
    } else {
        format!("ws://{http_url}")
    }
}

pub fn parse_ws_message(payload: &str) -> Option<WsEvent> {
    let json = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let msg_type = json.get("type")?.as_str()?;

    match msg_type {
        "pong" => Some(WsEvent::Pong),
        "desktop_state" => {
            serde_json::from_value::<DesktopState>(json.get("desktop").cloned().unwrap_or_default())
                .ok()
                .map(WsEvent::DesktopStateUpdate)
        }
        "window_opened" => {
            serde_json::from_value::<WindowState>(json.get("window").cloned().unwrap_or_default())
                .ok()
                .map(WsEvent::WindowOpened)
        }
        "window_closed" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowClosed(window_id.to_string())),
        "window_moved" => {
            if let (Some(window_id), Some(x), Some(y)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowMoved {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                })
            } else {
                None
            }
        }
        "window_resized" => {
            if let (Some(window_id), Some(width), Some(height)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowResized {
                    window_id: window_id.to_string(),
                    width: width as i32,
                    height: height as i32,
                })
            } else {
                None
            }
        }
        "window_focused" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowFocused(window_id.to_string())),
        "window_minimized" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowMinimized(window_id.to_string())),
        "window_maximized" => {
            if let (Some(window_id), Some(x), Some(y), Some(width), Some(height)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowMaximized {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                    width: width as i32,
                    height: height as i32,
                })
            } else {
                None
            }
        }
        "window_restored" => {
            if let (Some(window_id), Some(x), Some(y), Some(width), Some(height), Some(from)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
                json.get("from").and_then(|v| v.as_str()),
            ) {
                Some(WsEvent::WindowRestored {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                    width: width as i32,
                    height: height as i32,
                    from: from.to_string(),
                })
            } else {
                None
            }
        }
        "error" => json
            .get("message")
            .and_then(|v| v.as_str())
            .map(|message| WsEvent::Error(message.to_string())),
        _ => None,
    }
}

pub async fn connect_websocket<F>(desktop_id: &str, mut on_event: F)
where
    F: FnMut(WsEvent) + 'static,
{
    let api_base = crate::api::api_base();
    let ws_base = http_to_ws_url(api_base);
    let ws_url = format!("{ws_base}/ws");

    dioxus_logger::tracing::info!("Connecting to WebSocket: {}", ws_url);

    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            dioxus_logger::tracing::error!("Failed to create WebSocket: {:?}", e);
            on_event(WsEvent::Disconnected);
            return;
        }
    };

    let on_event_rc = Rc::new(RefCell::new(on_event));
    let on_event_open = on_event_rc.clone();
    let on_event_close = on_event_rc.clone();
    let desktop_id_clone = desktop_id.to_string();
    let ws_clone = ws.clone();

    let onopen_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::info!("WebSocket connected");
        on_event_open.borrow_mut()(WsEvent::Connected);

        let subscribe_msg =
            format!("{{\"type\":\"subscribe\",\"desktop_id\":\"{desktop_id_clone}\"}}");
        let _ = ws_clone.send_with_str(&subscribe_msg);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
            let text_str = text.as_string().unwrap_or_default();
            dioxus_logger::tracing::debug!("WebSocket message: {}", text_str);

            if let Some(event) = parse_ws_message(&text_str) {
                match &event {
                    WsEvent::Pong => {
                        dioxus_logger::tracing::debug!("WebSocket pong received");
                    }
                    WsEvent::Error(message) => {
                        dioxus_logger::tracing::error!("WebSocket error message: {}", message);
                    }
                    _ => {}
                }
                on_event_rc.borrow_mut()(event);
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    let onclose_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::info!("WebSocket disconnected");
        on_event_close.borrow_mut()(WsEvent::Disconnected);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
    onclose_callback.forget();

    let onerror_callback = Closure::wrap(Box::new(move |e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::error!("WebSocket error: {:?}", e);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();
}
