# UI Storage Reconciliation (2026-02-05)

## Status
Accepted

## Context

Recent research docs proposed `IndexedDB`/`localStorage` as primary persistence for viewer content,
playlists, and theme preferences. This conflicts with ChoirOS architecture principles in
`docs/ARCHITECTURE_SPECIFICATION.md`: actor-owned state, EventStore-backed persistence, and UI as
projection.

## Decision

1. Canonical domain state must persist through backend actors/EventStore.
2. Browser persistence (`IndexedDB`, `localStorage`) is optional and non-authoritative.
3. If browser cache and backend diverge, backend state wins.
4. Browser storage is allowed for:
   - startup performance hints,
   - ephemeral UX state,
   - feature flags and temporary drafts that are safe to lose.

## Consequences

- Viewer/media/theme implementation plans must include backend persistence contracts first.
- UI may still use local caches to improve latency, but must treat them as write-through/read-through
  caches with clear invalidation.
- Chat tool history rendering should work from both live WS stream and HTTP history fetch paths
  without introducing a local source-of-truth.
