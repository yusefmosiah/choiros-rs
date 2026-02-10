# Prompt 04: Writer PROMPT Button (Saved+Diff -> Conductor -> Live Multi-Round Edits)

You are working in `/Users/wiz/choiros-rs`.

## Mission
Implement Writer-side AI edit loop with Conductor orchestration:

- `PROMPT` button appears only when document is dirty.
- Clicking `PROMPT` sends:
  - last saved state
  - current draft
  - structured diff
  - file + revision context
  to Conductor.
- Conductor may run multi-round capability workflows (research/code/terminal/etc).
- Writer receives live patch/progress updates.
- Conductor emits explicit finished signal.

## Architecture Requirements (Non-Negotiable)

1. Conductor is orchestration authority.
2. Writer is a scoped app surface; no embedded orchestration engine.
3. Control flow uses typed states/events only.
4. No string matching for workflow transitions.
5. No Chat dependency.
6. Backend-authoritative state model remains intact (no localStorage persistence).

## Read First

- `/Users/wiz/choiros-rs/AGENTS.md`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
- `/Users/wiz/choiros-rs/docs/architecture/refactor-checklist-no-adhoc-workflow.md`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/writer.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/ws.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/state.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/writer.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/conductor.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/` (from Prompt 02)
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs`

## Implement

### Phase A: Typed Request Contract

Add typed request for writer prompt action:

- `desktop_id`
- `window_id`
- `file_path`
- `base_revision` (saved revision)
- `saved_content`
- `draft_content`
- `diff` (unified or structured)
- `user_instruction` (optional)

Define explicit `task_type`, e.g. `writer_edit`.

### Phase B: Conductor Task Path

Conductor must:
- create `writer_edit_task`
- choose capability path(s) as needed
- emit typed progress
- emit patch chunks/operations
- emit terminal completion (`completed | blocked | failed`)

Do not perform orchestration in writer API handler.

### Phase C: Live Event Contract

Add typed event family (or typed websocket payload variants):

- `writer.prompt.started`
- `writer.prompt.progress`
- `writer.prompt.patch`
- `writer.prompt.completed`
- `writer.prompt.failed`

Each event includes:
- `task_id`
- `correlation_id`
- `file_path`
- `base_revision`
- `current_revision` (if available)
- `phase`
- `message`
- timestamps

For patch payload, prefer explicit shape:
- operation list (`replace/insert/delete`) with ranges/text,
- or validated unified diff chunk + apply status.

### Phase D: Writer UX

In Writer frontend:
1. Show `PROMPT` button only when dirty.
2. On click:
   - compute diff against last saved content
   - submit typed request
   - enter `PromptRunning` UI state
3. Apply live patch events to editor content.
4. Preserve cursor/selection as best effort.
5. Exit run state only on typed completed/failed event.
6. Surface clear final status and keep document coherent.

### Phase E: Revision/Conflict Safety

- If revision changes unexpectedly during a run, transition to typed conflict state.
- Stop auto-apply on conflict unless explicit safe merge rule exists.
- Do not silently overwrite server state.

## Explicit Non-Goals

- perfect diff/merge algorithm
- CRDT real-time collaboration
- auth/authz hardening

## Acceptance Criteria

1. Dirty writer doc shows `PROMPT`; clean doc hides it.
2. Click `PROMPT` starts conductor writer_edit task with saved+diff payload.
3. Multiple live updates can be applied before completion.
4. Task ends only on typed completion/failure signal.
5. No string-matching workflow control introduced.
6. No localStorage persistence added.

## Validation

- `cargo check -p sandbox`
- `cargo check` in `/Users/wiz/choiros-rs/dioxus-desktop`
- Add backend tests:
  - writer prompt submit route
  - event emission sequence
  - completion/failure transitions
- Add frontend tests:
  - `PROMPT` visibility by dirty state
  - live patch apply state transitions
- Add one HTTP + websocket script demonstrating end-to-end run.

In final summary include:
- event sequence observed
- conflict behavior observed
- files changed + tests run
