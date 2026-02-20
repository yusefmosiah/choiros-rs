use std::collections::HashMap;

// ── Constants ────────────────────────────────────────────────────────────────

pub const TRACE_PRELOAD_WINDOW: i64 = 5_000;
pub const TRACE_PRELOAD_PAGE_LIMIT: i64 = 1_000;
pub const TRACE_SLOW_DURATION_MS: i64 = 5_000;
pub const TRACE_TRAJECTORY_MAX_COLUMNS: usize = 80;

// ── View mode ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceViewMode {
    Overview,
    RunDetail,
}

// ── LLM call events ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TraceEvent {
    pub seq: i64,
    pub event_id: String,
    pub trace_id: String,
    pub timestamp: String,
    pub event_type: String,
    pub role: String,
    pub function_name: String,
    pub model_used: String,
    pub provider: Option<String>,
    pub actor_id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub call_id: Option<String>,
    pub system_context: Option<String>,
    pub input: Option<serde_json::Value>,
    pub input_summary: Option<String>,
    pub output: Option<serde_json::Value>,
    pub output_summary: Option<String>,
    pub duration_ms: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub failure_kind: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct TraceGroup {
    pub trace_id: String,
    pub started: Option<TraceEvent>,
    pub terminal: Option<TraceEvent>,
}

// ── Prompt / run events ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct PromptEvent {
    pub seq: i64,
    pub event_id: String,
    pub timestamp: String,
    pub run_id: String,
    pub objective: String,
}

// ── Tool trace events ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ToolTraceEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub tool_trace_id: String,
    pub timestamp: String,
    pub role: String,
    pub actor_id: String,
    pub tool_name: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub call_id: Option<String>,
    pub success: Option<bool>,
    pub duration_ms: Option<i64>,
    pub reasoning: Option<String>,
    pub tool_args: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ── Writer enqueue events ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct WriterEnqueueEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub run_id: String,
    pub call_id: Option<String>,
}

// ── Conductor delegation events ──────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ConductorDelegationEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub run_id: String,
    pub worker_type: Option<String>,
    pub worker_objective: Option<String>,
    pub success: Option<bool>,
    pub result_summary: Option<String>,
    pub call_id: Option<String>,
    pub capability: Option<String>,
    pub error: Option<String>,
    pub failure_kind: Option<String>,
    pub reason: Option<String>,
    pub lane: Option<String>,
}

// ── Conductor run events ─────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ConductorRunEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub run_id: String,
    pub phase: Option<String>,
    pub status: Option<String>,
    pub message: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

// ── Worker lifecycle events ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct WorkerLifecycleEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub worker_id: String,
    pub task_id: String,
    pub phase: String,
    pub run_id: Option<String>,
    pub objective: Option<String>,
    pub model_used: Option<String>,
    pub message: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub finding_id: Option<String>,
    pub claim: Option<String>,
    pub confidence: Option<f64>,
    pub learning_id: Option<String>,
    pub insight: Option<String>,
    pub call_id: Option<String>,
}

// ── Trajectory types ─────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct TrajectoryCell {
    pub seq: i64,
    pub step_index: usize,
    pub row_key: String,
    pub event_type: String,
    pub tool_name: Option<String>,
    pub actor_key: Option<String>,
    pub status: TrajectoryStatus,
    pub duration_ms: Option<i64>,
    pub total_tokens: Option<i64>,
    pub loop_id: String,
    pub item_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrajectoryStatus {
    Completed,
    Failed,
    Inflight,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrajectoryMode {
    Status,
    Duration,
    Tokens,
}

// ── Delegation timeline ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DelegationTimelineBand {
    pub worker_type: String,
    pub worker_objective: Option<String>,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub call_id: Option<String>,
    pub loop_id: Option<String>,
}

// ── Run summary ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RunGraphSummary {
    pub run_id: String,
    pub objective: String,
    pub timestamp: String,
    pub llm_calls: usize,
    pub tool_calls: usize,
    pub tool_failures: usize,
    pub writer_enqueues: usize,
    pub writer_enqueue_failures: usize,
    pub actor_count: usize,
    pub loop_count: usize,
    pub worker_count: usize,
    pub worker_failures: usize,
    pub worker_calls: usize,
    pub capability_failures: usize,
    pub run_status: String,
    pub total_duration_ms: i64,
    pub total_tokens: i64,
}

// ── Graph types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphNodeKind {
    Prompt,
    Actor,
    Worker,
    Tools,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GraphNode {
    pub key: String,
    pub label: String,
    pub kind: GraphNodeKind,
    pub actor_key: Option<String>,
    pub worker_id: Option<String>,
    pub task_id: Option<String>,
    pub llm_calls: usize,
    pub tool_calls: usize,
    pub inbound_events: usize,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct GraphLayout {
    pub width: f32,
    pub height: f32,
    pub positions: HashMap<String, (f32, f32)>,
}

#[derive(Clone, Debug)]
pub struct GraphRenderNode {
    pub node: GraphNode,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub color: String,
    pub dashed: bool,
}

#[derive(Clone, Debug)]
pub struct GraphEdgeSegment {
    pub edge: GraphEdge,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

// ── Loop / sequence types ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TraceLoopGroup {
    pub loop_id: String,
    pub traces: Vec<TraceGroup>,
    pub sequence: Vec<LoopSequenceItem>,
}

#[derive(Clone, Debug)]
pub struct ToolTracePair {
    pub tool_trace_id: String,
    pub call: Option<ToolTraceEvent>,
    pub result: Option<ToolTraceEvent>,
}

#[derive(Clone, Debug)]
pub enum LoopSequenceItem {
    Llm(TraceGroup),
    Tool(ToolTracePair),
}
