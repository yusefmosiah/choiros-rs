# Tracing UX Upgrade — Session Record

Date: 2026-02-20

## Narrative Summary (1-minute read)

The tracing view had four compounding problems: broken light/dark mode, a bloated
title bar eating 51px per window, a 4,587-line unmaintainable monolith, and no
high-level view across multiple runs. This session fixed all of them. The monolith
is now an 8-module package. The UI adapts to light and dark mode. Every window is
20px shorter. And opening the Trace app now shows an overview grid of all runs
before drilling in.

The same problems exist in the Writer view. That work is scoped and queued for
the next session.

## What Changed

### 1. Light/Dark Mode — CSS variables throughout

**Problem.** `TRACE_VIEW_STYLES` and `CHAT_STYLES` had ~40 hardcoded dark hex values
(`#0b1222`, `#13213d`, `#111827`, etc.) that never adapted to `data-theme="light"`.
The trace window was visually broken in light mode.

**Fix.** Every hardcoded background, border, and text color was replaced with a CSS
custom property from the existing token system:

| Pattern replaced | Variable used |
|---|---|
| `#0b1222`, `#0f172a` | `var(--bg-primary)` |
| `#111827`, `#1e293b`, `#1f2937` | `var(--bg-secondary)` |
| `#334155`, `#1f2a44`, `#374151` | `var(--border-color)` |
| `#dbeafe`, `#bfdbfe` (text on dark) | `var(--text-primary)` |
| `#cbd5e1`, `#94a3b8`, `#d1d5db` | `var(--text-secondary)` |
| `#111b32`, `#13213d` (selected/hover) | `color-mix(in srgb, var(--bg-primary) %, var(--accent-bg) %)` |
| `#111b32` hover background | `var(--hover-bg)` |

A `[data-theme="light"]` block was appended to `TRACE_VIEW_STYLES` for status
chips, lifecycle chips, and delegation bands — these carry semantic color meaning
(green=success, red=failure) so they need light-mode versions rather than just
inheriting the dark values.

Token badge inline styles (`background:#1f2937;color:#d1d5db`) inside the view
were also converted to CSS variables.

**Files:** `dioxus-desktop/src/components/styles.rs`,
`dioxus-desktop/src/components/trace/styles.rs`

---

### 2. Window Title Bar Height

**Problem.** All floating windows had `padding: 0.75rem 1rem` on the title bar,
producing a 51px strip for three elements (icon, name, window controls). macOS
native title bars are 28–30px; VS Code is 30px. With 3 windows open, this wasted
~60px of vertical real estate purely on chrome.

**Fix.** Padding reduced to `0.35rem 0.75rem` → ~32px. Mobile variant unchanged.

**File:** `dioxus-desktop/src/desktop_window.rs:1041`

---

### 3. Redundant Inner Trace Header

**Problem.** The trace view had an 84px internal header rendering only an `h3`
reading "LLM Traces" — redundant since the window title bar already identifies
the app — plus a "Runs" toggle button. This consumed another ~50px of the content
area.

**Fix.** The `h3` was removed. The inner header padding was reduced to
`0.35rem 0.6rem` (~36px). Net content area reclaimed: ~48px.

Run sidebar defaults to open on load (`use_signal(|| true)`).

Run list items now show objective text (≤60 chars) as primary label, with ULID
in the `title` attribute tooltip. Falls back to first 16 chars of the run ID
when no objective is present.

**File:** `dioxus-desktop/src/components/trace/view.rs`

---

### 4. Multi-Run Overview (new root view)

**Problem.** Opening the Trace window dropped users directly into a single-run
drill-down with no fleet-level view. Users had to know to find and click the
sidebar runs list, which was collapsed by default with no visible toggle.

**New `TraceViewMode` enum:**

```rust
enum TraceViewMode {
    Overview,   // root — card grid of all runs
    RunDetail,  // drill-down — existing graph + trajectory + actor panels
}
```

**Overview view** renders a responsive card grid (`auto-fill, minmax(240px, 1fr)`).
Each card shows:
- Objective text (≤80 chars) or short run ID
- Status badge (completed / failed / in-progress)
- llm / tool / worker call counts, total duration
- Failure count (highlighted red when non-zero)
- Sparkline SVG (reuses existing `build_run_sparkline`)
- Relative timestamp ("just now", "5m ago", "2h ago")

Clicking a card sets `view_mode = RunDetail` and navigates to the full run view.

**Run detail header** now shows a `← Runs` back button that returns to the
overview, plus a "List" / "Hide List" toggle for the sidebar.

**File:** `dioxus-desktop/src/components/trace/view.rs`

New CSS classes in `trace/styles.rs`: `.trace-overview-grid`, `.trace-run-card`,
`.trace-run-card-title`, `.trace-run-card-meta`, `.trace-run-card-footer`,
`.trace-run-card-time`, `.trace-back-btn`.

---

### 5. trace.rs → trace/ module split

**Problem.** `trace.rs` was 4,587 lines containing data types, event parsers,
graph construction, trajectory grid, WebSocket runtime, CSS, and the render
component all in one file.

**New structure:**

```
dioxus-desktop/src/components/trace/
├── mod.rs           198 lines   re-exports, tests
├── types.rs         314 lines   all data structs and enums
├── parsers.rs       736 lines   all parse_* functions
├── graph.rs         733 lines   DAG construction, layout, run summaries
├── trajectory.rs    715 lines   trajectory grid, sparklines, delegation bands
├── styles.rs        595 lines   TRACE_VIEW_STYLES CSS const
├── ws.rs             69 lines   WebSocket runtime
└── view.rs        1,710 lines   TraceView component
```

Zero functional changes. Module dependency graph has no cycles:
`types` ← `parsers` ← `graph`, `trajectory` ← `view`.

Public API surface unchanged: `pub use trace::TraceView` in `components.rs`
continues to work.

---

## What To Do Next (Writer)

The Writer view (`dioxus-desktop/src/components/writer.rs`, 1,567 lines) has the
same class of problems:

1. **Light/dark mode**: `writer.rs` uses hardcoded dark hex values in its inline
   styles and any embedded CSS blocks. Audit and migrate to CSS variables.

2. **Title bar**: already fixed globally — Writer windows inherit the new 32px bar.

3. **Inner content header**: check whether Writer has a redundant internal header
   similar to the old trace `h3 "LLM Traces"` pattern. If so, remove or reduce it.

4. **Information architecture**: Writer currently renders a flat list of run
   revisions without a high-level overview. Consider the same Overview → Detail
   pattern used for Trace: a card grid of documents/revisions at root, drill
   into a specific document for the editor + diff view.

5. **Modular split**: 1,567 lines is manageable but would benefit from extracting
   the diff/revision logic and the live-update streaming into separate modules,
   keeping the render component under ~500 lines.

Reference the Playwright audit methodology used for Trace: open with agent-browser
in both light and dark mode, capture screenshots, inventory hardcoded values, then
apply the same CSS variable migration pattern.
