# R3 - Content Viewer MVP Spec

**Date:** 2026-02-05
**Status:** In progress

## Scope

Define a minimal viewer framework and first supported viewer types with backend-first persistence/data flow.

## Inputs

- `docs/content-viewer-research.md`
- `sandbox-ui/src/desktop_window.rs`
- `sandbox-ui/src/api.rs`

## Non-Goals

- No full media suite in MVP.
- No offline-first local-source-of-truth behavior.

## Current-State Evidence

1. Non-chat/terminal apps currently hit fallback "not yet implemented" window content.
2. Viewer functionality exists in research docs but not in production components.

## MVP Viewer Set (Draft)

1. Text viewer/editor (CodeMirror 6 interop path).
2. Image viewer (zoom/pan + metadata summary).
3. Optional phase-1.5: PDF read-only viewer if cost is manageable.

## Viewer Shell Contract (Draft)

- Header: title, source, actions.
- Body: viewer content mount area.
- Footer/status: loading, save state, errors.
- Shared states: loading, ready, dirty, failed.

## Data Flow Contract (Draft)

1. Backend API/event path is canonical for content metadata and persisted changes.
2. Local cache (if any) is write-through/read-through optimization only.
3. Backend value overrides stale cache on reconciliation.

## Interop Strategy (Draft)

- Lazy-load JS interop per viewer type on window open.
- Ensure teardown hooks run on window close.
- Guard against memory leaks from editor/viewer instances.

## Test Plan (Draft)

1. Viewer shell render contract tests (loading/error/content states).
2. Text viewer dirty/save lifecycle tests.
3. Reopen/restore tests for window-level viewer context.

## Acceptance Checklist

- [ ] MVP types finalized.
- [ ] Shell contract finalized.
- [ ] Data flow contract finalized.
- [ ] Test plan finalized.
