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
- Added generic scope wrapper helper:
  - `shared_types::with_scope(...)`
- Threaded scope through `ChatAgentMsg::ProcessMessage` and applied scope to:
  - assistant events (`chat.assistant_msg`)
  - tool call events (`chat.tool_call`)
  - tool result events (`chat.tool_result`)
- Added chat API integration coverage for mixed-thread filtering:
  - `test_get_messages_scope_filter_returns_only_matching_thread`
- Added explicit EventStore scope columns and indexing:
  - columns: `session_id`, `thread_id`
  - index: `idx_events_session_thread(session_id, thread_id)`
  - migration-time backfill from `payload.scope.*`
  - scoped query path prefers columns, with payload fallback for legacy rows
- Added API boundary enforcement for partial scope keys:
  - reject `session_id` without `thread_id` (400)
  - reject `thread_id` without `session_id` (400)
  - added integration tests for both GET and POST chat endpoints
- Added scope-aware websocket tool-stream retrieval:
  - initial event cursor now uses scoped query
  - incremental tool event polling now uses scoped query
  - prevents cross-thread tool event bleed on shared actor streams

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/actors/chat.rs`
- `sandbox/src/actors/chat_agent.rs`
- `sandbox/src/actors/event_store.rs`

## Immediate Next Actions

1. Add supervisor/API metrics for scope-missing and scope-mismatch rejections.
2. Add migration/backfill notes for legacy events without scope metadata.
3. Add scope-aware retrieval path for ChatAgent history preload (avoid cross-thread memory bleed).
4. Add explicit thread/session IDs on non-chat event domains where relevant (desktop/terminal).
