# Worker Live-Update Event Model

Date: 2026-02-14  
Status: Canonical runtime behavior  
Scope: Worker execution signals and live document update flow

## Narrative Summary (1-minute read)

Workers should do work and stream simple typed events.
We do not keep a separate "worker signal contract" abstraction.

The runtime model is:
1. workers execute bounded tasks,
2. workers emit `progress`, `result`, `failed`, or `request`,
3. app agents and Writer render/apply live document updates.

This keeps control flow explicit and removes old signaling complexity.

## What Changed

1. Removed the standalone Worker Signal Contract concept.
2. Standardized worker runtime outputs to four message kinds.
3. Clarified that live document updates are first-class runtime behavior.
4. Kept Conductor as orchestration-only and event-driven.

## What To Do Next

1. Remove legacy docs and references that depend on worker-signal-contract terminology.
2. Align event naming and tests around `progress/result/failed/request`.
3. Keep live document updates wired through Writer/app-agent paths.
4. Remove remaining task-id fallback paths while preserving scoped run correlation.

---

## Worker Output Model

Workers emit only:
1. `progress` - in-flight state updates.
2. `result` - successful completion payload.
3. `failed` - terminal failure payload.
4. `request` - typed ask to Conductor/app agent for guidance or coordination.

No extra signaling subsystem is required.

## Live Document Behavior

1. Worker events stream in real time.
2. App agents synthesize and shape interactive updates.
3. Writer remains canonical for living-document/revision mutation authority.
4. Users see document updates live as work progresses.

## Runtime Invariants

1. Conductor does not poll workers.
2. Conductor does not execute tools directly.
3. Tool schemas are shared once and reused via capability grants.
4. Terminal and Researcher include file tools as baseline worker capability.

## Acceptance Signals

1. Worker paths emit only the four canonical message kinds.
2. Live document updates are visible during in-flight worker execution.
3. No runtime path relies on the old worker signal contract abstraction.
4. Docs and tests describe one coherent worker event model.
