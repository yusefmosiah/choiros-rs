# Terminal Multi-Browser + Drag Follow-up (2026-02-06)

## Summary
This patch addresses two production-facing issues observed during the Dioxus rollback:

1. Terminal sessions becoming unreliable across reloads/multiple browser windows.
2. Window drag interactions getting stuck until `Escape`.

It also closes a browser-side CPU leak introduced by long-lived websocket event pump tasks that were never terminated on component unmount.

## Root Causes

### 1) Terminal actor creation race
`ActorManager::get_or_create_terminal` had a check-then-spawn race. Concurrent websocket connects for the same `terminal_id` could spawn duplicate PTY-backed actors before the registry insert completed.

Impact:
- Duplicate PTYs and extra file descriptors.
- Session split behavior across clients (different clients attached to different actors).

### 2) Frontend terminal reconnect dead-end
`TerminalView` reconnect logic depended on resetting `runtime` to trigger the init effect. In failure paths where `runtime` was already `None`, reconnect could stall.

### 3) Frontend terminal init race
Terminal init attempted to grab the container element once and returned silently if not found, leaving status at `Connecting...`.

### 4) Event-pump lifecycle leak
Websocket event pump loops in chat/desktop/terminal ran forever (`loop + TimeoutFuture`) and were not cancelled on unmount/reload.

Impact:
- Browser CPU growth over time.
- Increased callback/event churn after reloads.

### 5) Sticky drag state
Dragging relied on `pointerup`/`pointercancel` only. Under capture loss/focus transitions, pointer-up could be missed and interaction stayed active.

## Fixes Applied

### Backend
- Added serialized terminal slow-path creation lock in actor manager:
  - `sandbox/src/actor_manager.rs`
- Added terminal websocket startup hardening:
  - `sandbox/src/api/terminal.rs`
  - Retry once when an actor call fails, removing stale registry entries.
  - Added connection/session logs for terminal websocket attach/detach.

### Frontend (Dioxus)
- Terminal reconnect hardening:
  - `dioxus-desktop/src/terminal.rs`
  - Added reconnect nonce trigger so retries occur even when `runtime` is already `None`.
  - Added bounded wait for terminal container element before initialization.
- Event pump lifecycle safety:
  - `dioxus-desktop/src/components.rs`
  - `dioxus-desktop/src/desktop/shell.rs`
  - `dioxus-desktop/src/terminal.rs`
  - Added unmount flags to stop pump loops.
- Drag interaction robustness:
  - `dioxus-desktop/src/desktop_window.rs`
  - End interaction when pointer buttons are no longer held during move.
  - Switched pointer-capture release target lookup to event target.
- Added local ignore file for Dioxus artifacts:
  - `dioxus-desktop/.gitignore`

## Test Upgrades

### Added integration coverage
- `sandbox/tests/terminal_ws_smoketest.rs`
  - `test_terminal_ws_two_clients_share_terminal_output`
  - `test_terminal_ws_reconnect_keeps_terminal_available`

### Existing tests still passing
- Existing smoke test retained:
  - `test_terminal_ws_smoke`

## Commands Run

- `cargo fmt --all`
- `cargo check` (in `dioxus-desktop`)
- `cargo check -p sandbox`
- `cargo test -p sandbox --test terminal_ws_smoketest`
- `cargo test --lib` (in `dioxus-desktop`)
- `cargo test -p sandbox terminal -- --nocapture`

## Known Remaining Item

- A pre-existing warning remains from `ts-rs` parsing `#[serde(transparent)]` attributes. This patch does not change that behavior.

