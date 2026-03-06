# Tracing UX Upgrade

## Narrative Summary (1-minute read)

The tracing view was audited in both light and dark mode with Playwright. It has significant issues:
hardcoded dark-only colors make it unusable in light mode, the title bar wastes 51px on 3 elements
that fit in 32px, the 4,587-line monolith is hard to maintain, and there is no high-level view
across multiple runs. This document specifies what we're fixing and how.

## What Changed (from audit)

- **Light mode broken**: ~40 hardcoded hex colors in `TRACE_VIEW_STYLES` and `CHAT_STYLES` never
  adapt to `data-theme="light"`. The trace window stays near-black in light mode.
- **Title bar too tall**: `padding: 0.75rem 1rem` → 51px for icon + name + 3 buttons. Should be 32px.
- **Redundant inner header**: The `chat-header` inside TraceView is 84px tall and just says "LLM
  Traces" (the window titlebar already identifies the app) plus a "Runs" toggle button.
- **No multi-run overview**: The root view is always a single-run drill-down. Users must open the
  sidebar, find a run by raw ULID, and click in. There is no fleet-level overview.
- **4,587-line monolith**: All data types, parsers, layout algorithms, and rendering live in one
  file. Hard to navigate, review, or extend.
- **Run IDs shown as raw ULIDs**: No human-readable objective text as the primary label.
- **Sidebar collapsed by default**: The runs list is hidden on open, with no visible toggle affordance.

## What To Do Next

1. Fix light/dark mode — add CSS variable overrides to TRACE_VIEW_STYLES and CHAT_STYLES
2. Shrink the window titlebar — reduce padding from `0.75rem 1rem` to `0.35rem 0.75rem`
3. Remove the redundant inner header — collapse it to a minimal toolbar (just buttons, no h3)
4. Add a multi-run overview as the root/default view of TraceView
5. Refactor trace.rs into modules

---

## 1. Light/Dark Mode Fix

### Root cause

`TRACE_VIEW_STYLES` (462 lines, `trace.rs:15`) uses hardcoded hex values for all backgrounds,
borders, and text colors. The design token system in `shell.rs` provides CSS variables for both
themes but none of them are used in the trace CSS.

### Mapping: hardcoded → CSS variable

| Hardcoded value | Replace with | Role |
|---|---|---|
| `#0b1222` | `var(--bg-primary)` | deepest background (sidebar, graph bg) |
| `#0f172a` | `var(--bg-primary)` | base background (pill bg, run status bg) |
| `#081022` | `color-mix(in srgb, var(--bg-primary) 95%, black 5%)` | call card (darker than primary) |
| `#111827` | `var(--bg-secondary)` | secondary bg (lifecycle chips, system-bubble) |
| `#1e293b` | `var(--bg-secondary)` | secondary bg (thread-new-button) |
| `#1f2937` | `var(--bg-secondary)` | token badge bg |
| `#111b32` | `color-mix(in srgb, var(--bg-primary) 85%, var(--accent-bg) 15%)` | selected/chip bg |
| `#13213d` | `color-mix(in srgb, var(--bg-primary) 75%, var(--accent-bg) 25%)` | active selected bg |
| `#334155` | `var(--border-color)` | standard border |
| `#1f2a44` | `var(--border-color)` | trace-specific slightly lighter border |
| `#2f4f7a` | `color-mix(in srgb, var(--border-color) 60%, var(--accent-bg) 40%)` | accent border |
| `#dbeafe` | `var(--text-primary)` | light text on dark bg |
| `#cbd5e1` | `var(--text-secondary)` | muted text |
| `#bfdbfe` | `color-mix(in srgb, var(--text-primary) 80%, var(--accent-bg) 20%)` | loop title |
| `#d1d5db` | `var(--text-secondary)` | token badge text |

Status colors (green/red/yellow/blue for completed/failed/in-progress/inflight) are **semantic**
and intentionally remain hardcoded — they convey meaning regardless of theme.

### Additional: `CHAT_STYLES` hardcoded values

- `.thread-sidebar background: #0b1222` → `var(--bg-primary)`
- `.thread-item:hover background: #111b32` → `var(--hover-bg)`
- `.thread-item.active background: #13213d` → `color-mix(in srgb, var(--bg-primary) 75%, var(--accent-bg) 25%)`
- `.thread-item.active color: #dbeafe` → `var(--text-primary)`
- `.system-bubble background: #111827` → `var(--bg-secondary)`
- `.system-bubble border: #374151` → `var(--border-color)`
- `.tool-details background: #111827; border: #374151` → CSS variables
- `.tool-pre background: #030712` → keep near-black (code blocks always dark)
- `.thread-new-button`, `.thread-run-button` → `var(--bg-secondary)` / `var(--border-color)`

---

## 2. Window Title Bar Reduction

### Current state

```
padding: 0.75rem 1rem   (non-mobile)
→ 12px top + 12px bottom = 24px padding + ~20px content = 51px total
```

### Target state

```
padding: 0.35rem 0.75rem
→ 5.6px top + 5.6px bottom = ~11px padding + 20px content = ~32px total
```

This matches the macOS native title bar (28-30px), VS Code (30px), and typical web app headers.
The window controls are 20×20px (not 24×24) so they fit comfortably.

**Location:** `desktop_window.rs:1041`
```rust
"display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; ..."
```
Change both desktop and mobile variants. Mobile already uses `0.4rem 0.5rem` — adjust to match.

---

## 3. Remove Redundant Inner Trace Header

### Current state

```
TraceView renders:
  div.chat-header (84px, padding: 0.75rem 1rem)
    h3 "LLM Traces"          ← redundant: titlebar already says "🔍 Trace"
    div.trace-header-actions
      button "Runs" / "Hide Runs"
      span.chat-status "● Live" / "○ Connecting"
```

### Target state

Remove `h3`. Reduce `chat-header` padding to `0.35rem 0.6rem`. The header becomes ~36px and only
contains the action buttons and live indicator. Net savings: ~48px of vertical space.

---

## 4. Multi-Run Overview (New Root View)

### Concept

When the Trace window opens, show an **overview grid** of all runs instead of drilling directly
into one. Each run is a card showing:

- Objective text (first 80 chars, or ULID if no objective)
- Status badge (completed / failed / in-progress)
- Key metrics: llm_calls, tool_calls, worker_calls, duration
- Sparkline (the SVG dot row already built by `build_run_sparkline`)
- Relative timestamp ("2m ago", "just now")

Clicking a card drills down into the run-detail view (the existing graph + trajectory + actor panels).

A back button in the run-detail view returns to the overview.

### View states

```
TraceViewState::Overview    → render RunOverviewGrid
TraceViewState::RunDetail { run_id: String }  → render existing run detail
```

### Overview grid layout

```
┌─────────────────────────────────────────────────────┐
│ [Live ●]  3 runs  [+ New]                           │  ← minimal header (36px)
├─────────────────────────────────────────────────────┤
│ ┌────────────────┐ ┌────────────────┐ ┌───────────┐ │
│ │ ✓ completed    │ │ ⟳ in-progress  │ │ ✗ failed  │ │
│ │ "Analyze logs  │ │ "Fix tracing   │ │ "Build    │ │
│ │  and report"   │ │  light mode"   │ │  ui upgr" │ │
│ │ ─────────────  │ │ ────────────── │ │ ───────── │ │
│ │ 26 llm 57 tool │ │ 12 llm 28 tool │ │ 4 llm ... │ │
│ │ ············   │ │ ·····          │ │ ·××·      │ │  ← sparkline
│ │ 2m ago         │ │ just now       │ │ 5m ago    │ │
│ └────────────────┘ └────────────────┘ └───────────┘ │
└─────────────────────────────────────────────────────┘
```

Cards are in a responsive grid: 3 columns at ≥900px, 2 columns at ≥600px, 1 column below.

### Trajectory super-grid (secondary view toggle)

A second view mode (toggled by a "Timeline" button in the header) shows a single SVG grid where
rows are runs (not actors) and columns are time steps — giving a true fleet-level trajectory view.
This reuses `TrajectoryGrid` logic but with `row_key = run_id` and one dot per run per step-bucket.

---

## 5. Refactor: trace.rs → trace/ module

### Target structure

```
dioxus-desktop/src/components/
├── trace/
│   ├── mod.rs           (pub use, ~50 lines)
│   ├── types.rs         (all data structs: TraceEvent, ToolTraceEvent, etc. ~250 lines)
│   ├── parsers.rs       (all parse_* functions ~400 lines)
│   ├── graph.rs         (build_graph_*, GraphNode, GraphEdge, GraphLayout ~400 lines)
│   ├── trajectory.rs    (TrajectoryCell, build_trajectory_*, bucket_* ~300 lines)
│   ├── styles.rs        (TRACE_VIEW_STYLES const ~462 lines)
│   ├── ws.rs            (TraceRuntime, TraceWsEvent, websocket bootstrap ~150 lines)
│   ├── overview.rs      (RunOverviewGrid component ~200 lines)
│   └── view.rs          (TraceView component, main render ~500 lines)
```

Each module is ~200-500 lines. The main `view.rs` just orchestrates — no parsers or data structs.

---

## Implementation Priority

| # | Change | Effort | Impact |
|---|---|---|---|
| 1 | Light/dark CSS variable migration | Medium | High — currently broken |
| 2 | Title bar height reduction | Trivial | Medium — reclaims ~20px/window |
| 3 | Remove redundant trace inner header | Trivial | Medium — reclaims ~48px |
| 4 | Multi-run overview root view | Medium | High — new feature |
| 5 | trace.rs module split | Large | Medium — maintainability |
| 6 | Run sidebar open-by-default | Trivial | Low |
| 7 | Show objective text in run list | Trivial | Low |

Items 1–3 are pure CSS/layout fixes with no logic changes. Do those first.
Item 4 adds ~200 lines of new component code.
Item 5 is a file reorganization with zero functional changes.
