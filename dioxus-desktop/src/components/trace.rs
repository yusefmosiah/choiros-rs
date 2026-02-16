use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use crate::api::{fetch_latest_log_seq, fetch_logs_events, LogsEvent};

use super::styles::CHAT_STYLES;

const TRACE_VIEW_STYLES: &str = r#"
.trace-header-actions {
    display: flex;
    align-items: center;
    gap: 0.45rem;
}

.trace-run-toggle {
    background: #13213d;
    border: 1px solid #2f4f7a;
    color: #dbeafe;
    border-radius: 0.45rem;
    padding: 0.32rem 0.55rem;
    font-size: 0.72rem;
    cursor: pointer;
}

.trace-run-toggle:hover {
    background: #1c3157;
}

.trace-main {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
}

.trace-graph-card {
    border: 1px solid var(--border-color, #334155);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-secondary, #111827) 86%, #0b1225 14%);
    padding: 0.7rem;
}

.trace-graph-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.6rem;
    margin-bottom: 0.55rem;
}

.trace-graph-title {
    margin: 0;
    color: var(--text-primary, white);
    font-size: 1rem;
}

.trace-graph-objective {
    margin: 0.15rem 0 0 0;
    font-size: 0.78rem;
    color: var(--text-secondary, #9ca3af);
    line-height: 1.35;
}

.trace-graph-metrics {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    justify-content: flex-end;
}

.trace-pill {
    border: 1px solid #334155;
    background: #0f172a;
    color: #cbd5e1;
    border-radius: 999px;
    font-size: 0.68rem;
    padding: 0.2rem 0.45rem;
    white-space: nowrap;
}

.trace-graph-scroll {
    overflow-x: auto;
    border-radius: 8px;
    border: 1px solid #1f2a44;
    background: #0b1222;
}

.trace-node-chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.55rem;
}

.trace-node-chip {
    border: 1px solid #334155;
    background: #111b32;
    color: #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.3rem 0.5rem;
    font-size: 0.72rem;
    cursor: pointer;
}

.trace-node-chip:hover {
    border-color: #4b6587;
}

.trace-node-chip.active {
    border-color: #3b82f6;
    background: #13213d;
    color: #dbeafe;
}

.trace-node-panel {
    border: 1px solid var(--border-color, #334155);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-secondary, #111827) 88%, #020617 12%);
    padding: 0.6rem;
}

.trace-node-panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.4rem;
}

.trace-node-title {
    margin: 0;
    color: var(--text-primary, #f8fafc);
    font-size: 0.95rem;
}

.trace-node-close {
    display: none;
    background: #111b32;
    border: 1px solid #334155;
    color: #cbd5e1;
    border-radius: 0.4rem;
    font-size: 0.72rem;
    padding: 0.28rem 0.5rem;
    cursor: pointer;
}

.trace-loop-group {
    border: 1px solid #1f2a44;
    border-radius: 8px;
    background: #0b1222;
    margin-top: 0.55rem;
    padding: 0.45rem;
}

.trace-loop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
}

.trace-loop-title {
    margin: 0;
    color: #bfdbfe;
    font-size: 0.78rem;
    font-weight: 600;
}

.trace-call-card {
    border: 1px solid #1f2a44;
    border-radius: 8px;
    background: #081022;
    padding: 0.42rem;
    margin-top: 0.35rem;
}

.trace-call-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-bottom: 0.35rem;
}

.trace-call-title {
    margin: 0;
    color: #dbeafe;
    font-size: 0.78rem;
    font-weight: 600;
}

.trace-mobile-backdrop {
    display: none;
}

@media (max-width: 1024px) {
    .trace-node-panel {
        position: fixed;
        left: 0;
        right: 0;
        bottom: 0;
        max-height: 76vh;
        overflow: auto;
        border-radius: 12px 12px 0 0;
        border-bottom: none;
        z-index: 48;
        transform: translateY(105%);
        transition: transform 0.18s ease;
        margin: 0;
    }

    .trace-node-panel.open {
        transform: translateY(0);
    }

    .trace-node-close {
        display: inline-flex;
    }

    .trace-mobile-backdrop {
        display: block;
        position: fixed;
        inset: 0;
        background: rgba(2, 6, 23, 0.65);
        z-index: 45;
    }
}
"#;
const TRACE_PRELOAD_WINDOW: i64 = 5_000;
const TRACE_PRELOAD_PAGE_LIMIT: i64 = 1_000;

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
    task_id: Option<String>,
    call_id: Option<String>,
    system_context: Option<String>,
    input: Option<serde_json::Value>,
    input_summary: Option<String>,
    output: Option<serde_json::Value>,
    output_summary: Option<String>,
    duration_ms: Option<i64>,
    error_code: Option<String>,
    error_message: Option<String>,
    failure_kind: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    total_tokens: Option<i64>,
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
struct ToolTraceEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    tool_trace_id: String,
    timestamp: String,
    role: String,
    actor_id: String,
    tool_name: String,
    run_id: Option<String>,
    task_id: Option<String>,
    call_id: Option<String>,
    success: Option<bool>,
    duration_ms: Option<i64>,
    reasoning: Option<String>,
    tool_args: Option<serde_json::Value>,
    output: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct WriterEnqueueEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    run_id: String,
    call_id: Option<String>,
}

#[derive(Clone, Debug)]
struct RunGraphSummary {
    run_id: String,
    objective: String,
    timestamp: String,
    llm_calls: usize,
    tool_calls: usize,
    tool_failures: usize,
    writer_enqueues: usize,
    writer_enqueue_failures: usize,
    actor_count: usize,
    loop_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GraphNodeKind {
    Prompt,
    Actor,
    Tools,
}

#[derive(Clone, Debug)]
struct GraphNode {
    key: String,
    label: String,
    kind: GraphNodeKind,
    actor_key: Option<String>,
    llm_calls: usize,
    tool_calls: usize,
    inbound_events: usize,
    status: String,
}

#[derive(Clone, Debug)]
struct GraphLayout {
    width: f32,
    height: f32,
    positions: HashMap<String, (f32, f32)>,
}

#[derive(Clone, Debug)]
struct GraphRenderNode {
    node: GraphNode,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct GraphEdgeSegment {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

#[derive(Clone, Debug)]
struct TraceLoopGroup {
    loop_id: String,
    traces: Vec<TraceGroup>,
    sequence: Vec<LoopSequenceItem>,
}

#[derive(Clone, Debug)]
struct ToolTracePair {
    tool_trace_id: String,
    call: Option<ToolTraceEvent>,
    result: Option<ToolTraceEvent>,
}

#[derive(Clone, Debug)]
enum LoopSequenceItem {
    Llm(TraceGroup),
    Tool(ToolTracePair),
}

impl TraceGroup {
    fn status(&self) -> &'static str {
        if let Some(terminal) = &self.terminal {
            match terminal.event_type.as_str() {
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

    fn seq(&self) -> i64 {
        self.started
            .as_ref()
            .map(|e| e.seq)
            .or_else(|| self.terminal.as_ref().map(|e| e.seq))
            .unwrap_or(0)
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
            .or_else(|| self.terminal.as_ref().and_then(|e| e.provider.as_deref()))
    }

    fn actor_id(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.actor_id.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.actor_id.as_str()))
            .unwrap_or("unknown")
    }

    fn actor_key(&self) -> String {
        normalize_actor_key(self.role(), self.actor_id())
    }

    fn run_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.run_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.run_id.as_deref()))
    }

    fn task_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.task_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.task_id.as_deref()))
    }

    fn call_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.call_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.call_id.as_deref()))
    }

    fn duration_ms(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.duration_ms)
            .or_else(|| self.started.as_ref().and_then(|s| s.duration_ms))
    }

    fn input_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.input_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.input_tokens))
    }

    fn output_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.output_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.output_tokens))
    }

    fn cached_input_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.cached_input_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.cached_input_tokens))
    }

    fn total_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.total_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.total_tokens))
            .or_else(|| match (self.input_tokens(), self.output_tokens()) {
                (Some(input), Some(output)) => Some(input.saturating_add(output)),
                (Some(input), None) => Some(input),
                (None, Some(output)) => Some(output),
                (None, None) => None,
            })
    }
}

impl ToolTraceEvent {
    fn actor_key(&self) -> String {
        normalize_actor_key(&self.role, &self.actor_id)
    }

    fn loop_id(&self) -> String {
        self.task_id
            .clone()
            .or_else(|| {
                self.call_id
                    .clone()
                    .map(|call_id| format!("call:{call_id}"))
            })
            .unwrap_or_else(|| "direct".to_string())
    }
}

impl ToolTracePair {
    fn seq(&self) -> i64 {
        self.call
            .as_ref()
            .map(|event| event.seq)
            .or_else(|| self.result.as_ref().map(|event| event.seq))
            .unwrap_or(0)
    }

    fn tool_name(&self) -> &str {
        self.call
            .as_ref()
            .map(|event| event.tool_name.as_str())
            .or_else(|| self.result.as_ref().map(|event| event.tool_name.as_str()))
            .unwrap_or("unknown")
    }

    fn status(&self) -> &'static str {
        if let Some(result) = &self.result {
            if result.success == Some(true) {
                "completed"
            } else {
                "failed"
            }
        } else if self.call.is_some() {
            "started"
        } else {
            "unknown"
        }
    }

    fn duration_ms(&self) -> Option<i64> {
        self.result
            .as_ref()
            .and_then(|event| event.duration_ms)
            .or_else(|| self.call.as_ref().and_then(|event| event.duration_ms))
    }
}

fn parse_trace_event(event: &LogsEvent) -> Option<TraceEvent> {
    if !event.event_type.starts_with("llm.call.") {
        return None;
    }

    let payload = &event.payload;
    let usage = payload.get("usage").and_then(|v| v.as_object());

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
        provider: payload
            .get("provider")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        actor_id: payload
            .get("actor_id")
            .and_then(|v| v.as_str())
            .unwrap_or(event.actor_id.as_str())
            .to_string(),
        run_id: payload_run_id(payload),
        task_id: payload
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        call_id: payload
            .get("call_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        system_context: payload
            .get("system_context")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        input: decode_json_payload(payload.get("input")),
        input_summary: payload
            .get("input_summary")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        output: decode_json_payload(payload.get("output")),
        output_summary: payload
            .get("output_summary")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        duration_ms: payload.get("duration_ms").and_then(|v| v.as_i64()),
        error_code: payload
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        error_message: payload
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        failure_kind: payload
            .get("failure_kind")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        input_tokens: usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| payload.get("input_tokens").and_then(|v| v.as_i64())),
        output_tokens: usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| payload.get("output_tokens").and_then(|v| v.as_i64())),
        cached_input_tokens: usage
            .and_then(|u| u.get("cached_input_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| payload.get("cached_input_tokens").and_then(|v| v.as_i64())),
        total_tokens: usage
            .and_then(|u| u.get("total_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| payload.get("total_tokens").and_then(|v| v.as_i64())),
    })
}

fn parse_prompt_event(event: &LogsEvent) -> Option<PromptEvent> {
    if event.event_type != "trace.prompt.received" && event.event_type != "conductor.task.started" {
        return None;
    }

    let payload = &event.payload;
    let run_id = payload_run_id(payload)?;
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
        actor_id: payload
            .get("actor_id")
            .and_then(|v| v.as_str())
            .unwrap_or(event.actor_id.as_str())
            .to_string(),
        tool_name: payload
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        run_id: payload_run_id(payload),
        task_id: payload
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        call_id: payload
            .get("call_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        success: payload.get("success").and_then(|v| v.as_bool()),
        duration_ms: payload.get("duration_ms").and_then(|v| v.as_i64()),
        reasoning: payload
            .get("reasoning")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        tool_args: decode_json_payload(payload.get("tool_args")),
        output: decode_json_payload(payload.get("output")),
        error: payload
            .get("error")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
    })
}

fn parse_writer_enqueue_event(event: &LogsEvent) -> Option<WriterEnqueueEvent> {
    if event.event_type != "conductor.writer.enqueue"
        && event.event_type != "conductor.writer.enqueue.failed"
    {
        return None;
    }

    let payload = &event.payload;
    let data = payload.get("data").unwrap_or(payload);
    let run_id = payload_run_id(payload)?;

    Some(WriterEnqueueEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        call_id: data
            .get("call_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
    })
}

fn payload_run_id(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("run_id")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            payload
                .get("data")
                .and_then(|d| d.get("run_id"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
}

fn decode_json_payload(value: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    match value? {
        serde_json::Value::String(raw) => serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .or_else(|| Some(serde_json::Value::String(raw.clone()))),
        other => Some(other.clone()),
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn group_traces(events: &[TraceEvent]) -> Vec<TraceGroup> {
    let mut groups: HashMap<String, TraceGroup> = HashMap::new();

    for event in events {
        let trace_id = event.trace_id.clone();
        let entry = groups.entry(trace_id.clone()).or_insert(TraceGroup {
            trace_id,
            started: None,
            terminal: None,
        });

        match event.event_type.as_str() {
            "llm.call.started" => entry.started = Some(event.clone()),
            "llm.call.completed" | "llm.call.failed" => entry.terminal = Some(event.clone()),
            _ => {}
        }
    }

    let mut result: Vec<TraceGroup> = groups.into_values().collect();
    result.sort_by(|a, b| b.seq().cmp(&a.seq()));
    result
}

fn build_run_graph_summaries(
    traces: &[TraceGroup],
    prompts: &[PromptEvent],
    tools: &[ToolTraceEvent],
    writer_enqueues: &[WriterEnqueueEvent],
) -> Vec<RunGraphSummary> {
    #[derive(Default)]
    struct RunAccumulator {
        objective: String,
        timestamp: String,
        llm_calls: usize,
        tool_calls: usize,
        tool_failures: usize,
        writer_enqueues: usize,
        writer_enqueue_failures: usize,
        actor_keys: BTreeSet<String>,
        loop_ids: BTreeSet<String>,
    }

    let mut by_run: HashMap<String, RunAccumulator> = HashMap::new();

    for prompt in prompts {
        let entry = by_run.entry(prompt.run_id.clone()).or_default();
        entry.objective = prompt.objective.clone();
        if prompt.timestamp > entry.timestamp {
            entry.timestamp = prompt.timestamp.clone();
        }
    }

    for trace in traces {
        let Some(run_id) = trace.run_id() else {
            continue;
        };
        let entry = by_run.entry(run_id.to_string()).or_default();
        entry.llm_calls += 1;
        entry.actor_keys.insert(trace.actor_key());
        if let Some(task_id) = trace.task_id() {
            entry.loop_ids.insert(task_id.to_string());
        } else if let Some(call_id) = trace.call_id() {
            entry.loop_ids.insert(format!("call:{call_id}"));
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
        let entry = by_run.entry(run_id.clone()).or_default();
        if tool.event_type == "worker.tool.call" {
            entry.tool_calls += 1;
        }
        if tool.event_type == "worker.tool.result" && tool.success == Some(false) {
            entry.tool_failures += 1;
        }
        entry
            .actor_keys
            .insert(normalize_actor_key(&tool.role, &tool.actor_id));
        if let Some(task_id) = &tool.task_id {
            entry.loop_ids.insert(task_id.clone());
        } else if let Some(call_id) = &tool.call_id {
            entry.loop_ids.insert(format!("call:{call_id}"));
        }
        if tool.timestamp > entry.timestamp {
            entry.timestamp = tool.timestamp.clone();
        }
    }

    for enqueue in writer_enqueues {
        let entry = by_run.entry(enqueue.run_id.clone()).or_default();
        entry.writer_enqueues += 1;
        if enqueue.event_type == "conductor.writer.enqueue.failed" {
            entry.writer_enqueue_failures += 1;
        }
        entry.actor_keys.insert("writer".to_string());
        if let Some(call_id) = enqueue.call_id.as_ref() {
            entry.loop_ids.insert(format!("call:{call_id}"));
        }
        if enqueue.timestamp > entry.timestamp {
            entry.timestamp = enqueue.timestamp.clone();
        }
    }

    let mut result: Vec<RunGraphSummary> = by_run
        .into_iter()
        .map(|(run_id, acc)| RunGraphSummary {
            run_id,
            objective: if acc.objective.is_empty() {
                "Objective unavailable".to_string()
            } else {
                acc.objective
            },
            timestamp: acc.timestamp,
            llm_calls: acc.llm_calls,
            tool_calls: acc.tool_calls,
            tool_failures: acc.tool_failures,
            writer_enqueues: acc.writer_enqueues,
            writer_enqueue_failures: acc.writer_enqueue_failures,
            actor_count: acc.actor_keys.len(),
            loop_count: acc.loop_ids.len(),
        })
        .collect();
    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    result
}

fn build_graph_nodes_for_run(
    run_id: &str,
    traces: &[TraceGroup],
    tools: &[ToolTraceEvent],
    writer_enqueues: &[WriterEnqueueEvent],
) -> Vec<GraphNode> {
    #[derive(Default)]
    struct NodeAccumulator {
        llm_calls: usize,
        tool_calls: usize,
        tool_failures: usize,
        inbound_events: usize,
        inbound_failures: usize,
        loop_ids: BTreeSet<String>,
        has_failed: bool,
        has_started_only: bool,
    }

    let mut actors: HashMap<String, NodeAccumulator> = HashMap::new();

    for trace in traces {
        if trace.run_id() != Some(run_id) {
            continue;
        }
        let actor_key = trace.actor_key();
        let entry = actors.entry(actor_key).or_default();
        entry.llm_calls += 1;
        if let Some(task_id) = trace.task_id() {
            entry.loop_ids.insert(task_id.to_string());
        } else if let Some(call_id) = trace.call_id() {
            entry.loop_ids.insert(format!("call:{call_id}"));
        }
        match trace.status() {
            "failed" => entry.has_failed = true,
            "started" => entry.has_started_only = true,
            _ => {}
        }
    }

    for tool in tools {
        if tool.run_id.as_deref() != Some(run_id) {
            continue;
        }
        let actor_key = normalize_actor_key(&tool.role, &tool.actor_id);
        let entry = actors.entry(actor_key).or_default();
        if tool.event_type == "worker.tool.call" {
            entry.tool_calls += 1;
        }
        if tool.event_type == "worker.tool.result" && tool.success == Some(false) {
            entry.tool_failures += 1;
        }
        if let Some(task_id) = &tool.task_id {
            entry.loop_ids.insert(task_id.clone());
        } else if let Some(call_id) = &tool.call_id {
            entry.loop_ids.insert(format!("call:{call_id}"));
        }
    }

    for enqueue in writer_enqueues {
        if enqueue.run_id != run_id {
            continue;
        }
        let entry = actors.entry("writer".to_string()).or_default();
        entry.inbound_events += 1;
        if enqueue.event_type == "conductor.writer.enqueue.failed" {
            entry.inbound_failures += 1;
        }
        if let Some(call_id) = enqueue.call_id.as_ref() {
            entry.loop_ids.insert(format!("call:{call_id}"));
        }
    }

    let mut actor_keys: Vec<String> = actors.keys().cloned().collect();
    actor_keys.sort_by(|a, b| {
        let rank_a = actor_rank(a);
        let rank_b = actor_rank(b);
        rank_a.cmp(&rank_b).then_with(|| a.cmp(b))
    });

    let mut nodes = vec![GraphNode {
        key: "prompt:user".to_string(),
        label: "User Prompt".to_string(),
        kind: GraphNodeKind::Prompt,
        actor_key: None,
        llm_calls: 0,
        tool_calls: 0,
        inbound_events: 0,
        status: "completed".to_string(),
    }];

    let mut any_tool_calls = 0usize;
    let mut any_tool_failures = 0usize;

    for actor_key in actor_keys {
        if let Some(acc) = actors.get(&actor_key) {
            any_tool_calls += acc.tool_calls;
            any_tool_failures += acc.tool_failures;
            let status = if acc.has_failed || acc.inbound_failures > 0 {
                "failed"
            } else if acc.has_started_only {
                "started"
            } else {
                "completed"
            };
            nodes.push(GraphNode {
                key: format!("actor:{actor_key}"),
                label: display_actor_label(&actor_key),
                kind: GraphNodeKind::Actor,
                actor_key: Some(actor_key),
                llm_calls: acc.llm_calls,
                tool_calls: acc.tool_calls,
                inbound_events: acc.inbound_events,
                status: status.to_string(),
            });
        }
    }

    if any_tool_calls > 0 {
        nodes.push(GraphNode {
            key: "tools:all".to_string(),
            label: "Tools".to_string(),
            kind: GraphNodeKind::Tools,
            actor_key: None,
            llm_calls: 0,
            tool_calls: any_tool_calls,
            inbound_events: 0,
            status: if any_tool_failures > 0 {
                "degraded".to_string()
            } else {
                "completed".to_string()
            },
        });
    }

    nodes
}

fn build_graph_edges(nodes: &[GraphNode]) -> Vec<(String, String)> {
    let prompt_key = nodes
        .iter()
        .find(|node| node.kind == GraphNodeKind::Prompt)
        .map(|node| node.key.clone());
    let conductor_key = nodes
        .iter()
        .find(|node| node.actor_key.as_deref() == Some("conductor"))
        .map(|node| node.key.clone());
    let tools_key = nodes
        .iter()
        .find(|node| node.kind == GraphNodeKind::Tools)
        .map(|node| node.key.clone());

    let actor_nodes: Vec<&GraphNode> = nodes
        .iter()
        .filter(|node| node.kind == GraphNodeKind::Actor)
        .collect();
    let mut edges = BTreeSet::new();

    if let Some(prompt_key) = prompt_key {
        if let Some(conductor_key) = conductor_key.clone() {
            edges.insert((prompt_key.clone(), conductor_key.clone()));
            for actor in &actor_nodes {
                if actor.key != conductor_key {
                    edges.insert((conductor_key.clone(), actor.key.clone()));
                }
            }
        } else {
            for actor in &actor_nodes {
                edges.insert((prompt_key.clone(), actor.key.clone()));
            }
        }
    }

    if let Some(tools_key) = tools_key {
        for actor in &actor_nodes {
            if actor.tool_calls > 0 {
                edges.insert((actor.key.clone(), tools_key.clone()));
            }
        }
    }

    edges.into_iter().collect()
}

fn build_graph_layout(nodes: &[GraphNode]) -> GraphLayout {
    let mut prompt_col = Vec::new();
    let mut orchestrator_col = Vec::new();
    let mut actor_col = Vec::new();
    let mut tools_col = Vec::new();

    for node in nodes {
        match node.kind {
            GraphNodeKind::Prompt => prompt_col.push(node.key.clone()),
            GraphNodeKind::Tools => tools_col.push(node.key.clone()),
            GraphNodeKind::Actor => {
                if node.actor_key.as_deref() == Some("conductor") {
                    orchestrator_col.push(node.key.clone());
                } else {
                    actor_col.push(node.key.clone());
                }
            }
        }
    }

    if orchestrator_col.is_empty() && !actor_col.is_empty() {
        let first = actor_col.remove(0);
        orchestrator_col.push(first);
    }

    let columns_all = [prompt_col, orchestrator_col, actor_col, tools_col];
    let mut columns: Vec<Vec<String>> = columns_all
        .into_iter()
        .filter(|column| !column.is_empty())
        .collect();
    if columns.is_empty() {
        columns.push(vec![]);
    }

    let node_width = 188.0;
    let node_height = 66.0;
    let column_gap = 92.0;
    let row_gap = 20.0;
    let padding = 22.0;

    let max_rows = columns
        .iter()
        .map(std::vec::Vec::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let height = padding * 2.0
        + (max_rows as f32 * node_height)
        + ((max_rows.saturating_sub(1)) as f32 * row_gap);
    let width = padding * 2.0
        + (columns.len() as f32 * node_width)
        + ((columns.len().saturating_sub(1)) as f32 * column_gap);

    let mut positions = HashMap::new();
    for (col_idx, column) in columns.iter().enumerate() {
        let x = padding + col_idx as f32 * (node_width + column_gap);
        let col_height = (column.len() as f32 * node_height)
            + ((column.len().saturating_sub(1)) as f32 * row_gap);
        let start_y = (height - col_height) / 2.0;

        for (row_idx, key) in column.iter().enumerate() {
            let y = start_y + row_idx as f32 * (node_height + row_gap);
            positions.insert(key.clone(), (x, y));
        }
    }

    GraphLayout {
        width,
        height,
        positions,
    }
}

fn graph_node_color(node: &GraphNode) -> (&'static str, &'static str, &'static str) {
    match node.kind {
        GraphNodeKind::Prompt => ("#0f172a", "#475569", "#93c5fd"),
        GraphNodeKind::Tools => ("#111827", "#06b6d4", "#67e8f9"),
        GraphNodeKind::Actor => match node.actor_key.as_deref().unwrap_or_default() {
            "conductor" => ("#111827", "#3b82f6", "#60a5fa"),
            "researcher" => ("#0b1225", "#22c55e", "#86efac"),
            "terminal" => ("#0b1225", "#f59e0b", "#fcd34d"),
            "writer" => ("#0b1225", "#c084fc", "#ddd6fe"),
            _ => ("#0b1225", "#64748b", "#cbd5e1"),
        },
    }
}

fn graph_status_color(status: &str) -> &'static str {
    match status {
        "completed" => "#22c55e",
        "failed" => "#ef4444",
        "started" => "#f59e0b",
        "degraded" => "#f97316",
        _ => "#94a3b8",
    }
}

fn actor_rank(actor_key: &str) -> usize {
    match actor_key {
        "conductor" => 0,
        "writer" => 1,
        "researcher" => 2,
        "terminal" => 3,
        _ => 9,
    }
}

fn sanitize_actor_key(raw: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in raw.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            ch.to_ascii_lowercase()
        } else if previous_dash {
            continue;
        } else {
            previous_dash = true;
            '-'
        };
        out.push(mapped);
    }
    out.trim_matches('-').to_string()
}

fn normalize_actor_key(role: &str, actor_id: &str) -> String {
    let role_clean = sanitize_actor_key(role);
    if !role_clean.is_empty() && role_clean != "unknown" {
        return role_clean;
    }

    let actor_lower = actor_id.to_ascii_lowercase();
    for known in ["conductor", "writer", "researcher", "terminal"] {
        if actor_lower.contains(known) {
            return known.to_string();
        }
    }

    if let Some((prefix, _)) = actor_id.split_once(':') {
        let cleaned = sanitize_actor_key(prefix);
        if !cleaned.is_empty() {
            return cleaned;
        }
    }

    let cleaned = sanitize_actor_key(actor_id);
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn display_actor_label(actor_key: &str) -> String {
    match actor_key {
        "conductor" => "Conductor".to_string(),
        "writer" => "Writer".to_string(),
        "researcher" => "Researcher".to_string(),
        "terminal" => "Terminal".to_string(),
        other => other
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        let mut out = String::new();
                        out.push(first.to_ascii_uppercase());
                        out.push_str(chars.as_str());
                        out
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" "),
    }
}

fn pair_tool_events(mut tool_events: Vec<ToolTraceEvent>) -> Vec<ToolTracePair> {
    tool_events.sort_by_key(|event| event.seq);
    let mut by_trace: BTreeMap<String, ToolTracePair> = BTreeMap::new();

    for event in tool_events {
        let trace_key = if event.tool_trace_id.is_empty() {
            format!("{}:{}", event.event_type, event.event_id)
        } else {
            event.tool_trace_id.clone()
        };
        let entry = by_trace
            .entry(trace_key.clone())
            .or_insert_with(|| ToolTracePair {
                tool_trace_id: trace_key.clone(),
                call: None,
                result: None,
            });
        if event.event_type == "worker.tool.call" {
            entry.call = Some(event);
        } else {
            entry.result = Some(event);
        }
    }

    let mut pairs: Vec<ToolTracePair> = by_trace.into_values().collect();
    pairs.sort_by_key(|pair| pair.seq());
    pairs
}

fn merge_loop_sequence(
    traces: &[TraceGroup],
    tool_pairs: &[ToolTracePair],
) -> Vec<LoopSequenceItem> {
    let mut sequence: Vec<LoopSequenceItem> =
        traces.iter().cloned().map(LoopSequenceItem::Llm).collect();
    sequence.extend(tool_pairs.iter().cloned().map(LoopSequenceItem::Tool));
    sequence.sort_by_key(|item| match item {
        LoopSequenceItem::Llm(trace) => trace.seq(),
        LoopSequenceItem::Tool(pair) => pair.seq(),
    });
    sequence
}

fn build_loop_groups_for_actor(
    actor_key: &str,
    traces: &[TraceGroup],
    tools: &[ToolTraceEvent],
) -> Vec<TraceLoopGroup> {
    let mut by_loop: BTreeMap<String, Vec<TraceGroup>> = BTreeMap::new();
    let mut tool_by_loop: BTreeMap<String, Vec<ToolTraceEvent>> = BTreeMap::new();

    for trace in traces {
        if trace.actor_key() != actor_key {
            continue;
        }
        let loop_id = trace
            .task_id()
            .map(ToString::to_string)
            .or_else(|| trace.call_id().map(|call_id| format!("call:{call_id}")))
            .unwrap_or_else(|| "direct".to_string());
        by_loop.entry(loop_id).or_default().push(trace.clone());
    }

    for tool in tools {
        if tool.actor_key() != actor_key {
            continue;
        }
        tool_by_loop
            .entry(tool.loop_id())
            .or_default()
            .push(tool.clone());
    }

    let mut loop_ids: BTreeSet<String> = BTreeSet::new();
    loop_ids.extend(by_loop.keys().cloned());
    loop_ids.extend(tool_by_loop.keys().cloned());

    let mut groups: Vec<TraceLoopGroup> = by_loop
        .into_iter()
        .map(|(loop_id, traces)| (loop_id, traces))
        .collect::<BTreeMap<String, Vec<TraceGroup>>>()
        .into_iter()
        .map(|(loop_id, mut traces)| {
            traces.sort_by_key(|trace| trace.seq());
            let tool_pairs = pair_tool_events(tool_by_loop.remove(&loop_id).unwrap_or_default());
            let sequence = merge_loop_sequence(&traces, &tool_pairs);
            TraceLoopGroup {
                loop_id,
                traces,
                sequence,
            }
        })
        .collect();

    for loop_id in loop_ids {
        if groups.iter().any(|group| group.loop_id == loop_id) {
            continue;
        }
        let traces = Vec::new();
        let tool_pairs = pair_tool_events(tool_by_loop.remove(&loop_id).unwrap_or_default());
        let sequence = merge_loop_sequence(&traces, &tool_pairs);
        groups.push(TraceLoopGroup {
            loop_id,
            traces,
            sequence,
        });
    }

    groups.sort_by(|a, b| {
        let a_seq = a
            .sequence
            .last()
            .map(|item| match item {
                LoopSequenceItem::Llm(trace) => trace.seq(),
                LoopSequenceItem::Tool(pair) => pair.seq(),
            })
            .unwrap_or(0);
        let b_seq = b
            .sequence
            .last()
            .map(|item| match item {
                LoopSequenceItem::Llm(trace) => trace.seq(),
                LoopSequenceItem::Tool(pair) => pair.seq(),
            })
            .unwrap_or(0);
        b_seq.cmp(&a_seq)
    });
    groups
}

fn format_loop_title(loop_id: &str) -> String {
    if loop_id == "direct" {
        "Direct LLM calls".to_string()
    } else if loop_id.starts_with("call:") {
        format!("Capability {}", loop_id.trim_start_matches("call:"))
    } else {
        format!("Agent loop {loop_id}")
    }
}

#[component]
pub fn TraceView(desktop_id: String, window_id: String) -> Element {
    let mut trace_events = use_signal(Vec::<TraceEvent>::new);
    let mut prompt_events = use_signal(Vec::<PromptEvent>::new);
    let mut tool_events = use_signal(Vec::<ToolTraceEvent>::new);
    let mut writer_enqueue_events = use_signal(Vec::<WriterEnqueueEvent>::new);
    let mut since_seq = use_signal(|| 0_i64);
    let mut selected_run_id = use_signal(|| None::<String>);
    let mut selected_actor_key = use_signal(|| None::<String>);
    let mut run_sidebar_open = use_signal(|| false);
    let mut node_sheet_open = use_signal(|| false);
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
                }

                parsed_traces.sort_by_key(|event| event.seq);
                parsed_traces.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_prompts.sort_by_key(|event| event.seq);
                parsed_prompts.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_tools.sort_by_key(|event| event.seq);
                parsed_tools.dedup_by(|a, b| a.event_id == b.event_id);
                parsed_writer_enqueues.sort_by_key(|event| event.seq);
                parsed_writer_enqueues.dedup_by(|a, b| a.event_id == b.event_id);

                trace_events.set(parsed_traces);
                prompt_events.set(parsed_prompts);
                tool_events.set(parsed_tools);
                writer_enqueue_events.set(parsed_writer_enqueues);
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
                                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
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

                                        if let Some(trace_event) = parse_trace_event(&logs_event) {
                                            let mut list = trace_events.write();
                                            list.push(trace_event);
                                            list.sort_by_key(|event| event.seq);
                                            list.dedup_by(|a, b| a.event_id == b.event_id);
                                            if list.len() > 1_000 {
                                                let trim = list.len() - 1_000;
                                                list.drain(0..trim);
                                            }
                                        }

                                        if let Some(prompt_event) = parse_prompt_event(&logs_event)
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
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                queue_open.borrow_mut().push_back(TraceWsEvent::Connected);
            }) as Box<dyn FnMut(Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let queue_message = ws_event_queue.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                    return;
                };
                let text_string = text.as_string().unwrap_or_default();
                queue_message
                    .borrow_mut()
                    .push_back(TraceWsEvent::Message(text_string));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let queue_error = ws_event_queue.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                queue_error
                    .borrow_mut()
                    .push_back(TraceWsEvent::Error(e.message()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let queue_close = ws_event_queue.clone();
            let closing_for_close = closing.clone();
            let on_close = Closure::wrap(Box::new(move |_e: CloseEvent| {
                if closing_for_close.get() {
                    return;
                }
                queue_close.borrow_mut().push_back(TraceWsEvent::Closed);
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
    let trace_snapshot = trace_events.read().clone();
    let prompt_snapshot = prompt_events.read().clone();
    let tool_snapshot = tool_events.read().clone();
    let writer_enqueue_snapshot = writer_enqueue_events.read().clone();

    let traces_all = group_traces(&trace_snapshot);
    let run_summaries = build_run_graph_summaries(
        &traces_all,
        &prompt_snapshot,
        &tool_snapshot,
        &writer_enqueue_snapshot,
    );

    let active_run_id = selected_run_id()
        .filter(|run_id| run_summaries.iter().any(|run| run.run_id == *run_id))
        .or_else(|| run_summaries.first().map(|run| run.run_id.clone()));

    let traces_for_run: Vec<TraceGroup> = if let Some(run_id) = active_run_id.as_deref() {
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

    let graph_nodes = if let Some(run_id) = active_run_id.as_deref() {
        build_graph_nodes_for_run(
            run_id,
            &traces_for_run,
            &tools_for_run,
            &writer_enqueue_snapshot,
        )
    } else {
        Vec::new()
    };
    let graph_edges = build_graph_edges(&graph_nodes);
    let graph_layout = build_graph_layout(&graph_nodes);

    let actor_nodes: Vec<GraphNode> = graph_nodes
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
        .map(|actor_key| build_loop_groups_for_actor(actor_key, &traces_for_run, &tools_for_run))
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
        .filter_map(|(from, to)| {
            let from_pos = graph_layout.positions.get(from)?;
            let to_pos = graph_layout.positions.get(to)?;
            Some(GraphEdgeSegment {
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

    rsx! {
        style { {CHAT_STYLES} }
        style { {TRACE_VIEW_STYLES} }
        div {
            class: "chat-container",
            style: "padding: 0; overflow: hidden;",
            div {
                class: "chat-header",
                h3 { "LLM Traces" }
                div {
                    class: "trace-header-actions",
                    button {
                        class: "trace-run-toggle",
                        onclick: {
                            let next = !run_sidebar_open();
                            move |_| run_sidebar_open.set(next)
                        },
                        if run_sidebar_open() {
                            "Hide Runs"
                        } else {
                            "Runs"
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
                                        "thread-item active"
                                    } else {
                                        "thread-item"
                                    },
                                    onclick: {
                                        let run_id = run.run_id.clone();
                                        move |_| {
                                            selected_run_id.set(Some(run_id.clone()));
                                            selected_actor_key.set(None);
                                            node_sheet_open.set(false);
                                            run_sidebar_open.set(false);
                                        }
                                    },
                                    div {
                                        class: "thread-title",
                                        "{run.run_id}"
                                    }
                                    div {
                                        class: "thread-preview",
                                        "{run.llm_calls} llm | {run.tool_calls} tools | {run.actor_count} actors"
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
                                        span { class: "trace-pill", "{run.llm_calls} llm calls" }
                                        span { class: "trace-pill", "{run.tool_calls} tool calls" }
                                        span { class: "trace-pill", "{run.tool_failures} tool failures" }
                                        span { class: "trace-pill", "{run.writer_enqueues} writer enqueues" }
                                        if run.writer_enqueue_failures > 0 {
                                            span { class: "trace-pill", "{run.writer_enqueue_failures} enqueue failures" }
                                        }
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
                                                stroke: "#334155",
                                                stroke_width: "2"
                                            }
                                        }
                                        for render in &graph_render_nodes {
                                            {
                                                let (fill, stroke, label_color) = graph_node_color(&render.node);
                                                let selected = render
                                                    .node
                                                    .actor_key
                                                    .as_ref()
                                                    .zip(active_actor_key.as_ref())
                                                    .map(|(node_actor, active_actor)| node_actor == active_actor)
                                                    .unwrap_or(false);
                                                let stroke_color = if selected {
                                                    "#60a5fa"
                                                } else {
                                                    stroke
                                                };
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
                                                    format!("{} llm / {} tools", render.node.llm_calls, render.node.tool_calls)
                                                };
                                                let status_color = graph_status_color(&render.node.status);
                                                let actor_key_for_click = render.node.actor_key.clone();
                                                rsx! {
                                                    g {
                                                        onclick: move |_| {
                                                            if let Some(actor_key) = actor_key_for_click.clone() {
                                                                selected_actor_key.set(Some(actor_key));
                                                                node_sheet_open.set(true);
                                                            }
                                                        },
                                                        rect {
                                                            x: format!("{:.1}", render.x),
                                                            y: format!("{:.1}", render.y),
                                                            width: "188",
                                                            height: "66",
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
                                if !actor_nodes.is_empty() {
                                    div {
                                        class: "trace-node-chip-row",
                                        for node in &actor_nodes {
                                            if let Some(actor_key) = node.actor_key.clone() {
                                                button {
                                                    class: if active_actor_key.as_deref() == Some(actor_key.as_str()) {
                                                        "trace-node-chip active"
                                                    } else {
                                                        "trace-node-chip"
                                                    },
                                                    onclick: {
                                                        let actor_key = actor_key.clone();
                                                        move |_| {
                                                            selected_actor_key.set(Some(actor_key.clone()));
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
                                        div {
                                            class: "trace-loop-group",
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
                                                            class: "trace-call-card",
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
                                                                        style: "background:#1f2937;color:#d1d5db;padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "in:{input_tokens}"
                                                                    }
                                                                }
                                                                if let Some(output_tokens) = trace.output_tokens() {
                                                                    span {
                                                                        style: "background:#1f2937;color:#d1d5db;padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
                                                                        "out:{output_tokens}"
                                                                    }
                                                                }
                                                                if let Some(cached_tokens) = trace.cached_input_tokens() {
                                                                    span {
                                                                        style: "background:#1f2937;color:#d1d5db;padding:0.14rem 0.35rem;border-radius:3px;font-size:0.64rem;",
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
                                                            class: "trace-call-card",
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
