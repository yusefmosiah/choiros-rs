//! Run View Component
//!
//! Live streaming document view for conductor runs. Displays the run document
//! as it updates in real-time via WebSocket events, with a collapsible raw
//! events section below.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::writer_preview;
use crate::viewers::MarkdownViewer;

/// A raw event entry for the collapsible events section
#[derive(Clone, Debug)]
pub struct RawEvent {
    pub timestamp: String,
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// State for the run view
#[derive(Clone, Debug)]
pub struct RunViewState {
    pub run_id: String,
    pub document_path: String,
    pub document_content: String,
    pub rendered_html: String,
    pub raw_events: Vec<RawEvent>,
    pub connected: bool,
    pub error: Option<String>,
}

impl RunViewState {
    pub fn new(run_id: String, document_path: String) -> Self {
        Self {
            run_id,
            document_path,
            document_content: String::new(),
            rendered_html: String::new(),
            raw_events: Vec::new(),
            connected: false,
            error: None,
        }
    }

    pub fn update_document(&mut self, content_excerpt: String) {
        // Append the new content excerpt to the document
        if !self.document_content.is_empty() {
            self.document_content.push('\n');
        }
        self.document_content.push_str(&content_excerpt);
    }

    pub fn add_raw_event(&mut self, event: RawEvent) {
        self.raw_events.push(event);
        // Keep only the most recent 100 events to prevent memory bloat
        if self.raw_events.len() > 100 {
            self.raw_events.remove(0);
        }
    }
}

/// WebSocket event types for the run view
enum RunWsEvent {
    Connected,
    Message(String),
    Error(String),
    Closed,
}

type RunWsQueue = Rc<RefCell<VecDeque<RunWsEvent>>>;

fn enqueue_run_ws_event(queue: &RunWsQueue, event: RunWsEvent) {
    if let Ok(mut pending) = queue.try_borrow_mut() {
        pending.push_back(event);
        return;
    }

    let queue = queue.clone();
    spawn(async move {
        let mut deferred = Some(event);
        for _ in 0..3 {
            TimeoutFuture::new(1).await;
            if let Ok(mut pending) = queue.try_borrow_mut() {
                if let Some(event) = deferred.take() {
                    pending.push_back(event);
                }
                return;
            }
        }
        dioxus_logger::tracing::warn!("RunView websocket queue busy; dropping event");
    });
}

/// WebSocket runtime for the run view
struct RunWsRuntime {
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl Drop for RunWsRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

/// RunView component for live streaming conductor run documents
#[component]
pub fn RunView(desktop_id: String, window_id: String, run_id: String, document_path: String) -> Element {
    let _ = (&desktop_id, &window_id);

    // Core state
    let mut state = use_signal(|| RunViewState::new(run_id.clone(), document_path.clone()));
    let mut show_raw_events = use_signal(|| false);
    let mut ws_runtime = use_signal(|| None::<RunWsRuntime>);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<RunWsEvent>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));

    // Cleanup on unmount
    {
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_drop(move || {
            ws_event_pump_alive.set(false);
            if let Some(runtime) = ws_runtime.write().take() {
                runtime.closing.set(true);
                let _ = runtime.ws.close();
            }
        });
    }

    // WebSocket event pump
    {
        let ws_event_queue = ws_event_queue.clone();
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_effect(move || {
            if ws_event_pump_started() {
                return;
            }
            ws_event_pump_started.set(true);

            let ws_event_queue = ws_event_queue.clone();
            let ws_event_pump_alive = ws_event_pump_alive.clone();
            spawn(async move {
                while ws_event_pump_alive.get() {
                    let mut drained = Vec::new();
                    if let Ok(mut queue) = ws_event_queue.try_borrow_mut() {
                        while let Some(event) = queue.pop_front() {
                            drained.push(event);
                        }
                    } else {
                        TimeoutFuture::new(4).await;
                        continue;
                    }

                    for event in drained {
                        match event {
                            RunWsEvent::Connected => {
                                state.write().connected = true;
                                state.write().error = None;
                            }
                            RunWsEvent::Message(text_str) => {
                                handle_ws_message(&text_str, &mut state).await;
                            }
                            RunWsEvent::Error(message) => {
                                state.write().connected = false;
                                state.write().error = Some(message);
                                ws_runtime.set(None);
                            }
                            RunWsEvent::Closed => {
                                state.write().connected = false;
                                ws_runtime.set(None);
                            }
                        }
                    }

                    TimeoutFuture::new(16).await;
                }
            });
        });
    }

    // WebSocket connection
    {
        let ws_event_queue = ws_event_queue.clone();
        use_effect(move || {
            if ws_runtime.read().is_some() {
                return;
            }

            let ws_url = build_run_ws_url(&run_id);
            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    state.write().error = Some(format!("WebSocket open failed: {e:?}"));
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let ws_event_queue_open = ws_event_queue.clone();
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                enqueue_run_ws_event(&ws_event_queue_open, RunWsEvent::Connected);
            }) as Box<dyn FnMut(Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let ws_event_queue_message = ws_event_queue.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                    return;
                };
                let text_str = text.as_string().unwrap_or_default();
                enqueue_run_ws_event(&ws_event_queue_message, RunWsEvent::Message(text_str));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let ws_event_queue_error = ws_event_queue.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                enqueue_run_ws_event(&ws_event_queue_error, RunWsEvent::Error(e.message()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let ws_event_queue_close = ws_event_queue.clone();
            let closing_for_close = closing.clone();
            let on_close = Closure::wrap(Box::new(move |_e: CloseEvent| {
                if closing_for_close.get() {
                    return;
                }
                enqueue_run_ws_event(&ws_event_queue_close, RunWsEvent::Closed);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            ws_runtime.set(Some(RunWsRuntime {
                ws,
                closing,
                _on_open: on_open,
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            }));
        });
    }

    // Re-render markdown when document content changes
    use_effect(move || {
        let content = state.read().document_content.clone();
        if content.is_empty() {
            return;
        }

        spawn(async move {
            match writer_preview(Some(&content), None).await {
                Ok(response) => {
                    state.write().rendered_html = response.html;
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to render markdown: {}", e);
                }
            }
        });
    });

    let current_state = state.read().clone();
    let status_label = if current_state.connected {
        "Live"
    } else {
        "Reconnecting"
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; background: var(--window-bg); color: var(--text-primary); overflow: hidden;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); flex-shrink: 0;",

                // Left: Run info
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span { style: "font-size: 0.875rem; color: var(--text-secondary); max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "Run: {current_state.run_id}"
                    }
                }

                // Center: Connection status
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span {
                        style: if current_state.connected {
                            "font-size: 0.75rem; color: #16a34a; padding: 0.125rem 0.375rem; background: rgba(22, 163, 74, 0.1); border-radius: 0.25rem;"
                        } else {
                            "font-size: 0.75rem; color: #f59e0b; padding: 0.125rem 0.375rem; background: rgba(245, 158, 11, 0.1); border-radius: 0.25rem;"
                        },
                        "{status_label}"
                    }
                }

                // Right: Toggle raw events button
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    button {
                        style: if show_raw_events() {
                            "background: var(--accent-bg); border: none; color: var(--accent-text); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                        } else {
                            "background: transparent; border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer; padding: 0.375rem 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;"
                        },
                        onclick: move |_| show_raw_events.set(!show_raw_events()),
                        if show_raw_events() {
                            "Hide Events"
                        } else {
                            "Show Events"
                        }
                    }
                }
            }

            // Error banner
            if let Some(ref error) = current_state.error {
                div {
                    style: "padding: 0.75rem 1rem; background: var(--danger-bg); color: var(--danger-text); font-size: 0.875rem; border-bottom: 1px solid var(--border-color); display: flex; justify-content: space-between; align-items: center;",
                    div { "Error: {error}" }
                    button {
                        style: "background: transparent; border: 1px solid var(--danger-text); color: var(--danger-text); cursor: pointer; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem;",
                        onclick: move |_| state.write().error = None,
                        "Dismiss"
                    }
                }
            }

            // Main content area
            div {
                style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",

                // Document view (takes remaining space)
                div {
                    style: if show_raw_events() {
                        "flex: 1; overflow: hidden; min-height: 50%;"
                    } else {
                        "flex: 1; overflow: hidden;"
                    },
                    if current_state.rendered_html.is_empty() {
                        div {
                            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted);",
                            "Waiting for document content..."
                        }
                    } else {
                        MarkdownViewer { html: current_state.rendered_html }
                    }
                }

                // Raw events section (collapsible)
                if show_raw_events() {
                    div {
                        style: "flex: 1; overflow: auto; border-top: 1px solid var(--border-color); background: var(--bg-primary); min-height: 50%;",

                        // Events header
                        div {
                            style: "padding: 0.5rem 1rem; background: var(--titlebar-bg); border-bottom: 1px solid var(--border-color); font-size: 0.875rem; font-weight: 500; color: var(--text-secondary); position: sticky; top: 0;",
                            "Raw Events ({current_state.raw_events.len()})"
                        }

                        // Events list
                        if current_state.raw_events.is_empty() {
                            div {
                                style: "padding: 2rem; text-align: center; color: var(--text-muted); font-size: 0.875rem;",
                                "No events received yet"
                            }
                        } else {
                            div {
                                style: "padding: 0.5rem;",
                                for (idx, event) in current_state.raw_events.iter().enumerate() {
                                    details {
                                        key: "{idx}",
                                        style: "margin-bottom: 0.5rem; border: 1px solid var(--border-color); border-radius: 0.375rem; overflow: hidden;",
                                        summary {
                                            style: "padding: 0.5rem 0.75rem; background: var(--bg-secondary); cursor: pointer; font-size: 0.875rem; display: flex; align-items: center; gap: 0.5rem;",
                                            span { style: "color: var(--text-muted); font-size: 0.75rem;", "[{event.timestamp}]" }
                                            span { style: "color: var(--text-primary); font-weight: 500;", "{event.event_type}" }
                                        }
                                        div {
                                            style: "padding: 0.75rem; background: var(--bg-primary);",
                                            pre {
                                                style: "margin: 0; font-size: 0.75rem; color: var(--text-secondary); overflow: auto; max-height: 200px;",
                                                "{serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| event.payload.to_string())}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Handle an incoming WebSocket message
async fn handle_ws_message(text: &str, state: &mut Signal<RunViewState>) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };

    let msg_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "connected" | "pong" => {
            state.write().connected = true;
        }
        "conductor.run.document_update" => {
            if let (Some(_run_id), Some(_document_path), Some(content_excerpt)) = (
                json.get("run_id").and_then(|v| v.as_str()),
                json.get("document_path").and_then(|v| v.as_str()),
                json.get("content_excerpt").and_then(|v| v.as_str()),
            ) {
                let mut guard = state.write();
                guard.update_document(content_excerpt.to_string());

                // Add as raw event too
                let timestamp = json
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                guard.add_raw_event(RawEvent {
                    timestamp,
                    event_type: "conductor.run.document_update".to_string(),
                    payload: json.clone(),
                });
            }
        }
        "event" => {
            // Generic event - add to raw events
            let event_type = json
                .get("event_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let timestamp = json
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let payload = json.get("payload").cloned().unwrap_or_default();

            state.write().add_raw_event(RawEvent {
                timestamp,
                event_type,
                payload,
            });
        }
        _ => {
            // Unknown message type - log but ignore
            dioxus_logger::tracing::debug!("Unknown WebSocket message type: {}", msg_type);
        }
    }
}

/// Build the WebSocket URL for the run view
fn build_run_ws_url(run_id: &str) -> String {
    let ws_base = http_to_ws_url(crate::api::api_base());
    format!("{}/ws/runs/{}?subscribe=document,events", ws_base, run_id)
}

/// Convert HTTP URL to WebSocket URL
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
