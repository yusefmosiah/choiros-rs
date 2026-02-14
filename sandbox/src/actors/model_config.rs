use crate::baml_client::ClientRegistry;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

pub const REQUIRED_BAML_CLIENT_ALIASES: &[&str] = &["Orchestrator", "FastResponse"];
pub const DEFAULT_MODEL_POLICY_PATH: &str = "config/model-policy.toml";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ModelConfigError {
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("missing API key environment variable: {0}")]
    MissingApiKey(String),
    #[error("no fallback model available")]
    NoFallbackAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    AwsBedrock {
        model: String,
        region: String,
    },
    AnthropicCompatible {
        base_url: String,
        api_key_env: String,
        model: String,
        headers: HashMap<String, String>,
    },
    OpenAiGeneric {
        base_url: String,
        api_key_env: String,
        model: String,
        headers: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub provider: ProviderConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelResolutionSource {
    Request,
    App,
    User,
    EnvDefault,
    Fallback,
}

impl ModelResolutionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::App => "app",
            Self::User => "user",
            Self::EnvDefault => "env_default",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub config: ModelConfig,
    pub source: ModelResolutionSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelResolutionContext {
    pub request_model: Option<String>,
    pub app_preference: Option<String>,
    pub user_preference: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    configs: HashMap<String, ModelConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelPolicy {
    pub default_model: Option<String>,
    pub chat_default_model: Option<String>,
    pub terminal_default_model: Option<String>,
    pub conductor_default_model: Option<String>,
    pub writer_default_model: Option<String>,
    pub researcher_default_model: Option<String>,
    pub summarizer_default_model: Option<String>,
    pub allow_request_override: Option<bool>,
    pub allowed_models: Option<Vec<String>>,
    pub chat_allowed_models: Option<Vec<String>>,
    pub terminal_allowed_models: Option<Vec<String>>,
    pub conductor_allowed_models: Option<Vec<String>>,
    pub writer_allowed_models: Option<Vec<String>>,
    pub researcher_allowed_models: Option<Vec<String>>,
    pub summarizer_allowed_models: Option<Vec<String>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            configs: default_model_configs(),
        }
    }

    pub fn get(&self, model_id: &str) -> Option<&ModelConfig> {
        canonical_model_id(model_id).and_then(|id| self.configs.get(id))
    }

    pub fn available_model_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.configs.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn resolve(
        &self,
        context: &ModelResolutionContext,
    ) -> Result<ResolvedModel, ModelConfigError> {
        if let Some(request_model) = context.request_model.as_ref() {
            let resolved = self
                .get(request_model)
                .cloned()
                .ok_or_else(|| ModelConfigError::UnknownModel(request_model.clone()))?;
            return Ok(ResolvedModel {
                config: resolved,
                source: ModelResolutionSource::Request,
            });
        }

        if let Some(app_model) = context.app_preference.as_ref() {
            if let Some(resolved) = self.get(app_model).cloned() {
                return Ok(ResolvedModel {
                    config: resolved,
                    source: ModelResolutionSource::App,
                });
            }
        }

        if let Some(user_model) = context.user_preference.as_ref() {
            if let Some(resolved) = self.get(user_model).cloned() {
                return Ok(ResolvedModel {
                    config: resolved,
                    source: ModelResolutionSource::User,
                });
            }
        }

        if let Ok(default_model) = std::env::var("CHOIR_DEFAULT_MODEL") {
            if let Some(resolved) = self.get(&default_model).cloned() {
                return Ok(ResolvedModel {
                    config: resolved,
                    source: ModelResolutionSource::EnvDefault,
                });
            }
        }

        self.get("ClaudeBedrockSonnet45")
            .cloned()
            .map(|config| ResolvedModel {
                config,
                source: ModelResolutionSource::Fallback,
            })
            .ok_or(ModelConfigError::NoFallbackAvailable)
    }

    pub fn create_client_registry_for_model(
        &self,
        model_id: &str,
        aliases: &[&str],
    ) -> Result<ClientRegistry, ModelConfigError> {
        let config = self
            .get(model_id)
            .cloned()
            .ok_or_else(|| ModelConfigError::UnknownModel(model_id.to_string()))?;
        create_client_registry_for_config(&config, aliases)
    }

    pub fn create_runtime_client_registry_for_model(
        &self,
        model_id: &str,
    ) -> Result<ClientRegistry, ModelConfigError> {
        self.create_client_registry_for_model(model_id, REQUIRED_BAML_CLIENT_ALIASES)
    }

    /// Creates a ClientRegistry with semantic role mapping based on model capabilities.
    /// High-quality models (Opus, Sonnet) are registered as "Orchestrator".
    /// Fast/cheap models (Haiku, GLM Flash, GLM Air) are registered as "FastResponse".
    pub fn create_client_registry_with_role_mapping(
        &self,
        model_id: &str,
    ) -> Result<ClientRegistry, ModelConfigError> {
        let config = self
            .get(model_id)
            .cloned()
            .ok_or_else(|| ModelConfigError::UnknownModel(model_id.to_string()))?;

        let mut registry = ClientRegistry::new();

        // Determine which semantic role this model should be registered as
        let role_alias = if is_high_quality_model(model_id) {
            "Orchestrator"
        } else {
            "FastResponse"
        };

        // Register the model under its semantic role
        add_provider_client(&mut registry, role_alias, &config.provider)?;

        // Also register under its actual ID for direct access if needed
        if role_alias != config.id {
            add_provider_client(&mut registry, &config.id, &config.provider)?;
        }

        Ok(registry)
    }

    pub fn resolve_for_role(
        &self,
        role: &str,
        context: &ModelResolutionContext,
    ) -> Result<ResolvedModel, ModelConfigError> {
        let policy = load_model_policy();
        let allow_request_override = policy.allow_request_override.unwrap_or(true);
        let scoped_request = if allow_request_override {
            context.request_model.clone()
        } else {
            None
        };

        let role_default = match role {
            "chat" => policy.chat_default_model.clone(),
            "terminal" => policy.terminal_default_model.clone(),
            "conductor" => policy.conductor_default_model.clone(),
            "writer" => policy.writer_default_model.clone(),
            "researcher" => policy.researcher_default_model.clone(),
            "summarizer" => policy.summarizer_default_model.clone(),
            _ => None,
        };

        let mut resolved = self.resolve(&ModelResolutionContext {
            request_model: scoped_request,
            app_preference: context
                .app_preference
                .clone()
                .or(role_default)
                .or(policy.default_model.clone()),
            user_preference: context.user_preference.clone(),
        })?;

        let is_allowed = |model_id: &str| {
            let in_global = policy
                .allowed_models
                .as_ref()
                .map(|models| models.iter().any(|m| m == model_id))
                .unwrap_or(true);
            let in_role = match role {
                "chat" => policy
                    .chat_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                "terminal" => policy
                    .terminal_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                "conductor" => policy
                    .conductor_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                "writer" => policy
                    .writer_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                "researcher" => policy
                    .researcher_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                "summarizer" => policy
                    .summarizer_allowed_models
                    .as_ref()
                    .map(|models| models.iter().any(|m| m == model_id))
                    .unwrap_or(true),
                _ => true,
            };
            in_global && in_role
        };

        if !is_allowed(&resolved.config.id) {
            if let Some(fallback) = self
                .available_model_ids()
                .into_iter()
                .find(|candidate| is_allowed(candidate))
                .and_then(|candidate| self.get(&candidate).cloned())
            {
                resolved = ResolvedModel {
                    config: fallback,
                    source: ModelResolutionSource::Fallback,
                };
            } else {
                return Err(ModelConfigError::NoFallbackAvailable);
            }
        }

        Ok(resolved)
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn load_model_policy() -> ModelPolicy {
    let explicit_path = std::env::var("CHOIR_MODEL_POLICY_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);

    let path = explicit_path
        .or_else(|| find_default_model_policy_path(DEFAULT_MODEL_POLICY_PATH))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_POLICY_PATH));

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "Failed to load model policy file; using defaults"
            );
            return ModelPolicy::default();
        }
    };
    toml::from_str(&content).unwrap_or_else(|err| {
        tracing::warn!(
            path = %path.display(),
            error = %err,
            "Failed to parse model policy TOML; using defaults"
        );
        ModelPolicy::default()
    })
}

fn find_default_model_policy_path(relative_path: &str) -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join(relative_path);
        if candidate.exists() && candidate.is_file() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

pub fn create_client_registry_for_config(
    config: &ModelConfig,
    aliases: &[&str],
) -> Result<ClientRegistry, ModelConfigError> {
    let mut registry = ClientRegistry::new();
    let mut all_aliases: Vec<String> = aliases.iter().map(|v| (*v).to_string()).collect();
    if !all_aliases.iter().any(|alias| alias == &config.id) {
        all_aliases.push(config.id.clone());
    }

    for alias in all_aliases {
        add_provider_client(&mut registry, &alias, &config.provider)?;
    }

    Ok(registry)
}

fn add_provider_client(
    registry: &mut ClientRegistry,
    client_name: &str,
    provider: &ProviderConfig,
) -> Result<(), ModelConfigError> {
    match provider {
        ProviderConfig::AwsBedrock { model, region } => {
            let mut options = HashMap::new();
            options.insert("model".to_string(), json!(model));
            options.insert("region".to_string(), json!(region));
            registry.add_llm_client(client_name, "aws-bedrock", options);
            Ok(())
        }
        ProviderConfig::AnthropicCompatible {
            base_url,
            api_key_env,
            model,
            headers,
        } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(base_url));
            options.insert("model".to_string(), json!(model));
            if !headers.is_empty() {
                options.insert("headers".to_string(), json!(headers));
            }
            registry.add_llm_client(client_name, "anthropic", options);
            Ok(())
        }
        ProviderConfig::OpenAiGeneric {
            base_url,
            api_key_env,
            model,
            headers,
        } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(base_url));
            options.insert("model".to_string(), json!(model));
            if !headers.is_empty() {
                options.insert("headers".to_string(), json!(headers));
            }
            registry.add_llm_client(client_name, "openai-generic", options);
            Ok(())
        }
    }
}

/// Determines if a model is high-quality (suitable for Orchestrator role).
/// High-quality models: Opus, Sonnet variants
/// Fast/cheap models: Haiku, GLM Flash, GLM Air
fn is_high_quality_model(model_id: &str) -> bool {
    match model_id {
        "ClaudeBedrockOpus46" | "ClaudeBedrockSonnet45" => true,
        "ClaudeBedrock" | "ClaudeBedrockHaiku45" => false,
        "ZaiGLM47" => false,
        "ZaiGLM47Flash" | "GLM47Flash" => false,
        "ZaiGLM47Air" => false,
        "KimiK25" | "KimiK25Fallback" => true,
        _ => {
            // Default: check if model name contains quality indicators
            let id_lower = model_id.to_lowercase();
            id_lower.contains("opus") || id_lower.contains("sonnet")
        }
    }
}

pub fn canonical_model_id(model_id: &str) -> Option<&'static str> {
    match model_id {
        "ClaudeBedrock" => Some("ClaudeBedrockSonnet45"),
        "GLM47" => Some("ZaiGLM47"),
        "GLM47Flash" => Some("ZaiGLM47Flash"),
        "KimiK25OpenAI" => Some("KimiK25"),
        "KimiK25" => Some("KimiK25"),
        "KimiK25Fallback" => Some("KimiK25Fallback"),
        "ClaudeBedrockOpus46" => Some("ClaudeBedrockOpus46"),
        "ClaudeBedrockSonnet45" => Some("ClaudeBedrockSonnet45"),
        "ClaudeBedrockHaiku45" => Some("ClaudeBedrockHaiku45"),
        "ZaiGLM47" => Some("ZaiGLM47"),
        "ZaiGLM47Flash" => Some("ZaiGLM47Flash"),
        "ZaiGLM47Air" => Some("ZaiGLM47Air"),
        _ => None,
    }
}

fn default_model_configs() -> HashMap<String, ModelConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        "ClaudeBedrockOpus46".to_string(),
        ModelConfig {
            id: "ClaudeBedrockOpus46".to_string(),
            name: "Claude Opus 4.6 (Bedrock)".to_string(),
            provider: ProviderConfig::AwsBedrock {
                model: "us.anthropic.claude-opus-4-6-v1".to_string(),
                region: "us-east-1".to_string(),
            },
        },
    );
    configs.insert(
        "ClaudeBedrockSonnet45".to_string(),
        ModelConfig {
            id: "ClaudeBedrockSonnet45".to_string(),
            name: "Claude Sonnet 4.5 (Bedrock)".to_string(),
            provider: ProviderConfig::AwsBedrock {
                model: "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string(),
                region: "us-east-1".to_string(),
            },
        },
    );
    configs.insert(
        "ClaudeBedrockHaiku45".to_string(),
        ModelConfig {
            id: "ClaudeBedrockHaiku45".to_string(),
            name: "Claude Haiku 4.5 (Bedrock)".to_string(),
            provider: ProviderConfig::AwsBedrock {
                model: "us.anthropic.claude-haiku-4-5-20251001-v1:0".to_string(),
                region: "us-east-1".to_string(),
            },
        },
    );
    configs.insert(
        "ZaiGLM47".to_string(),
        ModelConfig {
            id: "ZaiGLM47".to_string(),
            name: "GLM 4.7 (Z.ai)".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.z.ai/api/anthropic".to_string(),
                api_key_env: "ZAI_API_KEY".to_string(),
                model: "glm-4.7".to_string(),
                headers: HashMap::new(),
            },
        },
    );
    configs.insert(
        "ZaiGLM47Flash".to_string(),
        ModelConfig {
            id: "ZaiGLM47Flash".to_string(),
            name: "GLM 4.7 Flash (Z.ai)".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.z.ai/api/anthropic".to_string(),
                api_key_env: "ZAI_API_KEY".to_string(),
                model: "glm-4.7-flash".to_string(),
                headers: HashMap::new(),
            },
        },
    );
    configs.insert(
        "ZaiGLM47Air".to_string(),
        ModelConfig {
            id: "ZaiGLM47Air".to_string(),
            name: "GLM 4.5 Air (Z.ai)".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.z.ai/api/anthropic".to_string(),
                api_key_env: "ZAI_API_KEY".to_string(),
                model: "glm-4.5-air".to_string(),
                headers: HashMap::new(),
            },
        },
    );
    configs.insert(
        "KimiK25".to_string(),
        ModelConfig {
            id: "KimiK25".to_string(),
            name: "Kimi K2.5 (coding)".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.kimi.com/coding/".to_string(),
                api_key_env: "ANTHROPIC_API_KEY".to_string(),
                model: "kimi-for-coding/k2p5".to_string(),
                headers: {
                    let mut headers = HashMap::new();
                    headers.insert("User-Agent".to_string(), "claude-code/1.0".to_string());
                    headers
                },
            },
        },
    );
    configs.insert(
        "KimiK25Fallback".to_string(),
        ModelConfig {
            id: "KimiK25Fallback".to_string(),
            name: "Kimi K2.5 Fallback".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.kimi.com/coding/".to_string(),
                api_key_env: "ANTHROPIC_API_KEY".to_string(),
                model: "kimi-k2.5".to_string(),
                headers: {
                    let mut headers = HashMap::new();
                    headers.insert("User-Agent".to_string(), "claude-code/1.0".to_string());
                    headers
                },
            },
        },
    );
    configs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_model_resolution_priority() {
        let registry = ModelRegistry::new();
        let ctx = ModelResolutionContext {
            request_model: Some("ZaiGLM47Flash".to_string()),
            app_preference: Some("ClaudeBedrockSonnet45".to_string()),
            user_preference: Some("ClaudeBedrockSonnet45".to_string()),
        };

        let resolved = registry.resolve(&ctx).expect("resolve should succeed");
        assert_eq!(resolved.config.id, "ZaiGLM47Flash");
        assert_eq!(resolved.source, ModelResolutionSource::Request);
    }

    #[test]
    fn test_model_resolution_legacy_aliases() {
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.get("ClaudeBedrock").map(|cfg| cfg.id.clone()),
            Some("ClaudeBedrockSonnet45".to_string())
        );
        assert_eq!(
            registry.get("GLM47").map(|cfg| cfg.id.clone()),
            Some("ZaiGLM47".to_string())
        );
    }

    #[test]
    fn test_invalid_model_returns_error() {
        let registry = ModelRegistry::new();
        let ctx = ModelResolutionContext {
            request_model: Some("NotARealModel".to_string()),
            ..Default::default()
        };
        let result = registry.resolve(&ctx);
        assert!(matches!(result, Err(ModelConfigError::UnknownModel(_))));
    }

    #[test]
    fn test_create_client_registry_requires_env_for_anthropic_compatible() {
        let config = ModelConfig {
            id: "TestMissingEnv".to_string(),
            name: "Test missing env".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://example.invalid/anthropic".to_string(),
                api_key_env: "CHOIR_TEST_MISSING_API_KEY_DO_NOT_SET".to_string(),
                model: "test-model".to_string(),
                headers: HashMap::new(),
            },
        };
        let missing = create_client_registry_for_config(&config, &["ClaudeBedrock"]);
        assert!(matches!(missing, Err(ModelConfigError::MissingApiKey(_))));
    }

    #[test]
    fn test_create_client_registry_with_env() {
        let config = ModelConfig {
            id: "TestPathEnv".to_string(),
            name: "Test existing env".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://example.invalid/anthropic".to_string(),
                api_key_env: "PATH".to_string(),
                model: "test-model".to_string(),
                headers: HashMap::new(),
            },
        };
        let result = create_client_registry_for_config(&config, &["ClaudeBedrock"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_model_resolution_uses_env_default_when_no_request_or_app() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let registry = ModelRegistry::new();
        let previous = std::env::var("CHOIR_DEFAULT_MODEL").ok();
        std::env::set_var("CHOIR_DEFAULT_MODEL", "GLM47");
        let resolved = registry
            .resolve(&ModelResolutionContext::default())
            .expect("resolve should use env default");
        assert_eq!(resolved.config.id, "ZaiGLM47");
        assert_eq!(resolved.source, ModelResolutionSource::EnvDefault);
        if let Some(value) = previous {
            std::env::set_var("CHOIR_DEFAULT_MODEL", value);
        } else {
            std::env::remove_var("CHOIR_DEFAULT_MODEL");
        }
    }

    #[test]
    fn test_app_preference_beats_env_default() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let registry = ModelRegistry::new();
        let previous = std::env::var("CHOIR_DEFAULT_MODEL").ok();
        std::env::set_var("CHOIR_DEFAULT_MODEL", "ZaiGLM47Flash");
        let resolved = registry
            .resolve(&ModelResolutionContext {
                app_preference: Some("ClaudeBedrockSonnet45".to_string()),
                ..Default::default()
            })
            .expect("resolve should use app preference");
        assert_eq!(resolved.config.id, "ClaudeBedrockSonnet45");
        assert_eq!(resolved.source, ModelResolutionSource::App);
        if let Some(value) = previous {
            std::env::set_var("CHOIR_DEFAULT_MODEL", value);
        } else {
            std::env::remove_var("CHOIR_DEFAULT_MODEL");
        }
    }

    #[test]
    fn test_create_client_registry_for_legacy_alias() {
        let registry = ModelRegistry::new();
        let result = registry.create_client_registry_for_model("ClaudeBedrock", &["ClaudeBedrock"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_model_resolution_source_labels() {
        assert_eq!(ModelResolutionSource::Request.as_str(), "request");
        assert_eq!(ModelResolutionSource::App.as_str(), "app");
        assert_eq!(ModelResolutionSource::User.as_str(), "user");
        assert_eq!(ModelResolutionSource::EnvDefault.as_str(), "env_default");
        assert_eq!(ModelResolutionSource::Fallback.as_str(), "fallback");
    }

    #[test]
    fn test_resolve_for_role_respects_override_denial() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let policy_path = temp_dir.path().join("model-policy.toml");
        std::fs::write(
            &policy_path,
            r#"
chat_default_model = "ClaudeBedrockSonnet45"
allow_request_override = false
"#,
        )
        .expect("write policy");

        let previous_policy = std::env::var("CHOIR_MODEL_POLICY_PATH").ok();
        std::env::set_var(
            "CHOIR_MODEL_POLICY_PATH",
            policy_path.to_string_lossy().to_string(),
        );

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_role(
                "chat",
                &ModelResolutionContext {
                    request_model: Some("ZaiGLM47".to_string()),
                    app_preference: None,
                    user_preference: None,
                },
            )
            .expect("resolve_for_role");

        assert_eq!(resolved.config.id, "ClaudeBedrockSonnet45");

        if let Some(value) = previous_policy {
            std::env::set_var("CHOIR_MODEL_POLICY_PATH", value);
        } else {
            std::env::remove_var("CHOIR_MODEL_POLICY_PATH");
        }
    }

    #[test]
    fn test_resolve_for_role_respects_allowed_model_lists() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let policy_path = temp_dir.path().join("model-policy.toml");
        std::fs::write(
            &policy_path,
            r#"
allowed_models = ["ClaudeBedrockSonnet45", "KimiK25"]
terminal_allowed_models = ["KimiK25"]
terminal_default_model = "KimiK25"
"#,
        )
        .expect("write policy");

        let previous_policy = std::env::var("CHOIR_MODEL_POLICY_PATH").ok();
        std::env::set_var(
            "CHOIR_MODEL_POLICY_PATH",
            policy_path.to_string_lossy().to_string(),
        );

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_role(
                "terminal",
                &ModelResolutionContext {
                    request_model: Some("ClaudeBedrockSonnet45".to_string()),
                    app_preference: None,
                    user_preference: None,
                },
            )
            .expect("resolve_for_role");

        assert_eq!(resolved.config.id, "KimiK25");

        if let Some(value) = previous_policy {
            std::env::set_var("CHOIR_MODEL_POLICY_PATH", value);
        } else {
            std::env::remove_var("CHOIR_MODEL_POLICY_PATH");
        }
    }

    #[test]
    fn test_resolve_for_role_uses_conductor_writer_researcher_and_summarizer_defaults() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let policy_path = temp_dir.path().join("model-policy.toml");
        std::fs::write(
            &policy_path,
            r#"
default_model = "ClaudeBedrockSonnet45"
conductor_default_model = "ClaudeBedrockOpus46"
writer_default_model = "KimiK25"
researcher_default_model = "ZaiGLM47"
summarizer_default_model = "ZaiGLM47Flash"
conductor_allowed_models = ["ClaudeBedrockOpus46"]
writer_allowed_models = ["KimiK25"]
researcher_allowed_models = ["ZaiGLM47"]
summarizer_allowed_models = ["ZaiGLM47Flash"]
"#,
        )
        .expect("write policy");

        let previous_policy = std::env::var("CHOIR_MODEL_POLICY_PATH").ok();
        std::env::set_var(
            "CHOIR_MODEL_POLICY_PATH",
            policy_path.to_string_lossy().to_string(),
        );

        let registry = ModelRegistry::new();
        let conductor = registry
            .resolve_for_role("conductor", &ModelResolutionContext::default())
            .expect("resolve conductor");
        assert_eq!(conductor.config.id, "ClaudeBedrockOpus46");

        let researcher = registry
            .resolve_for_role("researcher", &ModelResolutionContext::default())
            .expect("resolve researcher");
        assert_eq!(researcher.config.id, "ZaiGLM47");

        let writer = registry
            .resolve_for_role("writer", &ModelResolutionContext::default())
            .expect("resolve writer");
        assert_eq!(writer.config.id, "KimiK25");

        let summarizer = registry
            .resolve_for_role("summarizer", &ModelResolutionContext::default())
            .expect("resolve summarizer");
        assert_eq!(summarizer.config.id, "ZaiGLM47Flash");

        if let Some(value) = previous_policy {
            std::env::set_var("CHOIR_MODEL_POLICY_PATH", value);
        } else {
            std::env::remove_var("CHOIR_MODEL_POLICY_PATH");
        }
    }

    #[test]
    fn test_load_model_policy_discovers_config_in_ancestor_dir() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let previous_policy = std::env::var("CHOIR_MODEL_POLICY_PATH").ok();
        std::env::remove_var("CHOIR_MODEL_POLICY_PATH");

        let original_cwd = std::env::current_dir().expect("cwd");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let nested = repo_root.join("sandbox");
        let config_dir = repo_root.join("config");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::create_dir_all(&config_dir).expect("create config dir");

        let policy_path = config_dir.join("model-policy.toml");
        std::fs::write(
            &policy_path,
            r#"
summarizer_default_model = "ZaiGLM47Flash"
summarizer_allowed_models = ["ZaiGLM47Flash"]
"#,
        )
        .expect("write policy");

        std::env::set_current_dir(&nested).expect("set nested cwd");
        let policy = load_model_policy();
        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        assert_eq!(
            policy.summarizer_default_model.as_deref(),
            Some("ZaiGLM47Flash")
        );

        if let Some(value) = previous_policy {
            std::env::set_var("CHOIR_MODEL_POLICY_PATH", value);
        } else {
            std::env::remove_var("CHOIR_MODEL_POLICY_PATH");
        }
    }
}
