# Writer UX — Ship Path

**Status**: Ready to implement
**Workstream**: Iterative deploy/QA loop
**Goal**: Writer goes from broken to shippable to best-in-class, incrementally

## Where We Are

The Writer is the core product surface. Five out of five demo users wanted more.
But nobody can use it unsupervised yet. The UX degraded after the marginalia overhaul
(commit 51bb26f, Feb 20) which added a 3-column layout, contenteditable, overlay system,
and 25+ state signals to view.rs (now 1642 lines).

Current state on draft.choir-ip.com (post 30-commit cogent cutover deploy):
- Prompt submission works (conductor routes to writer, Bedrock key fixed)
- Writer completes but content area is BLANK
- Trace shows LLM calls succeeded, content was generated
- Registration works, no data loss

## The Design Principle

**Writer is single-writer authority.** Workers send messages to Writer. Writer decides
what becomes document content. This is the north star for all phases.

### What this means concretely:
- Workers (researcher, terminal) send **diffs and messages** to Writer, not full rewrites
- Messages can include **paths or links** (pointers, not payloads)
- Writer can **reprompt workers mid-flight** to steer their work
- Workers can **inquire of Writer and each other** via channels
- Writer is the **sole merge point** — nothing writes to the document except Writer

### What's wrong in the current code:
- `AUTO_ACCEPT_WORKER_DIFFS = true` (mod.rs:723) — workers bypass Writer authority
- Terminal has `canon_append` mode (terminal.rs:313-359) — direct document writes
- Worker completion triggers auto-rewrite LLM call (mod.rs:2199-2242) as "bug 4+5 fix"
- Overlay system exists (proposals, pending/applied/rejected) but is bypassed
- Researcher contract says "return rewrite instructions" — ambiguous, sometimes full drafts
- Dispatch is fire-and-forget — no mid-flight steering, Writer just waits

The code is trying to be three things: direct worker writes, overlay proposals, AND
Writer-mediated rewrites. The design is just the last one. Simplest and most correct.

## Phase 1: Make It Work (days)

### 1a. Fix the blank document bug

**Root cause**: `WriterDelegationAdapter` (adapter.rs:37-308) has no `write_revision`
tool. When the Conductor routes a simple prompt to Writer, the Writer calls `finished`
without creating any document version. The model obeys the system prompt which says
"call finished and explain why" for editorial tasks — but there's no tool to actually
compose the content.

`WriterUserPromptAdapter` (adapter.rs:318-652) has `write_revision` (lines 376-439)
which creates versions via `WriterMsg::CreateWriterDocumentVersion`. The delegation
adapter does not.

**Fix**: Add `write_revision` mode to `WriterDelegationAdapter`. Update system prompt:
"For editorial tasks, compose the content using `write_revision`, then call `finished`."

Files: `sandbox/src/actors/writer/adapter.rs` (lines 37-308)

### 1b. Verify frontend rendering path

Confirm that `prose_set_markdown()` in `prose_interop.js` is called when a new version
arrives. Signal chain: version created → event emitted → Dioxus signal updated →
`js_sys::eval()` calls `window.__writerProseInterop.setMarkdown(content)` →
contenteditable renders.

Files: `dioxus-desktop/src/components/writer/view.rs`, `prose_interop.js`

### 1c. Stream intermediate versions

During processing, the user sees nothing. Each `CreateWriterDocumentVersion` should
push a `document.version` event so the frontend renders progressively. User's prompt
is v0, then continuously improving versions appear.

### 1d. Hide the terminal app

Don't ship xterm.js to users. Remove from desktop icon grid or gate behind flag.

**QA gate**: User submits prompt → sees content appear → can edit after completion.

## Phase 2: Fix the Worker Contracts (weeks)

This is the structural fix. Align the code with the design principle.

### 2a. Remove direct document writes from workers

- Remove `canon_append` mode from terminal's `message_writer` tool (terminal.rs:313-359)
- Remove `proposal_append` mode (terminal.rs:361-406) or convert to message
- Set `AUTO_ACCEPT_WORKER_DIFFS = false` and eventually remove the flag entirely
- Workers communicate findings via messages, not document mutations

### 2b. Define the worker message contract

Workers send messages to Writer. A message is:
```
WorkerMessage {
    from: WorkerId,
    kind: "finding" | "question" | "progress" | "completion",
    content: String,           // natural language summary
    paths: Vec<String>,        // file paths, URLs, citations
    diff_intent: Option<String>, // suggested change, not a full rewrite
}
```

Writer receives messages, decides what to incorporate, calls `write_revision` to
update the document. Writer is always in the loop.

### 2c. Enable mid-flight steering

Currently dispatch is fire-and-forget (mod.rs:334-494, `tokio::spawn` returns immediately).
Writer should be able to send messages to running workers:
- "Focus on X, not Y"
- "I need citations for this claim"
- "Stop, I have what I need"

This is cogent's co-agent pattern: `spawn_agent`, `post_message`, `wait_for_message`.
The same primitives, applied inside the Writer actor.

### 2d. Enable worker-to-worker channels

Researcher needs something from Terminal? It messages Terminal directly.
Terminal found code that needs research context? It messages Researcher.
Writer observes the channel but doesn't have to mediate every hop.

### 2e. Remove the auto-rewrite workaround

The "bug 4+5 fix" at mod.rs:2199-2242 auto-triggers an LLM rewrite after every
worker completion. This is a patch for the broken communication flow. Once workers
send proper messages and Writer processes them, this workaround is unnecessary.

### 2f. Remove BAML from the conductor harness

The BAML layer adds an extra planning call before every agent action, creating latency
and the impedance mismatch that caused the blank document bug. Replace with direct
Anthropic API calls. The conductor and writer can use the same model call format as
cogent's native adapter.

**QA gate**: Submit prompt requiring research → see Worker progress in marginalia →
see content appear as Writer incorporates findings → final document has citations.

## Phase 3: Make It Good (weeks)

### 3a. Simplify the Writer actor

view.rs is 1642 lines with 25+ state signals. Refactor into:
- Core editor state (content, cursor, selection)
- Version management (version list, navigation)
- Marginalia (left: writer state + worker progress, right: user notes)
- Toolbar (open, save, prompt)

### 3b. Fix prose_interop.js

281 lines of hand-written vanilla JS for HTML↔Markdown in contenteditable. No editor
library. Options: ProseMirror (battle-tested, collaborative-ready) or keep simple
contenteditable but with a proper bridge (not string interpolation into eval).

Pretext (Cheng Lou/Midjourney): pure TS text measurement, ~500x faster than DOM
reflow. NOT an editor — useful for annotation bubble positioning alongside ProseMirror.

### 3c. VM boot time

Snapshots not loading on idle user login causes ~10s boot. Should be ~1-2s.
Investigate: stale virtiofsd sockets, TAP device state, data.img symlink validity.
`hypervisor/src/sandbox/systemd.rs:409-510` handles snapshot creation.

**QA gate**: Time-to-first-content < 3s. Editor feels responsive. Worker progress
is visible and meaningful (not just "processing...").

## Phase 4: Make It Great (the rewrite)

The rewrite isn't "Writer in Svelte with ProseMirror." It's a new rendering paradigm.

### The Vision: Text as Interface

The entire desktop is a spatial canvas of text, images, and video — floating,
animating, fluid forms that transform cogently. Text morphs/deforms into UI elements.
A document is text. A terminal is text. A trace is text. Settings are text.
They're all the same primitive, programmatically arranged on screen.

This paradigms the Writer app-agent while still enabling desktop multitasking,
which is essential for text-heavy processing. Not windowed apps competing for
screen real estate, but a fluid spatial arrangement where content flows between
contexts.

Not just text either. Text, images, video as floating animating forms. The
Writer's document, the Researcher's citations, the Terminal's output — all
rendered as programmable content flows on a WebGPU canvas, transforming between
states rather than living in separate window frames.

### Why this works technically

**Pretext** (Cheng Lou/Midjourney): Pure TS text measurement, ~500x faster than
DOM reflow. If you can measure and position any text at that speed, you can treat
text as a freely arrangeable primitive — not something stuffed into a `<div>`.
ProseMirror becomes irrelevant when the entire rendering surface is your editor.

**WebGPU**: GPU-accelerated rendering of text, images, video. No DOM, no CSS layout,
no browser reflow. You control every pixel. Text deformation, fluid animation,
spatial arrangement — all possible at 60fps when you own the render pipeline.

**Svelte**: Thin runtime, compiles to vanilla JS. The orchestration layer between
WebGPU canvas and the sandbox API. No virtual DOM overhead competing with your
custom renderer.

**ghostty-web** (Coder/Ghostty): GPU-accelerated terminal renderer. If the terminal
is just another text flow on the canvas, ghostty-web provides the terminal emulation
while your renderer handles the spatial positioning.

### 4a. Svelte + Pretext + WebGPU prototype

Build the canvas renderer. Text flows that can be positioned, scaled, deformed.
Pretext for measurement, WebGPU for rendering. Start with the Writer document
as the first text flow — it's the most important surface.

### 4b. Terminal as text flow

Replace xterm.js with ghostty-web, rendered as a text flow on the canvas.
Not a window — a spatial region that the user can resize, move, fade, overlap
with document text.

### 4c. Multi-flow desktop

Documents, terminals, traces, settings — all text flows on one canvas.
Multitasking is spatial arrangement, not window management. The user arranges
content flows by dragging, the system arranges them by context (Writer
foregrounds its document, Researcher foregrounds citations).

### 4d. Drop Dioxus entirely

Once the canvas renders everything, there's no WebView needed. The "app" is
a browser tab (or Tauri shell) loading the Svelte+WebGPU canvas. The sandbox
serves static assets + WebSocket API. That's it.

**QA gate**: Canvas Writer passes all Phase 1-3 tests. Performance benchmarks.
Side-by-side comparison before cutover. The old Writer stays running until the
new canvas is proven.

## Architecture Notes

### The Conductor → Writer → Worker hierarchy
```
Conductor (global policy, non-blocking orchestration)
  └── Writer (app agent, living document authority, SINGLE WRITER)
        ├── Researcher (findings, citations, questions → messages to Writer)
        └── Terminal (code execution, file ops → messages to Writer)
              ↕ (workers can message each other via channels)
```

### Current communication paths (broken)
```
Worker ──canon_append──→ Document (bypasses Writer!)
Worker ──proposal_append──→ Overlay (bypasses Writer!)
Worker ──completion──→ Writer ──auto-rewrite──→ Document (workaround)
```

### Target communication paths (clean)
```
Worker ──message──→ Writer ──write_revision──→ Document
Worker ←──steer───── Writer (mid-flight reprompt)
Worker ──message──→ Worker (peer channels)
```

### Frontend architecture (current)
```
Dioxus (Rust) → js_sys::eval() → prose_interop.js → contenteditable DOM
```

### Frontend architecture (target)
```
Browser/Tauri → http://localhost:PORT → Svelte orchestration
                                          ├── WebGPU canvas (all rendering)
                                          ├── Pretext (text measurement, ~500x DOM)
                                          ├── ghostty-web (terminal emulation)
                                          └── WebSocket to sandbox API

Everything is a text/media flow on one canvas.
No DOM layout. No windows. Spatial arrangement.
```

## Key Files

### Backend (sandbox)
- `sandbox/src/actors/writer/adapter.rs:37-308` — WriterDelegationAdapter (Phase 1 bug)
- `sandbox/src/actors/writer/adapter.rs:318-652` — WriterUserPromptAdapter (working ref)
- `sandbox/src/actors/writer/mod.rs:334-494` — dispatch_delegate_capability (fire-and-forget)
- `sandbox/src/actors/writer/mod.rs:723` — AUTO_ACCEPT_WORKER_DIFFS flag
- `sandbox/src/actors/writer/mod.rs:2064-2243` — handle_delegation_worker_completed
- `sandbox/src/actors/writer/mod.rs:2199-2242` — auto-rewrite workaround (bug 4+5)
- `sandbox/src/actors/writer/mod.rs:1366-1455` — create_writer_document_version
- `sandbox/src/actors/writer/document_runtime/` — Version persistence
- `sandbox/src/actors/writer/document_runtime/state.rs` — RunDocument, Overlay structs
- `sandbox/src/actors/agent_harness/mod.rs:950-1146` — Harness loop
- `sandbox/src/actors/terminal.rs:313-359` — canon_append (remove in Phase 2)
- `sandbox/src/actors/terminal.rs:361-406` — proposal_append (remove in Phase 2)
- `sandbox/src/actors/researcher/mod.rs:150-171` — ResearcherResult contract
- `sandbox/src/actors/conductor/runtime/conductor_adapter.rs:179-203` — Routing decision

### Frontend (Dioxus desktop)
- `dioxus-desktop/src/components/writer/view.rs` — 1642 lines, 25+ signals
- `dioxus-desktop/src/components/writer/prose_interop.js` — 281 lines, contenteditable
- `dioxus-desktop/src/components/writer/logic.rs` — Writer business logic
- `dioxus-desktop/src/components/writer/types.rs` — Writer types
- `dioxus-desktop/src/components/writer/styles.rs` — CSS

### Hypervisor
- `hypervisor/src/sandbox/systemd.rs:409-510` — Snapshot/restore (Phase 3c)

## Ship Strategy

Phase 1 unblocks demo users. Fix the blank document, stream versions, hide terminal.
Deploy, QA, get feedback from the 5 people who want more.

Phase 2 fixes the architecture. Clean worker contracts, enable steering, remove
workarounds. Each change is a deploy/QA cycle. This is where "Writer is single writer"
becomes real in the code.

Phase 3 polishes. Simpler frontend, better editor, faster boot.

Phase 4 is the rewrite. Developed independently, swapped in when proven. The old
Writer stays running until the new one passes all tests. Replace the engine while flying.
