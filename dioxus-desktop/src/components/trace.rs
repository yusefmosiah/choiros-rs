use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::{fetch_latest_log_seq, fetch_logs_events, LogsEvent};

use super::styles::CHAT_STYLES;

enum TraceWsEvent {
    Connected,
    Message(String),
    Error(String),
    Closed,
}

struct TraceRuntime {
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

#[derive(Clone, Debug)]
struct TraceEvent {
    seq: i64,
    event_id: String,
    trace_id: String,
    timestamp: String,
    event_type: String,
    role: String,
    function_name: String,
    model_used: String,
    provider: Option<String>,
    actor_id: String,
    run_id: Option<String>,
    call_id: Option<String>,
    session_id: Option<String>,
    thread_id: Option<String>,
    system_context: Option<String>,
    input: Option<serde_json::Value>,
    input_summary: Option<String>,
    output: Option<serde_json::Value>,
    output_summary: Option<String>,
    duration_ms: Option<i64>,
    error_code: Option<String>,
    error_message: Option<String>,
    failure_kind: Option<String>,
    #[allow(dead_code)]
    started_at: Option<String>,
    #[allow(dead_code)]
    ended_at: Option<String>,
}

#[derive(Clone, Debug)]
struct TraceGroup {
    trace_id: String,
    started: Option<TraceEvent>,
    terminal: Option<TraceEvent>,
}

#[derive(Clone, Debug)]
struct PromptEvent {
    seq: i64,
    event_id: String,
    timestamp: String,
    run_id: String,
    objective: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ToolTraceEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    tool_trace_id: String,
    timestamp: String,
    role: String,
    tool_name: String,
    run_id: Option<String>,
    call_id: Option<String>,
    success: Option<bool>,
    duration_ms: Option<i64>,
}

#[derive(Clone, Debug)]
struct RunGraphSummary {
    run_id: String,
    objective: String,
    timestamp: String,
    conductor_calls: usize,
    researcher_calls: usize,
    terminal_calls: usize,
    tool_calls: usize,
    tool_failures: usize,
}

impl TraceGroup {
    fn status(&self) -> &'static str {
        if self.terminal.is_some() {
            match self.terminal.as_ref().unwrap().event_type.as_str() {
                "llm.call.completed" => "completed",
                "llm.call.failed" => "failed",
                _ => "unknown",
            }
        } else if self.started.is_some() {
            "started"
        } else {
            "unknown"
        }
    }

    fn timestamp(&self) -> String {
        self.started
            .as_ref()
            .map(|e| e.timestamp.clone())
            .or_else(|| self.terminal.as_ref().map(|e| e.timestamp.clone()))
            .unwrap_or_default()
    }

    fn role(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.role.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.role.as_str()))
            .unwrap_or("unknown")
    }

    fn function_name(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.function_name.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.function_name.as_str()))
            .unwrap_or("unknown")
    }

    fn model_used(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.model_used.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.model_used.as_str()))
            .unwrap_or("unknown")
    }

    fn provider(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.provider.as_deref())
            .or_else(|| {
                self.terminal
                    .as_ref()
                    .and_then(|e| e.provider.as_deref())
            })
    }

    fn duration_ms(&self) -> Option<i64> {
        self.terminal.as_ref().and_then(|t| t.duration_ms)
    }

    fn run_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.run_id.as_deref())
            .or_else(|| {
                self.terminal
                    .as_ref()
                    .and_then(|e| e.run_id.as_deref())
            })
    }

    fn call_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.call_id.as_deref())
            .or_else(|| {
                self.terminal
                    .as_ref()
                    .and_then(|e| e.call_id.as_deref())
            })
    }

    fn session_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.session_id.as_deref())
            .or_else(|| {
                self.terminal
                    .as_ref()
                    .and_then(|e| e.session_id.as_deref())
            })
    }

    fn thread_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.thread_id.as_deref())
            .or_else(|| {
                self.terminal
                    .as_ref()
                    .and_then(|e| e.thread_id.as_deref())
            })
    }

    fn actor_id(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.actor_id.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.actor_id.as_str()))
            .unwrap_or("unknown")
    }
}

fn parse_trace_event(event: &LogsEvent) -> Option<TraceEvent> {
    if !event.event_type.starts_with("llm.call.") {
        return None;
    }

    let payload = &event.payload;
    let scope = payload.get("scope").cloned().unwrap_or(serde_json::Value::Null);

    Some(TraceEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        trace_id: payload
            .get("trace_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        timestamp: event.timestamp.clone(),
        event_type: event.event_type.clone(),
        role: payload
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        function_name: payload
            .get("function_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        model_used: payload
            .get("model_used")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        provider: payload.get("provider").and_then(|v| v.as_str()).map(|s| s.to_string()),
        actor_id: payload
            .get("actor_id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| event.actor_id.as_str())
            .to_string(),
        run_id: payload.get("run_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        call_id: payload.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        session_id: scope
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        thread_id: scope
            .get("thread_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        system_context: payload
            .get("system_context")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        input: payload.get("input").cloned(),
        input_summary: payload
            .get("input_summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        output: payload.get("output").cloned(),
        output_summary: payload
            .get("output_summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        duration_ms: payload.get("duration_ms").and_then(|v| v.as_i64()),
        error_code: payload
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        error_message: payload
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        failure_kind: payload
            .get("failure_kind")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        started_at: payload
            .get("started_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ended_at: payload
            .get("ended_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn parse_prompt_event(event: &LogsEvent) -> Option<PromptEvent> {
    if event.event_type != "trace.prompt.received" && event.event_type != "conductor.task.started" {
        return None;
    }
    let payload = &event.payload;
    let run_id = payload
        .get("run_id")
        .and_then(|v| v.as_str())?
        .to_string();
    let objective = payload
        .get("objective")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled objective")
        .to_string();

    Some(PromptEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        objective,
    })
}

fn parse_tool_trace_event(event: &LogsEvent) -> Option<ToolTraceEvent> {
    if event.event_type != "worker.tool.call" && event.event_type != "worker.tool.result" {
        return None;
    }
    let payload = &event.payload;
    Some(ToolTraceEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        tool_trace_id: payload
            .get("tool_trace_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        timestamp: event.timestamp.clone(),
        role: payload
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        tool_name: payload
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        run_id: payload.get("run_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        call_id: payload.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        success: payload.get("success").and_then(|v| v.as_bool()),
        duration_ms: payload.get("duration_ms").and_then(|v| v.as_i64()),
    })
}

fn build_run_graph_summaries(
    traces: &[TraceGroup],
    prompts: &[PromptEvent],
    tools: &[ToolTraceEvent],
) -> Vec<RunGraphSummary> {
    let mut by_run: HashMap<String, RunGraphSummary> = HashMap::new();

    for prompt in prompts {
        let entry = by_run
            .entry(prompt.run_id.clone())
            .or_insert_with(|| RunGraphSummary {
                run_id: prompt.run_id.clone(),
                objective: prompt.objective.clone(),
                timestamp: prompt.timestamp.clone(),
                conductor_calls: 0,
                researcher_calls: 0,
                terminal_calls: 0,
                tool_calls: 0,
                tool_failures: 0,
            });
        entry.objective = prompt.objective.clone();
        if prompt.timestamp > entry.timestamp {
            entry.timestamp = prompt.timestamp.clone();
        }
    }

    for trace in traces {
        let Some(run_id) = trace.run_id() else {
            continue;
        };
        let entry = by_run
            .entry(run_id.to_string())
            .or_insert_with(|| RunGraphSummary {
                run_id: run_id.to_string(),
                objective: "Objective unavailable".to_string(),
                timestamp: trace.timestamp(),
                conductor_calls: 0,
                researcher_calls: 0,
                terminal_calls: 0,
                tool_calls: 0,
                tool_failures: 0,
            });

        match trace.role() {
            "conductor" => entry.conductor_calls += 1,
            "researcher" => entry.researcher_calls += 1,
            "terminal" => entry.terminal_calls += 1,
            _ => {}
        }
        let ts = trace.timestamp();
        if ts > entry.timestamp {
            entry.timestamp = ts;
        }
    }

    for tool in tools {
        let Some(run_id) = tool.run_id.as_ref() else {
            continue;
        };
        let entry = by_run
            .entry(run_id.clone())
            .or_insert_with(|| RunGraphSummary {
                run_id: run_id.clone(),
                objective: "Objective unavailable".to_string(),
                timestamp: tool.timestamp.clone(),
                conductor_calls: 0,
                researcher_calls: 0,
                terminal_calls: 0,
                tool_calls: 0,
                tool_failures: 0,
            });
        if tool.event_type == "worker.tool.call" {
            entry.tool_calls += 1;
        }
        if tool.event_type == "worker.tool.result" && tool.success == Some(false) {
            entry.tool_failures += 1;
        }
    }

    let mut result: Vec<RunGraphSummary> = by_run.into_values().collect();
    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    result
}

fn group_traces(events: &[TraceEvent]) -> Vec<TraceGroup> {
    let mut groups: std::collections::HashMap<String, TraceGroup> =
        std::collections::HashMap::new();

    for event in events {
        let trace_id = event.trace_id.clone();
        let entry = groups.entry(trace_id.clone()).or_insert(TraceGroup {
            trace_id: trace_id.clone(),
            started: None,
            terminal: None,
        });

        match event.event_type.as_str() {
            "llm.call.started" => {
                entry.started = Some(event.clone());
            }
            "llm.call.completed" | "llm.call.failed" => {
                entry.terminal = Some(event.clone());
            }
            _ => {}
        }
    }

    let mut result: Vec<TraceGroup> = groups.into_values().collect();
    result.sort_by(|a, b| {
        let a_ts = a.timestamp();
        let b_ts = b.timestamp();
        b_ts.cmp(&a_ts)
    });
    result
}

#[component]
pub fn TraceView(desktop_id: String, window_id: String) -> Element {
    let mut trace_events = use_signal(Vec::<TraceEvent>::new);
    let mut prompt_events = use_signal(Vec::<PromptEvent>::new);
    let mut tool_events = use_signal(Vec::<ToolTraceEvent>::new);
    let mut since_seq = use_signal(|| 0i64);
    let mut selected_trace_id = use_signal(|| None::<String>);
    let mut selected_run_id = use_signal(|| None::<String>);
    let mut connected = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut ws_runtime = use_signal(|| None::<TraceRuntime>);
    let mut preload_started = use_signal(|| false);
    let mut preload_ready = use_signal(|| false);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<TraceWsEvent>::new())));
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

            spawn(async move {
                let latest_seq = match fetch_latest_log_seq().await {
                    Ok(seq) => seq,
                    Err(_) => 0,
                };
                let preload_since = latest_seq.saturating_sub(1_000);

                match fetch_logs_events(preload_since, 1_000, None).await {
                    Ok(events) => {
                        let mut max_seq = latest_seq;
                        let mut parsed = Vec::<TraceEvent>::new();
                        let mut parsed_prompts = Vec::<PromptEvent>::new();
                        let mut parsed_tools = Vec::<ToolTraceEvent>::new();
                        for event in events {
                            max_seq = max_seq.max(event.seq);
                            if let Some(trace) = parse_trace_event(&event) {
                                parsed.push(trace);
                            }
                            if let Some(prompt) = parse_prompt_event(&event) {
                                parsed_prompts.push(prompt);
                            }
                            if let Some(tool_event) = parse_tool_trace_event(&event) {
                                parsed_tools.push(tool_event);
                            }
                        }
                        parsed.sort_by_key(|e| e.seq);
                        parsed.dedup_by(|a, b| a.event_id == b.event_id);
                        parsed_prompts.sort_by_key(|e| e.seq);
                        parsed_prompts.dedup_by(|a, b| a.event_id == b.event_id);
                        parsed_tools.sort_by_key(|e| e.seq);
                        parsed_tools.dedup_by(|a, b| a.event_id == b.event_id);
                        trace_events.set(parsed);
                        prompt_events.set(parsed_prompts);
                        tool_events.set(parsed_tools);
                        since_seq.set(max_seq);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to preload traces: {e}")));
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
                            TraceWsEvent::Connected => {
                                connected.set(true);
                                error.set(None);
                            }
                            TraceWsEvent::Message(text_str) => {
                                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str)
                                else {
                                    continue;
                                };

                                match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                                    "connected" | "pong" => {
                                        connected.set(true);
                                    }
                                    "event" => {
                                        let logs_event = LogsEvent {
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
                                            payload: json.get("payload").cloned().unwrap_or(serde_json::Value::Null),
                                        };

                                        let next_seq = since_seq().max(logs_event.seq);
                                        since_seq.set(next_seq);

                                        if let Some(trace_event) = parse_trace_event(&logs_event) {
                                            let mut list = trace_events.write();
                                            list.push(trace_event);
                                            list.sort_by_key(|e| e.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 1000 {
                                                let trim = list.len() - 1000;
                                                list.drain(0..trim);
                                            }
                                        }
                                        if let Some(prompt_event) = parse_prompt_event(&logs_event) {
                                            let mut list = prompt_events.write();
                                            list.push(prompt_event);
                                            list.sort_by_key(|e| e.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 300 {
                                                let trim = list.len() - 300;
                                                list.drain(0..trim);
                                            }
                                        }
                                        if let Some(tool_event) = parse_tool_trace_event(&logs_event) {
                                            let mut list = tool_events.write();
                                            list.push(tool_event);
                                            list.sort_by_key(|e| e.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 2000 {
                                                let trim = list.len() - 2000;
                                                list.drain(0..trim);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            TraceWsEvent::Error(message) => {
                                connected.set(false);
                                error.set(Some(message));
                                ws_runtime.set(None);
                            }
                            TraceWsEvent::Closed => {
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

            let ws_url = build_trace_ws_url(since_seq());
            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    error.set(Some(format!("trace websocket open failed: {e:?}")));
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let ws_event_queue_open = ws_event_queue.clone();
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                ws_event_queue_open
                    .borrow_mut()
                    .push_back(TraceWsEvent::Connected);
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
                    .push_back(TraceWsEvent::Message(text_str));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let ws_event_queue_error = ws_event_queue.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                ws_event_queue_error
                    .borrow_mut()
                    .push_back(TraceWsEvent::Error(e.message()));
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
                    .push_back(TraceWsEvent::Closed);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            ws_runtime.set(Some(TraceRuntime {
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
    let snapshot = trace_events.read().clone();
    let prompt_snapshot = prompt_events.read().clone();
    let tool_snapshot = tool_events.read().clone();
    let traces_all = group_traces(&snapshot);
    let run_summaries = build_run_graph_summaries(&traces_all, &prompt_snapshot, &tool_snapshot);
    let active_run_id = selected_run_id()
        .filter(|id| run_summaries.iter().any(|r| r.run_id == *id))
        .or_else(|| run_summaries.first().map(|r| r.run_id.clone()));
    let traces: Vec<TraceGroup> = if let Some(run_id) = active_run_id.as_deref() {
        traces_all
            .into_iter()
            .filter(|t| t.run_id() == Some(run_id))
            .collect()
    } else {
        traces_all
    };
    let selected_run = active_run_id
        .as_ref()
        .and_then(|id| run_summaries.iter().find(|r| r.run_id == *id))
        .cloned();
    let active_trace_id = selected_trace_id()
        .filter(|id| traces.iter().any(|t| t.trace_id == *id))
        .or_else(|| traces.first().map(|t| t.trace_id.clone()));
    let selected_trace = active_trace_id
        .as_ref()
        .and_then(|id| traces.iter().find(|t| t.trace_id == *id))
        .cloned();

    rsx! {
        style { {CHAT_STYLES} }
        div {
            class: "chat-container",
            style: "padding: 0.75rem; overflow: auto;",
            div {
                class: "chat-header",
                h3 { "LLM Traces" }
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
                    "Trace stream error: {message}"
                }
            }
            if !run_summaries.is_empty() {
                div {
                    style: "display: flex; gap: 0.4rem; overflow-x: auto; padding: 0.35rem 0 0.6rem 0;",
                    for run in &run_summaries {
                        button {
                            style: if active_run_id.as_deref() == Some(run.run_id.as_str()) {
                                "background:#1d4ed8;color:white;border:1px solid #2563eb;border-radius:6px;padding:0.3rem 0.55rem;font-size:0.7rem;white-space:nowrap;cursor:pointer;"
                            } else {
                                "background:var(--bg-secondary,#1f2937);color:var(--text-secondary,#9ca3af);border:1px solid var(--border-color,#374151);border-radius:6px;padding:0.3rem 0.55rem;font-size:0.7rem;white-space:nowrap;cursor:pointer;"
                            },
                            onclick: {
                                let run_id = run.run_id.clone();
                                move |_| selected_run_id.set(Some(run_id.clone()))
                            },
                            "{run.run_id}"
                        }
                    }
                }
            }
            if run_summaries.is_empty() && traces.is_empty() {
                div {
                    class: "empty-state",
                    div { class: "empty-icon", "🔍" }
                    p { "No tracing graph data available yet" }
                    span { "Trace reads prompt, llm.call, and worker.tool events from /logs/events and streams live" }
                }
            } else {
                div {
                    class: "chat-body",
                    aside {
                        class: "thread-sidebar",
                        div {
                            class: "thread-sidebar-header",
                            span { "Traces" }
                            span { "{traces.len()}" }
                        }
                        div {
                            class: "thread-list",
                            for trace in traces {
                                button {
                                    class: if active_trace_id.as_deref() == Some(trace.trace_id.as_str()) {
                                        "thread-item active"
                                    } else {
                                        "thread-item"
                                    },
                                    onclick: {
                                        let trace_id = trace.trace_id.clone();
                                        move |_| selected_trace_id.set(Some(trace_id.clone()))
                                    },
                                    div {
                                        class: "thread-title",
                                        "{trace.role()} / {trace.function_name()}"
                                    }
                                    div {
                                        class: "thread-preview",
                                        span {
                                            style: match trace.status() {
                                                "completed" => "color: #22c55e;",
                                                "failed" => "color: #ef4444;",
                                                "started" => "color: #f59e0b;",
                                                _ => "",
                                            },
                                            "{trace.status()} "
                                        }
                                        span { "{trace.model_used()} " }
                                        if let Some(duration) = trace.duration_ms() {
                                            span { "{duration}ms" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "messages-scroll-area",
                        if let Some(run) = selected_run.clone() {
                            div {
                                style: "margin: 0.25rem 0 0.8rem 0; padding: 0.6rem; border: 1px solid var(--border-color,#374151); border-radius: 10px; background: color-mix(in srgb, var(--bg-secondary,#111827) 88%, #0b1225 12%);",
                                h4 {
                                    style: "margin: 0 0 0.35rem 0; color: var(--text-primary, white);",
                                    "Run Graph"
                                }
                                p {
                                    style: "margin: 0 0 0.45rem 0; font-size: 0.78rem; color: var(--text-secondary,#9ca3af);",
                                    "{run.objective}"
                                }
                                div {
                                    style: "overflow-x: auto;",
                                    svg {
                                        width: "860",
                                        height: "250",
                                        view_box: "0 0 860 250",
                                        line { x1: "140", y1: "70", x2: "300", y2: "70", stroke: "#475569", stroke_width: "2" }
                                        line { x1: "440", y1: "70", x2: "590", y2: "40", stroke: "#475569", stroke_width: "2" }
                                        line { x1: "440", y1: "70", x2: "590", y2: "110", stroke: "#475569", stroke_width: "2" }
                                        line { x1: "710", y1: "40", x2: "790", y2: "105", stroke: "#334155", stroke_width: "2" }
                                        line { x1: "710", y1: "110", x2: "790", y2: "105", stroke: "#334155", stroke_width: "2" }

                                        rect { x: "20", y: "42", width: "120", height: "56", rx: "8", fill: "#0f172a", stroke: "#475569" }
                                        text { x: "30", y: "64", fill: "#93c5fd", font_size: "12", "User Prompt" }
                                        text { x: "30", y: "82", fill: "#cbd5e1", font_size: "11", "run {run.run_id}" }

                                        rect { x: "300", y: "42", width: "140", height: "56", rx: "8", fill: "#111827", stroke: "#3b82f6" }
                                        text { x: "312", y: "64", fill: "#60a5fa", font_size: "12", "Conductor" }
                                        text { x: "312", y: "82", fill: "#cbd5e1", font_size: "11", "{run.conductor_calls} llm calls" }

                                        rect { x: "590", y: "16", width: "120", height: "48", rx: "8", fill: "#0b1225", stroke: "#22c55e" }
                                        text { x: "602", y: "36", fill: "#86efac", font_size: "12", "Researcher" }
                                        text { x: "602", y: "52", fill: "#cbd5e1", font_size: "11", "{run.researcher_calls} llm" }

                                        rect { x: "590", y: "86", width: "120", height: "48", rx: "8", fill: "#0b1225", stroke: "#f59e0b" }
                                        text { x: "602", y: "106", fill: "#fcd34d", font_size: "12", "Terminal" }
                                        text { x: "602", y: "122", fill: "#cbd5e1", font_size: "11", "{run.terminal_calls} llm" }

                                        rect { x: "790", y: "81", width: "62", height: "48", rx: "8", fill: "#111827", stroke: "#06b6d4" }
                                        text { x: "797", y: "101", fill: "#67e8f9", font_size: "11", "Tools" }
                                        text { x: "797", y: "117", fill: "#cbd5e1", font_size: "10", "{run.tool_calls}" }
                                    }
                                }
                                div {
                                    style: "display:flex; gap:0.5rem; flex-wrap:wrap; font-size:0.72rem; color: var(--text-secondary,#9ca3af);",
                                    span { "tool failures: {run.tool_failures}" }
                                    span { "researcher llm: {run.researcher_calls}" }
                                    span { "terminal llm: {run.terminal_calls}" }
                                }
                            }
                        }
                        if let Some(trace) = selected_trace {
                            div {
                                class: "trace-detail",
                                style: "padding: 0.5rem;",
                                div {
                                    class: "trace-header",
                                    style: "margin-bottom: 1rem; padding-bottom: 0.75rem; border-bottom: 1px solid var(--border-color, #374151);",
                                    h4 {
                                        style: "margin: 0 0 0.5rem 0; color: var(--text-primary, white);",
                                        "{trace.role()} / {trace.function_name()}"
                                    }
                                    div {
                                        style: "display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center;",
                                        span {
                                            style: match trace.status() {
                                                "completed" => "background: #166534; color: white; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600;",
                                                "failed" => "background: #991b1b; color: white; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600;",
                                                "started" => "background: #854d0e; color: white; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600;",
                                                _ => "",
                                            },
                                            "{trace.status()}"
                                        }
                                        span {
                                            style: "background: var(--bg-secondary, #374151); color: var(--text-secondary, #9ca3af); padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem;",
                                            "{trace.model_used()}"
                                        }
                                        if let Some(provider) = trace.provider() {
                                            span {
                                                style: "background: var(--bg-secondary, #374151); color: var(--text-secondary, #9ca3af); padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem;",
                                                "{provider}"
                                            }
                                        }
                                        if let Some(duration) = trace.duration_ms() {
                                            span {
                                                style: "color: var(--text-muted, #6b7280); font-size: 0.75rem;",
                                                "{duration}ms"
                                            }
                                        }
                                    }
                                }

                                div {
                                    style: "display: flex; flex-wrap: wrap; gap: 0.25rem; margin-bottom: 1rem;",
                                    if let Some(run_id) = trace.run_id() {
                                        span {
                                            style: "background: #1e3a5f; color: #60a5fa; padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.625rem;",
                                            "run:{run_id}"
                                        }
                                    }
                                    if let Some(call_id) = trace.call_id() {
                                        span {
                                            style: "background: #1e5f3b; color: #34d399; padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.625rem;",
                                            "call:{call_id}"
                                        }
                                    }
                                    if let Some(session_id) = trace.session_id() {
                                        span {
                                            style: "background: #5f3b1e; color: #fbbf24; padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.625rem;",
                                            "session:{session_id}"
                                        }
                                    }
                                    if let Some(thread_id) = trace.thread_id() {
                                        span {
                                            style: "background: #5f1e3b; color: #f472b6; padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.625rem;",
                                            "thread:{thread_id}"
                                        }
                                    }
                                }

                                div {
                                    style: "margin-bottom: 0.5rem; font-size: 0.75rem; color: var(--text-muted, #6b7280);",
                                    "trace_id: {trace.trace_id} | actor: {trace.actor_id()} | {trace.timestamp()}"
                                }

                                if let Some(started) = &trace.started {
                                    if let Some(system_context) = &started.system_context {
                                        details {
                                            class: "tool-details",
                                            summary { class: "tool-summary", "System Context" }
                                            div {
                                                class: "tool-body",
                                                pre {
                                                    class: "tool-pre",
                                                    style: "max-height: 200px; overflow: auto;",
                                                    "{system_context}"
                                                }
                                            }
                                        }
                                    }

                                    if let Some(input) = &started.input {
                                        details {
                                            class: "tool-details",
                                            summary { class: "tool-summary", "Input Payload" }
                                            div {
                                                class: "tool-body",
                                                if let Some(summary) = &started.input_summary {
                                                    p {
                                                        class: "tool-meta",
                                                        style: "font-style: italic; color: var(--text-secondary, #9ca3af);",
                                                        "{summary}"
                                                    }
                                                }
                                                pre {
                                                    class: "tool-pre",
                                                    style: "max-height: 300px; overflow: auto;",
                                                    "{serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string())}"
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(terminal) = &trace.terminal {
                                    if terminal.event_type == "llm.call.completed" {
                                        if let Some(output) = &terminal.output {
                                            details {
                                                class: "tool-details",
                                                open: true,
                                                summary { class: "tool-summary", "Output Payload" }
                                                div {
                                                    class: "tool-body",
                                                    if let Some(summary) = &terminal.output_summary {
                                                        p {
                                                            class: "tool-meta",
                                                            style: "font-style: italic; color: var(--text-secondary, #9ca3af);",
                                                            "{summary}"
                                                        }
                                                    }
                                                    pre {
                                                        class: "tool-pre",
                                                        style: "max-height: 400px; overflow: auto;",
                                                        "{serde_json::to_string_pretty(output).unwrap_or_else(|_| output.to_string())}"
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    if terminal.event_type == "llm.call.failed" {
                                        details {
                                            class: "tool-details",
                                            open: true,
                                            summary {
                                                class: "tool-summary",
                                                style: "color: #ef4444;",
                                                "Error"
                                            }
                                            div {
                                                class: "tool-body",
                                                if let Some(code) = &terminal.error_code {
                                                    p {
                                                        class: "tool-meta",
                                                        "Error Code: {code}"
                                                    }
                                                }
                                                if let Some(kind) = &terminal.failure_kind {
                                                    p {
                                                        class: "tool-meta",
                                                        "Failure Kind: {kind}"
                                                    }
                                                }
                                                if let Some(message) = &terminal.error_message {
                                                    pre {
                                                        class: "tool-pre",
                                                        style: "color: #fca5a5; background: #450a0a;",
                                                        "{message}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "empty-state",
                                div { class: "empty-icon", "🔍" }
                                p { "Select a trace from the sidebar" }
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

fn build_trace_ws_url(since_seq: i64) -> String {
    let ws_base = http_to_ws_url(crate::api::api_base());
    format!(
        "{}/ws/logs/events?since_seq={}&limit=300&poll_ms=200",
        ws_base,
        since_seq.max(0)
    )
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
