pub mod graph;
pub mod parsers;
pub mod styles;
pub mod trajectory;
pub mod types;
pub mod view;
pub mod ws;

pub use view::TraceView;

#[cfg(test)]
mod tests {
    use super::parsers::pair_tool_events;
    use super::trajectory::{bucket_trajectory_cells, build_trajectory_cells};
    use super::types::{
        ToolTraceEvent, TraceEvent, TraceGroup, TrajectoryCell, TrajectoryStatus,
    };

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
