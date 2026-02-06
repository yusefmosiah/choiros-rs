# React Terminal CPU Regression (Reload + Multi-Window)

## Summary

This document captures a high-severity React desktop regression where browser CPU usage spiked and desktop boot could appear stuck on `Loading desktop...`, especially after reloads or when multiple browser windows/tabs were open.

## Observed Behavior

- Browser renderer CPU spiked aggressively when terminal windows were present.
- The issue worsened with multiple open desktop sessions.
- UI could remain on `Loading desktop...` when WebSocket startup failed or was delayed.
- Backend logs could show secondary pressure symptoms, but the primary runaway was browser-side.

## Root Cause

### 1) Terminal resize feedback loop risk

`/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/Terminal.tsx`

- `ResizeObserver` events triggered terminal resize work.
- Resize work called `fit()` and sent resize messages.
- Under repeated observer callbacks, this created excessive render/resize churn.

### 2) Loading-state deadlock on failed desktop WS startup

`/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Desktop.tsx`

- Bootstrap intentionally waited for WS `connected`.
- If WS never reached `connected`, loading could stay true indefinitely.

## Fixes Implemented

### Terminal loop protection

`/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/Terminal.tsx`

- Added `requestAnimationFrame` scheduling for resize work.
- Added in-flight guard to avoid re-entrant resize execution.
- Added container-size change detection (ignore unchanged observer events).
- Added terminal rows/cols dedupe (skip duplicate resize WS sends).
- Added cleanup of pending animation-frame resize work on unmount.
- Disabled xterm cursor blinking to reduce steady-state repaint overhead with many terminals.

### Desktop loading fallback

`/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Desktop.tsx`

- Added an 8-second connection timeout fallback.
- If desktop WS is still not connected, loading is cleared and an error is shown.

### Rust warning cleanup (terminal actor)

`/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`

- Removed unused `env_vars` and `event_store` fields from `TerminalState`.
- This removes the PTY/terminal-actor dead-code warning introduced by unused fields.

## Test Coverage Added/Updated

`/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/Terminal.test.tsx`

- `does not spam resize messages when size is unchanged`
- `sends a new resize message when terminal dimensions change`

## Validation Run

- `npm test -- src/components/apps/Terminal/Terminal.test.tsx` (pass)
- `npm run build` in `sandbox-ui` (pass)
- `cargo test -p sandbox test_stop_terminates_terminal_process -- --nocapture` (pass)
- `cargo test -p sandbox test_repeated_start_stop_cleans_up_each_process -- --nocapture` (pass)

## Remaining Notes

- `ts-rs` still reports a separate existing warning about parsing a serde `transparent` attribute.
- The repository still has unrelated test-file warnings in `sandbox/tests/*` not introduced by this fix.
