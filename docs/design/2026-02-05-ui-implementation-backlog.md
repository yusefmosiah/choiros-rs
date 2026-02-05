# UI Implementation Backlog (Post-Reconciliation)

## Scope

Backlog derived from reconciled research docs and the storage policy in
`docs/design/2026-02-05-ui-storage-reconciliation.md`.

## Priority P0

1. Chat history parity across transports
- Ensure HTTP + WS paths both preserve tool call/result rendering payload shape.
- Add integration test coverage for tool event hydration in `GET /chat/{actor_id}/messages`.

2. Theme persistence contract
- Add backend endpoint/event path for theme preference persistence.
- Implement frontend write-through cache (`localStorage`) that mirrors backend value.

3. Window management decomposition
- Split monolithic desktop window logic into focused components:
  - window canvas,
  - title bar,
  - drag/resize handles,
  - z-index/focus coordinator.

## Priority P1

1. Viewer framework shell
- Introduce common viewer container API (loading/error/metadata/actions regions).
- Keep persistence backend-first; optional IndexedDB cache for large asset metadata.

2. Theme system rollout
- Migrate current desktop/chat styles to semantic CSS tokens.
- Add light/dark toggling with backend-backed preference restore.

3. Chat status UX
- Improve incremental thinking/status display and completion transitions.

## Priority P2

1. Performance and resilience
- Add cache invalidation/versioning policy for client caches.
- Add event replay stress tests for large chat/tool histories.

2. Optional advanced UX
- Add richer tool timeline grouping (call/result pairing by sequence).
- Add viewer prefetch heuristics with bounded cache size.
