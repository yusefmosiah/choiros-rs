// ChoirOS Conductor BAML Contracts
// Orchestration functions for the Conductor actor to manage multi-step agent workflows

// ============================================================================
// Simplified Decision Types
// ============================================================================

enum ConductorAction {
  SpawnWorker
  AwaitWorker
  MergeCanon
  Complete
  Block
}

class ConductorDecision {
  action ConductorAction
  args map<string, string>?
  reason string
}

class ConductorDecisionInput {
  run_id string
  objective string
  document_path string
  last_error string?
}

function ConductorDecide(input: ConductorDecisionInput) -> ConductorDecision {
  client Orchestrator
  prompt #"
    You are the ChoirOS Conductor, an orchestration AI.

    Current Run:
    - Run ID: {{ input.run_id }}
    - Objective: {{ input.objective }}
    - Document: {{ input.document_path }}
    {% if input.last_error %}
    - Last Error: {{ input.last_error }}
    {% endif %}

    Read the living document at {{ input.document_path }} to understand the current state.

    Decide the next action:
    - **SpawnWorker**: Dispatch a worker (researcher, terminal) with an objective
    - **AwaitWorker**: Wait for pending worker calls to complete
    - **MergeCanon**: Merge completed worker proposals into canon
    - **Complete**: The objective is achieved
    - **Block**: Cannot proceed (error or needs human help)

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Function: ConductorRefineObjective
// ============================================================================

class ConductorObjectiveRefineInput {
  raw_objective string
  context string[]
  target_capability string
}

class ConductorObjectiveRefineOutput {
  refined_objective string
  success_criteria string[]
  estimated_steps int
  confidence float
}

function ConductorRefineObjective(input: ConductorObjectiveRefineInput) -> ConductorObjectiveRefineOutput {
  client Orchestrator
  prompt #"
    You are the ChoirOS Conductor, refining user objectives into clear, actionable tasks for capability workers.

    Raw Objective: {{ input.raw_objective }}

    Target Capability: {{ input.target_capability }}

    Context from Previous Worker Outputs:
    {{ input.context }}

    Your task is to transform the raw objective into a refined, actionable objective specifically tailored for the target capability.

    Refinement Guidelines:
    1. Make the objective specific and unambiguous
    2. Include relevant context from previous outputs
    3. Frame it in terms the target capability understands
    4. Define clear success criteria (3-5 specific, measurable outcomes)
    5. Estimate the number of steps required (1-10)
    6. Set confidence based on clarity of the refinement (0.0-1.0)

    Success Criteria Guidelines:
    - Each criterion should be verifiable
    - Use specific metrics where possible
    - Include both positive outcomes and negative constraints
    - Consider edge cases and error conditions

    Example Transformations:
    - Raw: "Research this topic" -> Refined: "Search for recent academic papers on X published in 2024, extract key findings, and summarize methodology"
    - Raw: "Fix the bug" -> Refined: "Analyze the error logs in /var/log/app.log, identify the root cause of the timeout issue, and implement a fix with test coverage"

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Function: ConductorBootstrapAgenda
// ============================================================================

class ConductorBootstrapInput {
  raw_objective string
  available_capabilities string[]
}

class ConductorBootstrapOutput {
  dispatch_capabilities string[]
  block_reason string?
  rationale string
  confidence float
}

function ConductorBootstrapAgenda(input: ConductorBootstrapInput) -> ConductorBootstrapOutput {
  client Orchestrator
  prompt #"
    You are the ChoirOS Conductor bootstrap policy.

    Choose which capabilities should be dispatched first for a new run.

    Raw Objective: {{ input.raw_objective }}
    Available Capabilities: {{ input.available_capabilities }}

    Output contract:
    - dispatch_capabilities may include zero, one, or many capability names.
    - Only return capability names from available_capabilities.
    - Conductor routes app-level capabilities (for example: writer, immediate_response), not worker roles.
    - Do not return worker names such as researcher or terminal unless explicitly present in available_capabilities.
    - Use immediate_response only for short conversational acknowledgements (for example: hi, ping, quick status checks).
    - Return zero capabilities when the run should be blocked immediately.
    - If dispatch_capabilities is empty, block_reason is required.
    - rationale must explain why these capabilities were selected.
    - confidence is 0.0-1.0.
    - Avoid deterministic threshold logic and use semantic task fit.

    {{ ctx.output_format }}
  "#
}
