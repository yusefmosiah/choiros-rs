# ChoirOS Roadmap (Execution Lane)

Date: 2026-02-14  
Status: Authoritative immediate order

## Narrative Summary (1-minute read)

ChoirOS is now executing one lane only: `Prompt Bar -> Conductor` with model-led orchestration.
Conductor is the control-plane core and communicates through actor messages with workers and app agents.
Natural-language objectives remain first-class, but deterministic orchestration code should be removed
where model-managed control flow is expected.
Deterministic logic stays only in safety/operability rails (routing, auth, budgets, cancellation,
idempotency, loop prevention, and trace persistence).
Watcher/Wake are de-scoped from normal run progression authority.

## What Changed

- Roadmap ordering remains explicitly single-lane, not parallel-track.
- Control authority is now explicit: model-led planning with deterministic safety rails.
- Watcher/Wake are removed from normal orchestration progression.
- Direct request path is now `Worker/App Agent -> Conductor` through typed actor messages.
- Writer harness completion and tracing rollout are prioritized next.
- Tracing rollout sequence is fixed: human UX first, then headless API, then app-agent harness.
- Conductor now treats app/workers as logical subagents with non-blocking, no-poll turn invariants.
- Harness simplification direction is explicit: one while-loop runtime model and narrower worker execution boundary (`worker_port`).
- Capability ownership is explicit: Conductor executes no tools directly; tool schemas are shared once and granted per agent/worker.
- Terminal and Researcher include file tools (`file_read`, `file_write`, `file_edit`) as baseline worker capability.

## What To Do Next

**Immediate Next Execution Lane: Prompt Bar -> Conductor**

1. **Conductor backend MVP for report generation**
   - ConductorActor with capability dispatch to actors
   - Markdown report generation endpoint  
   - Integration with Files/Writer for output delivery
   - Model-led control flow on typed safety rails

2. **Prompt bar routing to Conductor**
   - Prompt Bar captures universal input
   - Routes living-document intent to Conductor for multi-step planning
   - Living-document UX is the canonical human interface

3. **Writer auto-open in markdown preview**
   - Conductor-generated reports auto-open in Writer
   - Writer launches in preview mode for .md files
   - Backend-driven UI state per `backend-authoritative-ui-state-pattern.md`

4. **Writer harness ownership cutover**
   - Writer app-agent harness owns canonical living-document/revision mutation flow
   - Remove active Conductor direct tool execution paths
   - Add boundary tests that fail on direct Conductor tool execution

5. **Direct request-message contract and tests**
   - Typed app/worker-to-conductor `request` envelopes
   - Minimal request kinds + correlation metadata
   - Ordered websocket assertions for scoped request streams

6. **Tracing staged rollout**
   - Phase 1: human-first tracing UX and navigation
   - Phase 2: headless tracing API for agent consumption
   - Phase 3: tracing app-agent harness (after API stability)

7. **Conductor wake-context hardening**
   - Include bounded system agent-tree snapshot on every wake
   - Assert conductor turns are finite and non-blocking
   - Prove no child polling loops exist in orchestration runtime
   - Implement `docs/architecture/2026-02-14-agent-tree-snapshot-contract.md`

8. **Harness loop + boundary simplification**
   - Treat harness runtime as one while loop with typed actions
   - Narrow `adapter` concept to execution-focused `worker_port`
   - Avoid new phase abstractions unless evidence requires them

**Architecture Principle**: **Living document is the human interface; Conductor is orchestrator.**

**Background work (ongoing but not primary lane)**:
- Harden Researcher v1 (provider quality, anti-spam, websocket tests)
- Harden worker live-update event model runtime enforcement
- Shared harness unification (living-document/terminal/researcher loop abstraction)

## Progress Review (2026-02-18)

Handoff checkpoint from seam-closure work:

- Writer delegation tool contract is now enforced at runtime:
  - Writer delegation allows `message_writer` and `finished` only.
  - Writer synthesis allows `finished` only.
  - Disallowed tool decisions are rejected and retried once with explicit tool-contract correction.
- Worker lifecycle bug fixed: delegated worker inflight entries are now removed on completion, not immediately after dispatch.
- End-to-end evidence captured with Playwright for prompt -> conductor -> writer -> researcher path.
- Playwright artifacts are now fully gitignored (`tests/artifacts/playwright/`, `playwright-report/`, `test-results/` and local test runner output paths).

Current gate status:
- Phase 0 remains open.
- Core direction is validated (non-blocking message-based delegation and writer-owned worker dispatch).
- Remaining work is focused on final seam closure and stronger multi-run concurrency verification.

Immediate next checks:
1. Add assertions for writer window/run isolation with concurrent runs.
2. Add websocket/event-order tests for delegated worker completion messages to writer.
3. Add negative tests for disallowed writer delegation tools to prevent regression.

## RLM + StateIndex Alignment (Added)

Goal:
- Move from per-actor ad-hoc memory loops to bounded frame/context execution that can scale to Prompt Bar + Conductor routing.

**Model-Led Control Flow Policy**: remove deterministic orchestration where model planning should lead. Keep deterministic rails for safety and operability only. See AGENTS.md for the authoritative policy.

Checklist:
- [ ] Add `StateIndexActor` scaffold (`FrameId`, frame stack projection, token budget structs).
- [ ] Add `ContextPack` assembly boundaries for living-document/research follow-up loops.
- [ ] Add frame-aware resume hooks for deferred background completions.
- [ ] Keep EventStore as source-of-truth; StateIndex remains projection/cache.
- [ ] Add integration test proving `deferred -> completion signal -> resumed final answer` with no stale status text.

Gate:
- Capability loops remain non-blocking, objective-complete, and bounded under repeat delegated workflows.

## Single Active Lane

**Current execution lane: Prompt Bar -> Conductor orchestration**

Previous foundations now operational:
- ✅ Logging (complete - available for use)
- ✅ Watcher (available as optional recurring-event detection)  
- ✅ Model Policy (complete - available for use)
- ✅ Worker live-update event model (baseline complete)
- ✅ Researcher (baseline live - delegated web_search active)

Active milestones:
1. **Milestone A**: Conductor backend MVP for report generation
2. **Milestone B**: Prompt bar routing to Conductor
3. **Milestone C**: Writer auto-open in markdown preview mode

Completed prerequisite (Phase 0, seam 9):
- **libsql → sqlx migration** completed on 2026-02-18 before Phase 6
  (Nix/cross-compilation).
  Tracked in `docs/architecture/2026-02-17-codesign-runbook.md` as seam 0.9.
  Unlocked: crane builds, `nix build .#sandbox`, SQLX_OFFLINE CI mode, `RETURNING`
  clause, and proper `sqlx migrate run` (replacing manual `PRAGMA table_info`
  workarounds).

## Core Architecture Principles

- **Model-Led Control Flow**: Multi-step orchestration is model-managed by default; deterministic logic is limited to safety/operability rails.
- **Typed Control Metadata**: Actor messages carry typed routing/request authority; natural language carries objective context.
- **Non-Blocking Subagent Pillar**: Conductor treats workers/apps as logical subagents but never polls or blocks on child completion.
- **Living document is the human interface; Conductor is orchestrator**: Human intent enters through living-document UX and is handed off to Conductor.

## Reconciliation Gate (Blocking Before Researcher)

Historical note:
- Sections below this point are preserved implementation history from earlier lanes.
- Current authoritative direction is the 2026-02-14 doc set listed in `docs/architecture/NARRATIVE_INDEX.md`.

Source:
- `docs/architecture/2026-02-08-architecture-reconciliation-review.md`

Locked decision:
- Messaging model **Option B** is authoritative:
  - `uActor -> Actor`: secure delegation envelope
  - `AppActor -> ToolActor`: typed tool contracts

Blocking checklist:
- [x] Remove `ChatAgent` direct tool execution path (`ToolRegistry` bypass).
- [x] Ensure all app-level bash execution is delegated through `TerminalActor`.
- [~] Remove/retire ambiguous dual app-tool contract path on terminal calls.
- [x] Verify watcher/log views render committed event stream output under active task traffic.

Note:
- C3 is now mostly a contract-clarity cleanup item. Runtime app delegation path is stable and typed.

Gate tests:
- [x] `cargo test -p sandbox --test websocket_chat_test test_websocket_streams_actor_call_for_delegated_terminal_task -- --nocapture`
- [x] `cargo test -p sandbox --test logs_ws_test -- --nocapture`
- [x] `cargo test -p sandbox --lib actors::watcher::tests:: -- --nocapture`
- [x] capability-boundary test for no direct tool execution from chat (`sandbox/tests/capability_boundary_test.rs`).

## Milestone 2.5: Model Policy (Blocking Before Researcher)

Goal:
- Make model routing/policy an explicit, inspectable runtime system before adding Researcher.

Checklist:
- [x] Add policy-aware model resolution hooks for `chat` and `terminal` roles.
- [x] Ensure delegated worker events persist `model_requested` and `model_used`.
- [x] Ensure logs view includes `model.*` and `chat.*` events, not only watcher/worker rows.
- [x] Add persisted model policy source file and Settings surface integration.
- [x] Add tests for policy allow-list and override denial behavior.
- [x] Set role defaults: chat=`ClaudeBedrockSonnet45`, conductor=`ClaudeBedrockOpus46`, summarizer=`ZaiGLM47Flash`.
- [x] Remove `ClaudeBedrockOpus45` from active runtime config.
- [x] Keep available-model catalog separate from editable policy document in Settings.

Gate:
- Policy decisions are traceable in EventStore and visible in logs, with deterministic fallbacks.

## Operating Constraints

- One active milestone at a time.
- Every milestone updates `roadmap_progress.md` with `Now / Next / Later / Blocked`.
- No speculative feature expansion during active milestone execution.
- Scope discipline: app agents use typed tool contracts; universal actors use secure delegation envelopes.

## Milestone 1: Logging (In Progress)

Goal:
- Make observability first-class and queryable in libSQL for high-concurrency agent runs.

Checklist:
- [x] Architecture gate: ADR approved for EventStore/EventBus reconciliation.
- [x] Add log query API for recent event inspection (`/logs/events` filters + limit).
- [x] Extend event envelope usage for traceable fields in payload (`trace_id`, `span_id`, `interface_kind`, `task_id`).
- [x] Add durable indexes for log-heavy access patterns (`seq`, `event_type`, `actor_id`, scope keys).
- [x] Ensure worker/model metadata is persisted on delegated paths (`model_used`, status, correlation).
- [x] Add committed-event relay path (`EventStore -> EventBus`) with delivery-only bus publish.
- [x] Add JSONL export path for eval portability (secondary sink, DB remains canonical).
- [x] Add integration tests for filtered append/query and scoped retrieval safety.

Gate:
- ADR approved and logs can be queried by scope + type + actor + recency with stable performance and deterministic replay slices.

## Milestone 2: Watcher (Next)

Goal:
- Add deterministic monitoring that processes event streams and emits actionable alerts/signals.

Checklist:
- [x] Implement WatcherActor subscription/read-loop over event log.
- [x] Add first deterministic rules: timeout spikes, repeated worker failures, retry storms, missing completions.
- [x] Rule `worker_failure_spike` (windowed failure count) implemented.
- [x] Rule `worker_timeout_spike` (timeout-like failure count) implemented.
- [x] Rule `worker_network_spike` (network-like failure count) implemented.
- [x] Rule `worker_retry_storm` (retry-like progress burst) implemented.
- [x] Rule `worker_stalled_task` (started without completion/failure past threshold) implemented.
- [x] Emit `watcher.alert.*` events and persist in EventStore.
- [x] Add suppression/dedup windows to avoid alert floods.
- [x] Add UI stream integration for watcher output in logs window.
- [x] Add backend log stream transport: `/ws/logs/events` (filterable live EventStore stream).

Gate:
- Watcher detects and emits stable alerts on synthetic failure scenarios without noisy false storms.

## Milestone 3: Researcher (In Progress)

Goal:
- Ship ResearcherActor with full observability so research work is inspectable in real time.

Checklist:
- [x] Implement typed worker turn report ingestion contract (`finding`, `learning`, `escalation`, `artifact`) before researcher rollout.
- [~] Add runtime anti-spam gates (caps, confidence floor, dedup hash, escalation cooldown).
- [~] Persist accepted/rejected signal events with rejection reasons.
- [x] Implement ResearcherActor with constrained capability surface.
- [x] Route chat `web_search` through ResearcherActor only (no terminal-side web search tool).
- [x] Implement provider adapters for Tavily + Brave + Exa under researcher capability boundary.
- [x] Stream lifecycle events (`planning`, `search`, `read`, `synthesis`, `citation_attach`).
- [x] Persist citations and source metadata in event payloads.
- [x] Emit objective completion metadata from researcher (`complete|incomplete|blocked`) with recommended next capability.
- [x] Add policy-driven `research -> terminal` escalation hook in supervisor for incomplete/blocked research outcomes.
- [~] Add websocket tests for ordered researcher event streaming.

Gate:
- Research flow is fully observable, replayable, and inspectable by watcher/log UI.

## Deferred (Explicit)

- PromptBar orchestration depth beyond what is needed for logging/watcher/researcher integration.
- Policy actor hardening beyond deterministic local rules.
- PDF app implementation (guide stays deferred).
- Mind-map / concept-map UI beyond initial log-centric foundations.

## References

- `docs/architecture/logging-watcher-architecture-design.md`
- `docs/architecture/2026-02-14-worker-live-update-event-model.md`
- `docs/architecture/roadmap-critical-analysis.md`
- `roadmap_progress.md`
