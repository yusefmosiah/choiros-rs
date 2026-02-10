# Prompt 03: Prompt Bar -> Conductor -> Writer Auto-Open

You are working in `/Users/wiz/choiros-rs`.

## Mission
Connect desktop Prompt Bar to Conductor backend so this UX works end-to-end:

`Prompt entered -> conductor task executes -> markdown report generated -> Writer opens report in preview mode`

This pass assumes Prompt 02 is landed (Conductor backend exists).

### Current Backend Contract (from Prompt 02)

- `POST /conductor/execute` is actor-backed and usually returns `202 Accepted` with in-flight status (`queued|running|waiting_worker`).
- `GET /conductor/tasks/:task_id` returns `ConductorTaskState` for polling.
- Completion is authoritative when polled task status is `completed` and `report_path` is present.

## Architecture Requirements (Non-Negotiable)

1. Prompt Bar is an **input surface only**.
2. Prompt Bar must not perform tool orchestration.
3. Conductor remains orchestration authority.
4. Use typed API payloads/responses only.
5. Backend-authoritative state: no localStorage for new persistence.
6. No Chat coupling in this lane.

## Read First

- `/Users/wiz/choiros-rs/AGENTS.md`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
- `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/components/prompt_bar.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/shell.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/actions.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop_window.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/writer.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/conductor.rs` (from Prompt 02)

## Implement

### Phase A: Frontend API Client

In `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs` add typed conductor client methods:
- execute request
- task status polling/helper (required)

Do not use untyped JSON blobs in call sites.
Use `shared_types` request/response/task types directly.

### Phase B: Prompt Bar Submission Path

In prompt bar component:
- send `objective + desktop_id + output_mode + worker_plan` to conductor endpoint
- handle typed response state: `queued | running | waiting_worker | completed | failed`
- show explicit UI states:
  - submitting/running
  - success + opened writer
  - failure (typed message)

Use a clear trigger contract for now (e.g. always conductor submit, or explicit prefix); whichever you choose must be documented in code comments and final summary.

For deterministic local success (without external search provider keys), default to a typed terminal worker plan:
- one `terminal` step with `terminal_command` derived from prompt intent.
- keep this typed; no phrase-matching control flow for authority.

### Phase C: Writer Auto-Open

On completion:
- call existing `open_window` path for `writer`
- if `writer_window_props` is present, pass returned props as-is
- if only `report_path` is available (from polled task state), build typed writer props from `report_path` with `preview_mode=true`
- ensure writer opens report path and preview mode is active

Do not bypass backend by trying to persist path locally.

### Phase D: State & Error Semantics

- Keep prompt bar state machine typed and minimal.
- Do not parse free-form backend strings to decide UI control flow.
- If response is `queued|running|waiting_worker` without immediate report path, show in-progress and poll `/conductor/tasks/:task_id` until `completed|failed`.

## Acceptance Criteria

1. Prompt from desktop prompt bar triggers conductor execution path.
2. Successful run opens Writer with generated report in markdown preview mode.
3. Typed errors surface cleanly in prompt bar UI.
4. No new localStorage persistence introduced.
5. No orchestration logic added to frontend beyond request/response handling.

## Validation

- `cargo check` in `/Users/wiz/choiros-rs/dioxus-desktop`
- `cargo check -p sandbox` (if touched shared API contracts)
- Add at least one frontend test for typed response handling/state transition.
- Use scoped test commands (avoid broad filtered runs):
  - `just test-sandbox-itest conductor_api_test`
  - `just test-sandbox-lib conductor -- --nocapture`
- Manual E2E steps in final summary:
  1. enter prompt
  2. verify conductor call
  3. verify writer opened path + preview mode
