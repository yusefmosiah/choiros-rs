# R2 - Window Management Execution Spec

**Date:** 2026-02-05
**Status:** In progress

## Scope

Define backend/frontend contracts and interaction behavior for window operations: open, close, focus, move, resize, minimize, maximize, restore.

## Inputs

- `docs/window-management-research.md`
- `sandbox/src/actors/desktop.rs`
- `sandbox/src/api/desktop.rs`
- `sandbox-ui/src/desktop_window.rs`
- `sandbox-ui/src/interop.rs`
- `shared-types/src/lib.rs`

## Non-Goals

- No speculative tiling/docking system in first pass.
- No visual restyle beyond control affordance clarity.

## Current-State Evidence

1. Backend event-sourced operations exist for open/close/move/resize/focus.
2. `WindowState` already includes `minimized` and `maximized` fields.
3. Frontend drag/resize behavior is not fully wired for production UX.

## API/Event Contract Matrix (Draft)

1. Open: creates window with bounds/z-index defaults.
2. Focus: promotes window to active + top layer.
3. Move/Resize: validates bounds, persists event stream updates.
4. Minimize/Maximize/Restore: state transition events required if missing.
5. Close: removes active window and resolves next focus target.

## Interaction Spec (Draft)

- Use pointer events (`pointerdown/move/up`) and capture lifecycle.
- Keep drag/resize local for frame-to-frame responsiveness.
- Persist debounced updates to backend during interaction and final commit on release.
- Enforce viewport bounds and minimum size constraints.

## Focus/Z-Index Policy (Draft)

1. Click/focus always promotes target window.
2. Active window tracked explicitly.
3. Modal/always-on-top reserved layers documented for future use.

## Accessibility Baseline

- Controls with `aria-label`: minimize, maximize/restore, close.
- Keyboard support baseline: focus traversal, close active window shortcut, escape behavior for modal contexts.

## Test Plan (Draft)

1. Backend integration tests for window transition events.
2. Frontend interaction tests for drag/resize constraints.
3. Persistence restore tests for minimized/maximized windows.

## Acceptance Checklist

- [ ] Contract matrix complete.
- [ ] Interaction sequence complete.
- [ ] Throttling/persistence strategy complete.
- [ ] Test cases mapped to files.
