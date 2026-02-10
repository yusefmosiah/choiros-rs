//! ConductorActor event emission module
//!
//! Provides typed event emission functions for Conductor task lifecycle.
//! All events are appended to the EventStore for observability and tracing.
//!
//! Phase B: Wake/Display Lane Separation
//! - Wake lane events trigger conductor decision-making
//! - Display-only lane events are for UI telemetry only

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use chrono::Utc;
use ractor::ActorRef;
use shared_types::{
    ConductorOutputMode, ConductorToastPayload, EventImportance, EventMetadata, FailureKind,
    WakePolicy,
};

/// Emit task started event
pub async fn emit_task_started(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    objective: &str,
    desktop_id: &str,
) {
    let payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "objective": objective,
        "desktop_id": desktop_id,
        "status": "started",
        "phase": "initialization",
        "timestamp": Utc::now().to_rfc3339(),
    });

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_TASK_STARTED.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit task progress event
pub async fn emit_task_progress(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    status: &str,
    phase: &str,
    details: Option<serde_json::Value>,
) {
    let mut payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "status": status,
        "phase": phase,
        "timestamp": Utc::now().to_rfc3339(),
    });

    if let Some(details) = details {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("details".to_string(), details);
        }
    }

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_TASK_PROGRESS.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit worker call event
pub async fn emit_worker_call(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    worker_type: &str,
    worker_objective: &str,
) {
    let payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "worker_type": worker_type,
        "worker_objective": worker_objective,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_WORKER_CALL.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit worker result event
pub async fn emit_worker_result(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    worker_type: &str,
    success: bool,
    result_summary: &str,
) {
    let payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "worker_type": worker_type,
        "success": success,
        "result_summary": result_summary,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_WORKER_RESULT.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit task completed event
pub async fn emit_task_completed(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    output_mode: ConductorOutputMode,
    report_path: &str,
    writer_props: Option<&serde_json::Value>,
    toast: Option<&ConductorToastPayload>,
) {
    let mut payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "output_mode": output_mode,
        "report_path": report_path,
        "status": "completed",
        "timestamp": Utc::now().to_rfc3339(),
    });
    if let Some(obj) = payload.as_object_mut() {
        if let Some(props) = writer_props {
            obj.insert("writer_window_props".to_string(), props.clone());
        }
        if let Some(toast) = toast {
            obj.insert("toast".to_string(), serde_json::json!(toast));
        }
    }

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_TASK_COMPLETED.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit task failed event
pub async fn emit_task_failed(
    event_store: &ActorRef<EventStoreMsg>,
    task_id: &str,
    correlation_id: &str,
    error_code: &str,
    error_message: &str,
    failure_kind: Option<FailureKind>,
) {
    let mut payload = serde_json::json!({
        "task_id": task_id,
        "correlation_id": correlation_id,
        "error_code": error_code,
        "error_message": error_message,
        "status": "failed",
        "timestamp": Utc::now().to_rfc3339(),
    });

    if let Some(failure_kind) = failure_kind {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("failure_kind".to_string(), serde_json::json!(failure_kind));
        }
    }

    let event = AppendEvent {
        event_type: shared_types::EVENT_TOPIC_CONDUCTOR_TASK_FAILED.to_string(),
        payload,
        actor_id: format!("conductor:{}", task_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

// ============================================================================
// Phase B: Wake/Display Lane Event Emission
// ============================================================================

/// Event payload with metadata for wake/display lane separation
#[derive(Debug, Clone)]
pub struct EventWithMetadata {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor_id: String,
    pub metadata: EventMetadata,
}

/// Emit a wake lane event (triggers conductor decision-making)
pub async fn emit_wake_event(
    event_store: &ActorRef<EventStoreMsg>,
    event_type: &str,
    run_id: &str,
    task_id: &str,
    capability: &str,
    phase: &str,
    payload: serde_json::Value,
) {
    let full_payload = serde_json::json!({
        "run_id": run_id,
        "task_id": task_id,
        "capability": capability,
        "phase": phase,
        "data": payload,
        "timestamp": Utc::now().to_rfc3339(),
        "_meta": {
            "wake_policy": "wake",
            "importance": "high",
        }
    });

    let event = AppendEvent {
        event_type: event_type.to_string(),
        payload: full_payload,
        actor_id: format!("conductor:{}", run_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

/// Emit a display-only lane event (UI telemetry only, no wake)
pub async fn emit_display_event(
    event_store: &ActorRef<EventStoreMsg>,
    event_type: &str,
    run_id: &str,
    task_id: &str,
    capability: &str,
    phase: &str,
    importance: EventImportance,
    payload: serde_json::Value,
) {
    let importance_str = match importance {
        EventImportance::Low => "low",
        EventImportance::Normal => "normal",
        EventImportance::High => "high",
    };

    let full_payload = serde_json::json!({
        "run_id": run_id,
        "task_id": task_id,
        "capability": capability,
        "phase": phase,
        "data": payload,
        "timestamp": Utc::now().to_rfc3339(),
        "_meta": {
            "wake_policy": "display_only",
            "importance": importance_str,
        }
    });

    let event = AppendEvent {
        event_type: event_type.to_string(),
        payload: full_payload,
        actor_id: format!("conductor:{}", run_id),
        user_id: "system".to_string(),
    };

    let _ = event_store
        .send_message(EventStoreMsg::AppendAsync { event })
        .ok();
}

// Wake Lane Events (trigger conductor wake)

/// Emit capability completion event (wake lane)
pub async fn emit_capability_completed(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    call_id: &str,
    capability: &str,
    summary: &str,
) {
    emit_wake_event(
        event_store,
        "conductor.capability.completed",
        run_id,
        task_id,
        capability,
        "completion",
        serde_json::json!({
            "call_id": call_id,
            "summary": summary,
        }),
    )
    .await;
}

/// Emit capability failed event (wake lane)
pub async fn emit_capability_failed(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    call_id: &str,
    capability: &str,
    error: &str,
    failure_kind: Option<FailureKind>,
) {
    emit_wake_event(
        event_store,
        "conductor.capability.failed",
        run_id,
        task_id,
        capability,
        "failure",
        serde_json::json!({
            "call_id": call_id,
            "error": error,
            "failure_kind": failure_kind,
        }),
    )
    .await;
}

/// Emit capability blocked event (wake lane)
pub async fn emit_capability_blocked(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    call_id: &str,
    capability: &str,
    reason: &str,
) {
    emit_wake_event(
        event_store,
        "conductor.capability.blocked",
        run_id,
        task_id,
        capability,
        "blocked",
        serde_json::json!({
            "call_id": call_id,
            "reason": reason,
        }),
    )
    .await;
}

/// Emit escalation event (wake lane)
pub async fn emit_escalation(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    escalation_id: &str,
    kind: &str,
    reason: &str,
    urgency: &str,
) {
    emit_wake_event(
        event_store,
        "conductor.escalation",
        run_id,
        task_id,
        "conductor",
        "escalation",
        serde_json::json!({
            "escalation_id": escalation_id,
            "kind": kind,
            "reason": reason,
            "urgency": urgency,
        }),
    )
    .await;
}

// Display-Only Lane Events (UI telemetry, no wake)

/// Emit finding event (display-only lane)
pub async fn emit_finding(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    capability: &str,
    finding_id: &str,
    claim: &str,
    confidence: f64,
) {
    emit_display_event(
        event_store,
        "conductor.finding",
        run_id,
        task_id,
        capability,
        "finding",
        EventImportance::Normal,
        serde_json::json!({
            "finding_id": finding_id,
            "claim": claim,
            "confidence": confidence,
        }),
    )
    .await;
}

/// Emit learning event (display-only lane)
pub async fn emit_learning(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    capability: &str,
    learning_id: &str,
    insight: &str,
) {
    emit_display_event(
        event_store,
        "conductor.learning",
        run_id,
        task_id,
        capability,
        "learning",
        EventImportance::Normal,
        serde_json::json!({
            "learning_id": learning_id,
            "insight": insight,
        }),
    )
    .await;
}

/// Emit tool call event (display-only lane)
pub async fn emit_tool_call(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    capability: &str,
    tool: &str,
    args: serde_json::Value,
) {
    emit_display_event(
        event_store,
        "conductor.tool.call",
        run_id,
        task_id,
        capability,
        "tool_call",
        EventImportance::Low,
        serde_json::json!({
            "tool": tool,
            "args": args,
        }),
    )
    .await;
}

/// Emit tool result event (display-only lane)
pub async fn emit_tool_result(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    capability: &str,
    tool: &str,
    success: bool,
    result_summary: &str,
) {
    emit_display_event(
        event_store,
        "conductor.tool.result",
        run_id,
        task_id,
        capability,
        "tool_result",
        EventImportance::Low,
        serde_json::json!({
            "tool": tool,
            "success": success,
            "summary": result_summary,
        }),
    )
    .await;
}

/// Emit progress event (display-only lane)
pub async fn emit_progress(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    capability: &str,
    message: &str,
    percent: Option<u8>,
) {
    emit_display_event(
        event_store,
        "conductor.progress",
        run_id,
        task_id,
        capability,
        "progress",
        EventImportance::Low,
        serde_json::json!({
            "message": message,
            "percent": percent,
        }),
    )
    .await;
}

/// Emit decision event (wake lane - decisions trigger next steps)
pub async fn emit_decision(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    decision_id: &str,
    decision_type: &str,
    reason: &str,
) {
    emit_wake_event(
        event_store,
        "conductor.decision",
        run_id,
        task_id,
        "conductor",
        "decision",
        serde_json::json!({
            "decision_id": decision_id,
            "decision_type": decision_type,
            "reason": reason,
        }),
    )
    .await;
}

/// Parse event metadata from payload
pub fn parse_event_metadata(payload: &serde_json::Value) -> EventMetadata {
    if let Some(meta) = payload.get("_meta") {
        let wake_policy = meta
            .get("wake_policy")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "wake" => WakePolicy::Wake,
                _ => WakePolicy::DisplayOnly,
            })
            .unwrap_or(WakePolicy::DisplayOnly);

        let importance = meta
            .get("importance")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "high" => EventImportance::High,
                "low" => EventImportance::Low,
                _ => EventImportance::Normal,
            })
            .unwrap_or(EventImportance::Normal);

        // Extract call_id from nested data structure (e.g., data.call_id from wake events)
        let call_id = payload
            .get("data")
            .and_then(|d| d.get("call_id"))
            .and_then(|v| v.as_str())
            .map(String::from);

        EventMetadata {
            wake_policy,
            importance,
            run_id: payload
                .get("run_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            task_id: payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            call_id,
            capability: payload
                .get("capability")
                .and_then(|v| v.as_str())
                .map(String::from),
            phase: payload
                .get("phase")
                .and_then(|v| v.as_str())
                .map(String::from),
        }
    } else {
        EventMetadata::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    // ============================================================================
    // parse_event_metadata tests
    // ============================================================================

    #[test]
    fn test_parse_event_metadata_wake_policy_wake() {
        let payload = serde_json::json!({
            "run_id": "run_123",
            "task_id": "task_456",
            "capability": "terminal",
            "phase": "execution",
            "_meta": {
                "wake_policy": "wake",
                "importance": "high"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert!(matches!(metadata.wake_policy, WakePolicy::Wake));
        assert!(matches!(metadata.importance, EventImportance::High));
        assert_eq!(metadata.run_id, Some("run_123".to_string()));
        assert_eq!(metadata.task_id, Some("task_456".to_string()));
        assert_eq!(metadata.capability, Some("terminal".to_string()));
        assert_eq!(metadata.phase, Some("execution".to_string()));
    }

    #[test]
    fn test_parse_event_metadata_wake_policy_display_only() {
        let payload = serde_json::json!({
            "run_id": "run_789",
            "task_id": "task_abc",
            "capability": "researcher",
            "phase": "finding",
            "_meta": {
                "wake_policy": "display_only",
                "importance": "normal"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert!(matches!(metadata.wake_policy, WakePolicy::DisplayOnly));
        assert!(matches!(metadata.importance, EventImportance::Normal));
    }

    #[test]
    fn test_parse_event_metadata_importance_low() {
        let payload = serde_json::json!({
            "_meta": {
                "wake_policy": "display_only",
                "importance": "low"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert!(matches!(metadata.importance, EventImportance::Low));
    }

    #[test]
    fn test_parse_event_metadata_default_when_no_meta() {
        let payload = serde_json::json!({
            "run_id": "run_123",
            "data": {
                "message": "test"
            }
        });

        let metadata = parse_event_metadata(&payload);

        // When no _meta present, returns default() which has all None values
        // and DisplayOnly/Normal for the enums
        assert!(matches!(metadata.wake_policy, WakePolicy::DisplayOnly));
        assert!(matches!(metadata.importance, EventImportance::Normal));
        assert_eq!(metadata.run_id, None); // Default has None for run_id
        assert_eq!(metadata.call_id, None);
    }

    #[test]
    fn test_parse_event_metadata_extracts_call_id_from_data() {
        let payload = serde_json::json!({
            "run_id": "run_123",
            "task_id": "task_456",
            "capability": "terminal",
            "phase": "completion",
            "data": {
                "call_id": "call_789",
                "summary": "Task completed successfully"
            },
            "_meta": {
                "wake_policy": "wake",
                "importance": "high"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert_eq!(metadata.call_id, Some("call_789".to_string()));
    }

    #[test]
    fn test_parse_event_metadata_call_id_none_when_missing() {
        let payload = serde_json::json!({
            "run_id": "run_123",
            "data": {
                "summary": "No call_id here"
            },
            "_meta": {
                "wake_policy": "wake"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert_eq!(metadata.call_id, None);
    }

    #[test]
    fn test_parse_event_metadata_invalid_wake_policy_defaults_to_display() {
        let payload = serde_json::json!({
            "_meta": {
                "wake_policy": "invalid_value"
            }
        });

        let metadata = parse_event_metadata(&payload);

        // Invalid wake_policy should default to DisplayOnly
        assert!(matches!(metadata.wake_policy, WakePolicy::DisplayOnly));
    }

    #[test]
    fn test_parse_event_metadata_invalid_importance_defaults_to_normal() {
        let payload = serde_json::json!({
            "_meta": {
                "importance": "unknown_level"
            }
        });

        let metadata = parse_event_metadata(&payload);

        // Invalid importance should default to Normal
        assert!(matches!(metadata.importance, EventImportance::Normal));
    }

    #[test]
    fn test_parse_event_metadata_empty_payload() {
        let payload = serde_json::json!({});

        let metadata = parse_event_metadata(&payload);

        assert!(matches!(metadata.wake_policy, WakePolicy::DisplayOnly));
        assert!(matches!(metadata.importance, EventImportance::Normal));
        assert_eq!(metadata.run_id, None);
        assert_eq!(metadata.task_id, None);
        assert_eq!(metadata.call_id, None);
        assert_eq!(metadata.capability, None);
        assert_eq!(metadata.phase, None);
    }

    #[test]
    fn test_parse_event_metadata_wake_event_with_call_id() {
        // Test a realistic wake event from capability completion
        let payload = serde_json::json!({
            "run_id": "run_abc",
            "task_id": "task_def",
            "capability": "terminal",
            "phase": "completion",
            "data": {
                "call_id": "call_xyz",
                "summary": "Command executed"
            },
            "timestamp": "2026-02-10T12:00:00Z",
            "_meta": {
                "wake_policy": "wake",
                "importance": "high"
            }
        });

        let metadata = parse_event_metadata(&payload);

        assert!(matches!(metadata.wake_policy, WakePolicy::Wake));
        assert!(matches!(metadata.importance, EventImportance::High));
        assert_eq!(metadata.run_id, Some("run_abc".to_string()));
        assert_eq!(metadata.task_id, Some("task_def".to_string()));
        assert_eq!(metadata.call_id, Some("call_xyz".to_string()));
        assert_eq!(metadata.capability, Some("terminal".to_string()));
        assert_eq!(metadata.phase, Some("completion".to_string()));
    }

    #[tokio::test]
    async fn test_emit_task_started() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_task_started(
            &store_ref,
            "task-123",
            "corr-456",
            "Test objective",
            "desktop-789",
        )
        .await;

        // Give async event time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_emit_task_progress() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_task_progress(
            &store_ref,
            "task-123",
            "corr-456",
            "running",
            "research",
            Some(serde_json::json!({"progress": 50})),
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_emit_worker_call() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_worker_call(
            &store_ref,
            "task-123",
            "corr-456",
            "researcher",
            "Research AI capabilities",
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_emit_worker_result() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_worker_result(
            &store_ref,
            "task-123",
            "corr-456",
            "researcher",
            true,
            "Found 5 relevant sources",
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_emit_task_completed() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_task_completed(
            &store_ref,
            "task-123",
            "corr-456",
            shared_types::ConductorOutputMode::MarkdownReportToWriter,
            "/reports/task-123.md",
            Some(&serde_json::json!({
                "x": 100,
                "y": 200,
                "width": 800,
                "height": 600
            })),
            None,
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_emit_task_failed() {
        let (store_ref, _handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        emit_task_failed(
            &store_ref,
            "task-123",
            "corr-456",
            "WORKER_FAILED",
            "Worker timed out after 30s",
            Some(FailureKind::Timeout),
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        store_ref.stop(None);
    }
}
