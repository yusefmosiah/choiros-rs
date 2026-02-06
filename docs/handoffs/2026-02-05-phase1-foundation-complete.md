# Phase 1 Foundation Complete - Handoff Document

**Date**: 2026-02-05
**Migration**: Dioxus to React - Phase 1 Foundation
**Status**: ✅ COMPLETE

---

## Summary

All 3 parallel Phase 1 tasks completed successfully. The React project foundation is now in place with TypeScript type generation from Rust and a working API client.

---

## Completed Tasks

### T1.1: React Project Scaffolding ✅
- **Backup**: Original Dioxus code backed up to `/Users/wiz/choiros-rs/sandbox-ui-backup/`
- **Project**: New React + TypeScript + Vite project initialized
- **Dependencies**: React 18, Zustand, xterm.js, Vite
- **Config**: TypeScript strict mode, path aliases (`@/*`), proxy to localhost:8080
- **Structure**: Folder hierarchy created (components/, hooks/, lib/, stores/, styles/)

**Files Created**:
- `sandbox-ui/package.json`
- `sandbox-ui/vite.config.ts`
- `sandbox-ui/tsconfig.json`
- `sandbox-ui/index.html`
- `sandbox-ui/src/main.tsx`
- `sandbox-ui/src/App.tsx`
- `sandbox-ui/src/styles/index.css`

### T1.2: Type Generation Pipeline ✅
- **ts-rs**: Added to shared-types/Cargo.toml with chrono-impl feature
- **Types Exported**: 19 Rust types → TypeScript
  - Core: `ActorId`
  - State: `DesktopState`, `WindowState`, `AppDefinition`
  - Messages: `ChatMsg`, `ChatMessage`, `Sender`, `ToolStatus`
  - Events: `Event`, `AppendEvent`, `QueryEvents`
  - WebSocket: `WsMsg`
  - Viewer: `ViewerKind`, `ViewerResource`, `ViewerCapabilities`, `ViewerDescriptor`, `ViewerRevision`
  - Tools: `ToolDef`, `ToolCall`
- **Script**: `scripts/generate-types.sh` automates type generation
- **Output**: `sandbox-ui/src/types/generated.ts`

**Key Type Mappings**:
```typescript
export type ActorId = string;
export type DesktopState = { windows: WindowState[], active_window: string | null, apps: AppDefinition[] };
export type WsMsg = { type: "Subscribe", actor_id: ActorId } | { type: "Send", ... } | ...;
```

### T1.3: API Client Foundation ✅
- **Error Handling**: Custom error classes (ApiError, NetworkError, TimeoutError, HttpError)
- **Client**: Fetch wrapper with timeout, abort controller, automatic JSON parsing
- **Endpoints**: Health, Chat, Desktop, Terminal, User APIs
- **Env Config**: `VITE_API_URL` defaults to localhost:8080

**Files Created**:
- `src/lib/api/errors.ts` - Error types
- `src/lib/api/client.ts` - ApiClient class
- `src/lib/api/index.ts` - Module exports
- `src/lib/api/health.ts` - Health check
- `src/lib/api/chat.ts` - Send/get messages
- `src/lib/api/desktop.ts` - Window management (CRUD + move/resize/focus/min/max/restore)
- `src/lib/api/terminal.ts` - Terminal lifecycle + WebSocket helper
- `src/lib/api/user.ts` - User preferences

**TypeScript**: Compiles successfully (`npx tsc --noEmit` passes)

---

## Critical Path Status

```
T1.2 (types) ✅ → T1.4 (WebSocket) → T1.5 (stores) → Phase 2
```

**T1.2 is unblocked** - All downstream tasks can now proceed.

---

## Next Phase: T1.4 + T1.5 (Ready to Start)

These tasks can now run in parallel:

### T1.4: WebSocket Client (CRITICAL PRIORITY)
**Location**: `src/lib/ws/client.ts`, `src/hooks/useWebSocket.ts`

**Requirements**:
- WebSocket connection to `ws://localhost:8080/ws`
- Support all `WsMessage` types from Rust:
  - Client → Server: `Subscribe`, `Ping`
  - Server → Client: `Pong`, `DesktopState`, `WindowOpened`, `WindowClosed`, `WindowMoved`, `WindowResized`, `WindowFocused`, `WindowMinimized`, `WindowMaximized`, `WindowRestored`, `AppRegistered`, `Error`
- Reconnection logic with exponential backoff
- React hook `useWebSocket(desktopId: string)` for components
- **Must be thoroughly tested** (T4.2 is critical priority per plan)

**Reference**: See `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs` for backend protocol

### T1.5: Zustand State Management
**Location**: `src/stores/*.ts`

**Requirements**:
- `desktop.ts`: Desktop state, window list, active window
- `windows.ts`: Individual window state (position, size, z-index, minimized, maximized)
- `chat.ts`: Chat messages, loading states
- Match Dioxus state structure - don't add new features yet

**Reference**: See `/Users/wiz/choiros-rs/sandbox-ui-backup/src/desktop/state.rs` for Dioxus state patterns

---

## Architecture Decisions (Locked In)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Type Generation** | `ts-rs` | Derive macro, low commitment |
| **State Management** | Zustand | Proven, minimal boilerplate |
| **Styling** | Plain CSS | Match Dioxus, agent-friendly |
| **Build Tool** | Vite | Fast dev server, proxy support |
| **API Client** | Fetch wrapper | No external deps, full control |

---

## Files Ready for Next Phase

```
sandbox-ui/
├── src/
│   ├── types/
│   │   └── generated.ts      ✅ (19 types from Rust)
│   ├── lib/
│   │   └── api/              ✅ (7 API modules)
│   │       ├── errors.ts
│   │       ├── client.ts
│   │       ├── index.ts
│   │       ├── health.ts
│   │       ├── chat.ts
│   │       ├── desktop.ts
│   │       ├── terminal.ts
│   │       └── user.ts
│   ├── stores/               📂 (empty - T1.5 target)
│   ├── lib/ws/               📂 (empty - T1.4 target)
│   │   └── client.ts
│   └── hooks/                📂 (empty - T1.4 target)
│       └── useWebSocket.ts
```

---

## Verification Commands

```bash
# Type generation
cd /Users/wiz/choiros-rs && ./scripts/generate-types.sh

# TypeScript check
cd /Users/wiz/choiros-rs/sandbox-ui && npx tsc --noEmit

# Install deps (if needed)
cd /Users/wiz/choiros-rs/sandbox-ui && npm install

# Dev server
cd /Users/wiz/choiros-rs/sandbox-ui && npm run dev  # port 3000
```

---

## Blockers

None. Phase 1 foundation is complete.

---

## Recommended Next Steps

1. **Compact/review** current state (as user requested)
2. **Launch T1.4 + T1.5 in parallel** (WebSocket client + Zustand stores)
3. **Then proceed to Phase 2**: Window Management (T2.1), Desktop Shell (T2.2)

---

## References

- Migration Plan: `/Users/wiz/choiros-rs/docs/dioxus-to-react.md`
- Rust Types: `/Users/wiz/choiros-rs/shared-types/src/lib.rs`
- Backend API: `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- WebSocket Handler: `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`
- Original Dioxus (for reference): `/Users/wiz/choiros-rs/sandbox-ui-backup/`
