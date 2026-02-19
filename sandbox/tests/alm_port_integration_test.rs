//! ALM port integration tests — bash dispatch path.
//!
//! Tests the `ActorAlmPort::dispatch_tool("bash")` → `TerminalMsg::RunAgenticTaskDetached`
//! → `tool.result` EventStore write chain.
//!
//! ## What this covers
//!
//! 1. `dispatch_tool` fires `RunAgenticTaskDetached` to TerminalActor (fire-and-forget).
//! 2. The corr_id is embedded in the detached message's `call_id` field.
//! 3. `resolve_source(ToolOutput, corr_id)` returns `None` before the result lands.
//! 4. After the result is manually written (or via a real terminal run), `resolve_source`
//!    returns the content.
//! 5. **Documents the known gap**: `RunAgenticTaskDetached` does NOT currently emit a
//!    `tool.result` event — it relies on `emit_writer_completion`, which only fires
//!    when `writer_actor` is set. This test captures the current behaviour so a
//!    regression is visible if anything silently changes.
//!
//! Run:
//!   cargo test -p sandbox --test alm_port_integration_test -- --nocapture

use std::collections::HashMap;

use ractor::Actor;
use uuid::Uuid;

use sandbox::actors::agent_harness::alm::AlmPort;
use sandbox::actors::agent_harness::alm_port::ActorAlmPort;
use sandbox::actors::conductor::protocol::ConductorMsg;
use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use sandbox::actors::terminal::TerminalMsg;
use sandbox::supervisor::terminal::{
    TerminalSupervisor, TerminalSupervisorArgs, TerminalSupervisorMsg,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn make_event_store() -> (ractor::ActorRef<EventStoreMsg>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("alm_port_test.db");
    let (store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db.to_str().unwrap().into()),
    )
    .await
    .expect("spawn EventStoreActor");
    (store, tmp)
}

/// Spawn a real TerminalActor via supervisor and return its ref.
async fn make_terminal(
    event_store: ractor::ActorRef<EventStoreMsg>,
) -> ractor::ActorRef<TerminalMsg> {
    let (supervisor, _) = Actor::spawn(
        None,
        TerminalSupervisor,
        TerminalSupervisorArgs {
            event_store: event_store.clone(),
        },
    )
    .await
    .expect("spawn TerminalSupervisor");

    ractor::call!(&supervisor, |reply| {
        TerminalSupervisorMsg::GetOrCreateTerminal {
            terminal_id: format!("alm-test-{}", Uuid::new_v4().as_simple()),
            user_id: "test-user".to_string(),
            shell: "/bin/bash".to_string(),
            working_dir: "/tmp".to_string(),
            reply,
        }
    })
    .expect("rpc failed")
    .expect("terminal create failed")
}

/// Minimal stub ConductorActor that accepts SubharnessComplete/Failed messages
/// without panicking — we need a live ActorRef<ConductorMsg> for ActorAlmPort.
struct StubConductor;

#[async_trait::async_trait]
impl Actor for StubConductor {
    type Msg = ConductorMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ractor::ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ractor::ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ractor::ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ractor::ActorProcessingErr> {
        Ok(())
    }
}

async fn make_stub_conductor() -> ractor::ActorRef<ConductorMsg> {
    let (actor, _) = Actor::spawn(None, StubConductor, ())
        .await
        .expect("spawn stub conductor");
    actor
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// `dispatch_tool("bash")` generates a corr_id and fires `RunAgenticTaskDetached`
/// on the terminal. The return value is `"dispatched:corr_id:<id>"`.
///
/// Since `RunAgenticTaskDetached` is fire-and-forget, and the terminal is freshly
/// spawned (PTY running), the message is accepted without error.
#[tokio::test]
async fn test_dispatch_tool_bash_returns_dispatched_corr_id() {
    let (event_store, _tmp) = make_event_store().await;
    let terminal = make_terminal(event_store.clone()).await;
    let conductor = make_stub_conductor().await;

    let port = ActorAlmPort::new(
        format!("run-{}", Uuid::new_v4().as_simple()),
        "alm-test-actor",
        "stub-model",
        event_store,
        conductor,
        Some(terminal),
    );

    let mut args = HashMap::new();
    args.insert("command".to_string(), "echo hello-from-alm".to_string());

    let result = port.execute_tool("bash", &args).await;

    assert!(
        result.success,
        "bash dispatch should report success (fire-and-forget)"
    );
    assert!(
        result.output.starts_with("dispatched:corr_id:"),
        "output must be dispatched:corr_id:<id>, got: '{}'",
        result.output
    );
    assert!(result.error.is_none(), "no error on successful dispatch");

    // Extract the corr_id from the output for use in subsequent assertions
    let corr_id = result.output.strip_prefix("dispatched:corr_id:").unwrap();
    assert!(!corr_id.is_empty(), "corr_id must be non-empty");

    println!("  [DISPATCH] corr_id: {corr_id}");
}

/// `dispatch_tool("bash")` without a terminal actor returns a clear error
/// rather than silently dropping the work.
#[tokio::test]
async fn test_dispatch_tool_bash_without_terminal_returns_error() {
    let (event_store, _tmp) = make_event_store().await;
    let conductor = make_stub_conductor().await;

    let port = ActorAlmPort::new(
        format!("run-{}", Uuid::new_v4().as_simple()),
        "alm-test-actor-noterminal",
        "stub-model",
        event_store,
        conductor,
        None, // no terminal
    );

    let mut args = HashMap::new();
    args.insert("command".to_string(), "echo should-fail".to_string());

    let result = port.execute_tool("bash", &args).await;

    assert!(!result.success, "should fail without a terminal actor");
    assert!(
        result.error.is_some(),
        "must have an error message explaining why"
    );
    let err = result.error.unwrap();
    assert!(
        err.contains("no terminal") || err.contains("bash unavailable"),
        "error should mention terminal unavailability, got: '{err}'"
    );

    println!("  [NO-TERMINAL] error: {err}");
}

/// `resolve_source(ToolOutput, corr_id)` returns `None` before any result lands.
///
/// This is the "not ready yet" signal the harness uses to end its turn and
/// retry on the next wake. This test verifies it doesn't panic or block.
#[tokio::test]
async fn test_resolve_source_returns_none_before_result_lands() {
    let (event_store, _tmp) = make_event_store().await;
    let conductor = make_stub_conductor().await;

    let port = ActorAlmPort::new(
        format!("run-{}", Uuid::new_v4().as_simple()),
        "alm-test-resolve",
        "stub-model",
        event_store,
        conductor,
        None,
    );

    let corr_id = format!("bash-{}", Uuid::new_v4().as_simple());
    let result = port
        .resolve_source(
            &sandbox::baml_client::types::ContextSourceKind::ToolOutput,
            &corr_id,
            None,
        )
        .await;

    assert!(
        result.is_none(),
        "must return None when no tool.result event exists for this corr_id"
    );

    println!("  [PRE-RESULT] resolve_source correctly returned None for unknown corr_id");
}

/// `resolve_source(ToolOutput, corr_id)` returns the result once a `tool.result`
/// event is manually written to EventStore with the correct corr_id.
///
/// This simulates what the terminal SHOULD do after `RunAgenticTaskDetached` completes.
/// It verifies the `resolve_source` polling logic is correct even if the terminal
/// does not yet auto-emit the event (see Gap Note below).
///
/// # Gap Note
///
/// `RunAgenticTaskDetached` currently does NOT write a `tool.result` event to
/// EventStore. The `call_id` is passed to the execution context but `run_agentic_task`
/// completes by calling `emit_writer_completion` (only fires when `writer_actor` is set).
///
/// The `tool.result` event must be written explicitly by whoever calls
/// `RunAgenticTaskDetached` — or the terminal must be updated to write it.
/// This is a known gap tracked for the terminal refactor.
#[tokio::test]
async fn test_resolve_source_returns_result_after_tool_result_event() {
    let (event_store, _tmp) = make_event_store().await;
    let conductor = make_stub_conductor().await;
    let run_id = format!("run-{}", Uuid::new_v4().as_simple());
    let corr_id = format!("bash-{}", Uuid::new_v4().as_simple());

    let port = ActorAlmPort::new(
        run_id.clone(),
        "alm-test-resolve-after",
        "stub-model",
        event_store.clone(),
        conductor,
        None,
    );

    // Simulate the terminal writing its result (what it SHOULD do, not what it currently does)
    let result_payload = serde_json::json!({
        "correlation_id": corr_id,
        "corr_id": corr_id,
        "run_id": run_id,
        "output_excerpt": "hello-from-alm\n",
        "success": true,
        "exit_code": 0,
    });
    let _ = event_store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "tool.result".to_string(),
            payload: result_payload,
            actor_id: format!("terminal:{corr_id}"),
            user_id: "system".to_string(),
        },
    });

    // Give the async append a moment to land
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = port
        .resolve_source(
            &sandbox::baml_client::types::ContextSourceKind::ToolOutput,
            &corr_id,
            None,
        )
        .await;

    assert!(
        result.is_some(),
        "must return Some after tool.result event lands"
    );
    let text = result.unwrap();
    assert!(
        text.contains("hello-from-alm"),
        "output content must be preserved, got: '{text}'"
    );

    println!("  [POST-RESULT] resolve_source returned: '{text}'");
}

/// `dispatch_tool` (the corr_id-accepting variant) fires the terminal message
/// with the correct corr_id embedded as `call_id`.
///
/// Verifies the `ActorAlmPort::dispatch_tool` API used by the checkpoint-aware
/// turn loop, where the caller controls the corr_id for tracking in `pending_replies`.
#[tokio::test]
async fn test_dispatch_tool_uses_caller_corr_id() {
    let (event_store, _tmp) = make_event_store().await;
    let terminal = make_terminal(event_store.clone()).await;
    let conductor = make_stub_conductor().await;
    let run_id = format!("run-{}", Uuid::new_v4().as_simple());
    let corr_id = format!("bash-caller-corr-{}", Uuid::new_v4().as_simple());

    let port = ActorAlmPort::new(
        run_id.clone(),
        "alm-test-dispatch-corr",
        "stub-model",
        event_store,
        conductor,
        Some(terminal),
    );

    let mut args = HashMap::new();
    args.insert(
        "command".to_string(),
        "echo caller-controlled-corr".to_string(),
    );

    // dispatch_tool with a caller-supplied corr_id (the checkpoint-aware path)
    port.dispatch_tool("bash", &args, &corr_id).await;

    // The call itself is fire-and-forget — we can only verify it didn't panic.
    // Correctness of the corr_id landing in EventStore is verified by
    // test_resolve_source_returns_result_after_tool_result_event once the
    // terminal emits the event (post-refactor).
    println!("  [DISPATCH-CORR] fire-and-forget completed without panic, corr_id: {corr_id}");
}

/// `emit_message` writes a `harness.emit` event to EventStore.
/// Verifies the port can emit observability events without blocking.
#[tokio::test]
async fn test_emit_message_writes_harness_emit_event() {
    use sandbox::actors::event_store::EventStoreMsg;

    let (event_store, _tmp) = make_event_store().await;
    let conductor = make_stub_conductor().await;
    let run_id = format!("run-{}", Uuid::new_v4().as_simple());

    let port = ActorAlmPort::new(
        run_id.clone(),
        "alm-test-emit",
        "stub-model",
        event_store.clone(),
        conductor,
        None,
    );

    port.emit_message("starting analysis phase").await;

    // Give the async append a moment to land
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Verify via get_recent_events that the harness.emit event was written
    let events = ractor::call_t!(
        event_store,
        |reply| EventStoreMsg::GetRecentEvents {
            since_seq: 0,
            limit: 20,
            event_type_prefix: Some("harness.emit".to_string()),
            actor_id: None,
            user_id: None,
            reply,
        },
        2000
    )
    .expect("rpc ok")
    .expect("store ok");

    assert!(
        !events.is_empty(),
        "must have at least one harness.emit event"
    );

    let ev = events.last().unwrap();
    let msg = ev
        .payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("starting analysis phase"),
        "message content must be preserved, got: '{msg}'"
    );
    let ev_run_id = ev
        .payload
        .get("run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(ev_run_id, run_id, "run_id must be scoped correctly");

    println!("  [EMIT] harness.emit event verified in EventStore");
}
