use chrono::{DateTime, Utc};
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

.trace-run-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-top: 0.35rem;
}

.trace-run-row-left {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
}

.trace-run-status {
    border-radius: 999px;
    font-size: 0.62rem;
    padding: 0.15rem 0.42rem;
    border: 1px solid #334155;
    background: #0f172a;
    color: #cbd5e1;
}

.trace-run-status--completed {
    border-color: #22c55e;
    color: #86efac;
    background: #052e16;
}

.trace-run-status--failed {
    border-color: #ef4444;
    color: #fca5a5;
    background: #450a0a;
}

.trace-run-status--in-progress {
    border-color: #eab308;
    color: #fde68a;
    background: #422006;
}

.trace-run-sparkline {
    width: 120px;
    height: 16px;
    min-width: 120px;
}

.trace-delegation-wrap {
    margin-top: 0.6rem;
}

.trace-delegation-timeline {
    display: flex;
    gap: 0.45rem;
    overflow-x: auto;
    padding-bottom: 0.2rem;
}

.trace-delegation-band {
    border: 1px solid #1f2a44;
    background: #0b1222;
    color: #dbeafe;
    border-radius: 8px;
    padding: 0.34rem 0.45rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 220px;
}

.trace-delegation-band--completed {
    border-color: #14532d;
    background: #052e16;
}

.trace-delegation-band--failed {
    border-color: #7f1d1d;
    background: #450a0a;
}

.trace-delegation-band--blocked {
    border-color: #854d0e;
    background: #422006;
}

.trace-delegation-band--inflight {
    border-color: #1e40af;
    background: #172554;
}

.trace-lifecycle-strip {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    margin-bottom: 0.45rem;
}

.trace-lifecycle-chip {
    border: 1px solid #334155;
    border-radius: 7px;
    background: #111827;
    color: #e2e8f0;
    font-size: 0.68rem;
    padding: 0.18rem 0.35rem;
}

.trace-lifecycle-chip summary {
    cursor: pointer;
    list-style: none;
}

.trace-lifecycle-chip summary::-webkit-details-marker {
    display: none;
}

.trace-lifecycle-chip--started {
    border-color: #64748b;
    background: #1e293b;
}

.trace-lifecycle-chip--progress {
    border-color: #2563eb;
    background: #172554;
}

.trace-lifecycle-chip--completed {
    border-color: #16a34a;
    background: #052e16;
}

.trace-lifecycle-chip--failed {
    border-color: #dc2626;
    background: #450a0a;
}

.trace-lifecycle-chip--finding {
    border-color: #d97706;
    background: #422006;
}

.trace-lifecycle-chip--learning {
    border-color: #0f766e;
    background: #042f2e;
}

.trace-traj-grid {
    border: 1px solid #1f2a44;
    border-radius: 8px;
    background: #0b1222;
    padding: 0.45rem;
    overflow: auto;
    margin-top: 0.6rem;
}

.trace-traj-grid-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-bottom: 0.35rem;
}

.trace-traj-cell--completed {
    fill: #22c55e;
}

.trace-traj-cell--failed {
    fill: #ef4444;
}

.trace-traj-cell--inflight {
    fill: #f59e0b;
}

.trace-traj-cell--blocked {
    fill: #f97316;
}

.trace-traj-slow-ring {
    fill: none;
    stroke: #ef4444;
    stroke-width: 1.25;
}

.trace-duration-bar {
    height: 3px;
    border-radius: 2px;
    background: #22c55e;
    margin-top: 4px;
    transition: width 0.2s;
}

.trace-duration-bar--slow {
    background: #ef4444;
}

.trace-token-bar {
    display: flex;
    width: 100%;
    height: 5px;
    border-radius: 999px;
    overflow: hidden;
    margin-top: 0.35rem;
}

.trace-token-segment--cached {
    background: #6366f1;
}

.trace-token-segment--input {
    background: #3b82f6;
}

.trace-token-segment--output {
    background: #22c55e;
}

.trace-worker-node {
    filter: drop-shadow(0 0 6px rgba(56, 189, 248, 0.28));
}

.trace-call-card--selected {
    border-color: #60a5fa;
    box-shadow: 0 0 0 1px #60a5fa;
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
const TRACE_SLOW_DURATION_MS: i64 = 5_000;
const TRACE_TRAJECTORY_MAX_COLUMNS: usize = 80;

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

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct ConductorDelegationEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    run_id: String,
    worker_type: Option<String>,
    worker_objective: Option<String>,
    success: Option<bool>,
    result_summary: Option<String>,
    call_id: Option<String>,
    capability: Option<String>,
    error: Option<String>,
    failure_kind: Option<String>,
    reason: Option<String>,
    lane: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct ConductorRunEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    run_id: String,
    phase: Option<String>,
    status: Option<String>,
    message: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct WorkerLifecycleEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    worker_id: String,
    task_id: String,
    phase: String,
    run_id: Option<String>,
    objective: Option<String>,
    model_used: Option<String>,
    message: Option<String>,
    summary: Option<String>,
    status: Option<String>,
    error: Option<String>,
    finding_id: Option<String>,
    claim: Option<String>,
    confidence: Option<f64>,
    learning_id: Option<String>,
    insight: Option<String>,
    call_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
struct TrajectoryCell {
    seq: i64,
    step_index: usize,
    row_key: String,
    event_type: String,
    tool_name: Option<String>,
    actor_key: Option<String>,
    status: TrajectoryStatus,
    duration_ms: Option<i64>,
    total_tokens: Option<i64>,
    loop_id: String,
    item_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TrajectoryStatus {
    Completed,
    Failed,
    Inflight,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrajectoryMode {
    Status,
    Duration,
    Tokens,
}

#[derive(Clone, Debug)]
struct DelegationTimelineBand {
    worker_type: String,
    worker_objective: Option<String>,
    status: String,
    duration_ms: Option<i64>,
    call_id: Option<String>,
    loop_id: Option<String>,
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
    worker_count: usize,
    worker_failures: usize,
    worker_calls: usize,
    capability_failures: usize,
    run_status: String,
    total_duration_ms: i64,
    total_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GraphNodeKind {
    Prompt,
    Actor,
    Worker,
    Tools,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct GraphNode {
    key: String,
    label: String,
    kind: GraphNodeKind,
    actor_key: Option<String>,
    worker_id: Option<String>,
    task_id: Option<String>,
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
struct GraphEdge {
    from: String,
    to: String,
    label: Option<String>,
    color: String,
    dashed: bool,
}

#[derive(Clone, Debug)]
struct GraphEdgeSegment {
    edge: GraphEdge,
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

fn parse_conductor_delegation_event(event: &LogsEvent) -> Option<ConductorDelegationEvent> {
    let is_delegation = matches!(
        event.event_type.as_str(),
        "conductor.worker.call"
            | "conductor.worker.result"
            | "conductor.capability.completed"
            | "conductor.capability.failed"
            | "conductor.capability.blocked"
    );
    if !is_delegation {
        return None;
    }
    let payload = &event.payload;
    let data = payload.get("data").unwrap_or(payload);
    let meta = payload.get("_meta");
    let run_id = payload_run_id(payload)?;

    Some(ConductorDelegationEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        worker_type: payload
            .get("worker_type")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or_else(|| {
                payload
                    .get("capability")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
            }),
        worker_objective: payload
            .get("worker_objective")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        success: payload.get("success").and_then(|v| v.as_bool()),
        result_summary: payload
            .get("result_summary")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        call_id: data
            .get("call_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        capability: payload
            .get("capability")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        error: data
            .get("error")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        failure_kind: data
            .get("failure_kind")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        reason: data
            .get("reason")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        lane: meta
            .and_then(|m| m.get("lane"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
    })
}

fn parse_conductor_run_event(event: &LogsEvent) -> Option<ConductorRunEvent> {
    let is_run = matches!(
        event.event_type.as_str(),
        "conductor.run.started"
            | "conductor.task.completed"
            | "conductor.task.failed"
            | "conductor.task.progress"
    );
    if !is_run {
        return None;
    }

    let payload = &event.payload;
    let run_id = payload_run_id(payload)?;
    Some(ConductorRunEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        phase: payload
            .get("phase")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        status: payload
            .get("status")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        error_code: payload
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        error_message: payload
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
    })
}

fn parse_worker_lifecycle_event(event: &LogsEvent) -> Option<WorkerLifecycleEvent> {
    let is_lifecycle = matches!(
        event.event_type.as_str(),
        "worker.task.started"
            | "worker.task.progress"
            | "worker.task.completed"
            | "worker.task.failed"
            | "worker.task.finding"
            | "worker.task.learning"
    );
    if !is_lifecycle {
        return None;
    }
    let payload = &event.payload;
    let task_id = payload.get("task_id").and_then(|v| v.as_str())?.to_string();
    let worker_id = payload
        .get("worker_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    Some(WorkerLifecycleEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        worker_id,
        task_id,
        phase: payload
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("agent_loop")
            .to_string(),
        run_id: payload_run_id(payload),
        objective: payload
            .get("objective")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        model_used: payload
            .get("model_used")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        summary: payload
            .get("summary")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        status: payload
            .get("status")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        error: payload
            .get("error")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        finding_id: payload
            .get("finding_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        claim: payload
            .get("claim")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        confidence: payload.get("confidence").and_then(|v| v.as_f64()),
        learning_id: payload
            .get("learning_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        insight: payload
            .get("insight")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        call_id: payload
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
    delegations: &[ConductorDelegationEvent],
    run_events: &[ConductorRunEvent],
    worker_lifecycle: &[WorkerLifecycleEvent],
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
        worker_ids: BTreeSet<String>,
        failed_tasks: BTreeSet<String>,
        worker_calls: usize,
        capability_failures: usize,
        run_status: String,
        run_terminal_seq: i64,
        total_duration_ms: i64,
        total_tokens: i64,
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
        if let Some(duration) = trace.duration_ms() {
            entry.total_duration_ms = entry.total_duration_ms.saturating_add(duration.max(0));
        }
        if let Some(tokens) = trace.total_tokens() {
            entry.total_tokens = entry.total_tokens.saturating_add(tokens.max(0));
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
        if let Some(duration) = tool.duration_ms {
            entry.total_duration_ms = entry.total_duration_ms.saturating_add(duration.max(0));
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

    for delegation in delegations {
        let entry = by_run.entry(delegation.run_id.clone()).or_default();
        if delegation.event_type == "conductor.worker.call" {
            entry.worker_calls += 1;
        }
        if delegation.event_type == "conductor.capability.failed" {
            entry.capability_failures += 1;
        }
        if delegation.timestamp > entry.timestamp {
            entry.timestamp = delegation.timestamp.clone();
        }
    }

    for lifecycle in worker_lifecycle {
        let Some(run_id) = lifecycle.run_id.as_ref() else {
            continue;
        };
        let entry = by_run.entry(run_id.clone()).or_default();
        entry.worker_ids.insert(lifecycle.worker_id.clone());
        entry.loop_ids.insert(lifecycle.task_id.clone());
        if lifecycle.event_type == "worker.task.failed" {
            entry.failed_tasks.insert(lifecycle.task_id.clone());
        }
        if lifecycle.timestamp > entry.timestamp {
            entry.timestamp = lifecycle.timestamp.clone();
        }
    }

    for run_event in run_events {
        let entry = by_run.entry(run_event.run_id.clone()).or_default();
        if run_event.timestamp > entry.timestamp {
            entry.timestamp = run_event.timestamp.clone();
        }
        match run_event.event_type.as_str() {
            "conductor.task.completed" => {
                if run_event.seq >= entry.run_terminal_seq {
                    entry.run_status = "completed".to_string();
                    entry.run_terminal_seq = run_event.seq;
                }
            }
            "conductor.task.failed" => {
                if run_event.seq >= entry.run_terminal_seq {
                    entry.run_status = "failed".to_string();
                    entry.run_terminal_seq = run_event.seq;
                }
            }
            _ => {
                if entry.run_status.is_empty() {
                    entry.run_status = "in-progress".to_string();
                }
            }
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
            worker_count: acc.worker_ids.len(),
            worker_failures: acc.failed_tasks.len(),
            worker_calls: acc.worker_calls,
            capability_failures: acc.capability_failures,
            run_status: if acc.run_status.is_empty() {
                "in-progress".to_string()
            } else {
                acc.run_status
            },
            total_duration_ms: acc.total_duration_ms,
            total_tokens: acc.total_tokens,
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
    worker_lifecycle: &[WorkerLifecycleEvent],
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

    #[derive(Default)]
    struct WorkerAccumulator {
        task_ids: BTreeSet<String>,
        has_completed: bool,
        has_failed: bool,
        has_inflight: bool,
    }

    let mut actors: HashMap<String, NodeAccumulator> = HashMap::new();
    let mut workers: HashMap<String, WorkerAccumulator> = HashMap::new();

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

    for lifecycle in worker_lifecycle
        .iter()
        .filter(|event| event.run_id.as_deref() == Some(run_id))
    {
        let entry = workers.entry(lifecycle.worker_id.clone()).or_default();
        entry.task_ids.insert(lifecycle.task_id.clone());
        match lifecycle.event_type.as_str() {
            "worker.task.completed" => entry.has_completed = true,
            "worker.task.failed" => entry.has_failed = true,
            _ => entry.has_inflight = true,
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
        worker_id: None,
        task_id: None,
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
                worker_id: None,
                task_id: None,
                llm_calls: acc.llm_calls,
                tool_calls: acc.tool_calls,
                inbound_events: acc.inbound_events,
                status: status.to_string(),
            });
        }
    }

    let mut worker_ids: Vec<String> = workers.keys().cloned().collect();
    worker_ids.sort();
    for worker_id in worker_ids {
        if let Some(acc) = workers.get(&worker_id) {
            let status = if acc.has_failed {
                "failed"
            } else if acc.has_completed {
                "completed"
            } else if acc.has_inflight {
                "started"
            } else {
                "unknown"
            };
            let task_id = acc.task_ids.iter().next().cloned();
            nodes.push(GraphNode {
                key: format!("worker:{worker_id}"),
                label: format!("Worker {}", display_worker_label(&worker_id)),
                kind: GraphNodeKind::Worker,
                actor_key: None,
                worker_id: Some(worker_id),
                task_id,
                llm_calls: 0,
                tool_calls: 0,
                inbound_events: acc.task_ids.len(),
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
            worker_id: None,
            task_id: None,
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

fn build_graph_edges(
    nodes: &[GraphNode],
    run_id: &str,
    delegations: &[ConductorDelegationEvent],
) -> Vec<GraphEdge> {
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
    let worker_nodes: Vec<&GraphNode> = nodes
        .iter()
        .filter(|node| node.kind == GraphNodeKind::Worker)
        .collect();
    let mut edges = Vec::<GraphEdge>::new();

    if let Some(prompt_key) = prompt_key {
        if let Some(conductor_key) = conductor_key.clone() {
            edges.push(GraphEdge {
                from: prompt_key.clone(),
                to: conductor_key.clone(),
                label: None,
                color: "#334155".to_string(),
                dashed: false,
            });
            for actor in &actor_nodes {
                if actor.key != conductor_key {
                    edges.push(GraphEdge {
                        from: conductor_key.clone(),
                        to: actor.key.clone(),
                        label: None,
                        color: "#334155".to_string(),
                        dashed: false,
                    });
                }
            }
        } else {
            for actor in &actor_nodes {
                edges.push(GraphEdge {
                    from: prompt_key.clone(),
                    to: actor.key.clone(),
                    label: None,
                    color: "#334155".to_string(),
                    dashed: false,
                });
            }
        }
    }

    if let Some(tools_key) = tools_key {
        for actor in &actor_nodes {
            if actor.tool_calls > 0 {
                edges.push(GraphEdge {
                    from: actor.key.clone(),
                    to: tools_key.clone(),
                    label: None,
                    color: "#334155".to_string(),
                    dashed: false,
                });
            }
        }
    }

    if let Some(conductor_key) = conductor_key {
        let mut status_by_worker_type: HashMap<String, String> = HashMap::new();
        for event in delegations.iter().filter(|event| event.run_id == run_id) {
            let worker_type = event
                .worker_type
                .clone()
                .or_else(|| event.capability.clone())
                .unwrap_or_else(|| "worker".to_string());
            let candidate_status = match event.event_type.as_str() {
                "conductor.capability.completed" => "completed",
                "conductor.capability.failed" => "failed",
                "conductor.capability.blocked" => "blocked",
                "conductor.worker.call" => "inflight",
                _ => continue,
            };
            status_by_worker_type
                .entry(worker_type)
                .and_modify(|current| {
                    if delegation_status_rank(candidate_status) > delegation_status_rank(current) {
                        *current = candidate_status.to_string();
                    }
                })
                .or_insert_with(|| candidate_status.to_string());
        }

        for worker in worker_nodes {
            let worker_id_lower = worker
                .worker_id
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let match_entry = status_by_worker_type.iter().find(|(worker_type, _)| {
                worker_id_lower.contains(&worker_type.to_ascii_lowercase())
            });
            let (edge_label, status) = match_entry
                .map(|(worker_type, status)| (worker_type.clone(), status.clone()))
                .unwrap_or_else(|| {
                    (
                        worker
                            .worker_id
                            .as_ref()
                            .cloned()
                            .unwrap_or_else(|| "worker".to_string()),
                        "inflight".to_string(),
                    )
                });
            let (color, dashed) = match status.as_str() {
                "completed" => ("#22c55e", false),
                "failed" => ("#ef4444", false),
                "blocked" => ("#f59e0b", true),
                _ => ("#64748b", false),
            };
            edges.push(GraphEdge {
                from: conductor_key.clone(),
                to: worker.key.clone(),
                label: Some(edge_label),
                color: color.to_string(),
                dashed,
            });
        }
    }

    let mut uniq = BTreeSet::new();
    edges
        .into_iter()
        .filter(|edge| {
            let key = format!(
                "{}>{}|{}|{}|{}",
                edge.from,
                edge.to,
                edge.label.clone().unwrap_or_default(),
                edge.color,
                edge.dashed
            );
            uniq.insert(key)
        })
        .collect()
}

fn build_graph_layout(nodes: &[GraphNode]) -> GraphLayout {
    let mut prompt_col = Vec::new();
    let mut orchestrator_col = Vec::new();
    let mut actor_col = Vec::new();
    let mut worker_col = Vec::new();
    let mut tools_col = Vec::new();

    for node in nodes {
        match node.kind {
            GraphNodeKind::Prompt => prompt_col.push(node.key.clone()),
            GraphNodeKind::Tools => tools_col.push(node.key.clone()),
            GraphNodeKind::Worker => worker_col.push(node.key.clone()),
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

    let columns_all = [
        prompt_col,
        orchestrator_col,
        actor_col,
        worker_col,
        tools_col,
    ];
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
        GraphNodeKind::Worker => ("#082f49", "#38bdf8", "#bae6fd"),
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

fn display_worker_label(worker_id: &str) -> String {
    worker_id
        .split(':')
        .next_back()
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| worker_id.to_string())
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

impl TrajectoryMode {
    fn label(self) -> &'static str {
        match self {
            TrajectoryMode::Status => "Status",
            TrajectoryMode::Duration => "Duration",
            TrajectoryMode::Tokens => "Tokens",
        }
    }
}

fn format_duration_short(ms: i64) -> String {
    if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

fn format_tokens_short(tokens: i64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn parse_rfc3339_utc(timestamp: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn duration_between_ms(start_ts: &str, end_ts: &str) -> Option<i64> {
    let start = parse_rfc3339_utc(start_ts)?;
    let end = parse_rfc3339_utc(end_ts)?;
    Some((end - start).num_milliseconds().max(0))
}

fn run_status_class(status: &str) -> &'static str {
    match status {
        "completed" => "trace-run-status trace-run-status--completed",
        "failed" => "trace-run-status trace-run-status--failed",
        _ => "trace-run-status trace-run-status--in-progress",
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

fn worker_summary(
    task_id: &str,
    events: &[WorkerLifecycleEvent],
) -> (&'static str, Option<String>) {
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

fn delegation_status_rank(status: &str) -> usize {
    match status {
        "failed" => 4,
        "blocked" => 3,
        "inflight" => 2,
        "completed" => 1,
        _ => 0,
    }
}

fn trajectory_status_rank(status: &TrajectoryStatus) -> usize {
    match status {
        TrajectoryStatus::Failed => 4,
        TrajectoryStatus::Blocked => 3,
        TrajectoryStatus::Inflight => 2,
        TrajectoryStatus::Completed => 1,
    }
}

fn trajectory_status_class(status: &TrajectoryStatus) -> &'static str {
    match status {
        TrajectoryStatus::Completed => "trace-traj-cell--completed",
        TrajectoryStatus::Failed => "trace-traj-cell--failed",
        TrajectoryStatus::Inflight => "trace-traj-cell--inflight",
        TrajectoryStatus::Blocked => "trace-traj-cell--blocked",
    }
}

fn loop_id_for_call(call_id: Option<&str>, lifecycle: &[WorkerLifecycleEvent]) -> Option<String> {
    let call_id = call_id?;
    lifecycle
        .iter()
        .find(|event| event.call_id.as_deref() == Some(call_id))
        .map(|event| event.task_id.clone())
}

fn build_delegation_timeline_bands(
    run_id: &str,
    delegations: &[ConductorDelegationEvent],
    lifecycle: &[WorkerLifecycleEvent],
) -> Vec<DelegationTimelineBand> {
    let mut calls: Vec<&ConductorDelegationEvent> = delegations
        .iter()
        .filter(|event| event.run_id == run_id && event.event_type == "conductor.worker.call")
        .collect();
    calls.sort_by_key(|event| event.seq);

    let mut terminals_by_call: HashMap<String, &ConductorDelegationEvent> = HashMap::new();
    let mut terminals_by_worker: HashMap<String, Vec<&ConductorDelegationEvent>> = HashMap::new();
    for event in delegations.iter().filter(|event| {
        event.run_id == run_id
            && matches!(
                event.event_type.as_str(),
                "conductor.capability.completed"
                    | "conductor.capability.failed"
                    | "conductor.capability.blocked"
            )
    }) {
        if let Some(call_id) = event.call_id.as_ref() {
            terminals_by_call
                .entry(call_id.clone())
                .and_modify(|current| {
                    if event.seq > current.seq {
                        *current = event;
                    }
                })
                .or_insert(event);
        }
        if let Some(worker_type) = event.worker_type.as_ref() {
            terminals_by_worker
                .entry(worker_type.clone())
                .or_default()
                .push(event);
        }
    }

    let mut bands = Vec::new();
    for call in calls {
        let worker_type = call
            .worker_type
            .clone()
            .unwrap_or_else(|| "worker".to_string());
        let terminal = call
            .call_id
            .as_ref()
            .and_then(|call_id| terminals_by_call.get(call_id).copied())
            .or_else(|| {
                terminals_by_worker.get(&worker_type).and_then(|events| {
                    events
                        .iter()
                        .copied()
                        .filter(|event| event.seq >= call.seq)
                        .min_by_key(|event| event.seq)
                })
            });
        let status = match terminal.map(|event| event.event_type.as_str()) {
            Some("conductor.capability.completed") => "completed",
            Some("conductor.capability.failed") => "failed",
            Some("conductor.capability.blocked") => "blocked",
            _ => "inflight",
        }
        .to_string();
        let duration_ms =
            terminal.and_then(|event| duration_between_ms(&call.timestamp, &event.timestamp));
        bands.push(DelegationTimelineBand {
            worker_type,
            worker_objective: call.worker_objective.clone(),
            status,
            duration_ms,
            call_id: call.call_id.clone(),
            loop_id: loop_id_for_call(call.call_id.as_deref(), lifecycle),
        });
    }

    bands
}

fn status_from_tool_pair(pair: &ToolTracePair) -> TrajectoryStatus {
    match pair.status() {
        "completed" => TrajectoryStatus::Completed,
        "failed" => TrajectoryStatus::Failed,
        "started" => TrajectoryStatus::Inflight,
        _ => TrajectoryStatus::Inflight,
    }
}

fn status_from_trace(trace: &TraceGroup) -> TrajectoryStatus {
    match trace.status() {
        "completed" => TrajectoryStatus::Completed,
        "failed" => TrajectoryStatus::Failed,
        "started" => TrajectoryStatus::Inflight,
        _ => TrajectoryStatus::Inflight,
    }
}

fn status_from_lifecycle(event: &WorkerLifecycleEvent) -> TrajectoryStatus {
    match event.event_type.as_str() {
        "worker.task.completed" => TrajectoryStatus::Completed,
        "worker.task.failed" => TrajectoryStatus::Failed,
        "worker.task.finding" | "worker.task.learning" => TrajectoryStatus::Blocked,
        _ => TrajectoryStatus::Inflight,
    }
}

fn build_trajectory_cells(
    traces: &[TraceGroup],
    tools: &[ToolTraceEvent],
    lifecycle: &[WorkerLifecycleEvent],
    delegations: &[ConductorDelegationEvent],
    run_id: &str,
) -> Vec<TrajectoryCell> {
    #[derive(Clone)]
    struct RawCell {
        seq: i64,
        row_key: String,
        event_type: String,
        tool_name: Option<String>,
        actor_key: Option<String>,
        status: TrajectoryStatus,
        duration_ms: Option<i64>,
        total_tokens: Option<i64>,
        loop_id: String,
        item_id: String,
    }

    let mut raw = Vec::<RawCell>::new();
    for trace in traces.iter().filter(|trace| trace.run_id() == Some(run_id)) {
        let loop_id = trace
            .task_id()
            .map(ToString::to_string)
            .or_else(|| trace.call_id().map(|call_id| format!("call:{call_id}")))
            .unwrap_or_else(|| "direct".to_string());
        raw.push(RawCell {
            seq: trace.seq(),
            row_key: format!("llm:{}", trace.actor_key()),
            event_type: trace
                .terminal
                .as_ref()
                .map(|event| event.event_type.clone())
                .unwrap_or_else(|| "llm.call.started".to_string()),
            tool_name: None,
            actor_key: Some(trace.actor_key()),
            status: status_from_trace(trace),
            duration_ms: trace.duration_ms(),
            total_tokens: trace.total_tokens(),
            loop_id,
            item_id: trace.trace_id.clone(),
        });
    }

    let tool_pairs = pair_tool_events(
        tools
            .iter()
            .filter(|tool| tool.run_id.as_deref() == Some(run_id))
            .cloned()
            .collect(),
    );
    for pair in &tool_pairs {
        let tool_name = pair.tool_name().to_string();
        let loop_id = pair
            .call
            .as_ref()
            .and_then(|event| event.task_id.clone())
            .or_else(|| {
                pair.call
                    .as_ref()
                    .and_then(|event| event.call_id.clone())
                    .map(|call_id| format!("call:{call_id}"))
            })
            .or_else(|| pair.result.as_ref().and_then(|event| event.task_id.clone()))
            .or_else(|| {
                pair.result
                    .as_ref()
                    .and_then(|event| event.call_id.clone())
                    .map(|call_id| format!("call:{call_id}"))
            })
            .unwrap_or_else(|| "direct".to_string());
        raw.push(RawCell {
            seq: pair.seq(),
            row_key: format!("tool:{tool_name}"),
            event_type: pair
                .result
                .as_ref()
                .map(|event| event.event_type.clone())
                .or_else(|| pair.call.as_ref().map(|event| event.event_type.clone()))
                .unwrap_or_else(|| "worker.tool.call".to_string()),
            tool_name: Some(tool_name),
            actor_key: None,
            status: status_from_tool_pair(pair),
            duration_ms: pair.duration_ms(),
            total_tokens: None,
            loop_id,
            item_id: pair.tool_trace_id.clone(),
        });
    }

    for event in lifecycle
        .iter()
        .filter(|event| event.run_id.as_deref() == Some(run_id))
    {
        raw.push(RawCell {
            seq: event.seq,
            row_key: format!("worker:{}", event.worker_id),
            event_type: event.event_type.clone(),
            tool_name: None,
            actor_key: None,
            status: status_from_lifecycle(event),
            duration_ms: None,
            total_tokens: None,
            loop_id: event.task_id.clone(),
            item_id: event.event_id.clone(),
        });
    }

    for delegation in delegations
        .iter()
        .filter(|event| event.run_id == run_id && event.event_type == "conductor.worker.call")
    {
        let worker_type = delegation
            .worker_type
            .clone()
            .unwrap_or_else(|| "worker".to_string());
        let terminal = delegations.iter().find(|candidate| {
            candidate.run_id == run_id
                && matches!(
                    candidate.event_type.as_str(),
                    "conductor.capability.completed"
                        | "conductor.capability.failed"
                        | "conductor.capability.blocked"
                )
                && delegation
                    .call_id
                    .as_ref()
                    .zip(candidate.call_id.as_ref())
                    .map(|(left, right)| left == right)
                    .unwrap_or(false)
        });
        let status = match terminal.map(|event| event.event_type.as_str()) {
            Some("conductor.capability.completed") => TrajectoryStatus::Completed,
            Some("conductor.capability.failed") => TrajectoryStatus::Failed,
            Some("conductor.capability.blocked") => TrajectoryStatus::Blocked,
            _ => TrajectoryStatus::Inflight,
        };
        let loop_id = loop_id_for_call(delegation.call_id.as_deref(), lifecycle)
            .unwrap_or_else(|| "direct".to_string());
        raw.push(RawCell {
            seq: delegation.seq,
            row_key: format!("delegation:{worker_type}"),
            event_type: delegation.event_type.clone(),
            tool_name: None,
            actor_key: None,
            status,
            duration_ms: terminal
                .and_then(|event| duration_between_ms(&delegation.timestamp, &event.timestamp)),
            total_tokens: None,
            loop_id,
            item_id: delegation
                .call_id
                .clone()
                .unwrap_or_else(|| delegation.event_id.clone()),
        });
    }

    raw.sort_by_key(|cell| cell.seq);
    raw.into_iter()
        .enumerate()
        .map(|(step_index, cell)| TrajectoryCell {
            seq: cell.seq,
            step_index,
            row_key: cell.row_key,
            event_type: cell.event_type,
            tool_name: cell.tool_name,
            actor_key: cell.actor_key,
            status: cell.status,
            duration_ms: cell.duration_ms,
            total_tokens: cell.total_tokens,
            loop_id: cell.loop_id,
            item_id: cell.item_id,
        })
        .collect()
}

fn bucket_trajectory_cells(cells: &[TrajectoryCell], max_columns: usize) -> Vec<TrajectoryCell> {
    let max_step = cells
        .iter()
        .map(|cell| cell.step_index)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    if max_step <= max_columns || max_columns == 0 {
        return cells.to_vec();
    }
    let mut by_bucket: HashMap<(String, usize), TrajectoryCell> = HashMap::new();

    for cell in cells {
        let bucket_index = cell.step_index.saturating_mul(max_columns) / max_step.max(1);
        let key = (cell.row_key.clone(), bucket_index);
        by_bucket
            .entry(key)
            .and_modify(|current| {
                if trajectory_status_rank(&cell.status) > trajectory_status_rank(&current.status) {
                    current.status = cell.status.clone();
                }
                current.duration_ms = current.duration_ms.max(cell.duration_ms);
                current.total_tokens = current.total_tokens.max(cell.total_tokens);
                if cell.seq < current.seq {
                    current.seq = cell.seq;
                }
            })
            .or_insert_with(|| {
                let mut cloned = cell.clone();
                cloned.step_index = bucket_index;
                cloned
            });
    }

    let mut out: Vec<TrajectoryCell> = by_bucket.into_values().collect();
    out.sort_by(|a, b| {
        a.step_index
            .cmp(&b.step_index)
            .then_with(|| a.row_key.cmp(&b.row_key))
    });
    out
}

fn row_sort_key(row_key: &str) -> (usize, String) {
    if row_key.starts_with("llm:") {
        (0, row_key.to_string())
    } else if row_key.starts_with("tool:") {
        (1, row_key.to_string())
    } else if row_key.starts_with("worker:") {
        (2, row_key.to_string())
    } else if row_key.starts_with("delegation:") {
        (3, row_key.to_string())
    } else {
        (9, row_key.to_string())
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

fn build_run_sparkline(
    run_id: &str,
    traces: &[TraceGroup],
    tools: &[ToolTraceEvent],
) -> Vec<(f32, f32, String)> {
    #[derive(Clone)]
    struct Dot {
        seq: i64,
        status: String,
    }

    let mut dots: Vec<Dot> = traces
        .iter()
        .filter(|trace| trace.run_id() == Some(run_id))
        .map(|trace| Dot {
            seq: trace.seq(),
            status: trace.status().to_string(),
        })
        .collect();
    let tool_pairs = pair_tool_events(
        tools
            .iter()
            .filter(|tool| tool.run_id.as_deref() == Some(run_id))
            .cloned()
            .collect(),
    );
    dots.extend(tool_pairs.iter().map(|pair| Dot {
        seq: pair.seq(),
        status: pair.status().to_string(),
    }));
    dots.sort_by_key(|dot| dot.seq);
    dots.truncate(60);

    let width = 120.0;
    let spacing = if dots.len() > 1 {
        (width - 8.0) / (dots.len() as f32 - 1.0)
    } else {
        0.0
    };
    dots.into_iter()
        .enumerate()
        .map(|(idx, dot)| {
            let color = match dot.status.as_str() {
                "completed" => "#22c55e",
                "failed" => "#ef4444",
                "started" => "#f59e0b",
                _ => "#94a3b8",
            }
            .to_string();
            let x = 4.0 + idx as f32 * spacing;
            let y = match dot.status.as_str() {
                "failed" => 11.0,
                "started" => 8.0,
                _ => 6.0,
            };
            (x, y, color)
        })
        .collect()
}

#[component]
fn TrajectoryGrid(
    cells: Vec<TrajectoryCell>,
    display_mode: TrajectoryMode,
    on_select: EventHandler<(String, String)>,
    on_mode_change: EventHandler<TrajectoryMode>,
) -> Element {
    let cells = bucket_trajectory_cells(&cells, TRACE_TRAJECTORY_MAX_COLUMNS);
    let mut rows: Vec<String> = cells
        .iter()
        .map(|cell| cell.row_key.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    rows.sort_by_key(|row| row_sort_key(row));

    let row_lookup: HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| (row.clone(), idx))
        .collect();
    let max_step = cells
        .iter()
        .map(|cell| cell.step_index)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    let mut max_duration = 1_i64;
    let mut max_tokens = 1_i64;
    for cell in &cells {
        if let Some(duration) = cell.duration_ms {
            max_duration = max_duration.max(duration.max(1));
        }
        if let Some(tokens) = cell.total_tokens {
            max_tokens = max_tokens.max(tokens.max(1));
        }
    }

    let left_pad = 182.0_f32;
    let top_pad = 22.0_f32;
    let col_gap = 12.5_f32;
    let row_gap = 18.0_f32;
    let width = left_pad + (max_step as f32 * col_gap) + 16.0;
    let height = top_pad + (rows.len() as f32 * row_gap) + 18.0;
    let view_box = format!("0 0 {:.1} {:.1}", width.max(420.0), height.max(80.0));

    rsx! {
        div {
            class: "trace-traj-grid",
            div {
                class: "trace-traj-grid-head",
                h5 {
                    class: "trace-loop-title",
                    "Trajectory Grid"
                }
                div {
                    style: "display:flex;gap:0.32rem;flex-wrap:wrap;",
                    for mode in [TrajectoryMode::Status, TrajectoryMode::Duration, TrajectoryMode::Tokens] {
                        button {
                            class: "trace-pill",
                            style: if mode == display_mode { "border-color:#60a5fa;color:#dbeafe;" } else { "" },
                            onclick: move |_| on_mode_change.call(mode),
                            "{mode.label()}"
                        }
                    }
                }
            }
            svg {
                width: "100%",
                height: format!("{:.0}", height.max(100.0)),
                view_box: "{view_box}",
                for row in &rows {
                    if let Some(row_idx) = row_lookup.get(row) {
                        text {
                            x: "4",
                            y: format!("{:.1}", top_pad + *row_idx as f32 * row_gap + 4.0),
                            class: "trace-traj-row-label",
                            fill: "#93c5fd",
                            font_size: "10",
                            "{row}"
                        }
                    }
                }
                for cell in &cells {
                    if let Some(row_idx) = row_lookup.get(&cell.row_key) {
                        {
                            let x = left_pad + cell.step_index as f32 * col_gap;
                            let y = top_pad + *row_idx as f32 * row_gap;
                            let mut radius = 3.8_f32;
                            if display_mode == TrajectoryMode::Duration {
                                if let Some(duration) = cell.duration_ms {
                                    let ratio = (duration.max(1) as f64).ln() / (max_duration as f64).ln().max(1.0);
                                    radius = (2.6 + (ratio as f32 * 4.8)).clamp(2.2, 7.8);
                                }
                            } else if display_mode == TrajectoryMode::Tokens {
                                if let Some(tokens) = cell.total_tokens {
                                    let ratio = (tokens.max(1) as f64).ln() / (max_tokens as f64).ln().max(1.0);
                                    radius = (2.4 + (ratio as f32 * 5.2)).clamp(2.0, 8.1);
                                }
                            }
                            let class = trajectory_status_class(&cell.status);
                            let loop_id = cell.loop_id.clone();
                            let item_id = cell.item_id.clone();
                            rsx! {
                                circle {
                                    cx: format!("{:.2}", x),
                                    cy: format!("{:.2}", y),
                                    r: format!("{:.2}", radius),
                                    class: "{class}",
                                    onclick: move |_| on_select.call((loop_id.clone(), item_id.clone())),
                                }
                                if display_mode == TrajectoryMode::Duration
                                    && cell.duration_ms.unwrap_or_default() > TRACE_SLOW_DURATION_MS
                                {
                                    circle {
                                        cx: format!("{:.2}", x),
                                        cy: format!("{:.2}", y),
                                        r: format!("{:.2}", radius + 1.8),
                                        class: "trace-traj-slow-ring"
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
                                                            class: if render.node.kind == GraphNodeKind::Worker { "trace-worker-node" } else { "" },
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
                                                                selected_loop_id.set(Some(loop_id));
                                                                scroll_to_element_id(&dom_id);
                                                            } else if let Some(call_id) = call_id.clone() {
                                                                let fallback = format!("call:{call_id}");
                                                                selected_loop_id.set(Some(fallback.clone()));
                                                                scroll_to_element_id(&loop_dom_id(&fallback));
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
                                        on_mode_change: move |next_mode| trajectory_mode.set(next_mode),
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
                                                    class: if active_actor_key.as_deref() == Some(actor_key.as_str()) {
                                                        "trace-node-chip active"
                                                    } else {
                                                        "trace-node-chip"
                                                    },
                                                    onclick: {
                                                        let actor_key = actor_key.clone();
                                                        move |_| {
                                                            selected_actor_key.set(Some(actor_key.clone()));
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
                                            let lifecycle_for_group: Vec<WorkerLifecycleEvent> = lifecycle_for_run
                                                .iter()
                                                .filter(|event| {
                                                    if group_loop_id.starts_with("call:") {
                                                        let expected_call = group_loop_id.trim_start_matches("call:");
                                                        event.call_id.as_deref() == Some(expected_call)
                                                    } else {
                                                        event.task_id == group_loop_id
                                                    }
                                                })
                                                .cloned()
                                                .collect();
                                            let (worker_state, worker_state_message) = worker_summary(
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
                                                if let Some(message) = worker_state_message.as_ref() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trace(seq: i64, run_id: &str, trace_id: &str, status: &str) -> TraceGroup {
        let started = TraceEvent {
            seq,
            event_id: format!("{trace_id}-started"),
            trace_id: trace_id.to_string(),
            timestamp: "2026-02-20T10:00:00Z".to_string(),
            event_type: "llm.call.started".to_string(),
            role: "conductor".to_string(),
            function_name: "respond".to_string(),
            model_used: "test-model".to_string(),
            provider: Some("test".to_string()),
            actor_id: "conductor:1".to_string(),
            run_id: Some(run_id.to_string()),
            task_id: Some("task-a".to_string()),
            call_id: Some("call-a".to_string()),
            system_context: None,
            input: None,
            input_summary: None,
            output: None,
            output_summary: None,
            duration_ms: None,
            error_code: None,
            error_message: None,
            failure_kind: None,
            input_tokens: Some(12),
            output_tokens: Some(6),
            cached_input_tokens: Some(2),
            total_tokens: Some(18),
        };
        let terminal_type = if status == "failed" {
            "llm.call.failed"
        } else {
            "llm.call.completed"
        };
        let terminal = TraceEvent {
            seq: seq + 1,
            event_id: format!("{trace_id}-terminal"),
            trace_id: trace_id.to_string(),
            timestamp: "2026-02-20T10:00:01Z".to_string(),
            event_type: terminal_type.to_string(),
            role: "conductor".to_string(),
            function_name: "respond".to_string(),
            model_used: "test-model".to_string(),
            provider: Some("test".to_string()),
            actor_id: "conductor:1".to_string(),
            run_id: Some(run_id.to_string()),
            task_id: Some("task-a".to_string()),
            call_id: Some("call-a".to_string()),
            system_context: None,
            input: None,
            input_summary: None,
            output: None,
            output_summary: None,
            duration_ms: Some(420),
            error_code: None,
            error_message: None,
            failure_kind: None,
            input_tokens: Some(12),
            output_tokens: Some(6),
            cached_input_tokens: Some(2),
            total_tokens: Some(18),
        };
        TraceGroup {
            trace_id: trace_id.to_string(),
            started: Some(started),
            terminal: Some(terminal),
        }
    }

    fn make_tool_event(
        seq: i64,
        event_type: &str,
        run_id: &str,
        tool_trace_id: &str,
        success: Option<bool>,
    ) -> ToolTraceEvent {
        ToolTraceEvent {
            seq,
            event_id: format!("{tool_trace_id}:{seq}"),
            event_type: event_type.to_string(),
            tool_trace_id: tool_trace_id.to_string(),
            timestamp: "2026-02-20T10:00:00Z".to_string(),
            role: "terminal".to_string(),
            actor_id: "terminal:1".to_string(),
            tool_name: "file_read".to_string(),
            run_id: Some(run_id.to_string()),
            task_id: Some("task-a".to_string()),
            call_id: Some("call-a".to_string()),
            success,
            duration_ms: Some(210),
            reasoning: None,
            tool_args: None,
            output: None,
            error: None,
        }
    }

    #[test]
    fn test_trajectory_cells_build_correctly() {
        let run_id = "run-trajectory";
        let traces = vec![
            make_trace(10, run_id, "trace-1", "completed"),
            make_trace(30, run_id, "trace-2", "completed"),
            make_trace(50, run_id, "trace-3", "completed"),
        ];
        let tools = vec![
            make_tool_event(15, "worker.tool.call", run_id, "tool-1", None),
            make_tool_event(16, "worker.tool.result", run_id, "tool-1", Some(true)),
            make_tool_event(40, "worker.tool.call", run_id, "tool-2", None),
            make_tool_event(41, "worker.tool.result", run_id, "tool-2", Some(false)),
        ];

        let cells = build_trajectory_cells(&traces, &tools, &[], &[], run_id);
        assert!(!cells.is_empty(), "expected trajectory cells");
        assert!(
            cells
                .windows(2)
                .all(|window| window[0].step_index < window[1].step_index),
            "step_index should be strictly increasing"
        );
        assert!(
            cells.iter().any(|cell| cell.row_key.starts_with("llm:")),
            "missing llm row"
        );
        assert!(
            cells.iter().any(|cell| cell.row_key.starts_with("tool:")),
            "missing tool row"
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.status == TrajectoryStatus::Failed
                    && cell.row_key.starts_with("tool:")),
            "missing failed tool cell"
        );
    }

    #[test]
    fn test_trajectory_cells_long_run_bucketing() {
        let cells: Vec<TrajectoryCell> = (0..120)
            .map(|idx| TrajectoryCell {
                seq: idx as i64,
                step_index: idx,
                row_key: "tool:file_read".to_string(),
                event_type: "worker.tool.result".to_string(),
                tool_name: Some("file_read".to_string()),
                actor_key: None,
                status: if idx == 17 {
                    TrajectoryStatus::Failed
                } else {
                    TrajectoryStatus::Completed
                },
                duration_ms: Some(100 + idx as i64),
                total_tokens: None,
                loop_id: "task-a".to_string(),
                item_id: format!("item-{idx}"),
            })
            .collect();

        let bucketed = bucket_trajectory_cells(&cells, 80);
        let max_column = bucketed
            .iter()
            .map(|cell| cell.step_index)
            .max()
            .unwrap_or(0);

        assert_eq!(cells.len(), 120);
        assert!(
            max_column < 80,
            "bucketed columns should fit within 80, got {}",
            max_column + 1
        );
        assert!(
            bucketed
                .iter()
                .any(|cell| cell.status == TrajectoryStatus::Failed),
            "failed status should survive bucketing"
        );
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
