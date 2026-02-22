use crate::baml_client::ClientRegistry;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

pub const REQUIRED_BAML_CLIENT_ALIASES: &[&str] = &["Orchestrator", "FastResponse"];
pub const DEFAULT_MODEL_CONFIG_PATH: &str = "sandbox/config/model-catalog.toml";
pub const DEFAULT_MODEL_CATALOG_PATH: &str = DEFAULT_MODEL_CONFIG_PATH;
const BUILTIN_MODEL_CATALOG_TOML: &str = include_str!("../../config/model-catalog.example.toml");

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
    aliases: HashMap<String, String>,
    routing: ModelRoutingConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCatalog {
    pub default_model: Option<String>,
    pub allow_request_override: Option<bool>,
    pub allowed_models: Option<Vec<String>>,
    pub callsite_defaults: Option<HashMap<String, String>>,
    pub models: HashMap<String, ModelCatalogEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCatalogEntry {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub aliases: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ModelRoutingConfig {
    default_model: Option<String>,
    allow_request_override: bool,
    allowed_models: Option<Vec<String>>,
    callsite_defaults: HashMap<String, String>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let (configs, aliases, routing) = load_model_catalog_configs()
            .or_else(|| {
                tracing::warn!("Falling back to built-in model catalog");
                load_model_catalog_configs_from(&built_in_model_catalog())
            })
            .unwrap_or_else(|| {
                tracing::warn!("Built-in model catalog parse failed; registry will be empty");
                (
                    HashMap::new(),
                    HashMap::new(),
                    ModelRoutingConfig::default(),
                )
            });
        Self {
            configs,
            aliases,
            routing,
        }
    }

    pub fn get(&self, model_id: &str) -> Option<&ModelConfig> {
        if let Some(canonical) = self.aliases.get(model_id) {
            return self.configs.get(canonical);
        }
        self.configs.get(model_id)
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

        if let Some(config_default) = self.routing.default_model.as_ref() {
            if let Some(resolved) = self.get(config_default).cloned() {
                return Ok(ResolvedModel {
                    config: resolved,
                    source: ModelResolutionSource::Fallback,
                });
            }
        }

        self.available_model_ids()
            .into_iter()
            .find_map(|id| self.get(&id).cloned())
            .map(|config| ResolvedModel {
                config,
                source: ModelResolutionSource::Fallback,
            })
            .ok_or(ModelConfigError::NoFallbackAvailable)
    }

    pub fn default_model_for_callsite(&self, callsite: &str) -> Option<String> {
        self.routing
            .callsite_defaults
            .get(callsite)
            .and_then(|id| self.get(id))
            .map(|cfg| cfg.id.clone())
            .or_else(|| {
                self.routing
                    .default_model
                    .as_ref()
                    .and_then(|id| self.get(id))
                    .map(|cfg| cfg.id.clone())
            })
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

    pub fn resolve_for_callsite(
        &self,
        callsite: &str,
        context: &ModelResolutionContext,
    ) -> Result<ResolvedModel, ModelConfigError> {
        let scoped_request = if self.routing.allow_request_override {
            context.request_model.clone()
        } else {
            None
        };

        let mut resolved = self.resolve(&ModelResolutionContext {
            request_model: scoped_request,
            app_preference: context
                .app_preference
                .clone()
                .or_else(|| self.routing.callsite_defaults.get(callsite).cloned())
                .or_else(|| self.routing.default_model.clone()),
            user_preference: context.user_preference.clone(),
        })?;

        let is_allowed = |model_id: &str| self.is_allowed_model(model_id);

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

    pub fn resolve_for_role(
        &self,
        role: &str,
        context: &ModelResolutionContext,
    ) -> Result<ResolvedModel, ModelConfigError> {
        self.resolve_for_callsite(role, context)
    }

    fn is_allowed_model(&self, model_id: &str) -> bool {
        let allowlist_matches = |candidate: &str| {
            self.get(candidate)
                .map(|cfg| cfg.id == model_id)
                .unwrap_or(candidate == model_id)
        };
        self.routing
            .allowed_models
            .as_ref()
            .map(|models| models.iter().any(|m| allowlist_matches(m)))
            .unwrap_or(true)
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn load_model_catalog() -> ModelCatalog {
    let explicit_path = std::env::var("CHOIR_MODEL_CONFIG_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("CHOIR_MODEL_CATALOG_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        });

    let path = explicit_path
        .or_else(|| find_default_config_path(DEFAULT_MODEL_CONFIG_PATH))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_CONFIG_PATH));

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "Failed to load model catalog file; using built-in defaults"
            );
            return built_in_model_catalog();
        }
    };
    toml::from_str(&content).unwrap_or_else(|err| {
        tracing::warn!(
            path = %path.display(),
            error = %err,
            "Failed to parse model catalog TOML; using built-in defaults"
        );
        built_in_model_catalog()
    })
}

fn built_in_model_catalog() -> ModelCatalog {
    toml::from_str(BUILTIN_MODEL_CATALOG_TOML).unwrap_or_else(|err| {
        tracing::error!(error = %err, "Failed to parse built-in model catalog");
        ModelCatalog::default()
    })
}

fn find_default_config_path(relative_path: &str) -> Option<PathBuf> {
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
            let (resolved_base_url, api_key, resolved_headers) =
                if let Some((gateway_url, gateway_token, gateway_headers)) =
                    provider_gateway_override(base_url, model, headers)
                {
                    (gateway_url, gateway_token, gateway_headers)
                } else {
                    let api_key = std::env::var(api_key_env)
                        .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
                    (base_url.clone(), api_key, headers.clone())
                };
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(resolved_base_url));
            options.insert("model".to_string(), json!(model));
            if !resolved_headers.is_empty() {
                options.insert("headers".to_string(), json!(resolved_headers));
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
            let (resolved_base_url, api_key, resolved_headers) =
                if let Some((gateway_url, gateway_token, gateway_headers)) =
                    provider_gateway_override(base_url, model, headers)
                {
                    (gateway_url, gateway_token, gateway_headers)
                } else {
                    let api_key = std::env::var(api_key_env)
                        .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
                    (base_url.clone(), api_key, headers.clone())
                };
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(resolved_base_url));
            options.insert("model".to_string(), json!(model));
            if !resolved_headers.is_empty() {
                options.insert("headers".to_string(), json!(resolved_headers));
            }
            registry.add_llm_client(client_name, "openai-generic", options);
            Ok(())
        }
    }
}

fn provider_gateway_override(
    upstream_base_url: &str,
    model: &str,
    headers: &HashMap<String, String>,
) -> Option<(String, String, HashMap<String, String>)> {
    let gateway_base = std::env::var("CHOIR_PROVIDER_GATEWAY_BASE_URL").ok()?;
    let gateway_token = std::env::var("CHOIR_PROVIDER_GATEWAY_TOKEN").ok()?;
    let gateway_base = gateway_base.trim_end_matches('/');
    let mut forwarded_headers = headers.clone();
    forwarded_headers.insert(
        "x-choiros-upstream-base-url".to_string(),
        upstream_base_url.to_string(),
    );
    forwarded_headers.insert("x-choiros-model".to_string(), model.to_string());

    if let Ok(sandbox_id) = std::env::var("CHOIR_SANDBOX_ID") {
        if !sandbox_id.trim().is_empty() {
            forwarded_headers.insert("x-choiros-sandbox-id".to_string(), sandbox_id);
        }
    }
    if let Ok(user_id) = std::env::var("CHOIR_SANDBOX_USER_ID") {
        if !user_id.trim().is_empty() {
            forwarded_headers.insert("x-choiros-user-id".to_string(), user_id);
        }
    }
    if let Ok(role) = std::env::var("CHOIR_SANDBOX_ROLE") {
        if !role.trim().is_empty() {
            forwarded_headers.insert("x-choiros-sandbox-role".to_string(), role);
        }
    }

    let routed_url = format!("{gateway_base}/provider/v1/forward/");
    Some((routed_url, gateway_token, forwarded_headers))
}

fn load_model_catalog_configs() -> Option<(
    HashMap<String, ModelConfig>,
    HashMap<String, String>,
    ModelRoutingConfig,
)> {
    let catalog = load_model_catalog();
    load_model_catalog_configs_from(&catalog)
}

fn load_model_catalog_configs_from(
    catalog: &ModelCatalog,
) -> Option<(
    HashMap<String, ModelConfig>,
    HashMap<String, String>,
    ModelRoutingConfig,
)> {
    if catalog.models.is_empty() {
        return None;
    }

    let mut configs = HashMap::new();
    let mut aliases = HashMap::new();

    for (id, entry) in &catalog.models {
        let Some(config) = model_config_from_catalog_entry(id, entry) else {
            continue;
        };

        aliases.insert(config.id.clone(), config.id.clone());
        if let Some(extra_aliases) = entry.aliases.as_ref() {
            for alias in extra_aliases {
                aliases.insert(alias.clone(), config.id.clone());
            }
        }
        configs.insert(config.id.clone(), config);
    }

    if configs.is_empty() {
        return None;
    }

    let routing = ModelRoutingConfig {
        default_model: catalog.default_model.clone(),
        allow_request_override: catalog.allow_request_override.unwrap_or(true),
        allowed_models: catalog.allowed_models.clone(),
        callsite_defaults: catalog.callsite_defaults.clone().unwrap_or_default(),
    };

    Some((configs, aliases, routing))
}

fn model_config_from_catalog_entry(id: &str, entry: &ModelCatalogEntry) -> Option<ModelConfig> {
    let Some(provider) = entry.provider.as_deref() else {
        tracing::warn!(model_id = %id, "Skipping catalog model with missing provider");
        return None;
    };
    let Some(model) = entry.model.as_deref() else {
        tracing::warn!(model_id = %id, "Skipping catalog model with missing model field");
        return None;
    };

    let provider = match provider {
        "aws-bedrock" => {
            let Some(region) = entry.region.as_deref() else {
                tracing::warn!(
                    model_id = %id,
                    "Skipping aws-bedrock model with missing region"
                );
                return None;
            };
            ProviderConfig::AwsBedrock {
                model: model.to_string(),
                region: region.to_string(),
            }
        }
        "anthropic" | "anthropic-compatible" => {
            let Some(base_url) = entry.base_url.as_deref() else {
                tracing::warn!(
                    model_id = %id,
                    "Skipping anthropic-compatible model with missing base_url"
                );
                return None;
            };
            let Some(api_key_env) = entry.api_key_env.as_deref() else {
                tracing::warn!(
                    model_id = %id,
                    "Skipping anthropic-compatible model with missing api_key_env"
                );
                return None;
            };
            ProviderConfig::AnthropicCompatible {
                base_url: base_url.to_string(),
                api_key_env: api_key_env.to_string(),
                model: model.to_string(),
                headers: entry.headers.clone().unwrap_or_default(),
            }
        }
        "openai-generic" => {
            let Some(base_url) = entry.base_url.as_deref() else {
                tracing::warn!(
                    model_id = %id,
                    "Skipping openai-generic model with missing base_url"
                );
                return None;
            };
            let Some(api_key_env) = entry.api_key_env.as_deref() else {
                tracing::warn!(
                    model_id = %id,
                    "Skipping openai-generic model with missing api_key_env"
                );
                return None;
            };
            ProviderConfig::OpenAiGeneric {
                base_url: base_url.to_string(),
                api_key_env: api_key_env.to_string(),
                model: model.to_string(),
                headers: entry.headers.clone().unwrap_or_default(),
            }
        }
        unknown => {
            tracing::warn!(
                model_id = %id,
                provider = %unknown,
                "Skipping catalog model with unknown provider"
            );
            return None;
        }
    };

    Some(ModelConfig {
        id: id.to_string(),
        name: entry.name.clone().unwrap_or_else(|| id.to_string()),
        provider,
    })
}

/// Determines if a model is high-quality (suitable for Orchestrator role).
/// High-quality models: Opus, Sonnet variants
/// Fast/cheap models: Haiku, GLM Flash, GLM Air
fn is_high_quality_model(model_id: &str) -> bool {
    match model_id {
        "ClaudeBedrockOpus46" | "ClaudeBedrockSonnet46" | "ClaudeBedrockSonnet45" => true,
        "ClaudeBedrock" | "ClaudeBedrockHaiku45" => false,
        "ZaiGLM47" | "ZaiGLM5" => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn set_env(key: &str, value: &str) -> Option<String> {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        previous
    }

    fn clear_env(key: &str) -> Option<String> {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        previous
    }

    fn restore_env(key: &str, previous: Option<String>) {
        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn test_model_resolution_priority() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
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
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.get("ClaudeBedrock").map(|cfg| cfg.id.clone()),
            Some("ClaudeBedrockSonnet45".to_string())
        );
        assert_eq!(
            registry.get("GLM47").map(|cfg| cfg.id.clone()),
            Some("ZaiGLM47".to_string())
        );
        assert_eq!(
            registry
                .get("anthropic.claude-sonnet-4-6")
                .map(|cfg| cfg.id.clone()),
            Some("ClaudeBedrockSonnet46".to_string())
        );
        assert_eq!(
            registry
                .get("global.anthropic.claude-sonnet-4-6")
                .map(|cfg| cfg.id.clone()),
            Some("ClaudeBedrockSonnet46".to_string())
        );
    }

    #[test]
    fn test_model_registry_loads_runtime_config_model() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let catalog_path = temp_dir.path().join("model-catalog.toml");
        std::fs::write(
            &catalog_path,
            r#"
default_model = "ZaiGLM5"

[models.ZaiGLM5]
name = "GLM 5 (Z.ai)"
provider = "anthropic"
base_url = "https://api.z.ai/api/anthropic"
api_key_env = "PATH"
model = "glm-5"
aliases = ["GLM5", "ZaiGLM5"]
"#,
        )
        .expect("write catalog");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &catalog_path.to_string_lossy());

        let registry = ModelRegistry::new();
        assert_eq!(
            registry.get("GLM5").map(|cfg| cfg.id.clone()),
            Some("ZaiGLM5".to_string())
        );
        assert_eq!(
            registry.get("ZaiGLM5").map(|cfg| cfg.id.clone()),
            Some("ZaiGLM5".to_string())
        );

        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_resolve_for_callsite_uses_runtime_config_added_model() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let catalog_path = temp_dir.path().join("model-config.toml");

        std::fs::write(
            &catalog_path,
            r#"
default_model = "ZaiGLM5"
allow_request_override = true
allowed_models = ["ZaiGLM5"]

[callsite_defaults]
chat = "GLM5"

[models.ZaiGLM5]
name = "GLM 5 (Z.ai)"
provider = "anthropic"
base_url = "https://api.z.ai/api/anthropic"
api_key_env = "PATH"
model = "glm-5"
aliases = ["GLM5", "ZaiGLM5"]
"#,
        )
        .expect("write config");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &catalog_path.to_string_lossy());

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_callsite("chat", &ModelResolutionContext::default())
            .expect("resolve_for_callsite");
        assert_eq!(resolved.config.id, "ZaiGLM5");

        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_invalid_model_returns_error() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
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
        restore_env("CHOIR_DEFAULT_MODEL", previous);
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
        restore_env("CHOIR_DEFAULT_MODEL", previous);
    }

    #[test]
    fn test_create_client_registry_for_legacy_alias() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
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
    fn test_resolve_for_callsite_respects_override_denial() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config_path = temp_dir.path().join("model-config.toml");
        std::fs::write(
            &config_path,
            r#"
allow_request_override = false
default_model = "ClaudeBedrockSonnet45"

[callsite_defaults]
chat = "ClaudeBedrockSonnet45"

[models.ClaudeBedrockSonnet45]
name = "Claude Sonnet 4.5 (Bedrock)"
provider = "aws-bedrock"
model = "us.anthropic.claude-sonnet-4-5-20250929-v1:0"
region = "us-east-1"
aliases = ["ClaudeBedrockSonnet45"]

[models.ZaiGLM47]
name = "GLM 4.7 (Z.ai)"
provider = "anthropic"
base_url = "https://api.z.ai/api/anthropic"
api_key_env = "PATH"
model = "glm-4.7"
aliases = ["ZaiGLM47"]
"#,
        )
        .expect("write config");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &config_path.to_string_lossy());

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_callsite(
                "chat",
                &ModelResolutionContext {
                    request_model: Some("ZaiGLM47".to_string()),
                    app_preference: None,
                    user_preference: None,
                },
            )
            .expect("resolve_for_callsite");

        assert_eq!(resolved.config.id, "ClaudeBedrockSonnet45");
        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_resolve_for_callsite_respects_global_allowlist() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config_path = temp_dir.path().join("model-config.toml");
        std::fs::write(
            &config_path,
            r#"
allowed_models = ["KimiK25"]
default_model = "KimiK25"

[callsite_defaults]
terminal = "KimiK25"

[models.KimiK25]
name = "Kimi K2.5 (coding)"
provider = "anthropic"
base_url = "https://api.kimi.com/coding/"
api_key_env = "PATH"
model = "kimi-for-coding/k2p5"
aliases = ["KimiK25"]

[models.ClaudeBedrockSonnet45]
name = "Claude Sonnet 4.5 (Bedrock)"
provider = "aws-bedrock"
model = "us.anthropic.claude-sonnet-4-5-20250929-v1:0"
region = "us-east-1"
aliases = ["ClaudeBedrockSonnet45"]
"#,
        )
        .expect("write config");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &config_path.to_string_lossy());

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_callsite(
                "terminal",
                &ModelResolutionContext {
                    request_model: Some("ClaudeBedrockSonnet45".to_string()),
                    app_preference: None,
                    user_preference: None,
                },
            )
            .expect("resolve_for_callsite");

        assert_eq!(resolved.config.id, "KimiK25");
        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_resolve_for_callsite_accepts_canonicalized_allowlist_entries() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config_path = temp_dir.path().join("model-config.toml");
        std::fs::write(
            &config_path,
            r#"
allowed_models = ["us.anthropic.claude-sonnet-4-6"]
default_model = "anthropic.claude-sonnet-4-6"

[callsite_defaults]
terminal = "global.anthropic.claude-sonnet-4-6"

[models.ClaudeBedrockSonnet46]
name = "Claude Sonnet 4.6 (Bedrock)"
provider = "aws-bedrock"
model = "us.anthropic.claude-sonnet-4-6"
region = "us-east-1"
aliases = ["anthropic.claude-sonnet-4-6", "us.anthropic.claude-sonnet-4-6", "global.anthropic.claude-sonnet-4-6"]
"#,
        )
        .expect("write config");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &config_path.to_string_lossy());

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_callsite("terminal", &ModelResolutionContext::default())
            .expect("resolve_for_callsite");

        assert_eq!(resolved.config.id, "ClaudeBedrockSonnet46");
        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_default_model_for_callsite_returns_canonical_model_id() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config_path = temp_dir.path().join("model-config.toml");
        std::fs::write(
            &config_path,
            r#"
default_model = "ClaudeBedrockSonnet45"

[callsite_defaults]
researcher = "GLM5"

[models.ClaudeBedrockSonnet45]
name = "Claude Sonnet 4.5 (Bedrock)"
provider = "aws-bedrock"
model = "us.anthropic.claude-sonnet-4-5-20250929-v1:0"
region = "us-east-1"
aliases = ["ClaudeBedrockSonnet45"]

[models.ZaiGLM5]
name = "GLM 5 (Z.ai)"
provider = "anthropic"
base_url = "https://api.z.ai/api/anthropic"
api_key_env = "PATH"
model = "glm-5"
aliases = ["GLM5", "ZaiGLM5"]
"#,
        )
        .expect("write config");

        let previous_config = set_env("CHOIR_MODEL_CONFIG_PATH", &config_path.to_string_lossy());
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.default_model_for_callsite("researcher"),
            Some("ZaiGLM5".to_string())
        );
        assert_eq!(
            registry.default_model_for_callsite("missing"),
            Some("ClaudeBedrockSonnet45".to_string())
        );
        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
    }

    #[test]
    fn test_load_model_catalog_discovers_config_in_ancestor_dir() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let previous_config = clear_env("CHOIR_MODEL_CONFIG_PATH");
        let previous_catalog = clear_env("CHOIR_MODEL_CATALOG_PATH");

        let original_cwd = std::env::current_dir().expect("cwd");
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let nested = repo_root.join("sandbox");
        let config_dir = repo_root.join("sandbox").join("config");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::create_dir_all(&config_dir).expect("create config dir");

        let config_path = config_dir.join("model-catalog.toml");
        std::fs::write(
            &config_path,
            r#"
default_model = "ZaiGLM47Flash"

[models.ZaiGLM47Flash]
name = "GLM 4.7 Flash (Z.ai)"
provider = "anthropic"
base_url = "https://api.z.ai/api/anthropic"
api_key_env = "PATH"
model = "glm-4.7-flash"
aliases = ["ZaiGLM47Flash"]
"#,
        )
        .expect("write config");

        std::env::set_current_dir(&nested).expect("set nested cwd");
        let catalog = load_model_catalog();
        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        assert_eq!(catalog.default_model.as_deref(), Some("ZaiGLM47Flash"));
        assert!(catalog.models.contains_key("ZaiGLM47Flash"));

        restore_env("CHOIR_MODEL_CONFIG_PATH", previous_config);
        restore_env("CHOIR_MODEL_CATALOG_PATH", previous_catalog);
    }

    #[test]
    fn test_provider_gateway_override_includes_context_headers() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let prev_gateway_base = set_env("CHOIR_PROVIDER_GATEWAY_BASE_URL", "http://127.0.0.1:9090");
        let prev_gateway_token = set_env("CHOIR_PROVIDER_GATEWAY_TOKEN", "gateway-token");
        let prev_sandbox_id = set_env("CHOIR_SANDBOX_ID", "user-1:live");
        let prev_user_id = set_env("CHOIR_SANDBOX_USER_ID", "user-1");
        let prev_role = set_env("CHOIR_SANDBOX_ROLE", "live");

        let mut initial_headers = HashMap::new();
        initial_headers.insert("x-extra".to_string(), "1".to_string());

        let override_result =
            provider_gateway_override("https://api.z.ai/api/anthropic", "glm-5", &initial_headers)
                .expect("gateway override should be enabled");

        assert_eq!(
            override_result.0,
            "http://127.0.0.1:9090/provider/v1/forward/"
        );
        assert_eq!(override_result.1, "gateway-token");
        assert_eq!(
            override_result
                .2
                .get("x-choiros-upstream-base-url")
                .map(String::as_str),
            Some("https://api.z.ai/api/anthropic")
        );
        assert_eq!(
            override_result.2.get("x-choiros-model").map(String::as_str),
            Some("glm-5")
        );
        assert_eq!(
            override_result
                .2
                .get("x-choiros-sandbox-id")
                .map(String::as_str),
            Some("user-1:live")
        );
        assert_eq!(
            override_result
                .2
                .get("x-choiros-user-id")
                .map(String::as_str),
            Some("user-1")
        );
        assert_eq!(
            override_result
                .2
                .get("x-choiros-sandbox-role")
                .map(String::as_str),
            Some("live")
        );
        assert_eq!(
            override_result.2.get("x-extra").map(String::as_str),
            Some("1")
        );

        restore_env("CHOIR_PROVIDER_GATEWAY_BASE_URL", prev_gateway_base);
        restore_env("CHOIR_PROVIDER_GATEWAY_TOKEN", prev_gateway_token);
        restore_env("CHOIR_SANDBOX_ID", prev_sandbox_id);
        restore_env("CHOIR_SANDBOX_USER_ID", prev_user_id);
        restore_env("CHOIR_SANDBOX_ROLE", prev_role);
    }

    #[test]
    fn test_provider_gateway_override_disabled_without_required_env() {
        let _lock = ENV_MUTEX.lock().expect("env mutex poisoned");
        let prev_gateway_base = clear_env("CHOIR_PROVIDER_GATEWAY_BASE_URL");
        let prev_gateway_token = clear_env("CHOIR_PROVIDER_GATEWAY_TOKEN");

        let headers = HashMap::new();
        let override_result =
            provider_gateway_override("https://api.z.ai/api/anthropic", "glm-5", &headers);
        assert!(override_result.is_none());

        restore_env("CHOIR_PROVIDER_GATEWAY_BASE_URL", prev_gateway_base);
        restore_env("CHOIR_PROVIDER_GATEWAY_TOKEN", prev_gateway_token);
    }
}
