# Handoff: R2 Window Management Premerge

**Date:** 2026-02-06  
**Branch/Worktree:** `codex/e99c` (`/Users/wiz/.codex/worktrees/e99c/choiros-rs`)  
**Scope owner:** R2 window management only (backend + frontend contracts/interactions)

## Summary
Implemented R2 window lifecycle and interaction contracts end-to-end for:
- minimize / maximize / restore
- focus and z-index consistency
- drag/resize pointer lifecycle with throttled backend updates + final commit
- API + websocket delta wiring for desktop mutations
- baseline keyboard/a11y controls in floating windows

No unrelated features were intentionally changed.

## Files Changed
- `sandbox/src/actors/desktop.rs`
- `sandbox/src/api/desktop.rs`
- `sandbox/src/api/mod.rs`
- `sandbox/src/api/websocket.rs`
- `sandbox/tests/desktop_api_test.rs`
- `sandbox/tests/desktop_ws_test.rs` (new)
- `sandbox/Cargo.toml`
- `sandbox-ui/src/api.rs`
- `sandbox-ui/src/desktop.rs`
- `sandbox-ui/src/desktop_window.rs`

## Backend Contract Changes
### New DesktopActor operations
Added message variants and handlers:
- `MinimizeWindow`
- `MaximizeWindow`
- `RestoreWindow`

Added event constant:
- `desktop.window_restored`

Added restore result metadata:
- `RestoreResult { window, from }` where `from` is `"minimized"` or `"maximized"`.

### Invariant/guard behavior now enforced
- Cannot move/resize maximized window.
- Resize enforces minimum size (`200x160`).
- Cannot focus minimized window (restore first).
- Active window reassignment prefers top-most non-minimized window.
- Minimized and maximized are mutually exclusive in handlers/projection.

### New HTTP endpoints
- `POST /desktop/{desktop_id}/windows/{window_id}/minimize`
- `POST /desktop/{desktop_id}/windows/{window_id}/maximize`
- `POST /desktop/{desktop_id}/windows/{window_id}/restore`

### Existing endpoint behavior update
- `PATCH /desktop/{desktop_id}/windows/{window_id}/size` now rejects invalid bounds (< `200x160`) with `400`.

## WebSocket Contract Changes
Added outbound message types in desktop websocket protocol:
- `window_minimized { window_id }`
- `window_maximized { window_id, x, y, width, height }`
- `window_restored { window_id, x, y, width, height, from }`

Also wired mutation routes to broadcast deltas using `broadcast_event(...)`.

`window_resized` payload is now signed integer dimensions (`i32`) on server/client wiring.

## Frontend Behavior Changes
### API client
Added calls:
- `minimize_window(...)`
- `maximize_window(...)`
- `restore_window(...)`

### Desktop state reconciliation
- Parses/handles new websocket events (`window_minimized|maximized|restored`).
- Keeps `active_window` consistent with minimize/focus/restore/maximize deltas.
- Minimized windows are excluded from canvas render but remain in running-app strip.

### Floating window interactions
Implemented pointer-driven drag/resize lifecycle:
- `pointerdown` start
- `pointermove` local frame updates
- 50ms throttled network writes with coalescing
- final commit on `pointerup`
- revert to committed bounds on `pointercancel`

### Window controls + keyboard/a11y baseline
- Buttons: Minimize / Maximize-or-Restore / Close
- A11y attributes: `role="dialog"`, `aria-label`, control `aria-label`s, titlebar keyboard focus
- Keyboard shortcuts:
  - `Alt+F4` close
  - `Ctrl+M` minimize
  - `Ctrl+Shift+M` maximize/restore toggle
  - `Esc` cancel active drag/resize
  - `Alt+Arrow` move by 10px
  - `Alt+Shift+Arrow` resize by 10px

## Tests Added/Updated
### Backend actor tests (`sandbox/src/actors/desktop.rs`)
Added coverage for:
- minimize active window active-reassignment
- maximize/restore round trip
- restore from minimized
- invalid maximize transition on minimized window
- resize minimum-size enforcement

### Backend API integration (`sandbox/tests/desktop_api_test.rs`)
Added coverage for:
- minimize/maximize/restore endpoint flow
- unknown window id on new endpoints
- invalid resize bounds rejection

### Backend websocket integration (`sandbox/tests/desktop_ws_test.rs`)
Added tests for:
- subscription + mutation emits expected deltas
- ordered minimize -> restore -> maximize delta sequence

### Frontend
Added unit test:
- `desktop_window::tests::clamp_respects_minimums`

## Validation Executed
Ran from `/Users/wiz/.codex/worktrees/e99c/choiros-rs`:

1. `cargo fmt`  
Status: pass

2. `cargo test -p sandbox --lib -- --nocapture`  
Status: pass (49 passed)

3. `cargo test -p sandbox --test desktop_api_test -- --nocapture`  
Status: pass (20 passed)

4. `cargo test -p sandbox --test desktop_ws_test -- --nocapture`  
Status: pass (2 passed)

5. `cargo test -p sandbox-ui --lib -- --nocapture`  
Status: pass (1 passed)

6. `cargo check -p sandbox-ui`  
Status: pass

## Premerge Checklist For Merge Agent
1. Rebase/cherry-pick this worktree changes.
2. Re-run these targeted commands exactly:
   - `cargo fmt --check`
   - `cargo test -p sandbox --test desktop_api_test`
   - `cargo test -p sandbox --test desktop_ws_test`
   - `cargo test -p sandbox --lib actors::desktop`
   - `cargo check -p sandbox-ui`
3. Verify no contract regressions in downstream consumers of websocket `window_resized` (signed ints now used in delta payload).
4. Verify route table includes new minimize/maximize/restore endpoints.
5. Verify no conflicts in concurrent edits to:
   - `sandbox/src/api/desktop.rs`
   - `sandbox-ui/src/desktop.rs`
   - `sandbox-ui/src/desktop_window.rs`

## Compatibility / Migration Notes
- Additive REST endpoints; existing clients continue to function.
- Websocket adds new message types and updates `window_resized` numeric typing alignment.
- Restore API responses include `from` in addition to `window`.
- Dev dependency update: `sandbox/Cargo.toml` enables `reqwest` `json` feature for websocket integration tests.

## Known Non-Blocking Warnings
Current workspace emits unrelated `dead_code`/`unused_imports` warnings in other modules during tests; no new blocking warnings introduced for R2 behavior.

## Out-of-Scope Confirmed
- No tiling/snap/docking
- No cross-desktop state work
- No broad UI redesign beyond required controls/interactions
