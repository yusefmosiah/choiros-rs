# R1 - Dioxus Architecture Decomposition (Implementation-Ready Spec)

**Date:** 2026-02-05  
**Status:** Finalized  
**Owner:** UI Architecture Lane

## Scope

Decompose `dioxus-desktop/src/desktop.rs` into mergeable, testable modules while preserving behavior, API contracts, and current UX.

## Non-Goals

- No redesign of interaction model or visual style.
- No backend API/schema changes.
- No changes to chat/terminal feature behavior beyond import-path movement.

## Repository Evidence (Current State)

1. `dioxus-desktop/src/desktop.rs` is a 1071-line mixed-responsibility file (`wc -l`) containing:
- Root orchestration + signals + effects: `dioxus-desktop/src/desktop.rs:22`
- Theme caching/apply/persist logic: `dioxus-desktop/src/desktop.rs:42`, `dioxus-desktop/src/desktop.rs:787`
- Core app registry + registration side effect: `dioxus-desktop/src/desktop.rs:244`, `dioxus-desktop/src/desktop.rs:280`
- Prompt bar + icon/grid + loading/error helper components: `dioxus-desktop/src/desktop.rs:385`, `dioxus-desktop/src/desktop.rs:488`, `dioxus-desktop/src/desktop.rs:605`
- WebSocket parse + event projection + connection setup: `dioxus-desktop/src/desktop.rs:816`, `dioxus-desktop/src/desktop.rs:920`
- Global token/style blob: `dioxus-desktop/src/desktop.rs:630`

2. Window rendering is already partially separated:
- `FloatingWindow` exists in `dioxus-desktop/src/desktop_window.rs:8` and is consumed by `Desktop` at `dioxus-desktop/src/desktop.rs:340`.
- Drag/resize handlers are stubs (`start_drag`, `start_resize`) in `dioxus-desktop/src/desktop_window.rs:117` and `dioxus-desktop/src/desktop_window.rs:121`.

3. Entry-point coupling is minimal and can be preserved:
- `Desktop` is launched from `dioxus-desktop/src/main.rs:18`.
- Re-exported via `dioxus-desktop/src/lib.rs:10`.

4. Desktop API surface consumed by UI is in one place:
- Window/desktop/theme calls in `dioxus-desktop/src/api.rs:186` through `dioxus-desktop/src/api.rs:541`.

5. Existing automated coverage is backend-heavy:
- Desktop endpoint integration tests in `sandbox/tests/desktop_api_test.rs:1`.
- No current `dioxus-desktop` tests discovered (`rg` for `#[cfg(test)]`, `mod tests`, `wasm_bindgen_test`).

## Target Architecture

### Component and File Map (Concrete)

Keep public entry `Desktop` in `dioxus-desktop/src/desktop.rs`, reduce file to composition + module wiring.

Planned module tree:

```text
dioxus-desktop/src/
  desktop.rs                          # thin public entry; re-export Desktop
  desktop/
    mod.rs                            # DesktopShell component + props wiring
    state.rs                          # DesktopStateModel + pure reducers/selectors
    effects.rs                        # bootstrapping effects (load desktop/apps/theme/ws)
    ws.rs                             # websocket transport + parsing -> typed events
    actions.rs                        # async intent handlers (open/close/focus/move/resize/prompt)
    theme.rs                          # apply/cache/fetch/persist theme bridge
    apps.rs                           # core app registry constants + helpers
    components/
      workspace_canvas.rs             # workspace + icon layer + window canvas shell
      desktop_icons.rs                # DesktopIcons + DesktopIcon
      prompt_bar.rs                   # PromptBar + RunningAppIndicator
      status_views.rs                 # LoadingState + ErrorState
```

Compatibility constraints:

- `dioxus-desktop/src/main.rs` remains unchanged (`Desktop` import path stable).
- `dioxus-desktop/src/lib.rs` keeps `pub mod desktop;` and `pub use desktop::*;`.
- `dioxus-desktop/src/desktop_window.rs` remains source of `FloatingWindow` until later window-lane work.

### Runtime Composition

1. `Desktop` (`dioxus-desktop/src/desktop.rs`) delegates to `desktop::DesktopShell`.
2. `DesktopShell` owns top-level signals and invokes `effects::*` hooks.
3. `WorkspaceCanvas` renders icon layer + floating windows using derived view model.
4. `PromptBar` receives a minimal view model (connection, running windows, active id, theme).
5. `ws.rs` parses inbound websocket payloads into typed desktop events.
6. `state.rs` applies those events with pure reducer-style functions.

## State Ownership Table

| State / Data | Owner Module | Write Path | Read Path | Persistence / Source of Truth |
|---|---|---|---|---|
| `desktop_state: Option<DesktopState>` | `desktop::state` (stored in `DesktopShell`) | `effects` initial fetch, `actions`, `ws` projection reducers | `WorkspaceCanvas`, `PromptBar` | Backend desktop API + websocket events (`dioxus-desktop/src/api.rs:186`, `dioxus-desktop/src/desktop.rs:920`) |
| `loading: bool` | `desktop::effects` + shell | set during initial fetch | `status_views` | UI-local transient |
| `error: Option<String>` | `desktop::effects` + shell | fetch/action failures | `status_views` | UI-local transient |
| `ws_connected: bool` | `desktop::ws` projection | ws connect/disconnect events | `PromptBar` status | websocket lifecycle (`dioxus-desktop/src/desktop.rs:822`) |
| `viewport: (u32,u32)` | `desktop::effects` | viewport bootstrap/updates | `WorkspaceCanvas`, `FloatingWindow` | browser runtime; currently stubbed getter (`dioxus-desktop/src/desktop.rs:869`) |
| `current_theme: String` (`light|dark`) | `desktop::theme` | toggle action + bootstrap fetch/cache | `PromptBar` theme toggle rendering | localStorage + backend user pref (`dioxus-desktop/src/desktop.rs:49`, `dioxus-desktop/src/api.rs:271`) |
| `apps_registered: bool` | `desktop::effects` | one-time side effect guard | internal only | UI-local guard for `register_app` loop (`dioxus-desktop/src/desktop.rs:285`) |
| Core app definitions (`chat`,`writer`,`terminal`,`files`) | `desktop::apps` | static/const | `DesktopIcons`, app registration effect | code-defined defaults (`dioxus-desktop/src/desktop.rs:245`) |
| Prompt input text | `components/prompt_bar.rs` local signal | input handlers | prompt bar only | UI-local ephemeral |
| Desktop icon pressed/double-click debounce | `components/desktop_icons.rs` local signal | icon click handlers | icon component only | UI-local ephemeral (`dioxus-desktop/src/desktop.rs:418`) |

Ownership rules:

- Only `state.rs` mutates `DesktopState` structures.
- Components remain presentational; they do not call API directly.
- All async API traffic goes through `actions.rs`/`effects.rs`.

## Phased Refactor Plan (Merge Slices)

Each slice is intended to merge independently with no behavior change.

### Slice 1: Create module skeleton + move pure helpers

Changes:

- Add `desktop/` module tree and `mod.rs` wiring.
- Move pure/non-async helpers from `desktop.rs`:
  - `get_app_icon` -> `desktop/apps.rs`
  - `LoadingState`, `ErrorState` -> `desktop/components/status_views.rs`
  - `PromptBar`, `RunningAppIndicator` -> `desktop/components/prompt_bar.rs`
  - `DesktopIcons`, `DesktopIcon` -> `desktop/components/desktop_icons.rs`
- Keep `Desktop` orchestration in place, importing moved components.

Acceptance criteria:

- `Desktop` renders identically with unchanged behavior paths.
- `dioxus-desktop/src/main.rs` and `dioxus-desktop/src/lib.rs` do not require public API changes.
- `cargo check -p dioxus-desktop` passes.

Explicit test impacts:

- No existing test file should require updates.
- Add first `wasm-bindgen-test` module for moved pure helpers (icon mapping, prompt indicator active-state class).

### Slice 2: Extract theme bridge

Changes:

- Move theme functions to `desktop/theme.rs`:
  - `apply_theme_to_document`
  - `get_cached_theme_preference`
  - `set_cached_theme_preference`
- Add a single shell-level helper/hook in `effects.rs` for initialization and toggle persistence flow.

Acceptance criteria:

- Theme initialization order preserved: cache first, then backend preference.
- Toggle persists to both local cache and backend exactly as today.
- Invalid theme values remain ignored.

Explicit test impacts:

- New `wasm-bindgen-test` coverage for cache filtering (`light|dark` only).
- New unit test (pure) for theme toggle state transition logic if extracted as pure fn.
- No backend integration test changes required.

### Slice 3: Extract websocket parsing/projection + reducers

Changes:

- Move `WsEvent`, `connect_websocket`, and message parsing from `desktop.rs:874-1071` into `desktop/ws.rs`.
- Move `handle_ws_event` mutation logic into `desktop/state.rs` as reducer functions, e.g. `apply_ws_event`.
- `DesktopShell` only subscribes and dispatches typed events.

Acceptance criteria:

- All existing websocket message types still handled (`desktop_state`, `window_opened`, `window_closed`, `window_moved`, `window_resized`, `window_focused`, `pong`, `error`).
- Connection indicator transitions unchanged.
- No duplicate windows introduced by reducer extraction.

Explicit test impacts:

- Add pure reducer tests for each ws event variant.
- Add parser tests for representative JSON payloads and unknown-type no-op behavior.
- Existing `sandbox/tests/desktop_api_test.rs` remains unchanged but becomes required regression signal for endpoint compatibility.

### Slice 4: Extract async actions/effects and thin root

Changes:

- Move open/close/focus/move/resize/prompt-submit callbacks into `desktop/actions.rs`.
- Move initial fetch, register-app bootstrap, viewport bootstrap, ws bootstrap into `desktop/effects.rs`.
- Collapse `dioxus-desktop/src/desktop.rs` to thin public entry forwarding to `desktop::DesktopShell`.

Acceptance criteria:

- `dioxus-desktop/src/desktop.rs` reduced to composition/wiring only.
- Side-effect ordering preserved:
  - initial state fetch
  - ws connect
  - app registration (best effort)
  - theme bootstrap
- Prompt submit still focuses existing chat window or opens new one before send.

Explicit test impacts:

- Add action tests around desktop-state mutation for open/close/focus local optimistic updates.
- Add prompt-submit decision-path tests (existing chat vs new chat).
- Run backend desktop API integration tests to confirm no request-shape regressions:
  - `cargo test -p sandbox --test desktop_api_test`

### Slice 5: Stabilization and cleanup

Changes:

- Remove dead code paths from old `desktop.rs`.
- Normalize imports and visibility in `desktop/mod.rs`.
- Keep behavior lock by avoiding style/markup refactors in this lane.

Acceptance criteria:

- `cargo fmt --check` clean.
- `cargo clippy --workspace -- -D warnings` clean (or documented pre-existing warnings outside touched scope).
- `just test-unit` passes for workspace.

Explicit test impacts:

- Ensure new `dioxus-desktop` tests are part of CI path (if wasm tests require explicit job, note in follow-up lane).
- No expected changes to `sandbox/tests/desktop_api_test.rs` assertions.

## Definition of Done

This architecture decomposition lane is complete when:

- The module/file map above exists and is wired.
- `Desktop` remains the stable entry component and external import path.
- State ownership rules are enforced (reducers own `DesktopState` mutation; components are presentational).
- Each slice merges independently with acceptance checks passing.
- Test coverage is expanded for extracted pure logic and ws/reducer behavior.

## Risks and Controls

1. **Risk:** Behavior drift during callback extraction.  
   **Control:** Keep payload contracts unchanged; add action-path tests per slice.

2. **Risk:** ws parser regressions due to serde/value handling split.  
   **Control:** Snapshot representative payload fixtures in parser tests.

3. **Risk:** Theme boot order regression (flash or wrong persisted value).  
   **Control:** Preserve cache-first semantics and test transition rules.

4. **Risk:** Merge conflicts with concurrent UI lanes.  
   **Control:** Slice boundaries isolate files (`desktop/` tree) and keep `desktop.rs` API stable until final slice.

## Implementation Notes for Next PR Author

- Prioritize moving code first, then renaming symbols; avoid mixed semantic rewrites.
- For each slice, land tests in same PR before further decomposition.
- Use the existing API wrappers in `dioxus-desktop/src/api.rs`; do not add direct HTTP logic in components.
