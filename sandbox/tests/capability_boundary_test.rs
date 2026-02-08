use ractor::Actor;
use sandbox::actors::chat_agent::{ChatAgent, ChatAgentArguments, ChatAgentMsg};
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};

#[tokio::test]
async fn test_chat_agent_exposes_only_bash_tool() {
    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");

    let (agent, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: "capability-tools".to_string(),
            user_id: "test-user".to_string(),
            event_store,
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    )
    .await
    .expect("spawn chat agent");

    let tools = ractor::call!(agent, |reply| ChatAgentMsg::GetAvailableTools { reply })
        .expect("get tools rpc");
    assert_eq!(tools, vec!["bash".to_string()]);
}

#[tokio::test]
async fn test_chat_bash_delegation_emits_appactor_toolactor_worker_event() {
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
