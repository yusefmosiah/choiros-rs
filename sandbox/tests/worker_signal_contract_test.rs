use ractor::Actor;
use sandbox::actors::event_store::{get_recent_events, EventStoreActor, EventStoreArguments};
use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

fn sample_report() -> shared_types::WorkerTurnReport {
    shared_types::WorkerTurnReport {
        turn_id: "turn-1".to_string(),
        worker_id: "researcher-1".to_string(),
        task_id: "task-1".to_string(),
        worker_role: Some("researcher".to_string()),
        status: shared_types::WorkerTurnStatus::Completed,
        summary: Some("finished first pass".to_string()),
        findings: vec![
            shared_types::WorkerFinding {
                finding_id: "f-1".to_string(),
                claim: "Models should be configurable at runtime".to_string(),
                confidence: 0.92,
                evidence_refs: vec!["doc://policy".to_string()],
                novel: Some(true),
            },
            shared_types::WorkerFinding {
                finding_id: "f-2".to_string(),
                claim: "Models should be configurable at runtime".to_string(),
                confidence: 0.91,
                evidence_refs: vec!["doc://policy".to_string()],
                novel: Some(false),
            },
            shared_types::WorkerFinding {
                finding_id: "f-3".to_string(),
                claim: "Low confidence filler".to_string(),
                confidence: 0.10,
                evidence_refs: vec!["doc://weak".to_string()],
                novel: Some(false),
            },
        ],
        learnings: vec![
            shared_types::WorkerLearning {
                learning_id: "l-1".to_string(),
                insight: "Typed envelopes reduce ambiguity".to_string(),
                confidence: 0.84,
                supports: vec!["f-1".to_string()],
                changes_plan: Some(true),
            },
            shared_types::WorkerLearning {
                learning_id: "l-2".to_string(),
                insight: "Second learning should be capped".to_string(),
                confidence: 0.83,
                supports: vec!["f-1".to_string()],
                changes_plan: Some(false),
            },
        ],
        escalations: vec![shared_types::WorkerEscalation {
            escalation_id: "e-1".to_string(),
            kind: shared_types::WorkerEscalationKind::Help,
            reason: "Need policy decision for high-risk route".to_string(),
            urgency: shared_types::WorkerEscalationUrgency::Medium,
            options: vec!["A".to_string(), "B".to_string()],
            recommended_option: Some("A".to_string()),
            requires_human: Some(false),
        }],
        artifacts: vec![shared_types::WorkerArtifact {
            artifact_id: "a-1".to_string(),
            kind: "report".to_string(),
            reference: "/tmp/research-note.md".to_string(),
        }],
        created_at: Some(chrono::Utc::now().to_rfc3339()),
    }
}

#[tokio::test]
async fn test_ingest_worker_turn_report_emits_canonical_events_and_rejections() {
    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");
    let (app_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn app supervisor");

    let result = ractor::call!(app_supervisor, |reply| {
        ApplicationSupervisorMsg::IngestWorkerTurnReport {
            actor_id: "researcher-1".to_string(),
            user_id: "test-user".to_string(),
            session_id: Some("session-a".to_string()),
            thread_id: Some("thread-a".to_string()),
            report: sample_report(),
            reply,
        }
    })
    .expect("rpc ok")
    .expect("ingest ok");

    assert_eq!(result.accepted_findings, 1);
    assert_eq!(result.accepted_learnings, 1);
    assert_eq!(result.accepted_escalations, 1);
    assert_eq!(result.accepted_artifacts, 1);
    assert!(result.escalation_notified);
    assert!(!result.rejections.is_empty());

    let research_finding_events = get_recent_events(
        &event_store,
        0,
        100,
        Some(shared_types::EVENT_TOPIC_RESEARCH_FINDING_CREATED.to_string()),
        Some("researcher-1".to_string()),
        None,
    )
    .await
    .expect("query finding rpc")
    .expect("query finding");
    assert_eq!(research_finding_events.len(), 1);

    let escalation_events = get_recent_events(
        &event_store,
        0,
        100,
        Some(shared_types::EVENT_TOPIC_WORKER_SIGNAL_ESCALATION_REQUESTED.to_string()),
        Some("researcher-1".to_string()),
        None,
    )
    .await
    .expect("query escalation rpc")
    .expect("query escalation");
    assert_eq!(escalation_events.len(), 1);

    let rejection_events = get_recent_events(
        &event_store,
        0,
        100,
        Some(shared_types::EVENT_TOPIC_WORKER_SIGNAL_REJECTED.to_string()),
        Some("researcher-1".to_string()),
        None,
    )
    .await
    .expect("query rejection rpc")
    .expect("query rejection");
    assert!(rejection_events.len() >= 2);
}

#[tokio::test]
async fn test_ingest_worker_turn_report_applies_escalation_cooldown() {
    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");
    let (app_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn app supervisor");

    let first = ractor::call!(app_supervisor, |reply| {
        ApplicationSupervisorMsg::IngestWorkerTurnReport {
            actor_id: "researcher-1".to_string(),
            user_id: "test-user".to_string(),
            session_id: Some("session-b".to_string()),
            thread_id: Some("thread-b".to_string()),
            report: sample_report(),
            reply,
        }
    })
    .expect("first rpc ok")
    .expect("first ingest ok");
    assert_eq!(first.accepted_escalations, 1);

    let second = ractor::call!(app_supervisor, |reply| {
        ApplicationSupervisorMsg::IngestWorkerTurnReport {
            actor_id: "researcher-1".to_string(),
            user_id: "test-user".to_string(),
            session_id: Some("session-b".to_string()),
            thread_id: Some("thread-b".to_string()),
            report: sample_report(),
            reply,
        }
    })
    .expect("second rpc ok")
    .expect("second ingest ok");
    assert_eq!(second.accepted_escalations, 0);
    assert!(second.rejections.iter().any(|r| {
        r.signal_type == shared_types::WorkerSignalType::Escalation
            && r.reason == shared_types::WorkerSignalRejectReason::EscalationCooldown
    }));

    let escalation_events = get_recent_events(
        &event_store,
        0,
        100,
        Some(shared_types::EVENT_TOPIC_WORKER_SIGNAL_ESCALATION_REQUESTED.to_string()),
        Some("researcher-1".to_string()),
        None,
    )
    .await
    .expect("query escalation rpc")
    .expect("query escalation");
    assert_eq!(escalation_events.len(), 1);
}
