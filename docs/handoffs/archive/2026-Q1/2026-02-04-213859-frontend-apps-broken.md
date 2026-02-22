# Handoff: Frontend apps broken (chat WS + terminal focus)

## Session Metadata
- Created: 2026-02-04 21:38:59
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~1h

### Recent Commits (for context)
  - e2f23af Add terminal UI integration and E2E smoke
  - 4ec9d58 terminal ui doc
  - f1ce9c9 Add terminal WS smoke test
  - eb170e6 remove actorcode and progress with terminalactor
  - c422c75 complete actix to ractor migration

## Handoff Chain

- **Continues from**: [2026-02-01-183056-docs-coherence-critique.md](./2026-02-01-183056-docs-coherence-critique.md)
  - Previous title: Docs Coherence Critique Attempt
- **Supersedes**: None

> Review the previous handoff for full context before filling this one.

## Current State Summary

User reported that desktop icons weren’t launching apps, terminal showed no output/input, and chat accepted input but never responded. Desktop icon click and window-canvas layering were fixed earlier to allow windows to open. In this session, the chat UI was wired to the `/ws/chat/{actor_id}/{user_id}` WebSocket for streaming responses (with HTTP polling fallback), and terminal now auto-focuses and surfaces backend error/status info. No backend changes yet; runtime behavior still depends on backend readiness (BAML/Bedrock for chat, terminal WS availability).

## Codebase Understanding

### Architecture Overview

- Frontend is Dioxus (`dioxus-desktop/src`), with chat in `components.rs` and terminal in `terminal.rs`.
- Terminal UI uses xterm.js + fit addon from `dioxus-desktop/public` and connects to `/ws/terminal/{terminal_id}`.
- Chat has HTTP endpoints (`/chat/send`, `/chat/{actor_id}/messages`) and a streaming WebSocket (`/ws/chat/{actor_id}/{user_id}`).
- ChatAgent uses BAML/Bedrock for responses and writes assistant messages to EventStore; frontend must pull/stream those events.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| dioxus-desktop/src/components.rs | Chat UI + WS client | Adds WS streaming + fallback polling for assistant responses |
| dioxus-desktop/src/terminal.rs | Terminal view + WS client | Handles WS status/info/error and resize for xterm |
| dioxus-desktop/public/terminal.js | xterm.js bridge | Focus handling so terminal accepts input |
| sandbox/src/api/websocket_chat.rs | Chat WS server | Streams `thinking`/`response` payloads to frontend |
| sandbox/src/api/terminal.rs | Terminal WS server | PTY IO and status info for terminal sessions |

### Key Patterns Discovered

- Dioxus uses `use_effect` for mount-only side effects and `Signal` for reactive state.
- WebSocket handling in WASM requires keeping `Closure` handles in a struct to prevent GC.
- API base in `dioxus-desktop/src/api.rs` uses localhost:8080 in dev and same-origin in prod; WS URLs are derived from that.

## Work Completed

### Tasks Finished

- [x] Wire chat UI to WebSocket streaming responses (with fallback HTTP polling)
- [x] Add terminal auto-focus and click-to-focus in JS bridge
- [x] Surface terminal WS info/error in UI status

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| dioxus-desktop/public/terminal.js | Focus terminal on open/click | xterm wasn’t receiving keyboard input without focus |
| dioxus-desktop/src/components.rs | Chat WS client, response parsing, HTTP fallback polling | Chat UI previously only posted user messages and never got assistant responses |
| dioxus-desktop/src/terminal.rs | Handle `info` and `error` WS messages | Provide status/error visibility for terminal sessions |
| dioxus-desktop/src/desktop.rs | Desktop icon click + window canvas layering | Fix app window launching (already done earlier) |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use chat WS for responses with HTTP fallback | WS-only vs HTTP-only | WS is required for streaming; fallback keeps chat usable if WS fails |
| Parse `response.content` JSON for `text` | Treat as raw string | Backend sends JSON string in `content`; parse avoids showing raw JSON |
| Focus xterm in JS bridge | Manage focus in Rust | JS access to xterm instance is simplest and reliable |

## Pending Work

### Immediate Next Steps

1. Run `just dev-sandbox` + `just dev-ui` and verify chat/terminal via UI or agent-browser.
2. If chat still silent, inspect backend logs for BAML/Bedrock errors and ensure credentials/config are present.
3. If terminal still blank, verify `/ws/terminal/{terminal_id}` is opening and note any UI error message.

### Blockers/Open Questions

- ChatAgent responses depend on Bedrock/BAML configuration; not verified in this session.

### Deferred Items

- Chat UI connection indicator and streaming thinking/tool call display.
- Terminal reconnect UX (error overlay/retry button).

## Context for Resuming Agent

### Important Context

- Chat UI now listens on `/ws/chat/{actor_id}/{user_id}` and expects `response` messages whose `content` field is a JSON string containing `text`. Errors are shown as assistant messages if provided.
- If WS is unavailable, chat falls back to HTTP and polls EventStore for assistant responses up to ~3 seconds.
- Terminal UI status now updates on WS `info` messages and shows any `error` payloads; xterm focus is handled in `dioxus-desktop/public/terminal.js`.

### Assumptions Made

- Backend API runs on `http://localhost:8080` in dev.
- User id is `user-1` in frontend.

### Potential Gotchas

- Chat backend still uses actix WebSocket handler (`sandbox/src/api/websocket_chat.rs`), which may be affected by upcoming actix->axum refactor.
- ChatAgent may fail if Bedrock credentials or BAML config are missing; frontend will then show errors or never receive assistant output.
- Terminal depends on xterm assets in `dioxus-desktop/public`; caching or missing assets will break rendering.

## Environment State

### Tools/Services Used

- agent-browser (used earlier to click desktop icons and validate window creation)

### Active Processes

- None noted

### Environment Variables

- Not verified; ChatAgent likely requires AWS/Bedrock credentials

## Related Resources

- docs/terminal-ui.md
- docs/CHOIR_MULTI_AGENT_VISION.md
- dioxus-desktop/src/components.rs
- dioxus-desktop/src/terminal.rs
- sandbox/src/api/websocket_chat.rs
- sandbox/src/api/terminal.rs

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
