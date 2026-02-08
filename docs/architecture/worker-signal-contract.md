# Worker Signal Contract (Control vs Observability)

Date: 2026-02-08  
Status: Proposed (docs gate before implementation)

## Narrative Summary (1-minute read)

Workers should not improvise signaling through ad-hoc files or freeform logs.  
Each worker turn returns a typed report envelope. Runtime validation then emits canonical events and optional conductor notifications.

This separates two planes:
- Control plane: explicit requests for decisions/actions (`blocker`, `help`, `approval`, `conflict`).
- Observability plane: findings/learnings/progress for replay, UI, and analysis.

Result:
- clear routing to Conductor when needed
- replayable event trail in EventStore
- reduced spam from model tendency to fill optional fields

## What Changed

- Locked contract: workers emit typed per-turn reports, not path-based signaling.
- Locked rule: findings/learnings are observability events by default.
- Locked rule: escalations are control-plane requests and may notify Conductor directly.
- Added anti-spam policy at prompt, schema, and runtime layers.
- Added canonical event mapping for worker report ingestion.

## What To Do Next

1. Implement typed report schema in BAML and Rust validation.
2. Add runtime guardrails (caps, dedup hash, cooldown, confidence gates).
3. Emit canonical events from accepted reports.
4. Wire conductor notification for accepted escalations only.
5. Add tests for spam resistance and escalation correctness.

## Decision

Primary worker output contract is a typed turn report envelope.

Workers do not decide transport mechanics. They only produce structured content.
Runtime owns:
- validation
- dedup/cooldown
- event emission
- control-plane notification

## Planes

### Control Plane

Purpose:
- request decisions or intervention from Conductor

Signals:
- `blocker`: cannot continue without missing dependency/input
- `help`: worker can continue but would benefit from guidance
- `approval`: risky action requires explicit authorization
- `conflict`: contradictory evidence/options need arbitration

Output:
- direct conductor message (internal call)
- persisted event for audit/replay

### Observability Plane

Purpose:
- durable facts, synthesis, progress, and artifacts

Signals:
- `finding`: grounded fact with evidence
- `learning`: synthesis that changes strategy/understanding
- `progress`: step/lifecycle status
- `artifact`: references to generated outputs

Output:
- EventStore events
- UI streams/logs/watcher inputs

## Typed Turn Report (Conceptual Schema)

```toml
# conceptual envelope shape (not final syntax)
turn_id = "..."
worker_id = "..."
task_id = "..."
status = "running|completed|failed|blocked"
summary = "short prose summary"

[[findings]]
finding_id = "..."
claim = "..."
confidence = 0.0
evidence_refs = ["url_or_event_or_artifact_ref"]
novel = true

[[learnings]]
learning_id = "..."
insight = "..."
confidence = 0.0
supports = ["finding_id"]
changes_plan = true

[[escalations]]
escalation_id = "..."
kind = "blocker|help|approval|conflict"
reason = "..."
urgency = "low|medium|high"
options = ["..."]
recommended_option = "..."
requires_human = false

[[artifacts]]
artifact_id = "..."
kind = "report|note|dataset|file"
ref = "path_or_uri_or_hash"
```

## Anti-Spam Policy

### 1) Prompt Layer (model behavior)

- Default to empty arrays.
- Emit finding only when evidence exists and confidence threshold is met.
- Emit learning only when it changes plan/understanding.
- Emit escalation only when blocked, risk-gated, or decision-required.

### 2) Schema Layer (shape constraints)

- Per-turn caps:
  - `findings <= 2`
  - `learnings <= 1`
  - `escalations <= 1`
- Required fields for each signal kind.
- Confidence required for finding/learning.

### 3) Runtime Layer (governance)

- Dedup by normalized claim hash in rolling window.
- Cooldown for repeated escalation keys.
- Reject low-quality signals and emit rejection telemetry.
- Keep accepted vs rejected counters per worker/model.

## Event Mapping (Canonical)

On accepted typed report:
- `worker.report.received`
- `worker.task.progress|completed|failed` (existing lifecycle)
- `research.finding.created` (for research workers)
- `research.learning.created` (for research workers)
- `worker.signal.escalation_requested` (control-plane request persisted)
- `artifact.created` (if artifacts are declared)

On rejected signals:
- `worker.signal.rejected` with reason (`duplicate`, `low_confidence`, `missing_evidence`, `cooldown`)

Conductor notification:
- only from accepted escalation payloads
- never from deterministic watcher replay alone

## Why Not File-Path Signaling

File-path filtering is brittle:
- path conventions drift
- writes can be delayed/retried/partial
- semantics are implicit, not typed

Files should be artifacts, not control channels.
If a file matters, emit `artifact.created` with a stable reference.

## Watcher Role After This Decision

Watcher remains valuable, but secondary for control:
- scans event log for patterns/anomalies/trends
- emits advisory alerts/findings/learnings
- does not replace direct worker escalation path

This avoids latency and ambiguity for urgent blockers.

## Acceptance Criteria

1. Every worker turn can be validated against one typed report schema.
2. Escalations notify Conductor only after runtime acceptance.
3. Findings/learnings are replayable from EventStore without reading files.
4. Duplicate/low-signal spam is rate-limited and observable in rejection events.
5. Logs UI can show raw event plus accepted summary without semantic mismatch.
