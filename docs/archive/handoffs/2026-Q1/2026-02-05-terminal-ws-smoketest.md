# Handoff: Terminal WS Smoke Test Added

**Date:** 2026-02-05  
**Status:** WS terminal output verified + smoke test added

## Summary
- Added a small integration smoke test for the terminal WebSocket to ensure basic input produces output.
- Test starts an Actix test server, connects to `/ws/terminal/{terminal_id}` with `user_id`, sends `echo hi\r`, and asserts output includes `hi`.

## Files Added
- `sandbox/tests/terminal_ws_smoketest.rs`

## Test Run
```bash
cargo test -p sandbox --test terminal_ws_smoketest
```

## Notes
- The smoke test uses carriage return (`\r`) for command submission; plain `\n` may only echo input.
- Output includes ANSI escape sequences; UI should render via a terminal emulator (e.g., xterm.js).

## Next Steps
1. Integrate terminal window into the Dioxus desktop UI.
2. Consider replaying buffered output on connect via `TerminalMsg::GetOutput` (optional UX improvement).
