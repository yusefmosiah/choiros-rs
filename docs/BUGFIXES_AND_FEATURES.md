# ChoirOS Bugfixes and Features Roadmap

*Last updated: 2026-03-06*

## Current Status

**Recently Completed (Last 3 days, ~50 commits):**
- [x] Dioxus → React migration foundation
- [x] React + TypeScript + Vite setup
- [x] TypeScript type generation from Rust (ts-rs)
- [x] WebSocket client implementation
- [x] Window management (minimize, maximize, focus, restore)
- [x] Desktop UI components (Desktop, WindowManager, PromptBar)
- [x] Zustand state management stores
- [x] Fix: Duplicate window creation bug
- [x] Fix: WebSocket race conditions
- [x] Fix: "Window not found" errors
- [x] Fix: React StrictMode issues
- [x] 33 frontend tests passing
- [x] 21 backend tests passing
- [x] E2E tests working with agent-browser

## Active Bugs

### P0 - Critical

- [ ] **Chat app replicates content across windows**
  - **Problem:** Opening multiple Chat windows shows the same conversation
  - **Expected:** Each window has independent thread/conversation
  - **Related:** Thread management needed

- [ ] **Multi-browser tab synchronization issues**
  - **Problem:** Terminal and Chat behave weird in multiple browsers/tabs
  - **Expected:** Shared backend state but different UI state per tab
  - **Note:** Related to deferred auth layer on hypervisor

### P1 - High

- [ ] **Desktop shows "Error loading desktop" / HTTP 502 on cold start (~10s)**
  - **Problem:** Hypervisor starts accepting requests before sandbox VM is ready on :8080. During the ~10s boot window, proxy returns 502 Bad Gateway.
  - **Root cause:** `hypervisor/src/main.rs:75-80` spawns `boot_live_sandbox()` in background while HTTP server starts immediately. Proxy at `hypervisor/src/proxy/mod.rs:39` returns 502 when `TcpStream::connect` to sandbox fails.
  - **Contributing factor:** `load_initial_desktop_state` in `dioxus-desktop/src/desktop/effects.rs:95-114` makes a single attempt with no retry. Compare to `register_apps` which retries 3x with backoff.
  - **Fix options:** (a) Add retry+backoff to desktop state loading, (b) show loading splash during cold boot, (c) have hypervisor queue requests until sandbox ready

- [ ] **WebSocket status dot turns green→orange after ~10s, requires page reload**
  - **Problem:** WS connects (green dot), then disconnects (orange dot) shortly after. No auto-reconnect exists.
  - **Root cause:** No server-side WS keepalive ping in `sandbox/src/api/websocket.rs:201-286`. No client-side reconnect logic in `dioxus-desktop/src/desktop/ws.rs:569-576` — `onclose` just fires `WsEvent::Disconnected` and stops. Possible Caddy idle timeout or sandbox actor restart during settling.
  - **Fix options:** (a) Add server-side periodic ping interval, (b) add client-side WS auto-reconnect with backoff, (c) investigate Caddy proxy timeout settings

- [ ] Window animation polish (minimize/maximize transitions)
- [ ] Chat status UX improvements (thinking/completion states)
- [ ] Theme persistence across sessions

## Features to Build

### Phase 1: Core UI Stabilization (Current)

- [ ] **Chat Thread Management**
  - Individual threads per Chat window
  - Thread list sidebar in Chat app
  - Thread switching logic
  - Grey out + toast for already-open threads
  - Files: Chat.tsx, chat.ts store, backend chat.rs

- [ ] **Multi-Browser State Handling**
  - Per-tab UI state isolation
  - Shared backend state synchronization
  - Session management per browser tab

### Phase 2: Content Apps (Next)

- [ ] **Mail Application**
  - Email client UI
  - Backend mail actor
  - Integration with mail providers

- [ ] **Calendar Application**
  - Calendar view UI
  - Event management
  - Backend calendar actor

### Phase 3: Infrastructure

- [ ] **Event Bus Implementation**
  - Pub/sub system for async workers
  - Worker integration with event emission
  - Dashboard WebSocket for real-time events

- [ ] **Prompt Bar Shell Interface**
  - Shell-like command interface
  - Spawning workers from prompt bar
  - Command completion and history

### Phase 4: Advanced Features

- [ ] **Multi-Agent System**
  - Supervisors for coordination
  - Workers for task execution
  - Watchers for monitoring
  - Non-blocking task architecture

- [ ] **Real Sandboxing with Nix**
  - Nix-based environment isolation
  - DevOps pipeline integration
  - Reproducible builds

- [ ] **Deferred Auth Layer**
  - Authentication system
  - Hypervisor integration
  - Multi-user support

## Technical Debt

- [ ] Update ARCHITECTURE_SPECIFICATION.md (Dioxus → React)
- [ ] Update DESKTOP_ARCHITECTURE_DESIGN.md
- [ ] Update README.md tech stack
- [ ] Document ts-rs type generation pipeline
- [ ] Document React WebSocket client architecture

## Archive Candidates

Documents to potentially archive after verification:
- Old Dioxus-specific research docs
- Superseded execution plans
- Completed phase handoffs (>30 days old)

---

## Development Notes

### Multi-Browser Architecture Goal
```
Backend State (Shared)
    ↓
WebSocket Broadcast
    ↓
┌──────────┬──────────┬──────────┐
│ Tab 1    │ Tab 2    │ Tab 3    │
│ UI State │ UI State │ UI State │
│ (local)  │ (local)  │ (local)  │
└──────────┴──────────┴──────────┘
```

### Next Immediate Priority
1. Fix Chat thread management
2. Document React architecture properly
3. Fix multi-browser tab issues
