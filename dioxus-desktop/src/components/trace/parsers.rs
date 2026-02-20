use crate::api::LogsEvent;

use super::types::{
    ConductorDelegationEvent, ConductorRunEvent, PromptEvent, ToolTraceEvent, TraceEvent,
    TraceGroup, WorkerLifecycleEvent, WriterEnqueueEvent,
};

// ── Helper utilities ─────────────────────────────────────────────────────────

pub fn payload_run_id(payload: &serde_json::Value) -> Option<String> {
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

pub fn decode_json_payload(value: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    match value? {
        serde_json::Value::String(raw) => serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .or_else(|| Some(serde_json::Value::String(raw.clone()))),
        other => Some(other.clone()),
    }
}

pub fn pretty_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}

// ── Actor key helpers ────────────────────────────────────────────────────────

pub fn sanitize_actor_key(raw: &str) -> String {
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

pub fn normalize_actor_key(role: &str, actor_id: &str) -> String {
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

// ── TraceGroup / TraceEvent helpers ─────────────────────────────────────────

impl TraceGroup {
    pub fn status(&self) -> &'static str {
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

    pub fn seq(&self) -> i64 {
        self.started
            .as_ref()
            .map(|e| e.seq)
            .or_else(|| self.terminal.as_ref().map(|e| e.seq))
            .unwrap_or(0)
    }

    pub fn timestamp(&self) -> String {
        self.started
            .as_ref()
            .map(|e| e.timestamp.clone())
            .or_else(|| self.terminal.as_ref().map(|e| e.timestamp.clone()))
            .unwrap_or_default()
    }

    pub fn role(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.role.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.role.as_str()))
            .unwrap_or("unknown")
    }

    pub fn function_name(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.function_name.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.function_name.as_str()))
            .unwrap_or("unknown")
    }

    pub fn model_used(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.model_used.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.model_used.as_str()))
            .unwrap_or("unknown")
    }

    pub fn provider(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.provider.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.provider.as_deref()))
    }

    pub fn actor_id(&self) -> &str {
        self.started
            .as_ref()
            .map(|e| e.actor_id.as_str())
            .or_else(|| self.terminal.as_ref().map(|e| e.actor_id.as_str()))
            .unwrap_or("unknown")
    }

    pub fn actor_key(&self) -> String {
        normalize_actor_key(self.role(), self.actor_id())
    }

    pub fn run_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.run_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.run_id.as_deref()))
    }

    pub fn task_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.task_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.task_id.as_deref()))
    }

    pub fn call_id(&self) -> Option<&str> {
        self.started
            .as_ref()
            .and_then(|e| e.call_id.as_deref())
            .or_else(|| self.terminal.as_ref().and_then(|e| e.call_id.as_deref()))
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.duration_ms)
            .or_else(|| self.started.as_ref().and_then(|s| s.duration_ms))
    }

    pub fn input_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.input_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.input_tokens))
    }

    pub fn output_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.output_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.output_tokens))
    }

    pub fn cached_input_tokens(&self) -> Option<i64> {
        self.terminal
            .as_ref()
            .and_then(|t| t.cached_input_tokens)
            .or_else(|| self.started.as_ref().and_then(|s| s.cached_input_tokens))
    }

    pub fn total_tokens(&self) -> Option<i64> {
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
    pub fn actor_key(&self) -> String {
        normalize_actor_key(&self.role, &self.actor_id)
    }

    pub fn loop_id(&self) -> String {
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

use super::types::ToolTracePair;

impl ToolTracePair {
    pub fn seq(&self) -> i64 {
        self.call
            .as_ref()
            .map(|event| event.seq)
            .or_else(|| self.result.as_ref().map(|event| event.seq))
            .unwrap_or(0)
    }

    pub fn tool_name(&self) -> &str {
        self.call
            .as_ref()
            .map(|event| event.tool_name.as_str())
            .or_else(|| self.result.as_ref().map(|event| event.tool_name.as_str()))
            .unwrap_or("unknown")
    }

    pub fn status(&self) -> &'static str {
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

    pub fn duration_ms(&self) -> Option<i64> {
        self.result
            .as_ref()
            .and_then(|event| event.duration_ms)
            .or_else(|| self.call.as_ref().and_then(|event| event.duration_ms))
    }
}

// ── Parse functions ──────────────────────────────────────────────────────────

pub fn parse_trace_event(event: &LogsEvent) -> Option<TraceEvent> {
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

pub fn parse_prompt_event(event: &LogsEvent) -> Option<PromptEvent> {
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

pub fn parse_tool_trace_event(event: &LogsEvent) -> Option<ToolTraceEvent> {
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

pub fn parse_writer_enqueue_event(event: &LogsEvent) -> Option<WriterEnqueueEvent> {
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

pub fn parse_conductor_delegation_event(event: &LogsEvent) -> Option<ConductorDelegationEvent> {
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

pub fn parse_conductor_run_event(event: &LogsEvent) -> Option<ConductorRunEvent> {
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

pub fn parse_worker_lifecycle_event(event: &LogsEvent) -> Option<WorkerLifecycleEvent> {
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

// ── Tool pair helpers ────────────────────────────────────────────────────────

pub fn pair_tool_events(mut tool_events: Vec<ToolTraceEvent>) -> Vec<ToolTracePair> {
    use std::collections::BTreeMap;
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

// ── Group helpers ────────────────────────────────────────────────────────────

pub fn group_traces(events: &[TraceEvent]) -> Vec<TraceGroup> {
    use std::collections::HashMap;

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
