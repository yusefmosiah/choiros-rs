# Terminal FD Leak Fix (2026-02-06)

## Summary

Fixed a terminal lifecycle bug that leaked PTY file descriptors and child shell processes. Under repeated terminal open/close cycles, the backend eventually hit `Too many open files (os error 24)`, which blocked new HTTP/WebSocket accepts and left the UI stuck on `Loading desktop...`.

## Impact

- Backend: listener accept loop failed with `os error 24`.
- Frontend: desktop WebSocket never reached connected state; bootstrap stayed in loading.
- Resource behavior: `sandbox` accumulated `/dev/ptmx` descriptors and orphaned shell processes.

## Root Cause

`TerminalActor::Stop` only cleared in-memory handles and flags. It did **not** terminate the spawned PTY child process.

Because `spawn_pty` detached blocking reader/writer/wait tasks, each started terminal could leave a live child process and PTY descriptors behind after stop/reconnect cycles.

## Fix Implemented

### 1. Explicit child-process termination in `TerminalActor`

File: `sandbox/src/actors/terminal.rs`

- Added `child_killer` to actor state (`Box<dyn ChildKiller + Send + Sync>`).
- `spawn_pty` now clones/stores a child killer and process id before moving child into wait task.
- `TerminalMsg::Stop` now:
  - calls `child_killer.kill()`,
  - clears PTY/input/output/process metadata.
- `post_stop` also performs best-effort child kill for safety.
- Added `process_id` to `TerminalInfo` for observability and testability.

### 2. Terminal actor eviction on stop

Files:
- `sandbox/src/actor_manager.rs`
- `sandbox/src/api/terminal.rs`

- Added `ActorManager::remove_terminal`.
- `stop_terminal` API now:
  - sends `TerminalMsg::Stop`,
  - stops the actor,
  - removes it from the terminal registry.

This avoids stale cached terminal actors after explicit stop.

## Test Upgrade

Added Unix regression tests in `sandbox/src/actors/terminal.rs`:

- `test_stop_terminates_terminal_process`
  - starts terminal,
  - captures child PID from `TerminalInfo`,
  - stops terminal,
  - asserts process exits within timeout.

- `test_repeated_start_stop_cleans_up_each_process`
  - repeats start/stop 5x,
  - validates each child PID is gone after stop.

## Verification

### Automated tests

- `cargo test -p sandbox terminal::tests::test_stop_terminates_terminal_process -- --nocapture`
- `cargo test -p sandbox terminal::tests::test_repeated_start_stop_cleans_up_each_process -- --nocapture`
- `cargo test -p sandbox --test terminal_ws_smoketest -- --nocapture`

All passed.

### Runtime stress check (dev config)

Using local dev DB setup from `Justfile` (`DATABASE_URL=./data/events.db`), ran 80 terminal create/ws/stop cycles against one terminal id.

- Before fix (baseline repro): FD count climbed (`16 -> 96`) and `/dev/ptmx` entries accumulated.
- After fix: FD count stabilized (`16`), no `/dev/ptmx` accumulation after cycles.

## Notes

- This addresses the high-severity descriptor/process leak path in terminal lifecycle.
- Desktop loading hang was a downstream symptom once the server hit FD limits.

## Multi-Window Follow-Up (same day)

Observed additional behavior with multiple browser windows connected to the same desktop:

- Terminal UI cleanup previously called `stopTerminal(terminalId)` on component unmount.
- With shared desktop state across browser windows, one window unmounting could stop another window's active terminal session.

### Follow-up fix

- Removed terminal stop-on-unmount from frontend terminal component:
  - `dioxus-desktop/src/components/apps/Terminal/Terminal.tsx`
- Added backend cleanup on explicit desktop window close:
  - `sandbox/src/api/desktop.rs`
  - If a terminal actor exists for the closed `window_id`, stop + evict it.

This makes terminal lifetime window-scoped instead of browser-component-scoped, which is safer for multi-window clients.
