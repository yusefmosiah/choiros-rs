use sandbox::actors::model_config::{ModelRegistry, ProviderConfig};
use sandbox::baml_client::B;
use std::time::Duration;

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

#[tokio::test]
async fn live_provider_smoke_matrix() {
    let _ = dotenvy::dotenv();
    let registry = ModelRegistry::new();

    let mut attempted = 0usize;
    let mut passed = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for model_id in registry.available_model_ids() {
        let Some(config) = registry.get(&model_id) else {
            failed.push(format!("{model_id} not found in registry"));
            continue;
        };
        let missing_for_case: Vec<String> = match &config.provider {
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
        };

        if !missing_for_case.is_empty() {
            skipped.push(format!(
                "{} (missing env: {})",
                model_id,
                missing_for_case.join(",")
            ));
            continue;
        }

        attempted += 1;

        let client_registry = match registry.create_runtime_client_registry_for_model(&model_id) {
            Ok(r) => r,
            Err(e) => {
                failed.push(format!("{} registry error: {e}", model_id));
                continue;
            }
        };

        let quick_response = B.QuickResponse.with_client_registry(&client_registry);
        let fut = quick_response.call("Reply with exactly: OK", "");

        match tokio::time::timeout(Duration::from_secs(20), fut).await {
            Ok(Ok(text)) => {
                if text.trim().is_empty() {
                    failed.push(format!("{} returned empty response", model_id));
                } else {
                    println!("PASS {} => {}", model_id, text.trim());
                    passed += 1;
                }
            }
            Ok(Err(e)) => {
                failed.push(format!("{} call error: {}", model_id, e));
            }
            Err(_) => {
                failed.push(format!("{} timed out", model_id));
            }
        }
    }

    println!("attempted={attempted} passed={passed}");
    if !skipped.is_empty() {
        println!("skipped:\n{}", skipped.join("\n"));
    }
    if !failed.is_empty() {
        println!("failed:\n{}", failed.join("\n"));
    }

    assert!(attempted > 0, "No live provider tests attempted; credentials missing");
    assert!(failed.is_empty(), "Live provider failures: {}", failed.join(" | "));
}
