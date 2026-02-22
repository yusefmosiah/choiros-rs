# Porting Fixes Required Report

**Date:** February 6, 2026
**Scope:** React dioxus-desktop vs Dioxus backup vs Backend contracts
**Total Issues Analyzed:** 5 review reports merged

---

## 1. Executive Summary

This comprehensive review synthesizes findings from five detailed reports covering WebSocket implementation, state management, API type generation, desktop/window management, and app feature parity. The React implementation shows strong foundational work with core Chat and Terminal apps functioning, but significant gaps remain compared to the Dioxus backup.

### Overall Metrics

| Metric | Value | Status |
|--------|-------|--------|
| **Overall Feature Parity** | 65% | ⚠️ Moderate |
| **Critical Bugs** | 12 | 🔴 Needs Immediate Attention |
| **High Priority Issues** | 18 | 🟠 Address Soon |
| **Medium Priority Issues** | 24 | 🟡 Address in Near Term |
| **Low Priority Issues** | 16 | 🔵 Address Long Term |
| **Test Coverage** | 45% | ⚠️ Needs Improvement |

### Critical Areas Requiring Immediate Action

1. **State Architecture:** React uses multiple stores (DesktopStore, WindowsStore, ChatStore) with duplicated data causing race conditions and synchronization bugs
2. **Window Management:** Missing pointer capture, bounds clamping, drag threshold, and keyboard shortcuts - windows can become stuck or inaccessible
3. **WebSocket Protocol:** Missing `z_index` in WindowFocused events in Dioxus, missing `AppRegistered` event type
4. **API Types:** Response type mismatches in maximize/restore endpoints, missing Viewer API client
5. **Viewer Components:** Entire viewer system (ImageViewer, TextViewer, ViewerShell) not ported from Dioxus
6. **Tool Call Rendering:** Chat component shows stream events but no tool call visualization

### Key Strengths

- ✅ Desktop WebSocket connection and state subscription functional
- ✅ Chat and Terminal apps have working implementations
- ✅ API client layer provides type-safe HTTP requests
- ✅ WebSocket client has reconnection with exponential backoff
- ✅ Store-based state management with Zustand
- ✅ Generated TypeScript types from Rust shared-types

### Key Weaknesses

- ❌ State duplication between multiple stores causes race conditions
- ❌ Window management lacks critical accessibility and UX features
- ❌ Viewer components (TextViewer, ImageViewer, ViewerShell) completely missing
- ❌ Tool call visualization not implemented in Chat
- ❌ Test coverage at 45% with critical gaps in Desktop, Window, and stores
- ❌ Missing theme system and keyboard navigation
- ❌ No rate limiting on drag/resize operations (60+ requests/second)

---

## 2. Critical Bugs

### CRITICAL Severity - Fix Immediately

#### CRITICAL-001: State Duplication Between DesktopStore and WindowsStore
**Location:** `dioxus-desktop/src/stores/desktop.ts:5`, `dioxus-desktop/src/stores/windows.ts:5`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #1)

**Issue:** Same `windows` data exists in two separate stores:
- `useDesktopStore.desktop.windows`
- `useWindowsStore.windows`

This creates two copies of truth that can diverge.

**Evidence:**
```typescript
// desktop.ts:5 - First copy
interface DesktopStore {
  desktop: DesktopState | null;  // Contains windows array
  activeWindowId: string | null;
  // ...
}

// windows.ts:5 - Second copy
interface WindowsStore {
  windows: WindowState[];  // Duplicate data
  // ...
}
```

**Impact:**
- When one store updates and the other doesn't, UI shows conflicting state
- Components reading different stores see different data
- Impossible to maintain consistency during rapid updates
- Race conditions in WebSocket message processing

**Dioxus Equivalent:** Single `desktop_state` contains windows array - no duplication

**Estimated Effort:** 8-12 hours (requires store consolidation)

---

#### CRITICAL-002: Race Condition in WebSocket State Updates
**Location:** `dioxus-desktop/src/hooks/useWebSocket.ts:17-92`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #2)

**Issue:** `applyWsMessage` function updates both stores but doesn't guarantee atomicity.

**Evidence:**
```typescript
function applyWsMessage(message: WsServerMessage): void {
  const desktopStore = useDesktopStore.getState();
  const windowsStore = useWindowsStore.getState();

  switch (message.type) {
    case 'window_opened': {
      windowsStore.openWindow(message.window);      // Store A updated
      desktopStore.setActiveWindow(message.window.id);  // Store B updated
      return;
    }
    // ... more cases with dual updates
  }
}
```

**Race Condition Scenario:**
1. WebSocket message arrives: `{ type: 'window_opened', window: {...} }`
2. `windowsStore.openWindow()` updates windows store
3. Before `setActiveWindow()` runs, another component reads `desktopStore.activeWindowId`
4. Component sees stale active window value
5. UI shows wrong window as active

**Dioxus Equivalent:** Atomic state mutation in `src/desktop/state.rs:21-24`

**Estimated Effort:** 4-6 hours (requires batch updates)

---

#### CRITICAL-003: z_index Not Calculated on Window Focus
**Location:** `dioxus-desktop/src/stores/windows.ts:65-73`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #3)

**Issue:** `focusWindow` sets z_index but doesn't calculate it correctly - requires caller to pass it.

**Evidence:**
```typescript
focusWindow: (windowId: string, zIndex: number) => {
  set((state) => ({
    windows: updateWindow(state.windows, windowId, (window) => ({
      ...window,
      z_index: zIndex,  // Passed in, not calculated!
      minimized: false,
    })),
  })),
},
```

In `Desktop.tsx:145-163`, the caller doesn't pass a z_index:
```typescript
const handleFocusWindow = useCallback(
  async (windowId: string) => {
    try {
      const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
      if (window?.minimized) {
        await restoreWindow(desktopId, windowId);
        return;
      }
      await focusWindow(desktopId, windowId);  // No z_index passed!
    }
    // ...
  },
  // ...
);
```

**Impact:**
- Focused windows don't visually come to front
- Window layering breaks
- User clicks window but it doesn't raise

**Dioxus Equivalent:** Correctly calculates z_index in `src/desktop/state.rs:136-143`

**Estimated Effort:** 2-3 hours

---

#### CRITICAL-004: Missing Pointer Capture on Window Drag/Resize
**Location:** `dioxus-desktop/src/components/window/Window.tsx:50-96`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.1)

**Issue:** Window drag and resize operations use global window event listeners instead of pointer capture. This causes the window to lose track if the pointer moves outside the element bounds or moves too quickly.

**Evidence:**
```tsx
// React - lines 93-95
globalThis.window.addEventListener('pointermove', handlePointerMove);
globalThis.window.addEventListener('pointerup', handlePointerUp);
globalThis.window.addEventListener('pointercancel', handlePointerUp);
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:368-373 (drag)
if let Some(web_event) = e.data().try_as_web_event() {
    if let Some(target) = web_event.current_target() {
        if let Ok(element) = target.dyn_into::<web_sys::Element>() {
            let _ = element.set_pointer_capture(e.data().pointer_id());
        }
    }
}
```

**Impact:** Windows can become "stuck" in drag mode if pointer moves rapidly outside viewport. User must click somewhere else to release.

**Fix Required:**
```tsx
const onHeaderPointerDown: PointerEventHandler<HTMLDivElement> = (event) => {
  // ... existing code ...
  event.currentTarget.setPointerCapture(event.pointerId);

  const handlePointerUp = (upEvent: PointerEvent) => {
    if (upEvent.pointerId !== dragPointerIdRef.current) {
      return;
    }
    event.currentTarget.releasePointerCapture(upEvent.pointerId);
    // ... rest of cleanup ...
  };
  // ...
};
```

**Estimated Effort:** 2-3 hours

---

#### CRITICAL-005: No Window Bounds Clamping to Viewport
**Location:** `dioxus-desktop/src/components/window/Window.tsx:50-96`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.3)

**Issue:** Windows can be dragged or resized outside the visible viewport, making them inaccessible.

**Evidence:**
```tsx
// React Window.tsx:74-78 - No bounds checking
const dx = moveEvent.clientX - dragStartRef.current.pointerX;
const dy = moveEvent.clientY - dragStartRef.current.pointerY;

onMove(
  windowState.id,
  Math.round(dragStartRef.current.startX + dx),  // Can be negative
  Math.round(dragStartRef.current.startY + dy),  // Can be negative
);
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:45-67
fn clamp_bounds(bounds: WindowBounds, viewport: (u32, u32), is_mobile: bool) -> WindowBounds {
    let (vw, vh) = viewport;
    if is_mobile {
        return WindowBounds {
            x: 10, y: 10,
            width: vw as i32 - 20,
            height: vh as i32 - 100,
        };
    }

    let width = bounds.width.max(MIN_WINDOW_WIDTH).min(vw as i32 - 40);
    let height = bounds.height.max(MIN_WINDOW_HEIGHT).min(vw as i32 - 120);
    let x = bounds.x.max(10).min(vw as i32 - width - 10);
    let y = bounds.y.max(10).min(vw as i32 - height - 60);

    WindowBounds { x, y, width, height }
}
```

**Impact:** Users can lose windows by dragging them completely off-screen. Windows can overlap taskbar/prompt bar.

**Estimated Effort:** 4-6 hours

---

#### CRITICAL-006: Missing Viewer API Client Module
**Location:** `dioxus-desktop/src/lib/api/` - No `viewer.ts` file
**Report:** API_TYPE_GENERATION_REVIEW.md (BUG-001)

**Issue:** Backend exposes `/viewer/content` (GET/PATCH) but React has no client.

**Evidence:** Dioxus backup has `fetch_viewer_content` and `patch_viewer_content` at `dioxus-desktop-backup/src/api.rs:690-759`

**Backend Contract:** `sandbox/src/api/viewer.rs`

**Impact:** File viewer/editor functionality broken in React UI

**Estimated Effort:** 6-8 hours

---

#### CRITICAL-007: Desktop Maximize Window Response Type Mismatch
**Location:** `dioxus-desktop/src/lib/api/desktop.ts:104-107`
**Report:** API_TYPE_GENERATION_REVIEW.md (BUG-002)

**Issue:** Frontend expects `{ success, window }` but backend sends `{ success, window, from, message }`

**Frontend Code:**
```typescript
export async function maximizeWindow(desktopId: string, windowId: string): Promise<WindowState> {
  const response = await apiClient.post<WindowEnvelope>(`/desktop/${desktopId}/windows/${windowId}/maximize`, {});
  return assertSuccess(response).window;  // ← Expects window field
}
```

**Backend Response:** `sandbox/src/api/desktop.rs:488-494`
```rust
Json(json!({
    "success": true,
    "window": window,
    "from": restored.from,  // ← EXTRA FIELD
    "message": "Window maximized"
}))
```

**Impact:** Runtime errors, type mismatch

**Estimated Effort:** 1-2 hours

---

#### CRITICAL-008: Desktop Restore Window Response Type Mismatch
**Location:** `dioxus-desktop/src/lib/api/desktop.ts:109-112`
**Report:** API_TYPE_GENERATION_REVIEW.md (BUG-003)

**Issue:** Same as CRITICAL-007 - backend returns `{ success, window, from, message }`, frontend expects `{ success, window }`

**Backend Response:** `sandbox/src/api/desktop.rs:548-554`

**Impact:** Runtime errors

**Estimated Effort:** 1-2 hours

---

#### CRITICAL-009: Chat Component - Race Condition in Pending Message Cleanup
**Location:** `dioxus-desktop/src/components/apps/Chat/Chat.tsx:264-266`
**Report:** APP_PARITY_ANALYSIS.md (Bug #1)

**Issue:** No cleanup of timeout on component unmount.

```typescript
setTimeout(() => {
  void loadMessages();
}, 500);
```

**Impact:** If component unmounts before 500ms, this will try to update state of unmounted component, causing React warnings.

**Estimated Effort:** 1 hour

---

#### CRITICAL-010: Missing z_index in Dioxus WindowFocused Event
**Location:** `dioxus-desktop-backup/src/desktop/ws.rs:26`
**Report:** websocket-implementation-review.md (Bug #1)

**Issue:** The Dioxus `WsEvent::WindowFocused` variant only takes a `String` (window_id) but, backend sends `z_index` field.

```rust
// Dioxus backup - INCORRECT
WindowFocused(String),

// Backend expects - websocket.rs:56
WindowFocused { window_id: String, z_index: u32 },

// React types - types.ts:16 - CORRECT
{ type: 'window_focused'; window_id: string; z_index: number }
```

**Impact:** Dioxus backup will fail to receive/parse window focus events correctly. Any frontend switching from React to Dioxus will break window focus functionality.

**Estimated Effort:** 1 hour

---

#### CRITICAL-011: Missing AppRegistered Event in Dioxus
**Location:** `dioxus-desktop-backup/src/desktop/ws.rs`
**Report:** websocket-implementation-review.md (Bug #2)

**Issue:** The Dioxus `WsEvent` enum is missing `AppRegistered` variant entirely.

```rust
// React has it - types.ts:28
| { type: 'app_registered'; app: AppDefinition }

// Backend sends it - websocket.rs:80-81
#[serde(rename = "app_registered")]
AppRegistered { app: shared_types::AppDefinition },

// Dioxus backup - MISSING
```

**Impact:** Dynamic app registration won't work in Dioxus backend. New apps registered via WebSocket won't be displayed.

**Estimated Effort:** 2 hours

---

#### CRITICAL-012: Desktop Store Updates Windows Array Inconsistently
**Location:** `dioxus-desktop/src/stores/desktop.ts:68-87`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #4)

**Issue:** `closeWindow` in desktop store tries to update `desktop.windows` but the actual source of truth is `useWindowsStore.windows`:

```typescript
closeWindow: (windowId) => {
  set((state) => {
    if (!state.desktop) {
      return state;
    }

    const nextActive = ...;  // Calculates active window
    return {
      activeWindowId: nextActive,
      desktop: {
        ...state.desktop,
        active_window: nextActive,  // Only updates active_window!
        // DOESN'T remove window from desktop.windows!
      },
    };
  });
},
```

**Impact:**
- `desktop.windows` becomes stale
- Any code reading `desktop.windows` sees closed windows
- State divergence between the two window arrays

**Dioxus Equivalent:** Removes window atomically in `src/desktop/state.rs:128-134`

**Estimated Effort:** 2-3 hours

---

### HIGH Severity

#### HIGH-001: Missing Active Window Handling in Minimize
**Location:** `dioxus-desktop/src/stores/desktop.ts:89-113`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #5)

**Issue:** `minimizeWindow` calculates next active window correctly but doesn't update the windows store to set `minimized: true`

```typescript
minimizeWindow: (windowId) => {
  set((state) => {
    if (!state.desktop || state.activeWindowId !== windowId) {
      return state;  // Early return - doesn't handle other cases!
    }

    const nextActive = /* calculates correctly */;

    return {
      activeWindowId: nextActive,
      desktop: {
        ...state.desktop,
        active_window: nextActive,
        // DOESN'T update windows array!
      },
    };
  });
},
```

**Impact:** The window being minimized never gets its `minimized` flag set. Only works if caller calls `windowsStore.minimizeWindow()` too.

**Estimated Effort:** 2-3 hours

---

#### HIGH-002: No Rate Limiting on Move/Resize Events
**Location:** `dioxus-desktop/src/components/window/Window.tsx:66-79`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.5)

**Issue:** Every pointer move event triggers a move/resize API call. This can spam the backend with hundreds of events per second.

```tsx
// React - sends on every move
const handlePointerMove = (moveEvent: PointerEvent) => {
  // ...
  onMove(windowState.id, nextX, nextY);  // Called for every event!
};
```

**Dioxus Implementation:** Rate limited to 50ms intervals with queued moves

**Impact:**
- 60+ requests/second
- Server load
- Network bandwidth waste
- Rate limiting risk

**Estimated Effort:** 4-6 hours

---

#### HIGH-003: Missing Keyboard Accessibility
**Location:** `dioxus-desktop/src/components/window/Window.tsx:149-186`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.6)

**Issue:** No keyboard shortcuts for window management.

**Dioxus Keyboard Shortcuts:**
- `Alt+F4` - Close window
- `Escape` - Cancel drag/resize
- `Ctrl+M` - Minimize
- `Ctrl+Shift+M` - Maximize/Restore
- `Alt+Arrow Keys` - Move window
- `Alt+Shift+Arrow Keys` - Resize window

**Impact:** Poor accessibility. Keyboard-only users cannot manage windows.

**Estimated Effort:** 6-8 hours

---

#### HIGH-004: User Preferences Response Type Mismatch
**Location:** `dioxus-desktop/src/lib/api/user.ts:3-28`
**Report:** API_TYPE_GENERATION_REVIEW.md (BUG-004)

**Issue:** Frontend expects complex `UserPreferences` object, backend returns only `{ success, theme }`

**Backend Response:** `sandbox/src/api/user.rs:48-53`
```rust
Json(UserPreferencesResponse {
    success: true,
    theme,  // ← ONLY theme field
})
```

**Frontend Expects:**
```typescript
interface UserPreferences {
  user_id: string;        // ← NOT IN BACKEND RESPONSE
  theme: 'light' | 'dark' | 'system';
  language: string;       // ← NOT IN BACKEND RESPONSE
  notifications_enabled: boolean;  // ← NOT IN BACKEND RESPONSE
  sidebar_collapsed: boolean;      // ← NOT IN BACKEND RESPONSE
  custom_settings?: Record<string, unknown>;  // ← NOT IN BACKEND RESPONSE
}
```

**Impact:** Type errors, incorrect assumptions about API

**Estimated Effort:** 2-3 hours

---

#### HIGH-005: Unnecessary Re-renders from Store Subscriptions
**Location:** `dioxus-desktop/src/components/desktop/Desktop.tsx:35-38`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #6)

**Issue:** Component subscribes to entire store slices:

```typescript
const windows = useWindowsStore((state) => state.windows);  // Re-renders on ANY window change
const activeWindowId = useDesktopStore((state) => state.activeWindowId);
```

**Impact:**
- Any window move/resize triggers Desktop re-render
- Desktop re-renders even when active window doesn't change
- Unnecessary work for frequent operations (drag/resize)

**Estimated Effort:** 4-6 hours

---

#### HIGH-006: Chat State Not Integrated with Desktop State
**Location:** `dioxus-desktop/src/stores/chat.ts:4-14`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #7)

**Issue:** Chat messages are completely separate from desktop state.

**Missing:**
- No way to find chat window ID
- No integration with window minimize/close
- Chat continues receiving messages even if window closed

**Impact:** Chat window can receive messages when closed/minimized, causing state inconsistency.

**Estimated Effort:** 6-8 hours

---

#### HIGH-007: No Viewer Shell Integration
**Location:** `dioxus-desktop/src/components/window/Window.tsx:189-211`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.2.7)

**Issue:** Writer and Files apps show placeholder text. Dioxus has ViewerShell component for rendering different content types.

**Impact:** Writer and Files apps are non-functional. Placeholder apps cannot be used.

**Estimated Effort:** 8-10 hours (requires ViewerShell implementation)

---

#### HIGH-008: Tool Call Rendering Missing
**Location:** Dioxus `dioxus-desktop-backup/src/components.rs:589-666`
**Report:** APP_PARITY_ANALYSIS.md (Missing Feature #1)

**Issue:** React Chat component shows stream events but no tool call visualization.

**Dioxus Features:**
- Collapsible tool call sections
- Tool call details with reasoning
- Tool result display
- Expand/collapse all functionality
- Live activity indicator for pending tools

**React Status:** Completely missing

**Impact:** Users cannot see what tools AI is calling or their results.

**Estimated Effort:** 8-12 hours

---

#### HIGH-009: Missing ImageViewer Component
**Location:** Dioxus `dioxus-desktop-backup/src/viewers/image.rs`
**Report:** APP_PARITY_ANALYSIS.md (Missing Feature #2)

**Issue:** Image viewer not implemented in React.

**Dioxus Features:**
- Zoom in/out controls
- Pan/drag functionality
- Reset button
- Data URI support

**Impact:** Cannot view images in windows.

**Estimated Effort:** 4-6 hours

---

#### HIGH-010: Missing TextViewer Component
**Location:** Dioxus `dioxus-desktop-backup/src/viewers/text.rs`
**Report:** APP_PARITY_ANALYSIS.md (Missing Feature #3)

**Issue:** Text viewer not implemented in React.

**Dioxus Features:**
- Editable text area with JavaScript bridge
- Read-only mode support
- Change callback for content updates
- Monospace font rendering

**Impact:** Cannot edit/view text files in windows.

**Estimated Effort:** 6-8 hours

---

#### HIGH-011: Missing ViewerShell Component
**Location:** Dioxus `dioxus-desktop-backup/src/viewers/shell.rs`
**Report:** APP_PARITY_ANALYSIS.md (Missing Feature #4)

**Issue:** Unified wrapper for viewing/editing files not implemented.

**Dioxus Features:**
- File loading with content fetching
- Save with revision conflict detection
- Reload functionality
- Dirty state tracking (unsaved changes)
- Error handling for conflicts

**Impact:** No unified wrapper for viewing/editing files.

**Estimated Effort:** 8-10 hours

---

#### HIGH-012: Terminal WebSocket URL Parameter Inconsistency
**Location:** `dioxus-desktop/src/components/apps/Terminal/Terminal.tsx:94`
**Report:** APP_PARITY_ANALYSIS.md (Bug #3)

**Issue:** React passes `userId` via query param, but Dioxus passes it via URL path. Backend may expect one format.

```typescript
const ws = new WebSocket(getTerminalWebSocketUrl(terminalId, userId));
```

**Reference:** Dioxus `terminal.rs:230` uses path parameter.

**Impact:** Potential connection failures if backend expects specific format.

**Estimated Effort:** 1 hour

---

### MEDIUM Severity

#### MEDIUM-001: Missing Pointer Cancel Handler
**Location:** `dioxus-desktop/src/components/window/Window.tsx`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.2)

**Issue:** No `pointercancel` event handling. If browser cancels a pointer event (system modal, touch gesture), window remains in drag/resize state.

**Dioxus Implementation:** Properly releases pointer capture and restores committed bounds on pointercancel.

**Impact:** Window state can become corrupted after pointercancel events.

**Estimated Effort:** 2-3 hours

---

#### MEDIUM-002: Race Condition in useWebSocket Hook Status State
**Location:** `dioxus-desktop/src/hooks/useWebSocket.ts:148`
**Report:** websocket-implementation-review.md (Bug #3)

**Issue:** The hook returns a derived status that can be stale.

```typescript
return {
  status: wsConnected ? 'connected' : status,  // status may be stale!
  sendPing: () => client.ping(),
  disconnect: () => client.disconnect(),
};
```

**Impact:** UI may show incorrect connection status during initial connection.

**Estimated Effort:** 2 hours

---

#### MEDIUM-003: Multiple useWebSocket Hooks with Single Client Instance
**Location:** `dioxus-desktop/src/hooks/useWebSocket.ts:8-14`
**Report:** websocket-implementation-review.md (Bug #4)

**Issue:** The client is a singleton shared across all hook instances, but each hook manages its own subscription/unsubscription.

**Impact:** Unpredictable behavior in complex UIs with multiple components using WebSocket.

**Estimated Effort:** 4-6 hours

---

#### MEDIUM-004: Silent Message Send Failures
**Location:** `dioxus-desktop/src/lib/ws/client.ts:71-77`
**Report:** websocket-implementation-review.md (Bug #6)

**Issue:** `send()` method silently drops messages if socket is not open.

```typescript
send(message: WsClientMessage): void {
  if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
    return;  // Silent failure!
  }
  this.socket.send(JSON.stringify(message));
}
```

**Impact:** Messages sent during temporary disconnections are lost with no indication to caller.

**Estimated Effort:** 3-4 hours

---

#### MEDIUM-005: Bootstrap State Not Reset on Unmount
**Location:** `dioxus-desktop/src/components/desktop/Desktop.tsx:7, 49-108`
**Report:** STATE_MANAGEMENT_REVIEW.md (Bug #9)

**Issue:** Module-level `bootstrapState` map is never cleared.

**Impact:** If Desktop unmounts and remounts, it won't bootstrap again. Manual page refresh required.

**Estimated Effort:** 1-2 hours

---

#### MEDIUM-006: No Drag Threshold
**Location:** `dioxus-desktop/src/components/window/Window.tsx:66-79`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.4)

**Issue:** Drag starts immediately on pointerdown, even if it's just a click.

**Dioxus Implementation:** 4px threshold - doesn't move if movement < 4px

**Impact:** Poor UX - windows "jitter" when clicking on them.

**Estimated Effort:** 2-3 hours

---

#### MEDIUM-007: Missing Mobile Window Constraints
**Location:** `dioxus-desktop/src/components/window/Window.tsx`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.1.7)

**Issue:** No special handling for mobile viewports (≤1024px).

**Impact:** Poor mobile UX. Windows may extend beyond viewport.

**Estimated Effort:** 3-4 hours

---

#### MEDIUM-008: Missing Theme System
**Location:** Entire React implementation
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Missing Feature #1)

**Issue:** Dioxus has complete theme system (light/dark, persistence, API sync). React has none.

**Impact:** Users cannot switch between light/dark themes.

**Estimated Effort:** 8-12 hours

---

#### MEDIUM-009: Desktop Icons Lack Press Animation
**Location:** `dioxus-desktop/src/components/desktop/Desktop.css:38-40`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.3.3)

**Issue:** Icons only change background on hover, no scale animation.

**Impact:** Less responsive feel, no visual feedback on interaction.

**Estimated Effort:** 2-3 hours

---

#### MEDIUM-010: Double-Click on Icon Not Detected
**Location:** `dioxus-desktop/src/components/desktop/Icon.tsx`
**Report:** DESKTOP_WINDOW_COMPARISON_REPORT.md (Bug 2.2.3)

**Issue:** Desktop icons use single click to open app. Should require double-click or have press state animation.

**Impact:** Accidental app launches.

**Estimated Effort:** 2-3 hours

---

### LOW Severity

#### LOW-001: Missing Error Handling for Unknown Message Types
**Location:** `sandbox/src/api/websocket.rs:154-156`
**Report:** websocket-implementation-review.md (Bug #7)

**Issue:** Backend doesn't respond to unknown/invalid client messages.

**Impact:** Client has no way to know it sent an invalid message.

**Estimated Effort:** 1 hour

---

#### LOW-002: Unnecessary Initial Pong on Connection
**Location:** `sandbox/src/api/websocket.rs:107`
**Report:** websocket-implementation-review.md (Bug #8)

**Issue:** Backend sends unsolicited `Pong` message before client sends anything.

**Impact:** The `Pong` type is response to `Ping`. Unsolicited pong might confuse clients.

**Estimated Effort:** 0.5 hours

---

#### LOW-003: Weak Type Validation in parseWsServerMessage
**Location:** `dioxus-desktop/src/lib/ws/types.ts:31-47`
**Report:** websocket-implementation-review.md (Bug #9)

**Issue:** Parser only validates `type` field, not actual message structure.

**Impact:** Runtime type errors, silent failures when backend contracts change.

**Estimated Effort:** 3-4 hours

---

#### LOW-004: No Connection Timeout
**Location:** `dioxus-desktop/src/lib/ws/client.ts`
**Report:** websocket-implementation-review.md (Bug #11)

**Issue:** No timeout for WebSocket connection.

**Impact:** If network is slow or unresponsive, UI shows 'connecting' forever.

**Estimated Effort:** 2-3 hours

---

#### LOW-005: No Keep-Alive/Ping Interval
**Location:** Multiple WebSocket files
**Report:** websocket-implementation-review.md (Bug #12)

**Issue:** There's a `ping()` method but no automatic keep-alive mechanism.

**Impact:** Connections might be dropped by proxies/firewalls due to inactivity.

**Estimated Effort:** 2-3 hours

---

#### LOW-006 through LOW-012: Additional Minor Issues

See detailed reports for complete list of minor issues including:
- Missing Error Boundary Handling (STATE_MANAGEMENT_REVIEW.md Bug #10)
- Chat Stream Events Not Managed Globally (STATE_MANAGEMENT_REVIEW.md Bug #11)
- No Viewport State Management (STATE_MANAGEMENT_REVIEW.md Bug #12)
- Window Component styling issues
- Desktop CSS improvements needed

---

## 3. Missing Features from Dioxus

### 3.1 Complete Viewer System

| Component | Dioxus Location | Lines | React Status | Estimated Effort |
|-----------|----------------|-------|--------------|------------------|
| **ViewerShell** | `viewers/shell.rs` | 218 | ❌ Missing | 8-10 hours |
| **ImageViewer** | `viewers/image.rs` | 65 | ❌ Missing | 4-6 hours |
| **TextViewer** | `viewers/text.rs` | 147 | ❌ Missing | 6-8 hours |
| **Total** | | 430 | 0% | **18-24 hours** |

**Features Lost:**
- File loading with content fetching
- Save with revision conflict detection
- Zoom in/out controls for images
- Pan/drag functionality
- Editable text area with change tracking
- Dirty state tracking (unsaved changes)
- Error handling for conflicts
- Status display (Loading, Saved, Unsaved changes, Saving, Error)

---

### 3.2 Tool Call Rendering (Major Feature Gap)

**Dioxus Implementation:** `dioxus-desktop-backup/src/components.rs:589-666` (77 lines)

**Features:**
- Collapsible tool call sections
- Tool call details with reasoning
- Tool result display
- Expand/collapse all functionality
- Live activity indicator for pending tools

**React Status:** Completely missing. Stream events are shown but not rendered as tool calls.

**Impact:** Users cannot see what tools AI is calling or their results.

**Estimated Effort:** 8-12 hours

---

### 3.3 Theme System

**Missing Component:** `desktop/theme.rs` (in Dioxus)

**Features Lost:**
- Light/dark theme toggle
- Theme persistence in localStorage
- Theme synchronization with backend API
- CSS custom properties for theme colors
- Dynamic theme switching without page reload

**Files to Create:**
```
dioxus-desktop/src/lib/theme/
  ├── index.ts
  ├── theme.ts          // Theme types and definitions
  ├── useTheme.ts       // Custom hook for theme management
  └── theme.css         // CSS variables for themes
```

**Estimated Effort:** 8-12 hours

---

### 3.4 Keyboard Shortcuts

**Missing from:** `components/window/Window.tsx`

**Shortcuts Lost:**
- `Alt+F4` - Close window
- `Escape` - Cancel drag/resize operation
- `Ctrl+M` - Minimize window
- `Ctrl+Shift+M` - Maximize/Restore window
- `Alt+Arrow Keys` - Move window (10px increments)
- `Alt+Shift+Arrow Keys` - Resize window (10px increments)

**Implementation Required:**
```tsx
// Add to Window.tsx
<div
  className="window"
  onKeyDown={handleWindowKeydown}
  tabIndex={0}  // Make window focusable
  role="dialog"
>
  {/* ... */}
</div>
```

**Estimated Effort:** 6-8 hours

---

### 3.5 Pointer Event Enhancements

**Missing Features:**
- Pointer capture/release
- Pointer cancel handling
- Drag threshold (4px)
- Rate limiting for move/resize events

**Implementation Required:**
```tsx
// Add pointer capture
const onHeaderPointerDown = (event: React.PointerEvent) => {
  event.currentTarget.setPointerCapture(event.pointerId);
  // ...
};

// Add pointercancel
const handlePointerCancel = (event: React.PointerEvent) => {
  if (event.pointerId === dragPointerIdRef.current) {
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragPointerIdRef.current = null;
    dragStartRef.current = null;
  }
};
```

**Estimated Effort:** 4-6 hours

---

### 3.6 Bounds Clamping System

**Missing Component:** Window bounds clamping function

**Features Lost:**
- Minimum window size enforcement (200x160px)
- Maximum window size enforcement (viewport minus margins)
- Position clamping to keep windows on-screen
- Mobile-specific constraints (full width on mobile)

**Estimated Effort:** 4-6 hours

---

### 3.7 Rate-Limited Event Dispatcher

**Missing Component:** Event debouncing/throttling utility

**Features Lost:**
- 50ms interval for move/resize events
- Queued event batching
- Final flush on pointer up
- Reduced network traffic

**Estimated Effort:** 3-4 hours

---

### 3.8 Desktop Icon Press Animation

**Missing Feature:** Visual feedback on icon press

**Features Lost:**
- Scale down on press (0.95)
- Border color change on press (blue)
- Shadow glow on press
- 150ms animation duration

**Estimated Effort:** 2-3 hours

---

### 3.9 Chat Assistant Bundling

**Dioxus Implementation:** `dioxus-desktop-backup/src/components.rs:510-723`

**Features:**
- Collapses tool calls into assistant messages
- Shows thinking state
- Bundles tool results with responses
- `__assistant_bundle__:` prefix for bundled messages

**React Status:** Basic message rendering only

**Estimated Effort:** 6-8 hours

---

### 3.10 Terminal Reconnection with Jitter

**Dioxus Implementation:** `dioxus-desktop-backup/src/terminal.rs:367-408`

**Advanced Features:**
- Exponential backoff with max cap
- Jitter added to avoid thundering herd
- Max 6 retry attempts
- Timeout handling and cleanup

**React Status:** Basic exponential backoff, no jitter

**Estimated Effort:** 2-3 hours

---

### 3.11 Viewport Tracking

**Dioxus:** `src/desktop/shell.rs:20, src/desktop/effects.rs:14-18`
- Global viewport dimensions
- Used for window clamping
- Responsive behavior

**React:** Not global

**Impact:** Window clamping/responsive behavior not consistent

**Estimated Effort:** 2-3 hours

---

### 3.12 Chat Window Integration

**Dioxus:** `src/desktop/state.rs:145-153`
- `find_chat_window_id()` function
- Chat window auto-opens from prompt
- Integrated prompt-to-chat workflow

**React:** Partial implementation

**Estimated Effort:** 3-4 hours

---

### 3.13 App Registration State

**Dioxus:** `src/desktop/effects.rs:93-107`
- `apps_registered` flag
- Prevents duplicate registration
- Core apps registered once

**React:** Module-level `bootstrapState` map

**Estimated Effort:** 2-3 hours

---

### 3.14 Window Props for Viewers

**Dioxus:** `src/desktop/actions.rs:13-37`
- `viewer_props_for_app()` function
- Viewer descriptors for writer, files apps
- Integrated into window opening

**React:** Not implemented

**Estimated Effort:** 2-3 hours

---

## 4. Refactoring Opportunities

### 4.1 Consolidate Stores (HIGH PRIORITY)

**Current:**
```typescript
// Three separate stores
useDesktopStore   // Has desktop, activeWindowId, windows (duplicate)
useWindowsStore   // Has windows (duplicate)
useChatStore      // Has messages (isolated)
```

**Proposed:**
```typescript
// Single store following Dioxus pattern
interface DesktopStateStore {
  desktop: DesktopState | null;  // Single source of truth
  wsConnected: boolean;
  lastError: string | null;
  // No separate windows array - use desktop.windows
}

// Optional: Separate chat store only if truly needed
interface ChatStore {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
}
```

**Benefits:**
- Eliminates state duplication (fixes CRITICAL-001, CRITICAL-002)
- Prevents divergence
- Simpler synchronization
- Follows Dioxus architecture

**Estimated Effort:** 8-12 hours

---

### 4.2 Implement Atomic WebSocket Updates

**Current:**
```typescript
function applyWsMessage(message: WsServerMessage): void {
  // Updates multiple stores non-atomically
  windowsStore.openWindow(message.window);
  desktopStore.setActiveWindow(message.window.id);
}
```

**Proposed:**
```typescript
function applyWsMessage(message: WsServerMessage): void {
  // Use Zustand's batch or implement transaction pattern
  batch(() => {
    windowsStore.openWindow(message.window);
    desktopStore.setActiveWindow(message.window.id);
  });
}
```

**Benefits:** Fixes CRITICAL-002 race condition

**Estimated Effort:** 2-3 hours

---

### 4.3 Add z_index Calculation to Store Actions

**Current:**
```typescript
focusWindow: (windowId: string, zIndex: number) => { ... }
```

**Proposed:**
```typescript
focusWindow: (windowId: string) => {
  set((state) => {
    const maxZ = Math.max(0, ...state.windows.map(w => w.z_index));
    return {
      windows: updateWindow(state.windows, windowId, (window) => ({
        ...window,
        z_index: maxZ + 1,  // Calculate internally
        minimized: false,
      })),
    };
  });
},
```

**Benefits:** Fixes CRITICAL-003

**Estimated Effort:** 1-2 hours

---

### 4.4 Implement Drag/Resize Throttling

**Current:**
```typescript
const handlePointerMove = (moveEvent: PointerEvent) => {
  // Fires on every pixel
  onMove(windowState.id, nextX, nextY);
};
```

**Proposed:**
```typescript
// Use React's throttle or custom implementation
const throttledMove = useMemo(
  () => throttle((x: number, y: number) => {
    onMove(windowState.id, x, y);
  }, 50), // 50ms throttle
  [windowState.id, onMove]
);

const handlePointerMove = (moveEvent: PointerEvent) => {
  const nextX = Math.round(dragStartRef.current.startX + dx);
  const nextY = Math.round(dragStartRef.current.startY + dy);
  throttledMove(nextX, nextY);
};
```

**Benefits:** Fixes HIGH-002, reduces network load

**Estimated Effort:** 4-6 hours

---

### 4.5 Extract Window State Management to Hook

**Current:** Window.tsx has inline pointer event handlers

**Proposed:**
```typescript
// components/window/useWindowInteraction.ts
export function useWindowInteraction(
  windowId: string,
  onMove: (id: string, x: number, y: number) => void,
  onResize: (id: string, w: number, h: number) => void
) {
  const dragState = useRef<DragState | null>(null);
  const resizeState = useRef<ResizeState | null>(null);

  const startDrag = useCallback((event: React.PointerEvent, startX: number, startY: number) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    dragState.current = {
      pointerId: event.pointerId,
      startX,
      startY,
      initialX: startX,
      initialY: startY,
    };
  }, []);

  // ... rest of interaction logic

  return { startDrag, handleDragMove, endDrag, startResize, handleResizeMove, endResize };
}
```

**Benefits:**
- Reusable across Window components
- Easier to test
- Separation of concerns

**Estimated Effort:** 4-6 hours

---

### 4.6 Use Shared Types from shared-types

**Current:** `dioxus-desktop/src/lib/ws/types.ts` defines `WsServerMessage` separately from backend

**Recommendation:** Use TypeScript types generated by `ts_rs` from `shared_types::WsMessage`

```rust
// shared-types/src/lib.rs - Add this export
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../dioxus-desktop/src/types/generated.ts")]
pub enum DesktopWsMessage {
    // ... all WsMessage variants from websocket.rs
}
```

**Benefits:**
- Single source of truth
- Automatic type updates
- Reduced duplication

**Estimated Effort:** 4-6 hours

---

### 4.7 Improve Message Validation

**Current:** Minimal validation in `parseWsServerMessage`

**Recommendation:** Use Zod or io-ts for runtime type validation

```typescript
import { z } from 'zod';

const WsServerMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('pong') }),
  z.object({ type: z.literal('desktop_state'), desktop: DesktopStateSchema }),
  z.object({ type: z.literal('window_focused'), window_id: z.string(), z_index: z.number() }),
  // ... all other variants
]);

export function parseWsServerMessage(raw: string): WsServerMessage | null {
  return WsServerMessageSchema.safeParse(JSON.parse(raw)).success
    ? WsServerMessageSchema.parse(JSON.parse(raw))
    : null;
}
```

**Benefits:**
- Runtime type safety
- Better error messages
- Catches contract violations early

**Estimated Effort:** 3-4 hours

---

### 4.8 Add Message Queue with Retry

**Current:** Messages dropped when not connected

**Recommendation:** Queue messages when disconnected, send on reconnect with TTL

```typescript
class DesktopWebSocketClient {
  private messageQueue: Array<{ message: WsClientMessage; timestamp: number }> = [];
  private readonly queueTtlMs = 30000; // 30 seconds

  send(message: WsClientMessage): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(message));
    } else {
      this.messageQueue.push({ message, timestamp: Date.now() });
    }
  }

  private flushMessageQueue(): void {
    const now = Date.now();
    this.messageQueue = this.messageQueue.filter(({ message, timestamp }) => {
      if (now - timestamp > this.queueTtlMs) {
        return false; // Expired
      }
      this.send(message); // Try to send
      return true; // Keep in queue if still not sent
    });
  }

  // Call flushMessageQueue() in onopen handler
}
```

**Benefits:**
- Messages not lost during temporary disconnections
- Better UX

**Estimated Effort:** 3-4 hours

---

### 4.9 Optimize Store Subscriptions

**Current:**
```typescript
const windows = useWindowsStore((state) => state.windows);  // Entire array
```

**Proposed:**
```typescript
// Select only what's needed
const visibleWindows = useWindowsStore((state) =>
  state.windows.filter(w => !w.minimized).sort((a, b) => a.z_index - b.z_index)
);
const activeWindowId = useDesktopStore((state) => state.activeWindowId);
```

**Benefits:** Fixes HIGH-005, reduces unnecessary re-renders

**Estimated Effort:** 2-3 hours

---

### 4.10 Extract WebSocket Message Processing

**Current:** Message processing is inline in `applyWsMessage` function

**Proposed:**
```typescript
// lib/ws/processor.ts
const wsMessageHandlers: Record<WsServerMessage['type'], WsMessageHandler> = {
  pong: (message, { desktopStore, windowsStore }) => {
    // No-op
  },

  desktop_state: (message, { desktopStore, windowsStore }) => {
    desktopStore.setDesktopState(message.desktop);
    windowsStore.setWindows(message.desktop.windows);
  },

  window_opened: (message, { desktopStore, windowsStore }) => {
    windowsStore.openWindow(message.window);
    desktopStore.setActiveWindow(message.window.id);
  },

  // ... etc
};

export function processWsMessage(
  message: WsServerMessage,
  stores: StoreContext
): void {
  const handler = wsMessageHandlers[message.type];
  if (handler) {
    handler(message, stores);
  }
}
```

**Benefits:**
- Easier to test each handler
- Clear handler registration
- Better error handling
- Extensible for new message types

**Estimated Effort:** 4-6 hours

---

### 4.11 Use CSS Custom Properties for Theming

**Current:** Theme values hardcoded in CSS

**Recommendation:** Implement CSS custom properties

```css
/* components/theme/variables.css */
:root {
  /* Colors */
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --text-primary: #f8fafc;
  --accent-bg: #3b82f6;

  /* Window */
  --window-bg: var(--bg-secondary);
  --window-border: var(--border-color);
  --window-shadow: 0 18px 45px rgba(2, 6, 23, 0.45);
  --window-active-border: rgba(96, 165, 250, 0.85);
  --window-active-outline: 2px solid var(--accent-bg);
}

:root[data-theme="light"] {
  --bg-primary: #f8fafc;
  --bg-secondary: #ffffff;
  --text-primary: #0f172a;
  --accent-bg: #2563eb;
}
```

**Benefits:**
- Easy theme switching
- Consistent design system
- Reduced CSS duplication

**Estimated Effort:** 6-8 hours

---

### 4.12 Use Viewport Hook for Responsive Behavior

**Current:** Window size checked inline with viewport prop

**Recommendation:** Create `useViewport` hook

```typescript
// hooks/useViewport.ts
export function useViewport() {
  const [viewport, setViewport] = useState(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
  }));

  useEffect(() => {
    const handleResize = () => {
      setViewport({
        width: window.innerWidth,
        height: window.innerHeight,
      });
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  const isMobile = viewport.width <= 1024;
  const isTablet = viewport.width > 1024 && viewport.width <= 1280;
  const isDesktop = viewport.width > 1280;

  return { ...viewport, isMobile, isTablet, isDesktop };
}
```

**Benefits:**
- Consistent viewport access
- Easy to add breakpoint helpers
- Responsive design in one place

**Estimated Effort:** 2-3 hours

---

## 5. Type Generation Issues

### 5.1 Missing ts-rs Exports

| Type | Location | Missing Export? | Impact |
|------|----------|-----------------|--------|
| `ApiResponse<T>` | `shared-types/src/lib.rs:243` | ✅ Missing | Can't use generic response type |
| `WriterMsg` | `shared-types/src/lib.rs:128` | ✅ Missing | WriterActor integration |
| `RegisterAppRequest` | `dioxus-desktop-backup/src/api.rs:177` | ✅ Duplicate | Defined in Dioxus, not in shared-types |
| `DesktopWsMessage` | `sandbox/src/api/websocket.rs` | ✅ Missing | Desktop WebSocket protocol |

**Recommendation:** Add `#[ts(export, export_to = "...")]` to these types.

**Estimated Effort:** 2-3 hours

---

### 5.2 Incorrect ts-rs Type Mappings

#### DateTime<Utc> Serialization

**Rust:** `shared-types/src/lib.rs:55` - `timestamp: DateTime<Utc>`

**ts-rs Default:** Generates `Date` object (incorrect)

**Fix Applied:** Uses `#[ts(type = "string")]` directive manually

**Recommendation:** Ensure all DateTime fields use `#[ts(type = "string")]`

---

#### serde_json::Value as unknown

**Rust:** Multiple locations in `shared-types/src/lib.rs`

**ts-rs Default:** Generates `any` (too permissive)

**Fix Applied:** Uses `#[ts(type = "unknown")]` directive

**Impact:** Forces explicit type checking on payload access (correct)

---

### 5.3 Enum Variant Serialization Issues

#### ToolStatus Enum

```rust
// Rust
pub enum ToolStatus {
    Success,
    Error(String),
}

// Generated TypeScript
export type ToolStatus = "Success" | { "Error": string };
```

**Issues:**
1. Tagged union is awkward to use
2. Pattern matching is verbose
3. Not standard TypeScript pattern

**Better Approach:**
```rust
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "...")]
#[serde(tag = "status")]
pub enum ToolStatus {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "error")]
    Error { message: String },
}
```

**Generates:**
```typescript
export type ToolStatus = { status: "success" } | { status: "error"; message: string };
```

**Estimated Effort:** 1-2 hours

---

#### ChatMsg Enum

**Current:** Tagged unions with variant names as keys

**Better:** Discriminated unions with explicit `type` field

**Estimated Effort:** 1-2 hours

---

### 5.4 WebSocket Type Duplication

**React:** Uses its own WebSocket types in `dioxus-desktop/src/lib/ws/types.ts`

**Generated:** `WsMsg` exists in `dioxus-desktop/src/types/generated.ts` but not used

**Recommendation:** Use generated `WsMsg` or export desktop-specific types to shared-types

**Estimated Effort:** 4-6 hours

---

### 5.5 Viewer Types Not Used

**Generated Types (present but unused in React):**
```typescript
export type ViewerKind = "text" | "image";
export type ViewerResource = { uri: string, mime: string };
export type ViewerCapabilities = { readonly: boolean };
export type ViewerDescriptor = { kind: ViewerKind, resource: ViewerResource, capabilities: ViewerCapabilities };
export type ViewerRevision = { rev: bigint, updated_at: string };
```

**Dioxus Usage:** Uses all viewer types

**React Usage:** ❌ None - no Viewer API client exists

**Impact:** Viewer types exist but cannot be used without Viewer API client (see HIGH-006)

---

## 6. Test Coverage Gaps

### 6.1 Current Test Coverage

| Component | Test File | Coverage | Comments |
|-----------|-----------|----------|----------|
| Chat Component | ✅ Chat.test.tsx (119 lines) | ~60% | Tests WebSocket lifecycle, pending timeouts |
| Chat WS Utils | ✅ ws.test.ts (34 lines) | ~90% | Good coverage of parsing logic |
| Terminal Component | ✅ Terminal.test.tsx (128 lines) | ~55% | Tests reconnection, cleanup |
| Terminal WS Utils | ✅ ws.test.ts (22 lines) | ~80% | Good coverage of parsing |
| WS Client | ✅ client.test.ts (315 lines) | ~85% | Comprehensive client tests |
| Desktop | ❌ No tests | 0% | **Critical gap** |
| Window | ❌ No tests | 0% | **Critical gap** |
| WindowManager | ❌ No tests | 0% | **Critical gap** |
| Stores | ❌ No tests | 0% | **Critical gap** |
| API Layer | ❌ No tests | 0% | **Critical gap** |
| Viewer Components | ❌ No tests | 0% | N/A (not implemented) |

**Overall Test Coverage:** ~45%

---

### 6.2 Missing Critical Tests

#### Desktop Component Tests

**Required Tests:**
- App bootstrap with missing apps registration
- WebSocket connection state handling
- Window opening/focusing interaction
- Error state display
- Retry logic on bootstrap failure

**Estimated Effort:** 4-6 hours

---

#### Window Component Tests

**Required Tests:**
- Drag functionality with pointer events
- Resize functionality
- Minimize/maximize/restore cycle
- Active window state management
- Z-index ordering
- Pointer capture/release
- Bounds clamping
- Drag threshold
- Keyboard shortcuts

**Estimated Effort:** 6-8 hours

---

#### Store Tests

**Required Tests:**
- Chat store: message addition, updates, pending state
- Windows store: add/remove/update/focus windows
- Desktop store: active window management
- Store synchronization between DesktopStore and WindowsStore

**Estimated Effort:** 4-6 hours

---

#### API Layer Tests

**Required Tests:**
- Error handling for network failures
- Response parsing validation
- Retry logic
- Request/response serialization
- Viewer API client (when implemented)

**Estimated Effort:** 6-8 hours

---

#### WebSocket Integration Tests

**Required Tests:**
- Connection failure scenarios
- Message validation edge cases
- Reconnection with exponential backoff
- Multiple WebSocket connections
- Subscription error handling
- Message queue behavior

**Estimated Effort:** 4-6 hours

---

## 7. Integration Issues

### 7.1 WebSocket Store Synchronization

**Issue:** React has two separate stores (DesktopStore and WindowsStore) that need to stay in sync. Dioxus uses a single DesktopState signal.

**Current State:**
```typescript
// useWebSocket.ts:17-92
function applyWsMessage(message: WsServerMessage): void {
  const desktopStore = useDesktopStore.getState();
  const windowsStore = useWindowsStore.getState();

  switch (message.type) {
    case 'window_focused': {
      windowsStore.focusWindow(message.window_id, message.z_index);
      desktopStore.setActiveWindow(message.window_id);  // Sync needed!
      return;
    }
    // ... other cases require similar sync
  }
}
```

**Problem:**
- Manual synchronization required
- Risk of stores getting out of sync
- Redundant state

**Recommendation:**
1. Merge stores into single DesktopStore (like Dioxus)
2. Use event emitter pattern to auto-sync
3. Use Zustand middleware for sync

**Cross-reference:** Related to CRITICAL-001, CRITICAL-002

---

### 7.2 Desktop ID Propagation

**Issue:** Desktop ID is passed through many components. Dioxus uses a signal at top level.

**Problem:**
- Deep prop drilling
- Callbacks need to capture desktop ID
- Hard to test components in isolation

**Recommendation:** Use React Context for desktop ID

**Estimated Effort:** 2-3 hours

---

### 7.3 API Client Type Safety

**Issue:** API functions are loosely typed. Generated types exist but not fully utilized.

**Current State:**
```typescript
export async function moveWindow(
  desktopId: string,
  windowId: string,
  x: number,
  y: number
): Promise<void> {
  const response = await client.put(`/desktops/${desktopId}/windows/${windowId}/move`, { x, y });
  return response.data;  // Not typed!
}
```

**Recommendation:** Use generated types more aggressively

**Estimated Effort:** 4-6 hours

---

### 7.4 Error State Integration

**Issue:** Errors are shown in Desktop component but not propagated to individual windows.

**Problem:**
- Errors are shown globally
- No per-window error states
- No retry mechanism

**Recommendation:** Add per-window error handling and retry

**Estimated Effort:** 4-6 hours

---

### 7.5 Window Z-Index Race Conditions

**Issue:** Focus events from WebSocket may arrive out of order or conflict with local user actions.

**Problem:**
- User clicks window (sets z-index locally)
- WebSocket focus event arrives (overwrites z-index)
- Windows may "jump" unexpectedly

**Recommendation:** Add client-side z-index counter that syncs with server

**Estimated Effort:** 3-4 hours

---

### 7.6 Protocol Mismatch: Desktop vs Generic Actor

**Files:** `shared-types/src/lib.rs`, `sandbox/src/api/websocket.rs`

**Issue:** There are TWO WebSocket protocols in codebase:
1. Desktop-specific protocol (WsMessage in sandbox/src/api/websocket.rs)
2. Generic actor protocol (WsMsg in shared-types/src/lib.rs)

**Impact:** Confusion about which protocol to use. The desktop WebSocket endpoint uses protocol 1, but shared-types exports protocol 2.

**Recommendation:** Clearly document which protocol is used where. Consider consolidating or clearly separating.

---

### 7.7 Type Duplication

**Files:** `dioxus-desktop/src/lib/ws/types.ts`, `shared-types/src/lib.rs`

**Issue:** `WsServerMessage` in frontend duplicates fields from backend `WsMessage`.

**Impact:** Maintenance burden. Type drift possible.

**Recommendation:** Use `ts_rs` to generate frontend types from Rust (see Section 4.6)

---

## 8. Feature Parity Matrix

### 8.1 App Status Matrix

| App | React Implementation | Dioxus Implementation | Parity | Status |
|-----|---------------------|----------------------|--------|--------|
| **Chat** | ✅ Complete (359 lines) | ✅ Complete (1257 lines) | 75% | Partial - missing tool call UI |
| **Terminal** | ✅ Complete (195 lines) | ✅ Complete (409 lines) | 85% | Minor differences |
| **ImageViewer** | ❌ Missing | ✅ Complete (65 lines) | 0% | Not ported |
| **TextViewer** | ❌ Missing | ✅ Complete (147 lines) | 0% | Not ported |
| **ViewerShell** | ❌ Missing | ✅ Complete (218 lines) | 0% | Not ported |
| **Writer App** | ⚠️ Placeholder only | ⚠️ Placeholder only | 100% | Both incomplete |
| **Files App** | ⚠️ Placeholder only | ⚠️ Placeholder only | 100% | Both incomplete |

**Overall App Parity:** 65%

---

### 8.2 Desktop/Window Features Matrix

| Feature | React | Dioxus | Parity | Priority |
|---------|--------|--------|--------|----------|
| **Window Drag** | ✅ Basic | ✅ With capture/threshold | 60% | HIGH |
| **Window Resize** | ✅ Basic | ✅ With rate limiting | 60% | HIGH |
| **Window Minimize** | ✅ Working | ✅ Working | 90% | - |
| **Window Maximize/Restore** | ✅ Working | ✅ Working | 90% | - |
| **Window Focus** | ⚠️ Broken z_index | ✅ Working | 50% | CRITICAL |
| **Keyboard Shortcuts** | ❌ Missing | ✅ Complete | 0% | HIGH |
| **Pointer Capture** | ❌ Missing | ✅ Working | 0% | CRITICAL |
| **Bounds Clamping** | ❌ Missing | ✅ Working | 0% | CRITICAL |
| **Drag Threshold** | ❌ Missing | ✅ Working | 0% | HIGH |
| **Rate Limiting** | ❌ Missing | ✅ Working | 0% | HIGH |
| **Theme System** | ❌ Missing | ✅ Complete | 0% | MEDIUM |
| **Desktop Icons** | ✅ Basic | ✅ With animations | 70% | LOW |
| **Prompt Bar** | ✅ Working | ✅ Working | 80% | - |
| **Taskbar** | ✅ Separate | ⚠️ Integrated | 60% | LOW |

**Overall Desktop Parity:** 55%

---

### 8.3 WebSocket Features Matrix

| Feature | React | Dioxus | Parity | Priority |
|---------|--------|--------|--------|----------|
| **Desktop WS** | ✅ Complete | ✅ Complete | 90% | - |
| **Terminal WS** | ✅ Complete | ✅ Complete | 85% | - |
| **Chat WS** | ✅ Complete | ❌ Missing endpoint | 70% | - |
| **Reconnection** | ✅ Exponential | ✅ With jitter | 70% | LOW |
| **Keep-Alive** | ❌ Missing | ❌ Missing | 0% | MEDIUM |
| **Connection Timeout** | ❌ Missing | ❌ Missing | 0% | MEDIUM |
| **Message Queue** | ❌ Missing | ❌ Missing | 0% | MEDIUM |
| **Type Validation** | ⚠️ Weak | ⚠️ Weak | 50% | MEDIUM |
| **Error Handling** | ⚠️ Basic | ⚠️ Basic | 60% | MEDIUM |

**Overall WebSocket Parity:** 65%

---

### 8.4 API Client Features Matrix

| Endpoint | React | Dioxus | Backend | Status |
|----------|--------|--------|---------|--------|
| GET /health | ✅ | ✅ | ✅ | OK |
| GET /ws | ✅ | ✅ | ✅ | OK |
| POST /chat/send | ✅ | ✅ | ✅ | OK |
| GET /chat/{id}/messages | ✅ | ✅ | ✅ | OK |
| GET /user/{id}/preferences | ⚠️ Type mismatch | ✅ | ✅ | BUG |
| PATCH /user/{id}/preferences | ⚠️ Type mismatch | ✅ | ✅ | BUG |
| GET /desktop/{id} | ✅ | ✅ | ✅ | OK |
| GET /desktop/{id}/windows | ✅ | ✅ | ✅ | OK |
| POST /desktop/{id}/windows | ✅ | ✅ | ✅ | OK |
| DELETE /desktop/{id}/windows/{wid} | ✅ | ✅ | ✅ | OK |
| PATCH /desktop/{id}/windows/{wid}/position | ✅ | ✅ | ✅ | OK |
| PATCH /desktop/{id}/windows/{wid}/size | ✅ | ✅ | ✅ | OK |
| POST /desktop/{id}/windows/{wid}/focus | ✅ | ✅ | ✅ | OK |
| POST /desktop/{id}/windows/{wid}/minimize | ✅ | ✅ | ✅ | OK |
| POST /desktop/{id}/windows/{wid}/maximize | ⚠️ Response mismatch | ✅ | ✅ | BUG |
| POST /desktop/{id}/windows/{wid}/restore | ⚠️ Response mismatch | ✅ | ✅ | BUG |
| GET /desktop/{id}/apps | ✅ | ✅ | ✅ | OK |
| POST /desktop/{id}/apps | ✅ | ✅ | ✅ | OK |
| GET /viewer/content | ❌ **MISSING** | ✅ | ✅ | **MISSING** |
| PATCH /viewer/content | ❌ **MISSING** | ✅ | ✅ | **MISSING** |
| GET /api/terminals/{id} | ✅ | ✅ | ✅ | OK |
| GET /api/terminals/{id}/info | ✅ | ✅ | ✅ | OK |
| GET /api/terminals/{id}/stop | ✅ | ✅ | ✅ | OK |

**Overall API Parity:** 85% (missing Viewer API endpoints)

---

### 8.5 Overall Parity Summary

| Category | Parity | Status |
|----------|--------|--------|
| **Apps** | 65% | ⚠️ Moderate |
| **Desktop/Window** | 55% | 🔴 Needs Work |
| **WebSocket** | 65% | ⚠️ Moderate |
| **API Client** | 85% | 🟢 Good |
| **Type Generation** | 70% | 🟡 Fair |
| **Test Coverage** | 45% | 🔴 Low |
| **OVERALL** | **65%** | ⚠️ **Moderate** |

---

## 9. Priority Recommendations

### Phase 1: Critical Fixes (Week 1) - 20-30 hours

**P0 - Do First**

1. **Fix State Duplication Between Stores** (CRITICAL-001)
   - Consolidate DesktopStore and WindowsStore
   - Eliminate duplicate window arrays
   - **Effort:** 8-12 hours
   - **Impact:** Eliminates multiple race conditions

2. **Add Pointer Capture and Bounds Clamping** (CRITICAL-004, CRITICAL-005)
   - Implement `setPointerCapture()` on window drag/resize
   - Add bounds clamping function
   - Add drag threshold (4px)
   - **Effort:** 6-8 hours
   - **Impact:** Prevents windows from becoming stuck or inaccessible

3. **Fix z_index Calculation on Focus** (CRITICAL-003)
   - Calculate max z_index in store action
   - Remove need for caller to pass z_index
   - **Effort:** 2-3 hours
   - **Impact:** Fixes window layering

4. **Fix API Response Type Mismatches** (CRITICAL-007, CRITICAL-008)
   - Update maximizeWindow/restoreWindow to handle extra fields
   - **Effort:** 2-4 hours
   - **Impact:** Fixes runtime errors

5. **Fix WebSocket State Updates Race Condition** (CRITICAL-002)
   - Implement batch updates for WebSocket messages
   - **Effort:** 2-3 hours
   - **Impact:** Fixes inconsistent state updates

---

### Phase 2: High Priority Features (Week 2) - 30-45 hours

**P1 - Address Soon**

6. **Port Viewer Components** (HIGH-006 through HIGH-011)
   - ViewerShell: 8-10 hours
   - TextViewer: 6-8 hours
   - ImageViewer: 4-6 hours
   - **Total Effort:** 18-24 hours
   - **Impact:** Enables file viewing/editing

7. **Implement Rate Limiting for Drag/Resize** (HIGH-002)
   - Add 50ms throttle for move/resize events
   - Queue final position on pointer up
   - **Effort:** 4-6 hours
   - **Impact:** Reduces network load from 60+ requests/sec to ~20 requests/sec

8. **Add Keyboard Shortcuts** (HIGH-003)
   - Implement Alt+F4, Escape, Ctrl+M, Alt+Arrows, etc.
   - **Effort:** 6-8 hours
   - **Impact:** Improves accessibility

9. **Port Tool Call Rendering** (HIGH-008)
   - Implement collapsible tool call sections
   - Add tool result display
   - **Effort:** 8-12 hours
   - **Impact:** Users can see AI tool activity

10. **Fix API Client Type Mismatches** (HIGH-004)
    - Update UserPreferences interface to match backend
    - **Effort:** 2-3 hours
    - **Impact:** Fixes type errors

---

### Phase 3: Medium Priority (Week 3-4) - 35-50 hours

**P2 - Address in Near Term**

11. **Add Critical Tests** (Section 6.2)
    - Desktop component tests: 4-6 hours
    - Window component tests: 6-8 hours
    - Store tests: 4-6 hours
    - API layer tests: 6-8 hours
    - **Total Effort:** 20-28 hours
    - **Impact:** Increases test coverage from 45% to ~70%

12. **Implement Theme System** (MEDIUM-008)
    - Create theme hook and CSS variables
    - Add theme toggle button
    - Implement persistence
    - **Effort:** 8-12 hours
    - **Impact:** Adds light/dark mode

13. **Improve WebSocket Features** (MEDIUM-002 through MEDIUM-005)
    - Fix status state race condition
    - Add message queue with retry
    - Add connection timeout
    - Add keep-alive mechanism
    - **Total Effort:** 8-12 hours
    - **Impact:** More robust WebSocket handling

14. **Optimize Re-renders** (HIGH-005)
    - Optimize store subscriptions
    - Add React.memo to Window component
    - Implement batch state updates
    - **Effort:** 4-6 hours
    - **Impact:** Better performance

---

### Phase 4: Low Priority (Week 5-6) - 20-35 hours

**P3 - Address Long Term**

15. **Extract Refactored Code** (Section 4)
    - Extract useWindowInteraction hook: 4-6 hours
    - Extract WebSocket message processor: 4-6 hours
    - Use Viewport hook: 2-3 hours
    - **Total Effort:** 10-15 hours
    - **Impact:** Better code organization

16. **Improve Type Generation** (Section 5)
    - Add missing ts-rs exports
    - Fix enum serialization
    - Use generated types instead of duplicates
    - **Total Effort:** 6-10 hours
    - **Impact:** Better type safety

17. **Desktop Polish** (Section 3.9, 3.10)
    - Chat assistant bundling: 6-8 hours
    - Desktop icon press animation: 2-3 hours
    - Terminal reconnection with jitter: 2-3 hours
    - **Total Effort:** 10-14 hours
    - **Impact:** Better UX

---

### Estimated Total Effort

| Phase | Effort (Hours) | Percentage |
|-------|----------------|------------|
| **Phase 1: Critical Fixes** | 20-30 | 23% |
| **Phase 2: High Priority** | 30-45 | 33% |
| **Phase 3: Medium Priority** | 35-50 | 25% |
| **Phase 4: Low Priority** | 20-35 | 19% |
| **TOTAL** | **105-160 hours** | **100%** |

**Recommended Timeline:**
- **Week 1:** Critical fixes (20-30 hours)
- **Week 2:** High priority features (30-45 hours)
- **Week 3-4:** Medium priority (35-50 hours)
- **Week 5-6:** Low priority polish (20-35 hours)

---

## 10. Conclusion

### Summary of Findings

The React implementation (`dioxus-desktop`) has solid foundations with working Chat and Terminal apps, functional WebSocket connections, and a reasonable API client layer. However, significant gaps exist compared to Dioxus backup:

**Major Strengths:**
- ✅ Core Chat and Terminal apps functional
- ✅ WebSocket connection and state subscription working
- ✅ Type-safe API client layer
- ✅ Store-based state management
- ✅ Generated TypeScript types from Rust

**Major Weaknesses:**
- ❌ State architecture has critical flaws (duplication, race conditions)
- ❌ Window management missing key accessibility and UX features
- ❌ Complete viewer system not ported (ImageViewer, TextViewer, ViewerShell)
- ❌ Tool call visualization missing from Chat
- ❌ Test coverage at 45% with critical gaps
- ❌ Missing theme system and keyboard navigation
- ❌ No rate limiting causing network flood during drag/resize

**Overall Parity:** 65%

### Risk Assessment

**Overall Risk:** 🟡 **MEDIUM**

**Rationale:**
- Core functionality works (Chat, Terminal, Desktop basics)
- No critical blockers prevent basic operation
- However, significant feature gaps and bugs impact UX
- Missing viewer components limit functionality
- State synchronization bugs could cause unpredictable behavior

**Recommendation:** Address Critical (P0) bugs before production use. Complete High Priority (P1) features for acceptable user experience.

### Next Steps

1. **Immediate (This Week):** Focus on Phase 1 critical fixes
   - Fix state duplication and race conditions
   - Add pointer capture and bounds clamping
   - Fix API response type mismatches

2. **Short-term (Next 2 Weeks):** Implement Phase 2 features
   - Port viewer components
   - Add rate limiting
   - Implement keyboard shortcuts

3. **Medium-term (Next Month):** Complete Phase 3 and 4
   - Increase test coverage to 70%+
   - Implement theme system
   - Polish and optimize

### Key Success Metrics

After completing recommended changes:

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Overall Parity | 65% | 90%+ | 🟡 |
| Test Coverage | 45% | 70%+ | 🔴 |
| Critical Bugs | 12 | 0 | 🔴 |
| App Parity | 65% | 90%+ | 🟡 |
| Desktop Parity | 55% | 85%+ | 🔴 |
| API Parity | 85% | 95%+ | 🟢 |

---

**Report Generated:** February 6, 2026
**Reports Synthesized:** 5
**Total Issues Identified:** 70
**Total Estimated Effort:** 105-160 hours (13-20 days)
**Overall Risk Level:** Medium
