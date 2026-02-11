// ChoirOS Watcher BAML Contracts
// Event-log review and escalation functions for the Watcher actor
//
// The Watcher provides deterministic detection/alerting over logs.
// These BAML functions enable LLM-powered review of event windows for
// anomaly detection and mitigation recommendations.

// ============================================================================
// Input Types
// ============================================================================

class WatcherLogWindowInput {
  window_id string
  run_id string
  task_id string
  events WatcherEvent[]
  scope ReviewScope
  review_reason string?  // Why this window is being reviewed
}

class WatcherEvent {
  event_id string
  timestamp string
  event_type string
  level string  // info, warn, error, critical
  payload string
  source string  // which component emitted
}

class ReviewScope {
  start_time string
  end_time string
  event_types string[]
  min_level string
}

// ============================================================================
// Output Types - Review
// ============================================================================

enum ReviewStatus {
  Clean
  IssuesDetected
  Critical
  Inconclusive
}

enum EscalationKind {
  FailureSpike
  TimeoutPattern
  ResourceExhaustion
  SecurityConcern
  CostAnomaly
  StalledWorkflow
  Other
}

enum UrgencyLevel {
  Low
  Medium
  High
  Critical
}

enum RiskCategory {
  Operational
  Technical
  Security
  Cost
  Compliance
}

class WatcherReviewOutput {
  review_status ReviewStatus
  escalations WatcherEscalation[]
  risks RiskItem[]
  anomalies DetectedAnomaly[]
  confidence float
  rationale string
}

class WatcherEscalation {
  escalation_id string
  run_id string
  task_id string
  kind EscalationKind
  urgency UrgencyLevel
  affected_calls string[]
  description string
  recommended_action string
  recommended_capability string?
  recommended_objective string?
}

class RiskItem {
  risk_id string
  category RiskCategory
  likelihood float  // 0.0-1.0
  impact float  // 0.0-1.0
  description string
  mitigating_factors string[]
}

class DetectedAnomaly {
  anomaly_type string
  severity string
  description string
  affected_events string[]
  pattern_detected string
}

// ============================================================================
// Input Types - Mitigation
// ============================================================================

class WatcherMitigationInput {
  escalation WatcherEscalation
  run_state RunStateSnapshot
  available_capabilities string[]
  historical_resolutions HistoricalResolution[]
}

class RunStateSnapshot {
  run_id string
  status string
  active_call_count int
  recent_failures int
  elapsed_time_ms int
}

class HistoricalResolution {
  pattern string
  successful_resolution string
  resolution_type string
}

// ============================================================================
// Output Types - Mitigation
// ============================================================================

enum EscalationAction {
  NotifyConductor
  RequestHumanReview
  AutoRetry
  ScaleResources
  TerminateRun
  ContinueMonitoring
  EscalateToOnCall
}

class WatcherMitigationOutput {
  escalation_action EscalationAction
  urgency UrgencyLevel
  recommended_capability string?
  recommended_objective string?
  rationale string
  confidence float
  expected_outcome string
  alternatives AlternativeAction[]
}

class AlternativeAction {
  action EscalationAction
  pros string[]
  cons string[]
  estimated_success_rate float
}

// ============================================================================
// Function 1: WatcherReviewLogWindow
// ============================================================================

function WatcherReviewLogWindow(input: WatcherLogWindowInput) -> WatcherReviewOutput {
  client FastResponse
  prompt #"
    You are the ChoirOS Watcher, an AI that reviews event logs for anomalies,
    failures, and concerning patterns.

    Your role is deterministic detection over logs - you identify issues that
    may require escalation to the Conductor or human operators.

    Window ID: {{ input.window_id }}
    Run ID: {{ input.run_id }}
    Task ID: {{ input.task_id }}

    Review Reason: {{ input.review_reason }}

    Review Scope:
    - Start Time: {{ input.scope.start_time }}
    - End Time: {{ input.scope.end_time }}
    - Event Types: {{ input.scope.event_types }}
    - Minimum Level: {{ input.scope.min_level }}

    Events to Review ({{ input.events | length }} events):
    {{ input.events }}

    Review Guidelines:

    1. **ReviewStatus Classification**:
       - Clean: No issues detected, normal operation
       - IssuesDetected: Minor issues found, may need attention
       - Critical: Serious problems requiring immediate escalation
       - Inconclusive: Cannot determine from available data

    2. **Escalation Detection** - Create escalations for:
       - FailureSpike: Multiple failures in short window
       - TimeoutPattern: Repeated timeout errors
       - ResourceExhaustion: High resource usage or limits hit
       - SecurityConcern: Suspicious activity or auth failures
       - CostAnomaly: Unexpected cost patterns
       - StalledWorkflow: Tasks not progressing
       - Other: Unclassified but concerning patterns

    3. **Risk Assessment**:
       - Evaluate likelihood (0.0-1.0) and impact (0.0-1.0)
       - Categories: Operational, Technical, Security, Cost, Compliance
       - List mitigating factors that reduce risk

    4. **Anomaly Detection**:
       - Identify unusual patterns in event sequences
       - Note severity and affected events
       - Describe the pattern detected (e.g., "retry storm", "cascade failure")

    5. **Escalation Recommendations**:
       - recommended_action: Clear, actionable next step
       - recommended_capability: Which capability should handle this (if known)
       - recommended_objective: What objective to assign (if applicable)
       - urgency: Low, Medium, High, or Critical

    Output Requirements:
    - Use exact enum values for ReviewStatus, EscalationKind, UrgencyLevel, RiskCategory
    - Confidence score 0.0-1.0 based on clarity of findings
    - Provide clear rationale for all escalations and risks
    - affected_calls should reference specific call IDs from events

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// Function 2: WatcherRecommendMitigation
// ============================================================================

function WatcherRecommendMitigation(input: WatcherMitigationInput) -> WatcherMitigationOutput {
  client FastResponse
  prompt #"
    You are the ChoirOS Watcher, recommending mitigation actions for detected issues.

    Your role is to select the best action given an escalation and current run state,
    including when to auto-retry vs request human review.

    Escalation Details:
    - ID: {{ input.escalation.escalation_id }}
    - Kind: {{ input.escalation.kind }}
    - Urgency: {{ input.escalation.urgency }}
    - Description: {{ input.escalation.description }}
    - Affected Calls: {{ input.escalation.affected_calls }}
    - Recommended Action: {{ input.escalation.recommended_action }}
    - Recommended Capability: {{ input.escalation.recommended_capability }}
    - Recommended Objective: {{ input.escalation.recommended_objective }}

    Current Run State:
    - Run ID: {{ input.run_state.run_id }}
    - Status: {{ input.run_state.status }}
    - Active Calls: {{ input.run_state.active_call_count }}
    - Recent Failures: {{ input.run_state.recent_failures }}
    - Elapsed Time (ms): {{ input.run_state.elapsed_time_ms }}

    Available Capabilities:
    {{ input.available_capabilities }}

    Historical Resolutions (similar patterns and their outcomes):
    {{ input.historical_resolutions }}

    Mitigation Guidelines:

    1. **EscalationAction Selection**:
       - NotifyConductor: Wake the Conductor for orchestration decision
       - RequestHumanReview: Human judgment needed (ambiguous, high-stakes)
       - AutoRetry: Safe to retry automatically (transient failures)
       - ScaleResources: Increase resources (resource exhaustion)
       - TerminateRun: Stop the run (unrecoverable, dangerous)
       - ContinueMonitoring: Watch but don't act yet (inconclusive)
       - EscalateToOnCall: Page the on-call engineer (critical production issue)

    2. **Decision Matrix**:
       - Transient failures (network, timeout) + low retry count -> AutoRetry
       - Resource exhaustion + scaling available -> ScaleResources
       - Ambiguous situation + high stakes -> RequestHumanReview
       - Conductor available + complex coordination needed -> NotifyConductor
       - Critical urgency + production impact -> EscalateToOnCall
       - Unclear pattern + no historical success -> ContinueMonitoring

    3. **Alternative Actions**:
       - List 2-3 alternative actions with pros/cons
       - Estimate success rate for each alternative
       - Explain why the primary action was chosen

    4. **Capability/Objectives**:
       - recommended_capability: Which capability to invoke (if action requires it)
       - recommended_objective: Clear objective statement for the capability
       - These flow into the Conductor wake lane for orchestration

    Output Requirements:
    - Use exact enum values for EscalationAction and UrgencyLevel
    - Confidence score 0.0-1.0 based on certainty of recommendation
    - Provide clear rationale linking escalation to chosen action
    - expected_outcome: What should happen if action is taken
    - Include at least 2 alternatives with honest pros/cons

    {{ ctx.output_format }}
  "#
}
