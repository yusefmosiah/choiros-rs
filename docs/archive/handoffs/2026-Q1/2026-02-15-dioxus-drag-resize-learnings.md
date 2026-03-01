# Dioxus Drag/Resize Learnings (2026-02-15)

## Narrative Summary (1-minute read)
The drag/resize regression was not a single bug. It was a chain:
1. Interaction could get stuck when `pointerup`/`pointercancel` did not arrive on window-local handlers.
2. A follow-up hardening pass accidentally made drag appear dead in some paths.
3. A subsequent fallback introduced `AlreadyBorrowedMut` panics from re-entrant signal borrows.

The final stable approach was:
1. Keep the existing Dioxus hook architecture (`use_signal`, `use_callback`) in `desktop_window.rs`.
2. Add a document-level pointer fallback path for `pointermove`/`pointerup`/`pointercancel`.
3. Keep `lostpointercapture` cleanup.
4. Remove re-entrant borrow patterns by ensuring mutable borrows are dropped before callbacks.
5. Stabilize active-window visuals by using border color highlight and avoiding unconditional re-focus clicks.

Result: drag works, resize works, panic smoke test is clean, and active border toggling/size jitter is no longer reproducible in automated checks.

## What Changed
1. Pointer event handling:
1. Added robust pointer id extraction helper (`event_pointer_id`) and event element lookup (`event_element`).
2. Added root `onlostpointercapture` cleanup handling.
3. Added document-level fallback listeners for pointer lifecycle events.

2. Signal/borrow safety:
1. Replaced `if let Some(..) = signal.write().take()` callback patterns with two-step extraction so mutable borrows are released before callback re-entry.
2. Simplified document fallback move path to update `live_bounds`; commit callbacks happen on pointer up.

3. Focus/visual state:
1. Active highlight now uses border color, not thick outline ring.
2. Root click focus callback is conditional (`!is_active`) to avoid unnecessary focus churn.

4. Verification:
1. `cargo test --lib` in `dioxus-desktop` passing.
2. Playwright checks passing for drag movement and resize deltas.
3. Repeated content-click checks show single active window and stable dimensions.
4. Panic smoke test no longer reports `AlreadyBorrowedMut`.

## What To Do Next
1. Add a checked-in Playwright regression suite for:
1. Drag start/move/end.
2. Resize start/move/end.
3. Repeated content clicks (active-border stability).
4. Panic detection in browser console.

2. Refactor duplicated interaction-finalization code into shared helpers in `desktop_window.rs` to reduce future regressions.

3. Add lightweight telemetry around interaction termination reason (`pointerup`, `pointercancel`, `lostpointercapture`, `contextmenu`) to aid debugging without introducing orchestration logic.

## Key Engineering Learnings
1. Pointer capture alone is not enough for reliability in this UI: document-level fallback is required in practice.
2. In Dioxus, re-entrant callbacks can invalidate assumptions about signal mut-borrows; never hold a mut borrow across external callback calls.
3. Visual focus indicators must not change box model unexpectedly during active window transitions.
4. Keep fixes surgical and observable: each hardening step should be validated with browser automation, not only unit tests.
