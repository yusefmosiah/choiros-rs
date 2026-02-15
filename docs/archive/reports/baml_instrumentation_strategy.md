# BAML Instrumentation Strategy for ChoirOS

**Date:** 2026-02-08
**Status:** Research Report
**Author:** Instrumentation Study Group

---

## Narrative Summary (1-minute read)

ChoirOS uses BAML for LLM orchestration (planning, synthesis, tool extraction) with multi-model, multi-provider support. This report defines a practical instrumentation strategy to log all model calls, parse results, tool calls, and synthesis while integrating with existing EventStoreActor patterns.

**Key Approach:** Wrap BAML function calls at the ChatAgent layer with non-blocking event emission. Use `tokio::spawn` for fire-and-forget logging to avoid blocking LLM calls. Capture model metadata (latency, tokens, cost), tool call representations (normalized vs raw), and error taxonomy with redaction for sensitive content.

**What Changed:** New instrumentation from scratch. Previously no BAML-specific telemetry existed.

**What To Do Next:** Implement `BamlTelemetry` trait, add event emission wrappers in `chat_agent.rs`, extend EventStore with BAML event types, add redaction utility, and write integration tests for all lifecycle events.

---

## 1. Instrumentation Hook List

### 1.1 Primary Entry Points (BAML Function Calls)

Wrap these exact functions in `sandbox/src/baml_client/functions/async_client.rs`:

| Hook Location | Function | Purpose |
|--------------|----------|---------|
| Line 84 | `PlanAction::call()` | Plan LLM call (determines tools/action) |
| Line 90 | `PlanAction::stream()` | Streaming plan (for real-time display) |
| Line 80 | `SynthesizeResponse::call()` | Synthesis LLM call (final response) |
| Line 87 | `SynthesizeResponse::stream()` | Streaming synthesis |
| Line 115 | `QuickResponse::call()` | Simple query LLM call |
| Line 117 | `QuickResponse::stream()` | Streaming quick response |
| Line 93 | `PlanAction::parse()` | Parse-only (no model call) |
| Line 97 | `PlanAction::parse_stream()` | Stream parsing |

**Implementation Strategy:** Do NOT modify generated BAML code. Instead, wrap calls in `ChatAgent` (sandbox/src/actors/chat_agent.rs):

```rust
// Wrap BAML calls in chat_agent.rs handle_process_message() around line 676-681
let plan_start = std::time::Instant::now();
let call_id = ulid::Ulid::new().to_string();

// Emit BamlCallStarted event (non-blocking)
emit_baml_event(&state, "baml.call_started", json!({
    "call_id": call_id,
    "function": "PlanAction",
    "model": model_used,
    "model_source": model_source,
}), session_id, thread_id).await;

let plan_result = crate::baml_client::B
    .PlanAction
    .with_client_registry(&client_registry)
    .call(&state.messages, &system_context, &tools_description)
    .await;

let latency_ms = plan_start.elapsed().as_millis() as i64;
match &plan_result {
    Ok(plan) => {
        emit_baml_event(&state, "baml.call_completed", json!({
            "call_id": call_id,
            "function": "PlanAction",
            "model": model_used,
            "latency_ms": latency_ms,
            "tool_calls_count": plan.tool_calls.len(),
        }), session_id, thread_id).await;
    }
    Err(e) => {
        emit_baml_event(&state, "baml.call_failed", json!({
            "call_id": call_id,
            "function": "PlanAction",
            "model": model_used,
            "latency_ms": latency_ms,
            "error_type": classify_baml_error(e),
            "error_message": redact_error_message(e),
        }), session_id, thread_id).await;
    }
}
```

### 1.2 Model/Provider/Client Registry Resolution

Wrap these in `sandbox/src/actors/model_config.rs`:

| Hook Location | Function | Purpose |
|--------------|----------|---------|
| Line 100 | `ModelRegistry::resolve()` | Model selection with resolution context |
| Line 163 | `ModelRegistry::create_runtime_client_registry_for_model()` | ClientRegistry creation |
| Line 177 | `create_client_registry_for_config()` | Provider-specific client creation |
| Line 194 | `add_provider_client()` | Individual provider addition |

**Implementation:** Add logging in `chat_agent.rs` after resolution (line 646-659):

```rust
// Model resolution event already exists at line 661-674
// Extend with client_registry metadata:
self.log_event(
    state,
    "baml.model_resolution",
    serde_json::json!({
        "call_id": call_id,
        "resolved_model": model_used.clone(),
        "resolution_source": model_source.clone(),
        "client_aliases": REQUIRED_BAML_CLIENT_ALIASES,
        "provider_config": extract_provider_config(&resolved_model),
    }),
    session_id.clone(),
    thread_id.clone(),
    state.args.user_id.clone(),
).await?;
```

### 1.3 Tool Call Extraction & Normalization

Wrap in `chat_agent.rs` around tool execution loop (line 694-785):

| Hook Location | Action | Purpose |
|--------------|--------|---------|
| Line 694 | Tool call iteration | Extract tool_name, tool_args, reasoning |
| Line 695 | `tool_args_for_log()` | Get normalized args for logging |
| Line 698 | `tool_args_for_execution()` | Get args for actual execution |

**Instrumentation point:** After line 706, before tool execution:

```rust
// Tool extraction event
self.log_event(
    state,
    "baml.tool_extracted",
    serde_json::json!({
        "call_id": call_id,
        "tool_name": tool_call.tool_name,
        "normalized_args": redact_tool_args(&tool_args_value),
        "raw_model_output": plan.thinking, // Full thinking output
        "parse_confidence": plan.confidence,
        "reasoning": tool_call.reasoning,
    }),
    session_id.clone(),
    thread_id.clone(),
    state.args.user_id.clone(),
).await?;
```

### 1.4 Synthesis Call (Final Response Generation)

Wrap in `chat_agent.rs` around line 794-803:

```rust
// Synthesis start event
let synthesis_start = std::time::Instant::now();
let synthesis_call_id = ulid::Ulid::new().to_string();

emit_baml_event(&state, "baml.synthesis_started", json!({
    "call_id": synthesis_call_id,
    "parent_call_id": call_id,
    "function": "SynthesizeResponse",
    "model": model_used,
    "tool_results_count": tool_results.len(),
}), session_id.clone(), thread_id.clone()).await;

let response_text = if let Some(final_response) = plan.final_response {
    final_response
} else {
    crate::baml_client::B
        .SynthesizeResponse
        .with_client_registry(&client_registry)
        .call(&user_text, &tool_results, &conversation_context)
        .await
        .map_err(|e| ChatAgentError::Baml(e.to_string()))?
};

let synthesis_latency_ms = synthesis_start.elapsed().as_millis() as i64;
emit_baml_event(&state, "baml.synthesis_completed", json!({
    "call_id": synthesis_call_id,
    "parent_call_id": call_id,
    "model": model_used,
    "latency_ms": synthesis_latency_ms,
}), session_id, thread_id).await;
```

---

## 2. Event Schemas (6-8 Event Types)

Add these to `shared-types/src/lib.rs` as constants:

```rust
// BAML Lifecycle Events
pub const EVENT_BAML_CALL_STARTED: &str = "baml.call_started";
pub const EVENT_BAML_CALL_COMPLETED: &str = "baml.call_completed";
pub const EVENT_BAML_CALL_FAILED: &str = "baml.call_failed";
pub const EVENT_BAML_MODEL_RESOLUTION: &str = "baml.model_resolution";
pub const EVENT_BAML_TOOL_EXTRACTED: &str = "baml.tool_extracted";
pub const EVENT_BAML_SYNTHESIS_STARTED: &str = "baml.synthesis_started";
pub const EVENT_BAML_SYNTHESIS_COMPLETED: &str = "baml.synthesis_completed";
pub const EVENT_BAML_PARSE_SUCCESS: &str = "baml.parse_success";
pub const EVENT_BAML_PARSE_FAILED: &str = "baml.parse_failed";
```

### 2.1 `baml.call_started`

Emitted immediately before any BAML function call.

```json
{
  "call_id": "01HZ... (ULID)",
  "function": "PlanAction" | "SynthesizeResponse" | "QuickResponse" | "ExtractResume",
  "model": "ClaudeBedrockOpus45" | "ZaiGLM47" | etc.,
  "model_source": "request" | "app" | "user" | "env_default" | "fallback",
  "correlation_id": "optional_parent_call_id",
  "timestamp_ms": 1739001234567
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlCallStartedPayload {
    pub call_id: String,
    pub function: String,
    pub model: String,
    pub model_source: String,
    pub correlation_id: Option<String>,
    pub timestamp_ms: i64,
}
```

### 2.2 `baml.call_completed`

Emitted on successful BAML function completion.

```json
{
  "call_id": "01HZ...",
  "function": "PlanAction",
  "model": "ClaudeBedrockOpus45",
  "latency_ms": 1234,
  "tokens_used": {
    "prompt_tokens": 1200,
    "completion_tokens": 800,
    "total_tokens": 2000
  },
  "cost_estimate_usd": 0.012,
  "stream_chunks": 42  // 0 for non-streaming
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlCallCompletedPayload {
    pub call_id: String,
    pub function: String,
    pub model: String,
    pub latency_ms: i64,
    pub tokens_used: Option<TokenUsage>,
    pub cost_estimate_usd: Option<f64>,
    pub stream_chunks: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}
```

**Note:** Token usage is provider-dependent. BAML may not always return this. Check BAML error handling for availability.

### 2.3 `baml.call_failed`

Emitted on any BAML function error.

```json
{
  "call_id": "01HZ...",
  "function": "PlanAction",
  "model": "ClaudeBedrockOpus45",
  "latency_ms": 5678,
  "error_type": "connection_error" | "parse_error" | "timeout" | "rate_limit" | "content_filter" | "auth_error" | "unknown",
  "error_code": "bedrock_500" | "anthropic_429" | etc.,
  "error_message": "redacted_message",
  "retry_attempt": 2,
  "max_retries": 2
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlCallFailedPayload {
    pub call_id: String,
    pub function: String,
    pub model: String,
    pub latency_ms: i64,
    pub error_type: String,
    pub error_code: Option<String>,
    pub error_message: String,
    pub retry_attempt: i64,
    pub max_retries: i64,
}
```

### 2.4 `baml.model_resolution`

Emitted when ModelRegistry resolves a model for a call.

```json
{
  "call_id": "01HZ...",
  "requested_model": "ClaudeBedrock",
  "resolved_model": "ClaudeBedrockOpus45",
  "resolution_source": "request" | "app" | "user" | "env_default" | "fallback",
  "provider": "aws-bedrock" | "anthropic" | "openai-generic",
  "provider_config": {
    "region": "us-east-1",
    "base_url": "redacted",
    "api_key_env": "ZAI_API_KEY"
  },
  "client_aliases": ["ClaudeBedrock", "GLM47"]
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlModelResolutionPayload {
    pub call_id: String,
    pub requested_model: Option<String>,
    pub resolved_model: String,
    pub resolution_source: String,
    pub provider: String,
    pub provider_config: serde_json::Value,
    pub client_aliases: Vec<String>,
}
```

### 2.5 `baml.tool_extracted`

Emitted when tool calls are parsed from model response.

```json
{
  "call_id": "01HZ...",
  "parent_call_id": "01HZ... (PlanAction call)",
  "tool_name": "bash" | "read_file" | "write_file" | "list_files" | "search_files",
  "normalized_args": {
    "command": "ls -la",
    "cwd": "/tmp"
  },
  "raw_model_output": "Thinking process...",
  "parse_confidence": 0.95,
  "reasoning": "Need to list directory contents"
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlToolExtractedPayload {
    pub call_id: String,
    pub parent_call_id: String,
    pub tool_name: String,
    pub normalized_args: serde_json::Value,
    pub raw_model_output: String,
    pub parse_confidence: f64,
    pub reasoning: Option<String>,
}
```

### 2.6 `baml.synthesis_started` / `baml.synthesis_completed`

Emitted for synthesis phase (SynthesizeResponse call).

```json
{
  "call_id": "01HZ...",
  "parent_call_id": "01HZ... (PlanAction call)",
  "function": "SynthesizeResponse",
  "model": "ClaudeBedrockOpus45",
  "tool_results_count": 3,
  "latency_ms": 890
}
```

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlSynthesisPayload {
    pub call_id: String,
    pub parent_call_id: String,
    pub function: String,
    pub model: String,
    pub tool_results_count: i64,
    pub latency_ms: Option<i64>,  // Only in completed event
}
```

---

## 3. Model Metadata Fields

### 3.1 Core Fields

| Field | Type | Description | Source |
|-------|------|-------------|--------|
| `call_id` | String (ULID) | Unique call identifier | Generated at call start |
| `model` | String | Resolved model ID (e.g., "ClaudeBedrockOpus45") | ModelRegistry |
| `model_source` | String | Resolution source | ModelResolutionSource enum |
| `latency_ms` | i64 | End-to-end latency | `Instant::now()` diff |
| `tokens_used` | `TokenUsage` | Prompt/completion/total tokens | BAML response metadata |
| `cost_estimate_usd` | f64 | Estimated cost in USD | Token pricing table |
| `stream_chunks` | i64 | Number of stream chunks received | BAML stream callback |

### 3.2 Cost Estimation Table

Add to `sandbox/src/actors/model_config.rs`:

```rust
#[derive(Debug, Clone)]
pub struct TokenPricing {
    pub prompt_per_1k: f64,
    pub completion_per_1k: f64,
}

pub fn get_token_pricing(model_id: &str) -> Option<TokenPricing> {
    match model_id {
        "ClaudeBedrockOpus45" | "ClaudeBedrockOpus46" => Some(TokenPricing {
            prompt_per_1k: 15.0,
            completion_per_1k: 75.0,
        }),
        "ClaudeBedrockSonnet45" => Some(TokenPricing {
            prompt_per_1k: 3.0,
            completion_per_1k: 15.0,
        }),
        "ClaudeBedrockHaiku45" => Some(TokenPricing {
            prompt_per_1k: 0.25,
            completion_per_1k: 1.25,
        }),
        "ZaiGLM47" => Some(TokenPricing {
            prompt_per_1k: 5.0,
            completion_per_1k: 15.0,
        }),
        "ZaiGLM47Flash" => Some(TokenPricing {
            prompt_per_1k: 0.1,
            completion_per_1k: 0.4,
        }),
        "KimiK25" => Some(TokenPricing {
            prompt_per_1k: 2.0,
            completion_per_1k: 8.0,
        }),
        _ => None,
    }
}

pub fn estimate_cost_usd(model_id: &str, prompt_tokens: i64, completion_tokens: i64) -> Option<f64> {
    let pricing = get_token_pricing(model_id)?;
    let prompt_cost = (prompt_tokens as f64 / 1000.0) * pricing.prompt_per_1k;
    let completion_cost = (completion_tokens as f64 / 1000.0) * pricing.completion_per_1k;
    Some(prompt_cost + completion_cost)
}
```

### 3.3 Usage in Event Payload

```rust
// In BamlCallCompletedPayload construction:
let cost_estimate = if let Some(usage) = tokens_used {
    estimate_cost_usd(&model, usage.prompt_tokens, usage.completion_tokens)
} else {
    None
};
```

---

## 4. Tool Call Representation

### 4.1 Normalized Args vs Raw Model Output

**Normalized Args:** Structured JSON with tool-specific fields.

```json
{
  "tool_name": "bash",
  "normalized_args": {
    "command": "ls -la",
    "cwd": "/tmp",
    "timeout_ms": 30000
  },
  "reasoning": "Need to check directory structure"
}
```

**Raw Model Output:** Full model response text (useful for debugging parse failures).

```json
{
  "raw_model_output": "I'll check the directory structure.\n\n<thinking>The user wants to see files in /tmp...</thinking>\n\n<tool_calls>\n<tool name=\"bash\">\n<args>...</args>\n</tool>\n</tool_calls>"
}
```

**Parse Confidence:** From `AgentPlan.confidence` field (0.0-1.0).

### 4.2 Redaction Strategy for Tool Args

Add to `sandbox/src/baml_client/telemetry.rs` (new file):

```rust
use serde_json::Value;
use regex::Regex;

lazy_static::lazy_static! {
    static ref SECRET_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)(api[_-]?key|token|secret|password|auth)\s*[:=]\s*[\"']?([^\s\"'<>]{8,})").unwrap(),
        Regex::new(r"(?i)sk-[a-zA-Z0-9]{20,}").unwrap(),  // OpenAI keys
        Regex::new(r"(?i)[a-zA-Z0-9]{32}").unwrap(),  // 32-char hex keys
    ];
}

pub fn redact_tool_args(args: &Value) -> Value {
    match args {
        Value::String(s) => redact_string(s),
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in map {
                let redacted_key = if is_sensitive_key(key) {
                    format!("{}_REDACTED", key)
                } else {
                    key.clone()
                };
                redacted.insert(redacted_key, redact_tool_args(value));
            }
            Value::Object(redacted)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(redact_tool_args).collect())
        }
        _ => args.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    key_lower.contains("api_key")
        || key_lower.contains("token")
        || key_lower.contains("secret")
        || key_lower.contains("password")
        || key_lower.contains("auth")
        || key_lower.contains("credential")
}

fn redact_string(s: &str) -> Value {
    for pattern in SECRET_PATTERNS.iter() {
        if let Some(caps) = pattern.captures(s) {
            if let Some(secret) = caps.get(2) {
                let redacted = format!("***{}***", &secret.as_str()[0..4]);
                return Value::String(pattern.replace(s, redacted));
            }
        }
    }
    Value::String(s.to_string())
}

pub fn redact_error_message(error: &baml::BamlError) -> String {
    let msg = error.to_string();
    redact_string(&msg)
        .as_str()
        .unwrap_or(&msg)
        .to_string()
}
```

---

## 5. Error Taxonomy

### 5.1 Error Classification

Add to `sandbox/src/baml_client/telemetry.rs`:

```rust
pub enum BamlErrorType {
    Connection,
    Parse,
    Timeout,
    RateLimit,
    ContentFilter,
    Auth,
    Unknown,
}

pub fn classify_baml_error(error: &baml::BamlError) -> &'static str {
    let msg = error.to_string().to_lowercase();

    if msg.contains("connection") || msg.contains("network") || msg.contains("timeout") {
        if msg.contains("timeout") {
            "timeout"
        } else {
            "connection_error"
        }
    } else if msg.contains("parse") || msg.contains("invalid") || msg.contains("format") {
        "parse_error"
    } else if msg.contains("rate limit") || msg.contains("429") || msg.contains("quota") {
        "rate_limit"
    } else if msg.contains("content filter") || msg.contains("moderation") || msg.contains("safety") {
        "content_filter"
    } else if msg.contains("auth") || msg.contains("unauthorized") || msg.contains("401") {
        "auth_error"
    } else {
        "unknown"
    }
}

pub fn extract_error_code(error: &baml::BamlError) -> Option<String> {
    let msg = error.to_string();
    // Try to extract provider error codes like "bedrock_500", "anthropic_429"
    if let Some(caps) = Regex::new(r"(\w+_\d{3})").unwrap().captures(&msg) {
        Some(caps[1].to_string())
    } else {
        None
    }
}
```

### 5.2 Retry Logging

BAML has built-in retry policies (exponential backoff). Log retry attempts:

```json
{
  "error_type": "rate_limit",
  "error_code": "anthropic_429",
  "retry_attempt": 2,
  "max_retries": 2,
  "retry_delay_ms": 300,
  "total_backoff_ms": 300
}
```

**Add to `BamlCallFailedPayload`:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryMetadata {
    pub retry_attempt: i64,
    pub max_retries: i64,
    pub retry_delay_ms: i64,
    pub total_backoff_ms: i64,
}
```

---

## 6. Redaction Strategy

### 6.1 Fields to Redact

**Always Redact:**
- API keys (OpenAI `sk-`, AWS tokens, etc.)
- Passwords
- Secrets/tokens
- Authentication headers

**Conditional Redaction (with flag):**
- Full prompts (user-provided content)
- Tool arguments (content parameter)
- Model responses (full output text)

**Never Redact:**
- Metadata (timestamps, IDs, counts)
- Error types/codes
- Model names
- Latency measurements

### 6.2 Redaction Implementation

Create `sandbox/src/baml_client/redaction.rs`:

```rust
use serde_json::Value;

pub struct RedactionOptions {
    pub redact_prompts: bool,
    pub redact_responses: bool,
    pub redact_tool_args: bool,
    pub redact_secrets: bool,  // Always true in production
}

impl Default for RedactionOptions {
    fn default() -> Self {
        Self {
            redact_prompts: true,
            redact_responses: true,
            redact_tool_args: false,  // Args already redacted by field-level
            redact_secrets: true,
        }
    }
}

pub fn redact_event_payload(
    event_type: &str,
    payload: &Value,
    options: &RedactionOptions,
) -> Value {
    match event_type {
        "baml.call_started" => {
            // Only redact if payload contains sensitive data (should not)
            payload.clone()
        }
        "baml.tool_extracted" => {
            if options.redact_tool_args {
                let mut redacted = payload.clone();
                if let Some(obj) = redacted.as_object_mut() {
                    if let Some(args) = obj.get_mut("normalized_args") {
                        *args = redact_tool_args(args);
                    }
                }
                redacted
            } else {
                payload.clone()
            }
        }
        "baml.call_failed" => {
            let mut redacted = payload.clone();
            if let Some(obj) = redacted.as_object_mut() {
                if let Some(msg) = obj.get_mut("error_message") {
                    *msg = Value::String(redact_string(msg.as_str().unwrap_or_default())));
                }
            }
            redacted
        }
        _ => payload.clone(),
    }
}
```

### 6.3 Integration with EventStore

Modify `chat_agent.rs` `log_event()` to apply redaction:

```rust
async fn log_event(
    &self,
    state: &ChatAgentState,
    event_type: &str,
    payload: serde_json::Value,
    session_id: Option<String>,
    thread_id: Option<String>,
    user_id: String,
) -> Result<(), ChatAgentError> {
    let redaction_options = crate::baml_client::RedactionOptions::default();
    let redacted_payload = crate::baml_client::redact_event_payload(
        event_type,
        &payload,
        &redaction_options,
    );

    let event = AppendEvent {
        event_type: event_type.to_string(),
        payload: shared_types::with_scope(redacted_payload, session_id, thread_id),
        actor_id: state.args.actor_id.clone(),
        user_id,
    };

    // ... rest of existing log_event implementation
}
```

---

## 7. Integration Approach: Non-Blocking Event Emission

### 7.1 Fire-and-Forget Pattern

**Key Principle:** Never block LLM calls on event persistence.

```rust
use ractor::cast;

async fn emit_baml_event_non_blocking(
    event_store: &ActorRef<EventStoreMsg>,
    actor_id: String,
    user_id: String,
    event_type: &str,
    payload: serde_json::Value,
    session_id: Option<String>,
    thread_id: Option<String>,
) {
    let event = AppendEvent {
        event_type: event_type.to_string(),
        payload: shared_types::with_scope(payload, session_id, thread_id),
        actor_id,
        user_id,
    };

    let store_ref = event_store.clone();
    tokio::spawn(async move {
        match ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event,
            reply,
        }) {
            Ok(Err(e)) => tracing::warn!("BAML event persistence failed: {}", e),
            Err(e) => tracing::warn!("BAML event send failed: {}", e),
            _ => {}
        }
    });
}
```

### 7.2 Async Channel Buffering (Optional Enhancement)

For high-throughput scenarios, use a channel to batch events:

```rust
use tokio::sync::mpsc;

pub struct BamlTelemetryActor {
    event_buffer: mpsc::Sender<BamlEvent>,
}

#[derive(Debug, Clone)]
pub struct BamlEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor_id: String,
    pub user_id: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

impl BamlTelemetryActor {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        let (tx, mut rx) = mpsc::channel::<BamlEvent>(1000);

        // Spawn buffer-flusher task
        tokio::spawn(async move {
            let mut batch = Vec::with_capacity(50);
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            flush_batch(&event_store, batch.drain(..).collect()).await;
                        }
                    }
                    Some(event) = rx.recv() => {
                        batch.push(event);
                        if batch.len() >= 50 {
                            flush_batch(&event_store, batch.drain(..).collect()).await;
                        }
                    }
                    else => break,  // Channel closed
                }
            }
        });

        Self { event_buffer: tx }
    }

    pub async fn emit(&self, event: BamlEvent) {
        let _ = self.event_buffer.send(event).await;
    }
}

async fn flush_batch(
    event_store: &ActorRef<EventStoreMsg>,
    events: Vec<BamlEvent>,
) {
    // Flush events to EventStoreActor in parallel
    let store_ref = event_store.clone();
    tokio::spawn(async move {
        for event in events {
            let append = AppendEvent {
                event_type: event.event_type,
                payload: shared_types::with_scope(
                    event.payload,
                    event.session_id,
                    event.thread_id,
                ),
                actor_id: event.actor_id,
                user_id: event.user_id,
            };

            let _ = ractor::cast!(store_ref.clone(), EventStoreMsg::Append {
                event: append,
            });
        }
    });
}
```

---

## 8. Implementation Checklist

### Phase 1: Core Telemetry Infrastructure

- [ ] Create `sandbox/src/baml_client/telemetry.rs` module
- [ ] Create `sandbox/src/baml_client/redaction.rs` module
- [ ] Add BAML event type constants to `shared-types/src/lib.rs`
- [ ] Implement `emit_baml_event_non_blocking()` helper
- [ ] Add token pricing table to `model_config.rs`
- [ ] Add error classification functions

### Phase 2: ChatAgent Integration

- [ ] Wrap `PlanAction.call()` with `baml.call_started/completed/failed` events
- [ ] Extend model resolution event with client_registry metadata
- [ ] Add `baml.tool_extracted` event in tool execution loop
- [ ] Wrap `SynthesizeResponse.call()` with `baml.synthesis_started/completed` events
- [ ] Apply redaction in `log_event()`

### Phase 3: EventStore & EventBus Integration

- [ ] Add BAML event indexes to EventStore migrations
- [ ] Subscribe ChatAgent to BAML events via EventBus (optional, for monitoring)
- [ ] Add BAML event aggregation queries (e.g., "average latency by model")

### Phase 4: Testing

- [ ] Unit tests for error classification
- [ ] Unit tests for token pricing
- [ ] Unit tests for redaction logic
- [ ] Integration tests for full BAML call lifecycle
- [ ] Performance tests (ensure <5ms overhead from telemetry)

### Phase 5: Observability & Monitoring

- [ ] Create BAML metrics dashboard (latency, errors, token usage, cost)
- [ ] Add alerting for high error rates (>10%)
- [ ] Add alerting for unexpected latency (>30s)
- [ ] Export metrics to Prometheus/OpenTelemetry (optional)

---

## 9. Testing Strategy

### 9.1 Unit Tests

**Error Classification (`telemetry.rs`):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_connection_error() {
        let err = baml::BamlError::Connection("Failed to connect to API".into());
        assert_eq!(classify_baml_error(&err), "connection_error");
    }

    #[test]
    fn test_classify_rate_limit() {
        let err = baml::BamlError::RateLimit("429 Too Many Requests".into());
        assert_eq!(classify_baml_error(&err), "rate_limit");
    }

    #[test]
    fn test_classify_timeout() {
        let err = baml::BamlError::Timeout("Request timed out after 30s".into());
        assert_eq!(classify_baml_error(&err), "timeout");
    }
}
```

**Redaction (`redaction.rs`):**

```rust
#[test]
fn test_redact_api_key() {
    let args = json!({"api_key": "sk-1234567890abcdef"});
    let redacted = redact_tool_args(&args);
    assert_eq!(redacted["api_key"].as_str(), Some("****1234****"));
}

#[test]
fn test_preserve_non_sensitive_fields() {
    let args = json!({"command": "ls -la", "timeout": 30});
    let redacted = redact_tool_args(&args);
    assert_eq!(redacted["command"], args["command"]);
    assert_eq!(redacted["timeout"], args["timeout"]);
}
```

### 9.2 Integration Tests

**Full BAML Lifecycle (`chat_agent_test.rs`):**

```rust
#[tokio::test]
async fn test_baml_call_lifecycle_events() {
    let (event_store_ref, _) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::InMemory
    ).await.unwrap();

    let (agent_ref, _) = Actor::spawn(
        None,
        ChatAgent::new(),
        ChatAgentArguments {
            actor_id: "test-agent".to_string(),
            user_id: "test-user".to_string(),
            event_store: event_store_ref.clone(),
            preload_session_id: None,
            preload_thread_id: None,
            application_supervisor: None,
        },
    ).await.unwrap();

    // Send a message
    let response = ractor::call!(agent_ref, |reply| ChatAgentMsg::ProcessMessage {
        text: "list files in /tmp".to_string(),
        session_id: Some("test-session".to_string()),
        thread_id: Some("test-thread".to_string()),
        model_override: Some("ZaiGLM47".to_string()),
        reply,
    }).await.unwrap().unwrap();

    // Wait for events to be persisted
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query events
    let events = ractor::call!(event_store_ref.clone(), |reply| {
        EventStoreMsg::GetEventsForActorWithScope {
            actor_id: "test-agent".to_string(),
            session_id: "test-session".to_string(),
            thread_id: "test-thread".to_string(),
            since_seq: 0,
            reply,
        }
    }).await.unwrap().unwrap();

    // Assert BAML events exist
    let baml_events: Vec<_> = events.iter()
        .filter(|e| e.event_type.starts_with("baml."))
        .collect();

    assert!(baml_events.len() >= 3);  // call_started, call_completed, tool_extracted

    let call_started = baml_events.iter()
        .find(|e| e.event_type == "baml.call_started")
        .unwrap();

    assert_eq!(call_started.payload["function"], "PlanAction");
    assert_eq!(call_started.payload["model"], "ZaiGLM47");

    let call_completed = baml_events.iter()
        .find(|e| e.event_type == "baml.call_completed")
        .unwrap();

    assert!(call_completed.payload["latency_ms"].as_i64().is_some());

    // Cleanup
    agent_ref.stop(None);
    event_store_ref.stop(None);
}
```

### 9.3 Performance Tests

Ensure telemetry adds <5ms overhead:

```rust
#[tokio::test]
async fn test_telemetry_overhead() {
    // Measure baseline without telemetry
    let start = Instant::now();
    // ... run BAML call without instrumentation ...
    let baseline = start.elapsed();

    // Measure with telemetry
    let start = Instant::now();
    // ... run BAML call with instrumentation ...
    let with_telemetry = start.elapsed();

    let overhead = with_telemetry - baseline;
    assert!(overhead < Duration::from_millis(5), "Overhead: {:?}", overhead);
}
```

---

## 10. Migration Path

### 10.1 Backward Compatibility

- Existing events (`model.selection`, `model.changed`, etc.) remain unchanged
- New BAML events are additive (no breaking changes)
- Optional: Add feature flag to disable BAML telemetry if needed

### 10.2 Rollout Strategy

1. **Stage 1:** Deploy telemetry infrastructure without ChatAgent hooks
2. **Stage 2:** Enable BAML event emission in development environment
3. **Stage 3:** Monitor for performance impact (<5% CPU/memory overhead)
4. **Stage 4:** Enable in production with canary (10% of ChatAgent instances)
5. **Stage 5:** Full rollout

### 10.3 Feature Flags

Add to `Cargo.toml`:

```toml
[features]
default = []
baml_telemetry = []
```

Conditionally compile telemetry:

```rust
#[cfg(feature = "baml_telemetry")]
mod telemetry;

#[cfg(not(feature = "baml_telemetry"))]
mod telemetry {
    // No-op implementations
    pub async fn emit_baml_event(...) { /* do nothing */ }
}
```

---

## 11. Appendix: Reference Implementation

### A. Full `sandbox/src/baml_client/telemetry.rs`

```rust
use std::time::Instant;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

pub use super::redaction::{redact_tool_args, redact_string, RedactionOptions};

/// Unique identifier for a BAML function call
pub type BamlCallId = String;

/// Generate a new BAML call ID
pub fn generate_call_id() -> BamlCallId {
    Ulid::new().to_string()
}

/// Token usage from provider response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// Cost estimation result
#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub prompt_cost_usd: f64,
    pub completion_cost_usd: f64,
    pub total_cost_usd: f64,
}

/// Classify BAML error into category
pub fn classify_baml_error(error: &baml::BamlError) -> &'static str {
    let msg = error.to_string().to_lowercase();

    if msg.contains("connection") || msg.contains("network") {
        if msg.contains("timeout") {
            "timeout"
        } else {
            "connection_error"
        }
    } else if msg.contains("parse") || msg.contains("invalid") {
        "parse_error"
    } else if msg.contains("rate limit") || msg.contains("429") {
        "rate_limit"
    } else if msg.contains("content filter") || msg.contains("safety") {
        "content_filter"
    } else if msg.contains("auth") || msg.contains("401") {
        "auth_error"
    } else {
        "unknown"
    }
}

/// Extract provider error code from error message
pub fn extract_error_code(error: &baml::BamlError) -> Option<String> {
    use regex::Regex;
    let msg = error.to_string();
    Regex::new(r"(\w+_\d{3})").unwrap()
        .captures(&msg)
        .map(|caps| caps[1].to_string())
}

/// Measure BAML call latency
pub struct LatencyTimer {
    start: Instant,
}

impl LatencyTimer {
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }
}
```

---

## Conclusion

This instrumentation strategy provides comprehensive telemetry for BAML lifecycle events in ChoirOS, covering:

1. **All lifecycle stages**: call start, completion, failure, model resolution, tool extraction, synthesis
2. **Rich metadata**: model, provider, latency, tokens, cost, parse confidence
3. **Safety**: redaction for sensitive content, non-blocking event emission
4. **Observability**: event schemas compatible with existing EventStoreActor patterns
5. **Practicality**: minimal code changes, clear implementation checklist, thorough testing strategy

The approach is production-ready and designed to scale with ChoirOS's multi-model, multi-provider architecture while maintaining <5ms telemetry overhead.

---

**Document Version:** 1.0
**Last Updated:** 2026-02-08
