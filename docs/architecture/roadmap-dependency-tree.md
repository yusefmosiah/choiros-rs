# ChoirOS Roadmap Dependency Tree

Date: 2026-02-06
Status: Working roadmap (execution-oriented)

## Purpose

Define a safe execution order for multiagent rollout, context management, sandbox persistence, app expansion, hypervisor work, and Nix/NixOS migration.

This roadmap is intentionally biased toward delivery safety and low rework.

## Critical Analysis of Current Plan

What is strong:
- Correct priority: finish supervision cutover before multiagent complexity.
- Correct intuition: context architecture and sandbox persistence affect long-term design.
- Correct ambition: move chat from direct tools to specialist agents.

What is risky if executed naively:
- Building advanced context layering before identity/scoping can cause data-leak rework.
- Forcing chat to delegate all tool calls immediately can reduce reliability and increase latency.
- Expanding app surface (mail/calendar/media) before core control-plane maturity creates maintenance drag.
- Running a full Nix/NixOS rebase in parallel with platform refactor can stall feature velocity.

Key simplification:
- Keep one canonical state backend (SQLite/libsql) for events and memory.
- Treat JSONL as export/debug artifact, not source of truth.
- Build minimal interfaces now, full implementations later.

## Dependency Tree

```text
A. Supervision Cutover (complete)
   |
   +--> B. Multiagent Control Plane v1
   |      |
   |      +--> C. Chat Delegation Refactor (planner/router + specialists)
   |      |      |
   |      |      +--> D. Context Broker v1 (layered retrieval)
   |      |             |
   |      |             +--> E. App Expansion Wave 1 (file explorer, settings, viewers, iframe)
   |      |
   |      +--> F. Identity and Scope Enforcement v1
   |             |
   |             +--> D. Context Broker v1 (hard dependency for safe memory isolation)
   |             +--> G. SandboxFS Persistence (sqlite/libsql snapshots)
   |                    |
   |                    +--> H. Hypervisor Integration
   |
   +--> I. Nix/NixOS Migration (staged, cross-cutting)
          +--> I1. Dev shell + toolchain pinning
          +--> I2. CI parity and reproducible builds
          +--> I3. Deploy packaging
          +--> I4. Host-level NixOS operations
```

## Critical Path

1. B Multiagent Control Plane v1
2. F Identity and Scope Enforcement v1
3. C Chat Delegation Refactor
4. D Context Broker v1
5. G SandboxFS Persistence
6. H Hypervisor Integration

Notes:
- F must land before D is considered production-safe.
- C can start in parallel with F, but production rollout depends on F.

## Now vs Later

Now (must execute before multiagent rollout):
- B Multiagent Control Plane v1
- F Identity and Scope Enforcement v1 (minimal, not full auth platform)
- C Chat Delegation Refactor v1 (selective delegation)
- D Context Broker v1 (summary + raw handle expansion)

Next (after stable multiagent baseline):
- G SandboxFS Persistence
- H Hypervisor Integration
- E App Expansion Wave 1

Later (do not block product path):
- E App Expansion Wave 2 (mail, calendar, richer multimedia)
- I3/I4 deeper Nix/NixOS operations changes

## Execution Phases and Gates

### Phase B - Multiagent Control Plane v1
Objective:
- Introduce supervisor orchestration and specialist agent contracts.

Deliverables:
- Message protocols for Supervisor, TerminalAgent, ResearcherAgent, DocsUpdater.
- Correlation IDs and event topic conventions.
- One end-to-end delegated flow.

Gate:
- A delegated terminal or research task completes with persisted events and traceable correlation ID.

### Phase F - Identity and Scope Enforcement v1
Objective:
- Prevent cross-user/session data leakage before compounding memory.

Deliverables:
- Required scope keys on requests/events: user_id, session_id, app_id, thread_id, sandbox_id.
- Enforcement checks at API and supervisor boundaries.

Gate:
- Isolation tests prove no cross-user/session retrieval leakage.

### Phase C - Chat Delegation Refactor v1
Objective:
- Chat is planner/router for specialist tasks; retain narrow direct-tool fallback.

Deliverables:
- Routing policy: which intents delegate and which stay local.
- Timeout/retry/error contracts for specialist calls.

Gate:
- Chat delegation success on terminal and research paths with graceful degradation on agent failure.

### Phase D - Context Broker v1
Objective:
- Layered context retrieval with compounding intelligence and drill-down handles.

Deliverables:
- Canonical events in libsql.
- Derived memory layers: global, workspace, session, thread.
- API: brief_context + relevant_handles + expand(handle).

Gate:
- Relevance test shows prior session insights improve a later task without violating scope boundaries.

### Phase G - SandboxFS Persistence
Objective:
- Durable virtual filesystem with snapshot and rehydrate.

Deliverables:
- SandboxFS interface: read/write/list/delete/snapshot/rehydrate.
- SQLite/libsql-backed storage and versioned snapshots.

Gate:
- Restart/rehydrate test restores files and metadata deterministically.

### Phase H - Hypervisor Integration
Objective:
- Bind sandbox lifecycle to identity and persistent state.

Deliverables:
- Session-attached sandbox allocation.
- Attach/detach/restore lifecycle.

Gate:
- Multi-session integration tests pass with strict isolation.

## App Expansion Strategy

Wave 1 (after D, optionally parallel with G):
- File Explorer
- Settings
- Multimedia and generic viewers
- Safe iframe window + YouTube embed support

Wave 2 (after G + H maturity):
- Mail
- Calendar
- Advanced media workflows

Rule:
- No new app requiring persistent per-user data until F is complete.

## Nix/NixOS Strategy (Do Not Stall Product)

Stage 1 (early):
- Nix dev shell with pinned toolchain.

Stage 2 (early-mid):
- CI reproducibility parity.

Stage 3 (mid):
- Deployment packaging improvements.

Stage 4 (late):
- NixOS host operations migration.

Rule:
- Product-critical milestones cannot be blocked on Stage 4.

## Risk Register and Mitigations

Risk: Context leakage across users/sessions.
- Mitigation: enforce scope keys and retrieval guards before memory compounding.

Risk: Delegation brittleness (agent unavailable or slow).
- Mitigation: selective delegation with bounded local fallback and hard timeouts.

Risk: Competing migrations (multiagent + Nix + hypervisor) cause thrash.
- Mitigation: WIP limits, staged gates, no cross-cutting epic starts without gate green.

Risk: Documentation drift during rapid implementation.
- Mitigation: require handoff update after each phase gate pass.

## Definition of Ready for Multiagent Rollout

- Supervision-first runtime is stable.
- Control-plane contracts are implemented and tested for at least one delegated flow.
- Scope enforcement v1 is active on all relevant request/event paths.
- Context broker API shape is defined (implementation may be partial, but interface is locked).

## Definition of Done for Pre-Sandbox Foundation

- Phases B, F, C, D are complete and green.
- No known cross-scope data leak issues.
- Delegated chat workflows are observable and recoverable.
- Clear implementation plan for G and H is documented with test gates.
