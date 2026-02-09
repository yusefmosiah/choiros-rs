use ractor::Actor;
use sandbox::actors::chat_agent::{
    get_available_tools, get_conversation_history, switch_model, ChatAgentArguments,
    ChatAgentError, ChatAgentMsg,
};
use sandbox::actors::{
    AppendEvent, ChatAgent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};

#[tokio::test]
async fn test_chat_agent_creation() {
    let (event_store_ref, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let (agent_ref, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: "agent-1".to_string(),
            user_id: "user-1".to_string(),
            event_store: event_store_ref.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    )
    .await
    .unwrap();

    let tools = get_available_tools(&agent_ref).await.unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"bash".to_string()));
    assert!(tools.contains(&"web_search".to_string()));

    agent_ref.stop(None);
    event_store_ref.stop(None);
}

#[tokio::test]
async fn test_model_switching() {
    let (event_store_ref, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let (agent_ref, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: "agent-1".to_string(),
            user_id: "user-1".to_string(),
            event_store: event_store_ref.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    )
    .await
    .unwrap();

    let result = switch_model(&agent_ref, "GLM47").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_ok());

    let result = switch_model(&agent_ref, "InvalidModel").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());

    agent_ref.stop(None);
    event_store_ref.stop(None);
}

#[tokio::test]
async fn test_per_request_model_override_validation() {
    let (event_store_ref, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let (agent_ref, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: "agent-override-test".to_string(),
            user_id: "user-1".to_string(),
            event_store: event_store_ref.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    )
    .await
    .unwrap();

    let result = ractor::call!(agent_ref, |reply| ChatAgentMsg::ProcessMessage {
        text: "hello".to_string(),
        session_id: None,
        thread_id: None,
        model_override: Some("NotARealModel".to_string()),
        reply,
    })
    .expect("chat agent call should succeed");

    assert!(matches!(result, Err(ChatAgentError::InvalidModel(_))));

    agent_ref.stop(None);
    event_store_ref.stop(None);
}

#[tokio::test]
async fn test_history_loaded_from_event_store() {
    let (event_store_ref, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .unwrap();

    let actor_id = "history-actor".to_string();

    let _ = ractor::call!(event_store_ref, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: shared_types::EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!("Hello"),
            actor_id: actor_id.clone(),
            user_id: "user-1".to_string(),
        },
        reply,
    })
    .unwrap()
    .unwrap();

    let _ = ractor::call!(event_store_ref, |reply| EventStoreMsg::Append {
        event: AppendEvent {
            event_type: shared_types::EVENT_CHAT_ASSISTANT_MSG.to_string(),
            payload: serde_json::json!({"text": "Hi there"}),
            actor_id: actor_id.clone(),
            user_id: "system".to_string(),
        },
        reply,
    })
    .unwrap()
    .unwrap();

    let (agent_ref, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id,
            user_id: "user-1".to_string(),
            event_store: event_store_ref.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    )
    .await
    .unwrap();

    let history = get_conversation_history(&agent_ref).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].role, "user");
    assert!(
        history[0].content.contains("Hello"),
        "user content should contain 'Hello'"
    );
    assert_eq!(history[1].role, "assistant");
    assert!(
        history[1].content.contains("Hi there"),
        "assistant content should contain 'Hi there'"
    );

    agent_ref.stop(None);
    event_store_ref.stop(None);
}
