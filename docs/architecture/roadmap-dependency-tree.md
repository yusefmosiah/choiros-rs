# ChoirOS Roadmap (Execution Lane)

Date: 2026-02-08  
Status: Authoritative immediate order

## Narrative Summary (1-minute read)

ChoirOS is now executing one lane only: `Logging -> Watcher -> Model Policy -> Worker Signal Contract -> Researcher`.
The goal is to finish observability foundations before expanding behavior.
EventStore is canonical; EventBus is delivery-only; watcher/researcher must emit rich, queryable events.
Run-level observability is now in place: persisted run indexing, watcher run navigation, run markdown projection, and structured worker failure telemetry with model attribution on worker lifecycle events.
Researcher baseline is now live through delegated `web_search` runs, with provider-level run events and citation payloads persisted in EventStore.

## What Changed

- Roadmap ordering is now explicitly single-lane, not parallel-track.
- Logging baseline is mostly complete (filters, APIs, relay, tests).
- Watcher baseline moved from prototype to deterministic multi-rule coverage.
- A dedicated backend live logs stream (`/ws/logs/events`) is now in scope as part of watcher observability.
- Architecture reconciliation review added a **blocking pre-Researcher gate** for capability boundaries and messaging contracts.
- Added docs gate for worker typed turn signaling, including anti-spam controls and conductor escalation contract.
- Added run-centric watcher UI foundations:
  - preload persisted events on load,
  - runs sidebar grouping by correlation/task,
  - run markdown projection path from watcher.
- Added structured worker failure monitoring fields:
  - `failure_kind`, `failure_retriable`, `failure_hint`, `failure_origin`, `error_code`, `duration_ms`.
- Added watcher network reliability rule:
  - `watcher.alert.network_spike`.
- Worker lifecycle model attribution normalized:
  - every worker lifecycle event now carries `model_requested` and `model_used`.
- Researcher baseline landed:
  - Chat `web_search` delegates through `ResearcherActor`,
  - provider call/result/error lifecycle is emitted and queryable per run,
  - citations/provider metadata are persisted into completed task payloads and run markdown.
- Prompt temporal-awareness hardening landed:
  - system prompts now include UTC timestamp metadata,
  - per-message prompt content in chat/terminal planning paths is timestamped.

## What To Do Next

- Close reconciliation gate:
  - remove direct ChatAgent tool execution path
  - enforce AppActor->ToolActor typed delegation boundary
  - fix logs/watcher visibility gaps end-to-end
- Close worker signal contract gate:
  - typed turn report envelope for worker outputs
  - control-plane escalation vs observability event split
  - anti-spam validation/dedup/cooldown semantics
- Harden Researcher v1 now that baseline is live:
  - validate Brave/Exa in live runs and harden provider fanout defaults,
  - tune finding/learning signal quality and anti-spam behavior,
  - tighten websocket ordering/replay assertions for multi-provider runs.
- Finish worker signal contract runtime enforcement:
  - confidence gating, dedup, cooldowns, and escalation throttles.
- After Researcher, build Prompt Bar + Conductor orchestration layer.

## Single Active Lane

We are running one primary lane only:

1. `Logging`
2. `Watcher`
3. `Model Policy`
4. `Worker Signal Contract`
5. `Researcher`

Everything else is parked unless it unblocks this lane.

## Reconciliation Gate (Blocking Before Researcher)

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
- `docs/architecture/worker-signal-contract.md`
- `docs/architecture/roadmap-critical-analysis.md`
- `roadmap_progress.md`
