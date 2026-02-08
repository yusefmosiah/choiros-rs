# ChoirOS Model-Provider Agnostic LLM Harness Runbook

**Status:** Research Complete - Ready for Implementation
**Last Updated:** 2026-02-08
**Author:** Research Agent
**Target:** Implementation/Testing Agent

> **✓ RESEARCHED:** AWS Bedrock model IDs have been verified from multiple sources. See **Task #3: Confirm all AWS Bedrock Claude model IDs**
> for final validation against AWS console before production deployment.

---

## 1. Executive Summary

### Current State of Model Coupling

The ChoirOS LLM harness currently has **hardcoded model selection** with limited runtime configurability:

| Location | Hardcoded Value | Purpose |
|----------|-----------------|---------|
| `baml_src/clients.baml:11` | `us.anthropic.claude-opus-4-5-v1` | AWS Bedrock model ID |
| `baml_src/clients.baml:18` | `zai-coding-plan/glm-4.7` | Z.ai GLM 4.7 model |
| `baml_src/clients.baml:24` | `zai-coding-plan/glm-4.7-flash` | Z.ai GLM 4.7 Flash model |
| `baml_src/agent.baml:10` | `client ClaudeBedrock` | PlanAction function |
| `baml_src/agent.baml:38` | `client ClaudeBedrock` | SynthesizeResponse function |
| `baml_src/agent.baml:60` | `client GLM47` / `client GLM47Flash` | QuickResponse function |
| `sandbox/src/actors/chat_agent.rs:743` | `"ClaudeBedrock".to_string()` | Default model in state (maps to Opus 4.5) |
| `sandbox/src/actors/chat_agent.rs:189` | `us.anthropic.claude-opus-4-5-v1` | Bedrock model in registry |
| `sandbox/src/actors/terminal.rs:445` | `us.anthropic.claude-opus-4-5-v1` | Terminal agent registry |

**Key Architectural Insight:** The system uses BAML's `ClientRegistry` pattern for runtime client configuration, but model selection is currently limited to a hardcoded enum (`"ClaudeBedrock"` | `"GLM47Flash"`) in the `create_client_registry()` functions. The new registry will support: `ClaudeBedrockOpus46`, `ClaudeBedrockOpus45`, `ClaudeBedrockSonnet45`, `ClaudeBedrockHaiku45`, `ZaiGLM47`, `ZaiGLM47Flash`, `KimiK25`.

### Target State for Model-Agnostic Harness

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Model Resolution Hierarchy                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  1. Request Override    (API call with ?model= or payload model field)     │
│       ↓                                                                      │
│  2. App Override        (per-app/per-session model preference)             │
│       ↓                                                                      │
│  3. Global Default      (env var CHOIR_DEFAULT_MODEL)                      │
│       ↓                                                                      │
│  4. Fallback            (ClaudeBedrock for backward compatibility)         │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Core Requirements:**
1. Support AWS Bedrock (existing), Z.ai GLM-4.7/GLM-4.7-flash, and Kimi K2.5
2. Model selection via API request parameters
3. Per-session/per-app model persistence
4. Backward compatibility: existing behavior unchanged when no override supplied
5. Provider fallback chain for reliability

---

## 2. Provider Configuration Matrix

### 2.1 AWS Bedrock (Existing)

| Field | Value | Notes |
|-------|-------|-------|
| **Provider Name** | `aws-bedrock` | BAML native provider |
| **Auth Method** | Environment variable | `AWS_BEARER_TOKEN_BEDROCK` |
| **Base URL** | N/A (AWS SDK handles) | Region-based endpoint |
| **Region** | `us-east-1` | Configurable |
| **Model IDs** | `us.anthropic.claude-opus-4-6-v1` | **NEW** Opus 4.6 (no date stamp) |
| | `us.anthropic.claude-opus-4-5-20251101-v1:0` | Opus 4.5 (with date stamp) |
| | `us.anthropic.claude-sonnet-4-5-20250929-v1:0` | Sonnet 4.5 (with date stamp) |
| | `us.anthropic.claude-haiku-4-5-20251001-v1:0` | Haiku 4.5 (with date stamp) |
| **Prefix Options** | `us.anthropic.*` | Regional (data residency) |
| | `global.anthropic.*` | Global HA (recommended by AWS) |
| **BAML Options** | `model`, `region` | See BAML aws-bedrock docs |
| **Streaming** | Yes | Via ConverseStream API |
| **Tool Calling** | Yes | Native Anthropic tool use |
| **Known Unknowns** | AWS introduced API key auth July 2025 - verify if SDK auto-detects or needs explicit config | |

> **ℹ️ Model ID Format Note:** AWS Bedrock uses two different ID formats:
> - **Opus 4.6**: `us.anthropic.claude-opus-4-6-v1` (no date stamp, no `:0` suffix)
> - **4.5 Models**: `us.anthropic.claude-{model}-4-5-YYYYMMDD-v1:0` (with date stamp and `:0` suffix)

**BAML ClientRegistry Configuration:**
```rust
let mut options = HashMap::new();
options.insert("model".to_string(), serde_json::json!("us.anthropic.claude-opus-4-5-20251101-v1:0"));
options.insert("region".to_string(), serde_json::json!("us-east-1"));
cr.add_llm_client("ClaudeBedrock", "aws-bedrock", options);
```

### 2.2 Z.ai (GLM Coding Plan)

| Field | Value | Notes |
|-------|-------|-------|
| **Provider Name** | `anthropic` | Uses Anthropic-compatible API |
| **Auth Method** | Environment variable | `ZAI_API_KEY` or `ANTHROPIC_AUTH_TOKEN` |
| **Base URL** | `https://api.z.ai/api/anthropic` | Anthropic-compatible endpoint |
| | `https://api.z.ai/api/coding/paas/v4` | OpenAI-compatible endpoint |
| **Model IDs** | `zai-coding-plan/glm-4.7` | Standard GLM 4.7 (complex tasks) |
| | `zai-coding-plan/glm-4.7-flash` | Flash variant (faster, cheaper) |
| **BAML Options** | `api_key`, `base_url`, `model` | Standard anthropic provider options |
| **Streaming** | Yes | ASSUMED - verify during testing |
| **Tool Calling** | Yes | ASSUMED - uses Anthropic message format |
| **Known Unknowns** | Tool calling format compatibility with BAML's anthropic provider | **VERIFY IN TIER 1** |
| **Fallback Option** | OpenAI-compatible mode | If Anthropic mode fails, switch BAML provider to `openai` with base_url `https://api.z.ai/api/coding/paas/v4` |

**BAML ClientRegistry Configuration:**
```rust
// GLM 4.7 (Standard)
let mut options = HashMap::new();
options.insert("api_key".to_string(), serde_json::json!(env::var("ZAI_API_KEY")?));
options.insert("base_url".to_string(), serde_json::json!("https://api.z.ai/api/anthropic"));
options.insert("model".to_string(), serde_json::json!("zai-coding-plan/glm-4.7"));
cr.add_llm_client("ZaiGLM47", "anthropic", options);

// GLM 4.7 Flash (Faster)
let mut options = HashMap::new();
options.insert("api_key".to_string(), serde_json::json!(env::var("ZAI_API_KEY")?));
options.insert("base_url".to_string(), serde_json::json!("https://api.z.ai/api/anthropic"));
options.insert("model".to_string(), serde_json::json!("zai-coding-plan/glm-4.7-flash"));
cr.add_llm_client("ZaiGLM47Flash", "anthropic", options);
```

### 2.3 Kimi (Moonshot AI)

| Field | Value | Notes |
|-------|-------|-------|
| **Provider Name** | `anthropic` OR `openai-generic` | Two compatibility modes |
| **Auth Method** | Environment variable | `ANTHROPIC_API_KEY` (Claude mode) or `MOONSHOT_API_KEY` (OpenAI mode) |
| **Base URL** | `https://api.kimi.com/coding/` | Anthropic-compatible |
| | `https://api.moonshot.ai/v1` | OpenAI-compatible |
| | `https://api.moonshot.cn/v1` | China endpoint |
| **Model IDs** | `kimi-for-coding/k2p5` | **Primary** - Kimi K2.5 for coding |
| | `kimi-k2.5` | Alternative model ID (fallback) |
| | `kimi-k2-thinking` | Reasoning model |
| | `kimi-k2-turbo-preview` | Fast preview |
| **BAML Options** | `api_key`, `base_url`, `model` | Standard options |
| **Streaming** | Yes | ASSUMED - verify during testing |
| **Tool Calling** | Unknown | **CRITICAL GAP** - Kimi docs don't specify tool calling support |
| **Known Unknowns** | Tool calling compatibility is UNVERIFIED | **MAY NEED FALLBACK TO DIRECT API** |

> **ℹ️ Kimi Model ID Note:** User indicates the primary model ID is `kimi-for-coding/k2p5` at `https://api.kimi.com/coding/`.
> The alternative ID `kimi-k2.5` is kept as a fallback. Both use Anthropic-compatible configuration.

**BAML ClientRegistry Configuration (Primary - Anthropic mode):**
```rust
// Primary config per user: model ID = kimi-for-coding/k2p5
let mut options = HashMap::new();
options.insert("api_key".to_string(), serde_json::json!(env::var("ANTHROPIC_API_KEY")?));
options.insert("base_url".to_string(), serde_json::json!("https://api.kimi.com/coding/"));
options.insert("model".to_string(), serde_json::json!("kimi-for-coding/k2p5"));
cr.add_llm_client("KimiK25", "anthropic", options);
```

**BAML ClientRegistry Configuration (Fallback):**
```rust
// Fallback: alternative model ID format
let mut options = HashMap::new();
options.insert("api_key".to_string(), serde_json::json!(env::var("ANTHROPIC_API_KEY")?));
options.insert("base_url".to_string(), serde_json::json!("https://api.kimi.com/coding/"));
options.insert("model".to_string(), serde_json::json!("kimi-k2.5"));
cr.add_llm_client("KimiK25-Fallback", "anthropic", options);
```

**BAML ClientRegistry Configuration (OpenAI-generic mode - if anthropic fails):**
```rust
let mut options = HashMap::new();
options.insert("api_key".to_string(), serde_json::json!(env::var("MOONSHOT_API_KEY")?));
options.insert("base_url".to_string(), serde_json::json!("https://api.moonshot.ai/v1"));
options.insert("model".to_string(), serde_json::json!("kimi-k2.5"));
cr.add_llm_client("KimiK25-OpenAI", "openai-generic", options);
```

### 2.4 Provider Capability Summary

| Provider | Provider Type | Tool Calls | Streaming | Notes |
|----------|---------------|------------|-----------|-------|
| AWS Bedrock | `aws-bedrock` | Verified | Verified | Production-ready |
| Z.ai | `anthropic` | **ASSUMED** | **ASSUMED** | Test in Tier 1 |
| Kimi | `anthropic` or `openai-generic` | **UNKNOWN** | **ASSUMED** | May need direct API fallback |

---

## 3. Harness Refactor Plan (Design-Only)

### 3.1 Model Configuration Types

```rust
// New file: sandbox/src/actors/model_config.rs

/// Unique identifier for a model configuration
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ModelConfigId(pub String);

/// Provider-specific configuration
#[derive(Debug, Clone)]
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

/// Complete model configuration
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub id: ModelConfigId,
    pub name: String,           // Display name
    pub provider: ProviderConfig,
    pub retry_policy: Option<String>,
}

/// Model resolution request context
#[derive(Debug, Clone, Default)]
pub struct ModelResolutionContext {
    pub request_model: Option<String>,     // From API request
    pub app_preference: Option<String>,    // From app/session settings
    pub user_preference: Option<String>,   // From user profile
}
```

### 3.2 Resolution Order Implementation

```rust
// In sandbox/src/actors/model_config.rs

impl ModelResolutionContext {
    /// Resolve the effective model configuration
    ///
    /// Resolution order:
    /// 1. Request override (highest priority)
    /// 2. App/session preference
    /// 3. User preference
    /// 4. Global default from env
    /// 5. Hardcoded fallback (lowest priority)
    pub fn resolve(&self, registry: &ModelRegistry) -> Result<ModelConfig, ModelError> {
        // Priority 1: Request override
        if let Some(ref model_id) = self.request_model {
            if let Some(config) = registry.get(model_id) {
                tracing::info!(model_id = %model_id, source = "request", "Resolved model");
                return Ok(config.clone());
            }
            tracing::warn!(model_id = %model_id, "Request model not found in registry");
        }

        // Priority 2: App preference
        if let Some(ref model_id) = self.app_preference {
            if let Some(config) = registry.get(model_id) {
                tracing::info!(model_id = %model_id, source = "app", "Resolved model");
                return Ok(config.clone());
            }
        }

        // Priority 3: User preference
        if let Some(ref model_id) = self.user_preference {
            if let Some(config) = registry.get(model_id) {
                tracing::info!(model_id = %model_id, source = "user", "Resolved model");
                return Ok(config.clone());
            }
        }

        // Priority 4: Global default
        if let Ok(default_model) = std::env::var("CHOIR_DEFAULT_MODEL") {
            if let Some(config) = registry.get(&default_model) {
                tracing::info!(model_id = %default_model, source = "env_default", "Resolved model");
                return Ok(config.clone());
            }
        }

        // Priority 5: Hardcoded fallback
        tracing::info!(model_id = "ClaudeBedrockOpus45", source = "fallback", "Resolved model");
        registry.get("ClaudeBedrockOpus45")
            .cloned()
            .ok_or(ModelError::NoFallbackAvailable)
    }
}
```

### 3.3 Actor Integration Points

#### ChatAgent Changes

```rust
// In sandbox/src/actors/chat_agent.rs

pub struct ChatAgentState {
    args: ChatAgentArguments,
    messages: Vec<BamlMessage>,
    tool_registry: Arc<ToolRegistry>,
    current_model: String,
    model_context: ModelResolutionContext,  // NEW: Track resolution context
}

pub enum ChatAgentMsg {
    // ... existing messages ...

    /// Switch model with full context
    SwitchModel {
        model: String,
        persist: bool,  // NEW: Whether to persist for session
        reply: RpcReplyPort<Result<(), ChatAgentError>>,
    },

    /// Process message with optional model override
    ProcessMessage {
        text: String,
        session_id: Option<String>,
        thread_id: Option<String>,
        model_override: Option<String>,  // NEW: Per-request model
        reply: RpcReplyPort<Result<AgentResponse, ChatAgentError>>,
    },
}

// Replace create_client_registry with:
fn create_client_registry_for_model(
    model_config: &ModelConfig
) -> Result<ClientRegistry, ChatAgentError> {
    let mut cr = ClientRegistry::new();

    match &model_config.provider {
        ProviderConfig::AwsBedrock { model, region } => {
            let mut options = HashMap::new();
            options.insert("model".to_string(), serde_json::json!(model));
            options.insert("region".to_string(), serde_json::json!(region));
            cr.add_llm_client(&model_config.id.0, "aws-bedrock", options);
        }
        ProviderConfig::AnthropicCompatible { base_url, api_key_env, model } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ChatAgentError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), serde_json::json!(api_key));
            options.insert("base_url".to_string(), serde_json::json!(base_url));
            options.insert("model".to_string(), serde_json::json!(model));
            cr.add_llm_client(&model_config.id.0, "anthropic", options);
        }
        ProviderConfig::OpenAiGeneric { base_url, api_key_env, model } => {
            let api_key = std::env::var(api_key_env)
                .map_err(|_| ChatAgentError::MissingApiKey(api_key_env.clone()))?;
            let mut options = HashMap::new();
            options.insert("api_key".to_string(), serde_json::json!(api_key));
            options.insert("base_url".to_string(), serde_json::json!(base_url));
            options.insert("model".to_string(), serde_json::json!(model));
            cr.add_llm_client(&model_config.id.0, "openai-generic", options);
        }
    }

    Ok(cr)
}
```

#### TerminalActor Changes

```rust
// In sandbox/src/actors/terminal.rs

// TerminalActor currently creates a hardcoded registry:
// fn create_client_registry() -> ClientRegistry { ... }

// Replace with:
fn create_client_registry(
    model_config: Option<&ModelConfig>
) -> Result<ClientRegistry, TerminalError> {
    match model_config {
        Some(config) => create_client_registry_for_model(config)
            .map_err(|e| TerminalError::ModelConfig(e.to_string())),
        None => {
            // Default fallback for backward compatibility
            let mut cr = ClientRegistry::new();
            let mut options = HashMap::new();
            options.insert("model".to_string(),
                serde_json::json!("us.anthropic.claude-opus-4-5-20251101-v1:0"));
            options.insert("region".to_string(), serde_json::json!("us-east-1"));
            cr.add_llm_client(&model_config.id.0, "aws-bedrock", options);
            Ok(cr)
        }
    }
}

// Add to TerminalAgentResult:
pub struct TerminalAgentResult {
    pub summary: String,
    pub reasoning: Option<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub executed_commands: Vec<String>,
    pub steps: Vec<TerminalExecutionStep>,
    pub model_used: String,  // NEW: Track which model was used
}
```

### 3.4 API Changes

#### HTTP API (chat.rs)

```rust
// Add model_override to SendMessageRequest
#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub actor_id: String,
    pub user_id: String,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]  // NEW
    pub model: Option<String>,  // Per-request model override
}

// Add model to SendMessageResponse
#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub success: bool,
    pub temp_id: String,
    pub message: String,
    pub model_used: Option<String>,  // NEW: Return which model was used
}
```

#### WebSocket API (websocket_chat.rs)

```rust
// ClientMessage already has SwitchModel, but we should add model to Message:
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "message")]
    Message {
        text: String,
        #[serde(default)]
        client_message_id: Option<String>,
        #[serde(default)]  // NEW
        model: Option<String>,  // Per-message model override
    },
    // ... rest unchanged ...
}
```

### 3.5 Backward Compatibility Plan

| Scenario | Behavior |
|----------|----------|
| No env vars set | Uses `ClaudeBedrockOpus45` (maps old "ClaudeBedrock" to Opus 4.5 for compatibility) |
| No model in request | Uses default resolution chain |
| Invalid model ID | Returns error with available models list |
| Missing API key for selected model | Returns 400 with clear error message |
| Existing WebSocket clients (no model field) | Works unchanged, uses default |
| Existing HTTP API calls | Works unchanged, uses default |
| **Legacy model ID "ClaudeBedrock"** | **Maps to `ClaudeBedrockOpus45`** |

**Migration Path:**
1. Phase 1: Add model registry and resolution logic (no breaking changes)
2. Phase 2: Add optional model parameter to APIs (backward compatible)
3. Phase 3: Add environment variable defaults (backward compatible)
4. Phase 4: Document and announce new capabilities

---

## 4. Experiment Plan

### Tier 1: Provider Connectivity/Smoke Tests

**Goal:** Verify each provider can authenticate and respond to simple requests.

#### Experiment 1.1: Z.ai Anthropic-Compatible Connectivity

**Preconditions:**
- `ZAI_API_KEY` environment variable set with valid key
- Network access to `https://api.z.ai/api/anthropic`

**Steps:**
```bash
# 1. Create test script that uses BAML ClientRegistry to call Z.ai
cargo test -p sandbox test_zai_connectivity -- --nocapture

# 2. Verify with curl first (bypass BAML)
curl -X POST https://api.z.ai/api/anthropic/v1/messages \
  -H "Authorization: Bearer $ZAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "zai-coding-plan/glm-4.7-flash",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'
```

**Expected Output:**
- HTTP 200 response
- Valid JSON with `content` or `message` field
- Response contains greeting

**Failure Signatures:**
- `401 Unauthorized`: Invalid API key
- `404 Not Found`: Wrong endpoint URL
- `400 Bad Request`: Model ID incorrect or payload malformed
- Timeout: Network/firewall issue

**Next Fallback:**
- If anthropic endpoint fails, try OpenAI-compatible endpoint at `https://api.z.ai/api/coding/paas/v4`

#### Experiment 1.2: Kimi Anthropic-Compatible Connectivity

**Preconditions:**
- `ANTHROPIC_API_KEY` set to Kimi API key
- Network access to `https://api.kimi.com/coding/`

**Steps:**
```bash
# 1. Verify with curl
curl -X POST https://api.kimi.com/coding/v1/messages \
  -H "Authorization: Bearer $ANTHROPIC_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "kimi-for-coding/k2p5",  // Primary model ID per user
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'
```

**Expected Output:**
- HTTP 200 response
- Valid JSON response

**Failure Signatures:**
- `404`: Kimi may not support `/v1/messages` endpoint
- `400`: Model ID format incorrect

**Next Fallback:**
- Try OpenAI-compatible endpoint at `https://api.moonshot.ai/v1/chat/completions`
- Use `openai-generic` provider instead of `anthropic`

#### Experiment 1.3: AWS Bedrock Verification

**Preconditions:**
- `AWS_BEARER_TOKEN_BEDROCK` set
- AWS credentials configured

**Steps:**
```bash
# Existing functionality - should already work
cargo test -p sandbox test_bedrock_connectivity -- --nocapture
```

**Expected Output:**
- Successful BAML function call
- Response from Claude model

---

### Tier 2: Structured Tool-Call Behavior Parity

**Goal:** Verify each provider correctly handles BAML's structured output (tool calls).

#### Experiment 2.1: Z.ai Tool Calling

**Preconditions:**
- Tier 1.1 passed
- BAML `PlanAction` function with tool schema

**Steps:**
```rust
// Test that Z.ai can produce valid AgentPlan with tool_calls
let client_registry = create_zai_registry();
let plan = b.PlanAction
    .with_client_registry(&client_registry)
    .call(&messages, &system_context, &tools_description)
    .await?;

// Verify plan has expected structure
assert!(!plan.thinking.is_empty());
// Tool calls may be empty for simple queries - that's OK
```

**Expected Output:**
- Valid `AgentPlan` structure returned
- `thinking` field populated
- `tool_calls` is Vec (may be empty for simple queries)
- `confidence` is valid f64

**Failure Signatures:**
- Deserialization error: Z.ai doesn't follow Anthropic tool format
- Empty response: Model doesn't support tool use
- Hallucinated tool calls: Model invents tools not in schema

**Next Fallback:**
- If tool calling fails, Z.ai may still work for `QuickResponse` (no tools)
- Document limitation: Z.ai only for simple responses, not agentic flows

#### Experiment 2.2: Kimi Tool Calling

**Preconditions:**
- Tier 1.2 passed

**Steps:**
Same as 2.1 but with Kimi registry.

**Expected Output:**
- ASSUMED: Similar to Z.ai

**Failure Signatures:**
- Kimi documentation doesn't mention tool calling
- **HIGH RISK:** May not support tools at all

**Next Fallback:**
- If Kimi doesn't support tools, only use for `QuickResponse` function
- For agentic tasks, fallback to Bedrock or Z.ai

#### Experiment 2.3: Tool Result Handling

**Goal:** Verify providers can synthesize responses from tool results.

**Steps:**
```rust
// Execute a tool, then call SynthesizeResponse
let tool_results = vec![ToolResult { ... }];
let response = b.SynthesizeResponse
    .with_client_registry(&client_registry)
    .call(&user_prompt, &tool_results, &context)
    .await?;
```

**Expected Output:**
- Response incorporates tool result data
- Response is coherent and helpful

---

### Tier 3: Reliability/Fallback Behavior

**Goal:** Test system behavior when providers fail.

#### Experiment 3.1: Invalid API Key Handling

**Steps:**
```bash
# Set invalid key
export ZAI_API_KEY="invalid_key"

# Run test
cargo test -p sandbox test_zai_invalid_key -- --nocapture
```

**Expected Output:**
- Clear error message: "Authentication failed for Z.ai"
- Error propagated to API response
- No panic or crash

#### Experiment 3.2: Model Unavailable (Rate Limit)

**Steps:**
```rust
// Simulate with a mock or use a known rate-limited endpoint
// Verify retry policy is applied
```

**Expected Output:**
- Exponential backoff retries
- Eventually returns error if all retries fail
- Error indicates rate limiting

#### Experiment 3.3: Fallback Chain Activation

**Steps:**
```rust
// Configure fallback: Z.ai -> Bedrock
// Block Z.ai endpoint (e.g., via hosts file or firewall rule)
// Verify request succeeds via Bedrock
```

**Expected Output:**
- Primary provider failure detected
- Fallback provider used
- Response returned successfully
- Event logged showing fallback activation

---

## 5. Test Plan (For Implementation Agent)

### 5.1 Unit Tests

```rust
// sandbox/src/actors/model_config.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_resolution_priority() {
        let registry = create_test_registry();

        // Request override takes priority
        let ctx = ModelResolutionContext {
            request_model: Some("ZaiGLM47Flash".to_string()),
            app_preference: Some("ClaudeBedrockOpus45".to_string()),
            ..Default::default()
        };
        let resolved = ctx.resolve(&registry).unwrap();
        assert_eq!(resolved.id.0, "ZaiGLM47Flash");

        // App preference takes priority over user/default
        let ctx = ModelResolutionContext {
            request_model: None,
            app_preference: Some("ClaudeBedrockOpus45".to_string()),
            user_preference: Some("ZaiGLM47Flash".to_string()),
        };
        let resolved = ctx.resolve(&registry).unwrap();
        assert_eq!(resolved.id.0, "ClaudeBedrockOpus45");
    }

    #[test]
    fn test_invalid_model_returns_error() {
        let registry = create_test_registry();
        let ctx = ModelResolutionContext {
            request_model: Some("NonExistent".to_string()),
            ..Default::default()
        };
        let result = ctx.resolve(&registry);
        assert!(result.is_err());
    }

    #[test]
    fn test_client_registry_creation() {
        let config = ModelConfig {
            id: ModelConfigId("test".to_string()),
            name: "Test".to_string(),
            provider: ProviderConfig::AnthropicCompatible {
                base_url: "https://test.example".to_string(),
                api_key_env: "TEST_API_KEY".to_string(),
                model: "test-model".to_string(),
            },
            retry_policy: None,
        };

        std::env::set_var("TEST_API_KEY", "test-key");
        let registry = create_client_registry_for_model(&config).unwrap();
        // Verify registry is valid (BAML doesn't expose internals,
        // so we just verify it was created)
    }
}
```

### 5.2 Integration Tests

```rust
// sandbox/tests/model_agnostic_test.rs

#[tokio::test]
async fn test_chat_agent_with_different_models() {
    let (event_store, _handle) = create_test_event_store().await;

    // Test with each provider
    for model_id in ["ClaudeBedrockOpus46", "ClaudeBedrockOpus45", "ClaudeBedrockSonnet45", "ZaiGLM47", "ZaiGLM47Flash"] {
        let (agent, _handle) = create_test_agent(&event_store, model_id).await;

        let response = process_message(&agent, "Hello, what model are you?").await;
        assert!(response.is_ok(), "Model {} failed: {:?}", model_id, response);

        let resp = response.unwrap().unwrap();
        assert!(!resp.text.is_empty());
        assert_eq!(resp.model_used, model_id);
    }
}

#[tokio::test]
async fn test_model_switching_persists() {
    let (event_store, _handle) = create_test_event_store().await;
    let (agent, _handle) = create_test_agent(&event_store, "ClaudeBedrockOpus45").await;

    // Switch model
    let result = switch_model(&agent, "ZaiGLM47Flash").await;
    assert!(result.unwrap().is_ok());

    // Process message and verify new model is used
    let response = process_message(&agent, "Hello").await.unwrap().unwrap();
    assert_eq!(response.model_used, "ZaiGLM47Flash");
}

#[tokio::test]
async fn test_model_override_in_request() {
    // Test that request-level model override works
    let (event_store, _handle) = create_test_event_store().await;
    let (agent, _handle) = create_test_agent(&event_store, "ClaudeBedrockOpus45").await;

    // Process with override
    let response = process_message_with_model(&agent, "Hello", Some("ZaiGLM47Flash".to_string()))
        .await
        .unwrap()
        .unwrap();

    // Should use override, not default
    assert_eq!(response.model_used, "ZaiGLM47Flash");
}
```

### 5.3 Negative Tests

```rust
#[tokio::test]
async fn test_missing_api_key_error() {
    // Unset the API key env var
    std::env::remove_var("ZAI_API_KEY");

    let config = create_zai_config();
    let result = create_client_registry_for_model(&config);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("ZAI_API_KEY"));
}

#[tokio::test]
async fn test_invalid_model_id_error() {
    let (event_store, _handle) = create_test_event_store().await;
    let (agent, _handle) = create_test_agent(&event_store, "ClaudeBedrockOpus45").await;

    let result = switch_model(&agent, "InvalidModel").await;
    assert!(result.unwrap().is_err());
}

#[tokio::test]
async fn test_timeout_handling() {
    // Configure a very short timeout
    // Verify appropriate error is returned, not panic
}
```

### 5.4 Regression Tests

```rust
#[tokio::test]
async fn test_default_behavior_unchanged() {
    // Without any model configuration, should behave exactly as before
    let (event_store, _handle) = create_test_event_store().await;
    let (agent, _handle) = create_test_agent_with_defaults(&event_store).await;

    let response = process_message(&agent, "Hello").await.unwrap().unwrap();

    // Should use ClaudeBedrockOpus45 default (backward compatibility)
    assert_eq!(response.model_used, "ClaudeBedrockOpus45");
}

#[tokio::test]
async fn test_websocket_without_model_field() {
    // Simulate old client sending message without model field
    // Should work and use default
}
```

---

## 6. Operational Runbook

### 6.1 Environment Variable Setup Templates

#### Development (.env file)

```bash
# AWS Bedrock (existing)
export AWS_BEARER_TOKEN_BEDROCK="your-aws-token"

# Z.ai GLM Coding Plan
export ZAI_API_KEY="your-zai-api-key"

# Kimi (Moonshot AI) - Anthropic-compatible mode
export ANTHROPIC_API_KEY="your-kimi-api-key"
export ANTHROPIC_BASE_URL="https://api.kimi.com/coding/"

# OR Kimi - OpenAI-compatible mode (alternative)
export MOONSHOT_API_KEY="your-kimi-api-key"

# Default model selection
export CHOIR_DEFAULT_MODEL="ClaudeBedrockOpus46"
# Options: ClaudeBedrockOpus46, ClaudeBedrockOpus45, ClaudeBedrockSonnet45, ClaudeBedrockHaiku45, ZaiGLM47, ZaiGLM47Flash, KimiK25, KimiK25-Fallback

# Optional: Model-specific timeouts (milliseconds)
export CHOIR_MODEL_TIMEOUT_MS="30000"
```

#### Production (Kubernetes/Docker)

```yaml
# docker-compose.yml or k8s deployment
env:
  - name: AWS_BEARER_TOKEN_BEDROCK
    valueFrom:
      secretKeyRef:
        name: choir-secrets
        key: aws-bedrock-token

  - name: ZAI_API_KEY
    valueFrom:
      secretKeyRef:
        name: choir-secrets
        key: zai-api-key

  - name: MOONSHOT_API_KEY
    valueFrom:
      secretKeyRef:
        name: choir-secrets
        key: kimi-api-key

  - name: CHOIR_DEFAULT_MODEL
    value: "ClaudeBedrockOpus46"  # Latest Opus 4.6 for production
```

### 6.2 Example API Payloads

#### HTTP API with Model Override

```bash
# Request with model override
curl -X POST http://localhost:8080/api/chat/send \
  -H "Content-Type: application/json" \
  -d '{
    "actor_id": "user-123",
    "user_id": "user-123",
    "text": "Explain quantum computing",
    "session_id": "session-456",
    "thread_id": "thread-789",
    "model": "ZaiGLM47Flash"
  }'

# Response
{
  "success": true,
  "temp_id": "msg-abc123",
  "message": "Message sent",
  "model_used": "ZaiGLM47Flash"
}
```

#### WebSocket with Model Override

```javascript
// Client message with model override
ws.send(JSON.stringify({
  type: "message",
  text: "Explain quantum computing",
  client_message_id: "msg-123",
  model: "ZaiGLM47Flash"  // Per-message override
}));

// Model switch message
ws.send(JSON.stringify({
  type: "switch_model",
  model: "KimiK25"
}));

// Response
{
  "type": "model_switched",
  "model": "KimiK25",
  "status": "success"
}
```

#### WebSocket Response Showing Model Used

```javascript
{
  "type": "response",
  "content": {
    "text": "Quantum computing is...",
    "confidence": 0.95,
    "model_used": "ZaiGLM47Flash",
    "client_message_id": "msg-123"
  },
  "timestamp": "2026-02-08T12:34:56Z"
}
```

### 6.3 Observability Checklist

#### Events to Log

```rust
// When model is resolved
tracing::info!(
    model_id = %resolved_model.id.0,
    model_name = %resolved_model.name,
    resolution_source = ?source,  // "request", "app", "user", "env_default", "fallback"
    "Model resolved for request"
);

// When model switch occurs
tracing::info!(
    actor_id = %actor_id,
    old_model = %old_model,
    new_model = %new_model,
    persisted = %persist,
    "Model switched"
);

// When fallback is activated
tracing::warn!(
    primary_model = %primary,
    fallback_model = %fallback,
    error = %error,
    "Model fallback activated"
);

// When provider error occurs
tracing::error!(
    model_id = %model_id,
    provider = ?provider_type,
    error = %error,
    "Provider request failed"
);
```

#### Metrics to Track

```rust
// Counter: Model usage
metrics::counter!("choir_model_requests_total",
    "model_id" => model_id,
    "provider" => provider_type
);

// Histogram: Request latency by model
metrics::histogram!("choir_model_request_duration_ms",
    latency as f64,
    "model_id" => model_id
);

// Counter: Fallback activations
metrics::counter!("choir_model_fallbacks_total",
    "primary_model" => primary,
    "fallback_model" => fallback
);

// Counter: Errors by model
metrics::counter!("choir_model_errors_total",
    "model_id" => model_id,
    "error_type" => error_category
);
```

#### Health Check Endpoint

```rust
// GET /health/models
{
  "status": "healthy",
  "models": {
    "ClaudeBedrockOpus46": {
      "status": "available",
      "last_check": "2026-02-08T12:34:56Z"
    },
    "ClaudeBedrockOpus45": {
      "status": "available",
      "last_check": "2026-02-08T12:34:56Z"
    },
    "ClaudeBedrockSonnet45": {
      "status": "available",
      "last_check": "2026-02-08T12:34:56Z"
    },
    "ZaiGLM47": {
      "status": "available",
      "last_check": "2026-02-08T12:34:56Z"
    },
    "ZaiGLM47Flash": {
      "status": "available",
      "last_check": "2026-02-08T12:34:56Z"
    },
    "KimiK25": {
      "status": "unavailable",
      "error": "Authentication failed",
      "last_check": "2026-02-08T12:34:56Z"
    }
  }
}
```

### 6.4 Rollback Plan

If provider integration destabilizes production:

**Immediate Rollback (30 seconds):**
```bash
# Set default back to ClaudeBedrock
kubectl set env deployment/choir CHOIR_DEFAULT_MODEL=ClaudeBedrockOpus46

# Restart to clear any cached model configurations
kubectl rollout restart deployment/choir
```

**Feature Flag Disable:**
```rust
// Add feature flag check
if env::var("CHOIR_ENABLE_MODEL_SELECTION").is_ok() {
    // Use new model resolution
} else {
    // Use hardcoded ClaudeBedrock (original behavior)
}
```

**Database Migration Rollback:**
If model preferences are persisted to database:
```sql
-- Disable model preference reads
UPDATE app_settings SET model_preference = NULL;
```

---

## 7. Open Questions / Decision Log

### Questions Needing Maintainer Decision

| # | Question | Context | Recommendation |
|---|----------|---------|----------------|
| 1 | Should we support per-user model preferences? | User profiles could store preferred model | Start with per-request only, add per-user later |
| 2 | How to handle model versioning? | `zai-coding-plan/glm-4.7` may get updates | Use model aliases: `ZaiGLM47` -> `zai-coding-plan/glm-4.7`, `ZaiGLM47Flash` -> `zai-coding-plan/glm-4.7-flash` |
| 3 | Should we cache ClientRegistry instances? | Creating registry per request may be expensive | Benchmark first, then decide |
| 4 | Fallback strategy: automatic or explicit? | If Z.ai fails, auto-fallback to Bedrock? | Explicit opt-in via `?fallback=true` param |
| 5 | Rate limiting per model? | Different providers have different limits | Add per-model rate limiter in future phase |
| 6 | Kimi tool calling: wait for official support or implement direct API? | Kimi docs don't mention tools | Document limitation, use Kimi for QuickResponse only |

### Recommended Defaults (Pending Uncertainty)

| Uncertainty | Conservative Default | Aggressive Default |
|-------------|---------------------|-------------------|
| Z.ai tool calling | Use only for QuickResponse | Use for all functions |
| Kimi support | Don't include in v1 | Include with openai-generic fallback |
| Model timeout | 30s (same as current) | 60s for complex reasoning models |
| Retry policy | 2 retries (current) | 3 retries for less reliable providers |

### Assumptions Made

1. **ASSUMPTION:** Z.ai's Anthropic-compatible endpoint supports tool calling like native Anthropic.
   - **Verification:** Tier 2.1 experiment
   - **Risk:** Medium - may limit Z.ai to simple responses only

2. **ASSUMPTION:** Kimi supports the `anthropic` provider type via `api.kimi.com/coding/`.
   - **Verification:** Tier 1.2 experiment
   - **Risk:** High - may need `openai-generic` instead

3. **ASSUMPTION:** BAML's `ClientRegistry` can be created per-request without significant overhead.
   - **Verification:** Benchmark during implementation
   - **Risk:** Low - can optimize with caching if needed

4. **ASSUMPTION:** All providers support the same BAML function schemas.
   - **Verification:** Tier 2 experiments
   - **Risk:** Medium - may need provider-specific prompt engineering

---

## 8. Next Agent Execution Checklist

### Phase 1: Foundation (Estimated: 2-3 hours)

- [ ] Create `sandbox/src/actors/model_config.rs` with types from Section 3.1
- [ ] Implement `ModelRegistry` with hardcoded configs for ClaudeBedrockOpus46, ClaudeBedrockOpus45, ClaudeBedrockSonnet45, ClaudeBedrockHaiku45, ZaiGLM47, ZaiGLM47Flash, KimiK25, KimiK25-Fallback
- [ ] Implement `ModelResolutionContext::resolve()` with priority order
- [ ] Write unit tests for model resolution logic
- [ ] Run `cargo test -p sandbox --lib` to verify

### Phase 2: Actor Integration (Estimated: 3-4 hours)

- [ ] Refactor `ChatAgent::create_client_registry()` to use `ModelConfig`
- [ ] Add `model_override` field to `ProcessMessage` and `SwitchModel` messages
- [ ] Update `ChatAgentState` to track `ModelResolutionContext`
- [ ] Refactor `TerminalActor::create_client_registry()` similarly
- [ ] Ensure `model_used` is returned in all response types
- [ ] Run `cargo test -p sandbox test_model_switching` to verify

### Phase 3: API Updates (Estimated: 2-3 hours)

- [ ] Add `model: Option<String>` to `SendMessageRequest` in `chat.rs`
- [ ] Add `model_used: Option<String>` to `SendMessageResponse`
- [ ] Update `ClientMessage::Message` in `websocket_chat.rs` with model field
- [ ] Update WebSocket response to include model_used
- [ ] Run existing integration tests to ensure backward compatibility

### Phase 4: Provider Experiments (Estimated: 2-3 hours)

- [ ] Run Tier 1.1: Z.ai connectivity test
- [ ] Run Tier 1.2: Kimi connectivity test
- [ ] Run Tier 2.1: Z.ai tool calling test
- [ ] Document results in experiment log
- [ ] Adjust provider configs based on results

### Phase 5: Integration Tests (Estimated: 2 hours)

- [ ] Create `sandbox/tests/model_agnostic_test.rs` with tests from Section 5
- [ ] Run `cargo test -p sandbox --test model_agnostic_test`
- [ ] Verify all tests pass or document known failures

### Phase 6: Documentation & Observability (Estimated: 1-2 hours)

- [ ] Add tracing logs from Section 6.3
- [ ] Create `.env.example` with all provider configs
- [ ] Update API documentation with model parameter
- [ ] Add health check endpoint for model status

### Phase 7: Validation (Estimated: 1 hour)

- [ ] Run full test suite: `cargo test -p sandbox`
- [ ] Verify backward compatibility: existing tests pass without modification
- [ ] Manual test: WebSocket chat with model switching
- [ ] Manual test: HTTP API with model override

### Final Verification Commands

```bash
# 1. Build check
cargo check -p sandbox

# 2. Unit tests
cargo test -p sandbox --lib

# 3. Integration tests
cargo test -p sandbox --test '*'

# 4. Clippy
cargo clippy -p sandbox -- -D warnings

# 5. Formatting
cargo fmt --check -p sandbox
```

---

## Appendix A: Model Registry Configuration (Reference)

```rust
// Default model configurations
pub fn default_model_registry() -> ModelRegistry {
    let mut registry = ModelRegistry::new();

    // AWS Bedrock - Claude Opus 4.6 (Latest, Most Capable)
    registry.register(ModelConfig {
        id: ModelConfigId("ClaudeBedrockOpus46".to_string()),
        name: "Claude Opus 4.6 (AWS Bedrock)".to_string(),
        provider: ProviderConfig::AwsBedrock {
            model: "us.anthropic.claude-opus-4-6-v1".to_string(),  // Note: No date stamp, no :0 suffix
            region: "us-east-1".to_string(),
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // AWS Bedrock - Claude Opus 4.5 (Stable)
    registry.register(ModelConfig {
        id: ModelConfigId("ClaudeBedrockOpus45".to_string()),
        name: "Claude Opus 4.5 (AWS Bedrock)".to_string(),
        provider: ProviderConfig::AwsBedrock {
            model: "us.anthropic.claude-opus-4-5-20251101-v1:0".to_string(),
            region: "us-east-1".to_string(),
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // AWS Bedrock - Claude Sonnet 4.5 (Balanced)
    registry.register(ModelConfig {
        id: ModelConfigId("ClaudeBedrockSonnet45".to_string()),
        name: "Claude Sonnet 4.5 (AWS Bedrock)".to_string(),
        provider: ProviderConfig::AwsBedrock {
            model: "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string(),
            region: "us-east-1".to_string(),
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // AWS Bedrock - Claude Haiku 4.5 (Fast/Cheap)
    registry.register(ModelConfig {
        id: ModelConfigId("ClaudeBedrockHaiku45".to_string()),
        name: "Claude Haiku 4.5 (AWS Bedrock)".to_string(),
        provider: ProviderConfig::AwsBedrock {
            model: "us.anthropic.claude-haiku-4-5-20251001-v1:0".to_string(),
            region: "us-east-1".to_string(),
        },
        retry_policy: Some("Fixed".to_string()),
    });

    // Z.ai - GLM 4.7 (Standard)
    registry.register(ModelConfig {
        id: ModelConfigId("ZaiGLM47".to_string()),
        name: "GLM 4.7 (Z.ai)".to_string(),
        provider: ProviderConfig::AnthropicCompatible {
            base_url: "https://api.z.ai/api/anthropic".to_string(),
            api_key_env: "ZAI_API_KEY".to_string(),
            model: "zai-coding-plan/glm-4.7".to_string(),
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // Z.ai - GLM 4.7 Flash
    registry.register(ModelConfig {
        id: ModelConfigId("ZaiGLM47Flash".to_string()),
        name: "GLM 4.7 Flash (Z.ai)".to_string(),
        provider: ProviderConfig::AnthropicCompatible {
            base_url: "https://api.z.ai/api/anthropic".to_string(),
            api_key_env: "ZAI_API_KEY".to_string(),
            model: "zai-coding-plan/glm-4.7-flash".to_string(),
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // Kimi - K2.5 (Anthropic-compatible mode) - PRIMARY
    registry.register(ModelConfig {
        id: ModelConfigId("KimiK25".to_string()),
        name: "Kimi K2.5 (Moonshot)".to_string(),
        provider: ProviderConfig::AnthropicCompatible {
            base_url: "https://api.kimi.com/coding/".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            model: "kimi-for-coding/k2p5".to_string(),  // Primary model ID
        },
        retry_policy: Some("Exponential".to_string()),
    });

    // Kimi - K2.5 (Fallback model ID)
    registry.register(ModelConfig {
        id: ModelConfigId("KimiK25-Fallback".to_string()),
        name: "Kimi K2.5 Fallback (Moonshot)".to_string(),
        provider: ProviderConfig::AnthropicCompatible {
            base_url: "https://api.kimi.com/coding/".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            model: "kimi-k2.5".to_string(),  // Alternative model ID
        },
        retry_policy: Some("Exponential".to_string()),
    });

    registry
}
```

---

## Appendix B: File Modification Summary

| File | Change Type | Description |
|------|-------------|-------------|
| `sandbox/src/actors/model_config.rs` | NEW | Model configuration types and registry |
| `sandbox/src/actors/chat_agent.rs` | MODIFY | Use ModelConfig for client registry creation |
| `sandbox/src/actors/terminal.rs` | MODIFY | Use ModelConfig for client registry creation |
| `sandbox/src/api/chat.rs` | MODIFY | Add model parameter to request/response |
| `sandbox/src/api/websocket_chat.rs` | MODIFY | Add model parameter to WebSocket messages |
| `sandbox/src/actors/mod.rs` | MODIFY | Add model_config module |
| `sandbox/tests/model_agnostic_test.rs` | NEW | Integration tests for model agnosticism |

---

**END OF RUNBOOK**
