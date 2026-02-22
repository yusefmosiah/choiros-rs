# React Migration Handoff - Reconnect + Streaming + Tests

**Date**: 2026-02-06  
**Scope**: Phase 2/3 follow-up (terminal resilience, chat streaming, test coverage)  
**Status**: ✅ Complete for requested next steps

---

## Summary

Implemented the requested next steps after initial desktop/chat/terminal migration:

1. Added terminal reconnect/backoff behavior and backend teardown on unmount.
2. Migrated Chat app from polling-first behavior to live WebSocket streaming via `/ws/chat/{actor_id}/{user_id}`.
3. Added unit tests for new WS utility logic (terminal and chat parsing/reconnect helpers).

All checks pass (`test`, `tsc`, `build`).

---

## Completed Work

### 1) Terminal reconnect/backoff + teardown

**What changed**
- Reconnect strategy with exponential backoff for terminal websocket disconnects.
- Automatic reconnect scheduling after close/error.
- Proper cleanup of timer, WebSocket, ResizeObserver, and xterm subscriptions.
- Best-effort backend session teardown via `stopTerminal(terminalId)` on component unmount.

**Files**
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Terminal/Terminal.tsx`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Terminal/ws.ts`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/lib/api/terminal.ts`

### 2) Chat streaming over websocket

**What changed**
- Chat now connects to `/ws/chat/{actor_id}/{user_id}`.
- Handles streamed server messages and adds assistant response messages live.
- Tracks connection state (`Live` / `Retrying`) in the header.
- Uses HTTP `sendMessage` as fallback only when websocket isn’t open.
- Keeps optimistic user message behavior with pending marker and resolves pending on response.

**Files**
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Chat/Chat.tsx`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Chat/ws.ts`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Chat/Chat.css`

### 3) Integration wiring and test harness

**What changed**
- Terminal app remains mounted in window renderer and now runs the live xterm runtime.
- Added `vitest` and `npm test` script.
- Added WS utility tests:
  - Terminal parser + reconnect delay cap
  - Chat stream parser + response-text extraction

**Files**
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/window/Window.tsx`
- `/Users/wiz/choiros-rs/dioxus-desktop/package.json`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Terminal/ws.test.ts`
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/apps/Chat/ws.test.ts`

---

## Verification

Executed and passing:

```bash
cd /Users/wiz/choiros-rs/dioxus-desktop && npm test
cd /Users/wiz/choiros-rs/dioxus-desktop && npx tsc --noEmit
cd /Users/wiz/choiros-rs/dioxus-desktop && npm run build
```

Results:
- `vitest`: 2 files, 7 tests passed.
- TypeScript strict compile passed.
- Production build passed.

---

## Notes / Caveats

- Local Vite cache files under `dioxus-desktop/node_modules/.vite/` continue to change during local builds.
- Chat streaming currently consumes final `response` chunks as assistant messages. Tool/thinking chunks are parsed but not rendered yet.
- Terminal reconnect is implemented, but max retry count is currently unbounded (bounded by delay cap); this is acceptable for dev UX and can be tightened if needed.

---

## Suggested Next Steps

1. Add chat stream UI for `thinking` / `tool_call` / `tool_result` chunks.
2. Add component-level tests for terminal reconnect lifecycle (mock `WebSocket`).
3. Implement Writer/Files app shells or real app integration for remaining placeholders.
4. Consider reducing bundle size (xterm dominates chunk) via lazy-loading terminal app.

