use ractor::call;
use shared_types::{ConductorExecuteRequest, ConductorOutputMode, ConductorRunState};

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
async fn test_get_run_state_nonexistent() {
    let (conductor_ref, store_ref) = setup_test_conductor(None, None).await;

    let state_result: Result<Option<ConductorRunState>, _> =
        call!(conductor_ref, |reply| ConductorMsg::GetRunState {
            run_id: "non-existent-run-id".to_string(),
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
        hints: None,
    };

    let result: Result<Result<ConductorRunState, ConductorError>, _> =
        call!(conductor_ref, |reply| ConductorMsg::ExecuteTask {
            request,
            reply,
        });

    assert!(result.is_ok());
    match result.unwrap().unwrap_err() {
        ConductorError::ActorUnavailable(msg) => {
            assert!(
                msg.contains("writer actor unavailable")
                    || msg.contains("No app-agent capabilities available"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("Expected ActorUnavailable, got {:?}", other),
    }

    conductor_ref.stop(None);
    store_ref.stop(None);
}
