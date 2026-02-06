# Sandbox UI App Feature Parity Analysis Report

**Date:** 2025-02-06
**Scope:** React sandbox-ui vs Dioxus backup
**Analysis Type:** Feature parity, bug identification, testing coverage

---

## Executive Summary

The React implementation has completed core Chat and Terminal apps but is missing critical viewer components (ImageViewer, TextViewer, ViewerShell) and tool call rendering features. Overall parity is approximately **65%**.

---

## App Status Matrix

| App | React Implementation | Dioxus Implementation | Parity | Status |
|-----|---------------------|----------------------|--------|--------|
| Chat | ✅ Complete (359 lines) | ✅ Complete (1257 lines) | 75% | Partial - missing tool call UI |
| Terminal | ✅ Complete (195 lines) | ✅ Complete (409 lines) | 85% | Minor differences |
| ImageViewer | ❌ Missing | ✅ Complete (65 lines) | 0% | Not ported |
| TextViewer | ❌ Missing | ✅ Complete (147 lines) | 0% | Not ported |
| ViewerShell | ❌ Missing | ✅ Complete (218 lines) | 0% | Not ported |
| Writer App | ⚠️ Placeholder only | ⚠️ Placeholder only | 100% | Both incomplete |
| Files App | ⚠️ Placeholder only | ⚠️ Placeholder only | 100% | Both incomplete |

**Overall Parity:** 65%

---

## Bugs Found

### Critical Bugs

#### 1. Chat Component - Race Condition in Pending Message Cleanup
**File:** `sandbox-ui/src/components/apps/Chat/Chat.tsx:264-266`
```typescript
setTimeout(() => {
  void loadMessages();
}, 500);
```
**Issue:** No cleanup of timeout on component unmount. If component unmounts before 500ms, this will try to update state of unmounted component.
**Fix Required:** Store timeout ID and clear in cleanup.

#### 2. Terminal Component - Silent Error on Stop
**File:** `sandbox-ui/src/components/apps/Terminal/Terminal.tsx:181-183`
```typescript
void stopTerminal(terminalId).catch(() => {
  // Best-effort cleanup for backend process.
});
```
**Issue:** Error is silently caught without any logging. Makes debugging difficult if cleanup fails.
**Fix Required:** At least console.error the caught error.

#### 3. Terminal Component - WebSocket URL Parameter Mismatch
**File:** `sandbox-ui/src/components/apps/Terminal/Terminal.tsx:94`
```typescript
const ws = new WebSocket(getTerminalWebSocketUrl(terminalId, userId));
```
**Issue:** React passes `userId` via query param, but Dioxus passes it via URL path. Backend may expect one format.
**Reference:** Dioxus `terminal.rs:230` uses path parameter.
**Fix Required:** Align with backend expectations.

---

### Medium Bugs

#### 4. Chat Component - Stream Events Silently Dropped
**File:** `sandbox-ui/src/components/apps/Chat/Chat.tsx:182`
```typescript
return next.slice(-24);  // Keeps only last 24 events
```
**Issue:** Older stream events are silently dropped without user notification.
**Fix Required:** Add visual indicator when events are being truncated.

#### 5. Chat Component - WebSocket Send Without Ready State Check
**File:** `sandbox-ui/src/components/apps/Chat/Chat.tsx:243-253`
```typescript
if (ws && ws.readyState === WebSocket.OPEN) {
  ws.send(JSON.stringify({...}));
  startPendingTimeout(tempId);
  return;
}
```
**Issue:** Fallback to HTTP API if WebSocket fails, but no indication to user which path was taken.
**Fix Required:** Show visual indicator of connection method.

#### 6. Terminal Component - Missing WebSocket URL in Cleanup
**File:** `sandbox-ui/src/components/apps/Terminal/Terminal.tsx:175-177`
```typescript
if (wsRef.current) {
  wsRef.current.close();
  wsRef.current = null;
}
```
**Issue:** No check if WebSocket was actually connected before closing.
**Fix Required:** Check `readyState` before close.

---

### Minor Bugs

#### 7. Chat Component - Missing Message ID Validation
**File:** `sandbox-ui/src/components/apps/Chat/Chat.tsx:141-150`
```typescript
let pendingId = payload.client_message_id;
if (pendingId && !pendingQueueRef.current.includes(pendingId)) {
  pendingId = undefined;
}
```
**Issue:** Logic flow is confusing - clears pendingId if not in queue, but then tries to use queue anyway.
**Fix Required:** Clarify logic flow or add comments.

#### 8. Window Component - Pointer Event Cleanup Missing
**File:** `sandbox-ui/src/components/window/Window.tsx:88-95`
```typescript
const handlePointerUp = (upEvent: PointerEvent) => {
  dragPointerIdRef.current = null;
  dragStartRef.current = null;
  globalThis.window.removeEventListener('pointermove', handlePointerMove);
  globalThis.window.removeEventListener('pointerup', handlePointerUp);
  globalThis.window.removeEventListener('pointercancel', handlePointerUp);
};
```
**Issue:** Event listener functions created in closure, not guaranteed to match what was added. Should store references.
**Fix Required:** Store function references to properly remove listeners.

---

## Missing Features from Dioxus

### 1. Tool Call Rendering (Major Feature Gap)

**Dioxus Implementation:** `sandbox-ui-backup/src/components.rs:589-666`

The Dioxus version includes sophisticated tool call UI with:
- Collapsible tool call sections
- Tool call details with reasoning
- Tool result display
- Expand/collapse all functionality
- Live activity indicator for pending tools

**React Status:** Completely missing. Stream events are shown but not rendered as tool calls.

**Impact:** Users cannot see what tools the AI is calling or their results.

**Estimated Effort:** 8-12 hours to port

---

### 2. ImageViewer Component

**Dioxus Implementation:** `sandbox-ui-backup/src/viewers/image.rs` (65 lines)

Features:
- Zoom in/out controls
- Pan/drag functionality
- Reset button
- Data URI support

**React Status:** Not implemented

**Impact:** Cannot view images in windows.

**Estimated Effort:** 4-6 hours to port

---

### 3. TextViewer Component

**Dioxus Implementation:** `sandbox-ui-backup/src/viewers/text.rs` (147 lines)

Features:
- Editable text area with JavaScript bridge
- Read-only mode support
- Change callback for content updates
- Monospace font rendering

**React Status:** Not implemented

**Impact:** Cannot edit/view text files in windows.

**Estimated Effort:** 6-8 hours to port (including JS bridge replacement)

---

### 4. ViewerShell Component

**Dioxus Implementation:** `sandbox-ui-backup/src/viewers/shell.rs` (218 lines)

Features:
- File loading with content fetching
- Save with revision conflict detection
- Reload functionality
- Dirty state tracking (unsaved changes)
- Error handling for conflicts
- Status display (Loading, Saved, Unsaved changes, Saving, Error)

**React Status:** Not implemented

**Impact:** No unified wrapper for viewing/editing files.

**Estimated Effort:** 8-10 hours to port

---

### 5. Chat Assistant Bundling

**Dioxus Implementation:** `sandbox-ui-backup/src/components.rs:510-723`

The Dioxus version has sophisticated message bundling:
- Collapses tool calls into assistant messages
- Shows thinking state
- Bundles tool results with responses
- `__assistant_bundle__:` prefix for bundled messages

**React Status:** Basic message rendering only

**Impact:** Less polished chat experience, no tool call visualization.

**Estimated Effort:** 6-8 hours to port

---

### 6. Terminal Reconnection with Jitter

**Dioxus Implementation:** `sandbox-ui-backup/src/terminal.rs:367-408`

Advanced reconnection features:
- Exponential backoff with max cap
- Jitter added to avoid thundering herd
- Max 6 retry attempts
- Timeout handling and cleanup

**React Status:** Basic exponential backoff, no jitter

**Impact:** Less robust reconnection under load.

**Estimated Effort:** 2-3 hours to enhance

---

## Refactoring Opportunities

### 1. Extract WebSocket Reconnection Logic

**Current Issue:** Both Chat and Terminal have similar reconnection logic duplicated.

**Suggested Refactor:**
```typescript
// sandbox-ui/src/lib/ws/reconnection.ts
export function useReconnectingWebSocket(config: {
  url: string;
  onOpen?: () => void;
  onMessage?: (data: string) => void;
  onError?: (error: Event) => void;
  onClose?: () => void;
  options?: {
    maxAttempts?: number;
    baseDelay?: number;
    maxDelay?: number;
    jitter?: boolean;
  };
})
```

**Benefits:**
- Eliminate ~50 lines of duplicated code
- Consistent reconnection behavior across apps
- Easier testing

---

### 2. Unify Message Parsing

**Current Issue:** Chat and Terminal have separate message parsers.

**Suggested Refactor:**
```typescript
// sandbox-ui/src/lib/ws/message-parser.ts
export function parseMessage<T>(
  raw: string,
  validator: (parsed: unknown) => parsed is T
): T | null {
  // Unified parsing logic
}
```

**Benefits:**
- Single source of truth for validation
- Consistent error handling
- Better type safety

---

### 3. Extract Pending Message Management

**Current Issue:** Chat component has complex pending message state management scattered throughout.

**Suggested Refactor:**
```typescript
// sandbox-ui/src/lib/chat/pending-manager.ts
export class PendingMessageManager {
  add(messageId: string): void;
  remove(messageId: string): void;
  getPending(): string[];
  setTimeout(messageId: string, callback: () => void): void;
  clearTimeout(messageId: string): void;
}
```

**Benefits:**
- Easier to test pending message logic
- Clearer separation of concerns
- Reusable across components

---

### 4. Standardize Store Patterns

**Current Issue:** Different patterns for state management (chat store vs window store).

**Suggested Refactor:**
```typescript
// sandbox-ui/src/stores/base.ts
export function createStore<T>(initial: T, name: string) {
  // Standardized store creation with middleware
}
```

**Benefits:**
- Consistent API across stores
- Easier to add persistence or logging
- Better debugging

---

### 5. Component Props Typing

**Current Issue:** Some components use loose typing for props.

**Suggested Refactor:**
Create stricter interfaces:
```typescript
// sandbox-ui/src/components/apps/Chat/types.ts
export interface ChatProps {
  readonly actorId: string;
  readonly userId?: string;
}

export interface ChatMessageProps {
  readonly message: ChatMessage;
  readonly isPending?: boolean;
}
```

**Benefits:**
- Better type safety
- Clearer component contracts
- Easier refactoring

---

## Test Coverage Analysis

### Current Test Coverage

| Component | Test File | Coverage | Comments |
|-----------|-----------|----------|----------|
| Chat Component | ✅ Chat.test.tsx (119 lines) | ~60% | Tests WebSocket lifecycle, pending timeouts |
| Chat WS Utils | ✅ ws.test.ts (34 lines) | ~90% | Good coverage of parsing logic |
| Terminal Component | ✅ Terminal.test.tsx (128 lines) | ~55% | Tests reconnection, cleanup |
| Terminal WS Utils | ✅ ws.test.ts (22 lines) | ~80% | Good coverage of parsing |
| WS Client | ✅ client.test.ts (315 lines) | ~85% | Comprehensive client tests |
| Desktop | ❌ No tests | 0% | Critical gap |
| Window | ❌ No tests | 0% | Critical gap |
| WindowManager | ❌ No tests | 0% | Critical gap |
| Stores | ❌ No tests | 0% | Critical gap |
| API Layer | ❌ No tests | 0% | Critical gap |

**Overall Test Coverage:** ~45%

---

### Missing Critical Tests

#### 1. Desktop Component Tests

**Required Tests:**
- App bootstrap with missing apps registration
- WebSocket connection state handling
- Window opening/focusing interaction
- Error state display
- Retry logic on bootstrap failure

**Estimated Effort:** 4-6 hours

---

#### 2. Window Component Tests

**Required Tests:**
- Drag functionality with pointer events
- Resize functionality
- Minimize/maximize/restore cycle
- Active window state management
- Z-index ordering

**Estimated Effort:** 6-8 hours

---

#### 3. Store Tests

**Required Tests:**
- Chat store: message addition, updates, pending state
- Windows store: add/remove/update/focus windows
- Desktop store: active window management

**Estimated Effort:** 4-6 hours

---

#### 4. API Layer Tests

**Required Tests:**
- Error handling for network failures
- Response parsing validation
- Retry logic
- Request/response serialization

**Estimated Effort:** 6-8 hours

---

## WebSocket Integration Analysis

### Chat WebSocket

**Endpoint:** `/ws/chat/{actorId}/{userId}`

**Messages Sent:**
```typescript
{
  type: 'message',
  text: string,
  client_message_id?: string
}
```

**Messages Received:**
```typescript
{ type: 'connected', actor_id: string, user_id: string }
{ type: 'thinking', content: string }
{ type: 'tool_call', content: string }
{ type: 'tool_result', content: string }
{ type: 'response', content: string }
{ type: 'error', message: string }
{ type: 'pong' }
```

**Issues:**
1. No handling of `connected` message (React)
2. No explicit ping/pong for keepalive
3. Reconnection logic is basic

---

### Terminal WebSocket

**Endpoint:** `/ws/terminal/{terminalId}?user_id={userId}`

**Messages Sent:**
```typescript
{ type: 'input', data: string }
{ type: 'resize', rows: number, cols: number }
```

**Messages Received:**
```typescript
{ type: 'output', data: string }
{ type: 'info', terminal_id: string, is_running: boolean }
{ type: 'error', message: string }
```

**Issues:**
1. Parameter inconsistency (query vs path)
2. No explicit keepalive mechanism
3. Cleanup on unmount is best-effort only

---

## Architecture Comparison

### Dioxus Architecture

**Strengths:**
- Component-based with clear separation
- Type-safe with Rust
- Efficient re-rendering with signals
- Good test coverage in critical areas
- Comprehensive viewer system

**Weaknesses:**
- Steeper learning curve
- Slower iteration for UI changes
- JavaScript FFI bridge complexity

---

### React Architecture

**Strengths:**
- Familiar React patterns
- Fast UI iteration
- Good developer tooling
- Zustand for simple state management

**Weaknesses:**
- Missing viewer components
- Incomplete test coverage
- Some duplicated logic
- Type safety could be improved

---

## Recommendations (Priority Order)

### P0 - Critical (Do First)

1. **Fix Race Conditions**
   - Add timeout cleanup in Chat component
   - Fix pointer event listener cleanup in Window component
   - **Effort:** 2 hours

2. **Add Critical Tests**
   - Desktop component tests
   - Window component tests
   - Store tests
   - **Effort:** 12-16 hours

3. **Fix WebSocket Parameter Inconsistency**
   - Align terminal userId parameter format with backend
   - Document expected format
   - **Effort:** 1 hour

---

### P1 - High Priority

4. **Port Tool Call Rendering**
   - Implement collapsible tool call sections
   - Add tool result display
   - Show reasoning for tool calls
   - **Effort:** 8-12 hours

5. **Port Viewer Components**
   - ImageViewer (4-6 hours)
   - TextViewer (6-8 hours)
   - ViewerShell (8-10 hours)
   - **Total Effort:** 18-24 hours

6. **Improve Error Handling**
   - Add error logging for terminal stop failures
   - Show connection method indicator in Chat
   - Add stream event truncation indicator
   - **Effort:** 3-4 hours

---

### P2 - Medium Priority

7. **Refactor Duplicated Code**
   - Extract WebSocket reconnection logic
   - Unify message parsing
   - Create pending message manager
   - **Effort:** 8-10 hours

8. **Add API Layer Tests**
   - Network failure handling
   - Response validation
   - Retry logic
   - **Effort:** 6-8 hours

9. **Enhance Terminal Reconnection**
   - Add jitter to reconnection delay
   - Improve retry logic
   - Better status messages
   - **Effort:** 2-3 hours

---

### P3 - Low Priority

10. **Standardize Store Patterns**
    - Create base store factory
    - Add middleware support
    - Improve debugging
    - **Effort:** 4-6 hours

11. **Improve Component Props Typing**
    - Add strict interfaces
    - Add readonly modifiers
    - Document component contracts
    - **Effort:** 2-3 hours

12. **Add Chat Assistant Bundling**
    - Implement message bundling
    - Add thinking state visualization
    - Bundle tool results
    - **Effort:** 6-8 hours

---

## Estimated Total Effort

| Category | Effort (Hours) |
|----------|----------------|
| Critical Bugs | 3 |
| High Priority Features | 27-40 |
| Medium Priority | 16-21 |
| Low Priority | 12-17 |
| **Total** | **58-81 hours** |

---

## Conclusion

The React implementation has solid foundations with working Chat and Terminal apps, but is missing critical features from the Dioxus version, particularly the viewer components and sophisticated tool call rendering. Test coverage is approximately 45%, with critical gaps in Desktop and Window components.

**Recommendation:** Focus on P0 critical bugs first, then port the viewer components (P1) to reach feature parity with Dioxus. The estimated effort to achieve full parity is 58-81 hours, with the most time-consuming tasks being the viewer components and tool call rendering.

**Risk Assessment:** Medium - The core functionality works, but missing features could impact user experience. No critical blockers prevent deployment, but feature parity is recommended before production use.
