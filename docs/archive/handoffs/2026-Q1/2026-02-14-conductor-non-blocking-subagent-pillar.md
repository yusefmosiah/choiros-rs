# Conductor Non-Blocking Subagent Pillar

Date: 2026-02-14  
Status: Key design pillar (authoritative)  
Scope: Conductor turn model, agent tree context, non-blocking orchestration invariants

## Narrative Summary (1-minute read)

Conductor should reason about workers and app agents as its logical subagents.
That hierarchy is real, but implementation must be actor-message-native, not blocking parent-child calls.

Conductor must never poll child agents and must never block waiting for child completion.
Another human input can arrive at any time, so conductor turns must stay finite and preemptable.

Child progress and completion arrive as pushed actor events.
Conductor wakes on those events, updates intent/priority, emits the next delegations, and yields.

Each wake should include a bounded system agent-tree state digest so the model plans with current topology,
leases, status, and recent signals.

## What Changed

1. Declared "logical subagents" as the conductor mental model for workers and app agents.
2. Declared polling and blocking parent loops as explicitly disallowed for conductor turns.
3. Declared event-driven wake semantics as the only orchestration progression path.
4. Declared agent-tree wake context as required prompt/state input for conductor decisions.
5. Clarified why CLI-style subagent APIs are insufficient as orchestration authority:
   they are often one-level and polling-oriented.
6. Replaced escalation-heavy wording with a minimal `request` message primitive.

## What To Do Next

1. Add a typed `agent_tree_snapshot` envelope included on every conductor wake.
2. Enforce non-blocking turn invariants in conductor runtime and tests.
3. Remove any code path that waits in loops for child completion.
4. Add websocket/integration tests proving progress via pushed events, not polling.
5. Add human-interrupt tests proving new user input can preempt ongoing orchestration.
6. Adopt `request` message v0 instead of an escalation subsystem.

Contract reference:
- `docs/architecture/2026-02-14-agent-tree-snapshot-contract.md`
- `docs/architecture/2026-02-14-conductor-request-message-v0.md`

---

## Core Pillar Statement

Conductor orchestrates a hierarchy of logical subagents over actor messaging.
The hierarchy is authoritative; the runtime behavior is event-driven and non-blocking.

## Why This Pillar Exists

Common subagent APIs in coding CLIs are useful but not sufficient for ChoirOS control-plane authority:
1. They are usually single-level parent-child abstractions.
2. They commonly require explicit polling for completion/background status.
3. They can encourage blocking parent control loops.

ChoirOS requires deeper, concurrent, interruptible orchestration where humans and agents can both
trigger control turns at any time.

## Conductor Turn Invariants (Hard Rules)

1. A conductor turn is finite and non-blocking.
2. Conductor never polls child agents.
3. Conductor never waits in blocking loops for child completion.
4. Child work advances through pushed actor events (`progress`, `result`, `failed`, `request`).
5. Human input always remains first-class wake traffic.

## Wake Context Requirement

Every conductor wake should include a bounded system agent-tree digest with at least:
1. `agent_id`, `role`, `parent_agent_id`.
2. lifecycle/status (`idle|running|blocked|failed|completed`).
3. lease state (`lease_owner`, `lease_expires_at`) where relevant.
4. last signal time and last signal kind.
5. active run/correlation handles.
6. request metadata when present (`request_kind`, optional `dedupe_key`, optional `ttl`).

This keeps planning model-led while preserving deterministic safety rails.

## Message-Driven Progression Pattern

1. Human/agent message wakes conductor.
2. Conductor emits typed delegations and metadata-rich objectives.
3. Worker/app agents execute independently.
4. Progress/results/requests push events back.
5. Conductor wakes again, replans, delegates, and yields.

No polling. No blocking.

## Acceptance Signals

1. No conductor code path blocks on child completion.
2. No conductor code path polls child status.
3. Run progression remains correct under concurrent child work.
4. Human interrupts are accepted while child work is in-flight.
5. Conductor prompts include bounded agent-tree snapshot context on wake.
