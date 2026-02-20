use std::collections::{BTreeSet, HashMap};

use super::parsers::normalize_actor_key;
use super::types::{
    ConductorDelegationEvent, GraphEdge, GraphLayout, GraphNode, GraphNodeKind, PromptEvent,
    RunGraphSummary, ToolTraceEvent, TraceGroup, WorkerLifecycleEvent, WriterEnqueueEvent,
};

// ── Ranking helpers ──────────────────────────────────────────────────────────

pub fn actor_rank(actor_key: &str) -> usize {
    match actor_key {
        "conductor" => 0,
        "writer" => 1,
        "researcher" => 2,
        "terminal" => 3,
        _ => 9,
    }
}

pub fn display_actor_label(actor_key: &str) -> String {
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

pub fn display_worker_label(worker_id: &str) -> String {
    worker_id
        .split(':')
        .next_back()
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| worker_id.to_string())
}

pub fn get_actor_node_color(actor_key: &str) -> (&'static str, &'static str, &'static str) {
    match actor_key {
        "conductor" => ("#111827", "#3b82f6", "#60a5fa"),
        "researcher" => ("#0b1225", "#22c55e", "#86efac"),
        "terminal" => ("#0b1225", "#f59e0b", "#fcd34d"),
        "writer" => ("#0b1225", "#c084fc", "#ddd6fe"),
        _ => ("#0b1225", "#64748b", "#cbd5e1"),
    }
}

pub fn graph_node_color(node: &GraphNode) -> (&'static str, &'static str, &'static str) {
    match node.kind {
        GraphNodeKind::Prompt => ("#0f172a", "#475569", "#93c5fd"),
        GraphNodeKind::Tools => ("#111827", "#06b6d4", "#67e8f9"),
        GraphNodeKind::Worker => ("#082f49", "#38bdf8", "#bae6fd"),
        GraphNodeKind::Actor => get_actor_node_color(node.actor_key.as_deref().unwrap_or_default()),
    }
}

pub fn graph_status_color(status: &str) -> &'static str {
    match status {
        "completed" => "#22c55e",
        "failed" => "#ef4444",
        "started" => "#f59e0b",
        "degraded" => "#f97316",
        _ => "#94a3b8",
    }
}

pub fn run_status_class(status: &str) -> &'static str {
    match status {
        "completed" => "trace-run-status trace-run-status--completed",
        "failed" => "trace-run-status trace-run-status--failed",
        _ => "trace-run-status trace-run-status--in-progress",
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

// ── Run graph summaries ──────────────────────────────────────────────────────

pub fn build_run_graph_summaries(
    traces: &[TraceGroup],
    prompts: &[PromptEvent],
    tools: &[ToolTraceEvent],
    writer_enqueues: &[WriterEnqueueEvent],
    delegations: &[ConductorDelegationEvent],
    run_events: &[super::types::ConductorRunEvent],
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

// ── Graph node building ──────────────────────────────────────────────────────

pub fn build_graph_nodes_for_run(
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

// ── Graph edges ──────────────────────────────────────────────────────────────

pub fn build_graph_edges(
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

// ── Graph layout ─────────────────────────────────────────────────────────────

pub fn build_graph_layout(nodes: &[GraphNode]) -> GraphLayout {
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
