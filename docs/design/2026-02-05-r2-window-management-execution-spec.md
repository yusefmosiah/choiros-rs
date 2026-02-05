# R2 - Window Management Execution Spec

**Date:** 2026-02-05  
**Status:** Finalized for implementation  
**Owners:** Sandbox backend (`sandbox`) + Dioxus frontend (`sandbox-ui`)

## 1. Scope

This spec defines the implementation contract for window lifecycle and interactions:

- open
- close
- focus
- move
- resize
- minimize
- maximize
- restore

It also defines event semantics, pointer interaction lifecycle, throttling, accessibility/keyboard behavior, and a test matrix.

## 2. Repository Evidence (Current State)

### 2.1 Backend contracts currently implemented

- REST routes exist for desktop state, window open/close/move/resize/focus, and app registration in `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`.
- Handlers exist for `OpenWindow`, `CloseWindow`, `MoveWindow`, `ResizeWindow`, `FocusWindow` in `/Users/wiz/choiros-rs/sandbox/src/api/desktop.rs`.
- Desktop actor message variants exist for open/close/move/resize/focus/get-state/register-app in `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs`.

### 2.2 Event model currently implemented

- Event types persisted by DesktopActor today: `desktop.window_opened`, `desktop.window_closed`, `desktop.window_moved`, `desktop.window_resized`, `desktop.window_focused`, `desktop.app_registered` in `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs`.
- `desktop.window_minimized` and `desktop.window_maximized` constants exist but are unused (`#[allow(dead_code)]`) in `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs`.
- Event store is append-only with monotonic `seq` (SQLite autoincrement) in `/Users/wiz/choiros-rs/sandbox/src/actors/event_store.rs` and shared event type in `/Users/wiz/choiros-rs/shared-types/src/lib.rs`.

### 2.3 Frontend behavior currently implemented

- Window rendering and callbacks exist in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs` and `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`.
- Drag/resize wiring is incomplete: `start_drag` and `start_resize` are stubs in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`.
- Minimize/maximize/restore controls are not present in window chrome (`close` only) in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`.

### 2.4 WebSocket behavior currently implemented

- Server supports `/ws`, subscribe, ping/pong, and desktop snapshot (`desktop_state`) in `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`.
- Server defines window delta message variants and `broadcast_event`, but there is no wiring from desktop mutations to broadcast in `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`.
- Client parses `window_opened/window_closed/window_moved/window_resized/window_focused` in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs`.

### 2.5 Existing tests

- Backend actor tests exist for open/close/move/focus/get-state/register-app in `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` (test module).
- Backend API integration tests exist for open/close/move/resize/focus and state persistence in `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs`.
- No minimize/maximize/restore tests exist.
- No dedicated desktop websocket streaming tests exist.
- No frontend pointer/a11y tests exist.

## 3. Non-Goals

- No tiling/snap/docking system in this phase.
- No cross-desktop shared window state.
- No visual redesign beyond adding required controls and focus affordances.

## 4. Canonical Data Contract

Canonical window shape remains `shared_types::WindowState` in `/Users/wiz/choiros-rs/shared-types/src/lib.rs`:

- `id: String`
- `app_id: String`
- `title: String`
- `x: i32`
- `y: i32`
- `width: i32`
- `height: i32`
- `z_index: u32`
- `minimized: bool`
- `maximized: bool`
- `props: serde_json::Value`

Normative invariants:

1. `minimized` and `maximized` MUST NOT both be `true`.
2. `width` and `height` MUST satisfy minimum constraints.
3. `z_index` is strictly increasing per focus/open operations (no decrement).
4. `active_window` MAY point only to an existing, non-minimized window; otherwise `None`.

## 5. Backend/Frontend Contract Matrix

| Operation | HTTP Contract | Actor Contract | Event Store | WS Broadcast | Frontend Behavior |
|---|---|---|---|---|---|
| Open | `POST /desktop/{desktop_id}/windows` with `{app_id,title,props?}` (existing) | `DesktopActorMsg::OpenWindow` (existing) | append `desktop.window_opened` full `WindowState` (existing) | `window_opened` with full `window` (required to wire) | optimistic append allowed; reconcile on WS/snapshot |
| Close | `DELETE /desktop/{desktop_id}/windows/{window_id}` (existing) | `CloseWindow` (existing) | append `desktop.window_closed` `{window_id}` (existing) | `window_closed` `{window_id}` (required to wire) | remove locally; if active, adopt backend next-active |
| Focus | `POST /desktop/{desktop_id}/windows/{window_id}/focus` (existing) | `FocusWindow` (existing) | append `desktop.window_focused` `{window_id}` (existing) | `window_focused` `{window_id,z_index}` (required to wire) | set active and z-index from payload |
| Move | `PATCH /desktop/{desktop_id}/windows/{window_id}/position` with `{x,y}` (existing) | `MoveWindow` (existing) | append `desktop.window_moved` `{window_id,x,y}` (existing) | `window_moved` `{window_id,x,y}` (required to wire) | local drag state per frame + throttled commit |
| Resize | `PATCH /desktop/{desktop_id}/windows/{window_id}/size` with `{width,height}` (existing) | `ResizeWindow` (existing) | append `desktop.window_resized` `{window_id,width,height}` (existing) | `window_resized` `{window_id,width,height}` (required to wire) | local resize per frame + throttled commit |
| Minimize | `POST /desktop/{desktop_id}/windows/{window_id}/minimize` (new) | `MinimizeWindow` (new) | append `desktop.window_minimized` `{window_id}` (new) | `window_minimized` `{window_id}` (new) | hide window from canvas; keep running-app indicator |
| Maximize | `POST /desktop/{desktop_id}/windows/{window_id}/maximize` (new) | `MaximizeWindow` (new) | append `desktop.window_maximized` `{window_id,prev_bounds}` (new) | `window_maximized` `{window_id,bounds}` (new) | expand to viewport work area; disable drag/resize while maximized |
| Restore | `POST /desktop/{desktop_id}/windows/{window_id}/restore` (new) | `RestoreWindow` (new) | append `desktop.window_restored` `{window_id,bounds,from}` (new) | `window_restored` `{window_id,bounds,from}` (new) | unminimize or unmaximize using restored bounds |

Implementation note: maximize/restore requires persisted previous bounds. Store previous normal bounds in `props.window_normal_bounds` if not introducing a typed field.

## 6. Event Semantics

### 6.1 Ordering and causality

1. Desktop state is projection-only from append-only events.
2. Events MUST be applied in ascending `seq` order.
3. For a given `desktop_id`, last-write-wins by highest `seq`.

### 6.2 Idempotency

1. Projection MUST be idempotent by `(event_id)` or by monotonic `last_seq` guard.
2. Duplicate delivery via websocket MUST NOT cause divergent state.
3. Replayed snapshots + deltas MUST converge to identical state.

### 6.3 Required payload schemas

- `desktop.window_opened`: full `WindowState`.
- `desktop.window_closed`: `{ window_id }`.
- `desktop.window_moved`: `{ window_id, x, y }`.
- `desktop.window_resized`: `{ window_id, width, height }`.
- `desktop.window_focused`: `{ window_id }`.
- `desktop.window_minimized`: `{ window_id }`.
- `desktop.window_maximized`: `{ window_id, prev_x, prev_y, prev_width, prev_height }`.
- `desktop.window_restored`: `{ window_id, x, y, width, height, from }` where `from in ["minimized","maximized"]`.

### 6.4 Active-window semantics

1. `open` and `focus` set `active_window = window_id`.
2. `minimize(active)` MUST choose next top-most non-minimized window or `None`.
3. `close(active)` MUST choose next top-most non-minimized window or `None`.
4. `maximize` MUST focus target window.
5. `restore` SHOULD focus target window.

## 7. Pointer Interaction Lifecycle

This section is normative for `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` implementation.

### 7.1 Interaction state machine

States:

- `Idle`
- `DragPending`
- `Dragging`
- `ResizePending(edge/corner)`
- `Resizing`
- `CommitPending`

Transitions:

1. `pointerdown` on titlebar enters `DragPending`.
2. Movement beyond threshold (4px) enters `Dragging` and calls `setPointerCapture(pointerId)`.
3. `pointerdown` on resize handle enters `ResizePending` then `Resizing` after threshold.
4. `pointerup`/`pointercancel` exits to `CommitPending` then `Idle` after final backend commit.

### 7.2 Per-frame behavior

1. During drag/resize, update local visual position/size every frame (`requestAnimationFrame`).
2. Do not block rendering on network calls.
3. Clamp live values to viewport/work-area constraints.

### 7.3 Commit behavior

1. Fire throttled backend updates while active interaction runs.
2. Always send one final authoritative commit on `pointerup` or `pointercancel`.
3. If final commit fails, keep local state and schedule retry; surface non-blocking error toast/log.

### 7.4 Mobile behavior

1. For mobile mode (`vw <= 1024`), drag and resize are disabled (matches current structure in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`).
2. Maximize should map to full-screen work area; restore returns to prior bounds when returning to desktop mode.

## 8. Throttling Strategy

### 8.1 Network throttling

- Drag move PATCH cadence: max once every 50ms.
- Resize PATCH cadence: max once every 50ms.
- Focus events: immediate, no throttle.
- Final commit: immediate on interaction end.

### 8.2 Frame throttling

- UI updates happen at animation frame rate (up to 60fps) using local transient state.
- Backend writes are decoupled from frame updates.

### 8.3 Coalescing

- While a request is in flight, queue only the latest pending move/resize payload (drop intermediate payloads).
- On response, if newer payload exists, send next immediately.

### 8.4 Backend guardrails

- Reject invalid bounds with `400` and error body.
- Enforce min size and optional viewport clamping server-side to prevent invalid persisted state.

## 9. Accessibility and Keyboard Rules

### 9.1 Roles and labels

For `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`:

1. Window container MUST expose `role="dialog"` and `aria-label="{title}"`.
2. Active window MUST expose `aria-modal="false"` and clear focus styling.
3. Control buttons MUST include `aria-label` values: `Minimize`, `Maximize`, `Restore`, `Close`.
4. Titlebar MUST be keyboard-focusable (`tabindex="0"`).

### 9.2 Keyboard interactions

1. `Enter` or `Space` on titlebar focuses window.
2. `Alt+F4` closes active window.
3. `Ctrl+M` minimizes active window.
4. `Ctrl+Shift+M` maximizes active window; same shortcut toggles restore when maximized.
5. `Esc` cancels active drag/resize interaction and reverts to last committed bounds.
6. Arrow-key move/resize mode:
- `Alt+Arrow`: move active window by 10px.
- `Alt+Shift+Arrow`: resize active window by 10px.

### 9.3 Focus order

1. Tab order inside each window: titlebar controls, then content.
2. Closing active window moves focus to next active window titlebar; if none, focus prompt input in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs`.
3. Minimized windows are removed from tab sequence but remain available through running-app indicators in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs`.

## 10. Implementation Requirements by File

### 10.1 Backend

- `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs`
- Add actor messages: `MinimizeWindow`, `MaximizeWindow`, `RestoreWindow`.
- Add handlers + projection support for minimized/maximized/restored events.
- Preserve normal bounds for maximize/restore transitions.

- `/Users/wiz/choiros-rs/sandbox/src/api/desktop.rs`
- Add REST handlers for minimize/maximize/restore.
- Validate bounds and operation preconditions.

- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- Register new routes.

- `/Users/wiz/choiros-rs/sandbox/src/api/websocket.rs`
- Wire desktop mutation paths to `broadcast_event`.
- Add message variants for minimize/maximize/restore.

### 10.2 Frontend

- `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs`
- Replace mouse-only stub logic with pointer event lifecycle and capture.
- Add minimize/maximize/restore controls and keyboard handling.
- Introduce local transient interaction state and throttled backend persistence.

- `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs`
- Handle new websocket messages and active-window/focus reconciliation.
- Ensure minimized windows are excluded from canvas render and included in running-app strip.

- `/Users/wiz/choiros-rs/sandbox-ui/src/api.rs`
- Add API calls for minimize/maximize/restore.

## 11. Detailed Test Matrix

### 11.1 Existing tests (keep)

| Area | File | Existing Coverage |
|---|---|---|
| Backend API | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | state fetch, app register, open/close/move/resize/focus, persistence smoke |
| Backend actor | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` (test module) | open defaults, unknown app error, close, move, focus z-order, state, register app |

### 11.2 New backend tests (required)

| ID | File | Scenario | Expected |
|---|---|---|---|
| BE-01 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | minimize active window | `minimized=true`, active reassigns |
| BE-02 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | maximize normal window | `maximized=true`, bounds become work-area |
| BE-03 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | restore from maximized | prior bounds restored |
| BE-04 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | restore from minimized | `minimized=false`, focus restored |
| BE-05 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | invalid transition (maximize minimized without restore policy) | error or deterministic auto-restore policy |
| BE-06 | `/Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs` | min size enforcement during resize | persisted width/height clamped or rejected |
| BE-07 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | minimize endpoint | HTTP 200, subsequent state minimized |
| BE-08 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | maximize endpoint | HTTP 200, subsequent state maximized |
| BE-09 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | restore endpoint | HTTP 200, prior bounds recovered |
| BE-10 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | bad window id on new endpoints | HTTP 400 with error |
| BE-11 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | move/resize with invalid bounds | HTTP 400 |
| BE-12 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs` | close active with multiple windows | deterministic next-active |
| BE-13 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_ws_test.rs` (new) | subscribe + mutation emits delta | client receives matching ws delta |
| BE-14 | `/Users/wiz/choiros-rs/sandbox/tests/desktop_ws_test.rs` (new) | sequence of deltas | order matches mutation order |

### 11.3 New frontend tests (required)

| ID | File | Scenario | Expected |
|---|---|---|---|
| FE-01 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (component/unit) | pointer drag lifecycle | local movement per frame + final commit fired |
| FE-02 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (component/unit) | pointer resize lifecycle | local resize + constraints enforced |
| FE-03 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (component/unit) | pointercancel during drag | state reverts to last committed |
| FE-04 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (component/unit) | throttle/coalescing | <= 1 request per 50ms and latest payload wins |
| FE-05 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs` (integration) | ws `window_moved`/`window_resized` | projected state updates correctly |
| FE-06 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs` (integration) | ws minimize/maximize/restore | canvas/running-app and active state consistent |
| FE-07 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (a11y) | ARIA labels and roles | required attributes present |
| FE-08 | `/Users/wiz/choiros-rs/sandbox-ui/src/desktop_window.rs` (keyboard) | shortcuts (`Alt+F4`, `Ctrl+M`, toggle maximize) | correct callbacks invoked |

### 11.4 Manual E2E checklist

Run against `just dev-sandbox` + `just dev-ui`:

1. Open 3 windows, drag/resize each, refresh, verify state persists.
2. Minimize middle window, verify active shifts and indicator remains.
3. Maximize then restore, verify exact previous bounds.
4. Use keyboard shortcuts only; verify no pointer required.
5. Open second browser tab; verify websocket deltas converge.

## 12. Acceptance Criteria

1. Backend supports minimize/maximize/restore end-to-end (API, actor, events, projection).
2. Frontend pointer interactions are smooth, captured, throttled, and finalized on release.
3. WebSocket mutation deltas are broadcast and consumed; snapshot+delta convergence holds.
4. Keyboard and ARIA requirements pass tests.
5. Test matrix items are implemented and passing in CI for backend; frontend automated coverage added for critical interaction/a11y paths.
