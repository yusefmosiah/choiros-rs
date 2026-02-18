//! ConductorHarnessAdapter — WorkerPort implementation for the conductor wake turn.
//!
//! Phase 4.3: conductor runs a brief AgentHarness turn (HarnessProfile::Conductor,
//! max_steps=10, 10s budget) to produce a routing decision.  The model is given
//! the objective, available capabilities, optional memory context, and routing
//! guidance.  It calls `finished(summary=<json>)` where the JSON encodes the
//! capability routing decision.
//!
//! ## Contract
//!
//! Allowed tools: `finished` only.  No direct tool execution occurs.
//! The `finished.summary` field must be a JSON string of the form:
//!
//! ```json
//! {
//!   "dispatch_capabilities": ["writer"],
//!   "rationale": "...",
//!   "confidence": 0.9,
//!   "block_reason": null
//! }
//! ```
//!
//! This mirrors `ConductorBootstrapOutput` so the existing capability-dispatch
//! machinery is reused without modification.
//!
//! If the summary cannot be parsed the caller falls back to the legacy single-shot
//! BAML path.

use async_trait::async_trait;

use crate::actors::agent_harness::{
    AgentProgress, ExecutionContext, HarnessError, ToolExecution, WorkerPort,
};
use crate::baml_client::types::Union8BashToolCallOrFetchUrlToolCallOrFileEditToolCallOrFileReadToolCallOrFileWriteToolCallOrFinishedToolCallOrMessageWriterToolCallOrWebSearchToolCall as AgentToolCall;

// ─────────────────────────────────────────────────────────────────────────────

const CONDUCTOR_ROUTING_GUIDANCE: &str = r#"You are a conductor orchestrator making a single routing decision.

Your only job is to decide which capabilities to dispatch for the given objective.

Available capabilities and their contracts:
- "immediate_response": respond directly and briefly to the user. Use ONLY for short conversational acknowledgements (hi, ping, status checks). Do NOT use for substantive tasks.
- "writer": app-agent with synthesis and document-update authority. Delegates to researcher/terminal workers as needed. Use for any substantive writing, research, or multi-step task.

Routing rules:
1. For every non-empty objective, dispatch at least one capability.
2. Never route researcher/terminal directly — writer owns worker delegation.
3. Use immediate_response ONLY for trivial conversational inputs.
4. Leave dispatch_capabilities empty only if truly blocked; supply a concrete block_reason.

Respond by calling the `finished` tool with a JSON summary in exactly this shape:
{
  "dispatch_capabilities": ["writer"],
  "rationale": "<brief reason>",
  "confidence": 0.9,
  "block_reason": null
}
"#;

/// `WorkerPort` implementation for the conductor's wake-time routing turn.
///
/// The conductor harness turn is intentionally minimal:
/// - Only `finished` is allowed (no tool execution).
/// - The model encodes its routing decision in `finished.summary` as JSON.
/// - `HarnessProfile::Conductor` enforces max_steps=10 and a 10s budget.
pub struct ConductorHarnessAdapter {
    objective: String,
    available_capabilities: Vec<String>,
    memory_context: Option<String>,
}

impl ConductorHarnessAdapter {
    pub fn new(
        objective: String,
        available_capabilities: Vec<String>,
        memory_context: Option<String>,
    ) -> Self {
        Self {
            objective,
            available_capabilities,
            memory_context,
        }
    }
}

#[async_trait]
impl WorkerPort for ConductorHarnessAdapter {
    fn get_model_role(&self) -> &str {
        "conductor"
    }

    fn get_tool_description(&self) -> String {
        r#"Available tools:

1. finished — Report the routing decision and complete this turn.
   Args:
   - summary: string (required) — JSON routing decision (see system prompt format)
"#
        .to_string()
    }

    /// Only `finished` is permitted. Any other tool is a contract violation.
    fn allowed_tool_names(&self) -> Option<&'static [&'static str]> {
        Some(&["finished"])
    }

    fn get_system_context(&self, _ctx: &ExecutionContext) -> String {
        let caps_list = self.available_capabilities.join(", ");
        let memory_section = self
            .memory_context
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|ctx| format!("\n\nRetrieved memory context (relevance-ranked):\n{ctx}"))
            .unwrap_or_default();

        format!(
            "{routing_guidance}\nAvailable capabilities: {caps_list}{memory_section}\n\nObjective:\n{objective}",
            routing_guidance = CONDUCTOR_ROUTING_GUIDANCE,
            caps_list = caps_list,
            memory_section = memory_section,
            objective = self.objective,
        )
    }

    /// No tools are actually executed in the conductor turn.
    /// This method should never be called because `allowed_tool_names` only
    /// permits `finished`, which is handled by the harness itself before
    /// calling execute_tool_call.
    async fn execute_tool_call(
        &self,
        _ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        let tool_name = match tool_call {
            AgentToolCall::BashToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::WebSearchToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FetchUrlToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileReadToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileWriteToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FileEditToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::MessageWriterToolCall(c) => c.tool_name.as_str(),
            AgentToolCall::FinishedToolCall(c) => c.tool_name.as_str(),
        };
        Err(HarnessError::ToolExecution(format!(
            "Conductor harness does not execute tools; '{tool_name}' is not permitted"
        )))
    }

    fn should_defer(&self, _tool_name: &str) -> bool {
        false
    }

    async fn emit_worker_report(
        &self,
        _ctx: &ExecutionContext,
        _report: shared_types::WorkerTurnReport,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn emit_progress(
        &self,
        _ctx: &ExecutionContext,
        _progress: AgentProgress,
    ) -> Result<(), HarnessError> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The routing decision extracted from the conductor harness `finished.summary`.
///
/// Mirrors `ConductorBootstrapOutput` so the existing dispatch machinery works
/// without modification.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConductorRoutingDecision {
    pub dispatch_capabilities: Vec<String>,
    pub rationale: String,
    pub confidence: f64,
    pub block_reason: Option<String>,
}

/// Parse the conductor harness `AgentResult.summary` as a `ConductorRoutingDecision`.
///
/// Returns `None` when the summary cannot be parsed — the caller should fall back
/// to the legacy BAML path in that case.
pub fn parse_routing_decision(summary: &str) -> Option<ConductorRoutingDecision> {
    let trimmed = summary.trim();

    // The model might wrap the JSON in a markdown code fence. Strip it.
    let json_str = if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        inner.trim_start().trim_end_matches("```").trim()
    } else {
        trimmed
    };

    serde_json::from_str(json_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_routing_decision_plain_json() {
        let json = r#"{"dispatch_capabilities":["writer"],"rationale":"needs research","confidence":0.9,"block_reason":null}"#;
        let result = parse_routing_decision(json);
        assert!(result.is_some());
        let d = result.unwrap();
        assert_eq!(d.dispatch_capabilities, vec!["writer"]);
        assert!((d.confidence - 0.9).abs() < 1e-9);
        assert!(d.block_reason.is_none());
    }

    #[test]
    fn test_parse_routing_decision_fenced_json() {
        let json = "```json\n{\"dispatch_capabilities\":[\"immediate_response\"],\"rationale\":\"simple ping\",\"confidence\":1.0,\"block_reason\":null}\n```";
        let result = parse_routing_decision(json);
        assert!(result.is_some());
        let d = result.unwrap();
        assert_eq!(d.dispatch_capabilities, vec!["immediate_response"]);
    }

    #[test]
    fn test_parse_routing_decision_invalid_returns_none() {
        let result = parse_routing_decision("not json at all");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_routing_decision_empty_returns_none() {
        let result = parse_routing_decision("");
        assert!(result.is_none());
    }

    #[test]
    fn test_conductor_adapter_allowed_tools() {
        let adapter = ConductorHarnessAdapter::new(
            "test objective".to_string(),
            vec!["writer".to_string()],
            None,
        );
        let allowed = adapter.allowed_tool_names();
        assert_eq!(allowed, Some(["finished"].as_ref()));
    }

    #[test]
    fn test_conductor_adapter_model_role() {
        let adapter = ConductorHarnessAdapter::new("obj".to_string(), vec![], None);
        assert_eq!(adapter.get_model_role(), "conductor");
    }

    #[test]
    fn test_conductor_adapter_system_context_contains_objective() {
        let adapter = ConductorHarnessAdapter::new(
            "write a report".to_string(),
            vec!["writer".to_string()],
            None,
        );
        let ctx = ExecutionContext {
            loop_id: "loop1".to_string(),
            worker_id: "conductor".to_string(),
            user_id: "system".to_string(),
            step_number: 1,
            max_steps: 10,
            model_used: "opus".to_string(),
            objective: "write a report".to_string(),
            run_id: None,
            call_id: None,
        };
        let sys = adapter.get_system_context(&ctx);
        assert!(sys.contains("write a report"));
        assert!(sys.contains("writer"));
    }

    #[test]
    fn test_conductor_adapter_system_context_includes_memory() {
        let adapter = ConductorHarnessAdapter::new(
            "obj".to_string(),
            vec!["writer".to_string()],
            Some("prior run summary".to_string()),
        );
        let ctx = ExecutionContext {
            loop_id: "l".to_string(),
            worker_id: "c".to_string(),
            user_id: "s".to_string(),
            step_number: 1,
            max_steps: 10,
            model_used: "m".to_string(),
            objective: "obj".to_string(),
            run_id: None,
            call_id: None,
        };
        let sys = adapter.get_system_context(&ctx);
        assert!(sys.contains("prior run summary"));
    }
}
