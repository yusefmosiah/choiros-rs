# Marginalia, QWY, and Writer Rendering Overhaul — Execution Directive

Date: 2026-02-20
Status: Authoritative directive for next execution session
Supersedes: Inline addendum written 2026-02-20 (verbal session)
Depends on: `2026-02-17-codesign-runbook.md` (read that first)

---

## Narrative Summary (1-minute read)

ChoirOS is a living-document-first system. The Writer app is the primary human
interface for that. Right now it is broken in two compounding ways: prior run documents
open in an error state ("Live patch stream lost continuity"), and the document model
underneath — a flat markdown file with a sidecar `.rev` counter — has no stable identity
for blocks, no provenance, and no durable run history across sandbox restarts.

This directive resolves both. It integrates the `.qwy` format spec (already fully
designed in the runbook) with a new rendering architecture for Writer (always-rendered
prose, editable in place, spatial margin lanes for annotations) and a clear fix for the
state persistence bug that is blocking everything else.

The order is strict: fix the state bug first, then implement the rendering substrate
(Phase 1.5), then introduce QWY format incrementally. Do not attempt QWY format changes
before the state bug is resolved — the breakage will mask whether QWY is working.

---

## What Changed (from the runbook)

1. **State persistence bug identified and partially fixed** (2026-02-20):
   - Terminal-status runs no longer trigger the continuity error (guard added to
     `view.rs`). This is a partial fix only — the root cause is not yet resolved.
   - `GET /conductor/runs` endpoint added; `conductor_list_runs()` frontend call added.
   - Writer overview now fetches from conductor API instead of the filesystem.
   - These changes are already merged.

2. **Writer rendering architecture decision made** (2026-02-20 session):
   - Collapse Edit/Preview toggle into a single always-rendered, always-editable view.
   - Three-column layout: left margin (AI suggestions), prose body (contenteditable),
     right margin (user notes).
   - Mobile: floating anchor bubbles over prose + bottom sheet.
   - This is not in the runbook. It is captured here for the first time.

3. **Phase 1.5 inserted** (new, between existing Phase 1 and Phase 2):
   - Rendering substrate that unblocks both Marginalia annotation display and, later,
     Phase 8 annotation creation.
   - The inline overlay-in-textarea approach (`compose_editor_text`,
     `INLINE_OVERLAY_MARKER`) is a stopgap that Phase 1.5 replaces.

4. **QWY format implementation sequencing clarified**:
   - Phase 2 types (QWY structs) can be defined in parallel with Phase 1.5 rendering.
   - QWY file I/O (load/save) must not begin until Phase 1.5 is stable.
   - Annotation anchors remain section-level through Phase 1.5; migrate to QWY block
     UUIDs in Phase 8 as the runbook specifies.

---

## What To Do Next (Ordered)

1. **Fix the state persistence bug** — see §State Persistence Bug below. This is the
   immediate blocker. Do not start Phase 1.5 until the Writer opens without errors.
2. **Phase 1.5: Writer rendering substrate** — three-column layout, contenteditable
   prose, margin note display, mobile bubbles.
3. **Phase 2: QWY types** — define structs in `shared-types`, no I/O yet.
4. **Phase 3–8**: follow the runbook sequence unchanged.

---

## Prerequisite: State Persistence Bug

### What is broken

Opening any prior conductor run document produces:
- Status chip: "Init..."
- Error banner: "Live patch stream lost continuity; missing patch event"

### Root cause (full diagnosis)

The bug has three compounding layers:

**Layer 1 — `ACTIVE_WRITER_RUNS` is ephemeral.**
`ACTIVE_WRITER_RUNS` is a `GlobalSignal<HashMap<String, ActiveWriterRun>>` populated
exclusively from live WebSocket events in the current browser session. It has no
persistence, no rehydration on mount, and no API-backed initialization. After any page
reload, it is empty. Runs that finished before the current session have no entry.

**Layer 2 — `ConductorRunState` does not survive sandbox restarts.**
`ConductorState.runs` is a `HashMap<String, ConductorRunState>` in-memory only
(`sandbox/src/actors/conductor/state.rs:15`). The runbook marks Phase 4.4 ("Run state
durability — `restore_run_states` from event store on restart") as Done, but the
`GET /conductor/runs` endpoint introduced on 2026-02-20 returns an empty list after any
sandbox process restart. Either the restoration path is not wired, or it is not being
called on actor startup. This must be diagnosed and fixed.

**Layer 3 — `last_applied_revision` is not reconciled on document open.**
When a run-scoped document is opened (`writer_open`), the response includes the file's
current `revision` from the `.rev` sidecar. The component sets
`last_applied_revision = response.revision` on successful open. However, if
`ACTIVE_WRITER_RUNS` has a stale entry for this path (from a prior live session that was
not cleaned up), and that entry's `revision` is higher than the file's `.rev` sidecar
value (which can happen if patches advanced the in-memory revision but the file was
saved at an earlier point), the gap check fires before the component has a chance to
reconcile.

### Required fixes

**Fix 1 — Reconcile `ACTIVE_WRITER_RUNS` from conductor API on Writer mount.**

In `WriterView`, on mount (before the document-open effect runs), fetch the run state
from `GET /conductor/runs/{run_id}` for the run associated with the document being
opened. If the run is terminal (Completed/Failed/Blocked), inject a synthetic
`ActiveWriterRun` entry with `last_applied_revision` set to the file's `.rev` value and
`status` set to the terminal status from the API. This prevents the gap check from ever
firing against a stale entry. The run_id is extractable from the document path via
`extract_run_id_from_document_path`.

**Fix 2 — Restore `ConductorRunState` from the event store on conductor actor startup.**

In `ConductorActor::pre_start` (or via a `RestoreState` message sent immediately after
spawn), query the `EventStore` for all `conductor.task.*` events and reconstruct
`ConductorRunState` entries. The runbook Phase 4.4 says this is done — verify it is
actually wired and diagnose why `GET /conductor/runs` returns an empty list after
restart. The event store schema has `event_type`, `payload`, `actor_id`, and the events
carry `run_id`, `objective`, `status`, and `desktop_id`. These are sufficient to
reconstruct the summary state needed for the overview listing.

**Fix 3 — Ensure the gap check never fires for terminal-status runs (already merged).**

The guard added on 2026-02-20 (`is_terminal` check before the gap check in `view.rs`)
is correct and should be kept. It handles the case where Fixes 1 and 2 are not yet
complete but the run is terminal.

**Fix 4 — Remove `ACTIVE_WRITER_RUNS` stale-entry accumulation.**

Currently, `ACTIVE_WRITER_RUNS` entries are added when runs start and their status is
updated by WebSocket events, but they are never removed. Over a long browser session,
this map accumulates entries for all runs seen since last reload. This causes false-
positive gap checks when the map contains a stale entry for a path being re-opened.
Add cleanup: when a run reaches a terminal status and its patches are all applied, mark
the entry for removal after a short TTL (e.g., 30 seconds). Or: on document open, if
the entry's status is terminal and `last_applied_revision` matches `response.revision`,
drop the entry (it is now reconciled and no longer needed).

### Acceptance criteria for state bug fix

- `just dev-sandbox` restart followed by opening any prior run document: no error, no
  "Init..." status, document loads with correct content.
- `GET /conductor/runs` returns at least the most recent 10 runs after a sandbox restart.
- Opening the same document twice in the same session does not trigger the gap check.
- The Writer overview shows run objectives (not ULIDs) for all returned runs.

---

## Phase 1.5 — Writer Rendering Substrate

**Insert between existing Phase 1 and Phase 2 in the runbook.**

Goal: replace the textarea/preview toggle with a unified contenteditable prose renderer
and three-column spatial layout. This is the rendering foundation for Marginalia display
(moving margin notes out of the toolbar changeset panel into actual margin lanes) and
for Phase 8 annotation creation.

### Context from existing implementation

The following are already in place and must be preserved:

- `WRITER_STYLES` CSS const in `writer/styles.rs` — extend, do not replace
- Autosave (2s debounce after `SaveState::Dirty`) — keep unchanged
- `Ctrl+S` save — keep unchanged
- Slim toolbar with always-visible Prompt + Save buttons — keep unchanged
- Version navigation (`< v1 of N >`) and provenance badge (AI/User/System) — keep in toolbar
- `LiveChangeset` changeset panel below toolbar — this moves to the left margin in 1.5
- `WriterOverlay` inline injection via `compose_editor_text` / `INLINE_OVERLAY_MARKER`
  — **this approach is superseded** by margin lane rendering in 1.5. Delete
  `compose_editor_text`, `strip_inline_overlay_block`, and `INLINE_OVERLAY_MARKER` from
  `logic.rs` as part of Phase 1.5. Until then, leave them in place.

### 1.5.1 Three-column layout

Replace the current single-column editor/preview area with a three-column CSS grid:

```
.writer-layout {
  display: grid;
  grid-template-columns: 180px 1fr 180px;
  grid-template-rows: 1fr;
  overflow: hidden;
  flex: 1;
}
```

- Left margin (`writer-margin-left`): AI suggestions and system annotations.
  Read-only in Phase 1.5; interactive Accept/Dismiss in Phase 8.
- Prose body (`writer-prose-body`): always-rendered, always-editable.
  Max-width ~680px, centered within the 1fr column using margin: auto.
- Right margin (`writer-margin-right`): user notes.
  Empty in Phase 1.5; user-creatable in Phase 8.

Responsive collapse:
- `< 900px`: `grid-template-columns: 0 1fr 180px` — left margin hidden with
  `overflow: hidden; width: 0`. Right margin becomes a slide-in overlay drawer,
  triggered by a note-count badge button at the prose column's right edge.
- `< 640px`: `grid-template-columns: 0 1fr 0` — both margins hidden. Annotations
  become floating bubbles (see §1.5.4).

The `< 640px` breakpoint is the mobile threshold. Use `@media (max-width: 640px)` in
`WRITER_STYLES`.

### 1.5.2 Contenteditable prose renderer

Replace `<textarea>` (Edit mode) and `dangerous_inner_html div` (Preview mode) with a
single `div[contenteditable="true"]` rendered by Dioxus via `use_eval` JS interop.

**Why `use_eval`:** Dioxus's `dangerous_inner_html` sets HTML but does not support
cursor preservation across re-renders. A contenteditable div requires JavaScript to
manage the Selection API. This is the correct boundary: Rust/Dioxus owns state and
signals; JS owns DOM mutations and cursor tracking.

**Initialization:**
```js
// Called once after mount
function initProse(elementId, initialHtml) {
  const el = document.getElementById(elementId);
  el.innerHTML = initialHtml;
}
```

**On content change:**
```js
// Called from the 'input' event listener
function getProseText(elementId) {
  const el = document.getElementById(elementId);
  // Walk text nodes to extract plain text, preserving paragraph structure
  // Return as a markdown string (paragraphs separated by \n\n)
}
```

**Cursor preservation for re-renders:**
```js
function saveSelection(elementId) { /* save Range offsets */ }
function restoreSelection(elementId, savedRange) { /* restore Range */ }
```

The JS shim lives in `dioxus-desktop/src/components/writer/prose_interop.js` and is
inlined as a `const &str` via `include_str!`. It is injected once via `use_eval` on
mount. Subsequent calls use named JS functions via `use_eval` with parameters.

**Autosave serialization:** On `Ctrl+S` or 2s idle, call `getProseText(id)` via
`use_eval`, receive the result through the eval channel, and pass to `handle_save`.

**Inline formatting (markdown-as-you-type):**
Implement only the four most common shortcuts; do not attempt a full markdown-as-you-
type engine in Phase 1.5:
- `**word**` → bold on closing `**`
- `_word_` → italic on closing `_`
- `` `word` `` → code on closing `` ` ``
- `# ` at start of line → heading

Everything else renders as plain text until the next full re-render (which happens on
save/load).

**No raw markdown visible:** The `textarea` in `ViewMode::Edit` showed raw markdown.
The contenteditable div shows rendered HTML. Switching from textarea to contenteditable
is the primary user-visible change in Phase 1.5.

### 1.5.3 Remove `ViewMode` enum

`ViewMode::Edit` and `ViewMode::Preview` no longer exist after Phase 1.5. Remove:
- `ViewMode` enum from `types.rs`
- `view_mode` signal from `WriterView`
- `set_view_mode` callback
- Edit/Preview toggle buttons from the toolbar
- `update_preview` function from `styles.rs` (the server-side markdown preview is no
  longer needed; rendering is done client-side in the contenteditable)
- `preview_html` signal from `WriterView`
- `writer_preview` API call (only used for the now-removed server preview)

The `ViewMode::Preview` server-side rendering path in `view.rs` (`ViewMode::Preview =>
rsx! { div { dangerous_inner_html: ... } }`) is deleted in favor of the contenteditable
approach.

Note: `writer_preview` API endpoint in the sandbox (`POST /writer/preview`) should be
kept for now — it may be used by other consumers in the future. Only the frontend call
is removed.

### 1.5.4 Margin note cards (desktop)

Move `LiveChangeset` display from the toolbar panel to the left margin column.

Each changeset card in the left margin:
```
.writer-margin-card {
  border-left: 2px solid var(--accent-bg);
  padding: 0.4rem 0.5rem;
  margin-bottom: 0.4rem;
  font-size: 0.72rem;
  color: var(--text-secondary);
  cursor: default;
  position: relative;
}
```

Anchor connector: a 1px horizontal line from the card's vertical midpoint extending
to the prose column edge, at the vertical position of the changeset's anchor paragraph.
Implemented as a pseudo-element (`::after`) with `position: absolute; right: -16px;
width: 16px; height: 1px; background: var(--border-color); top: 50%`.

Vertical positioning: cards stack in document order, not paragraph order. In Phase 1.5,
do not attempt to align cards precisely to their anchor paragraphs — that requires block
UUIDs (Phase 8). Stack them naturally from the top of the margin column.

Impact badge (HIGH/MED/LOW) remains as in the current changeset panel, using the
existing `.writer-impact-badge` CSS classes.

`WriterOverlay` proposal cards: render in the left margin with Accept and Dismiss
buttons. Accept calls `handle_prompt_submit` with the overlay's diff ops. Dismiss calls
a new `POST /writer/overlay/dismiss` endpoint (see §API Changes below). In Phase 1.5,
implement the rendering only; wire Accept/Dismiss in Phase 1.5 as well since they are
small.

### 1.5.5 Mobile floating bubbles (`< 640px`)

When viewport width is below 640px, both margin columns are hidden. Each margin note is
represented as a floating bubble anchored to its paragraph in the prose.

**Bubble element:**
```html
<button class="writer-bubble" style="top: {anchor_top}px" aria-label="Note">◉</button>
```

```css
.writer-bubble {
  position: absolute;
  right: 4px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--accent-bg) 40%, transparent);
  border: none;
  font-size: 10px;
  cursor: pointer;
  transition: opacity 0.2s;
  z-index: 10;
}

.writer-prose-container:focus-within .writer-bubble {
  opacity: 0.15;
}
```

`anchor_top` is computed via JS: `document.getElementById(paragraphId).getBoundingClientRect().top`
relative to the prose container. If `getBoundingClientRect().top === 0` (pre-layout),
set `visibility: hidden` until the next `requestAnimationFrame`.

Since paragraph IDs are not yet available (that is Phase 8 QWY block UUIDs), use a
simpler heuristic in Phase 1.5: assign each paragraph a sequential index on render,
use `data-para-index` attributes, and compute bubble positions based on paragraph count.

**Bottom sheet:** tapping a bubble opens a `position: fixed` bottom sheet:
```css
.writer-bottom-sheet {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  max-height: 60vh;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  border-radius: 12px 12px 0 0;
  padding: 1rem;
  overflow-y: auto;
  z-index: 100;
  transition: transform 0.2s;
}
```

The bottom sheet renders the same card content as the margin card. Close on tap outside
or a dismiss button.

### Phase 1.5 Gate

- Writer opens any prior run document without error or "Init..." status (state bug fix
  prerequisite must be met first).
- Three-column layout renders correctly at 1100px, 900px, 640px, and 375px without
  overflow or broken grid.
- Prose is rendered as HTML (not raw markdown) and is editable in place. Typing in the
  prose area updates content and triggers autosave after 2s idle.
- Cursor position survives a paragraph re-render triggered by a live patch.
- `Ctrl+S` saves immediately.
- `LiveChangeset` cards appear in the left margin, not in the toolbar panel.
- `WriterOverlay` proposal cards appear in the left margin with Accept and Dismiss
  buttons wired.
- Mobile bubbles appear at correct relative positions for documents with at least 3
  changesets.
- `ViewMode` enum, `view_mode` signal, Edit/Preview toggle buttons, `preview_html`
  signal, and `update_preview` function are all deleted.
- `compose_editor_text`, `strip_inline_overlay_block`, and `INLINE_OVERLAY_MARKER` are
  all deleted from `logic.rs`.
- No `<textarea>` and no `dangerous_inner_html` div remain in the writer component.
- `cargo check` passes with zero warnings across the workspace.

---

## Phase 2 — QWY Types (Unchanged from Runbook, Clarifications Added)

The full QWY type spec is in the runbook (§The `.qwy` Document Format). This section
adds implementation guidance only.

### Where types live

All QWY types go in `shared-types/src/lib.rs` alongside the existing `ConductorRunState`,
`WriterRunStatusKind`, etc. They are part of the shared contract, not sandbox-internal.

Key new types (newtypes in the runbook):
```rust
pub struct BlockId(pub ulid::Ulid);
pub struct ChunkHash(pub [u8; 32]);
pub struct TxId(pub ulid::Ulid);
```

`QwyDocument`, `BlockNode`, `BlockType`, `ProvenanceEnvelope`, `PatchEntry`,
`QwyPatchOp`, `VersionIndexEntry` — exactly as specified in the runbook §2.1.

Citation types (`CitationKind`, `CitationStatus`, `CitationRecord`) — as in §2.2.

Embedding collection record types (`UserInputRecord`, `VersionSnapshotRecord`,
`RunTrajectoryRecord`, `DocTrajectoryRecord`) — as in §2.3.
Note: `UserInputRecord` is already partially implemented (used in the conductor execute
endpoint). Reconcile the existing struct with the Phase 2 spec rather than creating a
duplicate.

### What Phase 2 does NOT include

- No file I/O in Phase 2. `QwyDocument` is a type definition only; no serialization to
  disk, no migration of existing `.md` files.
- No CBOR encoding in Phase 2. Use `serde_json` derives for all QWY types. CBOR
  ("canonical CBOR internal" in the runbook) is deferred to Phase 7 when global
  publishing requires a canonical wire format.
- No `chunk_hash` computation in Phase 2. Add the field to `BlockNode` with a `derive`
  but leave SHA-256 computation for Phase 5 when `sha2` is added as a dependency.

### Phase 2 Gate (same as runbook)

All types compile in `shared-types`. BAML types generate without error. No runtime
behavior added. `cargo test --lib` passes across workspace.

---

## API Changes Required (Phase 1.5)

### New: `POST /writer/overlay/dismiss`

```
Request:  { "path": string, "overlay_id": string }
Response: { "dismissed": true }
```

Marks a `WriterOverlay` as rejected. The overlay record status is updated to
`OverlayStatus::Rejected`. The writer actor for this path must be live to receive this;
if no actor is running, the call should return a 404 with code `RUN_NOT_FOUND`.

### Modified: `GET /conductor/runs`

No change to the endpoint signature. However, the handler must return runs that survive
sandbox restarts. This requires Fix 2 from the state bug section above (event store
reconstruction on actor startup). The endpoint is already registered; only the backend
restoration path needs to be wired.

### No other API changes in Phase 1.5

The three-column layout and contenteditable rendering are purely frontend changes. The
document save, open, patch, and version APIs remain unchanged.

---

## What Must NOT Be Done In This Session

1. **Do not start QWY file I/O before Phase 1.5 is stable.** Migrating existing `.md`
   files to `.qwy` format while the rendering substrate is being replaced simultaneously
   will make debugging impossible.

2. **Do not remove the `writer_preview` sandbox endpoint.** Only the frontend call is
   removed. The endpoint may be useful for other consumers.

3. **Do not implement annotation creation** (user writes a margin note). That is Phase 8
   per the runbook. Phase 1.5 only displays existing overlays and changesets in margin
   lanes.

4. **Do not attempt SONA learning, global vector store, or citation extraction** in this
   session. Those are Phases 5–7.

5. **Do not change the conductor dispatch model or the actor supervision tree.** This
   session is focused on state persistence (Fix 2), writer rendering (Phase 1.5), and
   QWY types (Phase 2). Seams 0.1–0.8 from the runbook Phase 0 remain open and should
   be addressed in a dedicated refactor session, not interleaved here.

---

## What Is Already Implemented (Reference)

Do not re-implement these:

| Item | Location | Status |
|---|---|---|
| Autosave (2s debounce) | `view.rs` autosave `use_effect` | ✅ Done 2026-02-20 |
| Terminal-status continuity error guard | `view.rs` `is_terminal` check | ✅ Done 2026-02-20 |
| `GET /conductor/runs` endpoint | `sandbox/src/api/conductor.rs` | ✅ Done 2026-02-20 |
| `conductor_list_runs()` frontend call | `dioxus-desktop/src/api.rs` | ✅ Done 2026-02-20 |
| `ConductorMsg::ListRuns` actor message | `conductor/protocol.rs` | ✅ Done 2026-02-20 |
| Writer overview fetches from conductor API | `writer/view.rs` overview effect | ✅ Done 2026-02-20 |
| Slim toolbar, always-visible Prompt button | `writer/view.rs` toolbar | ✅ Done 2026-02-20 |
| `WRITER_STYLES` CSS const | `writer/styles.rs` | ✅ Done 2026-02-20 |
| Writer module split (6 files) | `writer/` package | ✅ Done 2026-02-20 |
| Version navigation in toolbar | `writer/view.rs` | ✅ Done (prior session) |
| Provenance badge (AI/User/System) | `writer/view.rs` | ✅ Done (prior session) |
| `LiveChangeset` changeset panel | `writer/view.rs` | ✅ Done (prior session) |
| `WriterOverlay` inline injection | `writer/logic.rs` `compose_editor_text` | ⚠️ Stopgap — delete in Phase 1.5 |
| Default window size 1100×720 | `desktop/apps.rs` | ✅ Done 2026-02-20 |
| Phase 4.1, 4.2, 4.4, 4.5 | Various | ✅ Done (per NARRATIVE_INDEX) |
| Phase 5 (MemoryActor + sqlite-vec) | `actors/memory/` | ✅ Done (per NARRATIVE_INDEX) |
| Phase 6a (hypervisor process) | `hypervisor/` | ✅ Done (per runbook) |

---

## Architecture Constraints Carried Forward

From `AGENTS.md` and the Fixed Commitments in the runbook:

1. **Conductor turns are non-blocking and finite.** Do not add any blocking waits in
   conductor actor message handlers, including the new `ListRuns` handler.

2. **Marginalia consumes semantic changes and provenance — not raw chat stream.**
   Margin note content must derive from `WriterRunPatchPayload`, `DocumentVersion`,
   `Overlay`, and `LiveChangeset` events. Never from a conversation transcript.

3. **Writer is the canonical authority for living-document mutation.** The contenteditable
   renderer calls the same `writer_save` / `writer_save_version` / `writer_prompt` APIs
   as the textarea did. No new mutation paths are introduced.

4. **Filesystem artifacts are canonical truth.** QWY types are built on top of the
   existing file model, not as a replacement. In Phase 2, types are defined. In a later
   phase, new documents are created as `.qwy` files while existing `.md` documents
   continue to work unchanged. There is no forced migration.

5. **Backend is canonical for app/window UI state.** The three-column layout and margin
   note visibility state may be stored in frontend signals for now. Persist them via the
   user preferences API (`PATCH /user/{user_id}/preferences`) if they need to survive
   page reloads.

6. **Model-led control flow is default.** The contenteditable renderer and margin note
   display do not introduce any deterministic workflow logic. They render what the event
   stream delivers.

---

## References

- `docs/architecture/2026-02-17-codesign-runbook.md` — authoritative phase plan, QWY
  spec, citation schema, actor topology target state (read this before executing)
- `docs/architecture/2026-02-14-living-document-human-interface-pillar.md` — why Writer
  is the primary human interface
- `docs/architecture/2026-02-16-memory-agent-architecture.md` — episodic memory context
  for why QWY block UUIDs and chunk_hash matter
- `docs/architecture/writer-api-contract.md` — current Writer API contracts
- `docs/architecture/NARRATIVE_INDEX.md` — implementation status, current decisions
- `AGENTS.md` — model-led control flow hard rules, non-blocking conductor constraints
- `dioxus-desktop/src/components/writer/` — current Writer implementation
- `sandbox/src/actors/conductor/` — conductor actor, state, and protocol
- `sandbox/src/api/conductor.rs` — conductor API handlers
