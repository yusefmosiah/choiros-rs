# Roadmap Progress

Date: 2026-02-07
Source roadmap:
- `docs/architecture/roadmap-dependency-tree.md`
- `docs/architecture/roadmap-critical-analysis.md`

## Critical Path Status

| Phase | Status | Notes |
|---|---|---|
| B. Multiagent Control Plane v1 | In progress | Control-plane contract + async terminal delegation API implemented; terminal worker streaming still pending |
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

### 4) Phase B Kickoff: Control Plane Contract + Async Delegation API
- Added delegated task/result contracts in shared types:
  - `DelegatedTaskKind`
  - `DelegatedTask`
  - `DelegatedTaskStatus`
  - `DelegatedTaskResult`
- Added worker task topic constants:
  - `worker.task.started`
  - `worker.task.progress`
  - `worker.task.completed`
  - `worker.task.failed`
- Added `ApplicationSupervisorMsg::DelegateTerminalTask`:
  - returns immediate acceptance (`DelegatedTask`)
  - creates `task_id` + `correlation_id`
  - publishes lifecycle events via EventBus
  - executes terminal dispatch in background (`tokio::spawn`)
- Added app-state entrypoint for delegation:
  - `AppState::delegate_terminal_task(...)`
- Added integration test:
  - `test_application_supervisor_accepts_async_terminal_delegation`

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/app_state.rs`
- `sandbox/tests/supervision_test.rs`

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
- Added scope-aware ChatAgent identity + preload:
  - Chat agent key now includes session/thread when available
  - ChatAgent preload fetch uses scoped EventStore query when scope is present
  - prevents in-memory conversation history bleed across chat app instances

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
3. Add explicit thread/session IDs on non-chat event domains where relevant (desktop/terminal).
4. Add explicit scope assertions in websocket integration tests (multi-instance same actor_id).

## Phase B Implementation Checklist

### Step 1: Control-Plane Contract
- [x] Add delegated task envelope with:
  - [x] `task_id`
  - [x] `correlation_id`
  - [x] `actor_id`
  - [x] `session_id`
  - [x] `thread_id`
  - [x] `kind`
  - [x] `payload`
- [x] Add delegated task result envelope with:
  - [x] `status` (`accepted|running|completed|failed`)
  - [x] `output`
  - [x] `error`
  - [x] timestamps
- [x] Define event topic conventions:
  - [x] `worker.task.started`
  - [x] `worker.task.progress`
  - [x] `worker.task.completed`
  - [x] `worker.task.failed`

### Step 2: Supervisor Delegation API (`run_async` style)
- [x] Add non-blocking supervisor API for terminal delegation.
- [x] Supervisor generates `task_id` + `correlation_id`.
- [x] API returns immediate acceptance response (does not block on execution completion).
- [x] Background execution path publishes lifecycle events through EventBus.

### Step 3: Terminal Worker Path (Next)
- [x] Route delegated terminal command through TerminalActor session.
- [x] Emit streamed progress/output events.
- [x] Add timeout and cancellation behavior.

### Step 4: ChatAgent Routing Integration (Next)
- [x] Replace direct terminal tool execution path with delegation API.
- [x] Keep fallback for non-terminal tools during transition.

### Step 5: UI Actor-Call Timeline (Next)
- [x] Subscribe to worker lifecycle topics in websocket/UI.
- [ ] Render live actor-call state and output.

### Step 6: Phase B Gate Test (Next)
- [x] Add integration test: supervisor delegation -> terminal execution -> persisted trace.

## Phase B Progress Update (Steps 3-5)

- Step 3 completed:
  - delegated terminal tasks now route via TerminalActor sessions
  - supervisor publishes `worker.task.progress` updates with output chunks
  - timeout path sends terminal interrupt and marks task failure on timeout
  - TerminalActor now exposes an agentic execution harness (`RunAgenticTask`) that can plan multi-step command sequences and synthesize a summary
- Step 4 completed:
  - ChatAgent now routes `bash` tool calls to supervisor terminal delegation
  - ChatAgent now awaits delegated task completion and returns terminal output/error as tool result
  - direct `bash` execution is blocked; `bash` runs only via TerminalActor delegation
- Step 5 completed partially:
  - websocket stream emits `actor_call` chunks from `worker_*` events
  - terminal delegation completion payload now carries transparency fields:
    - `reasoning`
    - `executed_commands`
    - `steps` (command, exit_code, output_excerpt)
  - frontend rendering of a dedicated actor-call timeline is still pending
- Step 6 completed:
  - added supervision integration gate test for delegated terminal trace persistence with correlation ID continuity

## Code Review Outcome (2026-02-07)

Findings addressed in this pass:
- Fixed terminal-agent failure semantics:
  - non-zero terminal command exits now emit `worker_failed` (not `worker_complete`)
  - completion payload still includes transparency fields (`reasoning`, `executed_commands`, `steps`)
- Added regression test coverage:
  - `test_terminal_delegation_nonzero_exit_marks_failed`

Residual risks:
- Terminal agent client registry is currently Bedrock-only in `TerminalActor`; model-selection parity with `ChatAgent` is pending.
- UI currently receives transparency payloads but does not yet render dedicated step/timeline views.

Validation rerun:
- `cargo check -p sandbox`
- `cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test chat_api_test`
