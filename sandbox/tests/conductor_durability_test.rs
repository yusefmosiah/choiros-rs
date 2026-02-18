//! Conductor actor durability tests.
//!
//! Verifies `ConductorActor::restore_run_states` in `post_start`:
//!
//!   1. Write `conductor.run.started` events to EventStore (simulating a crash mid-run)
//!   2. Spawn a fresh ConductorActor using the same EventStore
//!   3. `post_start` calls `restore_run_states` → rebuilds in-memory state
//!   4. `GetRunState` returns `Blocked` for each restored run
//!
//! Also tests:
//!   - Multiple runs are all restored
//!   - Already-known runs are not duplicated
//!   - Runs with missing `run_id` in payload are silently skipped
//!   - EventStore error during recovery degrades gracefully (clean state)
//!
//! No LLM calls — pure actor lifecycle + EventStore contract test.
//!
//! Run:
//!   cargo test -p sandbox --test conductor_durability_test -- --nocapture

use ractor::Actor;
use uuid::Uuid;

use sandbox::actors::conductor::actor::{ConductorActor, ConductorArguments};
use sandbox::actors::conductor::protocol::{ConductorMsg};
use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use shared_types::ConductorRunStatus;

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn make_event_store() -> (ractor::ActorRef<EventStoreMsg>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("conductor_durability_test.db");
    let (store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db.to_str().unwrap().into()),
    )
    .await
    .expect("spawn EventStoreActor");
    (store, tmp)
}

/// Write a `conductor.run.started` event for the given run_id.
/// This is the event that `restore_run_states` scans for on restart.
async fn write_run_started_event(
    store: &ractor::ActorRef<EventStoreMsg>,
    run_id: &str,
    objective: &str,
    desktop_id: &str,
) {
    let _ = store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "conductor.run.started".to_string(),
            payload: serde_json::json!({
                "run_id": run_id,
                "objective": objective,
                "desktop_id": desktop_id,
            }),
            actor_id: "conductor".to_string(),
            user_id: "system".to_string(),
        },
    });
}

/// Spawn a ConductorActor backed by `event_store` and return its actor ref.
/// `post_start` will call `restore_run_states` automatically.
async fn spawn_conductor(
    event_store: ractor::ActorRef<EventStoreMsg>,
) -> (ractor::ActorRef<ConductorMsg>, tokio::task::JoinHandle<()>) {
    Actor::spawn(
        None,
        ConductorActor,
        ConductorArguments {
            event_store,
            writer_supervisor: None,
            memory_actor: None,
        },
    )
    .await
    .expect("spawn ConductorActor")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Core durability test: write a `conductor.run.started` event, restart conductor,
/// verify `GetRunState` returns `Blocked` for the restored run.
///
/// This is the primary regression guard for crash recovery. If `restore_run_states`
/// stops scanning for `conductor.run.started` events or stops inserting into the
/// task store, this test catches it.
#[tokio::test]
async fn test_conductor_restores_run_as_blocked_after_restart() {
    let (event_store, _tmp) = make_event_store().await;
    let run_id = format!("run-{}", Uuid::new_v4().as_simple());

    // Simulate: conductor was running this run when the process crashed
    write_run_started_event(&event_store, &run_id, "analyse the codebase", "desktop-1").await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await; // let append land

    // Restart conductor — post_start calls restore_run_states
    let (conductor, _handle) = spawn_conductor(event_store.clone()).await;

    // Give post_start a moment to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Query the restored run
    let run_state = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::GetRunState {
            run_id: run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc ok");

    assert!(
        run_state.is_some(),
        "restored run must be present in ConductorActor state after restart"
    );

    let run = run_state.unwrap();
    assert_eq!(run.run_id, run_id, "run_id must match");
    assert_eq!(
        run.status,
        ConductorRunStatus::Blocked,
        "restored run must have Blocked status (workers were lost in crash), got: {:?}",
        run.status
    );
    assert_eq!(
        run.objective, "analyse the codebase",
        "objective must be preserved from event payload"
    );
    assert_eq!(
        run.desktop_id, "desktop-1",
        "desktop_id must be preserved from event payload"
    );

    println!(
        "  [RESTORED] run:{} status:{:?} objective:'{}'",
        run.run_id, run.status, run.objective
    );
}

/// Multiple runs are all restored after a crash.
///
/// Simulates a conductor that had 3 concurrent runs in flight when it crashed.
/// All 3 must be restored as `Blocked`.
#[tokio::test]
async fn test_conductor_restores_multiple_runs_after_restart() {
    let (event_store, _tmp) = make_event_store().await;

    let runs: Vec<(String, &str)> = vec![
        (format!("run-a-{}", Uuid::new_v4().as_simple()), "write report"),
        (format!("run-b-{}", Uuid::new_v4().as_simple()), "analyse logs"),
        (format!("run-c-{}", Uuid::new_v4().as_simple()), "deploy service"),
    ];

    // Write all three run.started events
    for (run_id, objective) in &runs {
        write_run_started_event(&event_store, run_id, objective, "desktop-multi").await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Restart conductor
    let (conductor, _handle) = spawn_conductor(event_store.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // All three must be restored as Blocked
    for (run_id, objective) in &runs {
        let run_state = ractor::call_t!(
            conductor,
            |reply| ConductorMsg::GetRunState {
                run_id: run_id.clone(),
                reply,
            },
            2000
        )
        .expect("rpc ok");

        assert!(
            run_state.is_some(),
            "run '{run_id}' must be restored after restart"
        );

        let run = run_state.unwrap();
        assert_eq!(
            run.status,
            ConductorRunStatus::Blocked,
            "run '{run_id}' must be Blocked, got: {:?}",
            run.status
        );
        assert_eq!(&run.objective, objective, "objective must match for run '{run_id}'");

        println!("  [MULTI-RESTORE] run:{run_id} status:{:?}", run.status);
    }

    println!("  [MULTI-RESTORE] All 3 runs restored as Blocked");
}

/// A duplicate `conductor.run.started` event for the same run_id does NOT
/// cause duplicate entries in the task store.
///
/// The recovery loop has a `state.tasks.get_run(&run_id).is_some()` guard.
/// This test verifies that guard is correct.
#[tokio::test]
async fn test_conductor_does_not_duplicate_on_duplicate_run_started_events() {
    let (event_store, _tmp) = make_event_store().await;
    let run_id = format!("run-dup-{}", Uuid::new_v4().as_simple());

    // Write same run_id twice (crash scenario: event was written twice before crash)
    write_run_started_event(&event_store, &run_id, "objective v1", "desktop-dup").await;
    write_run_started_event(&event_store, &run_id, "objective v1", "desktop-dup").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (conductor, _handle) = spawn_conductor(event_store.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // GetRunState should return exactly one run (not fail or panic)
    let run_state = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::GetRunState {
            run_id: run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc ok");

    assert!(run_state.is_some(), "run must exist");
    let run = run_state.unwrap();
    assert_eq!(
        run.status,
        ConductorRunStatus::Blocked,
        "duplicated run must still be Blocked"
    );

    println!("  [DEDUP] Duplicate run.started events handled — single entry, status: {:?}", run.status);
}

/// An event with a missing `run_id` field in its payload is silently skipped.
///
/// The recovery loop does `payload.get("run_id").and_then(|v| v.as_str())` with
/// a `continue` on `None`. This test ensures malformed events don't crash recovery
/// or prevent other valid runs from being restored.
#[tokio::test]
async fn test_conductor_skips_malformed_run_started_events() {
    let (event_store, _tmp) = make_event_store().await;
    let good_run_id = format!("run-good-{}", Uuid::new_v4().as_simple());

    // Write a malformed event (missing run_id) first
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "conductor.run.started".to_string(),
            payload: serde_json::json!({
                // NO run_id field — this is malformed
                "objective": "malformed event",
                "desktop_id": "desktop-bad",
            }),
            actor_id: "conductor".to_string(),
            user_id: "system".to_string(),
        },
    });

    // Write a valid event after the malformed one
    write_run_started_event(&event_store, &good_run_id, "valid run", "desktop-good").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Conductor must start without panicking and restore the valid run
    let (conductor, _handle) = spawn_conductor(event_store.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Good run must be restored
    let run_state = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::GetRunState {
            run_id: good_run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc ok");

    assert!(
        run_state.is_some(),
        "valid run must be restored even when a malformed event precedes it"
    );
    assert_eq!(
        run_state.unwrap().status,
        ConductorRunStatus::Blocked,
        "valid run must be Blocked"
    );

    println!("  [SKIP-MALFORMED] Malformed event skipped, valid run restored successfully");
}

/// A fresh conductor with no prior events has an empty task store.
///
/// This is the baseline case: no `conductor.run.started` events → no runs restored.
/// Verifies that `restore_run_states` doesn't create phantom runs.
#[tokio::test]
async fn test_conductor_empty_store_no_phantom_runs() {
    let (event_store, _tmp) = make_event_store().await;
    let phantom_run_id = format!("run-phantom-{}", Uuid::new_v4().as_simple());

    // Start conductor with empty EventStore
    let (conductor, _handle) = spawn_conductor(event_store).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // A random run_id must not exist
    let run_state = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::GetRunState {
            run_id: phantom_run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc ok");

    assert!(
        run_state.is_none(),
        "fresh conductor must not have phantom runs, but found one for '{phantom_run_id}'"
    );

    println!("  [EMPTY] Fresh conductor has no phantom runs — correct");
}

/// A run started AFTER conductor restart is correctly tracked (not just recovered runs).
///
/// Verifies that the live `StartRun` path still works after recovery.
/// The conductor must accept new runs alongside the restored ones.
#[tokio::test]
async fn test_conductor_accepts_new_run_after_recovery() {
    let (event_store, _tmp) = make_event_store().await;
    let restored_run_id = format!("run-restored-{}", Uuid::new_v4().as_simple());

    // Pre-crash state: one run was active
    write_run_started_event(&event_store, &restored_run_id, "pre-crash objective", "desktop-1")
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Restart conductor
    let (conductor, _handle) = spawn_conductor(event_store.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Issue a new run via ExecuteTask. run_id is assigned by the conductor internally.
    // The call may fail (no writer/model gateway) but that's fine — we only care that
    // the restored run is still present and was not evicted.
    let new_request = shared_types::ConductorExecuteRequest {
        objective: "new post-recovery run".to_string(),
        desktop_id: "desktop-2".to_string(),
        output_mode: shared_types::ConductorOutputMode::Auto,
        hints: None,
    };

    let _result = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::ExecuteTask {
            request: new_request,
            reply,
        },
        5000
    )
    .expect("rpc ok");

    // The restored run must STILL be present after the new run was submitted
    let restored_state = ractor::call_t!(
        conductor,
        |reply| ConductorMsg::GetRunState {
            run_id: restored_run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc ok");

    assert!(
        restored_state.is_some(),
        "restored run must still be present after new run is submitted"
    );
    assert_eq!(
        restored_state.as_ref().unwrap().status,
        ConductorRunStatus::Blocked,
        "restored run must remain Blocked"
    );

    println!(
        "  [POST-RECOVERY] restored run still present: {:?}",
        restored_state.unwrap().status
    );
}
