use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::types::{Action, Message as BamlMessage};
use sandbox::baml_client::B;
use sandbox::runtime_env::ensure_tls_cert_env;
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
async fn live_decide_matrix() {
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
            let result = run_with_rate_limit_retry(&format!("decide:{model_label}"), || {
                let registry = registry.clone();
                let model_id = model_id.clone();
                let messages = messages.clone();
                let system_context = system_context.clone();
                let available_tools = available_tools.clone();
                async move {
                    let client_registry = registry
                        .create_runtime_client_registry_for_model(&model_id)
                        .map_err(|e| format!("registry error: {e}"))?;
                    let decide = B.Decide.with_client_registry(&client_registry);
                    let decide_call = decide.call(&messages, &system_context, &available_tools);

                    match tokio::time::timeout(Duration::from_secs(30), decide_call).await {
                        Ok(Ok(decision)) => {
                            if matches!(decision.action, Action::Block) {
                                return Err("returned blocked action".to_string());
                            }
                            if matches!(decision.action, Action::ToolCall)
                                && decision.tool_calls.is_empty()
                            {
                                return Err(
                                    "ToolCall action returned with no tool_calls".to_string()
                                );
                            }
                            Ok((format!("{:?}", decision.action), decision.tool_calls.len()))
                        }
                        Ok(Err(e)) => Err(format!("decide call error: {e}")),
                        Err(_) => Err("decide call timed out".to_string()),
                    }
                }
            })
            .await;

            result
                .map(|(action, tool_calls)| (model_id.clone(), action, tool_calls))
                .map_err(|reason| (model_id, reason))
        });
    }

    let attempted = sampled.len();
    let mut passed = 0usize;
    let mut failed: Vec<String> = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok(Ok((model_id, action, tool_calls))) => {
                println!(
                    "PASS {} => action={} tool_calls={}",
                    model_id, action, tool_calls
                );
                passed += 1;
            }
            Ok(Err((model_id, reason))) => failed.push(format!("{} {}", model_id, reason)),
            Err(e) => failed.push(format!("join error: {e}")),
        }
    }

    println!("decide attempted={attempted} passed={passed}");
    println!(
        "decide requested_models={}",
        requested_live_model_targets().join(",")
    );
    println!("decide sampled_models={}", sampled.join(","));
    if !skipped.is_empty() {
        println!("decide skipped:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("decide failed:\n{}", failed.join("\n"));
    }

    assert!(
        attempted > 0,
        "No live Decide tests attempted; credentials missing"
    );
    assert!(
        failed.is_empty(),
        "Live Decide failures: {}",
        failed.join(" | ")
    );
}
