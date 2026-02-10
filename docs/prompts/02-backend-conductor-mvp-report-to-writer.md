# Prompt 02: Backend Conductor MVP for Report-to-Writer Flow

You are working in `/Users/wiz/choiros-rs`.

## Goal
Implement backend MVP path:
Prompt intent -> Conductor orchestration -> capability actor calls -> markdown report file written -> response includes writer-open instructions.

## Hard Constraints
- Do not break existing Chat flow.
- No ad hoc string-matching workflow logic.
- Use typed request/response payloads.
- Keep scope minimal and testable.

## Read First
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/desktop.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/files.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/writer.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs`
- `/Users/wiz/choiros-rs/docs/architecture/refactor-checklist-no-adhoc-workflow.md`

## Implement
1. Add a minimal Conductor API endpoint (new module under `sandbox/src/api/`, wired in `api/mod.rs`), e.g. `POST /conductor/execute`.
2. Define typed payload:
   - objective
   - desktop_id
   - output_mode (for now only: `markdown_report_to_writer`)
   - optional hints
3. Implement conductor execution service/actor (minimal):
   - invokes existing capability paths (researcher + terminal as needed)
   - composes a markdown report string
   - writes report into sandbox path (e.g. `sandbox/reports/<timestamp>-report.md`) via backend file path logic
4. Return typed result:
   - status
   - report_path
   - writer_window_props (`path` + `preview_mode=true`)
   - trace/correlation ids
5. Add integration tests:
   - success case writes markdown file and returns report_path
   - path stays inside sandbox
   - failure returns typed error (no plain-text-only control)

## Non-Goals
- Full multi-step planner intelligence
- Auth layer
- Chat refactor

## Validation
- `cargo check -p sandbox`
- targeted tests for new conductor module and endpoint
- one HTTP script under `/Users/wiz/choiros-rs/scripts/http/` to call endpoint and verify file created
