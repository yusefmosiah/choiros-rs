# Unified Agentic Harness Refactor Checklist (All Loops)

Date: 2026-02-11
Owner: Next implementation session
Status: Planned (not started in this session)

## Narrative Summary (1-minute read)
The next refactor should not be limited to Terminal/Researcher/Chat. Conductor and Watcher are also agentic loops and must converge into the same harness universe.

Target: one shared loop framework that supports any number of loop-capable actors through typed adapters. Existing loops (Conductor, Watcher, Chat, Terminal, Researcher) are the first wave; future loops must plug into the same contract without new ad hoc orchestration logic.

This checklist is the implementation plan for that converged architecture.

## What Changed
1. Baseline hardening is complete in commit `401d0fd`:
- terminal deterministic bypass removed,
- planner failure returns explicit blocked state,
- conductor bootstrap uses typed BAML policy,
- run timeline API exists,
- live external-LLM E2E currently passing.

2. Harness refactor has not started.
- This document now explicitly includes Conductor + Watcher, plus future-loop extensibility.

## What To Do Next
1. Build a shared harness framework that supports both worker loops and orchestration/review loops.
2. Migrate Terminal, Researcher, Chat, Conductor, Watcher onto typed adapters.
3. Add a loop registry so new agent loops can be added without bespoke control flow.
4. Enforce a single observability/event contract across all loops.
5. Validate with unit, integration, and live external-LLM scenarios.

---

## Scope and Guardrails

### In Scope
- Shared loop harness used by:
  - Conductor
  - Watcher
  - Chat
  - Terminal
  - Researcher
- Adapter/trait system for arbitrarily many future loops.
- Unified typed lifecycle and event semantics.
- Ordered integration + live E2E validation.

### Out of Scope (unless explicitly requested)
- New user-facing features unrelated to loop unification.
- Unrelated UI work.
- Model-policy redesign beyond adapter pluggability.

### Hard Rules
- No natural-language phrase matching as control authority.
- No silent fallbacks.
- Typed states/messages only for loop transitions.
- Keep naming semantics strict:
  - Logging = capture/persist/transport
  - Watcher = deterministic detection/alerting over logs
  - Summarizer = human-readable compression

---

## Pre-Flight Checklist
- [ ] Start from latest branch including `401d0fd`.
- [ ] Confirm clean/intended working tree: `git status --short`.
- [ ] Run baseline tests before structural edits:
  - [ ] `./scripts/sandbox-test.sh --lib conductor`
  - [ ] `./scripts/sandbox-test.sh --lib terminal`
  - [ ] `./scripts/sandbox-test.sh --test run_lifecycle_e2e_test -- --nocapture` (live)

---

## Code Map (Primary Files)

### Conductor loop
- `sandbox/src/actors/conductor/actor.rs`
- `sandbox/src/actors/conductor/runtime/bootstrap.rs`
- `sandbox/src/actors/conductor/runtime/decision.rs`
- `sandbox/src/actors/conductor/runtime/call_result.rs`
- `sandbox/src/actors/conductor/policy.rs`

### Watcher loop
- `sandbox/src/actors/watcher.rs`

### Terminal loop
- `sandbox/src/actors/terminal.rs`

### Researcher loop
- `sandbox/src/actors/researcher/mod.rs`
- `sandbox/src/actors/researcher/policy.rs`
- `sandbox/src/actors/researcher/events.rs`

### Chat loop
- `sandbox/src/actors/chat_agent.rs`

### API/observability touchpoints
- `sandbox/src/api/run_observability.rs`
- `sandbox/src/api/logs.rs`

### Architecture references
- `docs/architecture/unified-agentic-loop-harness.md`
- `docs/architecture/2026-02-11-agentic-loop-simplification-observability-research-program.md`

---

## Unified Harness Target

All loop actors conform to a shared loop contract:
1. receive input/signal
2. plan/assess next action (typed)
3. execute tool/delegation/review action
4. observe results/events
5. continue, defer, complete, block, or fail (typed)

Two loop classes share the same framework:
- Worker loops: Chat/Terminal/Researcher
- Orchestration/review loops: Conductor/Watcher

No class gets ad hoc authority.

---

## Phase-by-Phase Implementation Checklist

## Phase 0: Canonical Loop Contract
- [ ] Define one canonical state machine used by all loop classes.
- [ ] Define typed transition enum (example: `Receive`, `Plan`, `Act`, `Observe`, `Continue`, `Defer`, `Complete`, `Block`, `Fail`).
- [ ] Define common loop result/report type with typed artifacts/findings/learnings/escalations.
- [ ] Define metadata requirements (`run_id`, `task_id`, `call_id`, `capability`, `phase`, correlation/scope).

### Phase 0 Acceptance
- [ ] Contract documented in code comments + architecture doc update.
- [ ] No actor retains private implicit loop states.

---

## Phase 1: Harness Core + Adapter Interfaces
- [ ] Create `sandbox/src/actors/agentic_harness/mod.rs`.
- [ ] Implement shared execution engine for typed transitions.
- [ ] Add adapter traits for loop-specific behavior:
  - [ ] plan function
  - [ ] action execution function
  - [ ] result observation formatting
  - [ ] synthesis/finalization function
  - [ ] event emission function
  - [ ] policy/model resolution function
- [ ] Add explicit blocked/failure error classes.
- [ ] Export harness in `sandbox/src/actors/mod.rs`.

### Phase 1 Acceptance
- [ ] Harness compiles with mock adapters.
- [ ] Unit tests cover success, block, fail, defer/resume.

---

## Phase 2: Worker Loop Migration (Terminal + Researcher)
- [ ] Migrate Terminal loop to `TerminalCapabilityAdapter`.
- [ ] Migrate Researcher loop to `ResearcherCapabilityAdapter`.
- [ ] Preserve existing typed events and explicit blocked semantics.
- [ ] Ensure no deterministic shortcuts are reintroduced.

### Phase 2 Acceptance
- [ ] Terminal and Researcher tests pass.
- [ ] Live E2E still passes.

---

## Phase 3: Worker Loop Migration (Chat)
- [ ] Migrate Chat loop to `ChatCapabilityAdapter` or compatibility wrapper using shared harness.
- [ ] Keep delegated-task semantics and non-blocking continuation behavior.
- [ ] Ensure final synthesis path remains typed and deterministic-authority free.

### Phase 3 Acceptance
- [ ] Chat core tests pass.
- [ ] No regression in chat tool/delegation flow.

---

## Phase 4: Conductor Migration (Orchestration Loop)
- [ ] Wrap Conductor runtime decision loop in harness adapter (`ConductorOrchestrationAdapter`).
- [ ] Move policy step + dispatch/retry/spawn/complete/block transitions under shared loop transitions.
- [ ] Keep typed agenda/call/run state authoritative.
- [ ] Preserve blocked/failed semantics and wake-policy provenance checks.

### Phase 4 Acceptance
- [ ] Conductor runtime tests pass.
- [ ] No change in authoritative typed agenda semantics.

---

## Phase 5: Watcher Migration (Review Loop)
- [ ] Wrap Watcher scan/review/escalation loop in harness adapter (`WatcherReviewAdapter`).
- [ ] Keep deterministic detection role semantics while using shared transition framework.
- [ ] Preserve self-trigger prevention and event-window behavior.
- [ ] Keep escalation emissions typed for Conductor wake lane.

### Phase 5 Acceptance
- [ ] Watcher tests pass.
- [ ] No self-loop regressions.

---

## Phase 6: Loop Registry for Arbitrarily Many Loops
- [ ] Add a loop registry abstraction (`LoopKind`, adapter registration, capabilities metadata).
- [ ] Support dynamic addition of new loop actors with no bespoke runtime logic.
- [ ] Define onboarding checklist/template for new loop adapters.
- [ ] Enforce compile-time or startup-time validation of required adapter hooks.

### Phase 6 Acceptance
- [ ] Add one synthetic test adapter to prove registry extensibility.
- [ ] New adapter can run through full harness lifecycle with no actor-specific hacks.

---

## Phase 7: Observability and Event Contract Unification
- [ ] Normalize loop lifecycle events across all adapters.
- [ ] Ensure timeline endpoint can reconstruct all loop classes coherently.
- [ ] Ensure watcher filtering rules still operate correctly with unified events.
- [ ] Add milestone assertions that fail on missing/renamed required events.

### Phase 7 Acceptance
- [ ] Timeline API remains complete and ordered for all loop classes.
- [ ] Missing milestones fail tests automatically.

---

## Phase 8: Test Matrix and Validation

### Required tests
- [ ] Harness unit tests (all transition outcomes).
- [ ] Adapter-level tests per loop class.
- [ ] Ordered integration tests: defer -> completion signal -> resume -> final outcome.
- [ ] Live external-LLM E2E for orchestrated runs.

### Required commands
- [ ] `./scripts/sandbox-test.sh --lib conductor`
- [ ] `./scripts/sandbox-test.sh --lib terminal`
- [ ] `./scripts/sandbox-test.sh --lib researcher` (or exact target equivalent)
- [ ] `./scripts/sandbox-test.sh --test run_lifecycle_e2e_test -- --nocapture`
- [ ] Exact-target integration binaries added by refactor.

### Phase 8 Acceptance
- [ ] All updated tests pass locally.
- [ ] Live external-LLM E2E remains green.

---

## Final Acceptance Gates
- [ ] Conductor, Watcher, Chat, Terminal, Researcher all run through shared harness framework.
- [ ] No deterministic authority remnants in control paths.
- [ ] No silent fallback behavior.
- [ ] Typed blocked/failed semantics preserved across all loop classes.
- [ ] Event/timeline observability remains complete and ordered.
- [ ] New loop adapter can be added via registry without bespoke orchestration logic.

---

## Risk Register
1. Scope explosion across five actors.
- Mitigation: phase migrations and gate each phase with tests.

2. Event schema drift breaks watcher/log consumers.
- Mitigation: compatibility assertions + timeline milestone tests.

3. Conductor behavior regression from structural changes.
- Mitigation: keep typed run/agenda state as source of truth and preserve existing runtime tests.

4. Live LLM test flakiness.
- Mitigation: assert typed milestones/order, not model prose.

---

## Suggested Session Kickoff Prompt (Copy/Paste)
"Implement unified agentic harness refactor across Conductor, Watcher, Chat, Terminal, and Researcher using `/Users/wiz/choiros-rs/docs/handoffs/2026-02-11-unified-agentic-harness-refactor-checklist.md` as checklist of record. Build shared harness + adapter registry for arbitrary future loop actors, preserve typed authority and blocked semantics, and validate with live external-LLM E2E before finish."
