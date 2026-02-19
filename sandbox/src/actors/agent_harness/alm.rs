//! RLM (Recursive Language Model) Harness
//!
//! Every turn the model outputs an `RlmTurn`:
//!   - `sources`: what context to load (documents, memory, prior turns)
//!   - `working_memory`: the model's articulated reasoning state
//!   - `next_action`: what to do next
//!
//! ## Action kinds
//!
//! **ToolCalls** — flat list of tool calls, no dependencies. The degenerate
//! case; equivalent to a single-layer DAG where every step is a ToolCall.
//!
//! **Program** — a DAG of typed operations (`DagStep`) with dependency
//! ordering, variable substitution, conditional gates, embedded LLM calls,
//! and pure transforms. Useful for deterministic multi-step workflows:
//! megacontext processing, business logic pipelines, structured data
//! extraction. Every step is traced. NOT a general scripting runtime —
//! side-effectful execution (shell, network, agent spawning) routes through
//! the actor system, not inline.
//!
//! **FanOut** — fire N `HarnessMsg::Execute` messages to the actor
//! system. Returns correlation IDs immediately (non-blocking). The model
//! reads results in a subsequent turn via `ContextSourceKind::ToolOutput`.
//!
//! **Recurse** — fire 1 `HarnessMsg::Execute`. Same non-blocking pattern.
//!
//! **Complete / Block** — terminal actions.
//!
//! ## Design rule
//! The harness never blocks waiting for an actor. FanOut and Recurse are
//! fire-and-forget: the step output is a correlation ID, not a result.
//! Result collection is the model's responsibility on the next turn.

use std::collections::HashMap;
use std::time::Instant;

use chrono::{Duration, Utc};
use regex::Regex;
use tracing::{debug, info, warn};

use crate::actors::model_config::ModelRegistry;
use crate::baml_client::types::{
    ContextSourceKind, DagStep, NextActionKind, RlmTurn, RlmTurnContext, StepOp,
};
use crate::baml_client::{new_collector, ClientRegistry, B};

// ─── Public types ────────────────────────────────────────────────────────────

/// Result of an RLM harness run.
#[derive(Debug, Clone)]
pub struct RlmRunResult {
    pub final_working_memory: String,
    pub completion_reason: String,
    pub turns_taken: usize,
    pub tool_executions: Vec<AlmToolExecution>,
    pub turn_log: Vec<RlmTurnLog>,
    pub dag_traces: Vec<DagTrace>,
}

/// Record of a single tool execution within the RLM loop.
#[derive(Debug, Clone)]
pub struct AlmToolExecution {
    pub turn: usize,
    pub tool_name: String,
    pub tool_args: HashMap<String, String>,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// Trace of a single DAG step execution (for observability).
#[derive(Debug, Clone)]
pub struct DagStepTrace {
    pub step_id: String,
    pub op: String,
    pub description: Option<String>,
    pub skipped: bool,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// Trace of a full DAG execution (one per Program action).
#[derive(Debug, Clone)]
pub struct DagTrace {
    pub turn: usize,
    pub steps: Vec<DagStepTrace>,
    pub total_elapsed_ms: u64,
}

/// Record of a single RLM turn (for observability/eval).
#[derive(Debug, Clone)]
pub struct RlmTurnLog {
    pub turn_number: usize,
    pub working_memory: String,
    pub sources_requested: Vec<String>,
    pub action_kind: String,
    pub action_summary: String,
    pub elapsed_ms: u64,
}

/// Configuration for the RLM harness.
#[derive(Debug, Clone)]
pub struct AlmConfig {
    pub max_turns: usize,
    pub max_recurse_depth: usize,
    pub timeout_budget_ms: u64,
    /// Maximum steps allowed in a single DAG program.
    pub max_dag_steps: usize,
}

impl Default for AlmConfig {
    fn default() -> Self {
        Self {
            max_turns: 15,
            max_recurse_depth: 3,
            timeout_budget_ms: 60_000,
            max_dag_steps: 30,
        }
    }
}

/// Result of executing a single LLM call within a DAG.
#[derive(Debug, Clone)]
pub struct LlmCallResult {
    pub output: String,
    pub success: bool,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// Trait for executing tools, LLM calls, and resolving context sources.
///
/// This is the execution boundary — the harness calls the model, the port
/// executes actions. Analogous to `WorkerPort` but designed for RLM semantics.
#[async_trait::async_trait]
pub trait AlmPort: Send + Sync {
    /// Get the capabilities description (tool list, available resources).
    fn capabilities_description(&self) -> String;

    /// Get the model ID to use for this port.
    fn model_id(&self) -> &str;

    /// Stable run identifier. Used as the partition key for checkpoints.
    fn run_id(&self) -> &str;

    /// Actor identifier (for EventStore actor_id field).
    fn actor_id(&self) -> &str;

    /// Resolve a context source into text.
    ///
    /// The harness calls this for each `ContextSource` in the model's output.
    /// Returns the resolved text, or None if the source can't be resolved.
    ///
    /// `ContextSourceKind::ToolOutput` with `source_ref = corr_id` reads the
    /// `tool.result` event from EventStore — this is the async reply wait point.
    async fn resolve_source(
        &self,
        kind: &ContextSourceKind,
        source_ref: &str,
        max_tokens: Option<i64>,
    ) -> Option<String>;

    /// Fire a tool call asynchronously.
    ///
    /// Sends a message to the appropriate actor and returns a corr_id
    /// immediately. The result will be written to EventStore as `tool.result`
    /// and can be retrieved via `resolve_source(ToolOutput, corr_id)`.
    ///
    /// This is non-blocking by design. The harness turn ends after firing.
    /// The model reads results on the next turn by requesting the corr_id
    /// as a context source.
    async fn dispatch_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
        corr_id: &str,
    );

    /// Execute a tool call synchronously (blocking within the turn).
    ///
    /// Used only for DAG steps where the next step directly depends on this
    /// result within the same turn. For all top-level tool calls, prefer
    /// `dispatch_tool` (async, non-blocking).
    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &HashMap<String, String>,
    ) -> AlmToolExecution;

    /// Execute an LLM call within a DAG step.
    async fn call_llm(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        model_hint: Option<&str>,
    ) -> LlmCallResult;

    /// Emit a message to the parent actor (progress report, partial result).
    async fn emit_message(&self, message: &str);

    /// Write a harness checkpoint to durable storage (EventStore).
    ///
    /// Called at every turn boundary where the harness fires outbound messages.
    /// On crash+restart the supervisor reads the latest checkpoint and recovers.
    async fn write_checkpoint(&self, checkpoint: &shared_types::HarnessCheckpoint);

    /// Fire a `HarnessMsg::Execute` to the actor system and return immediately.
    ///
    /// This is the FanOut/Recurse execution primitive. The harness calls this
    /// for each branch; the port sends the message to a fresh HarnessActor.
    /// The reply arrives later as a `subharness.result` event in EventStore,
    /// keyed by `corr_id`. The model reads it in a subsequent turn via
    /// `resolve_source(ToolOutput, corr_id)`.
    ///
    /// Implementations must be non-blocking — spawn the actor and return.
    async fn spawn_harness(&self, objective: &str, context: serde_json::Value, corr_id: &str);
}

// ─── DAG Executor ────────────────────────────────────────────────────────────

/// Outputs of completed DAG steps, keyed by step ID.
pub type StepOutputs = HashMap<String, String>;

/// Substitute `${step_id}` references in a string with resolved outputs.
fn substitute_refs(template: &str, outputs: &StepOutputs) -> String {
    let mut result = template.to_string();
    // Match ${identifier} patterns — identifiers are alphanumeric + underscore
    let re = Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}").expect("valid regex");
    // Iterate in reverse to preserve indices during replacement
    let captures: Vec<_> = re.captures_iter(template).collect();
    for cap in captures.iter().rev() {
        let full_match = cap.get(0).unwrap();
        let step_id = &cap[1];
        let replacement = outputs
            .get(step_id)
            .map(|s| s.as_str())
            .unwrap_or("(unresolved)");
        result.replace_range(full_match.range(), replacement);
    }
    result
}

/// Evaluate a gate predicate against an input string.
/// Format: "op:value" where op is contains, not_contains, matches, equals, not_equals.
fn evaluate_gate(predicate: &str, input: &str) -> bool {
    let (op, value) = match predicate.split_once(':') {
        Some((op, val)) => (op.trim(), val.trim()),
        None => {
            warn!("Gate predicate missing ':' separator: {predicate}");
            return false;
        }
    };

    match op {
        "contains" => input.contains(value),
        "not_contains" => !input.contains(value),
        "equals" => input.trim() == value,
        "not_equals" => input.trim() != value,
        "matches" => Regex::new(value)
            .map(|re| re.is_match(input))
            .unwrap_or_else(|e| {
                warn!("Gate regex error: {e}");
                false
            }),
        other => {
            warn!("Unknown gate operation: {other}");
            false
        }
    }
}

/// Execute a Transform step.
fn execute_transform(transform_op: &str, input: &str, pattern: &str) -> Result<String, String> {
    match transform_op {
        "regex" => {
            let re = Regex::new(pattern).map_err(|e| format!("regex error: {e}"))?;
            match re.captures(input) {
                Some(caps) => {
                    // Return first capture group if present, else full match
                    Ok(caps
                        .get(1)
                        .or_else(|| caps.get(0))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default())
                }
                None => Ok(String::new()),
            }
        }
        "truncate" => {
            let max: usize = pattern
                .parse()
                .map_err(|e| format!("truncate length parse error: {e}"))?;
            if input.len() <= max {
                Ok(input.to_string())
            } else {
                Ok(format!("{}...", &input[..max]))
            }
        }
        "json_extract" => {
            // Simple dotted path extraction from JSON
            let value: serde_json::Value =
                serde_json::from_str(input).map_err(|e| format!("JSON parse error: {e}"))?;
            let mut current = &value;
            for key in pattern.split('.') {
                current = current
                    .get(key)
                    .ok_or_else(|| format!("JSON path '{key}' not found"))?;
            }
            match current {
                serde_json::Value::String(s) => Ok(s.clone()),
                other => Ok(other.to_string()),
            }
        }
        "template" => {
            // Pattern is a template string; input is ignored (refs already substituted)
            Ok(pattern.to_string())
        }
        other => Err(format!("Unknown transform op: {other}")),
    }
}

// ─── Topological sort ────────────────────────────────────────────────────────
/// Topological sort of DAG steps. Returns step indices in execution order.
/// Detects cycles and returns an error if one is found.
fn topological_sort(steps: &[DagStep]) -> Result<Vec<usize>, String> {
    let n = steps.len();
    let id_to_idx: HashMap<&str, usize> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();

    // Build adjacency: for each step, which steps depend on it
    let mut in_degree = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (i, step) in steps.iter().enumerate() {
        for dep_id in &step.depends_on {
            match id_to_idx.get(dep_id.as_str()) {
                Some(&dep_idx) => {
                    dependents[dep_idx].push(i);
                    in_degree[i] += 1;
                }
                None => {
                    return Err(format!(
                        "Step '{}' depends on unknown step '{}'",
                        step.id, dep_id
                    ));
                }
            }
        }
        // Condition is also a dependency (on the gate step)
        if let Some(cond_id) = &step.condition {
            if let Some(&cond_idx) = id_to_idx.get(cond_id.as_str()) {
                // Only add as dependency if not already in depends_on
                if !step.depends_on.contains(cond_id) {
                    dependents[cond_idx].push(i);
                    in_degree[i] += 1;
                }
            } else {
                return Err(format!(
                    "Step '{}' condition references unknown gate '{}'",
                    step.id, cond_id
                ));
            }
        }
    }

    // Kahn's algorithm
    let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);

    while let Some(idx) = queue.pop() {
        order.push(idx);
        for &dep in &dependents[idx] {
            in_degree[dep] -= 1;
            if in_degree[dep] == 0 {
                queue.push(dep);
            }
        }
    }

    if order.len() != n {
        return Err("Cycle detected in DAG".to_string());
    }

    Ok(order)
}

/// Execute a DAG program. Returns a trace and the final step outputs.
pub async fn execute_dag<P: AlmPort>(
    port: &P,
    steps: &[DagStep],
    turn: usize,
    max_steps: usize,
) -> Result<(DagTrace, StepOutputs, Vec<AlmToolExecution>), String> {
    if steps.is_empty() {
        return Ok((
            DagTrace {
                turn,
                steps: vec![],
                total_elapsed_ms: 0,
            },
            HashMap::new(),
            vec![],
        ));
    }

    if steps.len() > max_steps {
        return Err(format!(
            "DAG has {} steps, exceeding max of {max_steps}",
            steps.len()
        ));
    }

    let dag_start = Instant::now();
    let execution_order = topological_sort(steps)?;

    let mut outputs: StepOutputs = HashMap::new();
    let mut gate_results: HashMap<String, bool> = HashMap::new();
    let mut step_traces: Vec<DagStepTrace> = Vec::new();
    let mut tool_executions: Vec<AlmToolExecution> = Vec::new();

    for &idx in &execution_order {
        let step = &steps[idx];
        let step_start = Instant::now();

        // Check condition — skip if gate was false
        if let Some(cond_id) = &step.condition {
            let gate_val = gate_results.get(cond_id.as_str()).copied().unwrap_or(false);
            if !gate_val {
                debug!(
                    "DAG step '{}' skipped (gate '{}' was false)",
                    step.id, cond_id
                );
                outputs.insert(step.id.clone(), "(skipped)".to_string());
                step_traces.push(DagStepTrace {
                    step_id: step.id.clone(),
                    op: format!("{:?}", step.op),
                    description: step.description.clone(),
                    skipped: true,
                    success: true,
                    output: "(skipped)".to_string(),
                    error: None,
                    elapsed_ms: 0,
                });
                continue;
            }
        }

        let result: Result<String, String> =
            match step.op {
                StepOp::ToolCall => {
                    let tool_name = step
                        .tool_name
                        .as_deref()
                        .ok_or_else(|| format!("Step '{}': ToolCall missing tool_name", step.id))?;
                    let raw_args = step.tool_args.as_ref().cloned().unwrap_or_default();
                    // Substitute ${refs} in all arg values
                    let args: HashMap<String, String> = raw_args
                        .into_iter()
                        .map(|(k, v)| (k, substitute_refs(&v, &outputs)))
                        .collect();

                    let exec = port.execute_tool(tool_name, &args).await;
                    let output = if exec.success {
                        exec.output.clone()
                    } else {
                        format!(
                            "ERROR: {}",
                            exec.error.as_deref().unwrap_or("unknown error")
                        )
                    };
                    let success = exec.success;
                    tool_executions.push(AlmToolExecution { turn, ..exec });
                    if success {
                        Ok(output)
                    } else {
                        // Tool errors are non-fatal to the DAG — downstream steps
                        // see the error text and can gate on it
                        Ok(output)
                    }
                }

                StepOp::LlmCall => {
                    let raw_prompt = step
                        .prompt
                        .as_deref()
                        .ok_or_else(|| format!("Step '{}': LlmCall missing prompt", step.id))?;
                    let prompt = substitute_refs(raw_prompt, &outputs);
                    let system_prompt = step
                        .system_prompt
                        .as_deref()
                        .map(|s| substitute_refs(s, &outputs));

                    let llm_result = port
                        .call_llm(
                            &prompt,
                            system_prompt.as_deref(),
                            step.model_hint.as_deref(),
                        )
                        .await;

                    if llm_result.success {
                        Ok(llm_result.output)
                    } else {
                        Err(format!(
                            "LLM call failed: {}",
                            llm_result.error.as_deref().unwrap_or("unknown")
                        ))
                    }
                }

                StepOp::Transform => {
                    let op = step.transform_op.as_deref().ok_or_else(|| {
                        format!("Step '{}': Transform missing transform_op", step.id)
                    })?;
                    let raw_input = step.transform_input.as_deref().ok_or_else(|| {
                        format!("Step '{}': Transform missing transform_input", step.id)
                    })?;
                    let input = substitute_refs(raw_input, &outputs);
                    let raw_pattern = step.transform_pattern.as_deref().ok_or_else(|| {
                        format!("Step '{}': Transform missing transform_pattern", step.id)
                    })?;
                    let pattern = substitute_refs(raw_pattern, &outputs);

                    execute_transform(op, &input, &pattern)
                }

                StepOp::Gate => {
                    let predicate = step.gate_predicate.as_deref().ok_or_else(|| {
                        format!("Step '{}': Gate missing gate_predicate", step.id)
                    })?;
                    // Gate input is the output of the first dependency
                    let dep_output = step
                        .depends_on
                        .first()
                        .and_then(|dep_id| outputs.get(dep_id))
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    let gate_value = evaluate_gate(predicate, dep_output);
                    gate_results.insert(step.id.clone(), gate_value);

                    Ok(format!("{gate_value}"))
                }

                StepOp::Emit => {
                    let raw_msg = step
                        .emit_message
                        .as_deref()
                        .ok_or_else(|| format!("Step '{}': Emit missing emit_message", step.id))?;
                    let msg = substitute_refs(raw_msg, &outputs);
                    port.emit_message(&msg).await;
                    Ok(format!("(emitted: {})", truncate_str(&msg, 200)))
                }

                StepOp::Eval => {
                    // Eval is a placeholder for Phase 4.5: async execution via
                    // TerminalActor message (non-blocking, returns corr_id).
                    // For now, surface the code field so the model can see what
                    // it requested, pending actor-message wiring.
                    let code = step.eval_code.as_deref().unwrap_or("(no code)");
                    Err(format!(
                        "StepOp::Eval not yet wired to TerminalActor. \
                     Code was: {}",
                        &code[..code.len().min(200)]
                    ))
                }
            };

        let elapsed_ms = step_start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                info!(
                    "DAG step '{}' ({:?}) completed in {}ms",
                    step.id, step.op, elapsed_ms
                );
                outputs.insert(step.id.clone(), output.clone());
                step_traces.push(DagStepTrace {
                    step_id: step.id.clone(),
                    op: format!("{:?}", step.op),
                    description: step.description.clone(),
                    skipped: false,
                    success: true,
                    output: truncate_str(&output, 4000).to_string(),
                    error: None,
                    elapsed_ms,
                });
            }
            Err(err) => {
                warn!("DAG step '{}' ({:?}) failed: {}", step.id, step.op, err);
                let error_output = format!("ERROR: {err}");
                outputs.insert(step.id.clone(), error_output.clone());
                step_traces.push(DagStepTrace {
                    step_id: step.id.clone(),
                    op: format!("{:?}", step.op),
                    description: step.description.clone(),
                    skipped: false,
                    success: false,
                    output: String::new(),
                    error: Some(err),
                    elapsed_ms,
                });
            }
        }
    }

    let total_elapsed_ms = dag_start.elapsed().as_millis() as u64;

    Ok((
        DagTrace {
            turn,
            steps: step_traces,
            total_elapsed_ms,
        },
        outputs,
        tool_executions,
    ))
}

// ─── The RLM Harness ─────────────────────────────────────────────────────────

pub struct AlmHarness<P: AlmPort> {
    port: P,
    model_registry: ModelRegistry,
    config: AlmConfig,
}

impl<P: AlmPort> AlmHarness<P> {
    pub fn new(port: P, model_registry: ModelRegistry, config: AlmConfig) -> Self {
        Self {
            port,
            model_registry,
            config,
        }
    }

    /// Run the RLM loop.
    ///
    /// Each turn:
    /// 1. Call `RlmCompose` with the current turn context
    /// 2. Resolve the sources the model requested
    /// 3. Execute the next_action (ToolCalls, Program, FanOut, Recurse)
    /// 4. Feed results back into the next turn's context
    /// 5. Repeat until Complete/Block or budget exhausted
    pub async fn run(&self, objective: String) -> Result<RlmRunResult, String> {
        self.run_inner(objective).await
    }

    async fn run_inner(&self, objective: String) -> Result<RlmRunResult, String> {
        let model_id = self.port.model_id();
        let client_registry = self
            .model_registry
            .create_runtime_client_registry_for_model(model_id)
            .map_err(|e| format!("model registry: {e}"))?;

        let capabilities = self.port.capabilities_description();
        let mut working_memory: Option<String> = None;
        let mut assembled_context: Option<String> = None;
        let mut action_results: Option<String> = None;
        let mut turn_history: Vec<String> = Vec::new();
        let mut all_tool_executions: Vec<AlmToolExecution> = Vec::new();
        let mut all_dag_traces: Vec<DagTrace> = Vec::new();
        let mut turn_log: Vec<RlmTurnLog> = Vec::new();
        // Durability tracking
        let mut pending_replies: Vec<shared_types::PendingReply> = Vec::new();
        let mut pending_corr_ids: Vec<String> = Vec::new();
        let mut turn_summaries: Vec<shared_types::TurnSummary> = Vec::new();
        // Recurse/FanOut depth tracking — counts subharness dispatches fired.
        // Once this reaches max_recurse_depth no further recursive dispatches
        // are allowed; the action is converted to a Block result instead.
        let mut recurse_dispatches: usize = 0;

        // Deadline from the configured budget. Checked at the top of each turn.
        let run_deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(self.config.timeout_budget_ms);

        for turn in 1..=self.config.max_turns {
            // Enforce wall-clock budget before starting each new turn.
            if tokio::time::Instant::now() >= run_deadline {
                tracing::warn!(
                    run_id = %self.port.run_id(),
                    turn,
                    budget_ms = self.config.timeout_budget_ms,
                    "RLM harness timeout budget exceeded"
                );
                return Ok(RlmRunResult {
                    final_working_memory: working_memory.unwrap_or_default(),
                    completion_reason: format!(
                        "timeout: exceeded {}ms budget after {} turns",
                        self.config.timeout_budget_ms,
                        turn - 1
                    ),
                    turns_taken: turn - 1,
                    tool_executions: all_tool_executions,
                    turn_log,
                    dag_traces: all_dag_traces,
                });
            }

            let turn_start = Instant::now();
            // Each new turn clears prior-turn pending state — if we reached
            // this turn, all prior pending replies were resolved.
            pending_replies.clear();
            pending_corr_ids.clear();

            // Build turn history summary (compressed)
            let history_summary = if turn_history.is_empty() {
                None
            } else {
                Some(turn_history.join("\n"))
            };

            // 1. Call RlmCompose
            let turn_ctx = RlmTurnContext {
                objective: objective.clone(),
                turn_number: turn as i64,
                max_turns: self.config.max_turns as i64,
                previous_working_memory: working_memory.clone(),
                assembled_context: assembled_context.take(),
                action_results: action_results.take(),
                turn_history_summary: history_summary,
            };

            let rlm_turn = self
                .call_compose(&client_registry, &turn_ctx, &capabilities)
                .await?;

            // 2. Record working memory
            working_memory = Some(rlm_turn.working_memory.clone());

            // 3. Resolve sources for the NEXT turn's assembled_context
            let mut resolved_sources = Vec::new();
            for source in &rlm_turn.sources {
                if let Some(text) = self
                    .port
                    .resolve_source(&source.kind, &source.source_ref, source.max_tokens)
                    .await
                {
                    resolved_sources.push(format!(
                        "[{:?} {}]\n{}",
                        source.kind, source.source_ref, text
                    ));
                }
            }
            if !resolved_sources.is_empty() {
                assembled_context = Some(resolved_sources.join("\n\n"));
            }

            // 4. Execute next_action
            let action = &rlm_turn.next_action;
            let action_kind = format!("{:?}", action.kind);

            match action.kind {
                NextActionKind::Complete => {
                    let reason = action
                        .reason
                        .clone()
                        .unwrap_or_else(|| "completed".to_string());

                    turn_log.push(RlmTurnLog {
                        turn_number: turn,
                        working_memory: rlm_turn.working_memory.clone(),
                        sources_requested: rlm_turn
                            .sources
                            .iter()
                            .map(|s| format!("{:?}:{}", s.kind, s.source_ref))
                            .collect(),
                        action_kind: action_kind.clone(),
                        action_summary: reason.clone(),
                        elapsed_ms: turn_start.elapsed().as_millis() as u64,
                    });

                    return Ok(RlmRunResult {
                        final_working_memory: rlm_turn.working_memory,
                        completion_reason: reason,
                        turns_taken: turn,
                        tool_executions: all_tool_executions,
                        turn_log,
                        dag_traces: all_dag_traces,
                    });
                }

                NextActionKind::Block => {
                    let reason = action
                        .reason
                        .clone()
                        .unwrap_or_else(|| "blocked".to_string());

                    turn_log.push(RlmTurnLog {
                        turn_number: turn,
                        working_memory: rlm_turn.working_memory.clone(),
                        sources_requested: vec![],
                        action_kind: action_kind.clone(),
                        action_summary: reason.clone(),
                        elapsed_ms: turn_start.elapsed().as_millis() as u64,
                    });

                    return Ok(RlmRunResult {
                        final_working_memory: rlm_turn.working_memory,
                        completion_reason: format!("BLOCKED: {reason}"),
                        turns_taken: turn,
                        tool_executions: all_tool_executions,
                        turn_log,
                        dag_traces: all_dag_traces,
                    });
                }

                NextActionKind::ToolCalls => {
                    // Degenerate case: flat list of tool calls, no dependencies.
                    let tool_calls = action.tool_calls.as_deref().unwrap_or(&[]);
                    let mut results = Vec::new();

                    for tc in tool_calls {
                        let args: HashMap<String, String> = tc
                            .tool_args
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();

                        let exec = self.port.execute_tool(&tc.tool_name, &args).await;
                        let summary = if exec.success {
                            format!(
                                "[{}] OK: {}",
                                tc.tool_name,
                                truncate_str(&exec.output, 2000)
                            )
                        } else {
                            format!(
                                "[{}] ERROR: {}",
                                tc.tool_name,
                                exec.error.as_deref().unwrap_or("unknown")
                            )
                        };
                        results.push(summary);
                        all_tool_executions.push(exec);
                    }

                    action_results = Some(results.join("\n\n"));
                }

                NextActionKind::Program => {
                    // The computationally universal case: execute a DAG.
                    let steps = action.program.as_deref().unwrap_or(&[]);
                    match execute_dag(&self.port, steps, turn, self.config.max_dag_steps).await {
                        Ok((trace, _outputs, tool_execs)) => {
                            // Build action results summary from the trace
                            let mut summary_parts: Vec<String> = Vec::new();
                            for st in &trace.steps {
                                let status = if st.skipped {
                                    "SKIPPED"
                                } else if st.success {
                                    "OK"
                                } else {
                                    "FAILED"
                                };
                                let out_preview = truncate_str(&st.output, 1000);
                                summary_parts.push(format!(
                                    "[step:{} op:{} {}] {}",
                                    st.step_id, st.op, status, out_preview
                                ));
                            }
                            action_results = Some(summary_parts.join("\n\n"));
                            all_tool_executions.extend(tool_execs);
                            all_dag_traces.push(trace);
                        }
                        Err(dag_err) => {
                            action_results = Some(format!("[DAG EXECUTION ERROR] {dag_err}"));
                        }
                    }
                }

                NextActionKind::FanOut => {
                    let branches = action.branches.as_deref().unwrap_or(&[]);
                    // Enforce max_recurse_depth — each FanOut counts as one
                    // recursive dispatch level.
                    if recurse_dispatches >= self.config.max_recurse_depth {
                        tracing::warn!(
                            run_id = %self.port.run_id(),
                            turn,
                            depth = recurse_dispatches,
                            max = self.config.max_recurse_depth,
                            "RLM FanOut blocked: max_recurse_depth reached"
                        );
                        action_results = Some(format!(
                            "[BLOCKED] max_recurse_depth ({}) reached; FanOut not dispatched",
                            self.config.max_recurse_depth
                        ));
                    } else {
                        recurse_dispatches += 1;
                        let now = Utc::now();
                        let timeout = now + Duration::seconds(120);
                        let mut dispatched: Vec<String> = Vec::new();
                        for (i, b) in branches.iter().enumerate() {
                            let corr_id = format!("fanout-{turn}-{i}");
                            info!("FanOut branch {i}: dispatching subharness corr:{corr_id}");
                            // Build context JSON from optional context_seed string
                            let ctx = b
                                .context_seed
                                .as_deref()
                                .map(|s| serde_json::json!({ "seed": s }))
                                .unwrap_or(serde_json::Value::Null);
                            // Fire-and-forget: port sends HarnessMsg::Execute and returns immediately
                            self.port
                                .spawn_harness(&b.objective, ctx, &corr_id)
                                .await;
                            pending_corr_ids.push(corr_id.clone());
                            pending_replies.push(shared_types::PendingReply {
                                corr_id: corr_id.clone(),
                                actor_kind: "harness".to_string(),
                                objective_summary: truncate_str(&b.objective, 120).to_string(),
                                sent_at: now,
                                timeout_at: Some(timeout),
                            });
                            dispatched.push(format!("corr:{corr_id} objective:{}", b.objective));
                        }
                        action_results = Some(format!(
                            "FanOut dispatched {} branches:\n{}",
                            dispatched.len(),
                            dispatched.join("\n")
                        ));
                    }
                }

                NextActionKind::Recurse => {
                    let spec = action.recurse.as_ref();
                    let recurse_obj = spec
                        .map(|s| s.objective.clone())
                        .unwrap_or_else(|| "(no objective)".to_string());
                    // Enforce max_recurse_depth.
                    if recurse_dispatches >= self.config.max_recurse_depth {
                        tracing::warn!(
                            run_id = %self.port.run_id(),
                            turn,
                            depth = recurse_dispatches,
                            max = self.config.max_recurse_depth,
                            "RLM Recurse blocked: max_recurse_depth reached"
                        );
                        action_results = Some(format!(
                            "[BLOCKED] max_recurse_depth ({}) reached; Recurse not dispatched for: {}",
                            self.config.max_recurse_depth,
                            truncate_str(&recurse_obj, 120)
                        ));
                    } else {
                        recurse_dispatches += 1;
                        let recurse_ctx = spec
                            .and_then(|s| s.context_seed.as_deref())
                            .map(|s| serde_json::json!({ "seed": s }))
                            .unwrap_or(serde_json::Value::Null);
                        let corr_id = format!("recurse-{turn}");
                        let now = Utc::now();
                        info!("Recurse dispatching subharness corr:{corr_id}");
                        // Fire-and-forget: same non-blocking pattern as FanOut
                        self.port
                            .spawn_harness(&recurse_obj, recurse_ctx, &corr_id)
                            .await;
                        pending_corr_ids.push(corr_id.clone());
                        pending_replies.push(shared_types::PendingReply {
                            corr_id: corr_id.clone(),
                            actor_kind: "harness".to_string(),
                            objective_summary: truncate_str(&recurse_obj, 120).to_string(),
                            sent_at: now,
                            timeout_at: Some(now + Duration::seconds(120)),
                        });
                        action_results = Some(format!(
                            "Recurse dispatched subharness: corr:{corr_id} objective:{recurse_obj}"
                        ));
                    }
                }
            }

            // 5. Record turn in history + write durable checkpoint
            let elapsed_ms = turn_start.elapsed().as_millis() as u64;
            let turn_summary = format!(
                "Turn {}: action={}, wm='{}'",
                turn,
                action_kind,
                truncate_str(&rlm_turn.working_memory, 200),
            );
            turn_history.push(turn_summary.clone());

            // Build compact TurnSummary for checkpoint
            let ts = shared_types::TurnSummary {
                turn_number: turn,
                action_kind: action_kind.clone(),
                working_memory_excerpt: truncate_str(&rlm_turn.working_memory, 300).to_string(),
                corr_ids_fired: pending_corr_ids.drain(..).collect(),
                elapsed_ms,
            };
            turn_summaries.push(ts);

            // Write checkpoint whenever we have pending replies.
            // This is the durability seam: if the process crashes after this
            // write, recovery reads this checkpoint and knows what to wait for.
            if !pending_replies.is_empty() {
                let checkpoint = shared_types::HarnessCheckpoint {
                    run_id: self.port.run_id().to_string(),
                    actor_id: self.port.actor_id().to_string(),
                    turn_number: turn,
                    working_memory: rlm_turn.working_memory.clone(),
                    objective: objective.clone(),
                    pending_replies: pending_replies.clone(),
                    turn_summaries: turn_summaries.clone(),
                    checkpointed_at: Utc::now(),
                };
                self.port.write_checkpoint(&checkpoint).await;
            }

            turn_log.push(RlmTurnLog {
                turn_number: turn,
                working_memory: rlm_turn.working_memory,
                sources_requested: rlm_turn
                    .sources
                    .iter()
                    .map(|s| format!("{:?}:{}", s.kind, s.source_ref))
                    .collect(),
                action_kind,
                action_summary: turn_summary,
                elapsed_ms,
            });
        }

        // Budget exhausted
        Ok(RlmRunResult {
            final_working_memory: working_memory.unwrap_or_default(),
            completion_reason: format!("budget exhausted after {} turns", self.config.max_turns),
            turns_taken: self.config.max_turns,
            tool_executions: all_tool_executions,
            turn_log,
            dag_traces: all_dag_traces,
        })
    }

    async fn call_compose(
        &self,
        client_registry: &ClientRegistry,
        turn_ctx: &RlmTurnContext,
        capabilities: &str,
    ) -> Result<RlmTurn, String> {
        let collector = new_collector("RlmCompose");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            B.RlmCompose
                .with_client_registry(client_registry)
                .with_collector(&collector)
                .call(turn_ctx, capabilities),
        )
        .await
        .map_err(|_| "RlmCompose timed out".to_string())?
        .map_err(|e| format!("RlmCompose call error: {e}"))?;

        Ok(result)
    }
}

fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find the nearest char boundary at or before `max`
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_refs_basic() {
        let mut outputs = StepOutputs::new();
        outputs.insert("read".to_string(), "file contents here".to_string());
        outputs.insert("analyze".to_string(), "analysis result".to_string());

        let result = substitute_refs("Input: ${read}\nAnalysis: ${analyze}", &outputs);
        assert_eq!(
            result,
            "Input: file contents here\nAnalysis: analysis result"
        );
    }

    #[test]
    fn test_substitute_refs_missing() {
        let outputs = StepOutputs::new();
        let result = substitute_refs("Value: ${missing}", &outputs);
        assert_eq!(result, "Value: (unresolved)");
    }

    #[test]
    fn test_substitute_refs_no_refs() {
        let outputs = StepOutputs::new();
        let result = substitute_refs("plain text with $dollar but no {refs}", &outputs);
        assert_eq!(result, "plain text with $dollar but no {refs}");
    }

    #[test]
    fn test_evaluate_gate_contains() {
        assert!(evaluate_gate(
            "contains:CRITICAL",
            "Found CRITICAL issue in auth"
        ));
        assert!(!evaluate_gate("contains:CRITICAL", "All looks fine"));
    }

    #[test]
    fn test_evaluate_gate_not_contains() {
        assert!(evaluate_gate("not_contains:error", "success"));
        assert!(!evaluate_gate("not_contains:error", "found an error"));
    }

    #[test]
    fn test_evaluate_gate_equals() {
        assert!(evaluate_gate("equals:true", "true"));
        assert!(evaluate_gate("equals:true", " true ")); // trimmed
        assert!(!evaluate_gate("equals:true", "false"));
    }

    #[test]
    fn test_evaluate_gate_matches() {
        assert!(evaluate_gate("matches:\\d{3}", "code 404 found"));
        assert!(!evaluate_gate("matches:^\\d+$", "not a number"));
    }

    #[test]
    fn test_execute_transform_regex() {
        let result = execute_transform("regex", "status: 200 OK", r"status: (\d+)").unwrap();
        assert_eq!(result, "200");
    }

    #[test]
    fn test_execute_transform_truncate() {
        let result = execute_transform("truncate", "hello world", "5").unwrap();
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_execute_transform_json_extract() {
        let json = r#"{"status": "ok", "data": {"count": 42}}"#;
        let result = execute_transform("json_extract", json, "status").unwrap();
        assert_eq!(result, "ok");

        let result = execute_transform("json_extract", json, "data.count").unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_topological_sort_simple() {
        let steps = vec![
            DagStep {
                id: "a".to_string(),
                op: StepOp::ToolCall,
                depends_on: vec![],
                condition: None,
                tool_name: Some("bash".to_string()),
                tool_args: None,
                prompt: None,
                model_hint: None,
                system_prompt: None,
                transform_op: None,
                transform_input: None,
                transform_pattern: None,
                gate_predicate: None,
                emit_message: None,
                eval_code: None,
                eval_inputs: None,
                description: None,
            },
            DagStep {
                id: "b".to_string(),
                op: StepOp::LlmCall,
                depends_on: vec!["a".to_string()],
                condition: None,
                tool_name: None,
                tool_args: None,
                prompt: Some("analyze ${a}".to_string()),
                model_hint: None,
                system_prompt: None,
                transform_op: None,
                transform_input: None,
                transform_pattern: None,
                gate_predicate: None,
                emit_message: None,
                eval_code: None,
                eval_inputs: None,
                description: None,
            },
            DagStep {
                id: "c".to_string(),
                op: StepOp::Gate,
                depends_on: vec!["b".to_string()],
                condition: None,
                tool_name: None,
                tool_args: None,
                prompt: None,
                model_hint: None,
                system_prompt: None,
                transform_op: None,
                transform_input: None,
                transform_pattern: None,
                gate_predicate: Some("contains:CRITICAL".to_string()),
                emit_message: None,
                eval_code: None,
                eval_inputs: None,
                description: None,
            },
        ];

        let order = topological_sort(&steps).unwrap();
        // a must come before b, b before c
        let pos_a = order.iter().position(|&x| x == 0).unwrap();
        let pos_b = order.iter().position(|&x| x == 1).unwrap();
        let pos_c = order.iter().position(|&x| x == 2).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_topological_sort_cycle() {
        let steps = vec![
            DagStep {
                id: "a".to_string(),
                op: StepOp::ToolCall,
                depends_on: vec!["b".to_string()],
                condition: None,
                tool_name: None,
                tool_args: None,
                prompt: None,
                model_hint: None,
                system_prompt: None,
                transform_op: None,
                transform_input: None,
                transform_pattern: None,
                gate_predicate: None,
                emit_message: None,
                eval_code: None,
                eval_inputs: None,
                description: None,
            },
            DagStep {
                id: "b".to_string(),
                op: StepOp::ToolCall,
                depends_on: vec!["a".to_string()],
                condition: None,
                tool_name: None,
                tool_args: None,
                prompt: None,
                model_hint: None,
                system_prompt: None,
                transform_op: None,
                transform_input: None,
                transform_pattern: None,
                gate_predicate: None,
                emit_message: None,
                eval_code: None,
                eval_inputs: None,
                description: None,
            },
        ];

        assert!(topological_sort(&steps).unwrap_err().contains("Cycle"));
    }

    #[test]
    fn test_topological_sort_unknown_dep() {
        let steps = vec![DagStep {
            id: "a".to_string(),
            op: StepOp::ToolCall,
            depends_on: vec!["nonexistent".to_string()],
            condition: None,
            tool_name: None,
            tool_args: None,
            prompt: None,
            model_hint: None,
            system_prompt: None,
            transform_op: None,
            transform_input: None,
            transform_pattern: None,
            gate_predicate: None,
            emit_message: None,
            eval_code: None,
            eval_inputs: None,
            description: None,
        }];

        assert!(topological_sort(&steps)
            .unwrap_err()
            .contains("unknown step"));
    }
}
