# Backend-Authoritative UI State Pattern

Date: 2026-02-10
Status: Adopted (architecture baseline)
Scope: Desktop apps (`Chat`, `Writer`, `Files`, `Settings`, future app surfaces)

## Narrative Summary (1-minute read)
ChoirOS should treat backend state as the single source of truth for app UI state. Client `localStorage` is removed as an authority layer. This keeps behavior consistent across browsers, sessions, and devices, and aligns with the OS metaphor: one orchestrated runtime with durable state, not per-browser islands.

The state model is split into two typed backend layers:
- Per-window state in Desktop window `props` (what this specific window is doing right now).
- Per-user preferences in user preference APIs/events (defaults and preferences across windows).

This pattern is mandatory for all apps. Client-side state may exist in memory for rendering responsiveness, but must not be persisted to browser storage.

## What Changed
- Formalized a hard architecture rule: no client `localStorage` for canonical app state.
- Defined the two-layer backend state split (`window props` vs `user preferences`).
- Defined synchronization lifecycle (hydrate, mutate, recover, reconcile) for all apps.
- Added migration and validation guidance to remove browser-local persistence safely.

## What To Do Next
1. Inventory every `localStorage` usage and classify each key as either `window props`, `user preferences`, or `delete`.
2. Add/confirm typed API contracts for both layers where gaps exist.
3. Migrate app-by-app (Files, Writer first), then remove browser persistence code.
4. Add tests for cross-browser/session restore to enforce backend-authoritative behavior.
5. Gate merges on this policy: no new browser-persisted app state.

## Problem Statement
Using browser-local persistence for app state introduces split-brain behavior:
- Different browsers/devices show different state.
- State recovery depends on where a user last opened the app.
- Debugging becomes non-deterministic (backend events disagree with frontend state).
- The orchestration model is weakened because app context is hidden in local storage.

For a multi-agent, multi-surface system, this is architectural drift.

## Architecture Rule
`NO CLIENT LOCALSTORAGE FOR APP STATE`

- `localStorage` must not store canonical UI/app state.
- Canonical state must be persisted to backend only.
- In-memory frontend state is allowed for transient UX only.
- If frontend and backend disagree, backend wins.

This rule is a direct extension of `NO ADHOC WORKFLOW`: state authority must be typed and explicit, not implicit in browser caches.

## State Model (Required)

### Layer A: Per-Window State (Desktop Window `props`)
Use for state that belongs to a specific window instance.

Examples:
- Files: `cwd`, `selection`, `expanded_nodes`, `view_mode`
- Writer: `file_path`, `cursor`, `selection`, `scroll`, `preview_enabled`, `dirty`
- Chat (compatibility surface): `thread_id`, `view_filters`, `input_draft` (if window-specific)

Properties:
- Scoped to desktop + window identity.
- Restored when reopening/syncing that window.
- Mutated via typed backend window-state update pathway.

### Layer B: Per-User Preferences
Use for defaults that apply across windows/sessions.

Examples:
- Files: `default_root`, `show_hidden`, `sort_order`
- Writer: `tab_size`, `word_wrap`, `markdown_preview_default`, `font_size`
- Global: `theme`, `time_format`, `density`

Properties:
- Scoped to user identity.
- Independent of any single window.
- Persisted as typed preference events/records.

## Lifecycle Pattern

### 1) Hydration
On app/window open:
1. Load desktop/window state from backend (`window props`).
2. Load user preferences from backend.
3. Compose effective state:
   - `effective = window_props override user_preferences override app_defaults`

### 2) Mutation
On user interaction:
- Update in-memory state immediately for UX.
- Persist typed update to backend (debounced where appropriate).
- Do not write to browser storage.

### 3) Reconnect / New Browser
- Rehydrate from backend only.
- Do not attempt to restore from browser-local cache.

### 4) Conflict Resolution
- Backend timestamps/revisions decide winner.
- Frontend treats server push/ack as canonical.

## API/Contract Guidance
All app state writes must be typed.

Required contract characteristics:
- Typed enum/struct payloads for state patches.
- Explicit scope fields (`user_id`, `desktop_id`, `window_id` as applicable).
- Version/revision marker for optimistic UI and conflict handling.
- Validation at API boundary (reject unknown/invalid keys).

Prohibited:
- Stringly-typed “blob” patches with implicit semantics.
- Content-based phrase matching to infer state transitions.

## Relationship to Conductor-First Direction
This pattern supports the `Prompt Bar -> Conductor` path:
- Conductor/orchestrators can reason over shared backend state.
- App actors remain deterministic and replayable.
- UI identity and agent identity remain legible across surfaces.

If state lives in browsers, conductor cannot reliably observe or coordinate behavior.

## Migration Strategy

### Phase 1: Audit
- Enumerate existing `localStorage` keys per app.
- Map each key to:
  - `window props`
  - `user preference`
  - `delete`

### Phase 2: Dual-Read (Short-lived)
- Read backend first.
- If backend missing and browser key exists, perform one-time backend backfill.
- Stop writing browser key.

### Phase 3: Cutover
- Remove browser reads entirely.
- Remove key migration code after one release window.

### Phase 4: Enforcement
- Lint/checklist rule: reject new app-state `localStorage` usage.
- Add integration tests for cross-browser restore parity.

## Validation Checklist
- Opening same desktop in two browsers shows identical app state.
- Refreshing browser preserves state via backend rehydrate.
- Window-specific state remains window-specific.
- User defaults apply to new windows consistently.
- No app feature depends on `localStorage` persistence.

## Non-Goals
- This policy does not define authn/authz implementation details.
- This policy does not require offline-first browser behavior.
- This policy does not prohibit ephemeral in-memory state for rendering.

## Terminology
- `Canonical state`: backend-authoritative state used for restore and sync.
- `Transient state`: in-memory frontend state that can be dropped/rebuilt.
- `Window props`: per-window state persisted in desktop/window model.
- `User preferences`: per-user defaults persisted across windows.
