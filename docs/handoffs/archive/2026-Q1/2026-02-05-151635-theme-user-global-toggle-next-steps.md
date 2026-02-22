# Handoff: User-Global Theme Persistence + UI Toggle

## Session Metadata
- Created: 2026-02-05 15:16:35
- Project: /Users/wiz/choiros-rs
- Branch: main

## Handoff Chain
- **Continues from**: `docs/handoffs/2026-02-05-144456-chat-tool-streaming-ui-next-steps.md`
- **Focus of this session**: finish small preference-contract hardening and connect UI toggle to backend theme persistence.

## Current State Summary
User-global theme preference is now fully wired end-to-end for `light|dark` with backend authority and local cache fallback. The desktop UI initializes from cache for startup latency, then syncs to backend preference and updates the DOM theme attribute. A prompt-bar theme toggle now updates UI state optimistically, caches locally, and persists preference via `PATCH /user/{user_id}/preferences`. API coverage includes default/get/set behavior and invalid theme rejection.

## Architecture Overview
- Theme authority scope for current MVP: **user-global only**.
- Canonical storage: EventStore event `user.theme_preference` on actor id `user:{user_id}`.
- UI behavior:
  1. Read local cache (`theme-preference`) and apply immediately.
  2. Fetch backend preference and apply/correct cache.
  3. Toggle action updates UI first, then persists to backend.
- Accepted values are strictly `light` and `dark`.

## Critical Files
- `sandbox/src/api/user.rs` - user preference API handlers.
- `sandbox/src/api/mod.rs` - API route registration for user preferences.
- `shared-types/src/lib.rs` - new `EVENT_USER_THEME_PREFERENCE` constant.
- `dioxus-desktop/src/api.rs` - frontend fetch/patch helpers for user theme preference.
- `dioxus-desktop/src/desktop.rs` - theme init/toggle wiring and prompt bar control.
- `sandbox/tests/desktop_api_test.rs` - preference API integration tests.
- `sandbox/tests/chat_api_test.rs` - tool history hydration integration test for HTTP path parity.

## Files Modified
- `sandbox/tests/desktop_api_test.rs`
  - Added `test_update_user_preferences_rejects_invalid_theme` to enforce `400` on non-`light|dark` values.
- `dioxus-desktop/src/desktop.rs`
  - Added `current_theme` signal and `toggle_theme` callback.
  - Added prompt-bar theme toggle button and wiring to backend persistence.
- `dioxus-desktop/src/api.rs`
  - Added `update_user_theme_preference` (`PATCH /user/{user_id}/preferences`).
- Already in prior step and now validated in this flow:
  - `sandbox/src/api/user.rs`, `sandbox/src/api/mod.rs`, `shared-types/src/lib.rs`.
  - `sandbox/tests/chat_api_test.rs` tool hydration test.

## Decisions Made
1. Keep theme scope user-global for now; defer sandbox-level overrides.
2. Enforce backend validation (`light|dark`) at API boundary.
3. Keep optimistic UI toggle behavior with warning-only logging on persistence failure.

## Validation Performed
- `cargo fmt`
- `cargo check -p dioxus-desktop`
- `cargo test -p sandbox --test desktop_api_test test_update_user_preferences_rejects_invalid_theme`
- `cargo test -p sandbox --test desktop_api_test`
- `cargo test -p sandbox --test chat_api_test`

All passed in this session (existing non-blocking warnings remain in sandbox actor modules).

## Immediate Next Steps
1. Add rollback behavior for theme toggle when backend persistence fails (currently optimistic without rollback).
2. Replace hardcoded `user-1` in desktop UI with authenticated user identity flow.
3. Extract theme logic from `desktop.rs` into dedicated module/hooks (`theme_state.rs` / `use_theme.rs`) to reduce component size.
4. Add browser-level E2E for theme toggle persistence across reload (cache + backend reconciliation).
5. Continue window-management decomposition by extracting prompt bar and desktop icon grid into separate modules.
6. Add API docs for `/user/{user_id}/preferences` contract and error semantics.

## Assumptions Made
- Current sandbox runs in single-user local mode where `user-1` is valid placeholder identity.
- Theme set remains binary (`light|dark`) for MVP; custom themes are deferred.
- EventStore remains canonical source for user preference state.

## Potential Gotchas
- If backend update fails after optimistic toggle, UI and canonical backend can diverge until next refresh.
- Hardcoded user id may create incorrect preference ownership once multi-user auth is introduced.
- Future custom theme expansion will require API/schema changes beyond current strict validation.

## Important Context
This session intentionally prioritized contract hardening and end-to-end completion over adding broader theming features. The current state is stable for MVP usage: the user can toggle theme and have it persist to backend, and invalid values are rejected. The next work should focus on robustness (rollback/error UX), identity correctness, and continued modularization of the large desktop component.

## Related Resources
- `docs/design/2026-02-05-ui-storage-reconciliation.md`
- `docs/design/2026-02-05-ui-implementation-backlog.md`
- `docs/handoffs/2026-02-05-144456-chat-tool-streaming-ui-next-steps.md`

