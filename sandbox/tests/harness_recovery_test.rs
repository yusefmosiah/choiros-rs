//! Harness checkpoint recovery integration test.
//!
//! Verifies the durability contract:
//! 1. Harness writes a `harness.checkpoint` event to EventStore (with pending corr_ids)
//! 2. Simulated crash — harness state dropped entirely
//! 3. Supervisor reads latest checkpoint from EventStore by run_id
//! 4. Subharness result arrives in EventStore (written by the terminal/subharness actor)
//! 5. `resolve_source(ToolOutput, corr_id)` returns the result
//! 6. Recovered state matches original: turn_number, working_memory, pending_replies
//!
//! No LLM calls — pure EventStore + AlmPort contract test.
//!
//! Run:
//!   cargo test -p sandbox --test harness_recovery_test -- --nocapture

use chrono::Utc;
use ractor::Actor;
use shared_types::{HarnessCheckpoint, PendingReply, TurnSummary};
use std::collections::HashMap;

use sandbox::actors::agent_harness::alm::{LlmCallResult, AlmPort, AlmToolExecution};
use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use sandbox::baml_client::types::ContextSourceKind;

// ─── Minimal AlmPort backed by a live EventStore actor ───────────────────────

struct RecoveryTestPort {
    event_store: ractor::ActorRef<EventStoreMsg>,
    run_id: String,
    actor_id: String,
}

#[async_trait::async_trait]
impl AlmPort for RecoveryTestPort {
    fn capabilities_description(&self) -> String {
        "test".into()
    }
    fn model_id(&self) -> &str {
        "test"
    }
    fn run_id(&self) -> &str {
        &self.run_id
    }
    fn actor_id(&self) -> &str {
        &self.actor_id
    }

    async fn resolve_source(
        &self,
        kind: &ContextSourceKind,
        source_ref: &str,
        _max_tokens: Option<i64>,
    ) -> Option<String> {
        if !matches!(kind, ContextSourceKind::ToolOutput) {
            return None;
        }
        // Poll EventStore with a 2s timeout — same logic as ActorAlmPort
        for event_prefix in &["harness.result", "tool.result"] {
            let result = ractor::call_t!(
                self.event_store,
                |reply| EventStoreMsg::GetEventsByCorrId {
                    corr_id: source_ref.to_string(),
                    event_type_prefix: Some(event_prefix.to_string()),
                    reply,
                },
                2000
            );
            if let Ok(Ok(events)) = result {
                if let Some(event) = events.last() {
                    let text = event
                        .payload
                        .get("output_excerpt")
                        .or_else(|| event.payload.get("output"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| event.payload.to_string());
                    return Some(text);
                }
            }
        }
        None
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
    ) -> AlmToolExecution {
        AlmToolExecution {
            turn: 0,
            tool_name: tool_name.into(),
            tool_args: tool_args.clone(),
            success: false,
            output: String::new(),
            error: Some("not used in recovery test".into()),
            elapsed_ms: 0,
        }
    }

    async fn call_llm(
        &self,
        _prompt: &str,
        _system: Option<&str>,
        _hint: Option<&str>,
    ) -> LlmCallResult {
        LlmCallResult {
            output: String::new(),
            success: false,
            error: None,
            elapsed_ms: 0,
        }
    }

    async fn emit_message(&self, _message: &str) {}

    async fn dispatch_tool(&self, _tool: &str, _args: &HashMap<String, String>, _corr_id: &str) {}

    async fn write_checkpoint(&self, checkpoint: &HarnessCheckpoint) {
        let payload = serde_json::to_value(checkpoint).expect("serialize checkpoint");
        let _ = self.event_store.send_message(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "harness.checkpoint".into(),
                payload,
                actor_id: self.actor_id.clone(),
                user_id: "system".into(),
            },
        });
    }

    async fn spawn_harness(&self, _objective: &str, _ctx: serde_json::Value, _corr_id: &str) {}
}

// ─── Test helpers ────────────────────────────────────────────────────────────

async fn make_event_store() -> (ractor::ActorRef<EventStoreMsg>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("recovery_test.db");
    let (store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db.to_str().unwrap().into()),
    )
    .await
    .expect("spawn EventStoreActor");
    (store, tmp)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Write a checkpoint, then read it back. Verifies the EventStore round-trip.
#[tokio::test]
async fn test_checkpoint_write_and_read() {
    let (store, _tmp) = make_event_store().await;
    let run_id = format!("test-run-{}", uuid::Uuid::new_v4().as_simple());

    let now = Utc::now();
    let corr_id = "fanout-2-0".to_string();

    let checkpoint = HarnessCheckpoint {
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
        turn_number: 2,
        working_memory: "I have dispatched two branches. Waiting for results.".into(),
        objective: "analyse the codebase and write a report".into(),
        pending_replies: vec![PendingReply {
            corr_id: corr_id.clone(),
            actor_kind: "harness".into(),
            objective_summary: "analyse src/actors".into(),
            sent_at: now,
            timeout_at: Some(now + chrono::Duration::seconds(120)),
        }],
        turn_summaries: vec![
            TurnSummary {
                turn_number: 1,
                action_kind: "ToolCalls".into(),
                working_memory_excerpt: "reading files".into(),
                corr_ids_fired: vec![],
                elapsed_ms: 42,
            },
            TurnSummary {
                turn_number: 2,
                action_kind: "FanOut".into(),
                working_memory_excerpt: "dispatching fanout".into(),
                corr_ids_fired: vec![corr_id.clone()],
                elapsed_ms: 15,
            },
        ],
        checkpointed_at: now,
    };

    let port = RecoveryTestPort {
        event_store: store.clone(),
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
    };

    // Write checkpoint (this is what the harness does at turn boundary)
    port.write_checkpoint(&checkpoint).await;

    // Give the async append a moment to land
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // ── Recovery: read latest checkpoint from EventStore ──
    let recovered_event = ractor::call_t!(
        store,
        |reply| EventStoreMsg::GetLatestHarnessCheckpoint {
            run_id: run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc call")
    .expect("event store ok");

    let recovered_event = recovered_event.expect("checkpoint event must exist");

    // Deserialize back into HarnessCheckpoint
    let recovered: HarnessCheckpoint =
        serde_json::from_value(recovered_event.payload).expect("deserialize checkpoint");

    // Assertions: state is preserved across crash/recovery
    assert_eq!(recovered.run_id, run_id, "run_id preserved");
    assert_eq!(recovered.turn_number, 2, "turn_number preserved");
    assert_eq!(
        recovered.working_memory, "I have dispatched two branches. Waiting for results.",
        "working_memory preserved"
    );
    assert_eq!(recovered.pending_replies.len(), 1, "one pending reply");
    assert_eq!(
        recovered.pending_replies[0].corr_id, corr_id,
        "corr_id preserved"
    );
    assert_eq!(
        recovered.turn_summaries.len(),
        2,
        "turn summaries preserved"
    );

    println!(
        "  [RECOVERY] turn:{} wm:'{}' pending:{}",
        recovered.turn_number,
        &recovered.working_memory[..50.min(recovered.working_memory.len())],
        recovered.pending_replies.len(),
    );
}

/// Write a checkpoint, simulate subharness result arriving, verify resolve_source works.
#[tokio::test]
async fn test_recovery_resolve_source_after_result_lands() {
    let (store, _tmp) = make_event_store().await;
    let run_id = format!("test-run-{}", uuid::Uuid::new_v4().as_simple());
    let corr_id = format!("fanout-3-0-{}", uuid::Uuid::new_v4().as_simple());

    let port = RecoveryTestPort {
        event_store: store.clone(),
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
    };

    // Step 1: Before result lands — resolve_source returns None
    let before = port
        .resolve_source(&ContextSourceKind::ToolOutput, &corr_id, None)
        .await;
    assert!(before.is_none(), "no result before subharness completes");
    println!("  [PRE-RESULT] resolve_source returned None (correct)");

    // Step 2: Simulate subharness completing — write harness.result to EventStore
    let result_payload = serde_json::json!({
        "correlation_id": corr_id,
        "corr_id": corr_id,
        "objective": "analyse src/actors",
        "objective_satisfied": true,
        "steps_taken": 7,
        "output_excerpt": "Found 12 actors. Terminal is isolated from all LLM actors. EventStore is append-only.",
        "timestamp": Utc::now().to_rfc3339(),
    });
    let _ = store.send_message(EventStoreMsg::AppendAsync {
        event: AppendEvent {
            event_type: "harness.result".into(),
            payload: result_payload,
            actor_id: format!("harness:{corr_id}"),
            user_id: "system".into(),
        },
    });

    // Give async append time to land
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Step 3: After result lands — resolve_source returns the output
    let after = port
        .resolve_source(&ContextSourceKind::ToolOutput, &corr_id, None)
        .await;
    assert!(
        after.is_some(),
        "result available after harness.result event written"
    );
    let text = after.unwrap();
    assert!(
        text.contains("Terminal is isolated"),
        "output content preserved: got '{text}'"
    );
    println!(
        "  [POST-RESULT] resolve_source returned: '{}'",
        &text[..80.min(text.len())]
    );
}

/// Write two checkpoints for the same run — GetLatestHarnessCheckpoint returns the newer one.
#[tokio::test]
async fn test_recovery_latest_checkpoint_wins() {
    let (store, _tmp) = make_event_store().await;
    let run_id = format!("test-run-{}", uuid::Uuid::new_v4().as_simple());

    let port = RecoveryTestPort {
        event_store: store.clone(),
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
    };

    let now = Utc::now();

    // Write turn 1 checkpoint
    port.write_checkpoint(&HarnessCheckpoint {
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
        turn_number: 1,
        working_memory: "turn 1 state".into(),
        objective: "test".into(),
        pending_replies: vec![PendingReply {
            corr_id: "corr-1".into(),
            actor_kind: "harness".into(),
            objective_summary: "branch 1".into(),
            sent_at: now,
            timeout_at: None,
        }],
        turn_summaries: vec![],
        checkpointed_at: now,
    })
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // Write turn 3 checkpoint (simulating turn 2 completed without FanOut)
    port.write_checkpoint(&HarnessCheckpoint {
        run_id: run_id.clone(),
        actor_id: "test-actor".into(),
        turn_number: 3,
        working_memory: "turn 3 state — deeper analysis".into(),
        objective: "test".into(),
        pending_replies: vec![PendingReply {
            corr_id: "corr-3".into(),
            actor_kind: "harness".into(),
            objective_summary: "branch 3".into(),
            sent_at: now,
            timeout_at: None,
        }],
        turn_summaries: vec![],
        checkpointed_at: now,
    })
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let event = ractor::call_t!(
        store,
        |reply| EventStoreMsg::GetLatestHarnessCheckpoint {
            run_id: run_id.clone(),
            reply,
        },
        2000
    )
    .expect("rpc")
    .expect("store ok")
    .expect("event exists");

    let recovered: HarnessCheckpoint = serde_json::from_value(event.payload).expect("deserialize");

    assert_eq!(
        recovered.turn_number, 3,
        "latest checkpoint (turn 3) returned"
    );
    assert_eq!(
        recovered.pending_replies[0].corr_id, "corr-3",
        "latest pending corr_id returned"
    );
    println!(
        "  [LATEST] recovered turn:{} corr:{}",
        recovered.turn_number, recovered.pending_replies[0].corr_id
    );
}
