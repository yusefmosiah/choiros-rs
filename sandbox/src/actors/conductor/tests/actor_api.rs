use ractor::call;
use shared_types::{ConductorExecuteRequest, ConductorOutputMode, ConductorTaskState};

use crate::actors::conductor::{ConductorError, ConductorMsg};

use super::support::setup_test_conductor;

#[tokio::test]
async fn test_conductor_actor_spawn() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;
    assert!(!conductor_ref.get_id().to_string().is_empty());
    conductor_ref.stop(None);
    store_ref.stop(None);
}

#[tokio::test]
async fn test_get_task_state_nonexistent() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let state_result: Result<Option<ConductorTaskState>, _> =
        call!(conductor_ref, |reply| ConductorMsg::GetTaskState {
            task_id: "non-existent-task-id".to_string(),
            reply,
        });

    assert!(state_result.is_ok());
    assert!(state_result.unwrap().is_none());

    conductor_ref.stop(None);
    store_ref.stop(None);
}

#[tokio::test]
async fn test_execute_task_message_missing_workers() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let request = ConductorExecuteRequest {
        objective: "Research Rust async patterns".to_string(),
        desktop_id: "test-desktop-001".to_string(),
        output_mode: ConductorOutputMode::MarkdownReportToWriter,
        worker_plan: None,
        hints: None,
        correlation_id: Some("test-correlation-001".to_string()),
    };

    let result: Result<Result<ConductorTaskState, ConductorError>, _> =
        call!(conductor_ref, |reply| ConductorMsg::ExecuteTask {
            request,
            reply,
        });

    assert!(result.is_ok());
    match result.unwrap().unwrap_err() {
        ConductorError::ActorUnavailable(msg) => {
            assert!(msg.contains("No worker actors available"));
        }
        other => panic!("Expected ActorUnavailable, got {:?}", other),
    }

    conductor_ref.stop(None);
    store_ref.stop(None);
}
