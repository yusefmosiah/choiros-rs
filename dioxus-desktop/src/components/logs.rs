use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::LogsEvent;

use super::styles::CHAT_STYLES;

enum LogsWsEvent {
    Connected,
    Message(String),
    Error(String),
    Closed,
}

struct LogsRuntime {
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

#[derive(Clone)]
struct LogFeedEntry {
    event: LogsEvent,
    received_at_ms: f64,
    summary: Option<String>,
}

#[component]
pub fn LogsView(desktop_id: String, window_id: String) -> Element {
    let mut entries = use_signal(Vec::<LogFeedEntry>::new);
    let mut since_seq = use_signal(load_logs_cursor);
    let mut connected = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut summarizer_model = use_signal(|| "local deterministic summarizer".to_string());
    let mut ws_runtime = use_signal(|| None::<LogsRuntime>);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<LogsWsEvent>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));
    let pump_alive = use_hook(|| Rc::new(Cell::new(true)));

    {
        let pump_alive = pump_alive.clone();
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_drop(move || {
            pump_alive.set(false);
            ws_event_pump_alive.set(false);
            if let Some(runtime) = ws_runtime.write().take() {
                runtime.closing.set(true);
                let _ = runtime.ws.close();
            }
        });
    }

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
                    {
                        let mut queue = ws_event_queue.borrow_mut();
                        while let Some(event) = queue.pop_front() {
                            drained.push(event);
                        }
                    }

                    for event in drained {
                        match event {
                            LogsWsEvent::Connected => {
                                connected.set(true);
                                error.set(None);
                            }
                            LogsWsEvent::Message(text_str) => {
                                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str)
                                else {
                                    continue;
                                };

                                match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                                    "connected" | "pong" => {
                                        connected.set(true);
                                        if let Some(model) = json
                                            .get("summarizer_model")
                                            .and_then(|v| v.as_str())
                                            .filter(|m| !m.trim().is_empty())
                                        {
                                            summarizer_model.set(model.to_string());
                                        }
                                    }
                                    "event" => {
                                        let event = LogsEvent {
                                            seq: json
                                                .get("seq")
                                                .and_then(|v| v.as_i64())
                                                .unwrap_or(0),
                                            event_id: json
                                                .get("event_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string(),
                                            timestamp: json
                                                .get("timestamp")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string(),
                                            event_type: json
                                                .get("event_type")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string(),
                                            actor_id: json
                                                .get("actor_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string(),
                                            user_id: json
                                                .get("user_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string(),
                                            payload: json
                                                .get("payload")
                                                .cloned()
                                                .unwrap_or(serde_json::Value::Null),
                                        };
                                        if !(event.event_type.starts_with("worker.task")
                                            || event.event_type.starts_with("watcher.alert")
                                            || event.event_type.starts_with("log.summary")
                                            || event.event_type.starts_with("model.")
                                            || event.event_type.starts_with("chat."))
                                        {
                                            continue;
                                        }

                                        let next_seq = since_seq().max(event.seq);
                                        since_seq.set(next_seq);
                                        persist_logs_cursor(next_seq);
                                        let now_ms = js_sys::Date::now();
                                        let mut list = entries.write();
                                        list.push(LogFeedEntry {
                                            event,
                                            received_at_ms: now_ms,
                                            summary: None,
                                        });
                                        list.sort_by_key(|entry| entry.event.seq);
                                        list.dedup_by(|a, b| a.event.event_id == b.event.event_id);
                                        if list.len() > 400 {
                                            let trim = list.len() - 400;
                                            list.drain(0..trim);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            LogsWsEvent::Error(message) => {
                                connected.set(false);
                                error.set(Some(message));
                                ws_runtime.set(None);
                            }
                            LogsWsEvent::Closed => {
                                connected.set(false);
                                ws_runtime.set(None);
                            }
                        }
                    }
                    TimeoutFuture::new(16).await;
                }
            });
        });
    }

    {
        let ws_event_queue = ws_event_queue.clone();
        use_effect(move || {
            if ws_runtime.read().is_some() {
                return;
            }

            let ws_url = build_logs_ws_url(since_seq());
            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    error.set(Some(format!("logs websocket open failed: {e:?}")));
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let ws_event_queue_open = ws_event_queue.clone();
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                ws_event_queue_open
                    .borrow_mut()
                    .push_back(LogsWsEvent::Connected);
            }) as Box<dyn FnMut(Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let ws_event_queue_message = ws_event_queue.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                    return;
                };
                let text_str = text.as_string().unwrap_or_default();
                ws_event_queue_message
                    .borrow_mut()
                    .push_back(LogsWsEvent::Message(text_str));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let ws_event_queue_error = ws_event_queue.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                ws_event_queue_error
                    .borrow_mut()
                    .push_back(LogsWsEvent::Error(e.message()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let ws_event_queue_close = ws_event_queue.clone();
            let closing_for_close = closing.clone();
            let on_close = Closure::wrap(Box::new(move |_e: CloseEvent| {
                if closing_for_close.get() {
                    return;
                }
                ws_event_queue_close
                    .borrow_mut()
                    .push_back(LogsWsEvent::Closed);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            ws_runtime.set(Some(LogsRuntime {
                ws,
                closing,
                _on_open: on_open,
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            }));
        });
    }

    use_effect(move || {
        let pump_alive = pump_alive.clone();
        spawn(async move {
            while pump_alive.get() {
                let now_ms = js_sys::Date::now();
                {
                    let mut list = entries.write();
                    for entry in list.iter_mut() {
                        if entry.summary.is_none() && now_ms - entry.received_at_ms >= 1200.0 {
                            if entry.event.event_type.starts_with("log.summary") {
                                entry.summary = Some(entry.event.event_type.clone());
                            } else {
                                entry.summary = Some(summarize_log_event(&entry.event));
                            }
                        }
                    }
                }
                TimeoutFuture::new(220).await;
            }
        });
    });

    let status_label = if connected() { "Live" } else { "Reconnecting" };
    let snapshot = entries.read().clone();
    let mut reversed = snapshot.into_iter().collect::<Vec<_>>();
    reversed.reverse();

    rsx! {
        style { {CHAT_STYLES} }
        div {
            class: "chat-container",
            style: "padding: 0.75rem; overflow: auto;",
            div {
                class: "chat-header",
                h3 { "Watcher Logs" }
                span {
                    class: "chat-status",
                    style: if connected() {
                        "color: #16a34a;"
                    } else {
                        "color: #f59e0b;"
                    },
                    "{status_label}"
                }
            }
            if let Some(message) = error() {
                div {
                    class: "message-bubble system-bubble",
                    "Log stream error: {message}"
                }
            }
            if reversed.is_empty() {
                div {
                    class: "empty-state",
                    div { class: "empty-icon", "📡" }
                    p { "No streamed log events yet" }
                    span { "Raw events stream via /ws/logs/events, then summarize in place" }
                }
            } else {
                div {
                    class: "messages-list",
                    for entry in reversed {
                        details {
                            class: "tool-details",
                            summary {
                                class: "tool-summary",
                                style: if entry.summary.is_some() { "font-style: italic;" } else { "" },
                                "{entry.summary.clone().unwrap_or_else(|| entry.event.event_type.clone())} #{entry.event.seq}"
                            }
                            div {
                                class: "tool-body",
                                if entry.summary.is_some() {
                                    if let Some(summary_model) = summary_model_label(&entry.event, &summarizer_model()) {
                                        p { class: "tool-meta", "summarized by {summary_model}" }
                                    }
                                }
                                p { class: "tool-meta", "Event: {entry.event.event_type}" }
                                p { class: "tool-meta", "Actor: {entry.event.actor_id}" }
                                p { class: "tool-meta", "Time: {entry.event.timestamp}" }
                                pre {
                                    class: "tool-pre",
                                    "{serde_json::to_string_pretty(&entry.event.payload).unwrap_or_else(|_| entry.event.payload.to_string())}"
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: "input-hint",
                "Desktop: {desktop_id} | Window: {window_id}"
            }
        }
    }
}

fn summarize_log_event(event: &LogsEvent) -> String {
    fn trim_snippet(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }
        text.chars().take(max_chars).collect::<String>() + "..."
    }

    fn soften(text: &str) -> String {
        text.replace(['|', ':', '(', ')', '[', ']', '{', '}', '"'], " ")
            .replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let payload = &event.payload;
    let summary = match event.event_type.as_str() {
        "chat.user_msg" => payload
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| payload.as_str())
            .map(|text| format!("user asked {}", soften(&trim_snippet(text, 200))))
            .unwrap_or_else(|| "user message received".to_string()),
        "chat.assistant_msg" => {
            let model = payload
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let model_source = payload
                .get("model_source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tools_used = payload
                .get("tools_used")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "assistant answered using model {} from {} with {} tools response {}",
                model,
                model_source,
                tools_used,
                soften(&trim_snippet(text, 220))
            )
        }
        "model.selection" => {
            let used = payload
                .get("model_used")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let source = payload
                .get("model_source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let requested = payload
                .get("requested_model")
                .and_then(|v| v.as_str())
                .unwrap_or("none");
            let chat_pref = payload
                .get("chat_model_preference")
                .and_then(|v| v.as_str())
                .unwrap_or("none");
            format!(
                "model routing selected {} from {} requested {} app preference {}",
                used, source, requested, chat_pref
            )
        }
        "model.changed" => {
            let old_model = payload
                .get("old_model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let new_model = payload
                .get("new_model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let source = payload
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!(
                "model changed from {} to {} triggered by {}",
                old_model, new_model, source
            )
        }
        event_type if event_type.starts_with("worker.task") => {
            let status = payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let model_used = payload
                .get("model_used")
                .and_then(|v| v.as_str())
                .unwrap_or("n/a");
            let phase = payload
                .get("phase")
                .and_then(|v| v.as_str())
                .unwrap_or("worker");
            let model_requested = payload
                .get("model_requested")
                .and_then(|v| v.as_str())
                .unwrap_or("n/a");
            let command = payload
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let output_excerpt = payload
                .get("output_excerpt")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "worker update status {} phase {} requested model {} used model {} command {} output {}",
                status,
                phase,
                model_requested,
                model_used,
                soften(&trim_snippet(command, 130)),
                soften(&trim_snippet(output_excerpt, 130))
            )
        }
        event_type if event_type.starts_with("watcher.alert") => {
            let summary = payload
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("Watcher alert");
            let threshold = payload
                .get("threshold")
                .and_then(|v| v.as_i64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string());
            let window = payload
                .get("window_ms")
                .and_then(|v| v.as_i64())
                .map(|v| format!("{v}ms"))
                .unwrap_or_else(|| "n/a".to_string());
            format!(
                "watcher alert {} threshold {} window {}",
                soften(summary),
                threshold,
                window
            )
        }
        _ => soften(&event.event_type),
    };
    trim_snippet(&summary, 420)
}

fn summary_model_label(event: &LogsEvent, fallback_model: &str) -> Option<String> {
    if let Some(model) = event
        .payload
        .get("summary_model")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
    {
        return Some(model.to_string());
    }
    Some(fallback_model.to_string())
}

fn build_logs_ws_url(since_seq: i64) -> String {
    let ws_base = http_to_ws_url(crate::api::api_base());
    format!(
        "{}/ws/logs/events?since_seq={}&limit=300&poll_ms=200",
        ws_base,
        since_seq.max(0)
    )
}

fn load_logs_cursor() -> i64 {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return 0;
    };
    let Ok(Some(raw)) = storage.get_item("choiros.logs_since_seq.v1") else {
        return 0;
    };
    raw.parse::<i64>().unwrap_or(0).max(0)
}

fn persist_logs_cursor(since_seq: i64) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let _ = storage.set_item("choiros.logs_since_seq.v1", &since_seq.max(0).to_string());
}

impl Drop for LogsRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
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
