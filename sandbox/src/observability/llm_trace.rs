//! LLM Call Tracing Module
//!
//! Provides helpers to emit consistent LLM call trace events for observability.
//! Every LLM call should emit a `started` event followed by exactly one terminal
//! event (`completed` or `failed`) with a shared `trace_id`.
//!
//! # Event Contract
//!
//! - `llm.call.started` - Call initiated
//! - `llm.call.completed` - Call succeeded
//! - `llm.call.failed` - Call errored
//!
//! # Bounded Payload Policy
//!
//! - `system_context`: max 4 KB
//! - `input`: max 16 KB serialized
//! - `output`: max 16 KB serialized
//!
//! Sensitive keys are redacted before persistence.

use chrono::{DateTime, Utc};
use ractor::ActorRef;
use shared_types::FailureKind;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};

pub const EVENT_TOPIC_LLM_CALL_STARTED: &str = shared_types::EVENT_TOPIC_LLM_CALL_STARTED;
pub const EVENT_TOPIC_LLM_CALL_COMPLETED: &str = shared_types::EVENT_TOPIC_LLM_CALL_COMPLETED;
pub const EVENT_TOPIC_LLM_CALL_FAILED: &str = shared_types::EVENT_TOPIC_LLM_CALL_FAILED;
pub const EVENT_TOPIC_WORKER_TOOL_CALL: &str = shared_types::EVENT_TOPIC_WORKER_TOOL_CALL;
pub const EVENT_TOPIC_WORKER_TOOL_RESULT: &str = shared_types::EVENT_TOPIC_WORKER_TOOL_RESULT;

pub const MAX_SYSTEM_CONTEXT_BYTES: usize = 4 * 1024;
pub const MAX_INPUT_BYTES: usize = 16 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024;

pub const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "api_key",
    "token",
    "password",
    "secret",
    "credential",
];

#[derive(Debug, Clone, Default)]
pub struct LlmCallScope {
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub call_id: Option<String>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmTokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

impl LlmTokenUsage {
    pub fn total_tokens(&self) -> Option<i64> {
        match (self.input_tokens, self.output_tokens) {
            (Some(input), Some(output)) => Some(input.saturating_add(output)),
            (Some(input), None) => Some(input),
            (None, Some(output)) => Some(output),
            (None, None) => None,
        }
    }

    fn to_json_value(&self) -> Option<serde_json::Value> {
        let mut usage = serde_json::Map::new();
        if let Some(input_tokens) = self.input_tokens {
            usage.insert("input_tokens".to_string(), serde_json::json!(input_tokens));
        }
        if let Some(output_tokens) = self.output_tokens {
            usage.insert(
                "output_tokens".to_string(),
                serde_json::json!(output_tokens),
            );
        }
        if let Some(cached_input_tokens) = self.cached_input_tokens {
            usage.insert(
                "cached_input_tokens".to_string(),
                serde_json::json!(cached_input_tokens),
            );
        }
        if let Some(total_tokens) = self.total_tokens() {
            usage.insert("total_tokens".to_string(), serde_json::json!(total_tokens));
        }
        if usage.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(usage))
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmCallContext {
    pub trace_id: String,
    pub role: String,
    pub function_name: String,
    pub actor_id: String,
    pub started_at: DateTime<Utc>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub call_id: Option<String>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallContext {
    pub tool_trace_id: String,
    pub role: String,
    pub tool_name: String,
    pub actor_id: String,
    pub started_at: DateTime<Utc>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub call_id: Option<String>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LlmTraceEmitter {
    event_store: ActorRef<EventStoreMsg>,
}

impl LlmTraceEmitter {
    pub fn new(event_store: ActorRef<EventStoreMsg>) -> Self {
        Self { event_store }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_call(
        &self,
        role: &str,
        function_name: &str,
        actor_id: &str,
        model_used: &str,
        provider: Option<&str>,
        system_context: &str,
        input: &serde_json::Value,
        input_summary: &str,
        scope: Option<LlmCallScope>,
    ) -> LlmCallContext {
        let trace_id = ulid::Ulid::new().to_string();
        let started_at = Utc::now();
        let scope = scope.unwrap_or_default();

        let (truncated_system_context, sc_truncated, sc_original_size) =
            truncate_to_bytes(system_context, MAX_SYSTEM_CONTEXT_BYTES);

        let mut input_clone = input.clone();
        redact_sensitive_keys(&mut input_clone);
        let input_json = serde_json::to_string(&input_clone).unwrap_or_else(|_| "{}".to_string());
        let (truncated_input, i_truncated, i_original_size) =
            truncate_to_bytes(&input_json, MAX_INPUT_BYTES);

        let mut payload = serde_json::json!({
            "trace_id": trace_id,
            "role": role,
            "function_name": function_name,
            "model_used": model_used,
            "actor_id": actor_id,
            "started_at": started_at.to_rfc3339(),
            "system_context": truncated_system_context,
            "input": truncated_input,
            "input_summary": input_summary,
        });

        if let Some(obj) = payload.as_object_mut() {
            if let Some(p) = provider {
                obj.insert("provider".to_string(), serde_json::json!(p));
            }
            if sc_truncated {
                obj.insert(
                    "system_context_truncated".to_string(),
                    serde_json::json!({
                        "truncated": true,
                        "original_size": sc_original_size,
                    }),
                );
            }
            if i_truncated {
                obj.insert(
                    "input_truncated".to_string(),
                    serde_json::json!({
                        "truncated": true,
                        "original_size": i_original_size,
                    }),
                );
            }
            if let Some(ref run_id) = scope.run_id {
                obj.insert("run_id".to_string(), serde_json::json!(run_id));
            }
            if let Some(ref task_id) = scope.task_id {
                obj.insert("task_id".to_string(), serde_json::json!(task_id));
            }
            if let Some(ref call_id) = scope.call_id {
                obj.insert("call_id".to_string(), serde_json::json!(call_id));
            }
            if scope.session_id.is_some() || scope.thread_id.is_some() {
                let mut scope_obj = serde_json::Map::new();
                if let Some(ref session_id) = scope.session_id {
                    scope_obj.insert("session_id".to_string(), serde_json::json!(session_id));
                }
                if let Some(ref thread_id) = scope.thread_id {
                    scope_obj.insert("thread_id".to_string(), serde_json::json!(thread_id));
                }
                obj.insert("scope".to_string(), serde_json::Value::Object(scope_obj));
            }
        }

        let event = AppendEvent {
            event_type: EVENT_TOPIC_LLM_CALL_STARTED.to_string(),
            payload,
            actor_id: actor_id.to_string(),
            user_id: "system".to_string(),
        };

        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });

        LlmCallContext {
            trace_id,
            role: role.to_string(),
            function_name: function_name.to_string(),
            actor_id: actor_id.to_string(),
            started_at,
            run_id: scope.run_id,
            task_id: scope.task_id,
            call_id: scope.call_id,
            session_id: scope.session_id,
            thread_id: scope.thread_id,
        }
    }

    pub fn complete_call(
        &self,
        ctx: &LlmCallContext,
        model_used: &str,
        provider: Option<&str>,
        output: &serde_json::Value,
        output_summary: &str,
    ) {
        self.complete_call_with_usage(ctx, model_used, provider, output, output_summary, None);
    }

    pub fn complete_call_with_usage(
        &self,
        ctx: &LlmCallContext,
        model_used: &str,
        provider: Option<&str>,
        output: &serde_json::Value,
        output_summary: &str,
        usage: Option<LlmTokenUsage>,
    ) {
        let ended_at = Utc::now();
        let duration_ms = (ended_at - ctx.started_at).num_milliseconds().max(0) as u64;

        let mut output_clone = output.clone();
        redact_sensitive_keys(&mut output_clone);
        let output_json =
            serde_json::to_string(&output_clone).unwrap_or_else(|_| "null".to_string());
        let (truncated_output, o_truncated, o_original_size) =
            truncate_to_bytes(&output_json, MAX_OUTPUT_BYTES);

        let mut payload = serde_json::json!({
            "trace_id": ctx.trace_id,
            "role": ctx.role,
            "function_name": ctx.function_name,
            "model_used": model_used,
            "actor_id": ctx.actor_id,
            "started_at": ctx.started_at.to_rfc3339(),
            "ended_at": ended_at.to_rfc3339(),
            "duration_ms": duration_ms,
            "output": truncated_output,
            "output_summary": output_summary,
        });

        if let Some(obj) = payload.as_object_mut() {
            if let Some(p) = provider {
                obj.insert("provider".to_string(), serde_json::json!(p));
            }
            if o_truncated {
                obj.insert(
                    "output_truncated".to_string(),
                    serde_json::json!({
                        "truncated": true,
                        "original_size": o_original_size,
                    }),
                );
            }
            if let Some(usage_value) = usage.and_then(|u| u.to_json_value()) {
                obj.insert("usage".to_string(), usage_value);
            }
            if let Some(ref run_id) = ctx.run_id {
                obj.insert("run_id".to_string(), serde_json::json!(run_id));
            }
            if let Some(ref task_id) = ctx.task_id {
                obj.insert("task_id".to_string(), serde_json::json!(task_id));
            }
            if let Some(ref call_id) = ctx.call_id {
                obj.insert("call_id".to_string(), serde_json::json!(call_id));
            }
            if ctx.session_id.is_some() || ctx.thread_id.is_some() {
                let mut scope_obj = serde_json::Map::new();
                if let Some(ref session_id) = ctx.session_id {
                    scope_obj.insert("session_id".to_string(), serde_json::json!(session_id));
                }
                if let Some(ref thread_id) = ctx.thread_id {
                    scope_obj.insert("thread_id".to_string(), serde_json::json!(thread_id));
                }
                obj.insert("scope".to_string(), serde_json::Value::Object(scope_obj));
            }
        }

        let event = AppendEvent {
            event_type: EVENT_TOPIC_LLM_CALL_COMPLETED.to_string(),
            payload,
            actor_id: ctx.actor_id.clone(),
            user_id: "system".to_string(),
        };

        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    pub fn fail_call(
        &self,
        ctx: &LlmCallContext,
        model_used: &str,
        provider: Option<&str>,
        error_code: Option<&str>,
        error_message: &str,
        failure_kind: Option<FailureKind>,
    ) {
        self.fail_call_with_usage(
            ctx,
            model_used,
            provider,
            error_code,
            error_message,
            failure_kind,
            None,
        );
    }

    pub fn fail_call_with_usage(
        &self,
        ctx: &LlmCallContext,
        model_used: &str,
        provider: Option<&str>,
        error_code: Option<&str>,
        error_message: &str,
        failure_kind: Option<FailureKind>,
        usage: Option<LlmTokenUsage>,
    ) {
        let ended_at = Utc::now();
        let duration_ms = (ended_at - ctx.started_at).num_milliseconds().max(0) as u64;

        let mut payload = serde_json::json!({
            "trace_id": ctx.trace_id,
            "role": ctx.role,
            "function_name": ctx.function_name,
            "model_used": model_used,
            "actor_id": ctx.actor_id,
            "started_at": ctx.started_at.to_rfc3339(),
            "ended_at": ended_at.to_rfc3339(),
            "duration_ms": duration_ms,
            "error_message": error_message,
        });

        if let Some(obj) = payload.as_object_mut() {
            if let Some(p) = provider {
                obj.insert("provider".to_string(), serde_json::json!(p));
            }
            if let Some(code) = error_code {
                obj.insert("error_code".to_string(), serde_json::json!(code));
            }
            if let Some(kind) = failure_kind {
                obj.insert("failure_kind".to_string(), serde_json::json!(kind));
            }
            if let Some(usage_value) = usage.and_then(|u| u.to_json_value()) {
                obj.insert("usage".to_string(), usage_value);
            }
            if let Some(ref run_id) = ctx.run_id {
                obj.insert("run_id".to_string(), serde_json::json!(run_id));
            }
            if let Some(ref task_id) = ctx.task_id {
                obj.insert("task_id".to_string(), serde_json::json!(task_id));
            }
            if let Some(ref call_id) = ctx.call_id {
                obj.insert("call_id".to_string(), serde_json::json!(call_id));
            }
            if ctx.session_id.is_some() || ctx.thread_id.is_some() {
                let mut scope_obj = serde_json::Map::new();
                if let Some(ref session_id) = ctx.session_id {
                    scope_obj.insert("session_id".to_string(), serde_json::json!(session_id));
                }
                if let Some(ref thread_id) = ctx.thread_id {
                    scope_obj.insert("thread_id".to_string(), serde_json::json!(thread_id));
                }
                obj.insert("scope".to_string(), serde_json::Value::Object(scope_obj));
            }
        }

        let event = AppendEvent {
            event_type: EVENT_TOPIC_LLM_CALL_FAILED.to_string(),
            payload,
            actor_id: ctx.actor_id.clone(),
            user_id: "system".to_string(),
        };

        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }

    pub fn start_tool_call(
        &self,
        role: &str,
        actor_id: &str,
        tool_name: &str,
        tool_args: &serde_json::Value,
        reasoning: Option<&str>,
        scope: Option<LlmCallScope>,
    ) -> ToolCallContext {
        let tool_trace_id = ulid::Ulid::new().to_string();
        let started_at = Utc::now();
        let scope = scope.unwrap_or_default();

        let mut args_value = tool_args.clone();
        redact_sensitive_keys(&mut args_value);
        let args_json = serde_json::to_string(&args_value).unwrap_or_else(|_| "{}".to_string());
        let (truncated_args, args_truncated, args_original_size) =
            truncate_to_bytes(&args_json, MAX_INPUT_BYTES);

        let mut payload = serde_json::json!({
            "tool_trace_id": tool_trace_id,
            "role": role,
            "tool_name": tool_name,
            "actor_id": actor_id,
            "started_at": started_at.to_rfc3339(),
            "tool_args": truncated_args,
        });

        if let Some(obj) = payload.as_object_mut() {
            if args_truncated {
                obj.insert(
                    "tool_args_truncated".to_string(),
                    serde_json::json!({
                        "truncated": true,
                        "original_size": args_original_size,
                    }),
                );
            }
            if let Some(reasoning) = reasoning {
                obj.insert("reasoning".to_string(), serde_json::json!(reasoning));
            }
            inject_scope_fields(
                obj,
                &scope.run_id,
                &scope.task_id,
                &scope.call_id,
                &scope.session_id,
                &scope.thread_id,
            );
        }

        self.emit_append_event(EVENT_TOPIC_WORKER_TOOL_CALL, payload, actor_id);

        ToolCallContext {
            tool_trace_id,
            role: role.to_string(),
            tool_name: tool_name.to_string(),
            actor_id: actor_id.to_string(),
            started_at,
            run_id: scope.run_id,
            task_id: scope.task_id,
            call_id: scope.call_id,
            session_id: scope.session_id,
            thread_id: scope.thread_id,
        }
    }

    pub fn complete_tool_call(
        &self,
        ctx: &ToolCallContext,
        success: bool,
        output: &str,
        error: Option<&str>,
    ) {
        let ended_at = Utc::now();
        let duration_ms = (ended_at - ctx.started_at).num_milliseconds().max(0) as u64;

        let (truncated_output, output_truncated, output_original_size) =
            truncate_to_bytes(output, MAX_OUTPUT_BYTES);

        let mut payload = serde_json::json!({
            "tool_trace_id": ctx.tool_trace_id,
            "role": ctx.role,
            "tool_name": ctx.tool_name,
            "actor_id": ctx.actor_id,
            "success": success,
            "started_at": ctx.started_at.to_rfc3339(),
            "ended_at": ended_at.to_rfc3339(),
            "duration_ms": duration_ms,
            "output": truncated_output,
        });

        if let Some(obj) = payload.as_object_mut() {
            if output_truncated {
                obj.insert(
                    "output_truncated".to_string(),
                    serde_json::json!({
                        "truncated": true,
                        "original_size": output_original_size,
                    }),
                );
            }
            if let Some(error) = error {
                obj.insert("error".to_string(), serde_json::json!(error));
            }
            inject_scope_fields(
                obj,
                &ctx.run_id,
                &ctx.task_id,
                &ctx.call_id,
                &ctx.session_id,
                &ctx.thread_id,
            );
        }

        self.emit_append_event(EVENT_TOPIC_WORKER_TOOL_RESULT, payload, &ctx.actor_id);
    }

    fn emit_append_event(&self, event_type: &str, payload: serde_json::Value, actor_id: &str) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: actor_id.to_string(),
            user_id: "system".to_string(),
        };
        let _ = self
            .event_store
            .send_message(EventStoreMsg::AppendAsync { event });
    }
}

pub fn token_usage_from_collector(collector: &baml::Collector) -> Option<LlmTokenUsage> {
    let usage = collector.usage();
    let input_tokens = usage.input_tokens();
    let output_tokens = usage.output_tokens();
    let cached_input_tokens = usage.cached_input_tokens();

    if input_tokens <= 0 && output_tokens <= 0 && cached_input_tokens.unwrap_or(0) <= 0 {
        return None;
    }

    Some(LlmTokenUsage {
        input_tokens: Some(input_tokens),
        output_tokens: Some(output_tokens),
        cached_input_tokens,
    })
}

fn inject_scope_fields(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    run_id: &Option<String>,
    task_id: &Option<String>,
    call_id: &Option<String>,
    session_id: &Option<String>,
    thread_id: &Option<String>,
) {
    if let Some(run_id) = run_id {
        obj.insert("run_id".to_string(), serde_json::json!(run_id));
    }
    if let Some(task_id) = task_id {
        obj.insert("task_id".to_string(), serde_json::json!(task_id));
    }
    if let Some(call_id) = call_id {
        obj.insert("call_id".to_string(), serde_json::json!(call_id));
    }
    if session_id.is_some() || thread_id.is_some() {
        let mut scope_obj = serde_json::Map::new();
        if let Some(session_id) = session_id {
            scope_obj.insert("session_id".to_string(), serde_json::json!(session_id));
        }
        if let Some(thread_id) = thread_id {
            scope_obj.insert("thread_id".to_string(), serde_json::json!(thread_id));
        }
        obj.insert("scope".to_string(), serde_json::Value::Object(scope_obj));
    }
}

pub fn truncate_to_bytes(text: &str, max_bytes: usize) -> (String, bool, usize) {
    let original_size = text.len();
    if original_size <= max_bytes {
        return (text.to_string(), false, original_size);
    }

    let mut byte_count = 0;
    for (idx, ch) in text.char_indices() {
        let char_len = ch.len_utf8();
        if byte_count + char_len > max_bytes {
            let truncated = text[..idx].to_string();
            return (truncated, true, original_size);
        }
        byte_count += char_len;
    }

    (text.to_string(), false, original_size)
}

pub fn redact_sensitive_keys(json: &mut serde_json::Value) {
    match json {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                let key_lower = key.to_lowercase();
                if SENSITIVE_KEYS.iter().any(|k| key_lower.contains(k)) {
                    *value = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_sensitive_keys(value);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_sensitive_keys(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_bytes_no_truncation_needed() {
        let text = "Hello, world!";
        let (result, truncated, original_size) = truncate_to_bytes(text, 100);
        assert_eq!(result, text);
        assert!(!truncated);
        assert_eq!(original_size, 13);
    }

    #[test]
    fn test_truncate_to_bytes_exact_fit() {
        let text = "Hello";
        let (result, truncated, original_size) = truncate_to_bytes(text, 5);
        assert_eq!(result, text);
        assert!(!truncated);
        assert_eq!(original_size, 5);
    }

    #[test]
    fn test_truncate_to_bytes_needs_truncation() {
        let text = "Hello, world!";
        let (result, truncated, original_size) = truncate_to_bytes(text, 5);
        assert_eq!(result, "Hello");
        assert!(truncated);
        assert_eq!(original_size, 13);
    }

    #[test]
    fn test_truncate_to_bytes_unicode_safe() {
        let text = "Hello 🌍";
        let (result, truncated, _) = truncate_to_bytes(text, 7);
        assert_eq!(result, "Hello ");
        assert!(truncated);
    }

    #[test]
    fn test_truncate_to_bytes_preserves_valid_unicode() {
        let text = "aábβc";
        let (result, truncated, _) = truncate_to_bytes(text, 5);
        assert_eq!(result, "aáb");
        assert!(truncated);
    }

    #[test]
    fn test_redact_sensitive_keys_simple() {
        let mut json = serde_json::json!({
            "api_key": "secret123",
            "name": "test"
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["api_key"], "[REDACTED]");
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_redact_sensitive_keys_nested() {
        let mut json = serde_json::json!({
            "config": {
                "authorization": "Bearer token123",
                "endpoint": "https://api.example.com"
            }
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["config"]["authorization"], "[REDACTED]");
        assert_eq!(json["config"]["endpoint"], "https://api.example.com");
    }

    #[test]
    fn test_redact_sensitive_keys_array() {
        let mut json = serde_json::json!({
            "items": [
                {"token": "abc123", "name": "item1"},
                {"password": "secret", "name": "item2"}
            ]
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["items"][0]["token"], "[REDACTED]");
        assert_eq!(json["items"][1]["password"], "[REDACTED]");
        assert_eq!(json["items"][0]["name"], "item1");
    }

    #[test]
    fn test_redact_sensitive_keys_case_insensitive() {
        let mut json = serde_json::json!({
            "API_KEY": "secret",
            "Authorization": "Bearer token",
            "SecretKey": "hidden"
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["API_KEY"], "[REDACTED]");
        assert_eq!(json["Authorization"], "[REDACTED]");
        assert_eq!(json["SecretKey"], "[REDACTED]");
    }

    #[test]
    fn test_redact_sensitive_keys_partial_match() {
        let mut json = serde_json::json!({
            "my_api_key_value": "secret",
            "x-authorization-header": "Bearer token",
            "user_credential_store": "data"
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["my_api_key_value"], "[REDACTED]");
        assert_eq!(json["x-authorization-header"], "[REDACTED]");
        assert_eq!(json["user_credential_store"], "[REDACTED]");
    }

    #[test]
    fn test_redact_sensitive_keys_non_sensitive_preserved() {
        let mut json = serde_json::json!({
            "model": "gpt-4",
            "temperature": 0.7,
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "max_tokens": 100
        });
        redact_sensitive_keys(&mut json);
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["temperature"], 0.7);
        assert_eq!(json["messages"][0]["content"], "Hello");
    }

    #[test]
    fn test_llm_call_scope_default() {
        let scope = LlmCallScope::default();
        assert!(scope.run_id.is_none());
        assert!(scope.task_id.is_none());
        assert!(scope.call_id.is_none());
        assert!(scope.session_id.is_none());
        assert!(scope.thread_id.is_none());
    }

    #[test]
    fn test_constants_match_shared_types() {
        assert_eq!(
            EVENT_TOPIC_LLM_CALL_STARTED,
            shared_types::EVENT_TOPIC_LLM_CALL_STARTED
        );
        assert_eq!(
            EVENT_TOPIC_LLM_CALL_COMPLETED,
            shared_types::EVENT_TOPIC_LLM_CALL_COMPLETED
        );
        assert_eq!(
            EVENT_TOPIC_LLM_CALL_FAILED,
            shared_types::EVENT_TOPIC_LLM_CALL_FAILED
        );
        assert_eq!(
            EVENT_TOPIC_WORKER_TOOL_CALL,
            shared_types::EVENT_TOPIC_WORKER_TOOL_CALL
        );
        assert_eq!(
            EVENT_TOPIC_WORKER_TOOL_RESULT,
            shared_types::EVENT_TOPIC_WORKER_TOOL_RESULT
        );
    }
}
