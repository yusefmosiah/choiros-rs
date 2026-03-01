# R3 - Content Viewer MVP Spec

**Date:** 2026-02-05
**Status:** Finalized (implementation-ready MVP)
**Owner:** UI Lane R3

## Scope

Define a production-implementable viewer framework MVP for ChoirOS desktop windows that:
1. Adds a shared viewer shell contract.
2. Ships two viewer types in MVP: text and image.
3. Uses backend-canonical persistence and reconciliation (no client source-of-truth).
4. Specifies JS interop, lazy loading, lifecycle teardown, and test coverage.

## Non-Goals

1. No offline-first canonical persistence in browser storage.
2. No media suite beyond text + image in MVP.
3. No speculative file indexing/search system in this lane.
4. No backend actor redesign outside viewer-specific contracts.

## Repository Evidence (Current State)

1. `dioxus-desktop/src/desktop_window.rs`
- Window content only renders `chat` and `terminal`; all other apps show `"App not yet implemented"`.
- Confirms viewer apps are currently absent and require explicit routing.

2. `dioxus-desktop/src/desktop.rs`
- Desktop registers core apps including `writer` and `files`, but no matching content implementation exists.
- Confirms shell already supports app launch/open-window flow we can reuse.

3. `shared-types/src/lib.rs`
- `WindowState.props: serde_json::Value` is already available for app-specific viewer context.
- Enables typed viewer descriptors without changing `WindowState` immediately.

4. `sandbox/src/actors/desktop.rs`
- `DesktopActor` persists `window_opened`, `window_moved`, `window_resized`, etc. to `EventStore`.
- `OpenWindow` stores full `WindowState` (including `props`) in event payload.

5. `sandbox/src/api/desktop.rs` and `sandbox/src/api/mod.rs`
- Existing desktop HTTP contract already supports open/focus/move/resize/close.
- `POST /desktop/{desktop_id}/windows` accepts `props`.

6. `sandbox/src/actors/event_store.rs`
- Append-only, ordered event log is implemented and used for persistence.

7. `docs/design/2026-02-05-ui-storage-reconciliation.md`
- Policy is accepted: backend/EventStore canonical, browser storage non-authoritative, backend wins on conflict.

8. `docs/ARCHITECTURE_SPECIFICATION.md`
- Architectural principle explicitly states actor-owned state and UI as projection.

9. `dioxus-desktop/src/terminal.rs` and `dioxus-desktop/public/terminal.js`
- Proven pattern for Rust<->JS bridge, script loading, runtime handles, and disposal API naming.
- Reused as the interop pattern baseline for viewer plugins.

## MVP Deliverables

1. `ViewerShell` UI contract shared by all viewer types.
2. `TextViewer` (read + edit + save).
3. `ImageViewer` (read-only with zoom/pan/reset).
4. Backend viewer APIs for canonical read/write.
5. Typed viewer metadata and payload contracts.
6. Lifecycle + lazy-load interop primitives.
7. Contract-level unit/integration/e2e test matrix.

## Viewer Shell API (Frontend Contract)

## Component Interface

```rust
pub enum ViewerKind {
    Text,
    Image,
}

pub enum ViewerShellState {
    Loading,
    Ready,
    Dirty,
    Saving,
    Error,
}

pub struct ViewerShellProps {
    pub window_id: String,
    pub desktop_id: String,
    pub descriptor: ViewerDescriptor,
    pub on_close: Callback<()>,
}
```

## Render Regions

1. Header: title, source path/uri, type badge, actions (`Reload`, `Save`, `Open Externally` optional).
2. Body: viewer mount node (text editor or image canvas).
3. Footer: status (`Loading`, `Saved`, `Unsaved changes`, `Saving`, `Error`) + revision metadata.

## Required Behavior

1. Shell owns request/lifecycle orchestration only.
2. Viewer plugin owns content rendering/edit mechanics.
3. Save action is disabled unless viewer supports write and state is `Dirty`.
4. Error state preserves prior last-known-good content in-memory (session only).

## MVP Types and Contracts

## Window Props Schema (MVP)

`WindowState.props` MUST carry a typed descriptor for viewer windows.

```json
{
  "viewer": {
    "kind": "text",
    "resource": {
      "uri": "file:///workspace/README.md",
      "mime": "text/markdown"
    },
    "capabilities": {
      "readonly": false
    }
  }
}
```

```json
{
  "viewer": {
    "kind": "image",
    "resource": {
      "uri": "file:///workspace/screenshots/new-desktop.png",
      "mime": "image/png"
    },
    "capabilities": {
      "readonly": true
    }
  }
}
```

## Rust DTOs (Shared or UI-local initially)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerDescriptor {
    pub kind: ViewerKind,
    pub resource: ViewerResource,
    pub capabilities: ViewerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerResource {
    pub uri: String,
    pub mime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerCapabilities {
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerRevision {
    pub rev: i64,
    pub updated_at: String,
}
```

## Backend API Contract (MVP Additions)

```http
GET  /viewer/content?uri={uri}
PATCH /viewer/content
```

`GET /viewer/content` response:

```json
{
  "success": true,
  "uri": "file:///workspace/README.md",
  "mime": "text/markdown",
  "content": "...",
  "revision": { "rev": 42, "updated_at": "2026-02-05T18:30:00Z" },
  "readonly": false
}
```

`PATCH /viewer/content` request:

```json
{
  "uri": "file:///workspace/README.md",
  "base_rev": 42,
  "content": "updated text"
}
```

`PATCH /viewer/content` success:

```json
{
  "success": true,
  "revision": { "rev": 43, "updated_at": "2026-02-05T18:31:15Z" }
}
```

`PATCH /viewer/content` conflict:

```json
{
  "success": false,
  "error": "revision_conflict",
  "latest": {
    "content": "backend newest",
    "revision": { "rev": 44, "updated_at": "2026-02-05T18:31:20Z" }
  }
}
```

## Canonical Data Flow Contract

## Storage Rules

1. Canonical persisted content/metadata is backend-owned and EventStore-backed.
2. Browser cache (if added later) is optimization only and must be replaceable.
3. On conflict/divergence, backend revision is authoritative.

## Open -> Load -> Edit -> Save -> Reconcile

1. `open_window` sends `props.viewer` descriptor via existing desktop API (`POST /desktop/{desktop_id}/windows`).
2. Viewer shell reads descriptor from window `props` and requests current canonical content from backend.
3. Viewer plugin emits local change events (`Dirty`), but no canonical mutation occurs until `PATCH /viewer/content` succeeds.
4. Save includes `base_rev` for optimistic concurrency.
5. Backend either:
- accepts and returns new revision, or
- returns `revision_conflict` with latest canonical value.
6. On conflict, shell transitions to `Error` with merge/reload options; reloading always uses backend latest.

## Event Requirements (Backend)

Viewer save operations must append event(s) (example names):
1. `viewer.content_saved`
2. `viewer.content_conflict` (optional observability event)

Event payload minimum:
- `uri`
- `mime`
- `base_rev`
- `new_rev` (save only)
- `content_hash` (not raw content when avoidable)
- `window_id`
- `user_id`

This preserves auditability and keeps behavior aligned with `EventStoreActor` append-only policy.

## JS Interop Decisions (MVP)

## Decision 1: Text Viewer Uses CodeMirror 6 via JS Bridge

Rationale:
1. Rich editing UX is difficult to replicate in pure Dioxus quickly.
2. Existing terminal integration already validates a Rust<->`window.*` bridge pattern.

Bridge API (new `dioxus-desktop/public/viewer-text.js`):

```js
window.createTextViewer(container, options) -> handle
window.setTextViewerContent(handle, text)
window.getTextViewerContent(handle) -> string
window.onTextViewerChange(handle, cb)
window.disposeTextViewer(handle)
```

Rust externs in `dioxus-desktop/src/interop.rs` follow existing terminal naming and lifecycle style.

## Decision 2: Image Viewer MVP Stays Mostly Native

1. Use Dioxus + pointer events + CSS transform for pan/zoom.
2. No external JS dependency required in MVP for image viewing.
3. Add JS interop later only if performance/gesture complexity proves insufficient.

## Decision 3: One Interop Loader Pattern

1. Script loading uses `ensure_script(id, src)` style already used in `dioxus-desktop/src/terminal.rs`.
2. Bridge availability check mirrors `wait_for_terminal_bridge()` pattern.
3. Every created handle must have explicit `dispose*` invocation on unmount/close.

## Lazy Loading and Lifecycle

## Lazy Loading

1. Do not preload viewer JS globally.
2. Load `viewer-text.js` only when first `ViewerKind::Text` window mounts.
3. Reuse loaded script and bridge for subsequent windows.

## Per-Window Lifecycle State Machine

1. `Mounting` -> parse descriptor + init shell.
2. `Loading` -> fetch canonical content.
3. `Ready` -> render plugin.
4. `Dirty` -> unsaved local edits exist.
5. `Saving` -> in-flight save.
6. `Ready` (post-save) or `Error` (failure/conflict).
7. `Unmounting` -> dispose plugin handle, remove listeners, cancel pending retries.

## Lifecycle Guarantees

1. Closing a window must call plugin dispose API exactly once.
2. Window reopen must create a fresh plugin handle.
3. Failed script load leaves shell in recoverable error state with `Retry`.
4. Failed content load preserves shell chrome and action controls.

## Implementation Slices

1. Slice A: Viewer shell and `WindowState.props.viewer` parsing in UI.
- Touchpoints: `dioxus-desktop/src/desktop_window.rs`, new viewer modules.

2. Slice B: Text viewer bridge and lifecycle wiring.
- Touchpoints: `dioxus-desktop/src/interop.rs`, `dioxus-desktop/public/viewer-text.js`, new `TextViewer` component.

3. Slice C: Backend viewer content endpoints + EventStore append path.
- Touchpoints: `sandbox/src/api/mod.rs`, new `sandbox/src/api/viewer.rs`, backend service module.

4. Slice D: Image viewer and shared status/footer.
- Touchpoints: viewer components only.

5. Slice E: Tests and conflict/retry hardening.

## Test Matrix (Implementation-Ready)

| Layer | Scenario | Expected Result | Target File(s) |
|---|---|---|---|
| UI unit | Shell renders loading/ready/error/dirty states | Correct region visibility and action enablement | `dioxus-desktop/src/viewers/shell.rs` (new tests) |
| UI unit | Viewer descriptor parse from `WindowState.props` | Invalid/missing descriptor yields shell error | `dioxus-desktop/src/viewers/types.rs` (new tests) |
| UI unit | Text viewer lifecycle dispose on unmount | `disposeTextViewer` invoked once per handle | `dioxus-desktop/src/viewers/text.rs` (new tests) |
| API integration | Open viewer window with `props.viewer` | Returned `window.props.viewer` preserved | `sandbox/tests/desktop_api_test.rs` (extend) |
| API integration | `GET /viewer/content` happy path | Canonical payload + revision returned | `sandbox/tests/viewer_api_test.rs` (new) |
| API integration | `PATCH /viewer/content` with valid `base_rev` | Save success + revision increments | `sandbox/tests/viewer_api_test.rs` (new) |
| API integration | `PATCH /viewer/content` conflict | `revision_conflict` response with latest canonical content | `sandbox/tests/viewer_api_test.rs` (new) |
| Actor/EventStore integration | Save appends viewer event | Event exists with required payload fields | `sandbox/tests/persistence_test.rs` (extend) |
| E2E | Open text viewer, edit, save, reopen | Reopen shows backend-saved canonical content | `tests/e2e/test_e2e_viewer_text_flow.ts` (new) |
| E2E | Network failure during save | Shell enters recoverable error, retry works | `tests/e2e/test_e2e_viewer_error_handling.ts` (new) |
| E2E | Parallel edit conflict (two windows) | Second stale save surfaces conflict and reload path | `tests/e2e/test_e2e_viewer_conflict.ts` (new) |

## Acceptance Checklist

- [x] Viewer shell API finalized.
- [x] MVP viewer types finalized (`text`, `image`).
- [x] Backend-canonical data-flow contract finalized.
- [x] JS interop decisions finalized.
- [x] Lazy-load + lifecycle teardown contract finalized.
- [x] Test matrix mapped to concrete file targets.

## Policy Conformance Statement

This MVP explicitly conforms to backend-canonical storage policy:
1. Canonical persisted viewer state is backend/EventStore-backed.
2. Browser-side state is transient projection/cache only.
3. Backend revision always wins on divergence.

