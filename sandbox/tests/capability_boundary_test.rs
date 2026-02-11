use ractor::Actor;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

#[tokio::test]
async fn test_terminal_delegation_emits_appactor_toolactor_worker_event() {
    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");
    let (application_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn application supervisor");

    let actor_id = "capability-delegation".to_string();
    let session_id = "capability-session".to_string();
    let thread_id = "capability-thread".to_string();
    let _task = ractor::call!(application_supervisor, |reply| {
        ApplicationSupervisorMsg::DelegateTerminalTask {
            terminal_id: format!("term-{actor_id}"),
            actor_id: actor_id.clone(),
            user_id: "test-user".to_string(),
            shell: "/bin/zsh".to_string(),
            working_dir: ".".to_string(),
            command: "printf CAP_BOUNDARY_OK".to_string(),
            timeout_ms: Some(20_000),
            model_override: None,
            objective: None,
            session_id: Some(session_id.clone()),
            thread_id: Some(thread_id.clone()),
            reply,
        }
    })
    .expect("delegate task rpc")
    .expect("delegate task accepted");

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let events = ractor::call!(event_store, |reply| {
        EventStoreMsg::GetEventsForActorWithScope {
            actor_id: actor_id.clone(),
            session_id,
            thread_id,
            since_seq: 0,
            reply,
        }
    })
    .expect("load events rpc")
    .expect("load events");

    let worker_started = events.iter().find(|event| {
        event.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_STARTED
            && event.payload.get("interface_kind").and_then(|v| v.as_str())
                == Some("appactor_toolactor")
    });
    assert!(
        worker_started.is_some(),
        "expected worker.task.started event with appactor_toolactor interface kind"
    );
}

#[tokio::test]
async fn test_research_delegation_emits_research_worker_events() {
    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");
    let (application_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn application supervisor");

    let actor_id = "capability-research".to_string();
    let session_id = "capability-research-session".to_string();
    let thread_id = "capability-research-thread".to_string();
    let task = ractor::call!(application_supervisor, |reply| {
        ApplicationSupervisorMsg::DelegateResearchTask {
            researcher_id: format!("researcher-{actor_id}"),
            actor_id: actor_id.clone(),
            user_id: "test-user".to_string(),
            query: "latest rust release notes".to_string(),
            objective: None,
            provider: Some("tavily".to_string()),
            max_results: Some(3),
            time_range: None,
            include_domains: None,
            exclude_domains: None,
            timeout_ms: Some(8_000),
            model_override: Some("DefinitelyUnknownModel".to_string()),
            reasoning: Some("boundary test".to_string()),
            session_id: Some(session_id.clone()),
            thread_id: Some(thread_id.clone()),
            reply,
        }
    })
    .expect("delegate research task rpc")
    .expect("delegate research task accepted");

    let start = std::time::Instant::now();
    let mut events = Vec::new();
    while start.elapsed() < std::time::Duration::from_secs(8) {
        events = ractor::call!(event_store, |reply| {
            EventStoreMsg::GetEventsForActorWithScope {
                actor_id: actor_id.clone(),
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                since_seq: 0,
                reply,
            }
        })
        .expect("load events rpc")
        .expect("load events");

        let has_terminal_state = events.iter().any(|event| {
            event.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED
                || event.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED
        });
        if has_terminal_state {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let worker_started = events.iter().find(|event| {
        event.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_STARTED
            && event.payload.get("interface_kind").and_then(|v| v.as_str())
                == Some("appactor_toolactor")
            && event.payload.get("correlation_id").and_then(|v| v.as_str())
                == Some(task.correlation_id.as_str())
    });
    assert!(
        worker_started.is_some(),
        "expected worker.task.started event with appactor_toolactor interface kind for research task"
    );

    let research_started = events.iter().find(|event| {
        event.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_STARTED
            && event.payload.get("task_id").and_then(|v| v.as_str()) == Some(task.task_id.as_str())
    });
    assert!(
        research_started.is_some(),
        "expected research.task.started event for delegated research task"
    );

    let worker_terminal = events.iter().find(|event| {
        event.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED
            || event.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_FAILED
    });
    assert!(
        worker_terminal.is_some(),
        "expected worker task terminal event for delegated research task"
    );

    let research_terminal = events.iter().find(|event| {
        event.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED
            || event.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED
    });
    assert!(
        research_terminal.is_some(),
        "expected research task terminal event for delegated research task"
    );
}
