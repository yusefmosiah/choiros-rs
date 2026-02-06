use std::cell::Cell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::api_base;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = createTerminal)]
    fn create_terminal(container: web_sys::Element) -> u32;

    #[wasm_bindgen(js_name = onTerminalData)]
    fn on_terminal_data(id: u32, cb: &Closure<dyn FnMut(String)>);

    #[wasm_bindgen(js_name = writeTerminal)]
    fn write_terminal(id: u32, data: &str);

    #[wasm_bindgen(js_name = fitTerminal)]
    fn fit_terminal(id: u32) -> js_sys::Array;

    #[wasm_bindgen(js_name = resizeTerminal)]
    fn resize_terminal(id: u32, rows: u16, cols: u16);

    #[wasm_bindgen(js_name = disposeTerminal)]
    fn dispose_terminal(id: u32);
}

struct TerminalRuntime {
    term_id: u32,
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_data: Closure<dyn FnMut(String)>,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

enum TerminalWsEvent {
    Opened,
    Message(String),
    Error(String),
    Closed,
}

#[component]
pub fn TerminalView(terminal_id: String, width: i32, height: i32) -> Element {
    let container_id = format!("terminal-container-{}", terminal_id);
    let mut runtime = use_signal(|| None::<TerminalRuntime>);
    let mut terminal_ready = use_signal(|| false);
    let mut status = use_signal(|| "Connecting...".to_string());
    let mut error = use_signal(|| None::<String>);
    let mut reconnect_attempts = use_signal(|| 0u32);
    let mut reconnect_timeout_id = use_signal(|| None::<i32>);
    let reconnect_nonce = use_signal(|| 0u64);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<TerminalWsEvent>::new())));
    let pending_output_chunks = use_hook(|| Rc::new(RefCell::new(VecDeque::<String>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));

    {
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_drop(move || {
            ws_event_pump_alive.set(false);
        });
    }

    {
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                if let Some(timeout_id) = *reconnect_timeout_id.read() {
                    window.clear_timeout_with_handle(timeout_id);
                }
            }
        });
    }

    {
        let ws_event_queue = ws_event_queue.clone();
        let pending_output_chunks = pending_output_chunks.clone();
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_effect(move || {
            if ws_event_pump_started() {
                return;
            }
            ws_event_pump_started.set(true);

            let ws_event_queue = ws_event_queue.clone();
            let pending_output_chunks = pending_output_chunks.clone();
            let ws_event_pump_alive = ws_event_pump_alive.clone();
            spawn(async move {
                while ws_event_pump_alive.get() {
                    let mut drained = Vec::new();
                    {
                        let mut queue = ws_event_queue.borrow_mut();
                        while let Some(event) = queue.pop_front() {
                            drained.push(event);
                        }
                    }

                    if terminal_ready() {
                        if let Some(term_id) = runtime.read().as_ref().map(|rt| rt.term_id) {
                            let mut pending = pending_output_chunks.borrow_mut();
                            while let Some(chunk) = pending.pop_front() {
                                write_terminal(term_id, &chunk);
                            }
                        }
                    }

                    for event in drained {
                        match event {
                            TerminalWsEvent::Opened => {
                                terminal_ready.set(false);
                                if let Some(window) = web_sys::window() {
                                    let existing_timeout = *reconnect_timeout_id.read();
                                    if let Some(timeout_id) = existing_timeout {
                                        window.clear_timeout_with_handle(timeout_id);
                                        reconnect_timeout_id.set(None);
                                    }
                                }
                                status.set("Connected".to_string());
                                reconnect_attempts.set(0);
                                error.set(None);

                                if let Some(rt) = runtime.read().as_ref() {
                                    if let Some((rows, cols)) = fit_and_get_size(rt.term_id) {
                                        let _ = send_resize(&rt.ws, rows, cols);
                                    }
                                }
                                terminal_ready.set(true);

                                if let Some(term_id) = runtime.read().as_ref().map(|rt| rt.term_id) {
                                    let mut pending = pending_output_chunks.borrow_mut();
                                    while let Some(chunk) = pending.pop_front() {
                                        write_terminal(term_id, &chunk);
                                    }
                                }
                            }
                            TerminalWsEvent::Message(text_str) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str)
                                {
                                    if let Some(msg_type) = json.get("type").and_then(|t| t.as_str())
                                    {
                                        match msg_type {
                                            "output" => {
                                                if let Some(data) =
                                                    json.get("data").and_then(|v| v.as_str())
                                                {
                                                    if terminal_ready() {
                                                        if let Some(rt) = runtime.read().as_ref() {
                                                            write_terminal(rt.term_id, data);
                                                        } else {
                                                            let mut pending =
                                                                pending_output_chunks.borrow_mut();
                                                            pending.push_back(data.to_string());
                                                            while pending.len() > 4096 {
                                                                pending.pop_front();
                                                            }
                                                        }
                                                    } else {
                                                        let mut pending =
                                                            pending_output_chunks.borrow_mut();
                                                        pending.push_back(data.to_string());
                                                        while pending.len() > 4096 {
                                                            pending.pop_front();
                                                        }
                                                    }
                                                }
                                            }
                                            "info" => {
                                                let is_running = json
                                                    .get("is_running")
                                                    .and_then(|v| v.as_bool())
                                                    .unwrap_or(false);
                                                if is_running {
                                                    status.set("Connected".to_string());
                                                } else {
                                                    status.set("Stopped".to_string());
                                                }
                                            }
                                            "error" => {
                                                if let Some(message) =
                                                    json.get("message").and_then(|v| v.as_str())
                                                {
                                                    dioxus_logger::tracing::error!(
                                                        "Terminal WS error: {}",
                                                        message
                                                    );
                                                    error.set(Some(message.to_string()));
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            TerminalWsEvent::Error(message) => {
                                terminal_ready.set(false);
                                status.set("Disconnected".to_string());
                                error.set(Some(format!("WebSocket error: {}", message)));
                            }
                            TerminalWsEvent::Closed => {
                                terminal_ready.set(false);
                                status.set("Disconnected".to_string());
                                let status_for_reconnect = status.clone();
                                schedule_reconnect(
                                    reconnect_attempts,
                                    reconnect_timeout_id,
                                    reconnect_nonce,
                                    runtime,
                                    status_for_reconnect,
                                    error,
                                );
                            }
                        }
                    }

                    TimeoutFuture::new(16).await;
                }
            });
        });
    }

    // Initialize xterm + websocket once
    let container_id_for_effect = container_id.clone();
    {
        let ws_event_queue = ws_event_queue.clone();
        use_effect(move || {
            let _ = reconnect_nonce();
            if runtime.read().is_some() {
                return;
            }
            terminal_ready.set(false);

            let container_id_inner = container_id_for_effect.clone();
            let terminal_id_inner = terminal_id.clone();
            let ws_event_queue_outer = ws_event_queue.clone();
            spawn(async move {
            if let Err(e) = ensure_terminal_scripts().await {
                error.set(Some(format!("Failed to load terminal scripts: {:?}", e)));
                return;
            }

            let container = match wait_for_terminal_container(&container_id_inner, 40, 50).await {
                Some(container) => container,
                None => {
                    error.set(Some("Terminal container not ready".to_string()));
                    schedule_reconnect(
                        reconnect_attempts,
                        reconnect_timeout_id,
                        reconnect_nonce,
                        runtime,
                        status,
                        error,
                    );
                    return;
                }
            };

            let term_id = create_terminal(container);
            if term_id == 0 {
                error.set(Some("Terminal bridge init failed".to_string()));
                schedule_reconnect(
                    reconnect_attempts,
                    reconnect_timeout_id,
                    reconnect_nonce,
                    runtime,
                    status,
                    error,
                );
                return;
            }
            let ws_url = build_ws_url(&terminal_id_inner);
            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    error.set(Some(format!("WebSocket error: {:?}", e)));
                    schedule_reconnect(
                        reconnect_attempts,
                        reconnect_timeout_id,
                        reconnect_nonce,
                        runtime,
                        status,
                        error,
                    );
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let ws_for_data = ws.clone();
            let on_data = Closure::wrap(Box::new(move |data: String| {
                let msg = serde_json::json!({
                    "type": "input",
                    "data": data,
                });
                let _ = ws_for_data.send_with_str(&msg.to_string());
            }) as Box<dyn FnMut(String)>);
            on_terminal_data(term_id, &on_data);

            let ws_event_queue_open = ws_event_queue_outer.clone();
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                ws_event_queue_open
                    .borrow_mut()
                    .push_back(TerminalWsEvent::Opened);
            }) as Box<dyn FnMut(Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let ws_event_queue_message = ws_event_queue_outer.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                    return;
                };
                let text_str = text.as_string().unwrap_or_default();
                ws_event_queue_message
                    .borrow_mut()
                    .push_back(TerminalWsEvent::Message(text_str));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let ws_event_queue_error = ws_event_queue_outer.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                ws_event_queue_error
                    .borrow_mut()
                    .push_back(TerminalWsEvent::Error(e.message()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let ws_event_queue_close = ws_event_queue_outer.clone();
            let closing_for_close = closing.clone();
            let on_close = Closure::wrap(Box::new(move |_e: CloseEvent| {
                if closing_for_close.get() {
                    return;
                }
                ws_event_queue_close
                    .borrow_mut()
                    .push_back(TerminalWsEvent::Closed);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            runtime.set(Some(TerminalRuntime {
                term_id,
                ws,
                closing,
                _on_data: on_data,
                _on_open: on_open,
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            }));
            });
        });
    }

    // Re-fit on window size changes
    use_effect(move || {
        let _ = (width, height);
        if let Some(rt) = runtime.read().as_ref() {
            if let Some((rows, cols)) = fit_and_get_size(rt.term_id) {
                let _ = send_resize(&rt.ws, rows, cols);
            }
        }
    });

    rsx! {
        style { {TERMINAL_STYLES} }
        div {
            class: "terminal-root",
            div {
                class: "terminal-status",
                "{status}"
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "terminal-error", "{err}" }
            }
            div {
                class: "terminal-container",
                id: "{container_id}",
            }
        }
    }
}

fn build_ws_url(terminal_id: &str) -> String {
    let ws_base = http_to_ws_url(api_base());
    format!("{}/ws/terminal/{}?user_id=user-1", ws_base, terminal_id)
}

fn http_to_ws_url(http_url: &str) -> String {
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

fn fit_and_get_size(term_id: u32) -> Option<(u16, u16)> {
    let size = fit_terminal(term_id);
    let mut rows = size
        .get(0)
        .as_f64()
        .unwrap_or(0.0)
        .clamp(0.0, u16::MAX as f64) as u16;
    let mut cols = size
        .get(1)
        .as_f64()
        .unwrap_or(0.0)
        .clamp(0.0, u16::MAX as f64) as u16;

    if rows < 2 || cols < 2 {
        rows = 24;
        cols = 80;
        resize_terminal(term_id, rows, cols);
    }

    Some((rows, cols))
}

fn send_resize(ws: &WebSocket, rows: u16, cols: u16) -> Result<(), JsValue> {
    if ws.ready_state() != WebSocket::OPEN {
        return Ok(());
    }

    let msg = serde_json::json!({
        "type": "resize",
        "rows": rows,
        "cols": cols,
    });
    ws.send_with_str(&msg.to_string())
}

async fn ensure_terminal_scripts() -> Result<(), JsValue> {
    ensure_script("xterm-js", "/xterm.js")?;
    wait_for_js_global("Terminal", 30, 100).await?;

    ensure_script("xterm-addon-fit-js", "/xterm-addon-fit.js")?;
    wait_for_js_global("FitAddon", 30, 100).await?;

    ensure_script("terminal-bridge-js", "/terminal.js")?;
    wait_for_js_global("createTerminal", 30, 100).await?;

    Ok(())
}

fn ensure_script(id: &str, src: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;

    if document.get_element_by_id(id).is_some() {
        return Ok(());
    }

    let script: web_sys::HtmlScriptElement = document
        .create_element("script")?
        .dyn_into::<web_sys::HtmlScriptElement>()?;
    script.set_id(id);
    script.set_src(src);
    script.set_async(false);

    if let Some(head) = document.head() {
        head.append_child(&script)?;
    } else if let Some(body) = document.body() {
        body.append_child(&script)?;
    }

    Ok(())
}

fn has_js_global(name: &str) -> bool {
    let global = js_sys::global();
    js_sys::Reflect::has(&global, &JsValue::from_str(name)).unwrap_or(false)
}

async fn wait_for_js_global(name: &str, attempts: usize, delay_ms: i32) -> Result<(), JsValue> {
    for _ in 0..attempts {
        if has_js_global(name) {
            return Ok(());
        }
        sleep_ms(delay_ms).await;
    }

    Err(JsValue::from_str(&format!(
        "Global '{name}' not available after waiting"
    )))
}

async fn sleep_ms(ms: i32) {
    TimeoutFuture::new(ms as u32).await;
}

const TERMINAL_STYLES: &str = r#"
.terminal-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #0b1020;
}

.terminal-status {
    font-size: 0.75rem;
    color: #94a3b8;
    padding: 0.25rem 0.75rem;
    border-bottom: 1px solid #1f2937;
}

.terminal-error {
    font-size: 0.75rem;
    color: #f87171;
    padding: 0.25rem 0.75rem;
    border-bottom: 1px solid #1f2937;
}

.terminal-container {
    flex: 1;
    width: 100%;
    height: 100%;
}
"#;

impl Drop for TerminalRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
        dispose_terminal(self.term_id);
    }
}

fn schedule_reconnect(
    mut reconnect_attempts: Signal<u32>,
    mut reconnect_timeout_id: Signal<Option<i32>>,
    reconnect_nonce: Signal<u64>,
    mut runtime: Signal<Option<TerminalRuntime>>,
    mut status: Signal<String>,
    error: Signal<Option<String>>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let attempt = reconnect_attempts() + 1;
    let max_attempts = 6;
    if attempt > max_attempts {
        status.set("Disconnected".to_string());
        return;
    }
    reconnect_attempts.set(attempt);
    status.set(format!("Reconnecting... ({}/{})", attempt, max_attempts));

    if let Some(timeout_id) = *reconnect_timeout_id.read() {
        window.clear_timeout_with_handle(timeout_id);
    }

    let base_delay = (500u32.saturating_mul(2u32.saturating_pow(attempt.min(6)))).min(8000) as i32;
    let jitter = (js_sys::Math::random() * 0.4 + 0.8) as f64;
    let delay_ms = (base_delay as f64 * jitter).round() as i32;

    let mut reconnect_timeout_id_timeout = reconnect_timeout_id;
    let mut reconnect_nonce_timeout = reconnect_nonce;
    let timeout_cb = Closure::wrap(Box::new(move || {
        reconnect_timeout_id_timeout.set(None);
        runtime.set(None);
        let current = reconnect_nonce_timeout();
        reconnect_nonce_timeout.set(current.saturating_add(1));
        let _ = &error;
    }) as Box<dyn FnMut()>);

    if let Ok(timeout_id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        timeout_cb.as_ref().unchecked_ref(),
        delay_ms,
    ) {
        reconnect_timeout_id.set(Some(timeout_id));
    }

    timeout_cb.forget();
}

async fn wait_for_terminal_container(
    container_id: &str,
    max_attempts: usize,
    delay_ms: i32,
) -> Option<web_sys::Element> {
    for _ in 0..max_attempts {
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(container_id))
        {
            return Some(element);
        }
        sleep_ms(delay_ms).await;
    }
    None
}
