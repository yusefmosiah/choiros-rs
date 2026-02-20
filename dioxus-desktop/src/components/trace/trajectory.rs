use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use super::parsers::pair_tool_events;
use super::types::{
    ConductorDelegationEvent, DelegationTimelineBand, ToolTraceEvent, ToolTracePair, TraceGroup,
    TrajectoryCell, TrajectoryMode, TrajectoryStatus, WorkerLifecycleEvent, TRACE_SLOW_DURATION_MS,
    TRACE_TRAJECTORY_MAX_COLUMNS,
};

// ── Time helpers ─────────────────────────────────────────────────────────────

fn parse_rfc3339_utc(timestamp: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn duration_between_ms(start_ts: &str, end_ts: &str) -> Option<i64> {
    let start = parse_rfc3339_utc(start_ts)?;
    let end = parse_rfc3339_utc(end_ts)?;
    Some((end - start).num_milliseconds().max(0))
}

// ── Status helpers ───────────────────────────────────────────────────────────

pub fn trajectory_status_rank(status: &TrajectoryStatus) -> usize {
    match status {
        TrajectoryStatus::Failed => 4,
        TrajectoryStatus::Blocked => 3,
        TrajectoryStatus::Inflight => 2,
        TrajectoryStatus::Completed => 1,
    }
}

pub fn trajectory_status_class(status: &TrajectoryStatus) -> &'static str {
    match status {
        TrajectoryStatus::Completed => "trace-traj-cell--completed",
        TrajectoryStatus::Failed => "trace-traj-cell--failed",
        TrajectoryStatus::Inflight => "trace-traj-cell--inflight",
        TrajectoryStatus::Blocked => "trace-traj-cell--blocked",
    }
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

// ── Sparkline ────────────────────────────────────────────────────────────────

pub fn build_run_sparkline(
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

// ── Delegation timeline ──────────────────────────────────────────────────────

fn loop_id_for_call(call_id: Option<&str>, lifecycle: &[WorkerLifecycleEvent]) -> Option<String> {
    let call_id = call_id?;
    lifecycle
        .iter()
        .find(|event| event.call_id.as_deref() == Some(call_id))
        .map(|event| event.task_id.clone())
}

pub fn build_delegation_timeline_bands(
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

// ── Trajectory cell building ─────────────────────────────────────────────────

pub fn build_trajectory_cells(
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

pub fn bucket_trajectory_cells(
    cells: &[TrajectoryCell],
    max_columns: usize,
) -> Vec<TrajectoryCell> {
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

// ── TrajectoryGrid component ─────────────────────────────────────────────────

#[component]
pub fn TrajectoryGrid(
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

// ── Loop group helpers (also used in view.rs) ─────────────────────────────────

impl TrajectoryMode {
    pub fn label(self) -> &'static str {
        match self {
            TrajectoryMode::Status => "Status",
            TrajectoryMode::Duration => "Duration",
            TrajectoryMode::Tokens => "Tokens",
        }
    }
}

// ── Loop group building ───────────────────────────────────────────────────────

use super::types::{LoopSequenceItem, TraceLoopGroup};

pub fn merge_loop_sequence(
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

pub fn build_loop_groups_for_actor(
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
