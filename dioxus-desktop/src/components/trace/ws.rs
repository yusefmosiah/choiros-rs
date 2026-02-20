use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

// ── WebSocket event type ─────────────────────────────────────────────────────

pub enum TraceWsEvent {
    Connected,
    Message(String),
    Error(String),
    Closed,
}

// ── WebSocket runtime ────────────────────────────────────────────────────────

pub struct TraceRuntime {
    pub ws: WebSocket,
    pub closing: Rc<Cell<bool>>,
    pub _on_open: Closure<dyn FnMut(Event)>,
    pub _on_message: Closure<dyn FnMut(MessageEvent)>,
    pub _on_error: Closure<dyn FnMut(ErrorEvent)>,
    pub _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl Drop for TraceRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

// ── URL helpers ──────────────────────────────────────────────────────────────

pub fn build_trace_ws_url(since_seq: i64) -> String {
    let ws_base = http_to_ws_url(crate::api::api_base());
    format!(
        "{}/ws/logs/events?since_seq={}&limit=300&poll_ms=200",
        ws_base,
        since_seq.max(0)
    )
}

pub fn http_to_ws_url(http_url: &str) -> String {
    if http_url.starts_with("http://") {
        http_url.replace("http://", "ws://")
    } else if http_url.starts_with("https://") {
        http_url.replace("https://", "wss://")
    } else if http_url.is_empty() {
        let protocol = web_sys::window()
            .and_then(|window| window.location().protocol().ok())
            .unwrap_or_else(|| "http:".to_string());
        let host = web_sys::window()
            .and_then(|window| window.location().host().ok())
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
