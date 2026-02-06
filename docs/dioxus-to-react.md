# Dioxus to React Migration: Task Dependency Graph

## Executive Summary

This document provides a comprehensive DAG (Directed Acyclic Graph) for migrating the ChoirOS sandbox UI from Dioxus to React while preserving the Rust backend. The migration is structured into phases with tasks designed for parallel execution by multiple subagents.

### Regression Tracking

- 2026-02-06: Terminal browser CPU regression (reload/multi-window) and desktop loading deadlock were addressed in React.
- Incident doc: `/Users/wiz/choiros-rs/docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md`
- 2026-02-06: Rollback experiment path created to validate reversibility:
  - `dioxus-desktop/` = Dioxus frontend (active rollback target)
  - `sandbox-ui/` = React frontend (kept as backup)
  - Backend API/websocket contracts intentionally unchanged for A/B validation.
- 2026-02-06: Dioxus terminal multi-browser/reload stability fix landed.
  - Handoff doc: `/Users/wiz/choiros-rs/docs/handoffs/2026-02-06-dioxus-terminal-multibrowser-fix.md`

**Key Principles:**
1. **Feature parity with Dioxus first** - Match existing Dioxus features before adding new ones
2. **WebSocket-first** - WebSocket testing is a critical priority
3. **Gradual feature addition** - Defer Writer, extended Chat, Files, Mail, etc. for later
4. **Rust ↔ TypeScript types** - Shared types now bridges Rust to TypeScript (not Rust to Rust)

---

## Architecture Overview

### Current State

- **dioxus-desktop/**: Dioxus frontend (restored for rollback validation)
- **sandbox/**: Rust backend with **Ractor** actors and **Axum** web framework (13,154 lines - PRESERVED)
- **sandbox-ui/**: React frontend kept as backup during rollback testing
- **shared-types/**: Rust structs shared with backend and Dioxus/React experiments
- **choiros/**: Reference React implementation (patterns only - NOT cloning)

### Target State

- **sandbox-ui/**: New React + TypeScript frontend (long-term target, currently backup)
- **dioxus-desktop/**: Active rollback frontend for regression isolation
- **sandbox/**: Unchanged Rust backend (Ractor + Axum)
- **shared-types/**: Rust types with TypeScript generation pipeline
- **Feature parity with existing Dioxus UI first**, then gradual enhancements

---

## Phase 1: Setup & Foundation

### Task 1.1: Project Scaffolding

- **ID**: T1.1
- **Name**: Initialize React Project Structure
- **Description**: Create new React + TypeScript + Vite project in sandbox-ui/, replacing Dioxus. Set up build configuration, linting, and folder structure.
- **Effort**: Medium
- **Dependencies**: None
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/package.json` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/vite.config.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/tsconfig.json` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/index.html` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/main.tsx` (new)
- **Parallelizable With**: T1.2, T1.3

### Task 1.2: Type Generation from Rust (Rust ↔ TypeScript)

- **ID**: T1.2
- **Name**: Generate TypeScript Types from Rust shared-types
- **Description**: Create type generation pipeline to convert Rust types to TypeScript. Use ts-rs or type-share to generate TypeScript definitions from Rust structs. **Critical**: This is now Rust-to-TypeScript (not Rust-to-Rust).
- **Effort**: Medium
- **Dependencies**: None
- **Files Affected**:
  - `/Users/wiz/choiros-rs/shared-types/` (add ts-rs derive macros)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/types/generated.ts` (new)
  - Type generation script in `/Users/wiz/choiros-rs/scripts/generate-types.sh`
- **Parallelizable With**: T1.1, T1.3
- **Key Types to Generate**:
  - `DesktopState`, `WindowState`
  - `ChatMessage`, `ClientMessage`, `ServerMessage`
  - `WsMessage` (WebSocket message types)
  - `AppDefinition`
  - `ViewerDescriptor`, `ViewerRevision`

### Task 1.3: API Client Foundation

- **ID**: T1.3
- **Name**: Create API Client Base
- **Description**: Implement base API client with fetch wrapper, error handling. Match the Rust backend API in sandbox/src/api/mod.rs.
- **Effort**: Small
- **Dependencies**: None
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/api/client.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/api/errors.ts` (new)
- **Parallelizable With**: T1.1, T1.2

### Task 1.4: WebSocket Connection Layer (CRITICAL)

- **ID**: T1.4
- **Name**: Implement WebSocket Client with Testing
- **Description**: Create WebSocket client for real-time events from Rust backend. **This is a critical priority** - must be thoroughly tested. Support all WsMessage types from Rust backend (Subscribe, Ping, Pong, DesktopState, WindowOpened, WindowClosed, etc.).
- **Effort**: Large
- **Dependencies**: T1.2 (needs types)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/ws/client.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/ws/types.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/hooks/useWebSocket.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/ws/client.test.ts` (tests - CRITICAL)
- **Parallelizable With**: T1.5
- **Testing Requirements**:
  - Connection establishment
  - Subscribe/unsubscribe to desktop
  - Receive and parse all event types
  - Reconnection logic
  - Error handling
  - Message serialization/deserialization

### Task 1.5: State Management Setup

- **ID**: T1.5
- **Name**: Configure Zustand Stores
- **Description**: Set up Zustand for state management. Create store structure for windows, desktop state, and events. Keep minimal - match Dioxus features only.
- **Effort**: Small
- **Dependencies**: T1.2 (needs types)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/stores/index.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/stores/windows.ts` (new)
  - `/Users/wiz/choiros-rs/sandbox-ui/src/stores/desktop.ts` (new)
- **Parallelizable With**: T1.4

---

## Phase 2: Core Infrastructure

### Task 2.1: Window Management System

- **ID**: T2.1
- **Name**: Implement Window Management
- **Description**: Create window management system matching Dioxus functionality. Support drag, resize, minimize, maximize, z-index, focus. Reference the Dioxus implementation, NOT the old choiros React app.
- **Effort**: Large
- **Dependencies**: T1.5 (needs stores), T1.2 (needs types)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/window/Window.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/window/WindowManager.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/window/Window.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/stores/windows.ts`
- **Parallelizable With**: T2.3

### Task 2.2: Desktop Shell Components

- **ID**: T2.2
- **Name**: Implement Desktop Shell
- **Description**: Create desktop shell with icons, taskbar, and prompt bar. Match existing Dioxus desktop shell features only.
- **Effort**: Medium
- **Dependencies**: T2.1 (window management), T1.5 (stores)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Desktop.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Desktop.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Icon.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Taskbar.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/PromptBar.tsx`

### Task 2.3: Theme System

- **ID**: T2.3
- **Name**: Implement Theme System (Dark/Light)
- **Description**: Create CSS variable-based theme system with dark/light modes. Match Dioxus theme implementation.
- **Effort**: Small
- **Dependencies**: T1.1 (project setup)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/styles/variables.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/styles/themes/dark.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/styles/themes/light.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/hooks/useTheme.ts`
- **Parallelizable With**: T2.1

### Task 2.4: App Registry System

- **ID**: T2.4
- **Name**: Create App Registry
- **Description**: Implement app registration system matching Dioxus apps. Define app metadata, icons, default sizes. **Only include apps that exist in Dioxus**: Chat, Terminal, basic apps.
- **Effort**: Small
- **Dependencies**: T1.2 (types), T2.3 (theme for icons)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/apps.ts`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/types/apps.ts`

### Task 2.5: Event Handling Infrastructure

- **ID**: T2.5
- **Name**: Implement Event Handling System
- **Description**: Create event handling layer for WebSocket events. Map to appropriate store updates. Handle all WsMessage variants from Rust backend.
- **Effort**: Medium
- **Dependencies**: T1.4 (WebSocket), T1.5 (stores)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/events/handler.ts`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/events/dispatcher.ts`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/hooks/useEvents.ts`

---

## Phase 3: Feature Migration (Dioxus Parity Only)

**IMPORTANT**: Only implement features that exist in the current Dioxus frontend. Defer Writer, Files, Mail, extended Chat features for later phases.

### Task 3.1: Chat API Integration

- **ID**: T3.1
- **Name**: Implement Chat API Client
- **Description**: Create API client for chat endpoints. POST /chat/send, GET /chat/{actor_id}/messages. Match Rust API.
- **Effort**: Small
- **Dependencies**: T1.3 (API client), T1.2 (types)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/api/chat.ts`

### Task 3.2: Chat App Component (Basic)

- **ID**: T3.2
- **Name**: Build Chat App UI (Basic)
- **Description**: Create Chat app component with message list, input. Match Dioxus Chat features only - do NOT add extra features from old choiros React app.
- **Effort**: Medium
- **Dependencies**: T3.1 (API), T2.1 (windows), T2.5 (events)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Chat/Chat.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Chat/Chat.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Chat/MessageList.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Chat/ChatInput.tsx`
- **Deferred for Later**: Advanced features like file attachments, rich formatting, etc.

### Task 3.3: Terminal App

- **ID**: T3.3
- **Name**: Implement Terminal App
- **Description**: Create terminal app using xterm.js with PTY integration. Match Dioxus Terminal features.
- **Effort**: Large
- **Dependencies**: T2.1 (windows), T1.4 (WebSocket for PTY)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/Terminal.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/Terminal.css`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/apps/Terminal/xterm.css`

### Task 3.4: Window-App Integration

- **ID**: T3.4
- **Name**: Integrate Apps with Window Manager
- **Description**: Wire Chat and Terminal apps into WindowManager component. Map app IDs to components. Handle app launching from desktop icons and taskbar.
- **Effort**: Medium
- **Dependencies**: T2.1 (windows), T2.4 (registry), T3.2, T3.3
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/window/WindowManager.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/App.tsx`

---

## Phase 4: Integration & Polish

### Task 4.1: Desktop Integration

- **ID**: T4.1
- **Name**: Full Desktop Shell Integration
- **Description**: Integrate all desktop components (icons, taskbar, prompt bar) with window system and apps. Handle window state in taskbar.
- **Effort**: Medium
- **Dependencies**: T2.2 (desktop), T3.4 (window-app integration)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Desktop.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/desktop/Taskbar.tsx`

### Task 4.2: WebSocket Testing Suite (CRITICAL)

- **ID**: T4.2
- **Name**: Comprehensive WebSocket Testing
- **Description**: Thoroughly test WebSocket functionality. This is a critical priority.
- **Effort**: Large
- **Dependencies**: T1.4 (WebSocket client), T2.5 (event handling), T3.4 (integration)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/lib/ws/client.test.ts`
  - `/Users/wiz/choiros-rs/sandbox-ui/e2e/websocket.spec.ts`
- **Test Coverage Required**:
  - Connection lifecycle (connect, disconnect, reconnect)
  - Subscription to desktop state
  - All window events (opened, closed, moved, resized, focused, minimized, maximized)
  - Chat WebSocket events
  - Terminal WebSocket I/O
  - Error scenarios (network failure, server errors)
  - Concurrent connections
  - Message ordering guarantees

### Task 4.3: Error Handling & Loading States

- **ID**: T4.3
- **Name**: Implement Error Boundaries and Loading States
- **Description**: Add React error boundaries, loading spinners, and graceful error handling throughout the UI.
- **Effort**: Medium
- **Dependencies**: T3.4 (integration complete)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/ErrorBoundary.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/LoadingSpinner.tsx`
  - `/Users/wiz/choiros-rs/sandbox-ui/src/components/ConnectionStatus.tsx`

### Task 4.4: Build & Deploy Configuration

- **ID**: T4.4
- **Name**: Configure Production Build
- **Description**: Set up production build, static file serving from Rust backend.
- **Effort**: Medium
- **Dependencies**: T1.1 (frontend project)
- **Files Affected**:
  - `/Users/wiz/choiros-rs/sandbox-ui/Dockerfile` (new)
  - `/Users/wiz/choiros-rs/sandbox/src/main.rs` (static file serving)
  - `/Users/wiz/choiros-rs/Cargo.toml` (workspace updates)

---

## Deferred Features (Post-Migration)

These features exist in the old choiros React app but should be added AFTER achieving Dioxus parity:

- **Writer App** (T3.3 in original plan) - Deferred
- **Files App** (T3.5 in original plan) - Deferred
- **Content Viewers** (T3.6 in original plan) - Deferred
- **Mail App** - Deferred
- **Extended Chat features** (file attachments, rich formatting) - Deferred
- **Event Stream UI** - Can be added later
- **Authentication** - Use existing if possible, extend later

---

## Dependency Graph (DAG)

```
PHASE 1: SETUP
├── T1.1: Project Scaffolding ──────┐
├── T1.2: Type Generation ──────────┤──┐
├── T1.3: API Client Foundation ────┤  │
│                                    │  │
├── T1.4: WebSocket Connection ◄────┘  │
│       (depends: T1.2)                │
│       ⚠️ CRITICAL: Needs testing     │
│                                      │
└── T1.5: State Management ◄───────────┘
        (depends: T1.2)

PHASE 2: CORE INFRASTRUCTURE
├── T2.1: Window Management ◄────────┐
│       (depends: T1.5, T1.2)        │
│                                    │
├── T2.2: Desktop Shell ◄────────────┤
│       (depends: T2.1, T1.5)        │
│                                    │
├── T2.3: Theme System ◄─────────────┤
│       (depends: T1.1)              │
│                                    │
├── T2.4: App Registry ◄─────────────┤
│       (depends: T1.2, T2.3)        │
│                                    │
└── T2.5: Event Handling ◄───────────┘
        (depends: T1.4, T1.5)

PHASE 3: FEATURE MIGRATION (DIOXUS PARITY)
├── T3.1: Chat API ◄─────────────────┐
│       (depends: T1.3, T1.2)        │
│                                    │
├── T3.2: Chat App (Basic) ◄─────────┤
│       (depends: T3.1, T2.1, T2.5)  │
│                                    │
├── T3.3: Terminal App ◄─────────────┤
│       (depends: T2.1, T1.4)        │
│                                    │
└── T3.4: Window-App Integration ◄───┘
        (depends: T2.1, T2.4, T3.x)

PHASE 4: INTEGRATION & TESTING
├── T4.1: Desktop Integration ◄──────┐
│       (depends: T2.2, T3.4)        │
│                                    │
├── T4.2: WebSocket Testing ◄────────┤
│       (depends: T1.4, T2.5, T3.4)  │
│       ⚠️ CRITICAL PRIORITY         │
│                                    │
├── T4.3: Error Handling ◄───────────┤
│       (depends: T3.4)              │
│                                    │
└── T4.4: Build & Deploy ◄───────────┘
        (depends: T1.1)
```

---

## Parallel Execution Groups

### Group A: Foundation (Week 1)
- **Agents**: 3
- **Tasks**: T1.1, T1.2, T1.3, T1.4, T1.5, T2.3
- **Parallelism**: Maximum (no dependencies between T1.x tasks)
- **Focus**: Get type generation and WebSocket client working

### Group B: Core Infrastructure (Week 2)
- **Agents**: 2
- **Tasks**: T2.1, T2.2, T2.4, T2.5
- **Parallelism**: T2.1 and T2.3 can start immediately; T2.2 depends on T2.1
- **Focus**: Window management and event handling

### Group C: Feature Migration (Week 3)
- **Agents**: 2
- **Tasks**: T3.1, T3.2, T3.3, T3.4
- **Parallelism**: Chat and Terminal can be developed in parallel
- **Focus**: Dioxus feature parity only

### Group D: Testing & Integration (Week 4)
- **Agents**: 2
- **Tasks**: T4.1, T4.2, T4.3, T4.4
- **Focus**: WebSocket testing is critical priority

---

## Critical Path

The critical path is:

```
T1.2 (types) → T1.4 (WebSocket) → T1.5 (stores) → T2.1 (windows) → T3.4 (integration) → T4.2 (WebSocket testing)
```

**Duration estimate**: 3-4 weeks with 2-3 parallel agents

---

## Risk Mitigation

1. **Type Generation Failure**: Fallback to manual TypeScript type definitions
2. **WebSocket Compatibility**: This is the highest risk - extensive testing required (T4.2)
3. **Window Management Complexity**: Reference Dioxus implementation, not old React app
4. **Scope Creep**: Strictly limit to Dioxus parity - defer all new features

---

## Documentation Updates Required

1. Fix all references to "Actix" → "Ractor + Axum" in existing docs
2. Document the Rust ↔ TypeScript type generation workflow
3. Document WebSocket protocol for frontend developers

---

## Critical Files for Implementation

- `/Users/wiz/choiros-rs/sandbox/src/main.rs` - Backend entry point (Axum)
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs` - API routes (Axum)
- `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs` - WebSocket handler
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs` - Shared types for ts-rs
- `/Users/wiz/choiros-rs/sandbox-ui/src/lib/ws/client.ts` - WebSocket client (CRITICAL)

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Type Generation** | `ts-rs` | Simple derive macro, low commitment, easy to change later. Add `#[derive(TS)]` alongside existing `#[derive(Serialize, Deserialize)]` |
| **WebSocket Testing** | Real backend | Integration tests run against actual Rust backend for reliability |
| **Styling** | Plain CSS | Match Dioxus approach. Better for agentic coding than Tailwind (class soup) or CSS Modules (build complexity) |
| **State Management** | Zustand | Proven pattern from old React app. Simple, no boilerplate, works well with agentic code generation |
| **Build Integration** | Justfile | Use existing Justfile to orchestrate both npm and cargo builds |
| **Deployment** | Rust serves static files | Axum serves built React files from `sandbox-ui/dist/` |

---

## Orchestration Strategy

### Agent Teams Approach

This migration can be executed by **2-3 parallel agents** working on independent tasks:

**Team A - Foundation (Week 1)**
- T1.1: Project scaffolding
- T1.2: ts-rs type generation setup
- T1.3: API client base
- T1.4: WebSocket client (critical)

**Team B - Core UI (Week 1-2)**
- T1.5: Zustand stores (depends on types)
- T2.1: Window management
- T2.2: Desktop shell
- T2.3: Theme system (plain CSS)

**Team C - Features (Week 2-3)**
- T2.4: App registry
- T2.5: Event handling
- T3.1-3.4: Chat, Terminal, integration

**Team D - Testing & Polish (Week 3-4)**
- T4.1: Desktop integration
- T4.2: WebSocket testing (critical)
- T4.3: Error handling
- T4.4: Build configuration

### Coordination Mechanism

1. **Task dependencies** in this doc define order
2. **Shared types** (T1.2) is the bottleneck - all other agents wait for this
3. **File-based coordination** - agents work on different files to avoid conflicts
4. **Integration points** - agents hand off via git commits or shared working directory
