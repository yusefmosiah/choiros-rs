# Prompt 04: Writer PROMPT Button + Live Conductor Edit Loop

You are working in `/Users/wiz/choiros-rs`.

## Goal
Add a `PROMPT` workflow to Writer:
- `PROMPT` button appears only when document has unsaved changes.
- On click, send the document’s last saved state + current draft diff + document context to Conductor.
- Conductor performs one or more edit/research/code rounds.
- Edits stream back live into Writer.
- Conductor emits explicit “finished” signal when done.

## Hard Constraints
- Do not use ad-hoc string matching for workflow control.
- Use typed contracts/events for all state transitions.
- Keep backend authoritative for persisted state.
- Do not introduce localStorage state.
- Do not involve Chat in this flow.

## Read First
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/writer.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/ws.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/state.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/writer.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
- `/Users/wiz/choiros-rs/docs/architecture/refactor-checklist-no-adhoc-workflow.md`

## Implement (MVP but complete loop)
1. Define typed request contract for writer prompt action:
   - desktop_id
   - window_id (or doc session id)
   - file_path
   - saved_content
   - draft_content
   - unified_diff (or structured diff)
   - optional user instruction
2. Add backend endpoint (or conductor command path) for this action.
3. Conductor-side execution model:
   - create a `writer_edit_task`
   - optional capability calls (research/terminal/etc) under conductor control
   - produce edit operations/patch chunks
   - stream incremental updates as typed events
   - emit terminal completion event with status = `completed|blocked|failed`
4. Add typed event schema for live writer orchestration:
   - `writer.prompt.started`
   - `writer.prompt.progress`
   - `writer.prompt.patch_applied`
   - `writer.prompt.completed`
   - `writer.prompt.failed`
   Include: `task_id`, `correlation_id`, `file_path`, `revision/base revision`, `phase`, `message`, timestamps.
5. Frontend Writer UX updates:
   - Show `PROMPT` button only when dirty state is true.
   - Clicking button starts task and shows in-progress indicator.
   - Apply incoming patch/progress events live to editor content.
   - Preserve cursor as reasonably as possible during live patch apply.
   - Show final done/failed state on completion signal.
6. Conflict/revision handling:
   - If server revision changed during run, surface typed conflict state and stop auto-apply.
   - Do not silently overwrite without explicit rule.
7. Testing:
   - Backend integration tests for endpoint + event emission + completion signal.
   - Frontend logic test for button visibility (dirty only).
   - Frontend logic test for live patch apply + completion state.
   - HTTP/WebSocket script demonstrating end-to-end run.

## Non-Goals
- Perfect diff algorithm
- Full collaborative editor CRDT
- AuthZ/authN

## Acceptance Criteria
- Unsaved change in Writer => `PROMPT` visible.
- Click `PROMPT` => typed task starts, live updates arrive, content changes in UI.
- Task ends only on typed completion/failure event.
- No workflow decisions based on freeform text matching.
- No chat dependency.
- No localStorage persistence introduced.

## Validation Commands
- `cargo check -p sandbox`
- `cargo test -p sandbox <new_writer_prompt_tests> -- --nocapture`
- `cargo check` (dioxus-desktop)
- Run manual E2E and include exact steps + observed events in final summary.
