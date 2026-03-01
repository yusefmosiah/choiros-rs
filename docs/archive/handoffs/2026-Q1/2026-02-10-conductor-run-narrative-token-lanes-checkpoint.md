# Conductor Run Narrative + Token-Lane Checkpoint (2026-02-10)

Purpose: consolidate the current direction for `03.5.1 -> 03.6 -> 04` into one implementation baseline.

## Narrative Summary (1-minute read)
ChoirOS must move from deterministic workflow scripts to concurrent, typed, agentic orchestration.  
Conductor is expensive and should not consume routine execution chatter. Worker and tool events stream continuously to UI and Watcher; Watcher performs low-cost semantic inference over event windows; Conductor wakes only on high-value control moments and replans from typed state.

From the user perspective, the system should be run-centric (not agent-centric): show meaningful semantic progress and an accumulated natural-language run narrative, with raw tool/event logs as optional drill-down.

## What Changed
- We are explicitly prioritizing **checkpoint correctness before token-efficiency optimization**.
- Wake strategy is now baseline-defined:
  - wake Conductor on run start,
  - wake Conductor on capability success completion,
  - wake Conductor on Watcher escalation.
- Routine progress/milestones remain evented and visible, but do not spend Conductor tokens unless escalated.
- We are adopting an aggressive cleanup stance: remove deterministic authority and stale compatibility paths quickly once replaced by typed contracts.
- `03.5.1` is treated as the cutover gate (Conductor + Watcher policy authority via BAML, no deterministic fallback authority).

## What To Do Next
1. Ship a basic working concurrent runtime where Conductor can always poll/pull canonical run state.
2. Make run narrative and semantic events first-class in UI and backend contracts.
3. Keep Watcher wake heuristics simple first; optimize prompts/models/evals later.
4. Continue aggressive codebase simplification to reduce split-brain abstractions.

## 03.5.1 Exit Gate (Must Hold Before 03.5.2)
1. Conductor policy decisions come from BAML output (typed decision enums), not preset worker order.
2. Watcher review/mitigation comes from BAML output (typed escalation/action enums), on lower-cost models than Conductor.
3. Policy failure behavior is explicit typed failure (`blocked`/`failed`), not silent deterministic fallback.
4. Model policy loading is robust across launch directories (repo-root config discovered reliably).
5. Watcher escalations without concrete `run_id` are observable but do not wake Conductor control loops.

## Core Runtime Principles
1. Conductor is orchestration authority for multi-step work.
2. Worker execution is asynchronous and parallel (non-blocking Conductor).
3. Control authority stays typed (actor messages + shared types + BAML outputs).
4. No deterministic fallback workflows.
5. Watcher is lower-cost inference lane; Conductor is high-cost planning lane.

## Baseline Wake Policy (Checkpoint, Not Final Optimization)
Conductor wake triggers:
1. `run.start`
2. `capability.completed.success`
3. `watcher.escalation.*`
4. optional terminal convergence timer/checkpoint

Non-wake traffic:
1. routine progress logs
2. intermediate milestones
3. verbose tool-call churn

These non-wake events must still be persisted, visible, and analyzable by Watcher and UI.

## Run-Centric UX Contract
Default user view:
1. accumulated natural-language run description
2. semantic timeline (high-signal events)
3. run health/status (working, blocked, needs input, completed)
4. test summary card (pass/fail deltas)

Secondary (drill-down) view:
1. raw tool calls/results
2. full event stream
3. per-worker traces

## Semantic Event Model (High-Signal)
Use semantic event families that improve human supervision and autonomous replan quality:
1. `hypothesis.evidence_for`
2. `hypothesis.evidence_against`
3. `finding.created` (severity/confidence)
4. `bug.discovered`
5. `bug.fixed`
6. `code_hygiene.issue`
7. `test.result` (suite, pass/fail, failing set)
8. `decision.made` (chosen path + rationale)
9. `artifact.created`
10. `risk.blocker` / `needs_input`
11. `milestone.reached`
12. `run.completed|failed|blocked`

## Conductor Wake Context Contract
On every wake, Conductor reads:
1. typed run state snapshot
2. latest semantic event window
3. accumulated run description
4. relevant Watcher escalations

Important: Conductor may read description text for continuity, but **typed outputs remain control authority**.

## 03.5.2 Checkpoint Scope (Immediate Follow-On)
1. True concurrent dispatch under one run:
   - multiple ready agenda items can be dispatched without blocking one another,
   - active capability calls are tracked independently by typed call status.
2. Run narrative as first-class state:
   - semantic events continuously update accumulated run description,
   - narrative is available to UI and included in Conductor wake context.
3. Token-lane behavior preserved:
   - routine execution chatter stays in Watcher/UI lanes,
   - Conductor wakes only on baseline triggers until optimization phase.

## Checkpoint Acceptance Criteria
1. Conductor can always poll/pull canonical worker/run state even if wake signals are imperfect.
2. Two or more capability calls can run concurrently under one run.
3. Watcher can emit typed escalations that wake Conductor and influence replanning.
4. UI shows meaningful semantic progress by default, with raw logs as optional debug view.
5. Policy/model failures transition to explicit typed `blocked` or `failed` states (no silent deterministic fallback).

## Cleanup Direction (Aggressive Simplification)
1. Remove dead deterministic orchestration authority as soon as typed replacement exists.
2. Remove duplicate legacy-vs-new runtime abstractions that split authority.
3. Keep compatibility only where absolutely required by active external surfaces.
4. Prefer fewer, clearer contracts over temporary parallel pathways.

## Optimization Phase (Later, Not Blocking Checkpoint)
After baseline works end-to-end:
1. tune wake thresholds and escalation policy
2. rotate Watcher/Conductor models and prompts frequently
3. add eval loops and prompt/model optimization workflows (including DSPy-style techniques)
4. compare live run description vs final result to extract reusable lessons

## Definition of Done for This Consolidation
This checkpoint is complete when:
1. architecture docs and prompts share this runtime mental model,
2. implementation work references this doc as the baseline,
3. no active docs claim deterministic workflow authority as acceptable runtime behavior.
