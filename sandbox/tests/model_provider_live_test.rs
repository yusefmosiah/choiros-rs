use ractor::Actor;
use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg};
use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::types::Message as BamlMessage;
use sandbox::baml_client::B;
use sandbox::runtime_env::ensure_tls_cert_env;
use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;

const DEFAULT_LIVE_MODEL_TARGETS: &[&str] = &["ZaiGLM47", "ZaiGLM47Flash", "KimiK25"];

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

fn missing_env_for_provider(provider: &ProviderConfig) -> Vec<String> {
    match provider {
        ProviderConfig::AwsBedrock { .. } => {
            let tls_ready = ensure_tls_cert_env().is_some();
            if bedrock_auth_present() && tls_ready {
                Vec::new()
            } else {
                let mut missing = vec![
                    "AWS_BEARER_TOKEN_BEDROCK or AWS_PROFILE or AWS_ACCESS_KEY_ID+AWS_SECRET_ACCESS_KEY"
                        .to_string(),
                ];
                if !tls_ready {
                    missing.push("SSL_CERT_FILE (or NIX_SSL_CERT_FILE)".to_string());
                }
                missing
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

fn live_retry_attempts() -> usize {
    std::env::var("CHOIR_LIVE_TEST_RETRIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 8)
}

fn live_retry_base_delay_ms() -> u64 {
    std::env::var("CHOIR_LIVE_TEST_RETRY_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1_200)
        .clamp(100, 30_000)
}

fn is_rate_limited_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("ratelimited")
        || lower.contains("rate limited")
        || lower.contains("too many requests")
        || lower.contains("status code: 429")
}

async fn run_with_rate_limit_retry<T, F, Fut>(label: &str, mut op: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let attempts = live_retry_attempts();
    let base_delay_ms = live_retry_base_delay_ms();
    let mut last_error = String::new();

    for attempt in 1..=attempts {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_error = err.clone();
                let retryable = is_rate_limited_error(&err);
                if attempt < attempts && retryable {
                    let delay = base_delay_ms.saturating_mul(attempt as u64);
                    println!(
                        "RETRY {} attempt {}/{} after {}ms due to rate limit: {}",
                        label, attempt, attempts, delay, err
                    );
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error)
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

fn requested_live_model_targets() -> Vec<String> {
    if let Ok(raw) = std::env::var("CHOIR_LIVE_MODEL_IDS") {
        let mut parsed = raw
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        parsed.dedup();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    DEFAULT_LIVE_MODEL_TARGETS
        .iter()
        .map(|m| (*m).to_string())
        .collect()
}

fn sampled_live_models(eligible: &[String]) -> Vec<String> {
    let requested = requested_live_model_targets();
    let mut selected = Vec::new();
    for model_id in requested {
        if eligible.iter().any(|eligible_id| eligible_id == &model_id) {
            selected.push(model_id);
        }
    }
    selected
}

async fn run_terminal_delegation_case(
    event_store: ractor::ActorRef<EventStoreMsg>,
    app_supervisor: ractor::ActorRef<sandbox::supervisor::ApplicationSupervisorMsg>,
    model_id: String,
) -> Result<String, String> {
    let actor_id = format!("terminal-live-{model_id}");
    let session_id = format!("session-{model_id}");
    let thread_id = format!("thread-{model_id}");

    let marker = format!("CHOIR_TOOL_OK_{}", model_id.to_lowercase());

    // Delegate terminal task directly through supervisor
    let task = ractor::call!(app_supervisor, |reply| {
        ApplicationSupervisorMsg::DelegateTerminalTask {
            terminal_id: format!("term-{actor_id}"),
            actor_id: actor_id.clone(),
            user_id: "live-test-user".to_string(),
            shell: "/bin/zsh".to_string(),
            working_dir: ".".to_string(),
            command: format!("printf {marker}"),
            timeout_ms: Some(20_000),
            model_override: Some(model_id.clone()),
            objective: Some(format!("Test terminal delegation with model {model_id}")),
            session_id: Some(session_id.clone()),
            thread_id: Some(thread_id.clone()),
            reply,
        }
    })
    .map_err(|e| format!("delegate task rpc failed: {e}"))?
    .map_err(|e| format!("delegate task failed: {e}"))?;

    // Wait for task completion
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        let events = ractor::call!(event_store, |reply| {
            EventStoreMsg::GetEventsForActorWithScope {
                actor_id: actor_id.clone(),
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                since_seq: 0,
                reply,
            }
        })
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();

        let has_terminal = events.iter().any(|e| {
            e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED
                || e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_FAILED
        });

        if has_terminal {
            return Ok(format!(
                "terminal delegation accepted with correlation: {}",
                task.correlation_id
            ));
        }

        sleep(Duration::from_millis(100)).await;
    }

    Err("timeout waiting for terminal task completion".to_string())
}

#[tokio::test]
async fn live_provider_smoke_matrix() {
    let _ = dotenvy::dotenv();
    ensure_tls_cert_env();
    let registry = ModelRegistry::new();
    let (eligible, skipped) = available_live_models(&registry);
    let sampled = sampled_live_models(&eligible);

    let concurrency = live_test_concurrency(2);
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
            let model_label = model_id.clone();
            let result = run_with_rate_limit_retry(&format!("quick:{model_label}"), || {
                let registry = registry.clone();
                let model_id = model_id.clone();
                async move {
                    let client_registry = registry
                        .create_runtime_client_registry_for_model(&model_id)
                        .map_err(|e| format!("registry error: {e}"))?;
                    let quick_response = B.QuickResponse.with_client_registry(&client_registry);
                    let fut = quick_response.call("Reply with exactly: OK", "");
                    match tokio::time::timeout(Duration::from_secs(20), fut).await {
                        Ok(Ok(text)) if !text.trim().is_empty() => Ok(text.trim().to_string()),
                        Ok(Ok(_)) => Err("returned empty response".to_string()),
                        Ok(Err(e)) => Err(format!("call error: {e}")),
                        Err(_) => Err("timed out".to_string()),
                    }
                }
            })
            .await;
            result
                .map(|text| (model_id.clone(), text))
                .map_err(|reason| (model_id, reason))
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
    println!(
        "requested_models={}",
        requested_live_model_targets().join(",")
    );
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

    let concurrency = live_test_concurrency(2);
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
            let model_label = model_id.clone();
            let result = run_with_rate_limit_retry(&format!("plan:{model_label}"), || {
                let registry = registry.clone();
                let model_id = model_id.clone();
                let messages = messages.clone();
                let system_context = system_context.clone();
                let available_tools = available_tools.clone();
                async move {
                    let client_registry = registry
                        .create_runtime_client_registry_for_model(&model_id)
                        .map_err(|e| format!("registry error: {e}"))?;
                    let plan_action = B.PlanAction.with_client_registry(&client_registry);
                    let plan_call = plan_action.call(&messages, &system_context, &available_tools);

                    match tokio::time::timeout(Duration::from_secs(30), plan_call).await {
                        Ok(Ok(plan)) => {
                            if !(0.0..=1.0).contains(&plan.confidence) {
                                return Err(format!(
                                    "returned invalid confidence {}",
                                    plan.confidence
                                ));
                            }
                            if plan.thinking.trim().is_empty() {
                                return Err("returned empty planning reasoning".to_string());
                            }
                            Ok((plan.confidence, plan.tool_calls.len()))
                        }
                        Ok(Err(e)) => Err(format!("plan call error: {e}")),
                        Err(_) => Err("plan call timed out".to_string()),
                    }
                }
            })
            .await;

            result
                .map(|(confidence, tool_calls)| (model_id.clone(), confidence, tool_calls))
                .map_err(|reason| (model_id, reason))
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
    println!(
        "plan_action requested_models={}",
        requested_live_model_targets().join(",")
    );
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
async fn live_terminal_delegation_matrix() {
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
            let result =
                run_terminal_delegation_case(event_store, app_supervisor, model_id.clone()).await;
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
    println!(
        "delegation requested_models={}",
        requested_live_model_targets().join(",")
    );
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
