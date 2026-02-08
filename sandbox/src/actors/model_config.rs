use crate::baml_client::ClientRegistry;
use serde_json::json;
use std::collections::HashMap;

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
    },
    OpenAiGeneric {
        base_url: String,
        api_key_env: String,
        model: String,
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

        self.get("ClaudeBedrockOpus45")
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
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
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
        } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(base_url));
            options.insert("model".to_string(), json!(model));
            registry.add_llm_client(client_name, "anthropic", options);
            Ok(())
        }
        ProviderConfig::OpenAiGeneric {
            base_url,
            api_key_env,
            model,
        } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ModelConfigError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), json!(api_key));
            options.insert("base_url".to_string(), json!(base_url));
            options.insert("model".to_string(), json!(model));
            registry.add_llm_client(client_name, "openai-generic", options);
            Ok(())
        }
    }
}

pub fn canonical_model_id(model_id: &str) -> Option<&'static str> {
    match model_id {
        "ClaudeBedrock" => Some("ClaudeBedrockOpus45"),
        "GLM47" => Some("ZaiGLM47"),
        "GLM47Flash" => Some("ZaiGLM47Flash"),
        "KimiK25OpenAI" => Some("KimiK25OpenAI"),
        "KimiK25" => Some("KimiK25"),
        "KimiK25Fallback" => Some("KimiK25Fallback"),
        "ClaudeBedrockOpus46" => Some("ClaudeBedrockOpus46"),
        "ClaudeBedrockOpus45" => Some("ClaudeBedrockOpus45"),
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
        "ClaudeBedrockOpus45".to_string(),
        ModelConfig {
            id: "ClaudeBedrockOpus45".to_string(),
            name: "Claude Opus 4.5 (Bedrock)".to_string(),
            provider: ProviderConfig::AwsBedrock {
                model: "us.anthropic.claude-opus-4-5-20251101-v1:0".to_string(),
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
                model: "zai-coding-plan/glm-4.7".to_string(),
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
                model: "zai-coding-plan/glm-4.7-flash".to_string(),
            },
        },
    );
    configs.insert(
        "ZaiGLM47Air".to_string(),
        ModelConfig {
            id: "ZaiGLM47Air".to_string(),
            name: "GLM 4.7 Air (Z.ai)".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://api.z.ai/api/anthropic".to_string(),
                api_key_env: "ZAI_API_KEY".to_string(),
                model: "zai-coding-plan/glm-4.7-air".to_string(),
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
            },
        },
    );
    configs.insert(
        "KimiK25OpenAI".to_string(),
        ModelConfig {
            id: "KimiK25OpenAI".to_string(),
            name: "Kimi K2.5 (OpenAI generic)".to_string(),
            provider: ProviderConfig::OpenAiGeneric {
                base_url: "https://api.moonshot.ai/v1".to_string(),
                api_key_env: "MOONSHOT_API_KEY".to_string(),
                model: "kimi-k2.5".to_string(),
            },
        },
    );

    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_resolution_priority() {
        let registry = ModelRegistry::new();
        let ctx = ModelResolutionContext {
            request_model: Some("ZaiGLM47Flash".to_string()),
            app_preference: Some("ClaudeBedrockOpus45".to_string()),
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
            Some("ClaudeBedrockOpus45".to_string())
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
            },
        };
        let result = create_client_registry_for_config(&config, &["ClaudeBedrock"]);
        assert!(result.is_ok());
    }
}
