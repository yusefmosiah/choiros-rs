# Handoff: TerminalActor Implementation Complete

**Date:** 2026-02-05  
**Status:** TerminalActor + WebSocket API working, ready for Desktop integration

---

## What Was Accomplished

### 1. TerminalActor (`sandbox/src/actors/terminal.rs`)
- **Real PTY support** using `portable-pty` crate
- **Bidirectional I/O**: Input channel + output channel with 1000-line buffer
- **Resize support**: Terminal dimensions via PTY master
- **Process lifecycle**: Spawn, monitor exit, cleanup
- **EventStore integration**: Ready for persistence

### 2. WebSocket API (`sandbox/src/api/terminal.rs`)
- **`/ws/terminal/{terminal_id}`** - WebSocket for real-time I/O
- **`GET /api/terminals/{terminal_id}`** - Create terminal
- **`GET /api/terminals/{terminal_id}/info`** - Get terminal info
- **`GET /api/terminals/{terminal_id}/stop`** - Stop terminal

### 3. ActorManager Updates
- Added `terminal_actors` registry with DashMap
- `get_or_create_terminal()` method
- `ActorManager` now derives `Clone`

### 4. Testing Results
- ✅ Terminal creation via HTTP API works
- ✅ WebSocket connection establishes
- ✅ PTY process spawns and runs
- ✅ Agent-browser E2E testing works
- ⚠️ Terminal output streaming needs debugging (PTY running but output not visible in browser)

---

## Current State

**Backend running on port 8080:**
```bash
curl http://localhost:8080/api/terminals/test-terminal-1/info
# Returns: {"terminal_id":"test-terminal-1",...,"is_running":true}
```

**Test page created:** `test_terminal.html`
- Opens WebSocket to backend
- Shows terminal status
- Ready for command input

**Agent-browser E2E working:**
```bash
agent-browser --session test open file:///Users/wiz/choiros-rs/test_terminal.html
agent-browser --session test screenshot /tmp/terminal_test.png
```

---

## Next Steps

### Immediate: Desktop Integration
The terminal is separate from the ChoirOS web desktop. Next task is to:

1. **Start the Dioxus frontend** (`just dev-ui` on port 3000)
2. **Integrate terminal into DesktopActor windows**
3. **Create terminal window type** in the desktop UI
4. **Connect terminal WebSocket** through the desktop interface

### Bootstrap Path
Once integrated:
1. Open ChoirOS Desktop (port 3000)
2. Open terminal window → spawns TerminalActor with PTY
3. Run opencode in the terminal
4. Eventually replace opencode with built-in agentic IDE

---

## Known Issues

1. **Output streaming**: PTY is running but output not appearing in browser
   - Likely WebSocket message formatting issue
   - Check `OutputReceived` message handling in terminal.rs
   - Verify output channel is properly connected

2. **Separate test page**: Terminal works in isolation but not in ChoirOS desktop yet
   - Need to integrate into Dioxus frontend
   - Add terminal as a window type in DesktopActor

---

## Files Modified

- `sandbox/Cargo.toml` - Added portable-pty, tokio-util
- `sandbox/src/actors/terminal.rs` - New TerminalActor
- `sandbox/src/actors/mod.rs` - Export TerminalActor
- `sandbox/src/actor_manager.rs` - Added terminal registry
- `sandbox/src/api/terminal.rs` - New terminal API
- `sandbox/src/api/mod.rs` - Added terminal routes
- `AGENTS.md` - Updated docs (removed actorcode refs)
- `Justfile` - Removed actorcode recipes

---

## Commands

```bash
# Start backend
just dev-sandbox

# Start frontend (next step)
just dev-ui

# Test terminal API
curl http://localhost:8080/api/terminals/test-1

# E2E test with agent-browser
agent-browser --session test open file:///Users/wiz/choiros-rs/test_terminal.html
agent-browser --session test screenshot /tmp/test.png
```

---

## Notes

- TerminalActor uses blocking I/O in `spawn_blocking` tasks
- PTY master handle stored in actor state for resize support
- Output buffered for reconnection (1000 lines)
- WebSocket uses JSON messages with type tags

**Ready for:** Desktop integration and terminal window UI
