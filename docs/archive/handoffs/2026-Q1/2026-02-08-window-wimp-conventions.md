# Window WIMP Conventions

Date: 2026-02-08
Status: Active implementation contract
Owners: `sandbox` backend + `dioxus-desktop` frontend

## Scope

This document defines the minimum Window/Icon/Menu/Pointer (WIMP) behavior we require for
ChoirOS floating windows.

## Core Invariants

1. Focus and z-order are coupled.
2. The focused window is always the top-most (largest `z_index`) non-minimized window.
3. At most one window is focused at a time (`active_window`).
4. Minimized windows are never focused.
5. A window cannot be both minimized and maximized.

## Focus and Raise Semantics

1. Pointer down on a window titlebar must focus and raise immediately.
2. Pointer down on a resize handle must focus and raise immediately.
3. Keyboard focus actions (`Enter` on titlebar, taskbar/app-switch click) must focus and raise.
4. A dragged window must remain top-most during drag; it must not "pop on top" only after drag
   ends.

## Maximize Semantics

1. Maximize means occupy the entire workspace work-area, not a fixed pixel size.
2. Work-area is the live `.window-canvas` rectangle (which already excludes prompt bar space).
3. Maximize geometry is client-provided (`x`, `y`, `width`, `height`) and persisted by backend.
4. Maximize must hide titlebar chrome and show floating window controls (minimize, restore,
   close) in the top-right.
5. Restore returns to persisted normal bounds from before maximize.

## Desktop vs Mobile Rules

Desktop (`viewport width > 1024`):
1. Windows may overhang left/right edges.
2. A minimum visible strip must remain on-screen so a window is always recoverable.
3. Vertical position remains bounded to keep title/content reachable.

Mobile (`viewport width <= 1024`):
1. Drag and resize are disabled.
2. Windows stay fully inside the work-area.
3. Maximize uses the same work-area contract and remains responsive to orientation/viewport
   changes.

## API Contract Requirements

`POST /desktop/{desktop_id}/windows/{window_id}/maximize` accepts optional JSON body:

```json
{
  "x": 0,
  "y": 0,
  "width": 1280,
  "height": 720
}
```

Rules:
1. If body is provided, backend uses those bounds (with min-size guards).
2. If body is absent, backend uses deterministic fallback bounds.
3. WebSocket `window_maximized` broadcasts the applied bounds.

## Test Requirements

1. Maximize uses provided work-area bounds.
2. Restore after maximize returns exact previous normal bounds.
3. Drag start immediately raises focused window.
4. Desktop clamp allows horizontal overhang with minimum visible strip.
5. Mobile clamp keeps windows in bounds.
