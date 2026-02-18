// ChoirOS RLM (Recursive Language Model) Harness Contract
//
// The RLM harness is the GENERAL execution mode. The model outputs a program
// each turn that specifies:
//   1. What context to load (sources)
//   2. Its reasoning state (working_memory)
//   3. What to do next (next_action)
//
// Linear tool loops are NextAction::ToolCalls — one degenerate case.
// Conductor single-shot is an RLM with max_turns=1.
// Everything is an RLM.

// ============================================================================
// Context Sources — the model selects what to include each turn
// ============================================================================

enum ContextSourceKind {
  /// Query episodic memory for relevant patterns
  MemoryQuery
  /// Load a specific file or document
  Document
  /// Selectively include output from a previous turn (not automatic)
  PreviousTurn
  /// Include a specific tool execution result
  ToolOutput
}

class ContextSource {
  kind ContextSourceKind
  /// For MemoryQuery: the search query. For Document: the file path.
  /// For PreviousTurn: turn number as string. For ToolOutput: call_id.
  source_ref string
  /// Why this source is needed (helps the harness prioritize under budget)
  rationale string?
  /// Max tokens to allocate to this source (model manages its own budget)
  max_tokens int?
}

// ============================================================================
// Next Action — the model controls topology
// ============================================================================

enum NextActionKind {
  /// Execute tools sequentially (linear mode — the common/degenerate case)
  ToolCalls
  /// Execute a multi-step program (DAG of operations with variable refs,
  /// conditionals, and embedded LLM calls). This is the computationally
  /// universal execution mode. ToolCalls is sugar for a single-layer DAG
  /// where every step is op=ToolCall with no dependencies.
  Program
  /// Spawn parallel branches with different approaches
  FanOut
  /// Delegate to a sub-harness with fresh context
  Recurse
  /// Terminal: objective achieved
  Complete
  /// Terminal: cannot proceed
  Block
}

class ToolCallSpec {
  tool_name string
  tool_args map<string, string>
  reasoning string?
}

// ============================================================================
// Execution DAG — the program the model writes
// ============================================================================
//
// Each DagStep is a node. Steps reference other steps' outputs via
// ${step_id} in their arguments. The harness executes in dependency order,
// substituting resolved values. Steps can be conditional on prior step
// outputs via the `condition` field.
//
// This is computationally universal: it has sequencing (deps), variables
// (${ref}), conditionals (condition), I/O (ToolCall, Bash), and embedded
// LLM calls (LlmCall). The harness traces every node.

enum StepOp {
  /// Execute a tool (bash, file_read, file_write, file_edit, web_search, fetch_url).
  /// Same tools available as ToolCallSpec but within a DAG.
  ToolCall

  /// Call an LLM with a composed prompt. The prompt can reference prior
  /// step outputs via ${step_id}. The harness resolves the model.
  LlmCall

  /// Pure string transformation: extract via regex, truncate, format.
  /// No I/O, no side effects. The harness evaluates this locally.
  Transform

  /// Conditional gate: evaluate a predicate on a prior step's output.
  /// Downstream steps with `condition: "gate_step_id"` only execute if
  /// the gate evaluates to true. Predicates: contains, not_contains,
  /// matches_regex, equals, not_equals.
  Gate

  /// Send a message to the parent actor (progress report, partial result).
  Emit

  /// Execute a Rhai script. The script has access to registered bindings:
  ///   read_file(path)           -> String
  ///   write_file(path, content) -> ()
  ///   fetch_url(url)            -> String
  ///   shell(cmd)                -> String
  ///   call_llm(prompt)          -> String
  ///   emit_msg(msg)             -> ()
  ///   step_output(id)           -> String  (access prior step outputs)
  /// The script's return value (last expression) becomes this step's output.
  /// Use `eval_code` field for the script source. Prior step outputs are
  /// available via step_output("step_id") or pre-injected as variables
  /// if listed in `eval_inputs`.
  Eval
}

class DagStep {
  /// Unique identifier for this step within the DAG. Other steps reference
  /// this via ${id} in their arguments.
  id string

  /// The operation to perform.
  op StepOp

  /// Step IDs that must complete before this step runs. The harness
  /// enforces topological ordering. Empty = no dependencies (runs first).
  depends_on string[]

  /// If set, this step only executes when the named Gate step evaluated
  /// to true. Skipped steps produce output "(skipped)".
  condition string?

  // ── Per-op arguments ──────────────────────────────────────────────

  /// For ToolCall: the tool name (bash, file_read, etc.)
  tool_name string?
  /// For ToolCall: the tool arguments. Values can contain ${step_id}
  /// refs which the harness substitutes with that step's output.
  tool_args map<string, string>?

  /// For LlmCall: the prompt to send. Can contain ${step_id} refs.
  prompt string?
  /// For LlmCall: optional model hint (e.g. "fast" for flash, "strong"
  /// for opus). The harness resolves this to an actual model ID.
  model_hint string?
  /// For LlmCall: optional system prompt.
  system_prompt string?

  /// For Transform: the operation. Supported: "regex", "truncate",
  /// "json_extract", "template".
  transform_op string?
  /// For Transform: the input (usually a ${step_id} ref).
  transform_input string?
  /// For Transform: the pattern or parameter.
  /// For regex: the regex pattern (first capture group is output).
  /// For truncate: max chars as string. For json_extract: the JSON path.
  /// For template: a string with ${step_id} refs to interpolate.
  transform_pattern string?

  /// For Gate: the predicate to evaluate. Format: "op:value" where op
  /// is one of contains, not_contains, matches, equals, not_equals.
  /// The input is the output of the first dependency.
  gate_predicate string?

  /// For Emit: the message to send to the parent. Can contain ${refs}.
  emit_message string?

  /// For Eval: the Rhai script source code.
  /// Prior step outputs can be accessed via step_output("id") or via
  /// variables pre-injected by listing their step IDs in `eval_inputs`.
  eval_code string?
  /// For Eval: step IDs whose outputs should be injected as variables.
  /// E.g. if eval_inputs=["read", "analyze"], the script can reference
  /// variables `read` and `analyze` directly (as strings).
  eval_inputs string[]?

  /// Human-readable description of what this step does (for tracing).
  description string?
}

class FanOutBranch {
  /// Objective for this branch
  objective string
  /// Optional: different model or config for this branch
  model_hint string?
  /// Context seed: what the branch starts with
  context_seed string?
}

class RecurseSpec {
  /// Objective for the sub-harness
  objective string
  /// Context to pass to the child (not the full parent context)
  context_seed string?
  /// Optional model preference for the child
  model_hint string?
  /// Max steps for the child
  max_steps int?
}

class NextAction {
  kind NextActionKind
  /// For Complete/Block: the reason
  reason string?
  /// For ToolCalls: the tool invocations (simple linear case)
  tool_calls ToolCallSpec[]?
  /// For Program: the execution DAG (computationally universal case)
  program DagStep[]?
  /// For FanOut: the parallel branches
  branches FanOutBranch[]?
  /// For Recurse: the delegation spec
  recurse RecurseSpec?
}

// ============================================================================
// RLM Turn Output — the "program" the model writes each turn
// ============================================================================

class RlmTurn {
  /// What context to load for this turn. The harness resolves these into text.
  /// Empty sources = the model is working from working_memory alone.
  sources ContextSource[]

  /// The model's articulation of its current reasoning state.
  /// This carries focus across turns. It is ephemeral — rewritten each turn.
  /// This IS the metacognition: the model reflecting on what it knows,
  /// what it's uncertain about, and what it needs next.
  working_memory string

  /// What to do next. The harness executes this.
  next_action NextAction
}

// ============================================================================
// Turn Context — what the harness provides to the model each turn
// ============================================================================

class RlmTurnContext {
  /// The original objective (constant across turns)
  objective string
  /// Current turn number
  turn_number int
  /// Max turns remaining
  max_turns int
  /// The model's working_memory from the previous turn (empty on turn 1)
  previous_working_memory string?
  /// Resolved context from the previous turn's source requests
  /// (assembled by the harness, not the model)
  assembled_context string?
  /// Results from the previous turn's action execution
  action_results string?
  /// Summary of all completed turns so far (compressed by harness)
  turn_history_summary string?
}

// ============================================================================
// The RLM Compose Function
// ============================================================================

function RlmCompose(turn_ctx: RlmTurnContext, capabilities: string) -> RlmTurn {
  client Orchestrator
  prompt #"
    You are operating within a Recursive Language Model (RLM) harness.

    OBJECTIVE: {{ turn_ctx.objective }}
    TURN: {{ turn_ctx.turn_number }}/{{ turn_ctx.max_turns }}

    {% if turn_ctx.previous_working_memory %}
    YOUR PREVIOUS WORKING MEMORY:
    {{ turn_ctx.previous_working_memory }}
    {% endif %}

    {% if turn_ctx.assembled_context %}
    ASSEMBLED CONTEXT (from your previous source requests):
    {{ turn_ctx.assembled_context }}
    {% endif %}

    {% if turn_ctx.action_results %}
    RESULTS FROM PREVIOUS ACTION:
    {{ turn_ctx.action_results }}
    {% endif %}

    {% if turn_ctx.turn_history_summary %}
    TURN HISTORY:
    {{ turn_ctx.turn_history_summary }}
    {% endif %}

    CAPABILITIES:
    {{ capabilities }}

    YOUR TASK: Output an RlmTurn that specifies:

    1. SOURCES — What context do you need? Request specific documents,
       memory queries, or prior turn outputs. The harness will resolve
       these and provide them on the next turn. Leave empty if you have
       enough context already.

    2. WORKING MEMORY — Articulate your current reasoning state. What do
       you know? What are you uncertain about? What's your plan? This
       carries your focus across turns. Be specific and concise.

    3. NEXT ACTION — What to do now:
       - ToolCalls: Execute tools sequentially (simple case, no dependencies between calls).
       - Program: Execute a DAG of steps with dependencies, variable references,
         conditionals, and embedded LLM calls. THIS IS THE POWERFUL MODE. Use it
         when you need multi-step computation where later steps depend on earlier
         outputs.
       - FanOut: Spawn parallel branches when you need to explore alternatives.
       - Recurse: Delegate a sub-task to a fresh harness with its own context.
       - Complete: You're done. Provide the final answer in reason.
       - Block: You can't proceed. Explain why in reason.

    PROGRAM (DAG) AUTHORING:
    When you choose kind=Program, populate the `program` field with DagStep objects.
    Each step has an `id` and an `op`:

    - ToolCall: Execute a tool. Set tool_name and tool_args.
    - LlmCall: Call an LLM. Set prompt (and optionally model_hint, system_prompt).
    - Transform: Pure string manipulation. Set transform_op, transform_input, transform_pattern.
      Ops: "regex" (first capture group), "truncate" (max chars), "json_extract" (path), "template" (interpolate).
    - Gate: Conditional. Set gate_predicate as "op:value" (contains, not_contains, matches, equals, not_equals).
      Downstream steps set condition=gate_step_id to only run if the gate is true.
    - Emit: Report to parent. Set emit_message.
    - Eval: Execute a Rhai script. Set eval_code with the Rhai source. Available bindings:
        read_file(path) -> String, write_file(path, content),
        fetch_url(url) -> String, shell(cmd) -> String,
        call_llm(prompt) -> String, emit_msg(msg),
        step_output(id) -> String (access prior step output by ID).
      Optionally set eval_inputs to a list of step IDs — their outputs are injected as
      same-named variables (e.g. eval_inputs:["read"] → variable `read` is available).
      The script's last expression is the step output.

    Variable references: Use ${step_id} in any string argument to inject that step's output.
    Dependencies: Set depends_on to list step IDs that must complete first.
    The harness executes in topological order and traces every step.

    EXAMPLE PROGRAM (analyze a file, conditionally deep-review):
    [
      { id: "read", op: ToolCall, depends_on: [], tool_name: "file_read", tool_args: { "path": "src/auth.rs" } },
      { id: "analyze", op: LlmCall, depends_on: ["read"], prompt: "Analyze for security issues:\n${read}" },
      { id: "is_critical", op: Gate, depends_on: ["analyze"], gate_predicate: "contains:CRITICAL" },
      { id: "deep", op: LlmCall, depends_on: ["read", "analyze"], condition: "is_critical",
        prompt: "Deep security review of:\n${read}\n\nInitial analysis:\n${analyze}", model_hint: "strong" },
      { id: "report", op: Emit, depends_on: ["analyze", "deep"],
        emit_message: "Security review complete. ${deep}" }
    ]

    GUIDELINES:
    - Use ToolCalls for simple one-or-two-step actions with no dependencies between them.
    - Use Program when steps depend on each other's outputs, or when you need
      conditionals, LLM calls within the execution, or multi-stage data transformation.
    - Use FanOut when multiple approaches seem viable and you need to explore.
    - Use Recurse when a sub-task is complex enough to benefit from fresh context.
    - Your working_memory is your scratch pad — use it to think out loud.
    - You control your own context. Don't ask for sources you don't need.
    - If you have enough information, go straight to Complete.

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Freeform LLM Call — used by DAG executor for embedded LlmCall steps
// ============================================================================

function DagLlmCall(prompt: string, system_prompt: string?) -> string {
  client Orchestrator
  prompt #"
    {% if system_prompt %}
    {{ system_prompt }}
    {% endif %}

    {{ prompt }}
  "#
}
