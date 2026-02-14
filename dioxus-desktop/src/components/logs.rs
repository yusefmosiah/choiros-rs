use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::{fetch_latest_log_seq, fetch_logs_events, open_window, LogsEvent};

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
}

#[derive(Clone)]
struct RunListEntry {
    run_id: String,
    actor_id: String,
    status: String,
    started_at: String,
    updated_at: String,
    last_seq: i64,
    event_count: usize,
    headline: String,
}

#[component]
pub fn LogsView(desktop_id: String, window_id: String) -> Element {
    let mut entries = use_signal(Vec::<LogFeedEntry>::new);
    let mut since_seq = use_signal(load_logs_cursor);
    let mut selected_run_id = use_signal(|| None::<String>);
    let mut connected = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut ws_runtime = use_signal(|| None::<LogsRuntime>);
    let mut preload_started = use_signal(|| false);
    let mut preload_ready = use_signal(|| false);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<LogsWsEvent>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));

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

    {
        use_effect(move || {
            if preload_started() {
                return;
            }
            preload_started.set(true);

            let cursor = since_seq().max(0);
            spawn(async move {
                let latest_seq = match fetch_latest_log_seq().await {
                    Ok(seq) => seq,
                    Err(_) => cursor,
                };
                let preload_since = latest_seq.saturating_sub(1_000);

                match fetch_logs_events(preload_since, 1_000, None).await {
                    Ok(events) => {
                        let mut max_seq = latest_seq.max(cursor);
                        let mut preload = events
                            .into_iter()
                            .filter(should_display_event)
                            .map(|event| {
                                max_seq = max_seq.max(event.seq);
                                LogFeedEntry { event }
                            })
                            .collect::<Vec<_>>();
                        preload.sort_by_key(|entry| entry.event.seq);
                        preload.dedup_by(|a, b| a.event.event_id == b.event.event_id);
                        entries.set(preload);
                        since_seq.set(max_seq);
                        persist_logs_cursor(max_seq);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to preload logs: {e}")));
                    }
                }
                preload_ready.set(true);
            });
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
                                        if !should_display_event(&event) {
                                            continue;
                                        }

                                        let next_seq = since_seq().max(event.seq);
                                        since_seq.set(next_seq);
                                        persist_logs_cursor(next_seq);
                                        let mut list = entries.write();
                                        list.push(LogFeedEntry { event });
                                        list.sort_by_key(|entry| entry.event.seq);
                                        list.dedup_by(|a, b| a.event.event_id == b.event.event_id);
                                        if list.len() > 1200 {
                                            let trim = list.len() - 1200;
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
            if !preload_ready() {
                return;
            }
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

    let status_label = if connected() { "Live" } else { "Reconnecting" };
    let snapshot = entries.read().clone();
    let runs = derive_runs(&snapshot);
    let active_run_id = selected_run_id()
        .filter(|id| runs.iter().any(|run| run.run_id == *id))
        .or_else(|| runs.first().map(|run| run.run_id.clone()));
    let selected_run = active_run_id
        .as_ref()
        .and_then(|id| runs.iter().find(|run| run.run_id == *id))
        .cloned();
    let mut reversed = snapshot.into_iter().collect::<Vec<_>>();
    reversed.reverse();
    let filtered = if let Some(run_id) = active_run_id.as_deref() {
        reversed
            .into_iter()
            .filter(|entry| run_key_for_event(&entry.event).as_deref() == Some(run_id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    rsx! {
        style { {CHAT_STYLES} }
        div {
            class: "chat-container",
            style: "padding: 0.75rem; overflow: auto;",
            div {
                class: "chat-header",
                h3 { "Logs" }
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
            if runs.is_empty() {
                div {
                    class: "empty-state",
                    div { class: "empty-icon", "📡" }
                    p { "No runs available yet" }
                    span { "Run index loads from /logs/events then stays live on /ws/logs/events" }
                }
            } else {
                div {
                    class: "chat-body",
                    aside {
                        class: "thread-sidebar",
                        div {
                            class: "thread-sidebar-header",
                            span { "Runs" }
                            span { "{runs.len()}" }
                        }
                        div {
                            class: "thread-list",
                            for run in runs {
                                button {
                                    class: if active_run_id.as_deref() == Some(run.run_id.as_str()) {
                                        "thread-item active"
                                    } else {
                                        "thread-item"
                                    },
                                    onclick: {
                                        let run_id = run.run_id.clone();
                                        move |_| selected_run_id.set(Some(run_id.clone()))
                                    },
                                    div { class: "thread-title", "{run.headline}" }
                                    div { class: "thread-preview", "#{run.last_seq} {run.status} {run.event_count} events" }
                                }
                            }
                        }
                    }
                    div {
                        class: "messages-scroll-area",
                        if let Some(run) = selected_run {
                            button {
                                class: "thread-run-button",
                                onclick: {
                                    let desktop_id = desktop_id.clone();
                                    let run = run.clone();
                                    move |_| {
                                        let desktop_id = desktop_id.clone();
                                        let run = run.clone();
                                        spawn(async move {
                                            if let Err(e) = open_run_markdown_from_logs(desktop_id, run).await
                                            {
                                                dioxus_logger::tracing::error!(
                                                    "Failed to open run markdown from logs: {}",
                                                    e
                                                );
                                            }
                                        });
                                    }
                                },
                                "Open Run Markdown"
                            }
                            div {
                                class: "message-bubble system-bubble",
                                "Run {run.run_id} | actor {run.actor_id} | status {run.status} | events {run.event_count} | start {run.started_at} | last {run.updated_at}"
                            }
                            if filtered.is_empty() {
                                div {
                                    class: "empty-state",
                                    div { class: "empty-icon", "🪵" }
                                    p { "No events found for selected run" }
                                }
                            } else {
                                div {
                                    class: "messages-list",
                                    for entry in filtered {
                                        details {
                                            class: "tool-details",
                                            summary {
                                                class: "tool-summary",
                                                "{event_headline(&entry.event)} #{entry.event.seq}"
                                            }
                                            div {
                                                class: "tool-body",
                                                p { class: "tool-meta", "Event: {entry.event.event_type}" }
                                                p { class: "tool-meta", "Scope actor: {entry.event.actor_id}" }
                                                if let Some(emitter_actor) = event_emitter_label(&entry.event) {
                                                    p { class: "tool-meta", "Emitter: {emitter_actor}" }
                                                }
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
                        } else {
                            div {
                                class: "empty-state",
                                div { class: "empty-icon", "🧭" }
                                p { "Select a run from the sidebar" }
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

fn run_key_for_event(event: &LogsEvent) -> Option<String> {
    let data = event
        .payload
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    event
        .payload
        .get("correlation_id")
        .and_then(|v| v.as_str())
        .or_else(|| data.get("correlation_id").and_then(|v| v.as_str()))
        .or_else(|| {
            event
                .payload
                .get("task")
                .and_then(|task| task.get("correlation_id"))
                .and_then(|v| v.as_str())
        })
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("corr:{value}"))
        .or_else(|| {
            event
                .payload
                .get("run_id")
                .and_then(|v| v.as_str())
                .or_else(|| data.get("run_id").and_then(|v| v.as_str()))
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("run:{value}"))
        })
}

fn derive_runs(entries: &[LogFeedEntry]) -> Vec<RunListEntry> {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.event.seq);

    let mut runs = std::collections::HashMap::<String, RunListEntry>::new();
    for entry in ordered {
        let Some(run_id) = run_key_for_event(&entry.event) else {
            continue;
        };
        let actor =
            event_emitter_label(&entry.event).unwrap_or_else(|| entry.event.actor_id.clone());
        let status = match entry.event.event_type.as_str() {
            "worker.task.failed"
            | "conductor.task.failed"
            | "conductor.capability.failed" => "failed",
            "worker.task.completed" | "conductor.task.completed" => "completed",
            "worker.task.started"
            | "worker.task.progress"
            | "conductor.task.started"
            | "conductor.task.progress"
            | "conductor.worker.call"
            | "conductor.worker.result"
            | "conductor.run.started"
            | "conductor.capability.completed"
            | "conductor.capability.blocked"
            | "conductor.decision"
            | "conductor.progress" => "running",
            _ => "active",
        }
        .to_string();
        let headline = event_headline(&entry.event);

        runs.entry(run_id.clone())
            .and_modify(|run| {
                run.last_seq = run.last_seq.max(entry.event.seq);
                run.updated_at = entry.event.timestamp.clone();
                run.event_count += 1;
                run.headline = headline.clone();
                if run.status != "failed" {
                    if status == "failed" || status == "completed" || status == "running" {
                        run.status = status.clone();
                    }
                }
            })
            .or_insert_with(|| RunListEntry {
                run_id,
                actor_id: actor,
                status,
                started_at: entry.event.timestamp.clone(),
                updated_at: entry.event.timestamp.clone(),
                last_seq: entry.event.seq,
                event_count: 1,
                headline,
            });
    }

    let mut out = runs.into_values().collect::<Vec<_>>();
    out.sort_by_key(|run| -run.last_seq);
    out
}

async fn open_run_markdown_from_logs(desktop_id: String, run: RunListEntry) -> Result<(), String> {
    let query = if let Some(correlation_id) = run.run_id.strip_prefix("corr:") {
        format!(
            "actor_id={}&correlation_id={}",
            url_encode(&run.actor_id),
            url_encode(correlation_id)
        )
    } else if let Some(run_id) = run.run_id.strip_prefix("run:") {
        format!(
            "actor_id={}&run_id={}",
            url_encode(&run.actor_id),
            url_encode(run_id)
        )
    } else {
        format!("actor_id={}", url_encode(&run.actor_id))
    };

    let uri = format!("runlog://export?{query}");
    let props = serde_json::json!({
        "viewer": {
            "kind": "text",
            "resource": {
                "uri": uri,
                "mime": "text/markdown"
            },
            "capabilities": { "readonly": true }
        }
    });
    open_window(&desktop_id, "writer", "Run Transcript", Some(props))
        .await
        .map(|_| ())
}

fn url_encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_else(|| value.to_string())
}

fn should_display_event(event: &LogsEvent) -> bool {
    match event.event_type.as_str() {
        "chat.user_msg"
        | "chat.assistant_msg"
        | "chat.tool_call"
        | "chat.tool_result"
        | "model.selection"
        | "model.changed"
        | "worker.task.started"
        | "worker.task.completed"
        | "worker.task.failed"
        | "conductor.task.started"
        | "conductor.task.completed"
        | "conductor.task.failed"
        | "conductor.run.started"
        | "conductor.capability.completed"
        | "conductor.capability.failed"
        | "conductor.capability.blocked"
        | "conductor.decision"
        | "conductor.progress"
        | "conductor.finding"
        | "conductor.learning"
        | "conductor.tool.call"
        | "conductor.tool.result"
        | "conductor.worker.call"
        | "conductor.worker.result" => true,
        "worker.task.progress" => {
            let phase = event
                .payload
                .get("phase")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            matches!(
                phase,
                "terminal_tool_dispatch"
                    | "terminal_tool_call"
                    | "terminal_tool_result"
                    | "terminal_agent_fallback"
                    | "terminal_agent_synthesizing"
                    | "research_task_started"
                    | "research_provider_call"
                    | "research_provider_result"
                    | "research_provider_error"
                    | "research_round_started"
                    | "research_round_refine_query"
                    | "research_task_completed"
            )
        }
        "conductor.task.progress" => {
            let phase = event
                .payload
                .get("phase")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            matches!(phase, "routing" | "worker_execution")
        }
        other => other.starts_with("conductor.") || other.starts_with("worker.task"),
    }
}

fn event_headline(event: &LogsEvent) -> String {
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

    let actor = event_emitter_label(event).unwrap_or_else(|| event.actor_id.clone());
    let payload = &event.payload;
    let data = payload.get("data").unwrap_or(payload);
    let headline = match event.event_type.as_str() {
        "chat.user_msg" => payload
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| payload.as_str())
            .map(|text| {
                format!(
                    "{actor} received user message {}",
                    soften(&trim_snippet(text, 180))
                )
            })
            .unwrap_or_else(|| format!("{actor} received user message")),
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
                "{actor} answered using {model} from {model_source} with {tools_used} tools {snippet}",
                snippet = soften(&trim_snippet(text, 180)),
            )
        }
        "chat.tool_call" => {
            let tool = payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_tool");
            format!("{actor} requested tool {tool}")
        }
        "chat.tool_result" => {
            let tool = payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_tool");
            let success = payload
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            format!("{actor} got tool result {tool} success={success}")
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
                "{actor} selected model {used} source={source} requested={requested} preference={chat_pref}"
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
                "{actor} changed model from {} to {} triggered by {}",
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
            let failure_kind = payload
                .get("failure_kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let error = payload
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if event_type == "worker.task.failed" {
                format!(
                    "{actor} worker failed kind={failure_kind} phase={phase} requested={model_requested} used={model_used} error={error} output={output}",
                    output = soften(&trim_snippet(output_excerpt, 130)),
                    error = soften(&trim_snippet(error, 140)),
                )
            } else {
                format!(
                    "{actor} worker status={status} phase={phase} requested={model_requested} used={model_used} command={command} output={output}",
                    command = soften(&trim_snippet(command, 130)),
                    output = soften(&trim_snippet(output_excerpt, 130)),
                )
            }
        }
        "conductor.task.started" => {
            let objective = payload
                .get("objective")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor task started objective={}",
                soften(&trim_snippet(objective, 160))
            )
        }
        "conductor.task.progress" => {
            let status = payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("running");
            let phase = payload
                .get("phase")
                .and_then(|v| v.as_str())
                .unwrap_or("conductor");
            format!("{actor} conductor progress status={status} phase={phase}")
        }
        "conductor.run.started" => {
            let objective = data
                .get("objective")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor run started objective={}",
                soften(&trim_snippet(objective, 160))
            )
        }
        "conductor.progress" => {
            let message = data
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let capability = payload
                .get("capability")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!(
                "{actor} conductor progress capability={capability} {}",
                soften(&trim_snippet(message, 180))
            )
        }
        "conductor.capability.completed"
        | "conductor.capability.failed"
        | "conductor.capability.blocked" => {
            let capability = payload
                .get("capability")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let call_id = data
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("n/a");
            let message = data
                .get("summary")
                .or_else(|| data.get("error"))
                .or_else(|| data.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} {} capability={capability} call_id={call_id} {}",
                event.event_type,
                soften(&trim_snippet(message, 160))
            )
        }
        "conductor.decision" => {
            let decision_type = data
                .get("decision_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let reason = data
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor decision={decision_type} {}",
                soften(&trim_snippet(reason, 160))
            )
        }
        "conductor.worker.call" => {
            let worker_type = payload
                .get("worker_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let objective = payload
                .get("worker_objective")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor called {worker_type} objective={}",
                soften(&trim_snippet(objective, 140))
            )
        }
        "conductor.worker.result" => {
            let worker_type = payload
                .get("worker_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let success = payload
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let summary = payload
                .get("result_summary")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor {worker_type} result success={success} summary={}",
                soften(&trim_snippet(summary, 140))
            )
        }
        "conductor.task.completed" => {
            let report_path = payload
                .get("report_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!("{actor} conductor task completed report={report_path}")
        }
        "conductor.task.failed" => {
            let code = payload
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let message = payload
                .get("error_message")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!(
                "{actor} conductor task failed code={code} message={}",
                soften(&trim_snippet(message, 160))
            )
        }
        _ => format!("{actor} {}", soften(&event.event_type)),
    };
    trim_snippet(&headline, 420)
}

fn event_emitter_label(event: &LogsEvent) -> Option<String> {
    event
        .payload
        .get("emitter_actor")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.to_string())
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
