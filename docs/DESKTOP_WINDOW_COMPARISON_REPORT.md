# Desktop & Window Management Comparison Report

**Date:** February 6, 2026
**Versions Analyzed:** React sandbox-ui vs Dioxus sandbox-ui-backup
**Scope:** Desktop shell, window management, taskbar, prompt bar, icon grid

---

## Executive Summary

The React implementation provides a functional desktop window system but is missing several critical features present in the Dioxus backup. Major gaps include keyboard accessibility, proper pointer event handling, viewport bounds constraints, rate limiting for drag/resize operations, theme support, and proper mobile responsiveness.

**Critical Issues:** 7
**Important Issues:** 12
**Minor Issues:** 8

---

## 1. Component Comparison

| Component | React Location | Dioxus Location | Status | Notes |
|-----------|----------------|-----------------|--------|-------|
| Desktop Shell | `components/desktop/Desktop.tsx` | `desktop/shell.rs` | ✅ Complete | React has Desktop.tsx, Dioxus has shell.rs |
| Desktop Icons | `components/desktop/Icon.tsx` | `desktop/components/desktop_icons.rs` | ⚠️ Partial | Missing: double-click detection, press state, scale animation |
| Prompt Bar | `components/desktop/PromptBar.tsx` | `desktop/components/prompt_bar.rs` | ⚠️ Partial | Missing: theme toggle button |
| Taskbar | `components/desktop/Taskbar.tsx` | (integrated into PromptBar) | ❌ Different | React has separate Taskbar component |
| Window Component | `components/window/Window.tsx` | `desktop_window.rs` | ⚠️ Partial | Missing: keyboard shortcuts, drag threshold, bounds clamping |
| Window Manager | `components/window/WindowManager.tsx` | `desktop/components/workspace_canvas.rs` | ✅ Complete | Both filter minimized windows |
| WebSocket Handler | `hooks/useWebSocket.ts` | `desktop/ws.rs` | ✅ Complete | Both implement WebSocket client |
| State Management | `stores/windows.ts`, `stores/desktop.ts` | `desktop/state.rs` | ✅ Complete | Different paradigms (Zustand vs Signals) |
| Theme Support | None | `desktop/theme.rs` | ❌ Missing | Dioxus has light/dark theme toggle |
| Actions/API | `lib/api/desktop.ts` | `desktop/actions.rs` | ✅ Complete | Both handle desktop operations |

---

## 2. Bugs Found

### 2.1 Critical Bugs

#### 2.1.1 Missing Pointer Capture (Window Drag/Resize)
**Location:** `sandbox-ui/src/components/window/Window.tsx:50-96`
**Severity:** CRITICAL
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

---

#### 2.1.2 Missing Pointer Cancel Handler
**Location:** `sandbox-ui/src/components/window/Window.tsx`
**Severity:** CRITICAL
**Issue:** No `pointercancel` event handling. If the browser cancels a pointer event (e.g., system modal, touch gesture interception), the window remains in drag/resize state.

**Dioxus Implementation:**
```rust
// desktop_window.rs:333-351
onpointercancel: move |e| {
    let Some(active) = interaction() else { return; };
    if e.data().pointer_id() != active.pointer_id { return; }

    // Release pointer capture
    if let Some(web_event) = e.data().try_as_web_event() {
        if let Some(target) = web_event.current_target() {
            if let Ok(element) = target.dyn_into::<web_sys::Element>() {
                let _ = element.release_pointer_capture(active.pointer_id);
            }
        }
    }

    // Restore committed bounds
    live_bounds.set(Some(active.committed_bounds));
    interaction.set(None);
}
```

**Impact:** Window state can become corrupted after pointercancel events.

---

#### 2.1.3 No Window Bounds Clamping to Viewport
**Location:** `sandbox-ui/src/components/window/Window.tsx:50-96`
**Severity:** CRITICAL
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

---

#### 2.1.4 Missing Drag Threshold
**Location:** `sandbox-ui/src/components/window/Window.tsx:66-79`
**Severity:** IMPORTANT
**Issue:** Drag starts immediately on pointerdown, even if it's just a click. This causes windows to move slightly when users just intend to click/focus.

**Evidence:**
```tsx
// React - starts drag immediately
const handlePointerMove = (moveEvent: PointerEvent) => {
  // No threshold check
  const dx = moveEvent.clientX - dragStartRef.current.pointerX;
  const dy = moveEvent.clientY - dragStartRef.current.pointerY;
  onMove(...);
};
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:221-223 - 4px threshold
if dx.abs() < DRAG_THRESHOLD_PX && dy.abs() < DRAG_THRESHOLD_PX {
    return; // Don't move if movement is less than 4px
}
```

**Impact:** Poor UX - windows "jitter" when clicking on them. Makes it harder to click on titlebar without slight movement.

---

#### 2.1.5 No Rate Limiting on Move/Resize Events
**Location:** `sandbox-ui/src/components/window/Window.tsx:66-79`
**Severity:** IMPORTANT
**Issue:** Every pointer move event triggers a move/resize API call. This can spam the backend with hundreds of events per second during drag/resize.

**Evidence:**
```tsx
// React - sends on every move
const handlePointerMove = (moveEvent: PointerEvent) => {
  // ...
  onMove(windowState.id, nextX, nextY);  // Called for every event!
};
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:243-267 - Rate limited to 50ms intervals
const PATCH_INTERVAL_MS: i64 = 50;

InteractionMode::Drag => {
    queued_move.set(Some((next.x, next.y)));
    let elapsed = now_ms() - last_move_sent_ms();
    if elapsed >= PATCH_INTERVAL_MS {
        // Send immediately if 50ms elapsed
        if let Some((next_x, next_y)) = queued_move.write().take() {
            on_move.call((window_id_clone, next_x, next_y));
            last_move_sent_ms.set(now_ms());
        }
    } else if !move_flush_scheduled() {
        // Otherwise, schedule final flush
        move_flush_scheduled.set(true);
        spawn(async move {
            TimeoutFuture::new(wait_ms).await;
            // Send queued move
        });
    }
}
```

**Impact:** Backend overload, performance degradation, unnecessary network traffic. Can cause race conditions.

---

#### 2.1.6 Missing Keyboard Accessibility
**Location:** `sandbox-ui/src/components/window/Window.tsx:149-186`
**Severity:** IMPORTANT
**Issue:** No keyboard shortcuts for window management. Windows cannot be moved, resized, or closed via keyboard.

**Dioxus Keyboard Shortcuts:**
```rust
// desktop_window.rs:131-196
// Alt+F4 - Close window
if key == Key::F4 && modifiers.alt() {
    on_close.call(window_id.clone());
}

// Escape - Cancel drag/resize
if key == Key::Escape {
    live_bounds.set(Some(active.committed_bounds));
    interaction.set(None);
}

// Ctrl+M - Minimize
if key == Key::Character("m".to_string()) && modifiers.ctrl() && !modifiers.shift() {
    on_minimize.call(window_id.clone());
}

// Ctrl+Shift+M - Maximize/Restore
if key == Key::Character("m".to_string()) && modifiers.ctrl() && modifiers.shift() {
    if window.maximized { on_restore(...); } else { on_maximize(...); }
}

// Alt+Arrows - Move window
if modifiers.alt() && !modifiers.shift() {
    match key {
        Key::ArrowLeft => next.x -= KEYBOARD_STEP_PX,
        Key::ArrowRight => next.x += KEYBOARD_STEP_PX,
        Key::ArrowUp => next.y -= KEYBOARD_STEP_PX,
        Key::ArrowDown => next.y += KEYBOARD_STEP_PX,
        _ => return,
    }
}

// Alt+Shift+Arrows - Resize window
if modifiers.alt() && modifiers.shift() {
    match key {
        Key::ArrowLeft => next.width -= KEYBOARD_STEP_PX,
        Key::ArrowRight => next.width += KEYBOARD_STEP_PX,
        Key::ArrowUp => next.height -= KEYBOARD_STEP_PX,
        Key::ArrowDown => next.height += KEYBOARD_STEP_PX,
        _ => return,
    }
}
```

**Impact:** Poor accessibility. Keyboard-only users cannot manage windows. Power users lose productivity.

---

#### 2.1.7 Missing Mobile Window Constraints
**Location:** `sandbox-ui/src/components/window/Window.tsx`
**Severity:** IMPORTANT
**Issue:** No special handling for mobile viewports (≤1024px). Windows can be too large or positioned inappropriately on mobile devices.

**Dioxus Implementation:**
```rust
// desktop_window.rs:47-54
fn clamp_bounds(bounds: WindowBounds, viewport: (u32, u32), is_mobile: bool) -> WindowBounds {
    if is_mobile {
        return WindowBounds {
            x: 10, y: 10,
            width: vw as i32 - 20,  // Full width minus margins
            height: vh as i32 - 100, // Leave space for prompt bar
        };
    }
    // ... normal desktop clamping
}
```

**Impact:** Poor mobile UX. Windows may extend beyond viewport, overlap with UI, or be too small to use.

---

### 2.2 Important Bugs

#### 2.2.1 Missing Active Window Visual Indication
**Location:** `sandbox-ui/src/components/window/Window.css:20-22`
**Severity:** IMPORTANT
**Issue:** Active window only changes border color. Dioxus adds a prominent outline and increased shadow.

**React Implementation:**
```css
.window--active {
  border-color: rgba(96, 165, 250, 0.85);  /* Only border change */
}
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:125-129
let active_outline = if is_active {
    "2px solid var(--accent-bg, #3b82f6)"  // Prominent outline
} else {
    "none"
};
```

**Impact:** Difficult to see which window is active, especially with multiple similar windows.

---

#### 2.2.2 Taskbar Shows All Windows Including Minimized
**Location:** `sandbox-ui/src/components/desktop/Desktop.tsx:340-351`
**Severity:** IMPORTANT
**Issue:** PromptBar shows all windows including minimized ones. Should only show active/minimized window indicators.

**Evidence:**
```tsx
// Desktop.tsx:355-361
<PromptBar
  windows={sortedWindows}  // Shows ALL windows
  activeWindowId={activeWindowId}
  onSubmit={handlePromptSubmit}
  onFocusWindow={handleActivateWindow}
/>
```

**Dioxus Implementation:**
```rust
// prompt_bar.rs:59-71 - Only shows running apps
if !windows.is_empty() {
    div {
        class: "running-apps",
        for window in windows.iter() {
            RunningAppIndicator { ... }
        }
    }
}
```

**Impact:** Prompt bar cluttered with minimized windows. No visual distinction between minimized and active windows.

---

#### 2.2.3 Double-Click on Icon Not Detected
**Location:** `sandbox-ui/src/components/desktop/Icon.tsx`
**Severity:** IMPORTANT
**Issue:** Desktop icons use single click to open app. Should require double-click like traditional desktops, or have press state animation.

**React Implementation:**
```tsx
// Icon.tsx:10-13
<button onClick={() => onOpen(app)} title={app.name}>
  {/* No double-click detection, no press animation */}
</button>
```

**Dioxus Implementation:**
```rust
// desktop_icons.rs:44-66 - Double-click detection with animation
let mut last_click_time = use_signal(|| 0i64);
let mut is_pressed = use_signal(|| false);

let handle_click = move |_| {
    let now = js_sys::Date::now() as i64;
    let last = *last_click_time.read();

    if now - last >= 500 {  // Debounce to prevent double-open
        on_open_app.call(app_for_closure.clone());
        last_click_time.set(now);
    }

    is_pressed.set(true);  // Press animation
    spawn(async move {
        TimeoutFuture::new(150).await;  // 150ms animation
        is_pressed_clone.set(false);
    });
};
```

**Impact:** Accidental app launches. No visual feedback on click.

---

#### 2.2.4 Missing Theme Support
**Location:** `sandbox-ui/src/components/desktop/PromptBar.tsx`
**Severity:** IMPORTANT
**Issue:** No theme toggle button. Dioxus has light/dark theme with persistence.

**Dioxus Implementation:**
```rust
// prompt_bar.rs:30-40 - Theme toggle button
button {
    class: "prompt-theme-btn",
    onclick: move |_| on_toggle_theme.call(()),
    title: "Toggle theme",
    if current_theme == "dark" {
        "☀️"  // Show sun for dark theme (click to switch to light)
    } else {
        "🌙"  // Show moon for light theme (click to switch to dark)
    }
}
```

**Theme Persistence:**
```rust
// theme.rs:24-41
pub fn apply_theme_to_document(theme: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute("data-theme", theme);
        }
    }
}

pub async fn initialize_theme(...) {
    // Load from localStorage
    if let Some(theme) = get_cached_theme_preference() {
        apply_theme_to_document(&theme);
        current_theme.set(theme);
    }
    // Fetch from backend
    match fetch_user_theme_preference(user_id).await {
        Ok(theme) => {
            set_cached_theme_preference(&theme);
            apply_theme_to_document(&theme);
        }
        // ...
    }
}
```

**Impact:** Users cannot switch between light/dark themes. Theme preference not persisted.

---

#### 2.2.5 Window Content Doesn't Receive Window Bounds
**Location:** `sandbox-ui/src/components/window/Window.tsx:189-211`
**Severity:** MINOR
**Issue:** Terminal app doesn't receive current window dimensions for resizing.

**React Implementation:**
```tsx
// Window.tsx:195-199
<Suspense fallback={<div className="window__placeholder">Loading terminal...</div>}>
  <Terminal terminalId={windowId} />  // No width/height props!
</Suspense>
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:445-450
TerminalView {
    terminal_id: window.id.clone(),
    width: bounds.width,   // Current window width
    height: bounds.height, // Current window height
}
```

**Impact:** Terminal may not resize properly or may have scrollbars. Terminal content size doesn't match window size.

---

#### 2.2.6 Window Title Missing App Icon
**Location:** `sandbox-ui/src/components/window/Window.tsx:161-162`
**Severity:** MINOR
**Issue:** Window titlebar only shows text, no app icon.

**React Implementation:**
```tsx
// Window.tsx:161-162
<header className="window__header" onPointerDown={onHeaderPointerDown}>
  <span className="window__title">{windowState.title}</span>
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:386-390
div {
    style: "display: flex; align-items: center; gap: 0.5rem;",
    span { style: "font-size: 1rem;", {get_app_icon(&window.app_id)} }
    span { style: "font-weight: 500;", "{window.title}" }
}
```

**Impact:** Harder to identify windows by icon alone. Inconsistent with desktop icons.

---

#### 2.2.7 No Viewer Shell Integration
**Location:** `sandbox-ui/src/components/window/Window.tsx:189-211`
**Severity:** IMPORTANT
**Issue:** Writer and Files apps show placeholder text. Dioxus has ViewerShell component for rendering different content types.

**Dioxus Implementation:**
```rust
// desktop_window.rs:433-458
if let Some(viewer_props) = viewer_props.clone() {
    ViewerShell {
        window_id: window.id.clone(),
        desktop_id: desktop_id.clone(),
        descriptor: viewer_props.descriptor,
    }
} else {
    match window.app_id.as_str() {
        "chat" => rsx! { ChatView { actor_id: window.id.clone() } },
        "terminal" => rsx! { TerminalView { ... } },
        _ => rsx! { "App not yet implemented" }
    }
}
```

**Viewer Props for Apps:**
```rust
// actions.rs:13-37
fn viewer_props_for_app(app_id: &str) -> Option<serde_json::Value> {
    match app_id {
        "writer" => Some(serde_json::json!({
            "viewer": {
                "kind": "text",
                "resource": {
                    "uri": "file:///workspace/README.md",
                    "mime": "text/markdown"
                },
                "capabilities": { "readonly": false }
            }
        })),
        "files" => Some(serde_json::json!({
            "viewer": {
                "kind": "image",
                "resource": {
                    "uri": "data:image/svg+xml;base64,...",
                    "mime": "image/svg+xml"
                },
                "capabilities": { "readonly": true }
            }
        })),
        _ => None,
    }
}
```

**Impact:** Writer and Files apps are non-functional. Placeholder apps cannot be used.

---

#### 2.2.8 Prompt Bar Help Button Has No Functionality
**Location:** `sandbox-ui/src/components/desktop/PromptBar.tsx:18-20`
**Severity:** MINOR
**Issue:** Help button exists but has no click handler.

```tsx
<button className="prompt-help" type="button" aria-label="Help">
  ?
</button>
```

**Impact:** Users cannot access help documentation.

---

#### 2.2.9 Window Close Button Styling Issue
**Location:** `sandbox-ui/src/components/window/Window.css:49-58`
**Severity:** MINOR
**Issue:** Window close button has same styling as minimize/maximize. Should be red or have hover danger state.

**React Implementation:**
```css
.window__controls button {
  width: 24px;
  height: 24px;
  /* Same for all buttons */
}
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:416-425
button {
    class: "window-close",  // Special class for close button
    style: "font-size: 1.25rem; line-height: 1;",  // Larger
    // ... styling ...
}
```

**Impact:** Harder to identify close button. No visual warning for destructive action.

---

#### 2.2.10 Desktop State Error Message Positioning
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:62-80`
**Severity:** MINOR
**Issue:** Error/loading states are centered over desktop, may overlap windows.

**React CSS:**
```css
.desktop-state {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  /* Overlaps everything */
}
```

**Impact:** Error messages may be hidden behind windows. Confusing when desktop loads with existing windows.

---

#### 2.2.11 No Desktop Background Gradient Variation
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:1-7`
**Severity:** MINOR
**Issue:** Static gradient. Dioxus has CSS variables for theme-specific backgrounds.

**React CSS:**
```css
.desktop-shell {
  background: radial-gradient(circle at top right, #1f2937 0%, #0b1120 40%, #05070d 100%);
}
```

**Dioxus CSS:**
```css
:root[data-theme="dark"] {
  --bg-primary: #0f172a;
}
:root[data-theme="light"] {
  --bg-primary: #f8fafc;
}
```

**Impact:** Background doesn't change with theme. Less cohesive design system.

---

#### 2.2.12 Mobile Icon Grid Layout Issue
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:182-186`
**Severity:** MINOR
**Issue:** Desktop icons only change columns at 1025px breakpoint. Should also change at smaller breakpoints.

**React CSS:**
```css
@media (min-width: 1025px) {
  .desktop-icons {
    grid-template-columns: repeat(4, minmax(84px, 1fr));
  }
}
/* No smaller breakpoints */
```

**Dioxus CSS:**
```css
@media (max-width: 1024px) {
  .desktop-icons { gap: 1rem !important; }
}
@media (max-width: 640px) {
  .desktop-icons { grid-template-columns: repeat(2, 4rem) !important; }
}
```

**Impact:** Poor mobile layout on smaller screens (<1024px but >640px).

---

### 2.3 Minor Issues

#### 2.3.1 Window Resize Handle Not Visible on All Sides
**Location:** `sandbox-ui/src/components/window/Window.tsx:184`
**Severity:** MINOR
**Issue:** Only bottom-right resize handle. Dioxus has corner handle only but cursor indicates resize direction.

**React Implementation:**
```tsx
<div className="window__resize-handle" onPointerDown={onResizeHandlePointerDown} />
```

**Dioxus Implementation:**
```rust
// desktop_window.rs:462-485
if !is_mobile && !window.maximized {
    div {
        class: "resize-handle",
        style: "position: absolute; right: 0; bottom: 0; width: 16px; height: 16px; cursor: se-resize;",
        onpointerdown: ...
    }
}
```

**Impact:** Can only resize from bottom-right corner. Less intuitive for some users.

---

#### 2.3.2 No Window Drop Shadow Changes on Focus
**Location:** `sandbox-ui/src/components/window/Window.css:14-15`
**Severity:** MINOR
**Issue:** Shadow is static regardless of focus state.

**React CSS:**
```css
.window {
  box-shadow: 0 18px 45px rgba(2, 6, 23, 0.45);  /* Static */
}
```

**Dioxus CSS:**
```css
/* --shadow-lg is used, but Dioxus uses inline styles for active outline */
box-shadow: var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5));
```

**Impact:** Active windows don't visually "pop" as much. Reduced depth perception.

---

#### 2.3.3 Desktop Icons Lack Hover Scale Animation
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:38-40`
**Severity:** MINOR
**Issue:** Icons only change background on hover, no scale animation.

**React CSS:**
```css
.desktop-icon:hover {
  background: rgba(148, 163, 184, 0.2);
}
```

**Dioxus CSS/Inline:**
```rust
// desktop_icons.rs:68-79 - Dynamic scale based on is_pressed state
let scale = if *is_pressed.read() { "0.95" } else { "1.0" };
div {
    style: "transform: scale({scale}); transition: all 0.15s ease-out;",
}
```

**Impact:** Less responsive feel. No visual feedback on interaction.

---

#### 2.3.4 Prompt Bar Input Lacks Focus Ring
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:137-146`
**Severity:** MINOR
**Issue:** Input doesn't show focus state clearly.

**React CSS:**
```css
.prompt-input {
  border: 1px solid rgba(100, 116, 139, 0.65);
  /* No focus style */
}
```

**Impact:** Harder to tell when input is focused. Accessibility issue.

---

#### 2.3.5 No Desktop Icon Border Pulse on Press
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:25-60`
**Severity:** MINOR
**Issue:** No visual feedback when clicking desktop icons.

**Dioxus Implementation:**
```rust
// desktop_icons.rs:70-79
let border_color = if *is_pressed.read() {
    "#60a5fa"  // Blue border on press
} else {
    "#334155"  // Default gray
};
let shadow = if *is_pressed.read() {
    "0 2px 12px rgba(96, 165, 250, 0.5)"  // Blue glow on press
} else {
    "none"
};
```

**Impact:** Icons feel less responsive. No confirmation of click action.

---

#### 2.3.6 Window Title Text Overflow Not Handled
**Location:** `sandbox-ui/src/components/window/Window.css:35-42`
**Severity:** MINOR
**Issue:** Long window titles may overflow or be truncated without ellipsis.

**React CSS:**
```css
.window__title {
  white-space: nowrap;
  text-overflow: ellipsis;  /* Has ellipsis */
  overflow: hidden;
}
```

**Note:** This is actually handled correctly, just noting it exists.

**Impact:** (None - properly implemented)

---

#### 2.3.7 No Desktop Wallpaper Support
**Location:** Entire desktop implementation
**Severity:** MINOR
**Issue:** Desktop uses CSS gradient only. No support for custom wallpapers.

**Impact:** Less customization. Dioxus also doesn't have this but could be future enhancement.

---

#### 2.3.8 Status Indicator in Prompt Bar Lacks Animation
**Location:** `sandbox-ui/src/components/desktop/Desktop.css:168-180`
**Severity:** MINOR
**Issue:** Connection status doesn't pulse or animate when connecting.

**React CSS:**
```css
.prompt-status {
  background: rgba(217, 119, 6, 0.8);  /* Static orange */
}
```

**Dioxus Implementation:**
```rust
// prompt_bar.rs:74-84 - Uses emoji but no animation
span { if connected { "●" } else { "◐" } }
```

**Impact:** Less clear feedback during connection states. Harder to tell if still connecting vs failed.

---

## 3. Missing Features from Dioxus

### 3.1 Theme System
**Missing Component:** `desktop/theme.rs`
**Features Lost:**
- Light/dark theme toggle
- Theme persistence in localStorage
- Theme synchronization with backend API
- CSS custom properties for theme colors
- Dynamic theme switching without page reload

**Files to Create:**
```
sandbox-ui/src/lib/theme/
  ├── index.ts
  ├── theme.ts          // Theme types and definitions
  ├── useTheme.ts       // Custom hook for theme management
  └── theme.css         // CSS variables for themes
```

---

### 3.2 Keyboard Shortcuts
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

---

### 3.3 Pointer Event Enhancements
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

---

### 3.4 Bounds Clamping System
**Missing Component:** Window bounds clamping function
**Features Lost:**
- Minimum window size enforcement (200x160px)
- Maximum window size enforcement (viewport minus margins)
- Position clamping to keep windows on-screen
- Mobile-specific constraints (full width on mobile)

**Implementation Required:**
```tsx
// Create: components/window/utils.ts
export function clampBounds(
  bounds: WindowBounds,
  viewport: { width: number; height: number },
  isMobile: boolean
): WindowBounds {
  const MIN_WIDTH = 200;
  const MIN_HEIGHT = 160;

  if (isMobile) {
    return {
      x: 10,
      y: 10,
      width: viewport.width - 20,
      height: viewport.height - 100,
    };
  }

  return {
    x: Math.max(10, Math.min(bounds.x, viewport.width - bounds.width - 10)),
    y: Math.max(10, Math.min(bounds.y, viewport.height - bounds.height - 60)),
    width: Math.max(MIN_WIDTH, Math.min(bounds.width, viewport.width - 40)),
    height: Math.max(MIN_HEIGHT, Math.min(bounds.height, viewport.height - 120)),
  };
}
```

---

### 3.5 Rate-Limited Event Dispatcher
**Missing Component:** Event debouncing/throttling utility
**Features Lost:**
- 50ms interval for move/resize events
- Queued event batching
- Final flush on pointer up
- Reduced network traffic

**Implementation Required:**
```tsx
// Create: components/window/useRateLimitedCallback.ts
export function useRateLimitedCallback<T extends (...args: any[]) => void>(
  callback: T,
  delayMs: number = 50
): T {
  const lastCallRef = useRef(0);
  const queueRef = useRef<Parameters<T> | null>(null);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  return useCallback((...args: Parameters<T>) => {
    const now = Date.now();
    const elapsed = now - lastCallRef.current;

    queueRef.current = args;

    if (elapsed >= delayMs) {
      if (queueRef.current) {
        callback(...queueRef.current);
        queueRef.current = null;
        lastCallRef.current = now;
      }
    } else if (!timeoutRef.current) {
      timeoutRef.current = setTimeout(() => {
        if (queueRef.current) {
          callback(...queueRef.current);
          queueRef.current = null;
        }
        lastCallRef.current = Date.now();
        timeoutRef.current = null;
      }, delayMs - elapsed);
    }
  }, [callback, delayMs]) as T;
}
```

---

### 3.6 Desktop Icon Press Animation
**Missing Feature:** Visual feedback on icon press
**Features Lost:**
- Scale down on press (0.95)
- Border color change on press (blue)
- Shadow glow on press
- 150ms animation duration

**Implementation Required:**
```tsx
// Update: components/desktop/Icon.tsx
export function Icon({ app, onOpen }: IconProps) {
  const [isPressed, setIsPressed] = useState(false);

  const handleClick = () => {
    onOpen(app);
    setIsPressed(true);
    setTimeout(() => setIsPressed(false), 150);
  };

  return (
    <button
      className={`desktop-icon ${isPressed ? 'desktop-icon--pressed' : ''}`}
      onClick={handleClick}
      onMouseDown={() => setIsPressed(true)}
      onMouseUp={() => setIsPressed(false)}
      onMouseLeave={() => setIsPressed(false)}
    >
      {/* ... */}
    </button>
  );
}
```

```css
/* Update: components/desktop/Desktop.css */
.desktop-icon {
  transform: scale(1);
  transition: transform 0.15s ease-out, box-shadow 0.15s ease-out;
}

.desktop-icon--pressed {
  transform: scale(0.95);
}

.desktop-icon__emoji {
  transition: background 0.15s ease-out, border-color 0.15s ease-out, box-shadow 0.15s ease-out;
}

.desktop-icon--pressed .desktop-icon__emoji {
  border-color: #60a5fa;
  box-shadow: 0 2px 12px rgba(96, 165, 250, 0.5);
}
```

---

### 3.7 Viewer Shell Integration
**Missing Component:** ViewerShell for rendering content
**Features Lost:**
- Generic viewer for different content types
- Text viewer for Writer app
- Image viewer for Files app
- Markdown rendering support
- Resource URI handling

**Implementation Required:**
```tsx
// Create: components/viewers/ViewerShell.tsx
export function ViewerShell({
  windowId,
  desktopId,
  descriptor,
}: ViewerProps) {
  switch (descriptor.kind) {
    case 'text':
      return <TextViewer resource={descriptor.resource} />;
    case 'image':
      return <ImageViewer resource={descriptor.resource} />;
    default:
      return <div>Unknown viewer type: {descriptor.kind}</div>;
  }
}
```

---

### 3.8 App Props for Viewer Apps
**Missing Feature:** Viewer props for Writer/Files apps
**Implementation Required:**
```tsx
// Update: components/desktop/Desktop.tsx
const viewerPropsForApp = (appId: string) => {
  switch (appId) {
    case 'writer':
      return {
        viewer: {
          kind: 'text',
          resource: {
            uri: 'file:///workspace/README.md',
            mime: 'text/markdown',
          },
          capabilities: { readonly: false },
        },
      };
    case 'files':
      return {
        viewer: {
          kind: 'image',
          resource: {
            uri: 'data:image/svg+xml;base64,...',
            mime: 'image/svg+xml',
          },
          capabilities: { readonly: true },
        },
      };
    default:
      return null;
  }
};
```

---

## 4. Refactoring Opportunities

### 4.1 Consolidate Taskbar into Prompt Bar
**Current State:**
- React has separate `Taskbar.tsx` component
- Dioxus integrates running apps into `PromptBar`
- Both show similar information

**Recommendation:**
Merge Taskbar functionality into PromptBar to reduce component count and match Dioxus architecture.

**Before:**
```tsx
// Desktop.tsx - separate Taskbar
<PromptBar {...promptProps} />
{/* Taskbar not currently rendered in Desktop.tsx */}
```

**After:**
```tsx
// Desktop.tsx - integrated
<PromptBar
  {...promptProps}
  showTaskbar={true}  // New prop to show running apps as taskbar
/>
```

**Benefits:**
- Fewer components to maintain
- Consistent with Dioxus architecture
- Reduced prop drilling
- Better mobile layout control

---

### 4.2 Extract Window State Management to Hook
**Current State:**
- Window.tsx has inline pointer event handlers
- Ref management is manual
- No reusable drag/resize logic

**Recommendation:**
Create custom hook `useWindowInteraction` to handle drag/resize state.

**Implementation:**
```tsx
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

  const handleDragMove = useCallback((event: React.PointerEvent) => {
    if (!dragState.current) return;
    // ... move logic with rate limiting
  }, [onMove]);

  const endDrag = useCallback((event: React.PointerEvent) => {
    if (!dragState.current) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragState.current = null;
  }, []);

  return { startDrag, handleDragMove, endDrag, startResize, handleResizeMove, endResize };
}
```

**Benefits:**
- Reusable across Window components
- Easier to test
- Separation of concerns
- Better type safety

---

### 4.3 Use CSS Custom Properties for Theming
**Current State:**
- Theme values hardcoded in CSS
- No support for dynamic theming

**Recommendation:**
Implement CSS custom properties for all colors and use them consistently.

**Implementation:**
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

  /* Desktop */
  --desktop-bg: radial-gradient(circle at top right, var(--bg-secondary) 0%, #0b1120 40%, #05070d 100%);

  /* Prompt Bar */
  --prompt-bar-bg: rgba(2, 6, 23, 0.95);
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
- Better maintainability

---

### 4.4 Consolidate Window Store Operations
**Current State:**
- WindowsStore has many similar methods
- DesktopStore duplicates some logic

**Recommendation:**
Create unified window management utilities.

**Implementation:**
```tsx
// stores/windowOperations.ts
export const windowOperations = {
  move: (state: WindowsStore, id: string, x: number, y: number) => ({
    ...state,
    windows: updateWindow(state.windows, id, (w) => ({ ...w, x, y })),
  }),

  resize: (state: WindowsStore, id: string, w: number, h: number) => ({
    ...state,
    windows: updateWindow(state.windows, id, (win) => ({ ...win, width: w, height: h })),
  }),

  focus: (state: WindowsStore, id: string, zIndex: number) => ({
    ...state,
    windows: updateWindow(state.windows, id, (w) => ({ ...w, z_index: zIndex, minimized: false })),
  }),

  minimize: (state: WindowsStore, id: string) => ({
    ...state,
    windows: updateWindow(state.windows, id, (w) => ({ ...w, minimized: true, maximized: false })),
  }),

  // ... etc
};
```

**Benefits:**
- Testable pure functions
- Reduced code duplication
- Easier to add new operations
- Clear separation between store and business logic

---

### 4.5 Extract WebSocket Message Processing
**Current State:**
- Message processing is inline in `applyWsMessage` function
- Hard to test

**Recommendation:**
Create message processor object with handlers for each message type.

**Implementation:**
```tsx
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

---

### 4.6 Use Viewport Hook for Responsive Behavior
**Current State:**
- Window size is checked inline with viewport prop
- No centralized viewport management

**Recommendation:**
Create `useViewport` hook and use it throughout.

**Implementation:**
```tsx
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

**Usage:**
```tsx
// Window.tsx
const { width, height, isMobile } = useViewport();

const clampedBounds = useMemo(() => {
  return clampBounds(bounds, { width, height }, isMobile);
}, [bounds, width, height, isMobile]);
```

**Benefits:**
- Consistent viewport access
- Easy to add breakpoint helpers
- Responsive design in one place
- Testable viewport behavior

---

## 5. Integration Issues

### 5.1 WebSocket Store Synchronization
**Issue:** React has two separate stores (DesktopStore and WindowsStore) that need to stay in sync. Dioxus uses a single DesktopState signal.

**Current State:**
```tsx
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
Either:
1. Merge stores into single DesktopStore (like Dioxus)
2. Use event emitter pattern to auto-sync
3. Use Zustand middleware for sync

**Solution (Option 1 - Merge Stores):**
```tsx
// stores/desktop.ts (merged)
interface DesktopStore {
  windows: WindowState[];
  activeWindowId: string | null;
  wsConnected: boolean;

  // All window operations on single store
  focusWindow: (windowId: string, zIndex: number) => void;
  moveWindow: (windowId: string, x: number, y: number) => void;
  // ... etc
}
```

---

### 5.2 Desktop ID Propagation
**Issue:** Desktop ID is passed through many components. Dioxus uses a signal at the top level.

**Current State:**
```tsx
// Desktop.tsx -> WindowManager.tsx -> Window.tsx -> onMove() -> API
<Desktop desktopId="desktop-1">
  <WindowManager onMove={handleMoveWindow} />  // Desktop ID captured in callback
    <Window onMove={onMove} />
```

**Problem:**
- Deep prop drilling
- Callbacks need to capture desktop ID
- Hard to test components in isolation

**Recommendation:**
Use React Context for desktop ID.

**Implementation:**
```tsx
// context/DesktopContext.tsx
const DesktopContext = createContext<{ desktopId: string } | null>(null);

export function useDesktopId() {
  const context = useContext(DesktopContext);
  if (!context) {
    throw new Error('useDesktopId must be used within DesktopProvider');
  }
  return context.desktopId;
}

export function DesktopProvider({ desktopId, children }: { desktopId: string; children: React.ReactNode }) {
  return <DesktopContext.Provider value={{ desktopId }}>{children}</DesktopContext.Provider>;
}
```

**Usage:**
```tsx
// Window.tsx
const desktopId = useDesktopId();

const handleMove = (x: number, y: number) => {
  moveWindow(desktopId, windowId, x, y);  // No prop needed!
};
```

---

### 5.3 API Client Type Safety
**Issue:** API functions are loosely typed. Generated types exist but not fully utilized.

**Current State:**
```tsx
// lib/api/desktop.ts
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

**Problem:**
- Return type is `Promise<void>` - no actual response data
- No validation of response types
- Hard to catch API errors at type level

**Recommendation:**
Use generated types more aggressively.

**Implementation:**
```tsx
// lib/api/desktop.ts (with better typing)
export async function moveWindow(
  desktopId: string,
  windowId: string,
  x: number,
  y: number
): Promise<WindowState> {  // Return actual window state
  const response = await client.put<WindowState>(
    `/desktops/${desktopId}/windows/${windowId}/move`,
    { x, y }
  );
  return response.data;  // Type-safe!
}
```

---

### 5.4 Error State Integration
**Issue:** Errors are shown in Desktop component but not propagated to individual windows.

**Current State:**
```tsx
// Desktop.tsx:337-338
{loading && <div className="desktop-state desktop-state--loading">Loading desktop...</div>}
{!loading && error && <div className="desktop-state desktop-state--error">{error}</div>}
```

**Problem:**
- Errors are shown globally
- No per-window error states
- No retry mechanism

**Recommendation:**
Add per-window error handling and retry.

**Implementation:**
```tsx
// Window.tsx
const [error, setError] = useState<string | null>(null);
const [isRetrying, setIsRetrying] = useState(false);

const handleRetry = async () => {
  setIsRetrying(true);
  setError(null);
  try {
    await refreshWindow(windowId);
  } catch (err) {
    setError(err.message);
  } finally {
    setIsRetrying(false);
  }
};

{error && (
  <div className="window__error">
    <span>{error}</span>
    <button onClick={handleRetry} disabled={isRetrying}>
      {isRetrying ? 'Retrying...' : 'Retry'}
    </button>
  </div>
)}
```

---

### 5.5 Window Z-Index Race Conditions
**Issue:** Focus events from WebSocket may arrive out of order or conflict with local user actions.

**Current State:**
```tsx
// useWebSocket.ts:48-51
case 'window_focused': {
  windowsStore.focusWindow(message.window_id, message.z_index);
  desktopStore.setActiveWindow(message.window_id);  // May conflict with user click!
  return;
}
```

**Problem:**
- User clicks window (sets z-index locally)
- WebSocket focus event arrives (overwrites z-index)
- Windows may "jump" unexpectedly

**Recommendation:**
Add client-side z-index counter that syncs with server.

**Implementation:**
```tsx
// stores/windows.ts
interface WindowsStore {
  nextZIndex: number;  // Track highest z-index client-side
  getNextZIndex: () => number;
  // ...
}

export const useWindowsStore = create<WindowsStore>((set, get) => ({
  nextZIndex: 0,

  getNextZIndex: () => {
    const current = get().nextZIndex;
    set({ nextZIndex: current + 1 });
    return current + 1;
  },

  focusWindow: (windowId, serverZIndex) => {
    set((state) => {
      // Use max of client and server z-index
      const clientZIndex = state.getNextZIndex();
      const maxZIndex = Math.max(serverZIndex, clientZIndex);
      return {
        windows: updateWindow(state.windows, windowId, (w) => ({
          ...w,
          z_index: maxZIndex,
          minimized: false,
        })),
      };
    });
  },
}));
```

---

## 6. Performance Considerations

### 6.1 Unnecessary Re-renders
**Issue:** Desktop component re-renders on every window state change.

**Current State:**
```tsx
// Desktop.tsx:323-326
const sortedWindows = useMemo(
  () => [...windows].sort((a, b) => a.z_index - b.z_index),
  [windows],  // Re-sorts on any window change!
);
```

**Problem:**
- Sorting runs on every window move/resize
- Expensive operation during drag
- Can cause jank

**Recommendation:**
Use React.memo on Window components and only update when necessary.

**Implementation:**
```tsx
// Window.tsx
export const Window = React.memo<WindowProps>(
  ({ window: windowState, isActive, ...handlers }) => {
    // ... component
  },
  (prevProps, nextProps) => {
    // Only re-render if active state or bounds change
    return (
      prevProps.window.id === nextProps.window.id &&
      prevProps.isActive === nextProps.isActive &&
      prevProps.window.x === nextProps.window.x &&
      prevProps.window.y === nextProps.window.y &&
      prevProps.window.width === nextProps.window.width &&
      prevProps.window.height === nextProps.window.height
    );
  }
);
```

---

### 6.2 WebSocket Message Processing Overhead
**Issue:** Every WebSocket message triggers state updates which cause re-renders.

**Current State:**
```tsx
// useWebSocket.ts:17-92
function applyWsMessage(message: WsServerMessage): void {
  // Every message updates multiple stores
  windowsStore.moveWindow(...);  // Triggers render
  desktopStore.setActiveWindow(...);  // Triggers another render
}
```

**Problem:**
- Multiple renders per message
- Unnecessary component updates

**Recommendation:**
Batch state updates.

**Implementation:**
```tsx
// useWebSocket.ts
function applyWsMessage(message: WsServerMessage): void {
  // Batch updates using store's batch method (if available)
  // OR use React.startTransition

  React.startTransition(() => {
    const desktopStore = useDesktopStore.getState();
    const windowsStore = useWindowsStore.getState();

    switch (message.type) {
      case 'window_focused': {
        windowsStore.focusWindow(message.window_id, message.z_index);
        desktopStore.setActiveWindow(message.window_id);
        break;
      }
      // ... other cases
    }
  });
}
```

---

### 6.3 Drag/Resize Event Spam
**Issue:** Every pointer move event sends API request (see Section 2.1.5).

**Impact:**
- Hundreds of requests per second during drag
- Network bandwidth waste
- Backend CPU load

**Recommendation:**
Implement rate limiting (see Section 3.5).

---

## 7. Testing Recommendations

### 7.1 Window Interaction Tests
**Missing Tests:**
- Pointer capture/release
- Drag threshold
- Bounds clamping
- Rate limiting
- Keyboard shortcuts

**Test Cases to Add:**
```tsx
// Window.test.tsx
describe('Window drag', () => {
  it('should not start drag until movement exceeds threshold', () => {
    const { onMove } = renderWindow();
    firePointerDown(windowHeader, { clientX: 100, clientY: 100 });
    firePointerMove(101, 101);  // Only 1px move
    expect(onMove).not.toHaveBeenCalled();  // Should not move
  });

  it('should clamp window to viewport bounds', () => {
    const { onMove } = renderWindow({ x: 0, y: 0 }, { width: 800, height: 600 });
    firePointerDown(windowHeader);
    firePointerMove(-200, -200);  // Try to move off-screen
    expect(onMove).toHaveBeenCalledWith(windowId, 10, 10);  // Clamped
  });
});

describe('Window keyboard shortcuts', () => {
  it('should close window on Alt+F4', () => {
    const { onClose } = renderWindow();
    fireEvent.keyDown(windowEl, { key: 'F4', altKey: true });
    expect(onClose).toHaveBeenCalledWith(windowId);
  });

  it('should cancel drag on Escape', () => {
    const { onMove } = renderWindow();
    firePointerDown(windowHeader);
    firePointerMove(200, 200);
    fireEvent.keyDown(windowEl, { key: 'Escape' });
    expect(onMove).not.toHaveBeenCalled();  // Move canceled
  });
});
```

---

### 7.2 Store Synchronization Tests
**Missing Tests:**
- WebSocket message processing
- Store state consistency
- Race conditions

**Test Cases to Add:**
```tsx
// stores/windows.test.ts
describe('Window store sync', () => {
  it('should keep windowsStore and desktopStore in sync on focus', () => {
    const windowsStore = useWindowsStore.getState();
    const desktopStore = useDesktopStore.getState();

    processWsMessage({
      type: 'window_focused',
      window_id: 'win-1',
      z_index: 5,
    }, { windowsStore, desktopStore });

    expect(windowsStore.windows.find(w => w.id === 'win-1')?.z_index).toBe(5);
    expect(desktopStore.activeWindowId).toBe('win-1');
  });

  it('should handle out-of-order z-index updates', () => {
    // Test race condition handling
  });
});
```

---

## 8. Migration Path

### Phase 1: Critical Fixes (Week 1)
1. Add pointer capture/release
2. Implement bounds clamping
3. Add drag threshold
4. Implement rate limiting
5. Add pointercancel handler

### Phase 2: Missing Features (Week 2)
1. Implement keyboard shortcuts
2. Add theme system
3. Create ViewerShell component
4. Add app props for Writer/Files

### Phase 3: Refactoring (Week 3)
1. Consolidate stores (or add sync middleware)
2. Extract useWindowInteraction hook
3. Implement useViewport hook
4. Add DesktopContext

### Phase 4: Polish & Performance (Week 4)
1. Add desktop icon press animation
2. Optimize re-renders with React.memo
3. Batch state updates
4. Add comprehensive tests

---

## 9. Summary

The React implementation is functional but lacks critical window management features present in the Dioxus version. The most urgent fixes are:

1. **Pointer event handling** - Capture/release, pointercancel
2. **Bounds clamping** - Prevent windows from going off-screen
3. **Rate limiting** - Prevent API spam during drag/resize
4. **Keyboard accessibility** - Add shortcuts for window management
5. **Theme support** - Light/dark mode toggle

Medium-priority issues include:
- Desktop icon press animation
- ViewerShell for Writer/Files apps
- Store synchronization improvements
- Mobile-specific window constraints

Low-priority polish items:
- Visual feedback improvements
- Better error handling
- Performance optimizations

The Dioxus implementation demonstrates a more complete and polished desktop experience. Migrating these features to React will significantly improve usability, accessibility, and user experience.
