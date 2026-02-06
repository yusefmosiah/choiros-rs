# State Management Review: React vs Dioxus

## Executive Summary

This review identifies **significant architectural problems** in the React implementation's state management that are absent in the Dioxus backup. The React codebase suffers from:
- Data duplication between stores
- Race conditions during WebSocket updates
- Missing state handling for key scenarios
- Inefficient re-render patterns
- Broken synchronization between desktop and window stores

---

## Architecture Comparison

### Dioxus Implementation (sandbox-ui-backup)

**State Architecture:**
- **Single Source of Truth**: `desktop_state: Signal<Option<DesktopState>>`
- Centralized in `src/desktop/shell.rs:15`
- Reactive signals (`use_signal`) for fine-grained reactivity
- State mutations are direct and atomic

**Key Files:**
- `src/desktop/state.rs` - State update functions
- `src/desktop/shell.rs` - Main desktop component
- `src/desktop_window.rs` - Window component
- `src/desktop/ws.rs` - WebSocket message handling

**State Flow:**
```
WebSocket → parse_ws_message → apply_ws_event → desktop_state mutation → Dioxus re-render
```

### React Implementation (sandbox-ui)

**State Architecture:**
- **Three Separate Zustand Stores**: `useDesktopStore`, `useWindowsStore`, `useChatStore`
- Data is duplicated between stores
- Distributed across multiple files
- Updates must coordinate across stores

**Key Files:**
- `src/stores/desktop.ts` - Desktop state
- `src/stores/windows.ts` - Window array
- `src/stores/chat.ts` - Chat messages
- `src/hooks/useWebSocket.ts` - WebSocket integration
- `src/components/desktop/Desktop.tsx` - Desktop component

**State Flow:**
```
WebSocket → parse message → applyWsMessage → updates BOTH desktop AND windows stores
```

---

## Bugs Found

### 1. CRITICAL: State Duplication Inconsistency

**Location:** `src/stores/desktop.ts:4-17`, `src/stores/windows.ts:4-22`

**Problem:**
The same `windows` data exists in two separate stores:
- `useDesktopStore.desktop.windows`
- `useWindowsStore.windows`

This creates two copies of the truth that can diverge.

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

**Dioxus Equivalent:**
- Single `desktop_state` contains windows array - no duplication

---

### 2. CRITICAL: Race Condition in WebSocket Updates

**Location:** `src/hooks/useWebSocket.ts:17-92`

**Problem:**
The `applyWsMessage` function updates both stores but doesn't guarantee atomicity:

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

**Dioxus Equivalent:**
`src/desktop/state.rs:21-24` - Atomic state mutation:
```rust
WsEvent::WindowOpened(window) => {
    if let Some(state) = desktop_state.write().as_mut() {
        state.windows.push(window);  // Single atomic update
    }
}
```

---

### 3. CRITICAL: Missing z_index Calculation on Window Focus

**Location:** `src/stores/windows.ts:65-73`

**Problem:**
`focusWindow` sets z_index but doesn't calculate it correctly - it requires caller to pass it:

```typescript
focusWindow: (windowId: string, zIndex: number) => {
  set((state) => ({
    windows: updateWindow(state.windows, windowId, (window) => ({
      ...window,
      z_index: zIndex,  // Passed in, not calculated!
      minimized: false,
    })),
  }));
},
```

**Evidence of Bug:**
In `src/components/desktop/Desktop.tsx:145-163`, the caller doesn't pass a z_index:
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
  },
  // ...
);
```

**Impact:**
- Focused windows don't visually come to front
- Window layering breaks
- User clicks window but it doesn't raise

**Dioxus Equivalent:**
`src/desktop/state.rs:136-143` - Correctly calculates z_index:
```rust
pub fn focus_window_and_raise_z(state: &mut DesktopState, window_id: &str) {
    state.active_window = Some(window_id.to_string());

    let max_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
    if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
        window.z_index = max_z + 1;  // Correct calculation!
    }
}
```

---

### 4. HIGH: Desktop Store Updates Windows Array Inconsistently

**Location:** `src/stores/desktop.ts:68-87`

**Problem:**
`closeWindow` in desktop store tries to update `desktop.windows` but the actual source of truth is `useWindowsStore.windows`:

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

**Dioxus Equivalent:**
`src/desktop/state.rs:128-134` - Removes window atomically:
```rust
pub fn remove_window_and_reselect_active(state: &mut DesktopState, window_id: &str) {
    state.windows.retain(|w| w.id != window_id);  // Actually removes it

    if state.active_window.as_deref() == Some(window_id) {
        state.active_window = state.windows.last().map(|w| w.id.clone());
    }
}
```

---

### 5. HIGH: Missing Active Window Handling in Minimize

**Location:** `src/stores/desktop.ts:89-113`

**Problem:**
`minimizeWindow` calculates next active window correctly but doesn't update the windows store to set `minimized: true`:

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

**Missing:**
- The window being minimized never gets its `minimized` flag set
- Only works if caller calls `windowsStore.minimizeWindow()` too

**Dioxus Equivalent:**
`src/desktop/state.rs:59-75` - Correctly updates window:
```rust
WsEvent::WindowMinimized(window_id) => {
    if let Some(state) = desktop_state.write().as_mut() {
        if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
            window.minimized = true;  // Actually sets minimized!
            window.maximized = false;
        }
        // Also updates active window correctly
    }
}
```

---

### 6. MEDIUM: Unnecessary Re-renders from Store Subscriptions

**Location:** `src/components/desktop/Desktop.tsx:35-38`

**Problem:**
Component subscribes to entire store slices:

```typescript
const windows = useWindowsStore((state) => state.windows);  // Re-renders on ANY window change
const activeWindowId = useDesktopStore((state) => state.activeWindowId);
```

**Impact:**
- Any window move/resize triggers Desktop re-render
- Desktop re-renders even when active window doesn't change
- Unnecessary work for frequent operations (drag/resize)

**Evidence:**
In `src/components/window/Window.tsx:66-96`, every pointer move calls `onMove()`:
```typescript
const handlePointerMove = (moveEvent: PointerEvent) => {
  // ...
  onMove(windowState.id, Math.round(nextX), Math.round(nextY));  // Fires on every pixel!
};
```

**Dioxus Equivalent:**
Signals provide fine-grained reactivity - only components reading specific signals re-render.

---

### 7. MEDIUM: Chat State Not Integrated with Desktop State

**Location:** `src/stores/chat.ts:4-14`

**Problem:**
Chat messages are completely separate from desktop state:

```typescript
interface ChatStore {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
  // ...
}
```

**Missing:**
- No way to find chat window ID
- No integration with window minimize/close
- Chat continues receiving messages even if window closed

**Evidence:**
In `src/components/apps/Chat/Chat.tsx:92-221`, WebSocket keeps running even if component unmounts:
```typescript
useEffect(() => {
  // ... WebSocket setup
  return () => {
    cancelled = true;
    if (wsRef.current) {
      wsRef.current.close();  // Only closes on unmount, not window close
    }
  };
}, [actorId, ...]);
```

**Dioxus Equivalent:**
Chat is tightly integrated - `find_chat_window_id()` in `src/desktop/state.rs:145-153`:
```rust
pub fn find_chat_window_id(state: &Option<DesktopState>) -> Option<String> {
    state.as_ref().and_then(|desktop| {
        desktop.windows.iter()
            .find(|window| window.app_id == "chat")
            .map(|window| window.id.clone())
    })
}
```

---

### 8. MEDIUM: Window Drag/Resize Throttling Missing

**Location:** `src/components/window/Window.tsx:66-96`

**Problem:**
Every pointer move sends network request:

```typescript
const handlePointerMove = (moveEvent: PointerEvent) => {
  if (moveEvent.pointerId !== dragPointerIdRef.current || !dragStartRef.current) {
    return;
  }

  const dx = moveEvent.clientX - dragStartRef.current.pointerX;
  const dy = moveEvent.clientY - dragStartRef.current.pointerY;

  onMove(
    windowState.id,
    Math.round(dragStartRef.current.startX + dx),
    Math.round(dragStartRef.current.startY + dy),
  );
};
```

**Impact:**
- 60+ requests per second during drag
- Server flooded with position updates
- Network overhead
- Potential race conditions with server-side state

**Dioxus Equivalent:**
`src/desktop_window.rs:208-296` - Implements throttling with `PATCH_INTERVAL_MS: i64 = 50`:
```rust
let elapsed = now_ms() - last_move_sent_ms();
if elapsed >= PATCH_INTERVAL_MS {
    if let Some((next_x, next_y)) = queued_move.write().take() {
        on_move.call((window_id_clone, next_x, next_y));
        last_move_sent_ms.set(now_ms());
    }
}
```

---

### 9. MEDIUM: Bootstrap State Not Reset on Unmount

**Location:** `src/components/desktop/Desktop.tsx:7, 49-108`

**Problem:**
Module-level `bootstrapState` map is never cleared:

```typescript
const bootstrapState = new Map<string, boolean>();  // Never cleared!

useEffect(() => {
  let cancelled = false;
  const bootstrapKey = `${desktopId}-bootstrap`;

  const bootstrap = async () => {
    if (bootstrapState.get(bootstrapKey) || status !== 'connected') {
      return;  // Won't bootstrap if already done, even if remounted!
    }
    bootstrapState.set(bootstrapKey, true);
    // ...
  };

  void bootstrap();

  return () => {
    cancelled = true;
    // Doesn't clear bootstrapState!
  };
}, [desktopId, setDesktopError, status]);
```

**Impact:**
- If Desktop unmounts and remounts, it won't bootstrap again
- Manual page refresh required to reset state
- Survives React StrictMode double-mounts but breaks remount scenarios

**Dioxus Equivalent:**
No such issue - component lifecycle is simpler in Dioxus.

---

### 10. LOW: Missing Error Boundary Handling

**Location:** `src/stores/desktop.ts:49-51`, `src/stores/chat.ts:48-50`

**Problem:**
Error states are set but no error recovery mechanism:

```typescript
setError: (message) => {
  set({ lastError: message });  // Just sets error
},
```

**Missing:**
- No automatic retry logic
- No way to clear errors programmatically
- Errors persist even after issues resolved

**Dioxus Equivalent:**
`src/desktop/shell.rs:69-74` - Errors are in signal and displayed, but similar limitation.

---

### 11. LOW: Chat Stream Events Not Managed Globally

**Location:** `src/components/apps/Chat/Chat.tsx:50`

**Problem:**
Stream events are component-scoped:

```typescript
const [streamEvents, setStreamEvents] = useState<StreamEvent[]>([]);
```

**Missing:**
- If Chat window closed and reopened, stream history is lost
- Events aren't persisted in global state
- No way to see tool activity from closed sessions

**Dioxus Equivalent:**
`src/components.rs:14-17` - AssistantBundle persists in message text as special format.

---

### 12. LOW: No Viewport State Management

**Location:** Missing in React

**Problem:**
Dioxus tracks viewport in `src/desktop/shell.rs:20`:
```rust
let viewport = use_signal(|| (1920u32, 1080u32));
```

React has no global viewport state.

**Impact:**
- Window clamping/responsive behavior not consistent
- Mobile/desktop detection duplicated across components

---

## Refactoring Opportunities

### 1. Consolidate Stores (HIGH PRIORITY)

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
- Eliminates state duplication
- Prevents divergence
- Simpler synchronization
- Follows Dioxus architecture

---

### 2. Implement Atomic WebSocket Updates

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

---

### 3. Add z_index Calculation to Store Actions

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

---

### 4. Implement Drag/Resize Throttling

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

---

### 5. Optimize Store Subscriptions

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

---

### 6. Add Window State Persistence

**Current:**
No persistence of window positions/sizes across reloads.

**Proposed:**
```typescript
// Add to desktop store
interface DesktopStateStore {
  // ...
  saveWindowState: () => Promise<void>;
  restoreWindowState: () => Promise<void>;
}
```

---

## Missing State from Dioxus

### 1. Theme State

**Dioxus:** `src/desktop/shell.rs:22-46`
- Current theme tracking
- Persistence to backend
- Cache in localStorage
- Toggle functionality

**React:** Not implemented
- No theme state in stores
- No dark/light mode toggle

---

### 2. Viewport Tracking

**Dioxus:** `src/desktop/shell.rs:20, src/desktop/effects.rs:14-18`
- Global viewport dimensions
- Used for window clamping
- Responsive behavior

**React:** Not global
- Each component checks viewport independently
- Window clamping may use stale viewport

---

### 3. Chat Window Integration

**Dioxus:** `src/desktop/state.rs:145-153`, `src/desktop/actions.rs:127-152`
- `find_chat_window_id()` function
- Chat window auto-opens from prompt
- Integrated prompt-to-chat workflow

**React:** Partial implementation
- `handlePromptSubmit` in Desktop.tsx:297-321
- But no helper function to find chat window
- Logic duplicated

---

### 4. App Registration State

**Dioxus:** `src/desktop/effects.rs:93-107`
- `apps_registered` flag
- Prevents duplicate registration
- Core apps registered once

**React:** `Desktop.tsx:7, 49-108`
- Module-level `bootstrapState` map
- Works but less elegant
- Doesn't survive all unmount scenarios

---

### 5. Window Props for Viewers

**Dioxus:** `src/desktop/actions.rs:13-37`
- `viewer_props_for_app()` function
- Viewer descriptors for writer, files apps
- Integrated into window opening

**React:** Not implemented
- Viewer apps not fully functional
- No viewer props mechanism

---

### 6. Pointer Capture Management

**Dioxus:** `src/desktop_window.rs:367-383, 465-483`
- Proper pointer capture for drag/resize
- Release on cancel/error

**React:** Missing
- No pointer capture API usage
- Drag continues even if pointer leaves window

---

### 7. Keyboard Navigation

**Dioxus:** `src/desktop_window.rs:131-196`
- Alt+Shift+Arrows for resize
- Alt+Arrows for move
- Ctrl+M for minimize
- Ctrl+Shift+M for maximize/restore
- Escape to cancel drag
- F4+Alt to close

**React:** Not implemented
- No keyboard window management
- Accessibility reduced

---

## Performance Concerns

### 1. Excessive Re-renders (HIGH)

**Location:** `src/components/desktop/Desktop.tsx:35`

**Problem:**
```typescript
const windows = useWindowsStore((state) => state.windows);
```

Every window move/resize (60+ times/second) triggers Desktop re-render.

**Impact:**
- Unnecessary virtual DOM diffing
- Layout thrashing
- Poor drag performance
- Battery drain on laptops

**Solution:**
Use selector that only changes when visible windows change:
```typescript
const visibleWindows = useWindowsStore((state) =>
  useMemo(() =>
    state.windows.filter(w => !w.minimized).sort((a, b) => a.z_index - b.z_index),
    [state.windows]
  )
);
```

---

### 2. Network Flood During Drag (HIGH)

**Location:** `src/components/window/Window.tsx:66-96`

**Problem:**
Every pixel of drag sends API request.

**Impact:**
- 60+ requests/second
- Server load
- Network bandwidth waste
- Rate limiting risk

**Solution:**
Implement 50ms throttle (like Dioxus).

---

### 3. Store Lookup on Every Event (MEDIUM)

**Location:** `src/components/desktop/Desktop.tsx:130, 148, 170, 190, 209, 228, 247, 274`

**Problem:**
Every window operation calls `useWindowsStore.getState().find()`:

```typescript
const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
```

**Impact:**
- O(n) lookup on every window operation
- Scans entire window array
- Gets worse with more windows

**Solution:**
Use Map for O(1) lookups:
```typescript
interface WindowsStore {
  windowsMap: Map<string, WindowState>;  // O(1) lookups
  windowsList: WindowState[];  // For rendering
  // ...
}
```

---

### 4. Message Array Sorting on Render (LOW)

**Location:** `src/components/desktop/Desktop.tsx:323-326`

**Problem:**
```typescript
const sortedWindows = useMemo(
  () => [...windows].sort((a, b) => a.z_index - b.z_index),
  [windows],  // Recalculates on ANY window change
);
```

**Impact:**
- Unnecessary sorting when windows unchanged
- O(n log n) cost
- Already sorted in most cases

**Solution:**
Sort only when z_index changes, or maintain sorted array.

---

### 5. No Virtual Scrolling (LOW)

**Location:** Not implemented

**Problem:**
All messages rendered in Chat component.

**Impact:**
- DOM grows unbounded
- Performance degrades with many messages
- Memory usage increases

**Solution:**
Implement virtual scrolling for chat messages.

---

## State Synchronization Issues

### 1. Desktop vs Windows Store Divergence (CRITICAL)

**Problem:**
Two stores contain overlapping state:
- `desktopStore.desktop.windows`
- `windowsStore.windows`

**Divergence Scenarios:**

**Scenario 1: Window Open**
1. WebSocket sends `window_opened` message
2. `applyWsMessage` calls `windowsStore.openWindow(window)`
3. `applyWsMessage` calls `desktopStore.setActiveWindow(id)`
4. `desktopStore.desktop.windows` NOT updated
5. Components reading `desktop.desktop.windows` see stale state

**Scenario 2: Window Close**
1. User closes window via `handleCloseWindow()`
2. `closeWindow()` API called
3. WebSocket sends `window_closed` message
4. `applyWsMessage` calls `windowsStore.closeWindow(id)`
5. `applyWsMessage` calls `desktopStore.closeWindow(id)`
6. But `desktopStore.closeWindow` doesn't remove from `desktop.windows` array (Bug #4)

**Scenario 3: Minimize**
1. User minimizes window via `handleMinimizeWindow()`
2. `minimizeWindow()` API called
3. WebSocket sends `window_minimized` message
4. `applyWsMessage` calls `windowsStore.minimizeWindow(id)`
5. `applyWsMessage` calls `desktopStore.minimizeWindow(id)`
6. But `desktopStore.minimizeWindow` doesn't set `minimized: true` flag (Bug #5)

---

### 2. Active Window State Inconsistency (HIGH)

**Problem:**
Active window tracked in two places:
- `desktopStore.activeWindowId`
- `desktopStore.desktop.active_window`

**Inconsistency:**
```typescript
// src/stores/desktop.ts:26-27
setDesktopState: (desktop) => {
  set({ desktop, activeWindowId: desktop.active_window, lastError: null });
  // Both set from same source, but can diverge!
}
```

**Scenario:**
1. Desktop state loads with `active_window: "window-1"`
2. User clicks "window-2"
3. Component calls `setActiveWindow("window-2")`
4. Only `activeWindowId` changes to "window-2"
5. `desktop.active_window` still "window-1"
6. Some components read one, some the other

**Solution:**
Use single source - remove duplicate.

---

### 3. WebSocket + Optimistic Updates Race (MEDIUM)

**Problem:**
User actions trigger API calls, then WebSocket events confirm them.

**Scenario - Window Move:**
1. User drags window to (100, 100)
2. `handleMoveWindow()` calls API: `moveWindow(id, 100, 100)`
3. API returns success with window at (100, 100)
4. Before WebSocket message arrives, user drags again to (150, 150)
5. `handleMoveWindow()` calls API: `moveWindow(id, 150, 150)`
6. WebSocket message for first move arrives: position (100, 100)
7. Windows store updates to (100, 100) - WRONG! Should be (150, 150)
8. Second WebSocket message arrives and corrects it

**Impact:**
- Visual "jump" of window position
- User sees window move backwards temporarily

**Solution:**
Use message sequencing/timestamps to discard stale updates.

---

### 4. Chat Message Pending State Race (LOW)

**Problem:**
User message added optimistically, but WebSocket response may be out of order.

**Scenario:**
1. User sends message "A"
2. Optimistic message added with `pending: true`, id: "pending-123"
3. WebSocket sends thinking events for "A"
4. User sends message "B" before "A" completes
5. Optimistic message added with `pending: true`, id: "pending-456"
6. WebSocket sends response for "B" first (out of order)
7. `updatePendingMessage("pending-456", false)` called
8. WebSocket sends response for "A" second
9. Both messages cleared, but order might be wrong

**Dioxus Equivalent:**
Uses `client_message_id` to correlate (see `src/components.rs:226-250`).

**Solution:**
Implement message correlation by client_message_id.

---

## Memoization Gaps

### 1. Missing `React.memo` on Window Component

**Location:** `src/components/window/Window.tsx:26`

**Problem:**
Window component re-renders on every parent re-render, even when window props unchanged.

**Solution:**
```typescript
export const Window = React.memo(function Window({
  window: windowState,
  isActive,
  // ...
}: WindowProps) {
  // ...
});
```

---

### 2. Missing `useMemo` on Sorted Windows

**Location:** `src/components/desktop/Desktop.tsx:323-326`

**Problem:**
Sorting recalculates on every render, even when windows array unchanged.

**Current:**
```typescript
const sortedWindows = useMemo(
  () => [...windows].sort((a, b) => a.z_index - b.z_index),
  [windows],  // ANY change to windows triggers sort
);
```

**Better:**
```typescript
const sortedWindows = useMemo(() => {
  const visible = windows.filter(w => !w.minimized);
  return [...visible].sort((a, b) => a.z_index - b.z_index);
}, [windows.map(w => ({ id: w.id, z_index: w.z_index, minimized: w.minimized })).join(',')]);
```

---

### 3. Callback Dependencies Missing

**Location:** `src/components/apps/Chat/Chat.tsx:283`

**Problem:**
```typescript
const renderedMessages = useMemo(() => sortMessages(messages), [messages]);
```

But `sortMessages` function is defined outside component, so it's not in dependency array - OK.

However, in `src/components/desktop/Desktop.tsx:110-124`:

```typescript
const handleOpenApp = useCallback(
  async (app: AppDefinition) => {
    // ...
  },
  [desktopId, setDesktopError],  // Missing setError dependency!
);
```

`setError` is called on line 86 but not in deps.

---

### 4. Event Handler Recreation

**Location:** `src/components/window/Window.tsx:50-96`

**Problem:**
`onHeaderPointerDown` and `onResizeHandlePointerDown` are recreated on every render.

**Solution:**
Wrap in `useCallback` with proper dependencies.

---

## Summary of Critical Issues

| Severity | Issue | Location | Impact |
|----------|-------|----------|--------|
| CRITICAL | State duplication (two window arrays) | `stores/desktop.ts:5`, `stores/windows.ts:5` | Data inconsistency |
| CRITICAL | Race condition in WebSocket updates | `hooks/useWebSocket.ts:17-92` | UI shows wrong state |
| CRITICAL | z_index not calculated on focus | `stores/windows.ts:65-73` | Window layering broken |
| HIGH | Desktop store doesn't update windows array | `stores/desktop.ts:68-87` | Stale window data |
| HIGH | Minimize doesn't set minimized flag | `stores/desktop.ts:89-113` | Minimize broken |
| HIGH | No drag/resize throttling | `components/window/Window.tsx:66-96` | Network flood |
| MEDIUM | Excessive re-renders | `components/desktop/Desktop.tsx:35` | Poor performance |
| MEDIUM | Store lookups on every operation | `components/desktop/Desktop.tsx:130+` | O(n) cost |
| MEDIUM | Chat state not integrated | `stores/chat.ts:4-14` | Closed windows receive messages |

---

## Recommended Refactor Priority

1. **Phase 1 (Critical):** Consolidate stores into single source of truth
2. **Phase 2 (Critical):** Fix z_index calculation and atomic updates
3. **Phase 3 (High):** Implement drag/resize throttling
4. **Phase 4 (Medium):** Optimize store subscriptions and re-renders
5. **Phase 5 (Medium):** Add missing Dioxus features (theme, viewport, keyboard nav)
6. **Phase 6 (Low):** Add memoization and virtual scrolling

---

## Conclusion

The React implementation's state management is significantly more complex and error-prone than the Dioxus backup. The core issue is **state duplication** between multiple stores, which creates:

- Synchronization bugs
- Race conditions
- Performance issues
- Missing features present in Dioxus

The Dioxus architecture (single source of truth, reactive signals, atomic updates) is superior and should guide the React refactoring. The most impactful improvement would be **eliminating the separate `useWindowsStore` and consolidating all state in `useDesktopStore`**, following the Dioxus pattern of `desktop_state` containing everything.

Additionally, the Dioxus implementation has several polish features missing from React (theme toggling, keyboard navigation, proper throttling) that should be ported over.
