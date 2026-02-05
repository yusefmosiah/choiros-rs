# R1 - Dioxus Architecture Decomposition

**Date:** 2026-02-05
**Status:** In progress

## Scope

Define a concrete decomposition plan for `sandbox-ui/src/desktop.rs` into independently testable and mergeable components while preserving current behavior.

## Inputs

- `docs/research-dioxus-architecture.md`
- `sandbox-ui/src/desktop.rs`
- `sandbox-ui/src/components.rs`
- `sandbox-ui/src/desktop_window.rs`

## Non-Goals

- No implementation code in this lane.
- No behavior or visual redesign.
- No persistence contract changes.

## Current-State Evidence

1. `sandbox-ui/src/desktop.rs` currently hosts shell layout, ws updates, window rendering, and theme logic.
2. `sandbox-ui/src/components.rs` contains chat and shared UI blocks.
3. `sandbox-ui/src/desktop_window.rs` contains floating window rendering and control stubs.

## Proposed Decomposition Targets

1. `DesktopShell` - top-level layout, bootstrapping, desktop metadata.
2. `WorkspaceCanvas` - desktop icon grid + window layer orchestration.
3. `WindowCanvas` - ordered window list render and active/focus handoff.
4. `PromptBar` - command input, app launcher/status widgets.
5. `ThemeBridge` - applies theme/profile tokens to root attributes.
6. `WsProjection` (module or hook) - websocket event projection into local signals.

## State Ownership Map (Draft)

- Backend-projected state: desktop/window/app registry/user pref snapshots.
- Optimistic state: pending UI actions, temporary drag state, transient error/status.
- Derived memoized state: active window list, sorted z-order, visible app chips.

## File Move Plan (Draft)

1. Extract pure presentational components first (`PromptBar`, window list shell).
2. Move websocket projection logic into dedicated module.
3. Move theme application logic into dedicated module/hook.
4. Reduce `desktop.rs` to orchestration root.

## Performance Notes (Draft)

- Add stable keys on window list and app list rendering paths.
- Memoize derived lists used every render.
- Avoid style string recomputation in hot render paths.

## Open Questions

1. Should drag state remain local to `WindowCanvas` or be lifted to desktop root?
2. Should theme bridge own local cache reads or delegate to API bootstrap layer?

## Acceptance Checklist

- [ ] Decomposition map finalized.
- [ ] State map finalized.
- [ ] Stepwise implementation sequence defined.
- [ ] Test impact noted for each extraction step.
