# Agentic Loop Simplification + Observability Research Program: Code Review Report v4

**Date:** 2026-02-10
**Reviewer:** opencode
**Scope:** Runtime architecture simplification, determinism removal, and observability hardening
**Based on:** `docs/architecture/2026-02-11-agentic-loop-simplification-observability-research-program.md`

---

## Executive Summary

This report presents findings from a comprehensive code review of the ChoirOS runtime architecture against the [Agentic Loop Simplification + Observability Research Program](docs/architecture/2026-02-11-agentic-loop-simplification-observability-research-program.md) (2026-02-11).

**Overall Assessment:** The system implements the target runtime architecture (Conductor → Workers → Watcher → EventStore) but contains **critical violations** of the program's non-negotiable architecture rules. Specifically:

1. **Deterministic authority persists** in multiple control paths (TerminalActor, Conductor bootstrap)
2. **Silent fallbacks** exist when LLM policy calls fail (TerminalActor)
3. **Missing headless E2E tests** for required observability scenarios
4. **Fragmented loop implementation** across Chat/Terminal/Researcher actors (no shared harness)

**Risk Level:** HIGH - Current state allows hidden deterministic routing and failure modes that violate the program's core principles.

---

## 1. Violations of Non-Negotiable Architecture Rules

### Rule 1: No Deterministic Workflow Authority ❌

#### Violation 1.1: TerminalActor Command Bypass

**File:** `sandbox/src/actors/terminal.rs:563-610`

**Issue:** The `run_agentic_task` function uses deterministic phrase matching to decide whether to skip the agentic planner entirely:

```rust
if Self::looks_like_shell_command(&objective) {
    // Direct execution bypasses agentic loop
    Self::emit_progress(
        &progress_tx,
        "terminal_tool_call",
        "executing direct bash command",
        Some("Direct command execution (explicit bash command).".to_string()),
        Some(objective.clone()),
        None,
        None,
        None,
        Some(1),
        Some(1),
    );
    let (output, exit_code) = self
        .execute_terminal_command(&ctx, &objective, per_step_timeout)
        .await?;
    // ... returns result without going through planner
}
```

The `looks_like_shell_command` function at line 10004 uses pattern matching:

```rust
fn looks_like_shell_command(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.contains('\n')
        || trimmed.starts_with("./")
        || trimmed.starts_with('/')
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('|')
        || trimmed.contains('>')
        || trimmed.contains('<')
        || trimmed.starts_with("ls ")
        // ... more patterns
}
```

**Why this is a violation:**
- Decision to bypass LLM planner is made by string patterns, not typed BAML outputs
- Creates "shortcut" paths that are invisible to policy enforcement
- Makes behavior appear deterministic in testing (same input → same bypass decision)

**Impact:** Terminal objectives that match patterns bypass the agentic loop entirely, violating the program's core principle that all worker loops must be policy-driven.

---

#### Violation 1.2: Conductor Agenda Construction via Threshold Logic

**File:** `sandbox/src/actors/conductor/runtime/bootstrap.rs:149-189`

**Issue:** The `build_initial_agenda` function uses deterministic confidence thresholding to decide which capabilities to dispatch:

```rust
candidates.sort_by(|a, b| {
    b.1.confidence
        .partial_cmp(&a.1.confidence)
        .unwrap_or(std::cmp::Ordering::Equal)
});

let mut selected = vec![candidates[0].clone()];
if candidates.len() > 1 {
    let top = &candidates[0].1;
    let second = &candidates[1].1;
    if second.confidence >= 0.75 && (top.confidence - second.confidence).abs() <= 0.20 {
        selected.push(candidates[1].clone());
    }
}
```

**Why this is a violation:**
- Agenda construction decisions are made by hardcoded numeric thresholds (0.75, 0.20)
- Not a typed BAML policy decision
- Creates deterministic behavior based on LLM confidence scores
- Thresholds are not configurable or observable

**Impact:** The Conductor's initial capability selection is not fully under policy authority, violating the principle that "Typed outputs are control authority."

**Correct approach:** Create a BAML function `ConductorBootstrapped` that returns typed agenda decisions.

---

### Rule 4: No Silent Fallback Paths ❌

#### Violation 4.1: TerminalActor Silent Fallback on PlanAction Failure

**File:** `sandbox/src/actors/terminal.rs:666-707`

**Issue:** When the planner (`B.PlanAction`) fails, there's a fallback that executes the objective directly without emitting an explicit failure:

```rust
let plan = match B
    .PlanAction
    .with_client_registry(&client_registry)
    .call(&messages, &system_context, tools_description)
    .await
{
    Ok(plan) => plan,
    Err(_) => {
        // SILENT FALLBACK - executes directly without explicit failure state
        let (output, exit_code) = self
            .execute_terminal_command(&ctx, &objective, per_step_timeout)
            .await?;
        let summary = if exit_code == 0 {
            output.clone()
        } else {
            format!("Command failed with exit status {exit_code}: {output}")
        };
        // ... returns result as if everything was normal
        return Ok(TerminalAgentResult {
            summary,
            reasoning: Some(
                "Planner unavailable; executed objective as direct command."
                    .to_string(),
            ),
            success: exit_code == 0,
            // ...
        });
    }
};
```

**Why this is a violation:**
- When LLM policy fails, execution continues silently
- No explicit `blocked` or `failed` typed state is emitted
- The system pretends everything worked normally
- Failures are hidden from observability traces

**Correct approach:**
```rust
Err(e) => {
    emit_blocked(&progress_tx, &format!("Planning failed: {}", e));
    emit_failed(&state.event_store, &run_id, "Planner unavailable");
    return Err(TerminalError::Blocked(format!("Planning failed: {}", e)));
}
```

**Impact:** Users and observers cannot distinguish between successful policy-driven execution and silent fallbacks. This violates the program's requirement that "Failures must be explicit typed states (blocked or failed) with reason."

---

## 2. Missing Capabilities from Research Workstream B

### 2.1 No Headless E2E Tests for Required Scenarios ❌

**Required scenarios from Workstream B** (not yet implemented):

1. **Basic run:** objective → completion with run events visible
2. **Replan run:** worker returns incomplete → Conductor dispatches follow-up worker
3. **Watcher wake run:** escalation causes Conductor policy wake/replan
4. **Blocked run:** policy failure produces explicit blocked state (no hidden fallback)
5. **Concurrency run:** multiple worker calls active simultaneously with stable run status
6. **Observability run:** semantic findings/learnings appear in logs and watcher review windows
7. **Live-stream run:** researcher `finding`/`learning` and terminal action/reasoning summaries are observed before `run.completed`

**Current test coverage:**
- ✅ Unit tests for individual actors (`ChatActor`, `TerminalActor`, `Conductor`)
- ✅ Conductor runtime loop tests (`tests/runtime_loop.rs`)
- ❌ **Missing:** WebSocket stream assertions for run timeline ordering
- ❌ **Missing:** Headless API tests for Prompt Bar run flow
- ❌ **Missing:** Live-stream event arrival verification (before completion)
- ❌ **Missing:** Watcher wake/replan flow tests

**Impact:** Without these tests, regressions will only be discovered manually, violating the research program's verification gap closure goal.

---

### 2.2 Missing Run-Level Observability API ❌

**Required from Workstream C - Minimum Run Visibility Contract:**

1. Run identity: run_id, task_id, correlation_id
2. Current status: queued/running/waiting/completed/blocked/failed
3. Active workers and in-flight calls
4. Semantic timeline: findings, learnings, escalations, key decisions
5. Test outcomes and failure reasons per run

**Current state:**
- ✅ EventStore stores all events
- ✅ Conductor tracks run state internally
- ❌ **Missing:** API endpoint to query complete run timeline
- ❌ **Missing:** Semantic event filtering (findings, learnings, decisions)
- ❌ **Missing:** Ability to detect missing required milestones via API

**Impact:** Completed runs do not reliably produce coherent run-level traces that can be queried programmatically.

---

## 3. Architecture Fragmentation: No Unified Agentic Harness

### Current State ❌

Each capability actor implements its own loop logic independently:

| Actor | Loop Implementation | Planning Mechanism | Tool Dispatch | Event Emission |
|--------|-------------------|-------------------|--------------|----------------|
| **TerminalActor** | Custom `run_agentic_task` loop | `B.PlanAction` (with bypass) | Direct bash execution | Ad-hoc progress events |
| **ResearcherActor** | Custom `handle_web_search` loop | `policy::plan_step` | Provider selection | Module-specific events |
| **ChatActor** | Event projection only | N/A (thin actor) | N/A | Message events |
| **ConductorActor** | Runtime module with decision loop | `state.policy.decide_next_action` | Worker delegation | Conductor events |

**Problems:**
1. No shared state machine
2. Inconsistent event semantics
3. Duplicate timeout/step cap logic
4. Harder to enforce policy rules uniformly

### Target State ✅ (from unified-agentic-loop-harness.md)

All capability actors use a **shared harness** with actor-specific adapters:

**Harness responsibilities:**
- Prompt assembly (timestamped)
- Plan step execution (`PlanAction`)
- Tool/delegation dispatch
- Observation injection for replanning
- Step caps and timeout budget
- Deferred/async yield behavior
- Typed turn report emission (`finding`, `learning`, `escalation`, `artifact`)
- Final synthesis (`SynthesizeResponse`) only when stable answer is ready

**Actor adapter responsibilities:**
- Allowed tool surface
- Model policy role (`chat`, `terminal`, `researcher`)
- Domain-specific result normalization
- Domain-specific validation gates

**Impact:** Without the unified harness, maintaining consistent policy enforcement and observability across actors is error-prone and leads to duplicate code.

---

## 4. Borderline Concerns

### 4.1 Watcher Self-Trigger Risk ⚠️

**File:** `sandbox/src/actors/watcher.rs:268-279`

The watcher attempts to ignore its own events:

```rust
fn should_ignore_event(&self, state: &WatcherState, event: &shared_types::Event) -> bool {
    if event.event_type.starts_with("watcher.") {
        return true;
    }

    let actor_id = event.actor_id.0.as_str();
    if actor_id == state.watcher_id || actor_id.starts_with("watcher:") {
        return true;
    }

    !self.should_review_event(event)
}
```

**Concern:** Filtering is based on string prefix matching. If the watcher emits events with types or actor_ids that don't match these patterns, it could trigger its own review loop.

**Recommendation:** Add an explicit `originating_actor` metadata field to events and filter based on that, not string patterns.

---

## 5. Refactoring Plan

### Phase 1: Remove Deterministic Authority (HIGH PRIORITY)

#### Task 1.1: Eliminate TerminalActor Command Bypass
**File:** `sandbox/src/actors/terminal.rs`

**Changes:**
1. Delete `looks_like_shell_command` function (lines 10004-10026)
2. Remove direct execution fallback before planner loop (lines 563-610)
3. Always route objectives through `B.PlanAction` → execute bash tool → loop
4. Ensure all paths go through the agentic state machine

**Acceptance:**
- [ ] No string pattern matching in control flow
- [ ] All terminal objectives go through planner first
- [ ] Test confirms bypass removal (previously bypassed inputs now go through planner)

---

#### Task 1.2: Replace Conductor Agenda Threshold Logic with BAML Policy
**Files:** `sandbox/src/actors/conductor/runtime/bootstrap.rs`, `sandbox/baml_client/` (add new function)

**Changes:**
1. Create BAML function `ConductorBootstrapped`:
   ```baml
   function ConductorBootstrapped {
     input {
       objective string
       available_capabilities string[]
     }
     output {
       agenda AgendaItem[]
       reason string
       confidence float
     }
   }
   ```
2. Replace `build_initial_agenda` threshold logic (lines 149-189) with BAML call
3. Ensure BAML function can decide to dispatch multiple, single, or no capabilities

**Acceptance:**
- [ ] No hardcoded numeric thresholds in agenda construction
- [ ] Agenda decisions are fully under BAML policy authority
- [ ] Test confirms varied dispatch behavior based on BAML output

---

### Phase 2: Add Explicit Failure States (HIGH PRIORITY)

#### Task 2.1: TerminalActor Explicit Blocking on PlanAction Failure
**File:** `sandbox/src/actors/terminal.rs:666-707`

**Changes:**
```rust
Err(e) => {
    // BEFORE (silent fallback):
    // let (output, exit_code) = self.execute_terminal_command(...).await?;

    // AFTER (explicit blocked state):
    emit_blocked(&progress_tx, &format!("Planning failed: {}", e));
    emit_failed(&state.event_store, &run_id, "Planner unavailable");
    return Err(TerminalError::Blocked(format!("Planning failed: {}", e)));
}
```

**Acceptance:**
- [ ] No silent fallback when LLM calls fail
- [ ] Explicit `blocked` event emitted to EventStore
- [ ] Test confirms blocked state is visible in run timeline

---

### Phase 3: Implement Headless E2E Tests (HIGH PRIORITY)

#### Task 3.1: Create E2E Test Suite for Required Scenarios
**New file:** `sandbox/tests/e2e_conductor_scenarios.rs`

**Test structure:**
```rust
#[tokio::test]
async fn test_basic_run_flow() {
    // 1. Start test server (or use existing at http://localhost:8080)
    // 2. Connect WebSocket for run logs
    // 3. Submit objective via API: POST /api/conductor/execute
    // 4. Subscribe to run/log websocket streams
    // 5. Assert streaming updates arrive while status == "running"
    // 6. Assert final run state == "completed"
    // 7. Assert required events present: run.started, worker.call, run.completed
}

#[tokio::test]
async fn test_live_stream_run_flow() {
    // 1. Submit objective that triggers researcher
    // 2. Subscribe to event stream
    // 3. Assert finding/learning events arrive BEFORE run.completed
    // 4. Assert timeline is ordered correctly
}
```

**Required scenarios:**
1. `test_basic_run_flow` - objective → completion with run events visible
2. `test_replan_flow` - worker returns incomplete → follow-up dispatched
3. `test_watcher_wake_flow` - escalation triggers replan
4. `test_blocked_run_flow` - explicit blocked state (no fallback)
5. `test_concurrency_run_flow` - multiple parallel workers
6. `test_observability_run_flow` - findings/learnings visible in logs
7. `test_live_stream_run_flow` - events observed pre-completion

**Acceptance:**
- [ ] All 7 scenarios pass headlessly (no browser required)
- [ ] Tests use WebSocket streams, not just final state queries
- [ ] Missing milestone events cause test failures immediately
- [ ] Tests run in CI (no manual steps required)

---

#### Task 3.2: Implement Run-Level Observability API
**New file:** `sandbox/src/api/run_observability.rs`

**New endpoint:** `GET /api/runs/{run_id}/timeline`

**Response schema:**
```rust
#[derive(Debug, Serialize)]
pub struct RunTimelineResponse {
    pub run_id: String,
    pub objective: String,
    pub status: String,
    pub current_step: Option<String>,
    pub timeline: Vec<TimelineEvent>,
    pub artifacts: Vec<Artifact>,
    pub active_workers: Vec<ActiveWorker>,
    pub total_duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TimelineEvent {
    pub seq: i64,
    pub timestamp: chrono::DateTime<Utc>,
    pub event_type: String,
    pub event_category: EventCategory,
    pub summary: String,
    pub run_id: String,
    pub task_id: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    RunLifecycle,
    WorkerCall,
    WorkerOutput,
    WorkerProgress,
    Decision,
    Escalation,
    Finding,
    Learning,
    Blocked,
    Failed,
}
```

**Query logic:**
1. Fetch all events for `run_id` from EventStore
2. Build timeline ordered by `seq`
3. Categorize events into semantic buckets
4. Filter by category if requested via query params
5. Ensure required milestones are present (assertable)

**Acceptance:**
- [ ] API returns complete, ordered event history
- [ ] Events are semantically categorized (findings, learnings, decisions)
- [ ] Missing milestones are detectable (400 error if required event missing)
- [ ] Response includes active workers count and status

---

### Phase 4: Unified Agentic Harness (MEDIUM PRIORITY)

#### Task 4.1: Create Shared Harness Module
**New file:** `sandbox/src/actors/agentic_harness/mod.rs`

**Harness structure:**
```rust
use crate::baml_client::{ClientRegistry, B};
use tokio::sync::mpsc;

pub struct AgenticHarness<C: CapabilityAdapter> {
    capability: C,
    model_registry: ModelRegistry,
    timeout_budget: Duration,
    max_steps: usize,
}

pub trait CapabilityAdapter: Send + Sync {
    fn get_model_role(&self) -> &str;
    fn get_tool_description(&self) -> &str;
    fn execute_tool_call(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> Result<ToolResult, ToolError>;
    fn should_defer(&self, tool_name: &str) -> bool;
}

#[derive(Debug)]
pub enum HarnessState {
    ReceivingMessage,
    PlanningStep,
    ExecutingTools,
    ObservingResults,
    SynthesizingFinal,
    Completed,
}

pub struct LoopResult {
    pub final_response: String,
    pub findings: Vec<shared_types::WorkerFinding>,
    pub learnings: Vec<shared_types::WorkerLearning>,
    pub artifacts: Vec<shared_types::ConductorArtifact>,
    pub steps_taken: usize,
    pub status: HarnessStatus,
}

#[derive(Debug, PartialEq)]
pub enum HarnessStatus {
    Completed,
    Blocked(String),  // reason
    Failed(String),   // reason
}
```

**Shared state machine:**
```text
RECEIVE_MESSAGE
  -> PLAN_STEP
    -> (no tools) SYNTHESIZE_FINAL
    -> (tools) EXECUTE_TOOL_CALLS
      -> TOOL_RESULT_OBSERVE
        -> (budget remaining) PLAN_STEP
        -> (needs background work) DEFER_ACK + YIELD

ON_COMPLETION_SIGNAL
  -> RESUME_WITH_COMPLETION_OBSERVATION
    -> PLAN_STEP
      -> SYNTHESIZE_FINAL
```

**Actor adapter implementation for Terminal:**
```rust
pub struct TerminalCapabilityAdapter {
    terminal_id: String,
    working_dir: String,
    shell: String,
}

impl CapabilityAdapter for TerminalCapabilityAdapter {
    fn get_model_role(&self) -> &str { "terminal" }

    fn get_tool_description(&self) -> &str {
        r#"Tool: bash
         Description: Execute shell commands in current terminal."#
    }

    fn execute_tool_call(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        // Execute command via execute_terminal_command
        // Return structured output
    }

    fn should_defer(&self, tool_name: &str) -> bool {
        // Terminal tools never defer (synchronous)
        false
    }
}
```

**Acceptance:**
- [ ] TerminalActor uses harness for loop orchestration
- [ ] ResearcherActor uses harness for loop orchestration
- [ ] ChatActor uses harness (when agentic features added)
- [ ] All actors emit consistent `finding`/`learning` events
- [ ] Step caps and timeout logic is shared (no duplicates)

---

### Phase 5: Watcher Hardening (LOW PRIORITY)

#### Task 5.1: Add Explicit Origin Metadata
**Files:** `shared-types/src/lib.rs`, `sandbox/src/actors/watcher.rs`

**Changes:**
1. Add `originating_actor` field to `EventMetadata`
2. Update watcher filtering to use this field instead of string patterns
3. Update all event emission sites to include originating actor

**Acceptance:**
- [ ] Watcher self-trigger is impossible by construction
- [ ] Filtering is based on typed metadata, not string matching

---

## 6. Implementation Order

| Phase | Task | Priority | Estimated Effort | Dependencies |
|-------|------|----------|-----------------|--------------|
| 1.1 | Remove TerminalActor bypass | HIGH | 2 days | None |
| 1.2 | Conductor agenda BAML policy | HIGH | 2 days | BAML function creation |
| 2.1 | TerminalActor explicit failures | HIGH | 1 day | None |
| 3.1 | Headless E2E tests | HIGH | 5 days | Phase 1-2 complete |
| 3.2 | Observability API | MEDIUM | 3 days | None |
| 4.1 | Unified agentic harness | MEDIUM | 4 days | Phases 1-2 complete |
| 5.1 | Watcher hardening | LOW | 1 day | Phase 3.2 complete |

**Total estimated effort:** 18 days (3.5 weeks)

---

## 7. Acceptance Criteria Checklist

Before un-freezing new orchestration feature work, all of the following must be verified:

### Rule Compliance
- [ ] No `looks_like_shell_command()` or equivalent deterministic phrase matching in control paths
- [ ] No hardcoded numeric thresholds in agenda construction or decision logic
- [ ] No silent fallback when LLM calls fail - explicit blocked/failed states only
- [ ] Conductor agenda construction is fully BAML-based
- [ ] All typed outputs serve as control authority (no phrase-matching control)

### Verification
- [ ] All 7 headless E2E scenarios pass (no browser required)
- [ ] Tests assert that streaming updates arrive while run status is `running`
- [ ] Missing milestone events cause immediate test failures
- [ ] Tests run in CI without manual intervention

### Observability
- [ ] Run timeline API returns complete, ordered event history
- [ ] Timeline includes semantic categorization (findings, learnings, decisions)
- [ ] Required milestone events are enforced (API error if missing)
- [ ] "What is happening now?" can be answered from live run state without guessing
- [ ] Completed runs have coherent, queryable timelines from start to finish

### Architecture
- [ ] Authority path from prompt to completion is < 15 steps, fully typed
- [ ] All capability actors (Terminal, Researcher, Chat) share one harness module
- [ ] Watcher filtering uses typed metadata, not string patterns
- [ ] No duplicate timeout/step cap logic across actors

---

## 8. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Deterministic paths cause hidden regressions** | HIGH | Remove bypasses immediately; add tests that would catch bypass re-addition |
| **Silent failures mask policy bugs** | HIGH | Make all failures explicit; test blocked state emission |
| **No E2E tests allow manual-only regression detection** | HIGH | Implement headless tests first; integrate into CI |
| **Watcher self-trigger causes event loops** | MEDIUM | Add typed origin metadata; test with watcher stress scenarios |
| **Unified harness refactor introduces new bugs** | MEDIUM | Implement gradually (one actor at a time); keep old code in branch until verified |
| **BAML policy changes affect behavior unpredictably** | MEDIUM | Version BAML functions; add test fixtures for expected policy outputs |

---

## 9. Recommendation

**DO NOT un-freeze new orchestration feature work** until the following are complete:

1. **Phase 1 and 2 are 100% complete** (determinism removal + explicit failures)
2. **Phase 3.1 is 100% complete** (all 7 headless E2E tests passing)
3. **At least one full cycle of regression testing** has been run on the simplified codebase

**Rationale:**
- The current violations (deterministic authority, silent fallbacks) fundamentally undermine the research program's goals
- Adding new features on top of this foundation will compound technical debt
- Headless tests are essential for preventing future regressions
- The unified harness (Phase 4) can proceed after the foundation is stable

---

## 10. Appendix: Code Reference Summary

### Files Requiring Changes

| File | Issue | Lines | Change Type |
|------|--------|-------|------------|
| `sandbox/src/actors/terminal.rs` | Deterministic command bypass | 563-610, 10004-10026 | Delete bypass logic |
| `sandbox/src/actors/terminal.rs` | Silent fallback on PlanAction failure | 666-707 | Add explicit blocked state |
| `sandbox/src/actors/conductor/runtime/bootstrap.rs` | Threshold-based agenda logic | 149-189 | Replace with BAML call |
| `sandbox/src/actors/watcher.rs` | String-based self-trigger filtering | 268-279 | Add typed origin metadata |
| **NEW** | `sandbox/tests/e2e_conductor_scenarios.rs` | N/A | Create headless E2E tests |
| **NEW** | `sandbox/src/api/run_observability.rs` | N/A | Create run timeline API |
| **NEW** | `sandbox/src/actors/agentic_harness/mod.rs` | N/A | Create unified harness |

### BAML Functions to Create/Add

| Function | Purpose | Current State |
|----------|---------|---------------|
| `ConductorBootstrapped` | Typed agenda construction | Does not exist (currently done via Rust logic) |
| `ConductorDecideNextAction` | Already exists ✅ | Policy-driven decision making |
| `PlanAction` | Already exists ✅ | Terminal/Researcher planning |
| `SynthesizeResponse` | Already exists ✅ | Final answer generation |

### Event Types to Enforce in Tests

| Event Category | Required Events | Current Coverage |
|----------------|----------------|------------------|
| Run Lifecycle | `conductor.run.started`, `conductor.run.completed` | ✅ Emitted |
| Worker Call | `conductor.worker.call` | ✅ Emitted |
| Worker Output | `worker.task.completed`, `worker.task.finding`, `worker.task.learning` | ✅ Partially emitted |
| Decision | `conductor.decision` | ✅ Emitted |
| Escalation | `watcher.escalation` | ✅ Emitted |
| Blocked | `conductor.capability.blocked` | ❌ **Not emitted** (silently fails) |
| Failed | `worker.task.failed`, `conductor.capability.failed` | ⚠️ Inconsistent |

---

## Conclusion

The ChoirOS runtime architecture is conceptually aligned with the research program's vision, but implementation contains **critical violations** of the non-negotiable rules:

1. **Deterministic authority persists** in TerminalActor (command bypass) and Conductor (threshold-based agenda)
2. **Silent fallbacks** exist when LLM policy calls fail (TerminalActor)
3. **Missing headless E2E tests** prevent automated regression detection
4. **Fragmented loop implementation** creates maintenance burden and inconsistency

**Recommended path forward:**
1. Complete Phase 1-2 (determinism removal + explicit failures) - immediate priority
2. Complete Phase 3.1 (headless E2E tests) - blocks new feature work
3. Complete Phase 3.2 and 4 (observability API + unified harness) - foundation hardening
4. Only then un-freeze new orchestration feature work

This refactoring will establish a solid, observable, and policy-driven foundation that aligns with the research program's goals and enables sustainable development of agentic capabilities.

---

**Report Version:** v4
**Date Generated:** 2026-02-10
**Next Review:** After Phase 1-2 completion
