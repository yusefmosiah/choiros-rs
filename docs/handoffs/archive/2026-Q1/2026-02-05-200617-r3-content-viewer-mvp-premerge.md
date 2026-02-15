# Handoff: R3 Content Viewer MVP Pre-Merge

## Session Metadata
- Created: 2026-02-05 20:06:17
- Project: /Users/wiz/.codex/worktrees/dc95/choiros-rs
- Branch: [not a git repo or detached HEAD]
- Session duration: ~2 hours

### Recent Commits (for context)
  - 1e884d4 Update docs and plan worktrees
  - 073b4d0 Document r1 dioxus architecture
  - 7aa3125 ui-master-execution-plan.md
  - cccd4f3 Restore live tool activity stream
  - 4c84e7a Fix theming toggle workflow

## Handoff Chain

- **Continues from**: [2026-02-05-151635-theme-user-global-toggle-next-steps.md](./2026-02-05-151635-theme-user-global-toggle-next-steps.md)
  - Previous title: User-Global Theme Persistence + UI Toggle
- **Supersedes**: None

> This handoff is focused on R3 implementation scope only (viewer MVP).

## Current State Summary

Implemented R3 MVP per `docs/design/2026-02-05-r3-content-viewer-mvp-spec.md` across backend + UI: shared viewer shell, text viewer, image viewer baseline, backend-canonical GET/PATCH content contract with optimistic revisions/conflict handling, and JS interop lazy-load + lifecycle cleanup for text viewer. Core API/UI/unit/integration tests were added and targeted validation passed. E2E viewer tests from the matrix were not added yet and remain follow-up for merge lane if required by PR gate.

## Codebase Understanding

### Architecture Overview

- Viewer routing is now driven by `window.props.viewer` descriptor parsing in UI content region (not solely `app_id`), which aligns to spec’s typed window-content contract.
- Canonical viewer content source is backend EventStore via `viewer.content_saved` events; client local edit state is transient only.
- Save conflict behavior is explicit optimistic concurrency with `base_rev`; backend returns `revision_conflict` and latest canonical content/revision.
- Text interop follows existing terminal bridge pattern: lazy script load, readiness wait loop, runtime handle, and `Drop` cleanup.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| docs/design/2026-02-05-r3-content-viewer-mvp-spec.md | R3 implementation contract | Source of truth for scope and API/lifecycle requirements |
| shared-types/src/lib.rs | Shared DTOs and event constants | Added viewer types and event names used by backend/UI |
| sandbox/src/api/viewer.rs | New viewer backend API implementation | Canonical load/save, conflict handling, event append |
| sandbox/src/api/mod.rs | API router | Registers `/viewer/content` GET/PATCH routes |
| sandbox-ui/src/viewers/shell.rs | Shared viewer shell | Shell lifecycle/state machine and save/reload orchestration |
| sandbox-ui/src/viewers/text.rs | Text viewer interop runtime | Lazy-load bridge and dispose-on-unmount lifecycle |
| sandbox-ui/public/viewer-text.js | JS bridge implementation | create/set/change/dispose APIs consumed by WASM |
| sandbox-ui/src/viewers/image.rs | Image viewer baseline | Read-only zoom/pan/reset MVP viewer |
| sandbox-ui/src/desktop_window.rs | Window content routing | Routes to viewer shell based on `props.viewer` |
| sandbox-ui/src/api.rs | Frontend viewer API client | GET/PATCH DTOs and conflict error surface |
| sandbox/tests/viewer_api_test.rs | New backend API tests | GET/PATCH/conflict/event payload matrix coverage |

### Key Patterns Discovered

- API handlers follow axum `impl IntoResponse` with explicit status + JSON shape.
- EventStore writes use `ractor::call!(..., EventStoreMsg::Append { ... })` and actor_id scoping for queryability.
- UI interop uses `wasm_bindgen` externs + script loader helper + polling bridge existence.
- Dioxus UI state uses `use_signal` and `spawn(async move { ... })` for async transitions.

## Work Completed

### Tasks Finished

- [x] Added shared viewer DTOs and viewer event constants.
- [x] Implemented backend `/viewer/content` GET/PATCH with optimistic revision contract.
- [x] Implemented viewer save/conflict EventStore append path.
- [x] Implemented viewer shell component and routing from `WindowState.props.viewer`.
- [x] Implemented text viewer JS interop bridge with lazy-load and cleanup lifecycle.
- [x] Implemented image viewer baseline (read-only zoom/pan/reset).
- [x] Added API/integration/UI-unit tests for R3 MVP slices.
- [x] Ran targeted tests and checks.

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| sandbox-ui/src/lib.rs | Exported `viewers` module | Make new viewer components/types available |
| sandbox/src/api/mod.rs | Added `viewer` module + `/viewer/content` GET/PATCH route | Expose new backend viewer contract |
| shared-types/src/lib.rs | Added `ViewerKind/Descriptor/Resource/Capabilities/Revision` and viewer event constants | Shared contract across backend/UI |
| sandbox-ui/src/desktop_window.rs | Added content routing to `ViewerShell` based on parsed `props.viewer` | Required by spec for window-content-driven viewer rendering |
| sandbox-ui/src/api.rs | Added viewer GET/PATCH DTOs and conflict error type | Frontend shell needs typed backend API client |
| sandbox/Cargo.toml | Added `base64` dependency | Encode initial image content into data URI payload |
| sandbox/tests/desktop_api_test.rs | Added test preserving `props.viewer` through open window API | Validate desktop contract preservation |
| sandbox-ui/src/desktop.rs | Added viewer props when opening writer/files windows | Ensure launch flow exercises new viewer route |
| sandbox/src/api/viewer.rs | New file implementing canonical viewer load/save/conflict logic | Core backend MVP implementation |
| sandbox/tests/viewer_api_test.rs | New viewer API integration test file | R3 backend test matrix coverage |
| sandbox-ui/public/viewer-text.js | New JS bridge file for text viewer | Text interop API and lifecycle surface |
| sandbox-ui/src/viewers/mod.rs | New module barrel | Organize viewer components |
| sandbox-ui/src/viewers/types.rs | Viewer descriptor parsing + tests | Validate window props contract |
| sandbox-ui/src/viewers/shell.rs | Shared shell state/actions + tests | Shell orchestration and status footer |
| sandbox-ui/src/viewers/text.rs | Text interop runtime | Lazy-load bridge, change events, dispose on drop |
| sandbox-ui/src/viewers/image.rs | Image baseline component | MVP image viewer type |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Persist canonical viewer content in EventStore payload for MVP | 1) New DB table now 2) EventStore-only MVP | Tight scope and alignment with existing event-sourced architecture |
| Route viewers by `props.viewer` presence | 1) By `app_id` only 2) By typed descriptor | Spec explicitly requires routing from window content descriptor |
| Text viewer bridge implemented with minimal JS textarea | 1) Full CodeMirror now 2) Bridge-compatible MVP then CM upgrade | Delivers lifecycle contract now while keeping MVP small |
| Image viewer mostly native Dioxus | 1) JS image lib 2) Native MVP | Matches spec decision and avoids extra dependency scope |

## Pending Work

## Immediate Next Steps

1. Merge these changes and resolve any conflicts around `sandbox-ui/src/desktop.rs` and `sandbox-ui/src/desktop_window.rs` if other lanes touched desktop/window rendering.
2. Decide whether to keep current text bridge MVP or swap bridge internals to CodeMirror 6 before merge-to-main.
3. Add/execute E2E viewer tests from spec matrix if required by CI/release gate.

### Blockers/Open Questions

- [ ] Should R3 mandate CodeMirror 6 before merge, or is current bridge-compatible text editor acceptable for MVP?
- [ ] Should backend persist binary image updates via PATCH in MVP, or keep image viewer strictly read-only as currently implemented?

### Deferred Items

- CodeMirror 6 integration deferred: current bridge uses textarea but preserves same JS API/lifecycle shape.
- E2E viewer tests deferred: added API + UI-unit coverage; no new `tests/e2e/test_e2e_viewer_*` in this change.
- No broad media suite beyond text/image baseline by scope constraint.

## Context for Resuming Agent

## Important Context

- Do not reintroduce client-side canonical state. `ViewerShell` local state is transient only; backend revision is authoritative.
- Conflict handling is implemented in both backend and frontend path; stale save returns HTTP 409 with `error: revision_conflict` and `latest`.
- Event payload fields required by spec are present on save/conflict paths in `sandbox/src/api/viewer.rs`.
- Routing currently checks `props.viewer`; windows without this descriptor still use legacy `app_id` match.
- There are unrelated existing warnings in `sandbox` not introduced by this work; avoid mixing warning cleanup into merge.

### Assumptions Made

- Viewer canonical persistence can be EventStore-backed without introducing a new dedicated content table for MVP.
- File-backed initial content is acceptable for `file://` URIs when no prior save event exists.
- `writer`/`files` launcher props in desktop are sufficient MVP entry points for new viewer shell.

### Potential Gotchas

- `sandbox-ui/src/viewers/shell.rs` uses cloned URI signals to satisfy Dioxus closure ownership; avoid naive refactors that reintroduce move errors.
- Text bridge options are currently passed as JSON string `JsValue`; if replaced, keep JS API stable (`create/set/onChange/dispose`).
- Backend `GET /viewer/content` accepts data URIs for image baseline; this is intentional for MVP demo path.
- Worktree reported detached head in scaffold metadata; verify merge target branch explicitly before cherry-picking/merging.

## Environment State

### Tools/Services Used

- Rust toolchain with workspace cargo commands.
- Axum + ractor backend.
- Dioxus WASM frontend (`wasm32-unknown-unknown` check used).
- Session-handoff scripts (`skills/session-handoff/scripts/create_handoff.py`, `validate_handoff.py`).

### Active Processes

- No long-running services intentionally left running by this session.

### Environment Variables

- `DATABASE_URL`

## Related Resources

- `docs/design/2026-02-05-r3-content-viewer-mvp-spec.md`
- `docs/design/2026-02-05-ui-storage-reconciliation.md`
- `sandbox/src/api/viewer.rs`
- `sandbox/tests/viewer_api_test.rs`
- `sandbox-ui/src/viewers/shell.rs`
- `sandbox-ui/src/viewers/text.rs`
- `sandbox-ui/src/viewers/image.rs`
- `sandbox-ui/public/viewer-text.js`

---

**Validation run in this session**:
- `cargo fmt`
- `cargo test -p sandbox --test viewer_api_test` (4 passed)
- `cargo test -p sandbox --test desktop_api_test test_open_window_preserves_viewer_props` (1 passed)
- `cargo check -p sandbox-ui --target wasm32-unknown-unknown` (passed)
- `cargo test -p sandbox-ui --lib` (3 passed)
