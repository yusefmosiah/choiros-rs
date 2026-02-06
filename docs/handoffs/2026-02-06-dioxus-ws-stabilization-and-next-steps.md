# Dioxus WS Stabilization Handoff (2026-02-06)

## Current Status

- Desktop/chat no longer crash with Dioxus runtime panics.
- Chat is now connected and rendering server responses in the UI again.
- Remaining issues:
  - Terminal windows can remain stuck in `Connecting...` and/or fail to stream I/O.
  - Window dragging behavior is incorrect (drag state can get stuck; release semantics are wrong).

## What Was Fixed

- Replaced direct signal mutation from raw `web_sys` websocket callbacks with queued event processing in component scope.
- Added websocket runtime lifecycle cleanup to avoid stale callback execution after unmount.
- Hardened terminal resize path against invalid dimensions.

## Files Changed (Relevant)

- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/ws.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/effects.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop/shell.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/terminal.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
- `/Users/wiz/choiros-rs/docs/handoffs/2026-02-06-dioxus-terminal-multibrowser-fix.md`

## Validation Completed

- `cargo check` in `/Users/wiz/choiros-rs/dioxus-desktop` passes.
- `cargo check -p sandbox` passes.
- User-confirmed: chat is connected and responses appear in UI.

## Open Issue 1: Terminal Connection Reliability

### Symptom
- Terminal windows can stay at `Connecting...` and not become interactive.

### Likely Investigation Targets

- `/Users/wiz/choiros-rs/dioxus-desktop/src/terminal.rs`
  - Event pump timing and queue drain behavior.
  - WS open/info/output sequencing.
  - Reconnect scheduling interactions with runtime reset.
- `/Users/wiz/choiros-rs/sandbox/src/api/terminal.rs`
  - Websocket startup sequence (`Start`, `SubscribeOutput`, initial buffer push, `Info` send).
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
  - Subscriber behavior and transitions around process start/stop.

### Recommended Next Steps

1. Add focused frontend logging around terminal WS event queue enqueue/dequeue and status transitions.
2. Add backend logs for terminal websocket session lifecycle (connect/start/subscribed/info/first output/disconnect).
3. Verify behavior across:
   - fresh load
   - page reload
   - second browser/tab attaching to same terminal id
4. If needed, add a watchdog timeout in terminal UI to trigger controlled reconnect when no `info`/`output` arrives after open.

## Open Issue 2: Drag Behavior

### Symptom
- Drag starts on title bar click/touch but can remain active after release.
- Expected behavior: drag only while mouse button/finger is actively held.

### Likely Investigation Targets

- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop_window.rs`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/interop.rs`

### Recommended Next Steps

1. Move drag state transitions to pointer lifecycle (`pointerdown` -> active, `pointermove` while active, `pointerup`/`pointercancel` -> inactive).
2. Ensure release handlers are bound at document/window level to catch releases outside title bar/window bounds.
3. Ensure touch and mouse share the same state machine (pointer events preferred).
4. Add a small interaction test script and manual checklist for:
   - click without move
   - drag then release
   - drag outside window then release
   - touch drag on mobile viewport

## Dev Commands

- Backend: `just dev-sandbox`
- Dioxus UI: `just dev-ui`

