# Capability Escalation Policy

## Narrative Summary (1-minute read)

ChoirOS now treats completion as a control-plane policy, not a model guess.  
Capability actors emit explicit objective state (`complete`, `incomplete`, `blocked`) and recommended next capability. Supervisors enforce escalation rules deterministically. This keeps prompts generic while preserving autonomy and non-blocking UX.

## What Changed

- Added explicit research objective status metadata in runtime results:
  - `objective_status`
  - `completion_reason`
  - `recommended_next_capability`
  - `recommended_next_objective`
- Added supervisor-side escalation hook:
  - when research returns `incomplete|blocked` with `recommended_next_capability=terminal`,
  - supervisor can escalate to `TerminalActor` (`RunAgenticTask`) and fold result back into one worker completion event.
- Added policy env controls:
  - `CHOIR_RESEARCH_ENABLE_TERMINAL_ESCALATION` (default `1`)
  - `CHOIR_RESEARCH_TERMINAL_ESCALATION_TIMEOUT_MS` (clamped)
  - `CHOIR_RESEARCH_TERMINAL_ESCALATION_MAX_STEPS` (clamped)

## What To Do Next

1. Apply the same objective-status contract to `ChatActor` and `TerminalActor` loops for full harness parity.
2. Add conductor-level policy for escalation caps and permission-tier gates.
3. Add integration tests for multi-step escalation (`research -> terminal -> final answer`) across providers.

---

## Policy Model

### 1) Objective State Is First-Class

Every capability completion must emit one terminal state:

- `complete`: objective satisfied with current evidence.
- `incomplete`: progress made, but objective not yet satisfied.
- `blocked`: objective cannot continue without external intervention.

### 2) Escalation Is Runtime-Controlled

Escalation decisions are not delegated entirely to prompt text.

- actor emits recommendation
- supervisor evaluates policy
- supervisor executes allowed next capability

This prevents early “good enough” stopping and reduces model-specific variance.

### 3) Capability Gradient

- `L0`: constrained retrieval/read
- `L1`: broader retrieval and synthesis
- `L2`: terminal delegation (read/verify)
- `L3`: terminal mutation/scoped write
- `L4`: privileged actions (policy/conductor approval)

Current implementation covers `research -> terminal` for `L1 -> L2` escalation.

## Current Runtime Contract (Research)

Researcher returns:

- `summary`
- `success`
- `objective_status`
- `completion_reason`
- `recommended_next_capability`
- `recommended_next_objective`

Supervisor behavior:

1. ingest report/signals
2. if `success=false`, fail task
3. if `success=true` and policy allows escalation and status is `incomplete|blocked` with recommendation `terminal`, run terminal escalation
4. publish single task completion with final output and objective metadata

## Design Intent

- App actors can remain action-scoped.
- Completion semantics live in control-plane policy.
- Uactors/conductor can reason over explicit states, not implicit model prose.
- Logging stays auditable: each escalation is evented and attributable.
