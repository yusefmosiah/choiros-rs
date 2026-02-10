use shared_types::{EventImportance, EventMetadata, WakePolicy};

use crate::actors::conductor::ConductorMsg;

use super::support::setup_test_conductor;

#[tokio::test]
async fn test_process_event_with_wake_policy_nonexistent_run() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let metadata = EventMetadata {
        wake_policy: WakePolicy::Wake,
        importance: EventImportance::High,
        run_id: Some("run_event_test".to_string()),
        task_id: Some("task_1".to_string()),
        call_id: Some("call_1".to_string()),
        capability: Some("terminal".to_string()),
        phase: Some("completion".to_string()),
    };

    let result = conductor_ref.send_message(ConductorMsg::ProcessEvent {
        run_id: "run_event_test".to_string(),
        event_type: "conductor.capability.completed".to_string(),
        payload: serde_json::json!({
            "call_id": "call_1",
            "summary": "Command completed"
        }),
        metadata,
    });
    assert!(result.is_ok());

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    conductor_ref.stop(None);
    store_ref.stop(None);
}

#[tokio::test]
async fn test_process_event_with_display_policy_nonexistent_run() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let metadata = EventMetadata {
        wake_policy: WakePolicy::DisplayOnly,
        importance: EventImportance::Low,
        run_id: Some("run_display_test".to_string()),
        task_id: Some("task_1".to_string()),
        call_id: None,
        capability: Some("researcher".to_string()),
        phase: Some("progress".to_string()),
    };

    let result = conductor_ref.send_message(ConductorMsg::ProcessEvent {
        run_id: "run_display_test".to_string(),
        event_type: "conductor.progress".to_string(),
        payload: serde_json::json!({
            "message": "Processing...",
            "percent": 50
        }),
        metadata,
    });
    assert!(result.is_ok());

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    conductor_ref.stop(None);
    store_ref.stop(None);
}

#[tokio::test]
async fn test_dispatch_ready_nonexistent_run() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let result = conductor_ref.send_message(ConductorMsg::DispatchReady {
        run_id: "nonexistent_run".to_string(),
    });
    assert!(result.is_ok());

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    conductor_ref.stop(None);
    store_ref.stop(None);
}
