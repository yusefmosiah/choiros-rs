# ChoirOS Roadmap (Execution Lane)

Date: 2026-02-08  
Status: Authoritative immediate order

## Narrative Summary (1-minute read)

ChoirOS is now executing one lane only: `Logging -> Watcher -> Researcher`.
The goal is to finish observability foundations before expanding behavior.
EventStore is canonical; EventBus is delivery-only; watcher/researcher must emit rich, queryable events.

## What Changed

- Roadmap ordering is now explicitly single-lane, not parallel-track.
- Logging baseline is mostly complete (filters, APIs, relay, tests).
- Watcher baseline moved from prototype to deterministic multi-rule coverage.
- A dedicated backend live logs stream (`/ws/logs/events`) is now in scope as part of watcher observability.

## What To Do Next

- Finish remaining logging envelope upgrades (`trace_id`, `span_id`, `interface_kind`, `task_id`).
- Wire logs/watcher output into UI consumption paths.
- Start ResearcherActor with mandatory lifecycle/citation event contracts and websocket tests.

## Single Active Lane

We are running one primary lane only:

1. `Logging`
2. `Watcher`
3. `Researcher`

Everything else is parked unless it unblocks this lane.

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
- [x] Rule `worker_retry_storm` (retry-like progress burst) implemented.
- [x] Rule `worker_stalled_task` (started without completion/failure past threshold) implemented.
- [x] Emit `watcher.alert.*` events and persist in EventStore.
- [x] Add suppression/dedup windows to avoid alert floods.
- [x] Add UI stream integration for watcher output in logs window.
- [x] Add backend log stream transport: `/ws/logs/events` (filterable live EventStore stream).

Gate:
- Watcher detects and emits stable alerts on synthetic failure scenarios without noisy false storms.

## Milestone 3: Researcher (Next After Watcher)

Goal:
- Ship ResearcherActor with full observability so research work is inspectable in real time.

Checklist:
- [ ] Implement ResearcherActor with constrained capability surface.
- [ ] Route chat `web_search` through ResearcherActor only (no terminal-side web search tool).
- [ ] Stream lifecycle events (`planning`, `search`, `read`, `synthesis`, `citation_attach`).
- [ ] Persist citations and source metadata in event payloads.
- [ ] Add websocket tests for ordered researcher event streaming.

Gate:
- Research flow is fully observable, replayable, and inspectable by watcher/log UI.

## Deferred (Explicit)

- PromptBar orchestration depth beyond what is needed for logging/watcher/researcher integration.
- Policy actor hardening beyond deterministic local rules.
- PDF app implementation (guide stays deferred).
- Mind-map / concept-map UI beyond initial log-centric foundations.

## References

- `docs/architecture/logging-watcher-architecture-design.md`
- `docs/architecture/roadmap-critical-analysis.md`
- `roadmap_progress.md`
