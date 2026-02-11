// ChoirOS Conductor BAML Contracts
// Orchestration functions for the Conductor actor to manage multi-step agent workflows

// ============================================================================
// Decision Types
// ============================================================================

enum DecisionType {
  Dispatch
  Retry
  SpawnFollowup
  Continue
  Complete
  Block
}

enum TerminalityStatus {
  Complete
  Blocked
  Continue
}

// ============================================================================
// Supporting Types
// ============================================================================

class ConductorAgendaItem {
  id string
  capability string
  objective string
  dependencies string[]
  status string
  priority int
}

class ConductorCapabilityCall {
  call_id string
  agenda_item_id string
  capability string
  objective string
  status string
}

class ConductorArtifact {
  artifact_id string
  name string
  content_type string
  summary string
}

class FollowupRecommendation {
  capability string
  objective string
  priority int
  rationale string
}

class WorkerOutput {
  call_id string
  agenda_item_id string
  status string
  result_summary string
  artifacts_produced ConductorArtifact[]
  followup_recommendations FollowupRecommendation[]
}

class EventSummary {
  event_id string
  event_type string
  timestamp string
  payload string
}

class RetryPolicy {
  max_attempts int
  backoff_strategy string
}

// ============================================================================
// Function 1: ConductorDecideNextAction
// ============================================================================

class ConductorDecisionInput {
  run_id string
  task_id string
  objective string
  run_status string
  agenda ConductorAgendaItem[]
  active_calls ConductorCapabilityCall[]
  artifacts ConductorArtifact[]
  recent_events EventSummary[]
  worker_outputs WorkerOutput[]
}

class ConductorDecisionOutput {
  decision_type DecisionType
  target_agenda_item_ids string[]
  new_agenda_items ConductorAgendaItem[]
  capability string?
  objective string?
  retry_policy RetryPolicy?
  completion_reason string?
  confidence float
  rationale string
}

function ConductorDecideNextAction(input: ConductorDecisionInput) -> ConductorDecisionOutput {
  client Orchestrator
  prompt #"
    You are the ChoirOS Conductor, an orchestration AI that manages multi-step agent workflows.

    Your role is to analyze the current run state and decide the next orchestration action.

    Current Run State:
    - Run ID: {{ input.run_id }}
    - Task ID: {{ input.task_id }}
    - Objective: {{ input.objective }}
    - Run Status: {{ input.run_status }}

    Agenda Items:
    {{ input.agenda }}

    Active Calls:
    {{ input.active_calls }}

    Artifacts Collected:
    {{ input.artifacts }}

    Recent Events:
    {{ input.recent_events }}

    Worker Outputs:
    {{ input.worker_outputs }}

    Analyze the current state and decide the next action:

    1. **Dispatch** - Start a new capability call when:
       - There are agenda items with status "ready" (dependencies satisfied)
       - No conflicting active calls exist
       - The objective requires more work

    2. **Retry** - Retry a failed call when:
       - A call failed with a recoverable error
       - Retry budget has not been exhausted
       - Adjusted parameters might succeed

    3. **SpawnFollowup** - Add new agenda items when:
       - Worker outputs recommend follow-up capabilities
       - New requirements discovered during execution
       - Sub-tasks needed to complete the objective

    4. **Continue** - Wait for in-flight calls when:
       - Active calls are still running
       - No ready agenda items can be started yet
       - Waiting for dependencies to complete

    5. **Complete** - Mark run as finished when:
       - All agenda items are completed
       - The original objective is achieved
       - Artifacts satisfy success criteria

    6. **Block** - Mark run as blocked when:
       - Unrecoverable errors occurred
       - Human intervention is required
       - Objective cannot be achieved with available capabilities

    Decision Guidelines:
    - Always check dependencies before dispatching
    - Consider priority when selecting agenda items
    - Set confidence based on clarity of the decision (0.0-1.0)
    - Provide clear rationale for observability
    - Use exact enum values for decision_type: Dispatch, Retry, SpawnFollowup, Continue, Complete, Block

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Function 2: ConductorRefineObjective
// ============================================================================

class ConductorObjectiveRefineInput {
  raw_objective string
  context WorkerOutput[]
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
// Function 3: ConductorBootstrapAgenda
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
    - Return zero capabilities when the run should be blocked immediately.
    - If dispatch_capabilities is empty, block_reason is required.
    - rationale must explain why these capabilities were selected.
    - confidence is 0.0-1.0.
    - Avoid deterministic threshold logic and use semantic task fit.

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Function 4: ConductorAssessTerminality
// ============================================================================

class ConductorTerminalityInput {
  run_id string
  agenda ConductorAgendaItem[]
  artifacts ConductorArtifact[]
  original_objective string
}

class ConductorTerminalityOutput {
  terminality_status TerminalityStatus
  reason string
  confidence float
}

function ConductorAssessTerminality(input: ConductorTerminalityInput) -> ConductorTerminalityOutput {
  client Orchestrator
  prompt #"
    You are the ChoirOS Conductor, assessing whether a workflow run has reached a terminal state.

    Run ID: {{ input.run_id }}

    Original Objective: {{ input.original_objective }}

    Current Agenda State:
    {{ input.agenda }}

    Artifacts Collected:
    {{ input.artifacts }}

    Assess whether the run should:

    1. **Complete** - The objective has been achieved when:
       - All agenda items are completed successfully
       - Artifacts satisfy the original objective
       - Success criteria have been met
       - No further work is needed

    2. **Blocked** - The run is blocked when:
       - One or more agenda items failed and cannot be retried
       - Required dependencies are unavailable
       - Human intervention is needed to proceed
       - The objective cannot be achieved with current resources

    3. **Continue** - The run should continue when:
       - Agenda items are still pending or in progress
       - Failed items can be retried
       - Additional work is needed to achieve the objective
       - Follow-up tasks have been spawned

    Assessment Guidelines:
    - Review the original objective carefully
    - Check if artifacts adequately address the objective
    - Consider partial success scenarios
    - Set confidence based on clarity of the assessment (0.0-1.0)
    - Provide specific reason referencing agenda state and artifacts
    - Use exact enum values for terminality_status: Complete, Blocked, Continue

    {{ ctx.output_format }}
  "#
}
