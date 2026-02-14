# ChoirOS Narrative Index (Read This First)

Date: 2026-02-14
Purpose: Human-readable map of the architecture docs, in plain language.

## 60-Second Story

ChoirOS is shifting from parallel feature work to a linear, testable roadmap.
The current top deliverable is the `Directives` app: a first-class planning/control view.
Core architecture rule: Conductor is the control-plane core, driven by actor messages from humans and agents.
Messages can carry natural-language objectives, but orchestration authority is model-led and bounded by typed rails.
Human AI interaction is living-document-first (no standalone chat app).
Domain direction reflects this: `choir-ip.com` emphasizes enduring outputs over ephemeral chat modality.
Current correction: remove deterministic workflow authority for normal multi-step orchestration.
Current checkpoint: workers/app agents send typed `request` messages directly to Conductor; Watcher/Wake are de-scoped from normal progression.
Conductor treats workers/apps as logical subagents, but turns are non-blocking and never poll child agents.
Current reset priority: simplify runtime authority, enforce headless verification, and make live run observability trustworthy.
Immediate app pattern: human UX first, then headless API, then app-agent harness. Tracing follows this sequence next.

## What We Are Building Right Now

1. Directives as the primary operator surface (app/window, not always-on).
2. Capability boundaries:
   - PromptBar/living-document surfaces orchestrate through Conductor; they do not call tools directly.
   - Shell execution remains delegated to TerminalActor.
3. Deterministic, reproducible operation on safety rails:
   - model/config decisions are logged
   - events are the system of record
4. Model-led control flow:
   - model plans decomposition and delegation for multi-step orchestration
   - deterministic logic is limited to routing, auth, budgets, cancellation, dedupe, and loop prevention
5. Temporal awareness by default:
   - prompt system context and prompt messages carry explicit UTC timestamps for model grounding
6. Orchestration correction lane:
   - direct request path is `Worker/App Agent -> Conductor` via typed actor envelopes.
   - Watcher is optional recurring-event detection only, not run-step authority.
   - Conductor does not call tools directly; capability agents/workers execute.
7. Run narrative checkpoint:
   - Users see semantic run progress and accumulated natural-language summaries by default.
   - Raw tool calls remain available as drill-down, not primary UX.
   - Conductor reads run description + typed state on wake.
8. Immediate execution lane:
    - finalize typed request-message contract for direct app/worker-to-conductor routing.
    - complete Writer app-agent harness hardening.
    - ship Tracing in order: human UX -> headless API -> app-agent harness.
    - harden conductor wake context with bounded system agent-tree snapshots.

## Read Order (High-Level to Deep Dive)

1. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-living-document-human-interface-pillar.md`
   - Human interface pillar: living-document UX is the primary human interaction model and feeds conductor orchestration.
2. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-conductor-non-blocking-subagent-pillar.md`
   - Key pillar: logical subagents over actor messaging, no polling, no blocking, bounded agent-tree wake context.
3. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-agent-tree-snapshot-contract.md`
   - Typed wake-context contract: bounded node digest, deterministic truncation, freshness semantics, and observability events.
4. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-conductor-request-message-v0.md`
   - Streamlined control primitive: `request` message v0 instead of an escalation subsystem.
5. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-conductor-model-led-control-plane-next-steps.md`
   - Direction update: model-led control flow, direct conductor requests, watcher/wake de-scope, and tracing rollout order.
6. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-operational-concepts-pruning-catalog.md`
   - Keep/simplify/remove catalog for operational concepts to reduce abstraction sprawl.
7. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-three-level-hierarchy-runtime.md`
   - Canonical end-state runtime: Conductor -> App Agents -> Workers with concurrent execution and bounded conductor context.
8. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-capability-ownership-matrix.md`
   - Canonical capability boundary: Conductor orchestrates only; tool schemas are shared once and granted per agent/worker.
9. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-harness-loop-worker-port-simplification.md`
   - Harness simplification: one while loop runtime model and `adapter -> worker_port` naming/contract reduction.
10. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-minimal-kernel-app-runtime-spec.md`
   - Authoritative simplification spec: minimal kernel state, generic app interface, shared worker model, and revision-first canon authority.
11. `/Users/wiz/choiros-rs/docs/architecture/2026-02-11-agentic-loop-simplification-observability-research-program.md`
   - Reset program: simplify control authority, require headless Prompt Bar verification, and enforce run-level observability gates before feature expansion.
12. `/Users/wiz/choiros-rs/docs/architecture/2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md`
   - Consolidated runtime baseline for concurrent orchestration, semantic run UX, token-lane separation, and `03.5.1 -> 03.5.2` gate.
13. `/Users/wiz/choiros-rs/docs/architecture/2026-02-10-conductor-watcher-baml-cutover.md`
   - Historical cutover context for prior Conductor+Watcher policy loop design.
14. `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
   - Authoritative linear checklist and phase gates.
15. `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
   - Product-level execution checklist and boundaries for Directives + policy pattern.
16. `/Users/wiz/choiros-rs/roadmap_progress.md`
   - What has already landed and what is next.
17. `/Users/wiz/choiros-rs/docs/architecture/2026-02-14-worker-live-update-event-model.md`
   - Canonical worker behavior: `progress/result/failed/request` plus live document updates.
18. `/Users/wiz/choiros-rs/docs/architecture/researcher-search-dual-interface-runbook.md`
   - Canonical researcher rollout spec: dual interface, provider isolation, and observability contracts.
19. `/Users/wiz/choiros-rs/docs/architecture/model-provider-agnostic-runbook.md`
   - Model/provider matrix and validation plan.
20. `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
   - Canonical policy for backend-synced app/window state (no browser-local authority).
21. `/Users/wiz/choiros-rs/docs/architecture/pdf-app-implementation-guide.md`
   - Deferred guide-only milestone (no build yet).
22. `/Users/wiz/choiros-rs/docs/architecture/roadmap-critical-analysis.md`
   - Historical gap analysis and risks (use as reference, not current ordering authority).

## Current Decisions (Explicit)

- Roadmap is linear, not parallel by default.
- One active milestone at a time; pass gate before moving on.
- Directives app is prioritized over PDF app implementation.
- Human interaction is living-document-first; chat app is removed.
- Policy actor is not first-line routing.
- Conductor is the orchestration authority for multi-step control via actor messaging.
- Runtime hierarchy is three-level: Conductor -> App Agents -> Workers.
- Conductor has no direct tool execution path; tool schemas are shared once with per-agent grants.
- Terminal and Researcher include `file_read`, `file_write`, and `file_edit` as baseline worker tools.
- Writer app agent is canonical for living-document/revision mutation authority.
- Model-led planning is default; deterministic logic is for safety/operability rails only.
- Conductor turns are non-blocking and never poll child agents.
- Conductor wake context includes bounded system agent-tree state.
- Watcher/Wake are not normal run progression authority.
- Run narrative + semantic events are first-class UX and conductor wake context.
- Backend is canonical for app/window UI state; browser localStorage is non-authoritative.

## One-Line Summary Per Core Doc

- `2026-02-14-living-document-human-interface-pillar.md`: "Human interaction runs through living documents first; conductor remains orchestration authority behind the interface."
- `2026-02-14-conductor-non-blocking-subagent-pillar.md`: "Conductor treats workers/apps as logical subagents via actor messaging with no polling, no blocking, and bounded agent-tree wake context."
- `2026-02-14-agent-tree-snapshot-contract.md`: "Typed wake context contract for conductor: bounded agent-tree digest with deterministic truncation and freshness markers."
- `2026-02-14-conductor-request-message-v0.md`: "Simplify control asks to one typed `request` message instead of introducing an escalation subsystem."
- `2026-02-14-conductor-model-led-control-plane-next-steps.md`: "Direction reset for model-led control flow, direct conductor request messages, watcher/wake de-scope, and tracing rollout order."
- `2026-02-14-operational-concepts-pruning-catalog.md`: "Keep/simplify/remove operational concepts to maintain a lean control-plane vocabulary."
- `2026-02-14-three-level-hierarchy-runtime.md`: "Canonical end-state structure: Conductor coordinates app agents, app agents run interactive sessions, workers provide concurrent execution."
- `2026-02-14-capability-ownership-matrix.md`: "Capability authority map: Conductor orchestrates only, tool schemas are single-source shared contracts, and Writer remains canonical for document/revision mutation."
- `2026-02-14-harness-loop-worker-port-simplification.md`: "Reduce harness complexity to one while loop and simplify `adapter` to an execution-focused `worker_port` boundary."
- `2026-02-14-minimal-kernel-app-runtime-spec.md`: "Kernel/app split with minimal state: obligations, leases, patches, revisions, and app-driven replanning through typed actions."
- `2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md`: "Concurrent runtime baseline: semantic run UX by default, token-lane separation, and conductor wake context contract."
- `2026-02-11-agentic-loop-simplification-observability-research-program.md`: "Reset program for runtime simplification, mandatory headless Prompt Bar verification, and live run observability gates."
- `2026-02-10-conductor-watcher-baml-cutover.md`: "Historical reference for the previous Conductor+Watcher policy-loop approach."
- `roadmap-dependency-tree.md`: "What order we execute in, and why."
- `directives-execution-checklist.md`: "What must be true before we call this architecture real."
- `roadmap_progress.md`: "What changed and what we tackle next."
- `model-provider-agnostic-runbook.md`: "How to prove model routing and provider support."
- `researcher-search-dual-interface-runbook.md`: "How researcher launches without breaking capability boundaries."
- `2026-02-14-worker-live-update-event-model.md`: "Workers do work and stream simple typed events; app agents/Writer apply live document updates."
- `pdf-app-implementation-guide.md`: "What PDF should become later, without building it now."
- `roadmap-critical-analysis.md`: "Where the earlier plan overestimated readiness."
- `backend-authoritative-ui-state-pattern.md`: "How app/window state stays synced across browsers without localStorage authority."

## Doc Readability Rule (Human-First)

For major architecture docs, include a top section:
- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`

If a doc is long, it is not done until this summary exists.
