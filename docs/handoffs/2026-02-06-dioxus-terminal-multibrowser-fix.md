# Dioxus Terminal Multi-Browser Stability Fix (2026-02-06)

## Summary

Fixed terminal instability in the Dioxus desktop when reloading and when multiple browser tabs/windows attach to the same desktop terminals.

## Root Causes

1. Terminal reconnect scheduling could fire during intentional component teardown, creating unintended reconnect churn.
2. Initial terminal fit/resize could send transient `0x0` dimensions during layout timing races.
3. Terminal sessions are shared by `terminal_id`; a bad resize from one client affected all attached clients.
4. `web_sys` websocket callbacks were mutating Dioxus signals directly (outside active scope), which can panic in Dioxus runtime (`current_scope_id().unwrap()`, `BorrowMutError`).

## Changes

### Frontend (`dioxus-desktop`)

- `/Users/wiz/choiros-rs/dioxus-desktop/src/terminal.rs`
  - Prevent reconnect-on-purposeful-close by marking runtime as `closing` and suppressing close-handler reconnect logic in teardown.
  - Clear pending reconnect timeout when websocket opens successfully.
  - Retry initial terminal fit a few times and only send resize when dimensions are valid (`>= 2x2`).
  - Skip resize sends if websocket is not open.
  - Clear websocket event handlers before closing in `Drop` to avoid callback churn.
  - Track reconnect timeout lifecycle more cleanly by resetting timeout signal when callback executes.

- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/ws.rs`
  - Removed unused `from` field from `WindowRestored` event variant (warning cleanup).
  - Replaced `Closure::forget()` websocket lifecycle with an owned `DesktopWsRuntime` that unhooks handlers and closes websocket on `Drop`.
  - Prevented callback execution after intentional close using a `closing` guard.
  - Websocket callback now enqueues events and schedules a rerender; state mutation happens in Dioxus effects only.

- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/state.rs`
  - Updated `WindowRestored` match arm for the variant change.

- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/shell.rs`
  - Desktop shell now owns websocket runtime state and establishes only one active desktop websocket per mounted shell.
  - This removes leaked stale callbacks that previously triggered Dioxus runtime panics:
    - `called Option::unwrap() on a None value`
    - `already borrowed: BorrowMutError`
  - Added queued WS event drain effect (`WsEvent` queue) so `apply_ws_event` runs inside runtime scope.

- `/Users/wiz/choiros-rs/dioxus-desktop/src/components.rs`
  - Chat websocket callbacks now enqueue `ChatWsEvent` entries; message parsing and signal updates occur in an effect.
  - Added `ChatRuntime` drop cleanup (unhook + close) and intentional-close guard.

- `/Users/wiz/choiros-rs/dioxus-desktop/src/terminal.rs`
  - Terminal websocket callbacks now enqueue `TerminalWsEvent` entries; all signal mutations and reconnect scheduling occur in an effect.
  - Keeps raw JS callbacks side-effect-free regarding Dioxus signals.

### Backend (`sandbox`)

- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
  - Clamp resize dimensions to minimum `2x2` in `TerminalMsg::Resize`.
  - This protects shared terminal sessions from invalid client resize payloads.

## Added Tests

- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
  - `test_multiple_subscribers_receive_terminal_output`
    - Verifies two output subscribers both receive terminal output from the same session.
  - `test_resize_clamps_zero_dimensions`
    - Verifies `0x0` resize requests are clamped and do not poison terminal dimensions.

## Validation Run

- `cargo check -p sandbox`
- `cargo check` (in `/Users/wiz/choiros-rs/dioxus-desktop`)
- `cargo test -p sandbox test_multiple_subscribers_receive_terminal_output -- --nocapture`
- `cargo test -p sandbox test_resize_clamps_zero_dimensions -- --nocapture`
- Browser smoke checks with `agent-browser`:
  - open desktop
  - reload multiple times
  - open additional tab/window
  - verify terminal inputs remain present and backend connection counts remain stable

## Notes

- The remaining warning is unrelated to this bug: `ts-rs` parsing `serde(transparent)` attribute.
- Window dragging issue remains separate and is next to fix.
