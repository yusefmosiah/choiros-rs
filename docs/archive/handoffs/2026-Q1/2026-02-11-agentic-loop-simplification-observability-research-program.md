# Agentic Loop Simplification + Observability Research Program (2026-02-11)

Purpose: define the reset plan to simplify runtime architecture, remove hidden bugs, and prove behavior with headless automated tests and live observability.

## Narrative Summary (1-minute read)
We are overcomplicating what should be a simple loop: Conductor sets constraints, workers run agentic loops, Watcher observes logs, and Conductor replans only on meaningful wake signals.  
Current behavior has too many moving parts, weak visibility, and regressions where routing appears deterministic or opaque. The immediate fix is not more features; it is architecture simplification plus verification discipline.

This program defines three lanes that run together:
1. Simplify and delete: aggressively remove dead paths, duplicate abstractions, and hidden deterministic authority.
2. Verify headlessly: treat Prompt Bar as testable text input and automate end-to-end runs.
3. Observe live: make run-level state and semantic progress first-class, so failures are obvious and reproducible.

## What Changed
- Priority changed from “ship next feature” to “stabilize and simplify runtime behavior.”
- Testing posture changed from ad hoc/manual checks to required headless scenario coverage.
- Observability scope changed from raw logs to run-semantic visibility with strict traceability.
- Success criteria now require proving:
  - agentic loop behavior,
  - non-deterministic adaptive replanning,
  - and end-to-end visibility in logs/UI.

## What To Do Next
1. Freeze new orchestration feature work until this program’s acceptance gates pass.
2. Build a minimal, explicit runtime map (Conductor, Researcher, Terminal, Watcher, EventStore).
3. Remove deterministic authority paths and duplicated fallback logic.
4. Stand up headless E2E for Prompt Bar input and run lifecycle assertions.
5. Implement run-level observability E2E API tests that fail locally when live key events are missing.

## Problem Statement
We currently have failures in three categories:
1. Behavioral ambiguity: loop appears agentic in code but deterministic in outcomes.
2. Verification gap: regressions are discovered manually instead of by automated tests.
3. Observability gap: completed runs do not reliably produce coherent run-level traces.

## Target Runtime (Simple, Explicit)
1. Prompt Bar accepts user objective text.
2. Conductor starts/updates run state and delegates natural-language objectives to workers.
3. Worker loops are policy-driven by typed BAML outputs:
   - continue/search/fetch/retry/complete/block decisions.
4. Worker semantic outputs (findings, learnings, blockers) are emitted as typed events.
5. Watcher reviews event windows on lower-cost models and emits typed escalations.
6. Conductor wakes on typed wake events, replans, and updates run status.
7. Final result and run narrative match observed event timeline.

## Non-Negotiable Architecture Rules
1. No deterministic workflow authority.
2. No natural-language phrase matching as control authority.
3. Typed outputs are control authority (BAML enums + actor messages + shared types).
4. No silent fallback paths.
5. Failures must be explicit typed states (`blocked` or `failed`) with reason.
6. Every run must be reconstructable from events without guessing.

## Research Workstream A: Simplify and Rectify
Goal: reduce complexity until control flow is obvious and testable.

### Questions to Answer
1. Which modules still contain duplicated or split control authority?
2. Which code paths are compatibility-only and safe to delete now?
3. Which abstractions hide runtime ownership boundaries?
4. Which event names/payloads are semantically redundant or inconsistent?

### Deliverables
1. Dependency and authority map for Conductor/Researcher/Watcher.
2. Deletion list:
   - dead code,
   - duplicate abstractions,
   - deterministic remnants.
3. Refactor plan with explicit ownership boundaries per module.
4. Naming cleanup plan aligned with Logging / Watcher / Summarizer definitions.

### Acceptance Gate
1. No actor module is control-authority-ambiguous.
2. No deterministic fallback authority remains.
3. Authority path from prompt to completion can be described in < 15 steps without caveats.

## Research Workstream B: Headless Automated Verification
Goal: make Prompt Bar orchestration behavior testable headlessly via API/WebSocket and reproducible locally.

### Test Harness Scope
1. Backend integration tests for actor/event behavior.
2. WebSocket stream assertions for run timeline ordering, scoped isolation, and live arrival before completion.
3. Headless E2E API tests for Prompt Bar run flow:
   - create prompt/run objective through API entrypoint,
   - subscribe to run/log websocket streams,
   - assert streaming updates during run,
   - assert final artifact/report state.

### Required E2E Scenarios
1. Basic run: objective -> completion with run events visible.
2. Replan run: worker returns incomplete -> Conductor dispatches follow-up worker.
3. Watcher wake run: escalation causes Conductor policy wake/replan.
4. Blocked run: policy failure produces explicit blocked state (no hidden fallback).
5. Concurrency run: multiple worker calls active simultaneously with stable run status.
6. Observability run: semantic findings/learnings appear in logs and watcher review windows.
7. Live-stream run: researcher `finding`/`learning` and terminal action/reasoning summaries are observed before `run.completed`.

### Assertions (Must Be Automated)
1. Event sequence contains required milestone topics for each run.
2. Every run has stable `run_id` correlation across worker/watcher/conductor events.
3. Streamed semantic events arrive while run status is `running` (not only after completion).
4. Live stream includes:
   - researcher finding/learning events,
   - terminal action/reasoning summary events.
5. UI-visible status matches backend run status.
6. Report output references actual run artifacts and not placeholder summaries.

### Acceptance Gate
1. All required scenarios pass headlessly.
2. A regression that removes/renames required events fails tests immediately.
3. After headless gates are stable, run `agent-browser` E2E as the next-stage UI validation.

## Research Workstream C: Live Observability
Goal: make runtime truth visible while runs are in progress.

### Minimum Run Visibility Contract
1. Run identity: run_id, task_id, correlation_id.
2. Current status: queued/running/waiting/completed/blocked/failed.
3. Active workers and in-flight calls.
4. Semantic timeline:
   - findings,
   - learnings,
   - escalations,
   - key decisions.
5. Test outcomes and failure reasons per run.

### Required Observability Features
1. Logs view must show current Conductor/Watcher/Worker semantic events by run.
2. Missing required run milestones must be detectable (and test-assertable).
3. Watcher loop idleness must be explicit:
   - sleep after last event,
   - resume on next event,
   - never self-loop on watcher-origin events.

### Acceptance Gate
1. “What is happening now?” can be answered from live run state without guessing.
2. A completed run has a coherent, queryable timeline from start to finish.
3. Live stream proof exists for each run class (events observed pre-completion).

## Program Milestones
1. Milestone 1: Architecture audit + deletion candidates.
2. Milestone 2: Refactor cutover (authority cleanup).
3. Milestone 3: Headless E2E API + websocket live-stream tests.
4. Milestone 4: `agent-browser` E2E validation (only after Milestone 3 is passing).
5. Milestone 5: Observability hardening and run-level narrative validation.
6. Milestone 6: Stabilization checkpoint before resuming roadmap feature work.

## Definition of Done
This program is complete only when all are true:
1. Agentic loops are demonstrably policy-driven and adaptive.
2. Deterministic fallback/control remnants are removed.
3. Prompt Bar end-to-end behavior is verified by headless automated tests.
4. Run-level observability is complete and trustworthy in real time.
5. Failures are explicit, typed, and debuggable without manual archaeology.
