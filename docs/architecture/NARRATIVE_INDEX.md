# ChoirOS Narrative Index (Read This First)

Date: 2026-02-11
Purpose: Human-readable map of the architecture docs, in plain language.

## 60-Second Story

ChoirOS is shifting from parallel feature work to a linear, testable roadmap.
The current top deliverable is the `Directives` app: a first-class planning/control view.
Core architecture rule: direct actor-to-actor calls stay fast; orchestration control is typed and policy-driven.
Researcher baseline is now live via delegated `web_search`, and run-level logs now include provider/citation trails.
Current correction: Conductor cutover to typed agentic orchestration is the baseline, with explicit BAML contracts for Conductor policy and Watcher log review.
Current checkpoint: runtime and UX are run-centric with token-lane separation (Watcher/UI handle routine event traffic; Conductor wakes on high-value control moments).
Current reset priority: simplify runtime authority, enforce headless verification, and make live run observability trustworthy.

## What We Are Building Right Now

1. Directives as the primary operator surface (app/window, not always-on).
2. Capability boundaries:
   - Chat can call `bash` interface but shell execution is delegated to TerminalActor.
   - PromptBar orchestrates actors and writes memos; it does not call tools.
3. Deterministic, reproducible operation:
   - model/config decisions are logged
   - events are the system of record
4. Temporal awareness by default:
   - prompt system context and prompt messages carry explicit UTC timestamps for model grounding
5. Orchestration correction lane:
   - Conductor and Watcher BAML contracts are now required for adaptive multi-step routing.
   - Conductor delegates natural-language objectives to capability agents.
   - Watcher runs LLM-driven event-log review on lower-power models than Conductor.
   - Deterministic `Terminal -> Researcher` workflow authority is removed as control-plane authority.
6. Run narrative checkpoint:
   - Users see semantic run progress and accumulated natural-language summaries by default.
   - Raw tool calls remain available as drill-down, not primary UX.
   - Conductor reads run description + typed state on wake.
7. Immediate execution lane:
   - `03.5.1`: policy cutover gate (Conductor + Watcher BAML, no deterministic fallback authority).
   - `03.5.2`: concurrent multi-worker run orchestration + run narrative checkpoint before efficiency tuning.

## Read Order (High-Level to Deep Dive)

1. `/Users/wiz/choiros-rs/docs/architecture/2026-02-11-agentic-loop-simplification-observability-research-program.md`
   - Reset program: simplify control authority, require headless Prompt Bar verification, and enforce run-level observability gates before feature expansion.
2. `/Users/wiz/choiros-rs/docs/architecture/2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md`
   - Consolidated runtime baseline for concurrent orchestration, semantic run UX, token-lane separation, and `03.5.1 -> 03.5.2` gate.
3. `/Users/wiz/choiros-rs/docs/architecture/2026-02-10-conductor-watcher-baml-cutover.md`
   - Root-cause and cutover plan that established Conductor+Watcher BAML contracts and removed deterministic control authority.
4. `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
   - Authoritative linear checklist and phase gates.
5. `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
   - Product-level execution checklist and boundaries for Directives + policy pattern.
6. `/Users/wiz/choiros-rs/roadmap_progress.md`
   - What has already landed and what is next.
7. `/Users/wiz/choiros-rs/docs/architecture/worker-signal-contract.md`
   - Control-plane vs observability contract, typed turn reports, anti-spam rules.
8. `/Users/wiz/choiros-rs/docs/architecture/researcher-search-dual-interface-runbook.md`
   - Canonical researcher rollout spec: dual interface, provider isolation, and observability contracts.
9. `/Users/wiz/choiros-rs/docs/architecture/model-provider-agnostic-runbook.md`
   - Model/provider matrix and validation plan.
10. `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
   - Canonical policy for backend-synced app/window state (no browser-local authority).
11. `/Users/wiz/choiros-rs/docs/architecture/pdf-app-implementation-guide.md`
   - Deferred guide-only milestone (no build yet).
12. `/Users/wiz/choiros-rs/docs/architecture/roadmap-critical-analysis.md`
   - Historical gap analysis and risks (use as reference, not current ordering authority).

## Current Decisions (Explicit)

- Roadmap is linear, not parallel by default.
- One active milestone at a time; pass gate before moving on.
- Directives app is prioritized over PDF app implementation.
- Policy actor is not first-line routing.
- Conductor agentic policy cutover is immediate and mandatory for multi-step orchestration.
- Run narrative + semantic events are first-class UX and conductor wake context.
- Backend is canonical for app/window UI state; browser localStorage is non-authoritative.

## One-Line Summary Per Core Doc

- `2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md`: "Concurrent runtime baseline: semantic run UX by default, token-lane separation, and conductor wake context contract."
- `2026-02-11-agentic-loop-simplification-observability-research-program.md`: "Reset program for runtime simplification, mandatory headless Prompt Bar verification, and live run observability gates."
- `2026-02-10-conductor-watcher-baml-cutover.md`: "Why deterministic orchestration failed, and how we cut over Conductor+Watcher to typed BAML policy loops."
- `roadmap-dependency-tree.md`: "What order we execute in, and why."
- `directives-execution-checklist.md`: "What must be true before we call this architecture real."
- `roadmap_progress.md`: "What changed and what we tackle next."
- `model-provider-agnostic-runbook.md`: "How to prove model routing and provider support."
- `researcher-search-dual-interface-runbook.md`: "How researcher launches without breaking capability boundaries."
- `worker-signal-contract.md`: "How workers decide what to signal, without spamming."
- `pdf-app-implementation-guide.md`: "What PDF should become later, without building it now."
- `roadmap-critical-analysis.md`: "Where the earlier plan overestimated readiness."
- `backend-authoritative-ui-state-pattern.md`: "How app/window state stays synced across browsers without localStorage authority."

## Doc Readability Rule (Human-First)

For major architecture docs, include a top section:
- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`

If a doc is long, it is not done until this summary exists.
