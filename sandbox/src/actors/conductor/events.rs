//! ConductorActor event emission module
//!
//! Provides typed event emission functions for Conductor task lifecycle.
//! All events are appended to the EventStore for observability and tracing.

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use chrono::Utc;
use ractor::ActorRef;
use shared_types::{ConductorOutputMode, ConductorToastPayload, FailureKind};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

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
