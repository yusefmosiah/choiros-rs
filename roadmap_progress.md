# Roadmap Progress

Date: 2026-02-07
Source roadmap:
- `docs/architecture/roadmap-dependency-tree.md`
- `docs/architecture/roadmap-critical-analysis.md`

## Critical Path Status

| Phase | Status | Notes |
|---|---|---|
| B. Multiagent Control Plane v1 | In progress | Phase A foundation unblocked; worker pattern still pending |
| F. Identity and Scope Enforcement v1 | In progress | Started scoped chat payloads (`session_id`, `thread_id`) |
| C. Chat Delegation Refactor v1 | Pending | Depends on B baseline contracts |
| D. Context Broker v1 | Pending | Blocked on F hardening |
| G. SandboxFS Persistence | Pending | Not started in this session |
| H. Hypervisor Integration | Pending | Not started in this session |

## Completed In This Session

### 1) Phase A Foundation: EventBus Integration + Correlation Tracing
- Wired `EventBusActor` into `ApplicationSupervisor` as a supervised child.
- Added supervisor request lifecycle events on all top-level calls:
  - `supervisor.desktop.get_or_create.{started|completed|failed}`
  - `supervisor.chat.get_or_create.{started|completed|failed}`
  - `supervisor.chat_agent.get_or_create.{started|completed|failed}`
  - `supervisor.terminal.get_or_create.{started|completed|failed}`
- Added per-request correlation IDs using ULID and attached them to published EventBus events.

Files:
- `sandbox/src/supervisor/mod.rs`

Validation:
- `cargo check -p sandbox`
- `cargo test -p sandbox --features supervision_refactor --test desktop_supervision_test`
- `cargo test -p sandbox --features supervision_refactor --test supervision_test`
- `cargo test -p sandbox event_bus`

### 2) Phase A Foundation: Supervisor Health Monitoring v1
- Added health snapshot message: `ApplicationSupervisorMsg::GetHealth`.
- Added health data:
  - child liveness (`event_bus_healthy`, `session_supervisor_healthy`)
  - supervision event counters (`actor_started`, `actor_failed`, `actor_terminated`)
  - `last_supervision_failure`
- Added integration assertions that health is populated after startup.

Files:
- `sandbox/src/supervisor/mod.rs`
- `sandbox/tests/supervision_test.rs`

## New Progress (Current Pass)

### 3) Phase F Starter: Scoped Chat Payloads (Backward Compatible)
- Added shared payload helpers:
  - `shared_types::chat_user_payload(...)`
  - `shared_types::parse_chat_user_text(...)`
- HTTP chat now accepts optional:
  - `session_id`
  - `thread_id`
- Chat user events now persist scope metadata when provided.
- WebSocket chat now carries default scope IDs and persists them on user message events.
- Runtime parsers updated to support both legacy payload strings and new scoped-object payloads.
- Added scope-aware event retrieval path in EventStore:
  - `GetEventsForActorWithScope { actor_id, session_id, thread_id, since_seq }`
  - API usage in chat messages endpoint when both scope keys are provided.
- Added EventStore test coverage for scoped retrieval filtering.

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/actors/chat.rs`
- `sandbox/src/actors/chat_agent.rs`
- `sandbox/src/actors/event_store.rs`

## Immediate Next Actions

1. Add isolation tests for chat API endpoint using mixed scope traffic.
2. Thread scope fields through chat agent assistant/tool event payloads.
3. Add explicit scope columns (`session_id`, `thread_id`) with indexed queries and migration.
4. Enforce required scope keys at supervisor/API boundaries (reject missing keys on scoped routes).
