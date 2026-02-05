# R4 - Storage Conformance Audit

**Date:** 2026-02-05
**Status:** Finalized

## Scope

Audit current and planned UI/storage behavior against the storage reconciliation policy and produce concrete remediation + test requirements.

## Inputs

- `docs/design/2026-02-05-ui-storage-reconciliation.md`
- `docs/design/2026-02-05-ui-implementation-backlog.md`
- `docs/design/2026-02-05-ui-master-execution-plan.md`
- `docs/design/2026-02-05-r1-dioxus-architecture-decomposition.md`
- `docs/design/2026-02-05-r2-window-management-execution-spec.md`
- `docs/design/2026-02-05-r3-content-viewer-mvp-spec.md`
- `docs/design/2026-02-05-r5-theme-style-profile-architecture.md`
- `sandbox-ui/src/api.rs`
- `sandbox-ui/src/components.rs`
- `sandbox-ui/src/desktop.rs`
- `sandbox-ui/src/desktop_window.rs`
- `sandbox/src/api/mod.rs`
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/api/user.rs`
- `sandbox/src/actors/desktop.rs`
- `shared-types/src/lib.rs`
- `sandbox/tests/chat_api_test.rs`
- `sandbox/tests/websocket_chat_test.rs`
- `sandbox/tests/desktop_api_test.rs`

## Policy Gates

1. Backend/EventStore canonical.
2. Browser storage non-authoritative.
3. Backend wins on conflict.
4. UI has no local source-of-truth for domain state.

## Evidence Snapshot

1. Theme preferences are backend-persisted (`/user/{user_id}/preferences`) and validated to `light|dark`, with frontend local cache as a bootstrap optimization.
- Backend: `sandbox/src/api/mod.rs`, `sandbox/src/api/user.rs`
- Frontend cache + backend override: `sandbox-ui/src/desktop.rs`
- API client path: `sandbox-ui/src/api.rs`
- Existing tests: `sandbox/tests/desktop_api_test.rs`

2. Chat history is EventStore-backed over HTTP and streamed over WS, but transport-shape parity is only partially tested.
- HTTP mapping with `__tool_call__:` / `__tool_result__:` prefixes: `sandbox/src/api/chat.rs`
- WS streaming for `thinking`, `tool_call`, `tool_result`, `response`: `sandbox/src/api/websocket_chat.rs`
- UI normalization + bundling (`collapse_tool_messages`, assistant bundles): `sandbox-ui/src/components.rs`
- Existing HTTP tool-prefix test: `sandbox/tests/chat_api_test.rs`
- WS tests currently focus on connection/protocol basics, not tool-stream parity: `sandbox/tests/websocket_chat_test.rs`

3. Window open/close/focus/move/resize persistence is event-sourced; minimize/maximize/restore is not implemented despite fields existing.
- Event append + projection for open/close/move/resize/focus: `sandbox/src/actors/desktop.rs`
- REST surface for same operations: `sandbox/src/api/desktop.rs`
- `WindowState` includes `minimized`/`maximized`: `shared-types/src/lib.rs`
- UI drag/resize handlers are placeholders (`start_drag`, `start_resize`): `sandbox-ui/src/desktop_window.rs`
- Existing desktop persistence tests: `sandbox/tests/desktop_api_test.rs`

4. Viewer framework is still unimplemented in runtime UI.
- Non-chat/terminal apps show fallback text: `sandbox-ui/src/desktop_window.rs`
- Viewer work is still spec/backlog only: `docs/design/2026-02-05-r3-content-viewer-mvp-spec.md`, `docs/design/2026-02-05-ui-implementation-backlog.md`

## Conformance Matrix (Current + Planned)

Legend: `Pass` = meets all gates now, `Partial` = directionally aligned with gaps, `Fail` = violates or missing required contract.

| Feature | Phase | Status | Evidence | Gap / Risk | Required Action |
|---|---|---|---|---|---|
| Theme preference persistence (`light/dark`) | Current | Pass | `sandbox/src/api/user.rs`, `sandbox-ui/src/desktop.rs`, `sandbox/tests/desktop_api_test.rs` | Cache-bootstrap conflict path is implemented but not explicitly tested at UI boundary. | Add targeted conflict tests (see Test Additions T1/T2). |
| Theme bootstrap cache behavior (`localStorage` non-authoritative) | Current | Partial | `sandbox-ui/src/desktop.rs` | No automated test proving backend override of stale cache on bootstrap and cache rewrite. | Add UI/unit-style bootstrap reconciliation tests (T1). |
| Theme style-profile architecture (`base_theme`, `style_profile`, overrides) | Planned (R5) | Fail | `docs/design/2026-02-05-r5-theme-style-profile-architecture.md`, `sandbox/src/api/user.rs` | Current backend contract only stores `theme`; profile metadata has no canonical schema/events yet. | Introduce versioned backend preference schema + migration path (R5-1, R5-2). |
| Chat history hydration from HTTP EventStore | Current | Pass | `sandbox/src/api/chat.rs`, `sandbox/tests/chat_api_test.rs` | HTTP-only correctness covered for tool-prefix inclusion but not end-to-end parity with WS stream framing. | Add parity tests across transports (T3/T4). |
| Chat WS stream rendering and tool timeline | Current | Partial | `sandbox/src/api/websocket_chat.rs`, `sandbox-ui/src/components.rs` | WS tests do not assert tool_call/tool_result ordering and compatibility with HTTP collapse path. | Add WS tool-stream integration tests and shared fixture parity tests (T3/T4). |
| Chat reconnect/recovery behavior | Current | Partial | `sandbox-ui/src/components.rs`, `sandbox/src/api/websocket_chat.rs` | Reconnect + replay semantics are implicit; no explicit conflict/replay contract test. | Define cursor/replay rule and test reconnect merge behavior (R4-3, T5). |
| Window state persistence (open/close/move/resize/focus) | Current | Pass | `sandbox/src/actors/desktop.rs`, `sandbox/src/api/desktop.rs`, `sandbox/tests/desktop_api_test.rs` | Core persistence exists; high-frequency interaction fidelity in UI not complete. | Implement production drag/resize behavior and interaction tests (R2-1, T6). |
| Window minimize/maximize/restore persistence | Planned (R2) | Fail | `shared-types/src/lib.rs`, `sandbox/src/actors/desktop.rs`, `docs/design/2026-02-05-r2-window-management-execution-spec.md` | Fields exist, but no API routes/events/actor handlers for transitions. | Add backend events + endpoints + projection + tests (R2-2, T7). |
| Frontend drag/resize write-through strategy | Planned/Current gap (R2) | Partial | `sandbox-ui/src/desktop_window.rs`, `docs/design/2026-02-05-r2-window-management-execution-spec.md` | Handlers are stubs; no debounce/final-commit contract implemented. | Implement pointer lifecycle + throttled persistence contract (R2-1, R2-3, T6). |
| Viewer persistence contract (metadata/content backend-first) | Planned (R3) | Fail | `docs/design/2026-02-05-r3-content-viewer-mvp-spec.md`, `sandbox-ui/src/desktop_window.rs` | No viewer backend API/event contract exists; UI fallback only. | Define viewer actor/event/API before UI implementation (R3-1, R3-2, T8/T9). |
| Browser cache invalidation/versioning policy | Planned (P2) | Fail | `docs/design/2026-02-05-ui-implementation-backlog.md` | Cache versioning/invalidation policy is listed but unimplemented and untested. | Add explicit cache envelope/version rules and invalidation tests (R4-4, T10). |
| UI decomposition (R1) without introducing local SoT | Planned (R1) | Partial | `docs/design/2026-02-05-r1-dioxus-architecture-decomposition.md` | Decomposition is still in-progress; state ownership boundaries not finalized in code. | Require storage-gate checklist per extraction slice (R1-1, T11). |

## Conflict-Resolution Rules (Normative)

These rules are required for current and future features.

| Scenario | Canonical Source | Resolution Rule | Required Behavior |
|---|---|---|---|
| Stale browser theme cache at startup | Backend `GET /user/{user_id}/preferences` | Apply cache only as initial hint; replace with backend value once fetched. | UI must rewrite cache to backend value and render backend value as final. |
| Theme update write race (cache write succeeds, backend write fails) | Backend | Treat local write as provisional; on next fetch, backend value wins. | Keep warning log; do not promote cache to authoritative state. |
| HTTP history and WS stream disagree on tool event shape | EventStore semantics as represented by backend contracts | Normalize both transports to a single UI shape (`ToolEntry` bundle). | Shared transport-parity tests must validate identical rendered structure. |
| WS disconnect during generation | EventStore + subsequent HTTP fetch | Reconnect path must reconcile using backend history, not cached pending-only state. | On reconnect, refresh via HTTP and collapse events deterministically. |
| Partial event replay / duplicate tool events on reconnect | Event sequence from EventStore (`seq`) | De-duplicate/merge based on deterministic replay boundary (cursor + message identity policy). | UI must not duplicate tool call/result rows after replay. |
| Concurrent window operations from multiple clients | Desktop actor event stream | Last appended event sequence defines final state. | Frontend local optimistic state must accept backend projection updates. |
| Future viewer local cache diverges from backend metadata/content | Backend viewer API/event stream | Viewer cache is optimization only. | Backend snapshot must overwrite stale local cache on open/reopen. |

## Concrete Remediation Backlog

### P0

1. `R4-1` Theme bootstrap conformance tests.
- Add explicit tests for stale-cache vs backend override and cache rewrite on success.
- Files: `sandbox-ui/src/desktop.rs` (logic under test), new test module under `sandbox-ui/src/` or `sandbox-ui/tests/`.

2. `R4-2` Chat transport parity harness.
- Add test fixtures asserting parity between HTTP message hydration (`/chat/{actor_id}/messages`) and WS stream (`/ws/chat/{actor_id}`) for thinking/tool call/tool result/final response sequences.
- Files: `sandbox/src/api/chat.rs`, `sandbox/src/api/websocket_chat.rs`, `sandbox-ui/src/components.rs`, tests in `sandbox/tests/`.

3. `R2-1` Implement real drag/resize interaction path.
- Replace `start_drag`/`start_resize` placeholders with pointer lifecycle + throttled writes + final commit semantics.
- Files: `sandbox-ui/src/desktop_window.rs`, `sandbox-ui/src/desktop.rs`.

4. `R2-2` Add minimize/maximize/restore backend contract.
- Add DesktopActor messages, event types, API routes, and projection logic for state transitions.
- Files: `sandbox/src/actors/desktop.rs`, `sandbox/src/api/desktop.rs`, `sandbox/src/api/mod.rs`, `shared-types/src/lib.rs`.

### P1

5. `R3-1` Define viewer canonical persistence contract before UI.
- Specify actor/event types and endpoint surface for viewer metadata/content and save lifecycle.
- Files: new backend API/actor modules in `sandbox/src/`, contract types in `shared-types/src/lib.rs`.

6. `R3-2` Implement viewer shell only after backend contract lands.
- Replace fallback path for non-chat/terminal windows with viewer shell states (`loading/ready/dirty/failed`) bound to backend APIs.
- Files: `sandbox-ui/src/desktop_window.rs`, `sandbox-ui/src/api.rs`.

7. `R5-1` Extend user preferences schema to profile model.
- Add backward-compatible response schema with `theme` + profile fields.
- Files: `sandbox/src/api/user.rs`, `shared-types/src/lib.rs`, `sandbox-ui/src/api.rs`.

8. `R5-2` Profile safety + migration.
- Enforce allowlisted style tokens and fallback rules; preserve legacy `theme` clients.
- Files: `sandbox/src/api/user.rs`, `sandbox-ui/src/desktop.rs`.

### P2

9. `R4-3` Reconnect/replay determinism contract for chat.
- Define replay cursor and duplicate suppression behavior; test against repeated reconnects.
- Files: `sandbox/src/api/websocket_chat.rs`, `sandbox-ui/src/components.rs`.

10. `R4-4` Cache envelope/versioning policy.
- Add explicit versioned cache key/value format and invalidation triggers for all browser-side caches.
- Files: `sandbox-ui/src/desktop.rs`, future viewer cache modules.

11. `R1-1` Storage-gate checklist for decomposition slices.
- For each R1 extraction PR, assert no new local source-of-truth introduced.
- Files: `docs/design/2026-02-05-r1-dioxus-architecture-decomposition.md`, affected `sandbox-ui/src/*` modules.

## Required Test Additions

1. `T1` Theme stale-cache override test.
- Seed local `theme-preference=light`, backend returns `dark`; assert final document theme and cache are `dark`.
- Target code: `sandbox-ui/src/desktop.rs`, `sandbox/src/api/user.rs`.

2. `T2` Theme backend failure fallback test.
- Backend request fails: cached valid theme persists; with no cache, default `dark` applies.
- Target code: `sandbox-ui/src/desktop.rs`.

3. `T3` WS tool stream contract test.
- Send message over WS and assert receipt/order of `thinking`, `tool_call`/`tool_result` (when emitted), and `response` envelope.
- Target test file: `sandbox/tests/websocket_chat_test.rs`.

4. `T4` HTTP-vs-WS render parity test.
- Given identical underlying events, assert `collapse_tool_messages` output shape matches WS-assembled assistant bundle semantics.
- Target files: `sandbox/tests/chat_api_test.rs`, `sandbox-ui/src/components.rs`.

5. `T5` Reconnect replay dedup test.
- Simulate disconnect/reconnect and verify no duplicated tool entries or assistant bundles.
- Target files: `sandbox/tests/websocket_chat_test.rs`, `sandbox-ui/src/components.rs`.

6. `T6` Window drag/resize interaction persistence tests.
- Assert pointer move produces bounded updates and final backend commit; refresh restores last committed bounds.
- Target files: `sandbox-ui/src/desktop_window.rs`, `sandbox/tests/desktop_api_test.rs`.

7. `T7` Minimize/maximize/restore API + persistence tests.
- Cover transition events, idempotency, and restore semantics across refresh.
- Target files: `sandbox/tests/desktop_api_test.rs`, `sandbox/src/actors/desktop.rs`.

8. `T8` Viewer metadata canonicality tests.
- Ensure backend metadata overrides stale client cache on open/reopen.
- Target files: new viewer backend tests under `sandbox/tests/`.

9. `T9` Viewer dirty/save lifecycle tests.
- Validate dirty flag transitions, save event append, and restore consistency.
- Target files: new viewer tests in `sandbox/tests/` and UI tests in `sandbox-ui/`.

10. `T10` Cache version invalidation tests.
- Validate stale-version cache entries are ignored and replaced by backend snapshots.
- Target files: `sandbox-ui/src/desktop.rs`, future viewer cache modules.

11. `T11` Decomposition regression guard tests.
- For each R1 extraction, assert identical behavior for bootstrap, WS updates, and persistence wiring.
- Target files: extracted modules from `sandbox-ui/src/desktop.rs`.

## Acceptance Checklist

- [x] Matrix expanded across current and planned features.
- [x] Every `Partial`/`Fail` entry mapped to concrete remediation.
- [x] Conflict-resolution rules defined and backend precedence made explicit.
- [x] Required tests enumerated with target files.
