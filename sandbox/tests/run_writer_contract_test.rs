//! RunWriterActor contract tests for Writer-First Cutover
//!
//! Tests for:
//! - Revision monotonicity
//! - Single-writer enforcement (run_id isolation)
//! - Document persistence with atomic rename

use ractor::Actor;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
use sandbox::actors::run_writer::{
    ApplyPatchResult, PatchOp, PatchOpKind, RunWriterActor, RunWriterArguments, RunWriterError,
    RunWriterMsg, SectionState,
};
use std::sync::Arc;
use tempfile::TempDir;

static TEST_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

async fn test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

struct TestContext {
    actor: ractor::ActorRef<RunWriterMsg>,
    temp_dir: Arc<TempDir>,
    original_dir: std::path::PathBuf,
}

async fn spawn_writer(run_id: &str) -> TestContext {
    let temp_dir = Arc::new(tempfile::tempdir().expect("Failed to create temp directory"));
    let original_dir = std::env::current_dir().expect("Failed to get current dir");

    std::env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

    let runs_dir = temp_dir.path().join("conductor").join("runs");
    std::fs::create_dir_all(&runs_dir).expect("Failed to create runs directory");

    let (actor, _handle) = Actor::spawn(
        None,
        RunWriterActor,
        RunWriterArguments {
            run_id: run_id.to_string(),
            desktop_id: "default-desktop".to_string(),
            objective: "test objective".to_string(),
            session_id: "test-session".to_string(),
            thread_id: "test-thread".to_string(),
            root_dir: Some(temp_dir.path().to_string_lossy().to_string()),
            event_store: Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("failed to spawn event store")
                .0,
        },
    )
    .await
    .expect("Failed to spawn RunWriterActor");

    TestContext {
        actor,
        temp_dir,
        original_dir,
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original_dir);
    }
}

async fn get_revision(actor: &ractor::ActorRef<RunWriterMsg>) -> u64 {
    ractor::call!(actor, |reply| RunWriterMsg::GetRevision { reply })
        .expect("Failed to get revision")
}

async fn apply_patch(
    actor: &ractor::ActorRef<RunWriterMsg>,
    run_id: &str,
    section_id: &str,
    text: &str,
    proposal: bool,
) -> Result<ApplyPatchResult, RunWriterError> {
    ractor::call!(actor, |reply| RunWriterMsg::ApplyPatch {
        run_id: run_id.to_string(),
        source: "test".to_string(),
        section_id: section_id.to_string(),
        ops: vec![PatchOp {
            kind: PatchOpKind::Append,
            position: None,
            text: Some(text.to_string()),
        }],
        proposal,
        reply,
    })
    .expect("Actor call failed")
}

#[tokio::test]
async fn test_run_writer_revision_starts_at_zero_for_new_document() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-001").await;
    let revision = get_revision(&ctx.actor).await;
    assert_eq!(revision, 0, "New document should start at revision 0");
}

#[tokio::test]
async fn test_run_writer_revision_increments_monotonically() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-002").await;

    let mut expected_revision = 0u64;

    for i in 1..=5 {
        let result = apply_patch(
            &ctx.actor,
            "test-run-002",
            "conductor",
            &format!("Line {}", i),
            false,
        )
        .await
        .expect("ApplyPatch should succeed");

        expected_revision += 1;
        assert_eq!(
            result.revision, expected_revision,
            "Revision should be {} after {} writes",
            expected_revision, i
        );

        let current_revision = get_revision(&ctx.actor).await;
        assert_eq!(
            current_revision, expected_revision,
            "GetRevision should match ApplyPatch result"
        );
    }
}

#[tokio::test]
async fn test_run_writer_rejects_mismatched_run_id() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-003").await;

    let result = apply_patch(
        &ctx.actor,
        "wrong-run-id",
        "conductor",
        "test content",
        false,
    )
    .await;

    match result {
        Err(RunWriterError::RunIdMismatch { expected, actual }) => {
            assert_eq!(expected, "test-run-003");
            assert_eq!(actual, "wrong-run-id");
        }
        _ => panic!("Expected RunIdMismatch error, got {:?}", result),
    }
}

#[tokio::test]
async fn test_run_writer_proposal_and_commit_increments_revision() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-004").await;

    let revision = ractor::call!(&ctx.actor, |reply| RunWriterMsg::AppendLogLine {
        run_id: "test-run-004".to_string(),
        source: "researcher".to_string(),
        section_id: "researcher".to_string(),
        text: "Proposal text".to_string(),
        proposal: true,
        reply,
    })
    .expect("Failed to call AppendLogLine")
    .expect("AppendLogLine should succeed");
    assert_eq!(revision, 1, "Proposal should increment revision to 1");

    let commit_result = ractor::call!(&ctx.actor, |reply| RunWriterMsg::CommitProposal {
        section_id: "researcher".to_string(),
        reply,
    })
    .expect("Failed to call CommitProposal")
    .expect("CommitProposal should succeed");

    assert_eq!(commit_result, 2, "Commit should increment revision to 2");
}

#[tokio::test]
async fn test_run_writer_append_log_line_increments_revision() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-005").await;

    let revision = ractor::call!(&ctx.actor, |reply| RunWriterMsg::AppendLogLine {
        run_id: "test-run-005".to_string(),
        source: "terminal".to_string(),
        section_id: "terminal".to_string(),
        text: "Executing command".to_string(),
        proposal: false,
        reply,
    })
    .expect("Failed to call AppendLogLine")
    .expect("AppendLogLine should succeed");

    assert_eq!(revision, 1, "AppendLogLine should increment revision to 1");
}

#[tokio::test]
async fn test_run_writer_mark_section_state_increments_revision() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-006").await;

    ractor::call!(&ctx.actor, |reply| RunWriterMsg::MarkSectionState {
        run_id: "test-run-006".to_string(),
        section_id: "researcher".to_string(),
        state: SectionState::Running,
        reply,
    })
    .expect("Failed to call MarkSectionState")
    .expect("MarkSectionState should succeed");

    let revision = get_revision(&ctx.actor).await;
    assert_eq!(
        revision, 1,
        "MarkSectionState should increment revision to 1"
    );
}

#[tokio::test]
async fn test_run_writer_concurrent_patches_preserve_ordering() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-007").await;

    let mut handles = vec![];

    for i in 0..10 {
        let actor_clone = ctx.actor.clone();
        let handle = tokio::spawn(async move {
            ractor::call!(actor_clone, |reply| RunWriterMsg::ApplyPatch {
                run_id: "test-run-007".to_string(),
                source: format!("worker-{}", i),
                section_id: "conductor".to_string(),
                ops: vec![PatchOp {
                    kind: PatchOpKind::Append,
                    position: None,
                    text: Some(format!("Concurrent line {}", i)),
                }],
                proposal: false,
                reply,
            })
            .expect("Failed to call ApplyPatch")
            .expect("ApplyPatch failed")
        });
        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    let revisions: Vec<u64> = results
        .into_iter()
        .map(|r| r.expect("Task panicked").revision)
        .collect();

    let final_revision = get_revision(&ctx.actor).await;
    assert_eq!(
        final_revision, 10,
        "Should have 10 revisions after 10 writes"
    );

    assert!(
        revisions.iter().all(|&r| r >= 1 && r <= 10),
        "All revisions should be between 1 and 10"
    );
}

#[tokio::test]
async fn test_run_writer_discard_proposal_increments_revision() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-008").await;

    let result = apply_patch(
        &ctx.actor,
        "test-run-008",
        "researcher",
        "Proposal to discard",
        true,
    )
    .await
    .expect("Proposal patch should succeed");
    assert_eq!(result.revision, 1);

    ractor::call!(&ctx.actor, |reply| RunWriterMsg::DiscardProposal {
        section_id: "researcher".to_string(),
        reply,
    })
    .expect("Failed to call DiscardProposal")
    .expect("DiscardProposal should succeed");

    let revision = get_revision(&ctx.actor).await;
    assert_eq!(
        revision, 2,
        "DiscardProposal should increment revision because it persists document state"
    );
}

#[tokio::test]
async fn test_run_writer_get_document_returns_current_state() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-009").await;

    apply_patch(
        &ctx.actor,
        "test-run-009",
        "conductor",
        "Test content",
        false,
    )
    .await
    .expect("ApplyPatch should succeed");

    let doc = ractor::call!(&ctx.actor, |reply| RunWriterMsg::GetDocument { reply })
        .expect("Failed to call GetDocument")
        .expect("GetDocument should succeed");

    assert!(
        doc.contains("Test content"),
        "Document should contain written content"
    );
    assert!(doc.contains("# "), "Document should have objective header");
}

#[tokio::test]
async fn test_run_writer_report_section_progress_does_not_mutate_revision_or_document() {
    let _guard = test_guard().await;
    let ctx = spawn_writer("test-run-010").await;

    apply_patch(
        &ctx.actor,
        "test-run-010",
        "conductor",
        "Stable baseline content",
        false,
    )
    .await
    .expect("ApplyPatch should succeed");

    let before_revision = get_revision(&ctx.actor).await;
    let before_doc = ractor::call!(&ctx.actor, |reply| RunWriterMsg::GetDocument { reply })
        .expect("Failed to call GetDocument")
        .expect("GetDocument should succeed");

    let reported_revision =
        ractor::call!(&ctx.actor, |reply| RunWriterMsg::ReportSectionProgress {
            run_id: "test-run-010".to_string(),
            source: "terminal".to_string(),
            section_id: "terminal".to_string(),
            phase: "executing_tool".to_string(),
            message: "Running concise status tick".to_string(),
            reply,
        })
        .expect("Failed to call ReportSectionProgress")
        .expect("ReportSectionProgress should succeed");

    let after_revision = get_revision(&ctx.actor).await;
    let after_doc = ractor::call!(&ctx.actor, |reply| RunWriterMsg::GetDocument { reply })
        .expect("Failed to call GetDocument")
        .expect("GetDocument should succeed");

    assert_eq!(
        reported_revision, before_revision,
        "ReportSectionProgress should return the current revision"
    );
    assert_eq!(
        after_revision, before_revision,
        "ReportSectionProgress should not increment revision"
    );
    assert_eq!(
        after_doc, before_doc,
        "ReportSectionProgress should not mutate document content"
    );
}
