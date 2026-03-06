# ChoirOS Multi-Agent Vision (2026 Bridge Doc)

## Purpose
This document bridges the current actorcode prototype with the aspirational ChoirOS multi-agent system. It describes the desired architecture, the reasoning model for agents, and the gaps between what exists today and where we want to go.

## North Star
Build a self-verifying SDLC automaton where:
- Notes, runs, evidence, and decisions flow through a unified event log.
- Watchers observe everything and signal attention only when needed.
- Supervisors wake on signals, not on polling.
- Verification gates determine when work is truly complete.
- The system scales beyond any single context window by promotion and delegation.

## Core Abstractions

### Events and Evidence
- **Event**: the canonical unit in the system (append-only).
- **Evidence**: proof that a requirement is satisfied (tests, screenshots, approvals, citations).
- **Decision**: a gate result derived from evidence.

### Notes vs Learnings
- **Notes**: raw, low-friction observations (never labeled as learnings).
- **Learnings**: derived from notes + evidence (human- or verifier-backed).
- Learnings do not replace notes; they emerge from them.

### Verifier Lattice
- **Coherence**: internal consistency within docs and claims.
- **Repo-Truth**: alignment with code/config reality.
- **World-Truth**: alignment with external sources and dependencies.
- **Human Gate**: required for architectural invariants and public-facing claims.

### Hyperthesis
- **Hyperthesis**: beliefs that cannot yet be verified.
- Non-blocking, clearly flagged, and never treated as fact.
- Promoted to claims only after evidence exists.

## Roles and Responsibilities

### Watchers (Pico)
- Observe all events on the bus.
- Emit `watcher.signal` only when patterns or thresholds warrant attention.
- Never mutate state; signals only.

### Workers (Nano)
- Execute bounded tasks or doc slices.
- Emit structured outputs plus evidence.

### Supervisors (Micro)
- Wake on signals or task assignments.
- Coordinate workers, enforce verification gates, and reconcile outputs.

### Producer (Supervisor Capability)
- The producer role is a supervisor with authority to spawn more supervisors.
- Promotion is a role change, not a new agent type.

## Event Bus Schema (Minimal v0.1)

### `note.created`
- `note_id`, `text`, `author`, `channel`, `context`, `timestamp`

### `watcher.signal`
- `signal_id`, `watcher_id`, `priority`, `reason`, `evidence`, `target`, `timestamp`

### `supervisor.wake`
- `wake_id`, `trigger`, `intent`, `timestamp`

### Subscription Contract
- `watcher.subscribe`: `watcher_id`, `filter`, `priority`, `rate_limit`, `timestamp`
- `watcher.unsubscribe`: `watcher_id`, `timestamp`

## Actix-Based Architecture (Target)

### Core Actors
- `EventStoreActor`: canonical append-only log (exists).
- `BusActor`: broadcast events to subscribers (Planned - Not Implemented).
- `NotesActor`: writes notes and emits `note.created` (Planned - Not Implemented).
- `WatcherActor`: pico observers that emit `watcher.signal` (Planned - Not Implemented).
- `SupervisorActor`: sleeps until signals arrive (Planned - Not Implemented).
- `RunActor`: one run lifecycle (Planned - Not Implemented).
- `RunRegistryActor`: run status, staleness detection (Planned - Not Implemented).
- `SummaryActor`: single-call summaries (glm-4.7-flash) (Planned - Not Implemented).

### Mailbox Semantics
- Actors process messages one at a time.
- Backpressure via mailbox capacity.
- `Supervisor` and `Watcher` use timers or bus subscriptions, not polling loops.

## Current Actorcode Prototype (Reality)

### What Exists
- HTTP-orchestrated runs via OpenCode SDK.
- Findings pipeline + JSONL store + web dashboard.
- Research launch/monitor tooling.
- Sessions registry and log tailing.

### What is Missing
- True event bus (broadcast vs polling).
- Notes stream as a first-class event source.
- Watchers that emit structured signals.
- Durable run lifecycle with Actix supervision.
- Verifier lattice integrated into completion gates.

## Bridging Map (Prototype -> Target)

| Prototype (Actorcode) | Target (ChoirOS) | Gap |
| --- | --- | --- |
| OpenCode sessions | RunActor lifecycle | Replace polling with event-driven bus |
| Findings JSONL | Evidence + learnings | Add verifiers and evidence typing |
| Dashboard.html (port 8765) | Actor UI app (port 8080) | Replace actorcode dashboard with native ChoirOS UI; add notes stream + summary views |
| research-monitor | WatcherActor | Replace long polls with signals |
| registry.json | RunRegistryActor | Durable actor registry + staleness rules |

## Immediate Priorities
1) Notes stream + watcher signaling (bus schema + minimal UI).
2) Observability: whole-log + summary views (per run, no mixing).
3) Doc accuracy verifier (coherence + repo-truth + world-truth).
4) Supervisor sleep/wake semantics (no blocking main thread).

## Risks
- Multiplying agents increases noise without strong signal gating.
- Multi-writer docs require ownership, review, and verifier gates.
- Polling loops will collapse under scale; event bus is mandatory.

## Open Questions
- Where does the canonical event log live (per sandbox or shared)?
- What is the minimum required external validation set?
- How to represent long-lived hypertheses without confusing users?

## References
- `docs/DOCUMENTATION_UPGRADE_PLAN.md`
- `skills/actorcode/dashboard.html` (actorcode prototype, port 8765 - separate system)
- `skills/actorcode/scripts/findings-server.js` (actorcode prototype server - NOT ChoirOS)
