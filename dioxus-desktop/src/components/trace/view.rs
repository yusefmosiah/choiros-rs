use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use web_sys::WebSocket;

use crate::api::{fetch_latest_log_seq, fetch_logs_events, LogsEvent};

use super::graph::{
    build_graph_edges, build_graph_layout, build_graph_nodes_for_run, build_run_graph_summaries,
    display_actor_label, graph_node_color, graph_status_color, run_status_class,
};
use super::parsers::{
    group_traces, parse_conductor_delegation_event,
    parse_conductor_run_event, parse_prompt_event, parse_tool_trace_event, parse_trace_event,
    parse_worker_lifecycle_event, parse_writer_enqueue_event, pretty_json,
};
use super::styles::TRACE_VIEW_STYLES;
use super::trajectory::{
    build_delegation_timeline_bands, build_loop_groups_for_actor, build_run_sparkline,
    build_trajectory_cells,
};
use super::types::{
    ConductorDelegationEvent, ConductorRunEvent, GraphEdgeSegment, GraphNodeKind, GraphRenderNode,
    LoopSequenceItem, PromptEvent, ToolTraceEvent, TraceEvent, TraceViewMode, TrajectoryMode,
    WorkerLifecycleEvent, WriterEnqueueEvent, TRACE_PRELOAD_PAGE_LIMIT, TRACE_PRELOAD_WINDOW,
    TRACE_SLOW_DURATION_MS,
};
use super::ws::{build_trace_ws_url, TraceRuntime, TraceWsEvent};
use super::trajectory::TrajectoryGrid;

use super::super::styles::CHAT_STYLES;

// ── Formatting helpers ───────────────────────────────────────────────────────

pub fn format_relative_time(timestamp: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };
    let now = js_sys::Date::now() as i64;
    let then_ms = dt.timestamp_millis();
    let diff_secs = ((now - then_ms) / 1000).max(0);
    match diff_secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", diff_secs / 60),
        3600..=86399 => format!("{}h ago", diff_secs / 3600),
        _ => format!("{}d ago", diff_secs / 86400),
    }
}

pub fn format_duration_short(ms: i64) -> String {
    if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

pub fn format_tokens_short(tokens: i64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

// ── View helper fns ──────────────────────────────────────────────────────────

fn format_loop_title(loop_id: &str) -> String {
    if loop_id == "direct" {
        "Direct LLM calls".to_string()
    } else if loop_id.starts_with("call:") {
        format!("Capability {}", loop_id.trim_start_matches("call:"))
    } else {
        format!("Agent loop {loop_id}")
    }
}

fn lifecycle_chip_class(event_type: &str) -> &'static str {
    match event_type {
        "worker.task.started" => "trace-lifecycle-chip trace-lifecycle-chip--started",
        "worker.task.progress" => "trace-lifecycle-chip trace-lifecycle-chip--progress",
        "worker.task.completed" => "trace-lifecycle-chip trace-lifecycle-chip--completed",
        "worker.task.failed" => "trace-lifecycle-chip trace-lifecycle-chip--failed",
        "worker.task.finding" => "trace-lifecycle-chip trace-lifecycle-chip--finding",
        "worker.task.learning" => "trace-lifecycle-chip trace-lifecycle-chip--learning",
        _ => "trace-lifecycle-chip",
    }
}

fn lifecycle_label(event: &WorkerLifecycleEvent) -> String {
    match event.event_type.as_str() {
        "worker.task.started" => "started".to_string(),
        "worker.task.progress" => "progress".to_string(),
        "worker.task.completed" => "completed".to_string(),
        "worker.task.failed" => "failed".to_string(),
        "worker.task.finding" => "finding".to_string(),
        "worker.task.learning" => "learning".to_string(),
        _ => event.event_type.clone(),
    }
}

fn lifecycle_detail(event: &WorkerLifecycleEvent) -> String {
    event
        .objective
        .as_ref()
        .cloned()
        .or_else(|| event.message.as_ref().cloned())
        .or_else(|| event.summary.as_ref().cloned())
        .or_else(|| event.error.as_ref().cloned())
        .or_else(|| event.claim.as_ref().cloned())
        .or_else(|| event.insight.as_ref().cloned())
        .unwrap_or_else(|| "No details".to_string())
}

fn worker_summary(task_id: &str, events: &[WorkerLifecycleEvent]) -> (&'static str, Option<String>) {
    let mut task_events: Vec<&WorkerLifecycleEvent> = events
        .iter()
        .filter(|event| event.task_id == task_id)
        .collect();
    task_events.sort_by_key(|event| event.seq);
    if let Some(terminal) = task_events.iter().rev().find(|event| {
        matches!(
            event.event_type.as_str(),
            "worker.task.completed" | "worker.task.failed"
        )
    }) {
        let status = if terminal.event_type == "worker.task.failed" {
            "failed"
        } else {
            "completed"
        };
        return (status, Some(lifecycle_detail(terminal)));
    }
    let latest = task_events.last().map(|event| lifecycle_detail(event));
    ("running", latest)
}

fn delegation_band_class(status: &str) -> &'static str {
    match status {
        "completed" => "trace-delegation-band trace-delegation-band--completed",
        "failed" => "trace-delegation-band trace-delegation-band--failed",
        "blocked" => "trace-delegation-band trace-delegation-band--blocked",
        _ => "trace-delegation-band trace-delegation-band--inflight",
    }
}

fn sanitize_dom_fragment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn loop_dom_id(loop_id: &str) -> String {
    format!("trace-loop-{}", sanitize_dom_fragment(loop_id))
}

fn item_dom_id(item_id: &str) -> String {
    format!("trace-item-{}", sanitize_dom_fragment(item_id))
}

fn scroll_to_element_id(id: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(element) = document.get_element_by_id(id) {
            element.scroll_into_view();
        }
    }
}

// ── TraceView component ──────────────────────────────────────────────────────

#[component]
pub fn TraceView(desktop_id: String, window_id: String) -> Element {
    let mut trace_events = use_signal(Vec::<TraceEvent>::new);
    let mut prompt_events = use_signal(Vec::<PromptEvent>::new);
    let mut tool_events = use_signal(Vec::<ToolTraceEvent>::new);
    let mut writer_enqueue_events = use_signal(Vec::<WriterEnqueueEvent>::new);
    let mut delegation_events = use_signal(Vec::<ConductorDelegationEvent>::new);
    let mut run_events = use_signal(Vec::<ConductorRunEvent>::new);
    let mut worker_lifecycle = use_signal(Vec::<WorkerLifecycleEvent>::new);
    let mut since_seq = use_signal(|| 0_i64);
    let mut selected_run_id = use_signal(|| None::<String>);
    let mut selected_actor_key = use_signal(|| None::<String>);
    let mut selected_loop_id = use_signal(|| None::<String>);
    let mut selected_item_id = use_signal(|| None::<String>);
    let mut trajectory_mode = use_signal(|| TrajectoryMode::Status);
    let mut run_sidebar_open = use_signal(|| true);
    let mut node_sheet_open = use_signal(|| false);
    let mut view_mode = use_signal(|| TraceViewMode::Overview);
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
                let latest_seq = fetch_latest_log_seq().await.unwrap_or(0);
                let preload_since = latest_seq.saturating_sub(TRACE_PRELOAD_WINDOW);
                let mut cursor = preload_since;
                let mut fetched_events = Vec::<LogsEvent>::new();

                loop {
                    match fetch_logs_events(cursor, TRACE_PRELOAD_PAGE_LIMIT, None).await {
                        Ok(page) => {
                            if page.is_empty() {
                                break;
                            }
                            let last_seq = page.last().map(|event| event.seq).unwrap_or(cursor);
                            fetched_events.extend(page);
                            if fetched_events.len() >= TRACE_PRELOAD_WINDOW as usize {
                                break;
                            }
                            if last_seq <= cursor {
                                break;
                            }
                            cursor = last_seq;
                        }
                        Err(fetch_error) => {
                            error.set(Some(format!("Failed to preload trace data: {fetch_error}")));
                            preload_ready.set(true);
                            return;
                        }
                    }
                }

                if fetched_events.len() > TRACE_PRELOAD_WINDOW as usize {
                    let keep = TRACE_PRELOAD_WINDOW as usize;
                    let trim = fetched_events.len() - keep;
                    fetched_events.drain(0..trim);
                }

                let mut max_seq = latest_seq;
                let mut parsed_traces = Vec::<TraceEvent>::new();
                let mut parsed_prompts = Vec::<PromptEvent>::new();
                let mut parsed_tools = Vec::<ToolTraceEvent>::new();
                let mut parsed_writer_enqueues = Vec::<WriterEnqueueEvent>::new();
                let mut parsed_delegations = Vec::<ConductorDelegationEvent>::new();
                let mut parsed_run_events = Vec::<ConductorRunEvent>::new();
                let mut parsed_worker_lifecycle = Vec::<WorkerLifecycleEvent>::new();

                for event in fetched_events {
                    max_seq = max_seq.max(event.seq);
                    if let Some(trace_event) = parse_trace_event(&event) {
                        parsed_traces.push(trace_event);
                    }
                    if let Some(prompt_event) = parse_prompt_event(&event) {
                        parsed_prompts.push(prompt_event);
                    }
                    if let Some(tool_event) = parse_tool_trace_event(&event) {
                        parsed_tools.push(tool_event);
                    }
                    if let Some(writer_enqueue_event) = parse_writer_enqueue_event(&event) {
                        parsed_writer_enqueues.push(writer_enqueue_event);
                    }
                    if let Some(delegation_event) = parse_conductor_delegation_event(&event) {
                        parsed_delegations.push(delegation_event);
                    }
                    if let Some(run_event) = parse_conductor_run_event(&event) {
                        parsed_run_events.push(run_event);
                    }
                    if let Some(lifecycle_event) = parse_worker_lifecycle_event(&event) {
                        parsed_worker_lifecycle.push(lifecycle_event);
                    }
                }

                parsed_traces.sort_by_key(|event| event.seq);
                parsed_traces.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_prompts.sort_by_key(|event| event.seq);
                parsed_prompts.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_tools.sort_by_key(|event| event.seq);
                parsed_tools.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_writer_enqueues.sort_by_key(|event| event.seq);
                parsed_writer_enqueues.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_delegations.sort_by_key(|event| event.seq);
                parsed_delegations.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_run_events.sort_by_key(|event| event.seq);
                parsed_run_events.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_worker_lifecycle.sort_by_key(|event| event.seq);
                parsed_worker_lifecycle.dedup_by(|a, b| a.event_id == b.event_id);

                trace_events.set(parsed_traces);
                prompt_events.set(parsed_prompts);
                tool_events.set(parsed_tools);
                writer_enqueue_events.set(parsed_writer_enqueues);
                delegation_events.set(parsed_delegations);
                run_events.set(parsed_run_events);
                worker_lifecycle.set(parsed_worker_lifecycle);
                since_seq.set(max_seq);

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
                        while let Some(ws_event) = queue.pop_front() {
                            drained.push(ws_event);
                        }
                    }

                    for ws_event in drained {
                        match ws_event {
                            TraceWsEvent::Connected => {
                                connected.set(true);
                                error.set(None);
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
                            TraceWsEvent::Message(text) => {
                                let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&text)
                                else {
                                    continue;
                                };
                                match json
                                    .get("type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                {
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
                                            payload: json
                                                .get("payload")
                                                .cloned()
                                                .unwrap_or(serde_json::Value::Null),
                                        };

                                        since_seq.set(since_seq().max(logs_event.seq));

                                        if let Some(trace_event) =
                                            parse_trace_event(&logs_event)
                                        {
                                            let mut list = trace_events.write();
                                            list.push(trace_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 1_000 {
                                                let trim = list.len() - 1_000;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(prompt_event) =
                                            parse_prompt_event(&logs_event)
                                        {
                                            let mut list = prompt_events.write();
                                            list.push(prompt_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 300 {
                                                let trim = list.len() - 300;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(tool_event) =
                                            parse_tool_trace_event(&logs_event)
                                        {
                                            let mut list = tool_events.write();
                                            list.push(tool_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 2_000 {
                                                let trim = list.len() - 2_000;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(writer_enqueue_event) =
                                            parse_writer_enqueue_event(&logs_event)
                                        {
                                            let mut list = writer_enqueue_events.write();
                                            list.push(writer_enqueue_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 1_000 {
                                                let trim = list.len() - 1_000;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(delegation_event) =
                                            parse_conductor_delegation_event(&logs_event)
                                        {
                                            let mut list = delegation_events.write();
                                            list.push(delegation_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 2_000 {
                                                let trim = list.len() - 2_000;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(run_event) =
                                            parse_conductor_run_event(&logs_event)
                                        {
                                            let mut list = run_events.write();
                                            list.push(run_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 1_500 {
                                                let trim = list.len() - 1_500;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(lifecycle_event) =
                                            parse_worker_lifecycle_event(&logs_event)
                                        {
                                            let mut list = worker_lifecycle.write();
                                            list.push(lifecycle_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 3_000 {
                                                let trim = list.len() - 3_000;
                                                list.drain(0..trim);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
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
                Err(err) => {
                    error.set(Some(format!("trace websocket open failed: {err:?}")));
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let queue_open = ws_event_queue.clone();
            let on_open = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::Event| {
                queue_open.borrow_mut().push_back(TraceWsEvent::Connected);
            }) as Box<dyn FnMut(web_sys::Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let queue_message = ws_event_queue.clone();
            let on_message =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                    let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                        return;
                    };
                    let text_string = text.as_string().unwrap_or_default();
                    queue_message
                        .borrow_mut()
                        .push_back(TraceWsEvent::Message(text_string));
                }) as Box<dyn FnMut(web_sys::MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let queue_error = ws_event_queue.clone();
            let on_error =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
                    queue_error
                        .borrow_mut()
                        .push_back(TraceWsEvent::Error(e.message()));
                }) as Box<dyn FnMut(web_sys::ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let queue_close = ws_event_queue.clone();
            let closing_for_close = closing.clone();
            let on_close =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::CloseEvent| {
                    if closing_for_close.get() {
                        return;
                    }
                    queue_close.borrow_mut().push_back(TraceWsEvent::Closed);
                }) as Box<dyn FnMut(web_sys::CloseEvent)>);
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
    let trace_snapshot = trace_events.read().clone();
    let prompt_snapshot = prompt_events.read().clone();
    let tool_snapshot = tool_events.read().clone();
    let writer_enqueue_snapshot = writer_enqueue_events.read().clone();
    let delegation_snapshot = delegation_events.read().clone();
    let run_event_snapshot = run_events.read().clone();
    let lifecycle_snapshot = worker_lifecycle.read().clone();

    let traces_all = group_traces(&trace_snapshot);
    let run_summaries = build_run_graph_summaries(
        &traces_all,
        &prompt_snapshot,
        &tool_snapshot,
        &writer_enqueue_snapshot,
        &delegation_snapshot,
        &run_event_snapshot,
        &lifecycle_snapshot,
    );

    let active_run_id = selected_run_id()
        .filter(|run_id| run_summaries.iter().any(|run| run.run_id == *run_id))
        .or_else(|| run_summaries.first().map(|run| run.run_id.clone()));

    let traces_for_run: Vec<super::types::TraceGroup> =
        if let Some(run_id) = active_run_id.as_deref() {
            traces_all
                .iter()
                .filter(|trace| trace.run_id() == Some(run_id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
    let tools_for_run: Vec<ToolTraceEvent> = if let Some(run_id) = active_run_id.as_deref() {
        tool_snapshot
            .iter()
            .filter(|tool| tool.run_id.as_deref() == Some(run_id))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let selected_run = active_run_id
        .as_ref()
        .and_then(|run_id| run_summaries.iter().find(|run| run.run_id == *run_id))
        .cloned();
    let delegations_for_run: Vec<ConductorDelegationEvent> =
        if let Some(run_id) = active_run_id.as_deref() {
            delegation_snapshot
                .iter()
                .filter(|event| event.run_id == run_id)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
    let lifecycle_for_run: Vec<WorkerLifecycleEvent> =
        if let Some(run_id) = active_run_id.as_deref() {
            lifecycle_snapshot
                .iter()
                .filter(|event| event.run_id.as_deref() == Some(run_id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
    let delegation_bands = if let Some(run_id) = active_run_id.as_deref() {
        build_delegation_timeline_bands(run_id, &delegations_for_run, &lifecycle_for_run)
    } else {
        Vec::new()
    };
    let trajectory_cells_for_run = if let Some(run_id) = active_run_id.as_deref() {
        build_trajectory_cells(
            &traces_for_run,
            &tools_for_run,
            &lifecycle_for_run,
            &delegations_for_run,
            run_id,
        )
    } else {
        Vec::new()
    };

    let graph_nodes = if let Some(run_id) = active_run_id.as_deref() {
        build_graph_nodes_for_run(
            run_id,
            &traces_for_run,
            &tools_for_run,
            &writer_enqueue_snapshot,
            &lifecycle_for_run,
        )
    } else {
        Vec::new()
    };
    let graph_edges = if let Some(run_id) = active_run_id.as_deref() {
        build_graph_edges(&graph_nodes, run_id, &delegations_for_run)
    } else {
        Vec::new()
    };
    let graph_layout = build_graph_layout(&graph_nodes);

    let actor_nodes: Vec<super::types::GraphNode> = graph_nodes
        .iter()
        .filter(|node| node.kind == GraphNodeKind::Actor)
        .cloned()
        .collect();
    let active_actor_key = selected_actor_key()
        .filter(|actor_key| {
            actor_nodes.iter().any(|node| {
                node.actor_key
                    .as_ref()
                    .map(|current| current == actor_key)
                    .unwrap_or(false)
            })
        })
        .or_else(|| actor_nodes.first().and_then(|node| node.actor_key.clone()));
    let loop_groups = active_actor_key
        .as_deref()
        .map(|actor_key| {
            build_loop_groups_for_actor(actor_key, &traces_for_run, &tools_for_run)
        })
        .unwrap_or_default();
    let active_actor_label = active_actor_key
        .as_deref()
        .map(display_actor_label)
        .unwrap_or_else(|| "Actor".to_string());

    let graph_render_nodes: Vec<GraphRenderNode> = graph_nodes
        .iter()
        .filter_map(|node| {
            graph_layout
                .positions
                .get(&node.key)
                .map(|(x, y)| GraphRenderNode {
                    node: node.clone(),
                    x: *x,
                    y: *y,
                })
        })
        .collect();
    let graph_edge_segments: Vec<GraphEdgeSegment> = graph_edges
        .iter()
        .filter_map(|edge| {
            let from_pos = graph_layout.positions.get(&edge.from)?;
            let to_pos = graph_layout.positions.get(&edge.to)?;
            Some(GraphEdgeSegment {
                edge: edge.clone(),
                x1: from_pos.0 + 188.0,
                y1: from_pos.1 + 33.0,
                x2: to_pos.0,
                y2: to_pos.1 + 33.0,
            })
        })
        .collect();
    let graph_width = graph_layout.width.max(720.0);
    let graph_height = graph_layout.height.max(220.0);
    let graph_view_box = format!("0 0 {:.0} {:.0}", graph_width, graph_height);
    let graph_width_attr = format!("{:.0}", graph_width);
    let graph_height_attr = format!("{:.0}", graph_height);

    let run_sidebar_class = if run_sidebar_open() {
        "thread-sidebar"
    } else {
        "thread-sidebar collapsed"
    };
    let panel_class = if node_sheet_open() {
        "trace-node-panel open"
    } else {
        "trace-node-panel"
    };

    let is_overview = view_mode() == TraceViewMode::Overview;

    rsx! {
        style { {CHAT_STYLES} }
        style { {TRACE_VIEW_STYLES} }
        div {
            class: "chat-container",
            style: "padding: 0; overflow: hidden;",
            div {
                class: "chat-header",
                style: "padding: 0.35rem 0.6rem;",
                div {
                    class: "trace-header-actions",
                    if !is_overview {
                        button {
                            class: "trace-back-btn",
                            onclick: move |_| {
                                view_mode.set(TraceViewMode::Overview);
                                selected_run_id.set(None);
                                selected_actor_key.set(None);
                                selected_loop_id.set(None);
                                selected_item_id.set(None);
                                node_sheet_open.set(false);
                            },
                            "← Runs"
                        }
                        button {
                            class: "trace-run-toggle",
                            onclick: {
                                let next = !run_sidebar_open();
                                move |_| run_sidebar_open.set(next)
                            },
                            if run_sidebar_open() {
                                "Hide List"
                            } else {
                                "List"
                            }
                        }
                    } else {
                        {
                            let run_count_label = if run_summaries.len() == 1 {
                                "1 run".to_string()
                            } else {
                                format!("{} runs", run_summaries.len())
                            };
                            rsx! {
                                span {
                                    style: "font-size: 0.75rem; color: var(--text-secondary); font-weight: 500;",
                                    "{run_count_label}"
                                }
                            }
                        }
                    }
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
            }
            if let Some(message) = error() {
                div {
                    class: "message-bubble system-bubble",
                    style: "margin: 0.6rem;",
                    "Trace stream error: {message}"
                }
            }
            if run_summaries.is_empty() {
                div {
                    class: "empty-state",
                    p { "No tracing graph data available yet." }
                    span { "Trace reads llm.call and worker.tool events from /logs/events and streams live updates." }
                }
            } else if is_overview {
                // ── Overview: card grid of all runs ──
                div {
                    class: "trace-overview-grid",
                    for run in &run_summaries {
                        {
                            let run_id = run.run_id.clone();
                            let run_id_for_title = run_id.clone();
                            let objective = run.objective.clone();
                            let display_title = if objective.is_empty() {
                                run_id[..run_id.len().min(20)].to_string()
                            } else {
                                objective[..objective.len().min(80)].to_string()
                            };
                            let status_class = run_status_class(&run.run_status);
                            let status_label_text = run.run_status.clone();
                            let llm_calls = run.llm_calls;
                            let tool_calls = run.tool_calls;
                            let worker_calls = run.worker_calls;
                            let total_dur = run.total_duration_ms;
                            let timestamp = run.timestamp.clone();
                            let tool_failures = run.tool_failures;
                            let sparkline_dots =
                                build_run_sparkline(&run.run_id, &traces_all, &tool_snapshot);
                            rsx! {
                                div {
                                    class: "trace-run-card",
                                    onclick: move |_| {
                                        selected_run_id.set(Some(run_id.clone()));
                                        selected_actor_key.set(None);
                                        selected_loop_id.set(None);
                                        selected_item_id.set(None);
                                        node_sheet_open.set(false);
                                        view_mode.set(TraceViewMode::RunDetail);
                                    },
                                    div {
                                        class: "trace-run-card-title",
                                        title: "{run_id_for_title}",
                                        "{display_title}"
                                    }
                                    div {
                                        class: "trace-run-card-meta",
                                        span { class: "{status_class}", "{status_label_text}" }
                                        if tool_failures > 0 {
                                            span { style: "color: #ef4444;", "{tool_failures} failures" }
                                        }
                                    }
                                    div {
                                        class: "trace-run-card-meta",
                                        span { "{llm_calls} llm" }
                                        span { "·" }
                                        span { "{tool_calls} tools" }
                                        span { "·" }
                                        span { "{worker_calls} workers" }
                                        if total_dur > 0 {
                                            span { "·" }
                                            span { "{format_duration_short(total_dur)}" }
                                        }
                                    }
                                    div {
                                        class: "trace-run-card-footer",
                                        svg {
                                            class: "trace-run-sparkline",
                                            view_box: "0 0 120 16",
                                            for (x, y, color) in &sparkline_dots {
                                                circle {
                                                    cx: format!("{:.1}", x),
                                                    cy: format!("{:.1}", y),
                                                    r: "2.5",
                                                    fill: "{color}"
                                                }
                                            }
                                        }
                                        span {
                                            class: "trace-run-card-time",
                                            "{format_relative_time(&timestamp)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div {
                    class: "chat-body",
                    aside {
                        class: "{run_sidebar_class}",
                        div {
                            class: "thread-sidebar-header",
                            span { "Runs" }
                            span { "{run_summaries.len()}" }
                        }
                        div {
                            class: "thread-list",
                            for run in &run_summaries {
                                button {
                                    class: if active_run_id.as_deref() == Some(run.run_id.as_str()) {
                                        "thread-item trace-run-toggle active"
                                    } else {
                                        "thread-item trace-run-toggle"
                                    },
                                    onclick: {
                                        let run_id = run.run_id.clone();
                                        move |_| {
                                            selected_run_id.set(Some(run_id.clone()));
                                            selected_actor_key.set(None);
                                            selected_loop_id.set(None);
                                            selected_item_id.set(None);
                                            node_sheet_open.set(false);
                                            run_sidebar_open.set(false);
                                            view_mode.set(TraceViewMode::RunDetail);
                                        }
                                    },
                                    div {
                                        class: "thread-title",
                                        title: "{run.run_id}",
                                        if run.objective.is_empty() {
                                            "{&run.run_id[..run.run_id.len().min(16)]}"
                                        } else {
                                            "{&run.objective[..run.objective.len().min(60)]}"
                                        }
                                    }
                                    div {
                                        class: "thread-preview",
                                        "{run.llm_calls} llm | {run.tool_calls} tools | {run.actor_count} actors"
                                    }
                                    div {
                                        class: "trace-run-row",
                                        div {
                                            class: "trace-run-row-left",
                                            span {
                                                class: "{run_status_class(&run.run_status)}",
                                                "{run.run_status}"
                                            }
                                            span { class: "trace-pill", "{run.worker_calls} worker calls" }
                                        }
                                        svg {
                                            class: "trace-run-sparkline",
                                            view_box: "0 0 120 16",
                                            for (x, y, color) in build_run_sparkline(
                                                &run.run_id,
                                                &traces_all,
                                                &tool_snapshot
                                            ) {
                                                circle {
                                                    cx: format!("{:.1}", x),
                                                    cy: format!("{:.1}", y),
                                                    r: "2.5",
                                                    fill: "{color}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "messages-scroll-area trace-main",
                        if let Some(run) = selected_run {
                            div {
                                class: "trace-graph-card",
                                div {
                                    class: "trace-graph-head",
                                    div {
                                        h4 { class: "trace-graph-title", "Run Graph" }
                                        p { class: "trace-graph-objective", "{run.objective}" }
                                    }
                                    div {
                                        class: "trace-graph-metrics",
                                        span { class: "{run_status_class(&run.run_status)}", "{run.run_status}" }
                                        span { class: "trace-pill", "{run.llm_calls} llm calls" }
                                        span { class: "trace-pill", "{run.tool_calls} tool calls" }
                                        span { class: "trace-pill", "{run.tool_failures} tool failures" }
                                        span { class: "trace-pill", "{run.worker_count} workers" }
                                        span { class: "trace-pill", "{run.worker_failures} worker failures" }
                                        span { class: "trace-pill", "{run.worker_calls} worker calls" }
                                        if run.capability_failures > 0 {
                                            span { class: "trace-pill", "{run.capability_failures} capability failures" }
                                        }
                                        span { class: "trace-pill", "{run.writer_enqueues} writer enqueues" }
                                        if run.writer_enqueue_failures > 0 {
                                            span { class: "trace-pill", "{run.writer_enqueue_failures} enqueue failures" }
                                        }
                                        span { class: "trace-pill", "{format_duration_short(run.total_duration_ms)}" }
                                        span { class: "trace-pill", "{format_tokens_short(run.total_tokens)} tok" }
                                        span { class: "trace-pill", "{run.loop_count} loops" }
                                    }
                                }
                                div {
                                    class: "trace-graph-scroll",
                                    svg {
                                        width: "{graph_width_attr}",
                                        height: "{graph_height_attr}",
                                        view_box: "{graph_view_box}",
                                        for segment in &graph_edge_segments {
                                            line {
                                                x1: format!("{:.1}", segment.x1),
                                                y1: format!("{:.1}", segment.y1),
                                                x2: format!("{:.1}", segment.x2),
                                                y2: format!("{:.1}", segment.y2),
                                                stroke: "{segment.edge.color}",
                                                stroke_width: "2",
                                                stroke_dasharray: if segment.edge.dashed { "6,4" } else { "none" }
                                            }
                                            if let Some(label) = segment.edge.label.as_ref() {
                                                text {
                                                    x: format!("{:.1}", (segment.x1 + segment.x2) / 2.0),
                                                    y: format!("{:.1}", ((segment.y1 + segment.y2) / 2.0) - 4.0),
                                                    text_anchor: "middle",
                                                    fill: "#cbd5e1",
                                                    font_size: "9.2",
                                                    "{label}"
                                                }
                                            }
                                        }
                                        for render in &graph_render_nodes {
                                            {
                                                let (fill, stroke, label_color) =
                                                    graph_node_color(&render.node);
                                                let selected = render
                                                    .node
                                                    .actor_key
                                                    .as_ref()
                                                    .zip(active_actor_key.as_ref())
                                                    .map(|(node_actor, active_actor)| {
                                                        node_actor == active_actor
                                                    })
                                                    .unwrap_or(false);
                                                let stroke_color = if selected { "#60a5fa" } else { stroke };
                                                let metrics = if render.node.kind == GraphNodeKind::Tools {
                                                    format!("{} calls", render.node.tool_calls)
                                                } else if render.node.inbound_events > 0 {
                                                    format!(
                                                        "{} llm / {} tools / {} inbound",
                                                        render.node.llm_calls,
                                                        render.node.tool_calls,
                                                        render.node.inbound_events
                                                    )
                                                } else {
                                                    format!(
                                                        "{} llm / {} tools",
                                                        render.node.llm_calls,
                                                        render.node.tool_calls
                                                    )
                                                };
                                                let status_color =
                                                    graph_status_color(&render.node.status);
                                                let actor_key_for_click =
                                                    render.node.actor_key.clone();
                                                rsx! {
                                                    g {
                                                        onclick: move |_| {
                                                            if let Some(actor_key) =
                                                                actor_key_for_click.clone()
                                                            {
                                                                selected_actor_key
                                                                    .set(Some(actor_key));
                                                                selected_loop_id.set(None);
                                                                selected_item_id.set(None);
                                                                node_sheet_open.set(true);
                                                            }
                                                        },
                                                        rect {
                                                            x: format!("{:.1}", render.x),
                                                            y: format!("{:.1}", render.y),
                                                            width: "188",
                                                            height: "66",
                                                            class: if render.node.kind == GraphNodeKind::Worker {
                                                                "trace-worker-node"
                                                            } else {
                                                                ""
                                                            },
                                                            rx: "8",
                                                            fill: "{fill}",
                                                            stroke: "{stroke_color}",
                                                            stroke_width: if selected { "2.5" } else { "1.5" }
                                                        }
                                                        text {
                                                            x: format!("{:.1}", render.x + 12.0),
                                                            y: format!("{:.1}", render.y + 23.0),
                                                            fill: "{label_color}",
                                                            font_size: "12",
                                                            font_weight: "600",
                                                            "{render.node.label}"
                                                        }
                                                        text {
                                                            x: format!("{:.1}", render.x + 12.0),
                                                            y: format!("{:.1}", render.y + 41.0),
                                                            fill: "#cbd5e1",
                                                            font_size: "10.5",
                                                            "{metrics}"
                                                        }
                                                        text {
                                                            x: format!("{:.1}", render.x + 12.0),
                                                            y: format!("{:.1}", render.y + 56.0),
                                                            fill: "{status_color}",
                                                            font_size: "10",
                                                            "{render.node.status}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !delegation_bands.is_empty() {
                                    div {
                                        class: "trace-delegation-wrap",
                                        h5 {
                                            class: "trace-loop-title",
                                            "Delegation Timeline"
                                        }
                                        div {
                                            class: "trace-delegation-timeline",
                                            for band in &delegation_bands {
                                                button {
                                                    class: "{delegation_band_class(&band.status)}",
                                                    title: band.worker_objective.clone().unwrap_or_default(),
                                                    onclick: {
                                                        let loop_id = band.loop_id.clone();
                                                        let call_id = band.call_id.clone();
                                                        move |_| {
                                                            if let Some(loop_id) = loop_id.clone() {
                                                                let dom_id = loop_dom_id(&loop_id);
                                                                selected_loop_id
                                                                    .set(Some(loop_id));
                                                                scroll_to_element_id(&dom_id);
                                                            } else if let Some(call_id) =
                                                                call_id.clone()
                                                            {
                                                                let fallback =
                                                                    format!("call:{call_id}");
                                                                selected_loop_id.set(Some(
                                                                    fallback.clone(),
                                                                ));
                                                                scroll_to_element_id(
                                                                    &loop_dom_id(&fallback),
                                                                );
                                                            }
                                                        }
                                                    },
                                                    span { "{band.worker_type}" }
                                                    span { class: "trace-pill", "{band.status}" }
                                                    if let Some(duration_ms) = band.duration_ms {
                                                        span { class: "trace-pill", "{duration_ms}ms" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !trajectory_cells_for_run.is_empty() {
                                    TrajectoryGrid {
                                        cells: trajectory_cells_for_run.clone(),
                                        display_mode: trajectory_mode(),
                                        on_mode_change: move |next_mode| {
                                            trajectory_mode.set(next_mode)
                                        },
                                        on_select: move |payload: (String, String)| {
                                            let (loop_id, item_id) = payload;
                                            let dom_loop_id = loop_dom_id(&loop_id);
                                            let dom_item_id = item_dom_id(&item_id);
                                            selected_loop_id.set(Some(loop_id.clone()));
                                            selected_item_id.set(Some(item_id.clone()));
                                            scroll_to_element_id(&dom_loop_id);
                                            scroll_to_element_id(&dom_item_id);
                                        }
                                    }
                                }
                                if !actor_nodes.is_empty() {
                                    div {
                                        class: "trace-node-chip-row",
                                        for node in &actor_nodes {
                                            if let Some(actor_key) = node.actor_key.clone() {
                                                button {
                                                    class: if active_actor_key.as_deref()
                                                        == Some(actor_key.as_str())
                                                    {
                                                        "trace-node-chip active"
                                                    } else {
                                                        "trace-node-chip"
                                                    },
                                                    onclick: {
                                                        let actor_key = actor_key.clone();
                                                        move |_| {
                                                            selected_actor_key
                                                                .set(Some(actor_key.clone()));
                                                            selected_loop_id.set(None);
                                                            selected_item_id.set(None);
                                                            node_sheet_open.set(true);
                                                        }
                                                    },
                                                    "{node.label} ({node.llm_calls})"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if active_actor_key.is_some() && node_sheet_open() {
                            div {
                                class: "trace-mobile-backdrop",
                                onclick: move |_| node_sheet_open.set(false)
                            }
                        }

                        if active_actor_key.is_some() {
                            div {
                                class: "{panel_class}",
                                div {
                                    class: "trace-node-panel-head",
                                    h4 {
                                        class: "trace-node-title",
                                        "{active_actor_label}"
                                    }
                                    button {
                                        class: "trace-node-close",
                                        onclick: move |_| node_sheet_open.set(false),
                                        "Close"
                                    }
                                }
                                if loop_groups.is_empty() {
                                    div {
                                        class: "empty-state",
                                        p { "No calls recorded for this actor in the selected run." }
                                    }
                                } else {
                                    for (index, group) in loop_groups.iter().enumerate() {
                                        {
                                            let group_loop_id = group.loop_id.clone();
                                            let selected = selected_loop_id()
                                                .as_ref()
                                                .map(|loop_id| loop_id == &group_loop_id)
                                                .unwrap_or(false);
                                            let group_dom_id = loop_dom_id(&group_loop_id);
                                            let lifecycle_for_group: Vec<WorkerLifecycleEvent> =
                                                lifecycle_for_run
                                                    .iter()
                                                    .filter(|event| {
                                                        if group_loop_id.starts_with("call:") {
                                                            let expected_call = group_loop_id
                                                                .trim_start_matches("call:");
                                                            event.call_id.as_deref()
                                                                == Some(expected_call)
                                                        } else {
                                                            event.task_id == group_loop_id
                                                        }
                                                    })
                                                    .cloned()
                                                    .collect();
                                            let (worker_state, worker_state_message) =
                                                worker_summary(
                                                    &group_loop_id,
                                                    &lifecycle_for_run,
                                                );
                                            rsx! {
                                        div {
                                            id: "{group_dom_id}",
                                            class: if selected {
                                                "trace-loop-group trace-call-card--selected"
                                            } else {
                                                "trace-loop-group"
                                            },
                                            div {
                                                class: "trace-loop-head",
                                                h5 {
                                                    class: "trace-loop-title",
                                                    "Loop {index + 1}: {format_loop_title(&group.loop_id)}"
                                                }
                                                span {
                                                    class: "trace-pill",
                                                    "{group.sequence.len()} events"
                                                }
                                                span {
                                                    class: "trace-pill",
                                                    "{worker_state}"
                                                }
                                                if let Some(message) =
                                                    worker_state_message.as_ref()
                                                {
                                                    span {
                                                        class: "trace-pill",
                                                        style: "max-width:320px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                                                        "{message}"
                                                    }
                                                }
                                            }
                                            if !lifecycle_for_group.is_empty() {
                                                div {
                                                    class: "trace-lifecycle-strip",
                                                    for event in &lifecycle_for_group {
                                                        details {
                                                            class: "{lifecycle_chip_class(&event.event_type)}",
                                                            summary {
                                                                "{lifecycle_label(event)}: {event.phase}"
                                                            }
                                                            p {
                                                                class: "tool-meta",
                                                                "{lifecycle_detail(event)}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if let Some(system_context) = group
                                                .traces
                                                .iter()
                                                .find_map(|trace| trace.started.as_ref().and_then(|started| started.system_context.as_ref()))
                                            {
                                                details {
                                                    class: "tool-details",
                                                    summary { class: "tool-summary", "System Context (shared for loop)" }
                                                    div {
                                                        class: "tool-body",
                                                        pre {
                                                            class: "tool-pre",
                                                            style: "max-height:180px;overflow:auto;",
                                                            "{system_context}"
                                                        }
                                                    }
                                                }
                                            }
                                            for item in &group.sequence {
                                                match item {
                                                    LoopSequenceItem::Llm(trace) => rsx! {
                                                        div {
                                                            id: "{item_dom_id(&trace.trace_id)}",
                                                            class: if selected_item_id().as_deref() == Some(trace.trace_id.as_str()) {
                                                                "trace-call-card trace-call-card--selected"
                                                            } else {
                                                                "trace-call-card"
                                                            },
                                                            div {
                                                                class: "trace-call-top",
                                                                h6 {
                                                                    class: "trace-call-title",
                                                                    "{trace.function_name()}"
                                                                }
                                                                div {
                                                                    style: "display:flex;gap:0.3rem;flex-wrap:wrap;justify-content:flex-end;",
                                                                    span {
                                                                        class: "trace-pill",
                                                                        "{trace.status()}"
                                                                    }
                                                                    span {
                                                                        class: "trace-pill",
                                                                        "{trace.model_used()}"
                                                                    }
                                                                    if let Some(provider) = trace.provider() {
                                                                        span {
                                                                            class: "trace-pill",
                                                                            "{provider}"
                                                                        }
                                                                    }
                                                                    if let Some(duration) = trace.duration_ms() {
                                                                        span {
                                                                            class: "trace-pill",
                                                                            "{duration}ms"
                                                                        }
                                                                    }
                                                                    if let Some(tokens) = trace.total_tokens() {
                                                                        span {
                                                                            class: "trace-pill",
                                                                            "{tokens} tok"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(duration) = trace.duration_ms() {
                                                                {
                                                                    let loop_max = group
                                                                        .sequence
                                                                        .iter()
                                                                        .filter_map(|item| match item {
                                                                            LoopSequenceItem::Llm(current) => current.duration_ms(),
                                                                            LoopSequenceItem::Tool(current) => current.duration_ms(),
                                                                        })
                                                                        .max()
                                                                        .unwrap_or(1)
                                                                        .max(1);
                                                                    let width_pct = ((duration.max(0) as f64 / loop_max as f64) * 100.0).clamp(2.0, 100.0);
                                                                    rsx! {
                                                                        div {
                                                                            class: if duration > TRACE_SLOW_DURATION_MS {
                                                                                "trace-duration-bar trace-duration-bar--slow"
                                                                            } else {
                                                                                "trace-duration-bar"
                                                                            },
                                                                            style: format!("width:{width_pct:.2}%;")
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(total_tokens) = trace.total_tokens() {
                                                                {
                                                                    let loop_total_tokens = group
                                                                        .traces
                                                                        .iter()
                                                                        .filter_map(|current| current.total_tokens())
                                                                        .sum::<i64>()
                                                                        .max(1);
                                                                    let cached = trace.cached_input_tokens().unwrap_or(0).max(0);
                                                                    let input = trace.input_tokens().unwrap_or(0).max(0);
                                                                    let output = trace.output_tokens().unwrap_or(0).max(0);
                                                                    let cached_pct = (cached as f64 / loop_total_tokens as f64 * 100.0).clamp(0.0, 100.0);
                                                                    let input_pct = (input as f64 / loop_total_tokens as f64 * 100.0).clamp(0.0, 100.0);
                                                                    let output_pct = (output as f64 / loop_total_tokens as f64 * 100.0).clamp(0.0, 100.0);
                                                                    rsx! {
                                                                        div {
                                                                            class: "trace-token-bar",
                                                                            div {
                                                                                class: "trace-token-segment--cached",
                                                                                style: format!("width:{cached_pct:.2}%;")
                                                                            }
                                                                            div {
                                                                                class: "trace-token-segment--input",
                                                                                style: format!("width:{input_pct:.2}%;")
                                                                            }
                                                                            div {
                                                                                class: "trace-token-segment--output",
                                                                                style: format!("width:{output_pct:.2}%;")
                                                                            }
                                                                        }
                                                                        div {
                                                                            style: "font-size:0.66rem;color:#94a3b8;margin-top:0.22rem;",
                                                                            "{format_tokens_short(input)} in / {format_tokens_short(output)} out / {format_tokens_short(cached)} cached / {format_tokens_short(total_tokens)} total"
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            div {
                                                                style: "display:flex;gap:0.25rem;flex-wrap:wrap;margin-bottom:0.35rem;",
                                                                if let Some(task_id) = trace.task_id() {
                                                                    span {
                                                                        style: "background:#1e3a5f;color:#93c5fd;padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "task:{task_id}"
                                                                    }
                                                                }
                                                                if let Some(call_id) = trace.call_id() {
                                                                    span {
                                                                        style: "background:#1e5f3b;color:#6ee7b7;padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "call:{call_id}"
                                                                    }
                                                                }
                                                                if let Some(input_tokens) = trace.input_tokens() {
                                                                    span {
                                                                        style: "background:var(--bg-secondary,#1f2937);color:var(--text-secondary,#d1d5db);padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "in:{input_tokens}"
                                                                    }
                                                                }
                                                                if let Some(output_tokens) = trace.output_tokens() {
                                                                    span {
                                                                        style: "background:var(--bg-secondary,#1f2937);color:var(--text-secondary,#d1d5db);padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "out:{output_tokens}"
                                                                    }
                                                                }
                                                                if let Some(cached_tokens) = trace.cached_input_tokens() {
                                                                    span {
                                                                        style: "background:var(--bg-secondary,#1f2937);color:var(--text-secondary,#d1d5db);padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "cached:{cached_tokens}"
                                                                    }
                                                                }
                                                            }

                                                            div {
                                                                style: "font-size:0.7rem;color:var(--text-muted,#64748b);margin-bottom:0.35rem;",
                                                                "trace_id: {trace.trace_id} | actor: {trace.actor_id()} | {trace.timestamp()}"
                                                            }

                                                            if let Some(started) = &trace.started {
                                                                if let Some(input) = &started.input {
                                                                    details {
                                                                        class: "tool-details",
                                                                        summary { class: "tool-summary", "Input Payload" }
                                                                        div {
                                                                            class: "tool-body",
                                                                            if let Some(summary) = &started.input_summary {
                                                                                p {
                                                                                    class: "tool-meta",
                                                                                    style: "font-style:italic;color:var(--text-secondary,#9ca3af);",
                                                                                    "{summary}"
                                                                                }
                                                                            }
                                                                            pre {
                                                                                class: "tool-pre",
                                                                                style: "max-height:260px;overflow:auto;",
                                                                                "{pretty_json(input)}"
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
                                                                                        style: "font-style:italic;color:var(--text-secondary,#9ca3af);",
                                                                                        "{summary}"
                                                                                    }
                                                                                }
                                                                                pre {
                                                                                    class: "tool-pre",
                                                                                    style: "max-height:300px;overflow:auto;",
                                                                                    "{pretty_json(output)}"
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
                                                                            style: "color:#ef4444;",
                                                                            "Error"
                                                                        }
                                                                        div {
                                                                            class: "tool-body",
                                                                            if let Some(code) = &terminal.error_code {
                                                                                p { class: "tool-meta", "Error Code: {code}" }
                                                                            }
                                                                            if let Some(kind) = &terminal.failure_kind {
                                                                                p { class: "tool-meta", "Failure Kind: {kind}" }
                                                                            }
                                                                            if let Some(message) = &terminal.error_message {
                                                                                pre {
                                                                                    class: "tool-pre",
                                                                                    style: "color:#fca5a5;background:#450a0a;",
                                                                                    "{message}"
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                    LoopSequenceItem::Tool(tool_pair) => rsx! {
                                                        div {
                                                            id: "{item_dom_id(&tool_pair.tool_trace_id)}",
                                                            class: if selected_item_id().as_deref() == Some(tool_pair.tool_trace_id.as_str()) {
                                                                "trace-call-card trace-call-card--selected"
                                                            } else {
                                                                "trace-call-card"
                                                            },
                                                            div {
                                                                class: "trace-call-top",
                                                                h6 {
                                                                    class: "trace-call-title",
                                                                    "Tool: {tool_pair.tool_name()}"
                                                                }
                                                                div {
                                                                    style: "display:flex;gap:0.3rem;flex-wrap:wrap;justify-content:flex-end;",
                                                                    span { class: "trace-pill", "{tool_pair.status()}" }
                                                                    if let Some(duration) = tool_pair.duration_ms() {
                                                                        span { class: "trace-pill", "{duration}ms" }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(duration) = tool_pair.duration_ms() {
                                                                {
                                                                    let loop_max = group
                                                                        .sequence
                                                                        .iter()
                                                                        .filter_map(|item| match item {
                                                                            LoopSequenceItem::Llm(current) => current.duration_ms(),
                                                                            LoopSequenceItem::Tool(current) => current.duration_ms(),
                                                                        })
                                                                        .max()
                                                                        .unwrap_or(1)
                                                                        .max(1);
                                                                    let width_pct = ((duration.max(0) as f64 / loop_max as f64) * 100.0).clamp(2.0, 100.0);
                                                                    rsx! {
                                                                        div {
                                                                            class: if duration > TRACE_SLOW_DURATION_MS {
                                                                                "trace-duration-bar trace-duration-bar--slow"
                                                                            } else {
                                                                                "trace-duration-bar"
                                                                            },
                                                                            style: format!("width:{width_pct:.2}%;")
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div {
                                                                style: "font-size:0.7rem;color:var(--text-muted,#64748b);margin-bottom:0.35rem;",
                                                                "tool_trace_id: {tool_pair.tool_trace_id}"
                                                            }
                                                            if let Some(call) = &tool_pair.call {
                                                                details {
                                                                    class: "tool-details",
                                                                    summary { class: "tool-summary", "Tool Call Input" }
                                                                    div {
                                                                        class: "tool-body",
                                                                        if let Some(reasoning) = &call.reasoning {
                                                                            p { class: "tool-meta", "{reasoning}" }
                                                                        }
                                                                        if let Some(args) = &call.tool_args {
                                                                            pre {
                                                                                class: "tool-pre",
                                                                                style: "max-height:220px;overflow:auto;",
                                                                                "{pretty_json(args)}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(result) = &tool_pair.result {
                                                                details {
                                                                    class: "tool-details",
                                                                    open: true,
                                                                    summary { class: "tool-summary", "Tool Result Output" }
                                                                    div {
                                                                        class: "tool-body",
                                                                        if let Some(output) = &result.output {
                                                                            pre {
                                                                                class: "tool-pre",
                                                                                style: "max-height:260px;overflow:auto;",
                                                                                "{pretty_json(output)}"
                                                                            }
                                                                        }
                                                                        if let Some(error) = &result.error {
                                                                            pre {
                                                                                class: "tool-pre",
                                                                                style: "color:#fca5a5;background:#450a0a;",
                                                                                "{error}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                }
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
                                p { "Select an actor node in the graph to inspect loop inputs and outputs." }
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
