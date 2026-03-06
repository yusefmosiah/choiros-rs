# Web Desktop Window Management - Research Document

## Executive Summary

This document provides comprehensive technical requirements, design patterns, and architectural recommendations for implementing window management in a web desktop system using Dioxus (Rust/WebAssembly). Research covers existing ChoirOS implementation, web platform best practices, and production-ready patterns for drag-and-drop, resizing, z-index management, and multi-window handling.

---

## 1. Existing ChoirOS Implementation Analysis

### Current Architecture

**Backend (Rust - Ractor):**
- `DesktopActor` (`sandbox/src/actors/desktop.rs`) manages window state
- State stored in-memory with EventStore persistence
- Window operations via RPC: `OpenWindow`, `CloseWindow`, `MoveWindow`, `ResizeWindow`, `FocusWindow`
- WebSocket events: `window_opened`, `window_closed`, `window_moved`, `window_resized`, `window_focused`

**Frontend (Dioxus):**
- `FloatingWindow` component (`dioxus-desktop/src/desktop.rs:432-536`)
- Positioning via inline styles: `left`, `top`, `width`, `height`, `z-index`
- Stub drag/resize handlers (`desktop.rs:828-838`)
- Partial JS interop in `dioxus-desktop/src/interop.rs`

**Shared Types (`shared-types/src/lib.rs`):**
```rust
pub struct WindowState {
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z_index: u32,
    pub minimized: bool,
    pub maximized: bool,
    pub props: serde_json::Value,
}
```

### Gaps & Limitations

1. **Drag/Resize Not Implemented**: `start_drag` and `start_resize` functions exist but are stubs
2. **No Minimize/Maximize UI**: State exists but no controls in title bar
3. **Limited Z-Index Management**: Basic increment, no window docking or tiling
4. **No Window Snapping**: No edge detection or viewport constraints
5. **Mobile-First Missing**: Basic responsive logic, no touch gesture support
6. **No Modal Window Mode**: No backdrop or focus trap for dialogs
7. **Missing Keyboard Navigation**: No tab order management for window controls

---

## 2. Technical Requirements & Challenges

### 2.1 Window State Management

| Requirement | Description | Complexity |
|-------------|-------------|-------------|
| Position Tracking | Real-time (x, y) updates with boundary validation | Medium |
| Size Management | Width/height with min/max constraints, aspect ratio locks | Medium |
| Z-Index Stacking | Dynamic layering with focus promotion, always-on-top windows | High |
| Window States | Normal, minimized, maximized, fullscreen, modal | High |
| Persistence | Save state to backend via EventStore, restore on reload | Medium |
| Responsive Layout | Mobile (fullscreen), tablet (tiled), desktop (floating) | High |

### 2.2 Drag and Drop Positioning

**Technical Challenges:**
- **Pointer Events vs Mouse Events**: Need to use `pointerdown`, `pointermove`, `pointerup` for cross-device support (mouse, touch, pen)
- **Pointer Capture**: During drag, all pointer events must be captured to prevent losing target
- **Coordinate Systems**: Need to handle client coordinates vs viewport coordinates
- **Touch Action**: Use `touch-action: none` to prevent browser default pan/zoom during drag

**Required Features:**
1. Title bar drag initiation
2. Real-time position updates (60fps target)
3. Boundary constraints (viewport edges)
4. Drag ghost image (optional for polish)
5. Window snapping to edges (optional)

### 2.3 Window Controls

**Required Controls:**
- **Minimize Button**: Hide window, show in taskbar
- **Maximize Button**: Expand to viewport (minus prompt bar), restore to previous size
- **Close Button**: Remove window, clean up resources
- **Restore**: Undo maximize/minimize

**Implementation Options:**
```html
<!-- Standard Title Bar Pattern -->
<div class="titlebar">
  <div class="window-controls-left">
    <!-- Optional: Menu button, back/forward -->
  </div>
  <div class="window-title">
    <span class="window-icon">{icon}</span>
    <span class="window-title-text">{title}</span>
  </div>
  <div class="window-controls-right">
    <button class="control minimize" aria-label="Minimize">−</button>
    <button class="control maximize" aria-label="Maximize">□</button>
    <button class="control close" aria-label="Close">×</button>
  </div>
</div>
```

### 2.4 Multiple Window Handling & Focus Management

**Focus Management Rules:**
1. **Focus on Click**: Any window click brings it to front
2. **Focus Cascade**: Parent windows bring child windows forward
3. **Focus Retention**: Dialog windows keep focus while open
4. **Focus Return**: When dialog closes, return focus to previous window
5. **Active Window Tracking**: Maintain `active_window` state for keyboard navigation

**Keyboard Navigation:**
- **Tab**: Cycle through windows in z-index order
- **Shift+Tab**: Reverse cycle
- **Escape**: Close focused modal/minimize focused window
- **Alt+F4**: Close active window (Windows-style)

**Z-Index Strategies:**

```rust
// Incrementing strategy (current ChoirOS)
let new_z = state.next_z_index;
state.next_z_index += 1;

// Layered strategy (better for docking/tiling)
const TASKBAR_LAYER: u32 = 1000;
const NORMAL_LAYER: u32 = 2000;
const MODAL_LAYER: u32 = 3000;
const ALWAYS_ON_TOP: u32 = 9999;
```

### 2.5 Z-Index Layering & Window Stacking

**CSS Stacking Context Rules:**
1. `position: relative` creates stacking context
2. `position: absolute` creates stacking context when `z-index` is set
3. Nested stacking contexts are independent
4. Elements with higher `z-index` appear on top regardless of DOM order

**Recommended Layering:**
```css
/* Desktop layers */
.window-canvas {
  position: relative;
  z-index: 1;
}

.window {
  position: absolute;
  z-index: var(--z-index, 100);
}

.window.active {
  z-index: calc(var(--z-index) + 1000);
}

.window.modal {
  z-index: 3000;
}

.window.always-on-top {
  z-index: 9999;
}
```

**Stacking Order Rules:**
1. Root → lowest
2. Positioned elements with `z-index` → above root
3. Higher `z-index` → above lower `z-index`
4. Within same stacking context: DOM order matters
5. Transform/filters create new stacking contexts (be careful)

### 2.6 Resize Handles & Window Sizing

**Required Handles:**
- **8-point resize** (desktop): N, NE, E, SE, S, SW, W, NW corners + edges
- **4-point resize** (mobile): SE corner drag only
- **Constraints:**
  - Min width/height (per app or system default)
  - Max width/height (viewport or parent container)
  - Aspect ratio lock (for certain apps like terminal)
  - Grid snapping (for tiling windows)

**Resize Cursor Indicators:**
```css
.resize-handle-nw { cursor: nw-resize; }
.resize-handle-n  { cursor: n-resize; }
.resize-handle-ne { cursor: ne-resize; }
.resize-handle-e  { cursor: e-resize; }
.resize-handle-se { cursor: se-resize; }
.resize-handle-s  { cursor: s-resize; }
.resize-handle-sw { cursor: sw-resize; }
.resize-handle-w  { cursor: w-resize; }
```

**Edge Snapping:**
- **Snap to other windows**: Align edges when close (10px threshold)
- **Snap to viewport**: Magnetic edges (10-20px threshold)
- **Half-screen/Quarter-screen**: Hold Shift during resize (optional)

### 2.7 Title Bar Design Patterns

**Standard Patterns:**

| Pattern | Use Case | Example |
|---------|-------------|----------|
| Classic OS (Windows/Mac) | General purpose apps | [icon] Title [min][max][close] |
| IDE Style | Editor/terminal | [icon] Title [tabs] [menu][min][max][close] |
| Browser Style | Tabbed apps | [tab1][tab2][tab3][+] [min][max][close] |
| Mobile Style | Fullscreen apps | [←] Title [close][→] |

**Accessibility Requirements:**
```html
<div role="dialog" aria-label="{title}" aria-modal="{is_modal}">
  <div role="heading" aria-level="2">{title}</div>
  <!-- window controls need aria-label -->
  <button aria-label="Minimize window">−</button>
  <button aria-label="Maximize window">□</button>
  <button aria-label="Close window">×</button>
</div>
```

### 2.8 Best Practices Summary

**Performance:**
- Use `requestAnimationFrame` for smooth drag/resize (60fps)
- Debounce position updates to backend (100-300ms)
- Use CSS transforms instead of top/left for GPU acceleration
- Avoid reflows during drag (measure once, animate with transforms)

**Accessibility:**
- Maintain focus trap in modal windows
- Announce window state changes to screen readers
- Provide keyboard shortcuts for all window operations
- Use sufficient color contrast for active vs inactive windows
- Ensure minimum touch target size (44x44px minimum)

**Cross-Device:**
- Use Pointer Events API (unified mouse/touch/pen)
- Test on mobile touch, desktop mouse, and tablet hybrid
- Support both click and double-click (icon activation)
- Handle right-click context menus (future)

---

## 3. Recommended Data Structures

### 3.1 Enhanced WindowState

```rust
pub struct WindowState {
    pub id: String,
    pub app_id: String,
    pub title: String,
    
    // Position & Size
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    
    // Previous state (for restore from maximize)
    pub prev_rect: Option<WindowRect>,
    
    // Stacking
    pub z_index: u32,
    pub layer: WindowLayer,  // Normal, Modal, AlwaysOnTop
    
    // State
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub modal: bool,
    
    // Constraints
    pub min_width: Option<i32>,
    pub min_height: Option<i32>,
    pub max_width: Option<i32>,
    pub max_height: Option<i32>,
    pub aspect_ratio: Option<(u32, u32)>,  // width:height
    
    // Docking (for tiling)
    pub docked_edge: Option<DockEdge>,
    
    // App-specific data
    pub props: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WindowLayer {
    Normal = 0,
    Modal = 1,
    AlwaysOnTop = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
```

### 3.2 Enhanced DesktopState

```rust
pub struct DesktopState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
    pub apps: Vec<AppDefinition>,
    
    // Layout mode
    pub layout_mode: LayoutMode,
    
    // Constraints
    pub viewport: ViewportRect,
    pub screen_safety_margin: i32,  // Pixels to keep away from edge
    
    // Preferences
    pub snap_to_edges: bool,
    pub snap_to_windows: bool,
    pub snap_grid_size: Option<i32>,  // For grid snapping
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayoutMode {
    Floating,      // Desktop: free positioning
    Tiled,         // Desktop: grid-based tiling
    Mobile,         // Mobile: fullscreen only
}
```

### 3.3 Frontend State Management (Dioxus)

```rust
use dioxus::prelude::*;

// Window manager hook for global state
#[hook]
pub fn use_window_manager() -> WindowManagerContext {
    let windows = use_context::<WindowManagerContext>();
    windows
}

// Per-window state hook
#[hook]
pub fn use_window_state(window_id: String) -> WindowStateSignal {
    let windows = use_context::<WindowManagerContext>();
    
    let window = use_memo(move || {
        windows.read()
            .windows
            .iter()
            .find(|w| w.id == window_id)
            .cloned()
    });
    
    window
}

// Drag/resize operation state
#[derive(Clone, Debug)]
pub struct WindowOperation {
    pub window_id: String,
    pub operation_type: OperationType,
    pub start_x: i32,
    pub start_y: i32,
    pub start_width: i32,
    pub start_height: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OperationType {
    Drag,
    Resize { handle: ResizeHandle },
}
```

---

## 4. API Design Considerations

### 4.1 DesktopActor Message Extensions

```rust
pub enum DesktopActorMsg {
    // Existing
    OpenWindow { ... },
    CloseWindow { ... },
    MoveWindow { ... },
    ResizeWindow { ... },
    FocusWindow { ... },
    
    // New: Window state changes
    MinimizeWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    MaximizeWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    RestoreWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    ToggleFullscreen {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    
    // New: Window positioning features
    DockWindow {
        window_id: String,
        edge: DockEdge,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    UndockWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    
    // New: Batch operations
    SetWindowsLayout {
        layout_mode: LayoutMode,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    
    // New: Event streaming for real-time updates
    BeginWindowOperation {
        window_id: String,
        operation_type: OperationType,
        start_state: WindowOperation,
        reply: RpcReplyPort<Result<String, DesktopError>>,  // Returns operation_id
    },
    UpdateWindowOperation {
        operation_id: String,
        delta_x: i32,
        delta_y: i32,
        delta_width: i32,
        delta_height: i32,
    },
    EndWindowOperation {
        operation_id: String,
        final_state: WindowState,
        reply: RpcReplyPort<Result<WindowState, DesktopError>>,
    },
}
```

### 4.2 HTTP API Extensions

```
PUT  /desktop/{desktop_id}/windows/{window_id}/minimize
PUT  /desktop/{desktop_id}/windows/{window_id}/maximize
PUT  /desktop/{desktop_id}/windows/{window_id}/restore
PUT  /desktop/{desktop_id}/windows/{window_id}/fullscreen
PUT  /desktop/{desktop_id}/windows/{window_id}/dock
PUT  /desktop/{desktop_id}/windows/{window_id}/undock

# Batch operations (for performance)
POST /desktop/{desktop_id}/windows/batch-move
Body: { moves: [{window_id, x, y}, ...] }

POST /desktop/{desktop_id}/windows/batch-resize
Body: { resizes: [{window_id, width, height}, ...] }
```

### 4.3 WebSocket Event Extensions

```json
{
  "type": "window_minimized",
  "window_id": "win-123"
}

{
  "type": "window_maximized",
  "window_id": "win-123",
  "prev_rect": {"x": 100, "y": 100, "width": 600, "height": 400}
}

{
  "type": "window_operation_started",
  "operation_id": "op-456",
  "window_id": "win-123",
  "operation_type": "drag"
}

{
  "type": "window_operation_updated",
  "operation_id": "op-456",
  "window_id": "win-123",
  "delta_x": 5,
  "delta_y": -3
}
```

---

## 5. JS Interop Requirements

### 5.1 Drag & Resize Implementation

**Required Web APIs:**
- `PointerEvent` API (`pointerdown`, `pointermove`, `pointerup`)
- `Element.setPointerCapture()` / `releasePointerCapture()`
- `requestAnimationFrame()` for smooth updates
- `getBoundingClientRect()` for boundary calculations

**Dioxus Interop Pattern:**

```rust
// dioxus-desktop/src/interop.rs

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::{PointerEvent, Element};

/// Begin window drag operation
#[wasm_bindgen]
pub fn start_window_drag(
    window_id: String,
    on_move_callback: &js_sys::Function,
    on_end_callback: &js_sys::Function,
) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    
    // Find window element by ID
    let window_el = document
        .get_element_by_id(&format!("window-{}", window_id))
        .expect("Window element not found");
    
    // Track operation state
    let is_dragging = std::rc::Rc::new(std::cell::RefCell::new(false));
    let start_x = std::rc::Rc::new(std::cell::RefCell::new(0i32));
    let start_y = std::rc::Rc::new(std::cell::RefCell::new(0i32));
    let window_x = std::rc::Rc::new(std::cell::RefCell::new(0i32));
    let window_y = std::rc::Rc::new(std::cell::RefCell::new(0i32));
    
    // Pointer down handler
    {
        let is_dragging = is_dragging.clone();
        let start_x = start_x.clone();
        let start_y = start_y.clone();
        let window_x = window_x.clone();
        let window_y = window_y.clone();
        
        let pointerdown_closure = Closure::wrap(Box::new(move |e: PointerEvent| {
            *is_dragging.borrow_mut() = true;
            *start_x.borrow_mut() = e.client_x();
            *start_y.borrow_mut() = e.client_y();
            
            let rect = window_el.get_bounding_client_rect();
            *window_x.borrow_mut() = rect.x() as i32;
            *window_y.borrow_mut() = e.y() as i32;
            
            // Capture pointer for consistent event delivery
            let _ = window_el.set_pointer_capture(e.pointer_id());
            
            e.stop_propagation();
        }) as Box<dyn FnMut(PointerEvent)>);
        
        window_el.add_event_listener_with_callback(
            "pointerdown",
            pointerdown_closure.as_ref().unchecked_ref()
        );
        pointerdown_closure.forget();
    }
    
    // Pointer move handler (attached to document for capture)
    {
        let is_dragging = is_dragging.clone();
        let start_x = start_x.clone();
        let start_y = start_y.clone();
        let window_x = window_x.clone();
        let window_y = window_y.clone();
        
        let pointermove_closure = Closure::wrap(Box::new(move |e: PointerEvent| {
            if !*is_dragging.borrow() {
                return;
            }
            
            let delta_x = e.client_x() - *start_x.borrow();
            let delta_y = e.client_y() - *start_y.borrow();
            let new_x = *window_x.borrow() + delta_x;
            let new_y = *window_y.borrow() + delta_y;
            
            // Call Dioxus callback
            let _ = on_move_callback.call2(
                &js_sys::JsValue::undefined(),
                &js_sys::JsValue::from(new_x),
                &js_sys::JsValue::from(new_y),
            );
            
            e.prevent_default();
        }) as Box<dyn FnMut(PointerEvent)>);
        
        document.add_event_listener_with_callback(
            "pointermove",
            pointermove_closure.as_ref().unchecked_ref()
        );
        pointermove_closure.forget();
    }
    
    // Pointer up handler
    {
        let is_dragging = is_dragging.clone();
        let pointer_id = std::cell::Cell::new(0u32);
        
        let pointerup_closure = Closure::wrap(Box::new(move |e: PointerEvent| {
            if *is_dragging.borrow() {
                *is_dragging.borrow_mut() = false;
                
                // Release pointer capture
                let _ = window_el.release_pointer_capture(e.pointer_id());
                
                // Call end callback
                let _ = on_end_callback.call0(&js_sys::JsValue::undefined());
            }
        }) as Box<dyn FnMut(PointerEvent)>);
        
        document.add_event_listener_with_callback(
            "pointerup",
            pointerup_closure.as_ref().unchecked_ref()
        );
        pointerup_closure.forget();
    }
}

/// Begin window resize operation
#[wasm_bindgen]
pub fn start_window_resize(
    window_id: String,
    handle_type: String,  // "nw", "n", "ne", "e", "se", "s", "sw", "w"
    on_resize_callback: &js_sys::Function,
    on_end_callback: &js_sys::Function,
) {
    // Similar implementation to drag, but with delta width/height
    // Calculate based on handle type and pointer movement
}
```

### 5.2 Dioxus Integration

```rust
// dioxus-desktop/src/desktop.rs

#[component]
pub fn FloatingWindow(
    window: WindowState,
    is_active: bool,
    viewport: (u32, u32),
    on_close: Callback<String>,
    on_focus: Callback<String>,
    on_move: Callback<(String, i32, i32)>,
    on_resize: Callback<(String, i32, i32)>,
) -> Element {
    let window_id = window.id.clone();
    let mut is_dragging = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0i32, 0i32));
    
    // Initialize drag from JS interop
    use_effect(move || {
        let window_id = window_id.clone();
        let on_move = on_move.clone();
        let on_resize = on_resize.clone();
        
        spawn(async move {
            let on_move_js = Closure::wrap(Box::new(move |x: i32, y: i32| {
                on_move.call((window_id.clone(), x, y));
            }) as Box<dyn FnMut(i32, i32)>);
            
            let on_move_js_ref = on_move_js.as_ref().unchecked_ref();
            
            interop::start_window_drag(
                window_id,
                on_move_js_ref,
                &js_sys::Function::new_no_args(|| {})
            );
            
            on_move_js.forget();
        });
    });
    
    let (vw, vh) = viewport;
    let is_mobile = vw <= 1024;
    
    rsx! {
        div {
            class: "floating-window",
            class: if is_active { "active" },
            style: "position: absolute; left: {window.x}px; top: {window.y}px; \
                    width: {window.width}px; height: {window.height}px; \
                    z-index: {window.z_index};",
            onclick: move |_| on_focus.call(window_id.clone()),
            
            // Title bar with drag handle
            div {
                class: "window-titlebar",
                style: "cursor: grab; touch-action: none;",
                onpointerdown: move |e| {
                    if !is_mobile {
                        is_dragging.set(true);
                        drag_offset.set((
                            e.client_x() - window.x,
                            e.client_y() - window.y,
                        ));
                    }
                },
                
                span { "{window.title}" }
                
                div {
                    class: "window-controls",
                    button {
                        class: "control minimize",
                        onclick: move |e| {
                            e.stop_propagation();
                            // Call minimize API
                        },
                        "−"
                    }
                    button {
                        class: "control maximize",
                        onclick: move |e| {
                            e.stop_propagation();
                            // Call maximize API
                        },
                        "□"
                    }
                    button {
                        class: "control close",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_close.call(window_id.clone());
                        },
                        "×"
                    }
                }
            }
            
            // Window content
            div {
                class: "window-content",
                // Render app content based on app_id
            }
            
            // Resize handles (desktop only)
            if !is_mobile {
                div {
                    class: "resize-handle-se",
                    style: "cursor: se-resize; touch-action: none;",
                    onpointerdown: move |e| {
                        e.stop_propagation();
                        // Initialize resize from JS interop
                    }
                }
                
                // Add other handles (nw, n, ne, e, s, sw, w) as needed
            }
        }
    }
}
```

### 5.3 Performance Optimizations

**Batch Updates:**
```rust
// Debounce rapid position updates
let mut debounce_timer = use_signal(|| None::<u32>);

let update_position = use_callback(move |(window_id, x, y)| {
    debounce_timer.set(Some(window.request_animation_frame(move || {
        on_move.call((window_id, x, y));
    })));
});
```

**GPU Acceleration:**
```css
.window {
  /* Use transform for positioning (GPU accelerated) */
  will-change: transform;
  transform: translate3d(var(--x), var(--y), 0);
}
```

---

## 6. Best Practices & Patterns

### 6.1 Drag & Drop Best Practices

**1. Use Pointer Events API**
- ✅ Do: Use `pointerdown`/`pointermove`/`pointerup`
- ❌ Don't: Use legacy `mousedown`/`mousemove`/`mouseup`

**2. Implement Pointer Capture**
```rust
// Capture ensures consistent event delivery during drag
element.set_pointer_capture(pointer_id)?;
```

**3. Prevent Default Touch Actions**
```css
.window-titlebar, .resize-handle {
  touch-action: none;  /* Prevent browser pan/zoom */
}
```

**4. Use requestAnimationFrame**
```rust
// Smooth 60fps updates instead of raw event frequency
let callback = move || {
    window.request_animation_frame(move |timestamp| {
        // Update position
    });
};
```

**5. Handle Cancel Events**
```rust
// Browser may cancel pointer if it interprets as pan/zoom
element.add_event_listener("pointercancel", closure);
```

### 6.2 Z-Index Management Best Practices

**1. Use Layers Instead of Raw z-index**
```rust
pub enum WindowLayer {
    Taskbar = 1000,
    Normal = 2000,
    Modal = 3000,
    AlwaysOnTop = 9999,
}
```

**2. Increment Within Layers**
```rust
// When focusing a normal window
let new_z = max(normal_windows_z) + 1;
```

**3. Respect Modal Hierarchy**
```rust
// Modal windows always above normal
if new_window.modal {
    new_z = MODAL_LAYER_BASE + modals_count;
} else {
    new_z = NORMAL_LAYER_BASE + normal_count;
}
```

**4. Avoid z-index Wars**
- Use CSS variables for z-index
- Keep values in ranges (1000-9999)
- Document layering in comments

### 6.3 Accessibility Best Practices

**1. Focus Management**
```html
<!-- Modal windows trap focus -->
<div role="dialog" aria-modal="true">
  <button autofocus>Close</button>
</div>
```

**2. Keyboard Navigation**
- Tab through window controls
- Escape to close/minimize
- Alt+Tab to switch windows

**3. Screen Reader Support**
```html
<div aria-label="Chat window" role="dialog">
  <h2 aria-level="2">Chat</h2>
  <!-- Content -->
</div>
```

**4. Visual Indicators**
- High contrast active vs inactive
- Focus rings visible on all controls
- Minimum 3:1 contrast ratio

### 6.4 Responsive Design Best Practices

**1. Mobile-First Approach**
```css
@media (max-width: 768px) {
  .window {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    z-index: 100;
  }
  
  .window-titlebar {
    display: flex;
    justify-content: space-between;
  }
  
  .resize-handle {
    display: none;
  }
}
```

**2. Breakpoint Strategy**
```css
/* Mobile (<768px) */
@media (max-width: 767px) { }

/* Tablet (768px-1024px) */
@media (min-width: 768px) and (max-width: 1023px) { }

/* Desktop (>=1024px) */
@media (min-width: 1024px) { }
```

**3. Touch Targets**
```css
button.window-control {
  min-width: 44px;
  min-height: 44px;
}
```

### 6.5 Performance Best Practices

**1. Reduce Reflows**
```css
.window {
  /* Avoid triggering layout recalculations */
  will-change: transform, opacity;
}
```

**2. Batch DOM Updates**
```rust
// Collect changes, then update once
let mut changes = Vec::new();
// ... collect changes
apply_changes(changes);
```

**3. Use CSS Variables**
```css
.window {
  --x: {window.x}px;
  --y: {window.y}px;
  --width: {window.width}px;
  --height: {window.height}px;
  
  transform: translate3d(var(--x), var(--y), 0);
}
```

**4. Debounce Network Requests**
```rust
// Don't spam the backend on every pixel change
let debounce_timer = use_signal(|| None::<u32>);

let send_position_update = move |(window_id, x, y)| {
    debounce_timer.set(Some(window.request_animation_frame(move || {
        // Send after debounce delay
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            api::move_window(&desktop_id, &window_id, x, y).await;
        });
    })));
};
```

---

## 7. Architectural Recommendations

### 7.1 Frontend Architecture

**Component Hierarchy:**
```
Desktop (root)
├── DesktopIcons (app launcher)
├── WindowCanvas (container for windows)
│   ├── FloatingWindow (per window instance)
│   │   ├── WindowTitlebar (drag handle)
│   │   │   ├── WindowIcon
│   │   │   ├── WindowTitle
│   │   │   └── WindowControls
│   │   │       ├── MinimizeButton
│   │   │       ├── MaximizeButton
│   │   │       └── CloseButton
│   │   ├── WindowContent (app iframe/component)
│   │   └── ResizeHandles (8 handles)
│   │       ├── NWHandle
│   │       ├── NHandle
│   │       ├── NEHandle
│   │       ├── EHandle
│   │       ├── SEHandle
│   │       ├── SHandle
│   │       ├── SWHandle
│   │       └── WHandle
└── PromptBar (command input + taskbar)
    ├── RunningAppsIndicator
    └── CommandLineInput
```

**State Management Pattern:**
```rust
// Global context for window manager
#[derive(Clone, Debug)]
pub struct WindowManagerContext {
    pub windows: Signal<Vec<WindowState>>,
    pub active_window: Signal<Option<String>>,
    pub layout_mode: Signal<LayoutMode>,
    pub operations: Signal<HashMap<String, WindowOperation>>,
}

// Use provider in root
fn App() -> Element {
    use_context_provider(|| WindowManagerContext {
        windows: use_signal(Vec::new),
        active_window: use_signal(None),
        layout_mode: use_signal(LayoutMode::Floating),
        operations: use_signal(HashMap::new()),
    });
    
    rsx! {
        Desktop { }
    }
}
```

### 7.2 Backend Architecture

**Enhanced DesktopActor:**
```rust
pub struct DesktopState {
    // Existing
    windows: HashMap<String, WindowState>,
    apps: HashMap<String, AppDefinition>,
    active_window: Option<String>,
    next_z_index: u32,
    
    // New: Layer tracking
    normal_windows_z: u32,
    modal_windows_z: u32,
    
    // New: Operation tracking
    active_operations: HashMap<String, WindowOperation>,
    
    // New: Layout constraints
    viewport_rect: ViewportRect,
    snap_settings: SnapSettings,
}

pub struct SnapSettings {
    pub enabled: bool,
    pub snap_to_edges: bool,
    pub snap_to_windows: bool,
    pub edge_threshold: i32,
    pub window_threshold: i32,
    pub grid_size: Option<i32>,
}
```

**Message Processing Pipeline:**
```
Client → DesktopActor
    ↓ BeginWindowOperation (start drag/resize)
    ↓ Stream UpdateWindowOperation (during move)
    ↓ UpdateWindowOperation (during move)
    ↓ UpdateWindowOperation (during move)
    ↓ EndWindowOperation (commit final state)
    ↓ Broadcast WebSocket event
    ↓ Client renders new position
```

### 7.3 WebSocket Real-Time Updates

**Event Broadcasting:**
```rust
// When window operation completes
async fn handle_end_operation(
    state: &mut DesktopState,
    window_id: String,
    final_state: WindowState,
) -> Result<(), DesktopError> {
    // Update state
    state.windows.insert(window_id.clone(), final_state.clone());
    
    // Broadcast to all connected clients
    if let Some(broadcast_tx) = state.broadcast_tx.as_ref() {
        let _ = broadcast_tx.send(WsEvent::WindowMoved {
            window_id: window_id.clone(),
            x: final_state.x,
            y: final_state.y,
        });
    }
    
    // Append event for persistence
    self.append_event_unit(EVENT_WINDOW_MOVED, json!({
        "window_id": window_id,
        "x": final_state.x,
        "y": final_state.y,
    }), state).await?;
    
    Ok(())
}
```

**Optimistic Updates:**
```rust
// Client updates immediately, server confirms
function onDragStart(windowId) {
    setDragging(true);
    
    // Local update first (instant feedback)
    updateWindowPosition(windowId, newX, newY);
    
    // Then notify server (async)
    api.moveWindow(desktopId, windowId, newX, newY);
}

// Server confirms via WebSocket
function onWindowMoved(event) {
    // Reconcile with server state
    if (event.window_id === windowId) {
        updateWindowPosition(windowId, event.x, event.y);
    }
}
```

---

## 8. Testing Strategy

### 8.1 Unit Tests (Rust)

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_window_focus_increments_z_index() {
        let mut state = DesktopState::new();
        
        // Open window 1
        let w1 = open_window(&mut state, "app1", "Window 1").await;
        assert_eq!(w1.z_index, 100);
        
        // Open window 2
        let w2 = open_window(&mut state, "app2", "Window 2").await;
        assert_eq!(w2.z_index, 101);
        
        // Focus window 1 (should increment above window 2)
        focus_window(&mut state, &w1.id).await;
        assert_eq!(state.windows[&w1.id].z_index, 102);
    }
    
    #[tokio::test]
    async fn test_minimize_restore_preserves_position() {
        let mut state = DesktopState::new();
        let window = open_window(&mut state, "app1", "Window 1").await;
        let original_x = window.x;
        let original_y = window.y;
        let original_width = window.width;
        let original_height = window.height;
        
        // Maximize
        maximize_window(&mut state, &window.id).await;
        assert!(state.windows[&window.id].maximized);
        
        // Restore
        restore_window(&mut state, &window.id).await;
        assert!(!state.windows[&window.id].maximized);
        assert_eq!(state.windows[&window.id].x, original_x);
        assert_eq!(state.windows[&window.id].y, original_y);
    }
}
```

### 8.2 Integration Tests (API)

```rust
#[tokio::test]
async fn test_drag_resize_flow() {
    let app = spawn_test_app().await;
    
    // Open window
    let window = open_window(&desktop_id, "chat", "Chat").await;
    
    // Begin drag
    let op_id = begin_operation(&desktop_id, &window.id, Drag).await;
    
    // Update position multiple times
    update_operation(&desktop_id, &op_id, 10, 20, 0, 0).await;
    update_operation(&desktop_id, &op_id, 20, 40, 0, 0).await;
    update_operation(&desktop_id, &op_id, 30, 60, 0, 0).await;
    
    // End drag
    let final = end_operation(&desktop_id, &op_id, new_state).await;
    assert_eq!(final.x, 130);
    assert_eq!(final.y, 160);
}
```

### 8.3 E2E Tests (agent-browser)

```bash
# test-window-drag.sh
agent-browser open http://localhost:3000
agent-browser open_chat_window
agent-browser screenshot tests/screenshots/window-open.png

agent-browser drag_window_by_id "chat-window" dx:100 dy:50
agent-browser screenshot tests/screenshots/window-dragged.png

agent-browser resize_window_by_id "chat-window" width:800 height:600
agent-browser screenshot tests/screenshots/window-resized.png

agent-browser close_window_by_id "chat-window"
agent-browser screenshot tests/screenshots/window-closed.png
```

---

## 9. Implementation Roadmap

### Phase 1: Core Window Management (Priority: High)

**Backend:**
- [ ] Extend `WindowState` with `prev_rect`, `layer`, constraints
- [ ] Add `MinimizeWindow`, `MaximizeWindow`, `RestoreWindow` messages
- [ ] Add z-index layer tracking (normal vs modal)
- [ ] Implement window constraint validation

**Frontend:**
- [ ] Implement `start_window_drag` in `interop.rs`
- [ ] Implement `start_window_resize` with 8 handles
- [ ] Add minimize/maximize/close buttons to title bar
- [ ] Add state transition animations (minimize → dock, maximize → expand)

**Testing:**
- [ ] Unit tests for z-index management
- [ ] Unit tests for minimize/restore cycle
- [ ] Integration tests for drag API

### Phase 2: Drag & Polish (Priority: High)

**Backend:**
- [ ] Add batch move/resize operations
- [ ] Implement window snapping logic
- [ ] Add viewport constraint enforcement

**Frontend:**
- [ ] Implement pointer capture
- [ ] Add drag ghost image
- [ ] Add snap visual feedback (magnetic edges)
- [ ] Optimize with `requestAnimationFrame`
- [ ] Add cursor changes (grab, grabbing, resize cursors)

**Testing:**
- [ ] E2E tests for drag behavior
- [ ] Cross-browser tests (Chrome, Firefox, Safari)
- [ ] Performance tests (60fps validation)

### Phase 3: Accessibility & Mobile (Priority: Medium)

**Frontend:**
- [ ] Add keyboard navigation (Tab, Escape, Alt+Tab)
- [ ] Implement ARIA roles and labels
- [ ] Add focus trap for modal windows
- [ ] Mobile-first responsive layout
- [ ] Touch gesture support (double-tap to close)

**Testing:**
- [ ] Screen reader testing (NVDA, VoiceOver)
- [ ] Keyboard-only navigation tests
- [ ] Mobile device testing (iOS, Android)

### Phase 4: Advanced Features (Priority: Low)

**Backend:**
- [ ] Tiled window layout mode
- [ ] Window docking API
- [ ] Multi-monitor support
- [ ] Window grouping/parenting

**Frontend:**
- [ ] Tiled window UI
- [ ] Visual docking indicators
- [ ] Window tabs (browser-style)
- [ ] Animations (minimize to dock, restore animation)

**Testing:**
- [ ] Multi-monitor tests
- [ ] Complex window hierarchy tests

---

## 10. References & Resources

### Web Platform APIs
- [Pointer Events API](https://developer.mozilla.org/en-US/docs/Web/API/Pointer_events)
- [CSS Positioned Layout](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Positioned_layout)
- [HTML Drag and Drop API](https://web.dev/articles/drag-and-drop/)
- [Dialog (Modal) Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)

### CSS References
- [Understanding z-index](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Positioned_layout/Understanding_z-index)
- [CSS Transform](https://developer.mozilla.org/en-US/docs/Web/CSS/transform)
- [will-change property](https://developer.mozilla.org/en-US/docs/Web/CSS/will-change)

### Dioxus & Rust
- [Dioxus Documentation](https://dioxuslabs.com/docs)
- [WASM Bindgen Guide](https://rustwasm.github.io/wasm-bindgen/)
- [Ractor Documentation](https://docs.rs/ractor/latest/ractor/)

### Design Patterns
- [Window Management UI Patterns](https://ui-patterns.com/patterns/window-management)
- [Drag and Drop UX](https://www.nngroup.com/articles/drag-and-drop-in-the-modern-ui)
- [Accessible Modal Dialogs](https://w3c.github.io/aria-practices/examples/dialog-modal/)

---

## Appendix: Code Snippets

### A.1 CSS Window Styles

```css
:root {
  --window-bg: #1f2937;
  --window-border: #374151;
  --titlebar-bg: #111827;
  --active-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
  --inactive-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
}

.window {
  position: absolute;
  background: var(--window-bg);
  border: 1px solid var(--window-border);
  border-radius: 8px;
  box-shadow: var(--inactive-shadow);
  will-change: transform;
  transition: box-shadow 0.2s ease;
}

.window.active {
  box-shadow: var(--active-shadow);
}

.window-titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--titlebar-bg);
  border-bottom: 1px solid var(--window-border);
  border-radius: 8px 8px 0 0;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.window-titlebar:active {
  cursor: grabbing;
}

.window-controls {
  display: flex;
  gap: 8px;
}

.window-control {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: #9ca3af;
  cursor: pointer;
  font-size: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.window-control:hover {
  background: rgba(255, 255, 255, 0.1);
}

.window-control.close:hover {
  background: #ef4444;
  color: white;
}

.window-content {
  overflow: hidden;
  flex: 1;
}

.resize-handle {
  position: absolute;
  background: transparent;
}

.resize-handle-se {
  right: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  cursor: se-resize;
}

/* Mobile responsive */
@media (max-width: 768px) {
  .window {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    border-radius: 0;
  }
  
  .window-titlebar {
    padding: 12px 16px;
  }
  
  .resize-handle {
    display: none;
  }
}
```

### A.2 JS Helper Functions

```javascript
// window-manager.js (loaded in index.html)

class WindowManager {
  constructor() {
    this.activeDrag = null;
    this.activeResize = null;
    this.setupEventListeners();
  }
  
  setupEventListeners() {
    document.addEventListener('pointerdown', this.onPointerDown.bind(this));
    document.addEventListener('pointermove', this.onPointerMove.bind(this));
    document.addEventListener('pointerup', this.onPointerUp.bind(this));
    document.addEventListener('pointercancel', this.onPointerCancel.bind(this));
  }
  
  onPointerDown(e) {
    const target = e.target.closest('.window');
    if (!target) return;
    
    const windowId = target.dataset.windowId;
    const resizeHandle = e.target.closest('.resize-handle');
    
    if (resizeHandle) {
      this.startResize(e, windowId, resizeHandle.dataset.handleType);
    } else if (e.target.closest('.window-titlebar')) {
      this.startDrag(e, windowId);
    } else {
      this.focusWindow(windowId);
    }
  }
  
  startDrag(e, windowId) {
    const windowEl = document.querySelector(`[data-window-id="${windowId}"]`);
    const rect = windowEl.getBoundingClientRect();
    
    this.activeDrag = {
      windowId,
      startX: e.clientX,
      startY: e.clientY,
      initialX: rect.left,
      initialY: rect.top,
    };
    
    windowEl.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  
  onPointerMove(e) {
    if (this.activeDrag) {
      const dx = e.clientX - this.activeDrag.startX;
      const dy = e.clientY - this.activeDrag.startY;
      
      // Call Rust callback
      window.updateWindowPosition(
        this.activeDrag.windowId,
        this.activeDrag.initialX + dx,
        this.activeDrag.initialY + dy
      );
      
      e.preventDefault();
    }
  }
  
  onPointerUp(e) {
    if (this.activeDrag) {
      const windowEl = document.querySelector(`[data-window-id="${this.activeDrag.windowId}"]`);
      windowEl.releasePointerCapture(e.pointerId);
      
      this.activeDrag = null;
    }
  }
  
  onPointerCancel(e) {
    this.onPointerUp(e);
  }
  
  focusWindow(windowId) {
    window.focusWindow(windowId);
  }
}

// Initialize
window.windowManager = new WindowManager();
```

---

**Document Version:** 1.0  
**Last Updated:** 2025-02-05  
**Status:** Research Complete, Ready for Implementation
