# Prompt 02: Backend Conductor MVP (Report -> Writer Path)

You are working in `/Users/wiz/choiros-rs`.

## Mission
Implement a real **ConductorActor-backed** backend MVP for this path:

`Prompt intent -> ConductorActor orchestration -> capability worker(s) -> markdown report file -> typed response for Writer open`

This pass is backend-only. Do not wire Prompt Bar yet.

## Architecture Requirements (Non-Negotiable)

1. **Conductor must be an actor loop**, not API-layer orchestration.
2. **API layer submits work to ConductorActor** and returns typed results/status.
3. **Control plane is typed** (messages, enums, status transitions).
4. **Worker reasoning input can be natural-language objective payloads**, but transitions must remain typed.
5. **No ad hoc workflow** (no string matching for lifecycle control authority).
6. **No Chat dependency** for orchestration in this path.

## Required Module Layout

Use a folder module (not a single large file):

- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/actor.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/state.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/router.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/protocol.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/events.rs`

Wire export in:
- `/Users/wiz/choiros-rs/sandbox/src/actors/mod.rs`

## Read First

- `/Users/wiz/choiros-rs/AGENTS.md`
- `/Users/wiz/choiros-rs/docs/architecture/actor-network-orientation.md`
- `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
- `/Users/wiz/choiros-rs/docs/architecture/refactor-checklist-no-adhoc-workflow.md`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/desktop.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/files.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/writer.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs`

## Implement

### Phase A: Typed Conductor Protocol

Add/extend typed contracts (prefer shared-types for cross-layer payloads):

- `ConductorExecuteRequest`:
  - `objective: String`
  - `desktop_id: String`
  - `output_mode: enum` (`markdown_report_to_writer` only for now)
  - `hints: Option<...>`

- `ConductorTaskStatus` enum:
  - `queued | running | waiting_worker | completed | failed`

- `ConductorExecuteResponse`:
  - `task_id`
  - `status`
  - `report_path: Option<String>`
  - `writer_window_props: Option<serde_json::Value>`
  - `correlation_id`
  - `error: Option<TypedError>`

### Phase B: ConductorActor Loop

Implement ConductorActor message handling with typed transitions:

- Submit objective
- Route to worker path (MVP may use Researcher only or Researcher + Terminal fallback)
- Collect worker output
- Build markdown report
- Write report under sandbox-safe path (e.g. `/Users/wiz/choiros-rs/sandbox/reports/...`)
- Emit final typed result payload for writer open (`path`, `preview_mode=true`)

The report-write must reuse existing sandbox boundary rules (no traversal escapes).

### Phase C: API Surface

Add new API module, e.g.:
- `/Users/wiz/choiros-rs/sandbox/src/api/conductor.rs`

Add endpoints:
- `POST /conductor/execute` (submit and return typed response)
- optional `GET /conductor/tasks/:task_id` (status polling)

Constraint:
- API must not embed orchestration logic; only validation + actor submission + typed response mapping.

### Phase D: Observability Events

Persist typed event family:

- `conductor.task.started`
- `conductor.task.progress`
- `conductor.worker.call`
- `conductor.worker.result`
- `conductor.task.completed`
- `conductor.task.failed`

Each event must include:
- `task_id`
- `correlation_id`
- `status/phase`
- actor/scope metadata already used in this codebase

## Explicit Non-Goals

- Prompt Bar UI integration
- Writer PROMPT-button workflow
- Chat escalation/refactor
- auth/authz hardening
- full planner intelligence

## Acceptance Criteria

1. Conductor exists as actor module folder and is wired in startup/runtime.
2. `/conductor/execute` flows through ConductorActor, not API orchestration.
3. Successful task produces sandbox report file and typed writer-open props.
4. Failure paths are typed and observable.
5. No control-state transitions depend on phrase matching.

## Validation

- `cargo check -p sandbox`
- Targeted conductor actor tests (unit and/or integration)
- Endpoint integration tests under `/Users/wiz/choiros-rs/sandbox/tests/`
- One HTTP script in `/Users/wiz/choiros-rs/scripts/http/` proving end-to-end:
  - submit objective
  - receive typed response
  - verify report file exists and is inside sandbox

In final summary include:
- files changed
- state machine implemented
- event names emitted
- exact commands run
