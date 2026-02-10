# Prompt 03: Prompt Bar Routing + Writer Auto-Open Integration

You are working in `/Users/wiz/choiros-rs`.

## Goal
Wire desktop Prompt Bar to conductor endpoint so user can trigger report generation and auto-open Writer in markdown preview mode.

## Hard Constraints
- Keep Prompt Bar as input surface; no tool execution in frontend.
- Backend remains source of truth for app/window state (no localStorage for new state).
- Do not regress Files/Writer current behavior.

## Read First
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/components/prompt_bar.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/shell.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/writer.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop_window.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/files.rs`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`

## Implement
1. Add frontend API client call for `POST /conductor/execute` in `api.rs`.
2. In `prompt_bar.rs`, add command path:
   - send objective + desktop_id to conductor execute endpoint
   - on success, call `open_window` for writer using returned `writer_window_props`
3. Ensure Writer opens target report file and enters preview mode automatically.
4. Keep this feature behind a simple, explicit trigger pattern for now (documented), but with typed response handling.
5. Add UI feedback in prompt bar:
   - running
   - success (opened report)
   - typed error message

## Tests
- Add at least one frontend integration/unit test for response handling logic.
- Add backend+frontend manual verification steps in final summary.

## Validation
- `cargo check` (workspace or relevant crates)
- Run app and verify: prompt -> report file created -> Writer opens with markdown preview
- Confirm no new localStorage persistence added
