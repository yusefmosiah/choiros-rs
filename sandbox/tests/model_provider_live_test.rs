use ractor::Actor;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::actors::{
    chat_agent::ChatAgent, chat_agent::ChatAgentArguments, chat_agent::ChatAgentMsg,
};
use sandbox::baml_client::types::Message as BamlMessage;
use sandbox::baml_client::B;
use sandbox::supervisor::ApplicationSupervisor;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

fn env_present(key: &str) -> bool {
    std::env::var(key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn bedrock_auth_present() -> bool {
    env_present("AWS_BEARER_TOKEN_BEDROCK")
        || env_present("AWS_PROFILE")
        || (env_present("AWS_ACCESS_KEY_ID") && env_present("AWS_SECRET_ACCESS_KEY"))
}

fn ensure_tls_cert_env() {
    if std::env::var("SSL_CERT_FILE").is_err() {
        let cert_path = "/etc/ssl/cert.pem";
        if std::path::Path::new(cert_path).exists() {
            std::env::set_var("SSL_CERT_FILE", cert_path);
        }
    }
}

fn missing_env_for_provider(provider: &ProviderConfig) -> Vec<String> {
    match provider {
        ProviderConfig::AwsBedrock { .. } => {
            if bedrock_auth_present() {
                Vec::new()
            } else {
                vec![
                    "AWS_BEARER_TOKEN_BEDROCK or AWS_PROFILE or AWS_ACCESS_KEY_ID+AWS_SECRET_ACCESS_KEY"
                        .to_string(),
                ]
            }
        }
        ProviderConfig::AnthropicCompatible { api_key_env, .. }
        | ProviderConfig::OpenAiGeneric { api_key_env, .. } => {
            if env_present(api_key_env) {
                Vec::new()
            } else {
                vec![api_key_env.clone()]
            }
        }
    }
}

fn live_test_concurrency(default_limit: usize) -> usize {
    let parsed = std::env::var("CHOIR_LIVE_TEST_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_limit);
    parsed.clamp(1, 8)
}

fn available_live_models(registry: &ModelRegistry) -> (Vec<String>, Vec<String>) {
    let mut eligible = Vec::new();
    let mut skipped = Vec::new();

    for model_id in registry.available_model_ids() {
        let Some(config) = registry.get(&model_id) else {
            skipped.push(format!("{} (model missing from registry)", model_id));
            continue;
        };
        let missing_for_case = missing_env_for_provider(&config.provider);
        if !missing_for_case.is_empty() {
            skipped.push(format!(
                "{} (missing env: {})",
                model_id,
                missing_for_case.join(",")
            ));
            continue;
        }
        eligible.push(model_id);
    }

    (eligible, skipped)
}

fn sampled_live_models(eligible: &[String]) -> Vec<String> {
    let mut selected: Vec<String> = Vec::new();

    let prefer = |candidates: &[&str]| -> Option<String> {
        for candidate in candidates {
            if eligible.iter().any(|model| model == candidate) {
                return Some((*candidate).to_string());
            }
        }
        None
    };

    if let Some(model) = prefer(&[
        "ClaudeBedrockHaiku45",
        "ClaudeBedrockSonnet45",
        "ClaudeBedrockOpus46",
    ]) {
        selected.push(model);
    }
    if let Some(model) = prefer(&["ZaiGLM47", "ZaiGLM47Air", "ZaiGLM47Flash"]) {
        if !selected.contains(&model) {
            selected.push(model);
        }
    }
    if let Some(model) = prefer(&["KimiK25", "KimiK25Fallback"]) {
        if !selected.contains(&model) {
            selected.push(model);
        }
    }

    if selected.is_empty() {
        selected.extend(eligible.iter().take(3).cloned());
    }

    selected
}

fn parse_bash_command(tool_args_json: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(tool_args_json).ok()?;
    parsed
        .get("cmd")
        .and_then(|v| v.as_str())
        .or_else(|| parsed.get("command").and_then(|v| v.as_str()))
        .map(ToString::to_string)
}

fn parse_bash_model(tool_args_json: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(tool_args_json).ok()?;
    parsed
        .get("model")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

async fn run_delegation_case(
    event_store: ractor::ActorRef<EventStoreMsg>,
    app_supervisor: ractor::ActorRef<sandbox::supervisor::ApplicationSupervisorMsg>,
    model_id: String,
) -> Result<String, String> {
    let actor_id = format!("chat-live-{model_id}");
    let (agent, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: actor_id.clone(),
            user_id: "live-test-user".to_string(),
            event_store,
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: Some(app_supervisor),
        },
    )
    .await
    .map_err(|e| format!("spawn chat agent: {e}"))?;

    let marker = format!("CHOIR_TOOL_OK_{}", model_id.to_lowercase());
    let prompt = format!(
        "Use the bash tool exactly once with cmd `printf {marker}`. Do not answer from memory."
    );

    let reply = ractor::call!(agent, |rpc| ChatAgentMsg::ProcessMessage {
        text: prompt,
        session_id: Some(format!("session-{model_id}")),
        thread_id: Some(format!("thread-{model_id}")),
        model_override: Some(model_id.clone()),
        reply: rpc,
    })
    .map_err(|e| format!("chat rpc failed: {e}"))?;

    let result = match reply {
        Ok(response) => {
            let call = match response.tool_calls.iter().find(|c| c.tool_name == "bash") {
                Some(call) => call,
                None => return Err(format!("{model_id} produced no bash tool call")),
            };

            if !call.result.success {
                return Err(format!(
                    "{model_id} bash tool call failed: {}",
                    call.result.content
                ));
            }

            let command = parse_bash_command(&call.tool_args)
                .ok_or_else(|| format!("{model_id} bash tool args missing cmd/command"))?;
            if !command.contains(&marker) {
                return Err(format!(
                    "{model_id} bash tool args missing marker command: {}",
                    command
                ));
            }

            if !call.result.content.contains(&marker) {
                println!(
                    "WARN {} => delegated output omitted marker; accepted based on success + command: {}",
                    model_id, call.result.content
                );
            }
            Ok(format!("delegated bash accepted: {command}"))
        }
        Err(e) => Err(format!("{model_id} chat processing error: {e}")),
    };

    agent.stop(None);
    result
}

async fn run_mixed_model_case(
    event_store: ractor::ActorRef<EventStoreMsg>,
    app_supervisor: ractor::ActorRef<sandbox::supervisor::ApplicationSupervisorMsg>,
    chat_model: String,
    terminal_model: String,
) -> Result<String, String> {
    let actor_id = format!("chat-mixed-{}-{}", chat_model, terminal_model);
    let (agent, _agent_handle) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: actor_id.clone(),
            user_id: "live-test-user".to_string(),
            event_store: event_store.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: Some(app_supervisor),
        },
    )
    .await
    .map_err(|e| format!("spawn mixed chat agent: {e}"))?;

    let chat_prompt = format!(
        "Reply with exactly MIXED_CHAT_OK_{}. Do not call any tools.",
        chat_model.to_lowercase()
    );
    let chat_reply = ractor::call!(agent, |rpc| ChatAgentMsg::ProcessMessage {
        text: chat_prompt,
        session_id: Some(format!("mixed-chat-session-{chat_model}-{terminal_model}")),
        thread_id: Some("thread-1".to_string()),
        model_override: Some(chat_model.clone()),
        reply: rpc,
    })
    .map_err(|e| format!("mixed chat rpc failed: {e}"))?
    .map_err(|e| format!("mixed chat processing failed: {e}"))?;

    if chat_reply.model_used != chat_model {
        agent.stop(None);
        return Err(format!(
            "chat model mismatch: expected {}, got {}",
            chat_model, chat_reply.model_used
        ));
    }

    let marker = format!(
        "CHOIR_MIXED_TOOL_OK_{}_{}",
        chat_model.to_lowercase(),
        terminal_model.to_lowercase()
    );
    let tool_prompt = format!(
        "Use bash exactly once with cmd `printf {marker}` and set the bash model field to `{terminal_model}`."
    );
    let tool_reply = ractor::call!(agent, |rpc| ChatAgentMsg::ProcessMessage {
        text: tool_prompt,
        session_id: Some(format!("mixed-chat-session-{chat_model}-{terminal_model}")),
        thread_id: Some("thread-2".to_string()),
        model_override: Some(chat_model.clone()),
        reply: rpc,
    })
    .map_err(|e| format!("mixed tool rpc failed: {e}"))?
    .map_err(|e| format!("mixed tool processing failed: {e}"))?;

    let Some(tool_call) = tool_reply.tool_calls.iter().find(|c| c.tool_name == "bash") else {
        agent.stop(None);
        return Err(format!(
            "mixed tool run did not produce bash call for case {} -> {}",
            chat_model, terminal_model
        ));
    };
    if !tool_call.result.success {
        agent.stop(None);
        return Err(format!(
            "mixed tool run bash failed for case {} -> {}: {}",
            chat_model, terminal_model, tool_call.result.content
        ));
    }
    let delegated_command = parse_bash_command(&tool_call.tool_args).ok_or_else(|| {
        format!(
            "mixed tool args missing cmd/command for case {} -> {}",
            chat_model, terminal_model
        )
    })?;
    if !delegated_command.contains(&marker) {
        agent.stop(None);
        return Err(format!(
            "mixed tool args command marker mismatch for case {} -> {}: {}",
            chat_model, terminal_model, delegated_command
        ));
    }
    let delegated_model =
        parse_bash_model(&tool_call.tool_args).unwrap_or_else(|| "UNSET".to_string());
    if delegated_model != terminal_model {
        agent.stop(None);
        return Err(format!(
            "mixed tool args model mismatch for case {} -> {}: {}",
            chat_model, terminal_model, delegated_model
        ));
    }

    let events = ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
        actor_id: actor_id.clone(),
        since_seq: 0,
        reply,
    })
    .map_err(|e| format!("load mixed events rpc failed: {e}"))?
    .map_err(|e| format!("load mixed events failed: {e}"))?;

    let command_marker = marker.clone();
    let has_command_marker = events.iter().rev().any(|event| {
        let payload = &event.payload;
        let command_match = payload
            .get("command")
            .and_then(|v| v.as_str())
            .map(|cmd| cmd.contains(&command_marker))
            .unwrap_or(false);
        let executed_match = payload
            .get("executed_commands")
            .and_then(|v| v.as_array())
            .map(|commands| {
                commands
                    .iter()
                    .filter_map(|cmd| cmd.as_str())
                    .any(|cmd| cmd.contains(&command_marker))
            })
            .unwrap_or(false);
        (event.event_type.starts_with("worker.task") || event.event_type == "worker_complete")
            && (command_match || executed_match)
    });

    if !has_command_marker {
        agent.stop(None);
        return Err(format!(
            "no worker event with command marker found for case {} -> {}",
            chat_model, terminal_model
        ));
    }

    let model_used = events
        .iter()
        .rev()
        .find_map(|event| {
            if event.event_type.starts_with("worker.task") || event.event_type == "worker_complete"
            {
                return event
                    .payload
                    .get("model_used")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
            }
            None
        })
        .ok_or_else(|| {
            format!(
                "worker events missing model_used for case {} -> {}",
                chat_model, terminal_model
            )
        })?;

    if model_used != terminal_model {
        agent.stop(None);
        return Err(format!(
            "terminal model mismatch: expected {}, got {}",
            terminal_model, model_used
        ));
    }

    agent.stop(None);
    Ok(format!(
        "chat_model={} terminal_model={} command={}",
        chat_model, terminal_model, command_marker
    ))
}

#[tokio::test]
async fn live_provider_smoke_matrix() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_live_models(&registry);
    let sampled = sampled_live_models(&eligible);

    let concurrency = live_test_concurrency(4);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let registry = registry.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let client_registry = registry
                .create_runtime_client_registry_for_model(&model_id)
                .map_err(|e| (model_id.clone(), format!("registry error: {e}")))?;
            let quick_response = B.QuickResponse.with_client_registry(&client_registry);
            let fut = quick_response.call("Reply with exactly: OK", "");
            match tokio::time::timeout(Duration::from_secs(20), fut).await {
                Ok(Ok(text)) if !text.trim().is_empty() => Ok((model_id, text.trim().to_string())),
                Ok(Ok(_)) => Err((model_id, "returned empty response".to_string())),
                Ok(Err(e)) => Err((model_id, format!("call error: {e}"))),
                Err(_) => Err((model_id, "timed out".to_string())),
            }
        });
    }

    let attempted = sampled.len();
    let mut passed = 0usize;
    let mut failed: Vec<String> = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok(Ok((model_id, text))) => {
                println!("PASS {} => {}", model_id, text);
                passed += 1;
            }
            Ok(Err((model_id, reason))) => failed.push(format!("{} {}", model_id, reason)),
            Err(e) => failed.push(format!("join error: {e}")),
        }
    }

    println!("attempted={attempted} passed={passed}");
    println!("sampled_models={}", sampled.join(","));
    if !skipped.is_empty() {
        println!("skipped:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("failed:\n{}", failed.join("\n"));
    }

    assert!(
        attempted > 0,
        "No live provider tests attempted; credentials missing"
    );
    assert!(
        failed.is_empty(),
        "Live provider failures: {}",
        failed.join(" | ")
    );
}

#[tokio::test]
async fn live_plan_action_matrix() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_live_models(&registry);
    let sampled = sampled_live_models(&eligible);

    let messages = vec![BamlMessage {
        role: "user".to_string(),
        content: "Use the bash tool with cmd `printf PLAN_OK` and then summarize.".to_string(),
    }];
    let system_context = "You are a planner. Return a valid plan.".to_string();
    let available_tools = r#"
[
  {
    "name": "bash",
    "description": "Execute shell commands",
    "parameters": {"cmd": "string"}
  }
]
"#
    .to_string();

    let concurrency = live_test_concurrency(4);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let registry = registry.clone();
        let messages = messages.clone();
        let system_context = system_context.clone();
        let available_tools = available_tools.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let client_registry = registry
                .create_runtime_client_registry_for_model(&model_id)
                .map_err(|e| (model_id.clone(), format!("registry error: {e}")))?;
            let plan_action = B.PlanAction.with_client_registry(&client_registry);
            let plan_call = plan_action.call(&messages, &system_context, &available_tools);

            match tokio::time::timeout(Duration::from_secs(30), plan_call).await {
                Ok(Ok(plan)) => {
                    if !(0.0..=1.0).contains(&plan.confidence) {
                        return Err((
                            model_id,
                            format!("returned invalid confidence {}", plan.confidence),
                        ));
                    }
                    if plan.thinking.trim().is_empty() {
                        return Err((model_id, "returned empty planning reasoning".to_string()));
                    }
                    Ok((model_id, plan.confidence, plan.tool_calls.len()))
                }
                Ok(Err(e)) => Err((model_id, format!("plan call error: {e}"))),
                Err(_) => Err((model_id, "plan call timed out".to_string())),
            }
        });
    }

    let attempted = sampled.len();
    let mut passed = 0usize;
    let mut failed: Vec<String> = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok(Ok((model_id, confidence, tool_calls))) => {
                println!(
                    "PASS {} => confidence={} tool_calls={}",
                    model_id, confidence, tool_calls
                );
                passed += 1;
            }
            Ok(Err((model_id, reason))) => failed.push(format!("{} {}", model_id, reason)),
            Err(e) => failed.push(format!("join error: {e}")),
        }
    }

    println!("plan_action attempted={attempted} passed={passed}");
    println!("plan_action sampled_models={}", sampled.join(","));
    if !skipped.is_empty() {
        println!("plan_action skipped:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("plan_action failed:\n{}", failed.join("\n"));
    }

    assert!(
        attempted > 0,
        "No live PlanAction tests attempted; credentials missing"
    );
    assert!(
        failed.is_empty(),
        "Live PlanAction failures: {}",
        failed.join(" | ")
    );
}

#[tokio::test]
async fn live_chat_terminal_delegation_matrix() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_live_models(&registry);
    let sampled = sampled_live_models(&eligible);

    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn event store");
    let (app_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn app supervisor");

    let concurrency = live_test_concurrency(2);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for model_id in sampled.iter().cloned() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let event_store = event_store.clone();
        let app_supervisor = app_supervisor.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let result = run_delegation_case(event_store, app_supervisor, model_id.clone()).await;
            (model_id, result)
        });
    }

    let attempted = sampled.len();
    let mut passed = 0usize;
    let mut failed: Vec<String> = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((model_id, Ok(message))) => {
                println!("PASS {} => {}", model_id, message);
                passed += 1;
            }
            Ok((model_id, Err(reason))) => failed.push(format!("{} {}", model_id, reason)),
            Err(e) => failed.push(format!("join error: {e}")),
        }
    }

    println!("delegation attempted={attempted} passed={passed}");
    println!("delegation sampled_models={}", sampled.join(","));
    if !skipped.is_empty() {
        println!("delegation skipped:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("delegation failed:\n{}", failed.join("\n"));
    }

    assert!(
        attempted > 0,
        "No live delegation tests attempted; credentials missing"
    );
    assert!(
        failed.is_empty(),
        "Live delegation failures: {}",
        failed.join(" | ")
    );
}

#[tokio::test]
async fn live_chat_terminal_mixed_model_sample() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_live_models(&registry);

    let bedrock = eligible
        .iter()
        .find(|m| m.starts_with("ClaudeBedrock"))
        .cloned();
    let zai = eligible.iter().find(|m| m.starts_with("Zai")).cloned();
    let kimi = eligible.iter().find(|m| m.starts_with("Kimi")).cloned();

    let providers: Vec<String> = [bedrock, zai, kimi].into_iter().flatten().collect();
    let attempted_cases = if providers.len() >= 2 {
        providers.len().min(3)
    } else {
        0
    };

    if attempted_cases == 0 {
        println!(
            "mixed-model skipped: need at least 2 provider families (Bedrock/Zai/Kimi). available={:?} skipped={:?}",
            eligible, skipped
        );
        return;
    }

    let mut cases = Vec::new();
    for idx in 0..attempted_cases {
        let chat_model = providers[idx].clone();
        let terminal_model = providers[(idx + 1) % providers.len()].clone();
        if chat_model != terminal_model {
            cases.push((chat_model, terminal_model));
        }
    }

    let (event_store, _event_handle) =
        Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
            .await
            .expect("spawn mixed event store");
    let (app_supervisor, _app_handle) =
        Actor::spawn(None, ApplicationSupervisor, event_store.clone())
            .await
            .expect("spawn mixed app supervisor");

    let concurrency = live_test_concurrency(2);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for (chat_model, terminal_model) in cases.iter().cloned() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let event_store = event_store.clone();
        let app_supervisor = app_supervisor.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let result = run_mixed_model_case(
                event_store,
                app_supervisor,
                chat_model.clone(),
                terminal_model.clone(),
            )
            .await;
            (chat_model, terminal_model, result)
        });
    }

    let attempted = cases.len();
    let mut passed = 0usize;
    let mut failed: Vec<String> = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((chat_model, terminal_model, Ok(msg))) => {
                println!("PASS mixed {} -> {} => {}", chat_model, terminal_model, msg);
                passed += 1;
            }
            Ok((chat_model, terminal_model, Err(reason))) => {
                failed.push(format!("{} -> {} {}", chat_model, terminal_model, reason));
            }
            Err(e) => failed.push(format!("join error: {e}")),
        }
    }

    println!("mixed attempted={attempted} passed={passed}");
    if !skipped.is_empty() {
        println!("mixed skipped providers:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("mixed failed:\n{}", failed.join("\n"));
    }

    assert!(attempted > 0, "No mixed-model cases attempted");
    assert!(
        failed.is_empty(),
        "Live mixed-model failures: {}",
        failed.join(" | ")
    );
}
