# Roadmap Progress

Date: 2026-02-09
Source roadmap:
- `docs/architecture/roadmap-dependency-tree.md`
- `docs/architecture/roadmap-critical-analysis.md`
- `docs/architecture/2026-02-08-architecture-reconciliation-review.md`

## Narrative Summary (1-minute read)

Execution order is explicitly reset to avoid architecture drift: ship real desktop file apps first (`Files` + `Writer` in one sandbox-root universe), then finish `Prompt Bar -> Conductor` orchestration, then move Chat to conductor escalation + identity UX. Chat remains a compatibility interface during this transition.

## What Changed (2026-02-09, latest)

- Pathway reset decisions:
  - Prioritize product-legible desktop behavior over deeper chat orchestration hacks.
  - `Files` and `Writer` must stop behaving like generic markdown/viewer shells.
  - Conductor is the orchestration authority; app actors are capability endpoints.
  - App prompt bars route intent to Conductor; they do not bypass orchestration.
- Current state:
  - `Files` app implementation COMPLETE with 9 REST API endpoints, 43 integration tests, 31 HTTP tests, and Dioxus frontend.
  - `Writer` app implementation COMPLETE with 3 REST API endpoints, revision-based conflict handling, 16 integration tests, and Dioxus frontend with editor UX.
  - `Files` and `Writer` share sandbox scope and open sandbox-root resources.
  - Both apps now have app-specific UX (not generic viewer shells).
  - Chat-to-conductor escalation refactor is queued behind prompt-bar/conductor stabilization.

## Files App Implementation Status (2026-02-09) - COMPLETE

**Backend API:**
- 9 REST endpoints: list, metadata, content, create, write, mkdir, rename, delete, copy
- Path traversal protection and sandbox boundary enforcement
- Comprehensive error handling (403, 404, 409 status codes)

**Testing:**
- 43 Rust integration tests (all passing)
- 11 HTTP smoke tests (all passing)
- 20 HTTP negative tests (all passing)
- Total: 74 automated tests

**Frontend:**
- Dioxus component with file browser UI
- Directory navigation, breadcrumb, toolbar actions
- File listing with icons, sizes, dates
- Context actions (rename, delete, open)
- Dialog system for create/rename/delete

**Known Gaps:**
- No drag-and-drop file upload
- No file search functionality
- No bulk operations

---

## Writer App Implementation Status (2026-02-09) - COMPLETE

**Backend API:**
- 3 REST endpoints: open, save (with conflict detection), preview
- Revision-based optimistic concurrency control
- Path traversal protection and sandbox boundary enforcement
- Typed error responses (403 PATH_TRAVERSAL, 404 NOT_FOUND, 409 CONFLICT)

**Testing:**
- 16 Rust integration tests (all passing)
- HTTP smoke tests (writer_api_smoke.sh)
- HTTP conflict tests (writer_api_conflict.sh)
- Total: 16+ automated tests

**Frontend:**
- Dioxus component with editor UX
- Editable text area with save functionality
- State machine: Clean/Dirty/Saving/Saved/Conflict/Error
- Conflict resolution UI (Reload Latest / Overwrite)
- Markdown mode toggle (Edit/Preview)
- Ctrl+S keyboard shortcut

## Current Execution Lane (2026-02-09, authoritative)

1. ~~`Files` app: real explorer behavior in sandbox scope~~ - COMPLETE
2. ~~`Writer` app: real editor behavior + markdown mode~~ - COMPLETE
3. `Prompt Bar -> Conductor` primary orchestration path
4. Chat compatibility escalation into conductor (no app-level orchestration)
5. Resume watcher/signal/model-policy hardening in conductor-centric flow

## What To Do Next

1. ~~Convert `Files` from viewer shell to true explorer UX (navigate, select, open).~~ - COMPLETE
2. ~~Convert `Writer` to focused editor UX with save-first flow and optional markdown preview mode.~~ - COMPLETE
3. Complete prompt-bar conductor routing for app-scoped intents.
4. Migrate unresolved Chat requests to Conductor after prompt-bar flow is stable.
5. Resume watcher/signal/model-policy hardening in conductor-centric flow.

## Historical Execution Lane (2026-02-08, archived)

1. Logging
2. Watcher
3. Model Policy
4. Worker Signal Contract
5. Researcher

## RLM Alignment Slice (2026-02-09)

Objective:
- Align current harness work with `RLM_INTEGRATION_REPORT.md` and `state_index_addendum.md` so Prompt Bar + Conductor can scale without context drift.

Now:
- Keep EventStore as canonical memory while introducing bounded-loop semantics in capability actors.
- Make delegated research async-first by default (`chat` returns early, final answer arrives as async follow-up).
- Prevent stale in-progress language once delegated outputs are already available.

Next:
1. Add `StateIndexActor` scaffold with frame IDs, frame stack projection, and token budget structs.
2. Add `ContextPack` assembly boundary for chat/research follow-up loops (bounded slices + priority compaction).
3. Route completion wake-up policy through Conductor path for cross-actor continuations.
4. Keep appactor output clean: no raw provider dumps as final user-facing assistant messages.

## Objective Propagation Update (2026-02-09, latest)

Now:
- Chat planner contract now carries explicit objective fields (`objective_status`, `completion_reason`) instead of relying on response-text inference.
- Objective contract context is propagated through delegated chat capability calls.
- Chat loop completion is objective-driven with compatibility fallback when a model omits status fields.
- Added evidence-first guard so verifiable/time-sensitive asks do not complete without attempted evidence gathering.

Validated:
- `cargo check -p sandbox`
- `cargo test -p sandbox chat_agent::tests:: -- --nocapture`
- `cargo test -p sandbox --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture`

Urgent:
- Live Superbowl matrix currently regressed in this branch (`non_blocking=false`, `signal_to_answer=false`, `web_search=false` in fast run), indicating planner/delegation failure before expected async flow.

Next:
1. Add explicit error-path event emission for failed chat planning turns so matrix can classify failure root-cause.
2. Recover delegated `web_search` path in no-hint matrix.
3. Re-run full matrix and refresh strict-pass report row before merge.

## Narrative Summary (Legacy Observability Slice)

Logging + Watcher + Model Policy are now at a strong foundation level for operations, and Researcher moved from design-only to live delegated execution for chat `web_search`. We now have run-scoped observability (not just raw event tails), markdown run projection, structured worker failure telemetry, deterministic watcher rules (including network-spike detection), and timestamped prompt context to improve model temporal awareness. The current highest-value next build target is Researcher hardening + Worker Signal quality, with Prompt Bar + Conductor as the next orchestration layer once researcher lifecycle and signals are stable.

## Matrix Eval Update (2026-02-08, async researcher flow)

- Completed live matrix run:
  - `5 models x 4 providers = 20` executed cases
  - strict passes: `6` (`model honored + non-blocking + signal->answer + final quality`)
- Result distribution:
  - strongest: `KimiK25`, `ZaiGLM47`
  - weak in this harness/run: `ClaudeBedrockOpus46`, `ClaudeBedrockSonnet45`, `ZaiGLM47Flash`
- Provider notes:
  - `brave` underperformed this query class in strict quality
  - `tavily/exa/all` produced comparable strict pass counts
- Report file:
  - `docs/architecture/chat-superbowl-live-matrix-report-2026-02-08.md`
- Immediate implication:
  - prioritize orchestration fixes (deferred-tool short-circuit + completion-signal synthesis) over raw provider adapter changes.

## Matrix Eval Update (2026-02-08, no-hint prompt, latest)

- Re-ran live matrix with no tool/provider hinting in user prompt:
  - prompt: `As of today, whats the weather for the superbowl?`
  - models: `KimiK25`, `ZaiGLM47`
  - providers: `auto,tavily,brave,exa,all`
  - executed: `10`
  - strict passes: `8`
  - polluted follow-ups: `0`
  - `web_search -> bash` chain count: `0`
- Provider routing change:
  - `Researcher provider=auto` now defaults to parallel fanout across available providers (configurable via `CHOIR_RESEARCHER_AUTO_PROVIDER_MODE`).
- Bedrock note:
  - Expanded Bedrock-inclusive run hit a local `hyper-rustls` platform cert panic in this environment and was split out from stable subset metrics.

Implication:
- Async orchestration is mostly healthy.
- Autonomous multi-tool chaining remains the primary behavior gap before Prompt Bar + Conductor can rely on fully self-directed capability escalation.

## Matrix Eval Update (2026-02-09, refreshed + chat-loop hardening)

- Chat deferred path was hardened before rerun:
  - per-turn scoped history reload,
  - deferred-status tagging with prompt-history exclusion,
  - stale post-completion assistant status removal.
- Mixed matrix rerun:
  - `6 models x 5 providers = 30` executed,
  - summary: `strict_passes=8`, `polluted_count=0`, `search_then_bash=true`.
- Clean non-Bedrock rerun:
  - models: `KimiK25,ZaiGLM47,ZaiGLM47Flash`
  - providers: `auto,tavily,brave,exa,all`
  - summary: `executed=15 strict_passes=8 polluted_count=0 search_then_bash=false`.
- Isolated Bedrock probes:
  - `ClaudeBedrockOpus46`: pass,
  - `ClaudeBedrockSonnet45`: pass,
  - `ClaudeBedrockOpus45`: fails in this harness run (model unresolved/no tool flow).
- Targeted post-bootstrap rerun:
  - models: `ClaudeBedrockOpus46,ClaudeBedrockSonnet45,KimiK25,ZaiGLM47`,
  - providers: `auto,exa,all`,
  - summary: `executed=12 strict_passes=12 polluted_count=0 search_then_bash=false`.
- Async-first research delegation rerun:
  - summary: `executed=15 strict_passes=11 polluted_count=0 non_blocking=true signal_to_answer=true search_then_bash=false`.
- Bedrock TLS stabilization:
  - centralized cert bootstrap (`runtime_env::ensure_tls_cert_env`) now runs before
    live provider calls and app startup,
  - Bedrock matrix/provider tests preflight CA bundle availability to prevent
    `hyper-rustls` platform-cert panic -> `LazyLock poisoned` cascade behavior.

Implication:
- Non-blocking background->signal->answer flow is now reliable enough for continued researcher hardening.
- Main remaining eval gap is autonomous multi-tool escalation (`web_search -> bash`) without object-level prompt hints.

## What Changed (Latest)

- Logging UX moved to run-centric operation:
  - watcher logs preload persisted events on load (not empty after restart),
  - runs sidebar groups by correlation/task ids,
  - selected run filters main event pane,
  - run markdown can be opened directly from watcher.
- Run markdown export upgraded:
  - worker events collapsed by default,
  - explicit worker completion/failure diagnostic sections,
  - expand/collapse/copy-all controls in markdown viewer.
- Worker failure telemetry hardened:
  - structured fields now emitted (`failure_kind`, `failure_retriable`, `failure_hint`, `failure_origin`, `error_code`, `duration_ms`).
- Watcher rules hardened:
  - new `watcher.alert.network_spike`,
  - timeout detection reads structured failure fields first,
  - startup stale-task bootstrap false-positive reduced.
- Model observability completed for worker path:
  - worker events now normalize and persist both `model_requested` and `model_used` on every lifecycle event.
- Model policy extended for upcoming Researcher rollout:
  - added `researcher_default_model` + `researcher_allowed_models` in backend policy resolution and config files,
  - synced Settings model-policy document view with the new researcher role.
- Researcher runtime path is now active:
  - chat `web_search` delegations execute through ResearcherActor and emit `research.*` + `worker.task.*` run events,
  - provider-level call/result/error events are persisted and visible in run markdown,
  - provider selection now supports `auto`, explicit provider, `all`, and comma-list parallel fanout.
- Prompt temporal context was hardened:
  - chat/terminal system prompts include UTC timestamp metadata,
  - per-message prompt payloads are timestamped for LLM temporal grounding.

## What To Do Next (Priority Order)

1. Shared harness extraction:
   - unify chat/terminal/researcher loop semantics into one `agentic_harness` abstraction.
2. Multi-tool continuation policy:
   - add loop-level guidance + guardrails for discovery -> measurement escalation (`web_search` then `bash` when required).
3. Researcher v1 hardening:
   - finish Brave + Exa live-provider hardening and failure-path observability,
   - tune parallel fanout quality/reranking for low-signal result suppression,
   - verify websocket ordering and run-level replay under parallel provider calls.
4. Worker Signal Contract implementation:
   - typed turn report ingestion, validation, anti-spam gates,
   - escalation signaling semantics into Conductor control plane.
5. Prompt Bar + Conductor:
   - route universal input to actors (not chat-only),
   - maintain directives/checklist state at a glance,
   - preserve run/log traceability for every routed action.
6. Model Policy UX hardening:
   - keep document-first settings flow,
   - ensure policy edits emit model-change events with actor attribution.

## Researcher Runbook Reconciliation Update (2026-02-08, latest)

Completed in docs:
- Rewrote `docs/architecture/researcher-search-dual-interface-runbook.md` to match current runtime architecture.
- Removed stale assumptions:
  - EventBus-first ownership (now EventStore-first per ADR-0001),
  - outdated module/path references,
  - oversized speculative implementation checklist.
- Locked researcher implementation shape to current contracts:
  - `uactor -> actor` delegation envelope for orchestration,
  - `appactor -> toolactor` typed `web_search` surface for app actors,
  - provider isolation behind Researcher (Tavily, Brave, Exa),
  - typed findings/learnings/escalations via worker signal contract.
- Locked required observability fields for researcher events:
  - scope IDs, correlation/trace/span IDs, interface kind, model requested/used.

Immediate execution impact:
- Researcher is now decision-ready for code implementation without architectural ambiguity.
- Next coding slice should begin with model-policy researcher-role fields, then actor/adapter implementation.

Now:
- Reconciliation gate closeout (blocking Researcher):
  - close capability boundary leak (`ChatAgent` direct tools)
  - close terminal dual-contract drift (typed vs natural-language objective)
  - close observability gap where watcher/log panels can appear empty

Next:
- Complete Worker Signal Contract docs gate, then implement typed report validation/event mapping before Researcher.

Later:
- Implement ResearcherActor with full lifecycle + citation event streaming.

Blocked:
- Researcher milestone start is blocked by reconciliation criticals.
- Researcher is additionally blocked on Model Policy milestone completion.
- Researcher is additionally blocked on Worker Signal Contract implementation gate.

## Worker Signal Contract Decision Update (2026-02-08, latest)

Completed in docs:
- Added contract doc: `docs/architecture/worker-signal-contract.md`.
- Locked plane split:
  - control plane: worker escalations to Conductor (`blocker|help|approval|conflict`)
  - observability plane: findings/learnings/progress/artifacts as durable events
- Locked transport decision:
  - workers emit typed turn report envelopes
  - runtime validates, dedups, applies cooldowns, then emits canonical events
- Locked anti-spam strategy:
  - prompt defaults to sparse emission
  - schema caps (`findings<=2`, `learnings<=1`, `escalations<=1`)
  - runtime rejection events (`worker.signal.rejected`) with reason

Next implementation tasks:
- Add typed BAML schema for turn reports.
- Add Rust validator + governance layer (confidence thresholds, dedup, cooldown).
- Add event mapping:
  - `worker.report.received`
  - `research.finding.created`
  - `research.learning.created`
  - `worker.signal.escalation_requested`
  - `worker.signal.rejected`
- Add tests proving non-spam behavior and escalation routing correctness.

## Reconciliation Gate (2026-02-08, from architecture review)

Decision locked:
- Messaging model: **Option B** (separate contracts)
  - `uActor -> Actor`: secure delegation envelope
  - `AppActor -> ToolActor`: typed tool call envelope

Critical conflicts to resolve before Researcher:
- [x] C1: Remove `ChatAgent` local direct tool execution path.
- [x] C2: Ensure all bash execution routes through `TerminalActor`.
- [ ] C3: Remove/retire dual terminal contract drift (untyped natural-language execution path for app-tool calls).
- [x] C4: Ensure watcher/log UI receives the same committed event stream semantics as backend (`EventStore` source of truth).

Acceptance checks (must pass):
- [x] `cargo test -p sandbox --test websocket_chat_test test_websocket_streams_actor_call_for_delegated_terminal_task -- --nocapture`
- [x] `cargo test -p sandbox --test logs_ws_test -- --nocapture`
- [x] `cargo test -p sandbox --lib actors::watcher::tests:: -- --nocapture`
- [x] New capability-boundary test proving chat cannot execute tools directly (`sandbox/tests/capability_boundary_test.rs`).

Notes:
- The reconciliation review recommended dropping `Automatic*` naming migration as a priority item.
- Naming cleanup is explicitly deferred until after capability + contract + observability gates.

## Reconciliation Execution Update (2026-02-08, pass 2)

Completed in this pass:
- `ChatAgent` no longer executes local tools directly:
  - removed local `ToolRegistry` from actor state.
  - removed `ChatAgentMsg::ExecuteTool` message path.
  - removed direct `execute_tool_impl` helper.
- Chat app capability surface is now explicit and narrow:
  - `GetAvailableTools` now returns `["bash"]`.
  - planner tool description now documents delegated `bash` contract only.
  - non-bash model tool calls now return explicit unsupported-tool errors.
- Added capability boundary integration tests:
  - `sandbox/tests/capability_boundary_test.rs`
  - validates chat-visible tool surface and delegated worker event interface kind.
- Added terminal-side centralized soft policy control:
  - `CHOIR_TERMINAL_ALLOWED_COMMAND_PREFIXES` (comma-separated prefix allowlist)
  - terminal rejects command execution when policy does not match.
- Updated mixed-model live test harness to avoid deprecated direct execute API and use `ProcessMessage` flow.

Validation executed:
- `cargo check -p sandbox`
- `cargo test -p sandbox --test capability_boundary_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test test_websocket_streams_actor_call_for_delegated_terminal_task -- --nocapture`
- `cargo test -p sandbox --test logs_ws_test -- --nocapture`
- `cargo test -p sandbox --lib actors::watcher::tests:: -- --nocapture`
- `cargo test -p sandbox --test model_provider_live_test --no-run`

Remaining from the reconciliation gate:
- C3 still open: finalize terminal contract split/retirement strategy for natural-language `RunAgenticTask` vs typed app-tool dispatch envelope.

Status update:
- C3 is functionally narrowed:
  - appactor->toolactor terminal delegation now stable and typed.
  - remaining C3 work is contract clarity/documentation and explicit retirement boundaries, not core runtime breakage.

## C3 + Logging Expansion Update (2026-02-08, pass 3)

Completed in this pass:
- Closed appactor->toolactor typed terminal path for delegation:
  - `ApplicationSupervisor` now dispatches typed `TerminalMsg::RunBashTool { request, ... }`.
  - `RunAgenticTask` remains for higher-level objective execution (uactor path), but app delegation path is now explicit.
- Expanded worker event model telemetry:
  - worker lifecycle payloads now include `model_requested`.
  - progress payloads include `model_used` when available.
  - completion/failure payloads persist both `model_requested` and `model_used`.
- Added policy-aware model resolution hooks:
  - `ModelRegistry::resolve_for_role("chat" | "terminal", ...)`
  - `load_model_policy()` from `CHOIR_MODEL_POLICY_PATH` / `config/model-policy.toml`.
- Increased chat logging coverage:
  - `ChatAgent` now logs `chat.user_msg` directly in actor flow (not only API edge paths).
  - API/ws duplicate user-message persistence path removed to avoid duplicate events.
- Logs UX now summary-first and less dense:
  - Logs window fetches targeted prefixes: `worker.task`, `watcher.alert`, `model.`, `chat.`.
  - Adds one-line human-readable summary per event while keeping raw JSON in collapsible details.

Validation:
- `cargo check -p sandbox`
- `cargo check --manifest-path dioxus-desktop/Cargo.toml`
- `cargo test -p sandbox --test capability_boundary_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test test_websocket_streams_actor_call_for_delegated_terminal_task -- --nocapture`
- `cargo test -p sandbox --test logs_ws_test -- --nocapture`
- `cargo test -p sandbox --lib actors::watcher::tests:: -- --nocapture`

Remaining to fully close model-policy-before-researcher:
- Add backend summarizer actor (raw stream already present in WS logs UI).
- Emit `log.summary.*` events from summarizer and keep self-skip guard active.
- Start researcher implementation on top of policy-complete model routing.

## Model Policy + Logs UX Update (2026-02-08, latest)

Completed:
- Added committed policy source file: `config/model-policy.toml`.
- Updated defaults:
  - chat: `ClaudeBedrockSonnet45`
  - conductor: `ClaudeBedrockOpus46`
  - summarizer: `ZaiGLM47Flash`
- Removed `ClaudeBedrockOpus45` from active runtime model registry and canonical fallback path.
- Extended policy schema with role-specific fields:
  - `conductor_default_model`, `summarizer_default_model`
  - `conductor_allowed_models`, `summarizer_allowed_models`
- Updated Settings `Model Policy` tab doc preview to match runtime defaults and role allowlists.
- Added separate `Available models` catalog block in Settings (outside the policy document view).
- Logs WS summarization guard now skips recursive summarization for `log.summary.*` events.

Validation:
- `cargo check -p sandbox`
- `cargo check --manifest-path dioxus-desktop/Cargo.toml`

## Pre-Researcher Gate Update (2026-02-08, latest pass)

Completed:
- Logging envelope metadata now attached on supervisor/worker persisted payloads:
  - `trace_id` (mapped from correlation)
  - `span_id`
  - `interface_kind` (`uactor_actor` / `appactor_toolactor`)
  - normalized `task_id` where applicable
- Added JSONL export path:
  - `GET /logs/events.jsonl`
  - NDJSON output for eval/export portability
- Added watcher retry-storm rule:
  - `watcher.alert.retry_storm`
  - configurable via `WATCHER_RETRY_STORM_THRESHOLD`
- Added supervisor auto-recovery wiring for EventBus/EventRelay child termination:
  - EventBus respawn on termination
  - EventRelay receives automatic `SetEventBus` rebind
  - EventRelay respawn if terminated while EventBus is available
- Added logs/watcher UI integration path in desktop app:
  - new `Logs` app window
  - live watcher alert display via `/logs/events` polling

Validation:
- `cargo fmt --all`
- `cargo test -p sandbox --test logs_api_test -- --nocapture` (3 passed)
- `cargo test -p sandbox --test logs_ws_test -- --nocapture` (2 passed)
- `cargo test -p sandbox --lib actors::watcher::tests:: -- --nocapture` (4 passed)
- `cargo test -p sandbox --lib actors::event_relay::tests:: -- --nocapture` (3 passed)
- `cargo check -p sandbox`
- `cargo check --manifest-path dioxus-desktop/Cargo.toml`

What To Do Next:
- Begin ResearcherActor implementation with provider-isolated tool surface.
- Keep event contracts/citations first-class from day one.

## Logging MVP Update (2026-02-08)

Completed in this pass:
- Added EventStore filtered recent-event query for observability:
  - `EventStoreMsg::GetRecentEvents { since_seq, limit, event_type_prefix, actor_id, user_id }`
  - hard cap `limit <= 1000`
  - deterministic ordering by `seq ASC`
- Added backend logs API endpoint:
  - `GET /logs/events`
  - query params: `since_seq`, `limit`, `event_type_prefix`, `actor_id`, `user_id`
- Added EventStore coverage:
  - `test_get_recent_events_with_filters`

Files:
- `sandbox/src/actors/event_store.rs`
- `sandbox/src/api/logs.rs`
- `sandbox/src/api/mod.rs`

Validation:
- `cargo fmt`
- `cargo check -p sandbox`
- `cargo test -p sandbox test_get_recent_events_with_filters -- --nocapture`

Next up (Watcher milestone start):
- Add WatcherActor deterministic rule loop over `GetRecentEvents`.
- Emit `watcher.alert.*` events into EventStore.
- Surface watcher alerts in logs query/UI stream.

## Watcher MVP Update (2026-02-08)

Completed in this pass:
- Added deterministic `WatcherActor` with periodic scan loop over EventStore recent events.
- Implemented first watcher rule:
  - if `worker.task.failed` count in scan window >= 3, emit `watcher.alert.failure_spike`.
- Added watcher dedup memory window to suppress duplicate alerts.
- Wired watcher startup in `sandbox` server:
  - env toggle: `WATCHER_ENABLED` (default on)
  - poll interval: `WATCHER_POLL_MS` (default `1500`)
- Added watcher unit test:
  - `test_watcher_emits_failure_spike_alert`

Files:
- `sandbox/src/actors/watcher.rs`
- `sandbox/src/actors/mod.rs`
- `sandbox/src/main.rs`

Validation:
- `cargo fmt`
- `cargo check -p sandbox`
- `cargo test -p sandbox test_watcher_emits_failure_spike_alert -- --nocapture`

Immediate next:
- Approve ADR-0001 and enforce single-write/event relay model.
- Then add watcher rules for timeout/retry/missing completion.
- Then add logs UI consumption path for `watcher.alert.*` event types.

## ADR-0001 Rollout Update (2026-02-08)

Completed (phase-1):
- Approved architecture direction in ADR:
  - `docs/architecture/adr-0001-eventstore-eventbus-reconciliation.md`
- Enforced EventStore-first persistence on supervisor request/worker events.
- Switched supervisor EventBus publishes to delivery-only (`persist: false`).
- Set EventBus default persistence to disabled (`default_persist: false`).
- Updated websocket actor-call stream matching to include canonical `worker.task.*` event names.

Validation:
- `cargo check -p sandbox`
- `cargo test -p sandbox --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture`

Remaining (phase-2):
- Complete follow-on outage/recovery invariants that include resumed relay after EventBus restore.

## ADR-0001 Phase-2 Update (2026-02-08)

Completed:
- Added committed relay actor:
  - `sandbox/src/actors/event_relay.rs`
  - polls `EventStore` with cursor and publishes committed events to `EventBus`
  - adds `committed_event` metadata to relayed payloads
- Wired relay into supervision tree:
  - `ApplicationSupervisor` now spawns/supervises `EventRelayActor`
  - health now includes `event_relay_healthy`
- Removed duplicate-delivery risk:
  - supervisor helper path no longer directly fans out events to EventBus
  - relay is now the delivery path from committed events

Validation:
- `cargo check -p sandbox`
- `cargo test -p sandbox test_event_relay_publishes_committed_events -- --nocapture`
- `cargo test -p sandbox --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test test_websocket_streams_actor_call_for_delegated_terminal_task -- --nocapture`

Execution mode update (2026-02-08):
- `docs/architecture/roadmap-dependency-tree.md` is now a linear checklist (authoritative order).
- We no longer treat parallel feature-track execution as default roadmap strategy.

## Critical Path Status

| Phase | Status | Notes |
|---|---|---|
| B. Multiagent Control Plane v1 | In progress | Control-plane contract + async terminal delegation API implemented; run-scoped logs + structured failure telemetry live |
| F. Identity and Scope Enforcement v1 | In progress | Started scoped chat payloads (`session_id`, `thread_id`) |
| C. Chat Delegation Refactor v1 | Pending | Depends on B baseline contracts |
| D. Context Broker v1 | Pending | Blocked on F hardening; likely absorbed into Prompt Bar + Conductor routing layer |
| G. SandboxFS Persistence | Pending | Not started in this session |
| H. Hypervisor Integration | Pending | Not started in this session |

## Priority Reset (Most Important View)

Top deliverable is now explicit:
- First-class Directives app cockpit (hierarchical, event-linked, always available as an app/window).

Execution source of truth:
- `docs/architecture/directives-execution-checklist.md`

Non-negotiables from this reset:
- Chat uses `bash` as interface but all shell execution is delegated to `TerminalActor`.
- PromptBar orchestrates actors and writes memos; it does not call tools.
- Capability boundaries are enforced in supervisor/actor code paths, not only prompts.

## Phase A Model Routing Validation (2026-02-08)

Completed:
- Added and ran non-network model-routing matrix tests (resolution precedence, aliases, API override parsing).
- Added deterministic env-var test guard for model config tests.
- Published report:
  - `docs/reports/model-agnostic-test-report.md`

Outstanding for full external gate:
- Run websocket model-switch integration tests in unrestricted local environment.
- Run live provider smoke checks for Bedrock/Z.ai/Kimi with credentials.

## Phase B Model Routing UX Bake-In (2026-02-08)

Completed:
- Added `model_source` propagation in chat response payloads (`request|app|user|env_default|fallback`).
- Added model routing audit events:
  - `model.selection` (per processed turn)
  - `model.changed` (switch model action)
- Wired websocket response payload to include `model_source`.
- Updated chat UI assistant bundle rendering to display `model_used` + `model_source`.

Validation:
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture` (19 passed)
- `cargo check --manifest-path dioxus-desktop/Cargo.toml`

## Completed In This Session

### 1) Phase A Foundation: EventBus Integration + Correlation Tracing
- Wired `EventBusActor` into `ApplicationSupervisor` as a supervised child.
- Added supervisor request lifecycle events on all top-level calls:
  - `supervisor.desktop.get_or_create.{started|completed|failed}`
  - `supervisor.chat.get_or_create.{started|completed|failed}`
  - `supervisor.chat_agent.get_or_create.{started|completed|failed}`
  - `supervisor.terminal.get_or_create.{started|completed|failed}`
- Added per-request correlation IDs using ULID and attached them to published EventBus events.

Files:
- `sandbox/src/supervisor/mod.rs`

Validation:
- `cargo check -p sandbox`
- `cargo test -p sandbox --features supervision_refactor --test desktop_supervision_test`
- `cargo test -p sandbox --features supervision_refactor --test supervision_test`
- `cargo test -p sandbox event_bus`

### 2) Phase A Foundation: Supervisor Health Monitoring v1
- Added health snapshot message: `ApplicationSupervisorMsg::GetHealth`.
- Added health data:
  - child liveness (`event_bus_healthy`, `session_supervisor_healthy`)
  - supervision event counters (`actor_started`, `actor_failed`, `actor_terminated`)
  - `last_supervision_failure`
- Added integration assertions that health is populated after startup.

Files:
- `sandbox/src/supervisor/mod.rs`
- `sandbox/tests/supervision_test.rs`

## New Progress (Current Pass)

### 4) Phase B Kickoff: Control Plane Contract + Async Delegation API
- Added delegated task/result contracts in shared types:
  - `DelegatedTaskKind`
  - `DelegatedTask`
  - `DelegatedTaskStatus`
  - `DelegatedTaskResult`
- Added worker task topic constants:
  - `worker.task.started`
  - `worker.task.progress`
  - `worker.task.completed`
  - `worker.task.failed`
- Added `ApplicationSupervisorMsg::DelegateTerminalTask`:
  - returns immediate acceptance (`DelegatedTask`)
  - creates `task_id` + `correlation_id`
  - publishes lifecycle events via EventBus
  - executes terminal dispatch in background (`tokio::spawn`)
- Added app-state entrypoint for delegation:
  - `AppState::delegate_terminal_task(...)`
- Added integration test:
  - `test_application_supervisor_accepts_async_terminal_delegation`

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/app_state.rs`
- `sandbox/tests/supervision_test.rs`

### 5) Logging/Watcher Hardening: Deterministic Rules + Outage Safety

Completed:
- Confirmed relay cursor safety invariant with explicit outage test:
  - `test_event_relay_does_not_advance_cursor_when_bus_unavailable`
- Expanded watcher deterministic rules:
  - `watcher.alert.failure_spike` (existing, threshold-configurable)
  - `watcher.alert.timeout_spike` (new)
  - `watcher.alert.stalled_task` (new; task started without completion/failure past timeout)
- Added watcher runtime configuration via env:
  - `WATCHER_FAILURE_SPIKE_THRESHOLD`
  - `WATCHER_TIMEOUT_SPIKE_THRESHOLD`
  - `WATCHER_STALLED_TASK_TIMEOUT_MS`
- Ensured stalled-task rule evaluates every scan tick, including empty-delta scans.

Files:
- `sandbox/src/actors/watcher.rs`
- `sandbox/src/main.rs`

Validation:
- `cargo fmt --all`
- `cargo test -p sandbox --lib watcher::tests:: -- --nocapture` (3 passed)
- `cargo test -p sandbox --lib actors::event_relay::tests::test_event_relay_does_not_advance_cursor_when_bus_unavailable -- --nocapture` (1 passed)
- WebSocket regression target was started but manually terminated in this pass (signal 15) after hang during integration run.

### 6) Observability Transport: Live Logs WS + Relay Rebind Invariant

Completed:
- Added live logs websocket stream endpoint:
  - `GET /ws/logs/events`
  - filter params: `since_seq`, `limit`, `event_type_prefix`, `actor_id`, `user_id`, `poll_ms`
  - stream payload includes committed EventStore rows (`seq`, `event_id`, `event_type`, `payload`, actor/user, timestamp)
- Added EventRelay bus rebind message:
  - `EventRelayMsg::SetEventBus { event_bus }`
  - enables relay continuation after EventBus replacement without cursor loss.
- Added resumed-delivery invariant coverage:
  - outage on original bus does not advance cursor
  - rebind to replacement bus + tick delivers previously committed event

Files:
- `sandbox/src/api/websocket_logs.rs`
- `sandbox/src/api/mod.rs`
- `sandbox/src/actors/event_relay.rs`
- `sandbox/tests/logs_ws_test.rs`

Validation:
- `cargo fmt --all`
- `cargo test -p sandbox --test logs_ws_test -- --nocapture` (2 passed)
- `cargo test -p sandbox --lib actors::event_relay::tests:: -- --nocapture` (3 passed)
- `cargo test -p sandbox --lib watcher::tests:: -- --nocapture` (3 passed)

Remaining:
- Wire logs WS stream into Dioxus logs/watcher UI pane (frontend consumption).
- Add automatic relay rebind in supervisor path when EventBus is replaced by supervision.

### 5) Phase B Observability: Live Terminal-Agent Progress + UI Actor Timeline
- Added structured terminal-agent progress payloads:
  - `TerminalAgentProgress { phase, message, reasoning, command, output_excerpt, exit_code, step_index, step_total, timestamp }`
- `TerminalMsg::RunAgenticTask` now accepts an optional progress stream channel.
- TerminalActor now emits progress during:
  - objective start
  - planning cycles
  - reasoning updates
  - tool call dispatch
  - tool results
  - synthesis/fallback
- ApplicationSupervisor now listens to terminal progress events and publishes enriched `worker.task.progress` payloads (scope/correlation preserved).
- Chat UI websocket now ingests `actor_call` chunks and renders them inline in the tool activity timeline.
- Added dedicated UI section for actor updates (`phase`, `message`, `reasoning`, `command`, `output_excerpt`, `exit_code`).
- Added integration coverage:
  - `test_terminal_delegation_emits_reasoning_progress_events`

Files:
- `sandbox/src/actors/terminal.rs`
- `sandbox/src/supervisor/mod.rs`
- `sandbox/tests/supervision_test.rs`
- `dioxus-desktop/src/components.rs`

### 3) Phase F Starter: Scoped Chat Payloads (Backward Compatible)
- Added shared payload helpers:
  - `shared_types::chat_user_payload(...)`
  - `shared_types::parse_chat_user_text(...)`
- HTTP chat now accepts optional:
  - `session_id`
  - `thread_id`
- Chat user events now persist scope metadata when provided.
- WebSocket chat now carries default scope IDs and persists them on user message events.
- Runtime parsers updated to support both legacy payload strings and new scoped-object payloads.
- Added scope-aware event retrieval path in EventStore:
  - `GetEventsForActorWithScope { actor_id, session_id, thread_id, since_seq }`
  - API usage in chat messages endpoint when both scope keys are provided.
- Added EventStore test coverage for scoped retrieval filtering.
- Added generic scope wrapper helper:
  - `shared_types::with_scope(...)`
- Threaded scope through `ChatAgentMsg::ProcessMessage` and applied scope to:
  - assistant events (`chat.assistant_msg`)
  - tool call events (`chat.tool_call`)
  - tool result events (`chat.tool_result`)
- Added chat API integration coverage for mixed-thread filtering:
  - `test_get_messages_scope_filter_returns_only_matching_thread`
- Added explicit EventStore scope columns and indexing:
  - columns: `session_id`, `thread_id`
  - index: `idx_events_session_thread(session_id, thread_id)`
  - migration-time backfill from `payload.scope.*`
  - scoped query path prefers columns, with payload fallback for legacy rows
- Added API boundary enforcement for partial scope keys:
  - reject `session_id` without `thread_id` (400)
  - reject `thread_id` without `session_id` (400)
  - added integration tests for both GET and POST chat endpoints
- Added scope-aware websocket tool-stream retrieval:
  - initial event cursor now uses scoped query
  - incremental tool event polling now uses scoped query
  - prevents cross-thread tool event bleed on shared actor streams
- Added scope-aware ChatAgent identity + preload:
  - Chat agent key now includes session/thread when available
  - ChatAgent preload fetch uses scoped EventStore query when scope is present
  - prevents in-memory conversation history bleed across chat app instances

Files:
- `shared-types/src/lib.rs`
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/actors/chat.rs`
- `sandbox/src/actors/chat_agent.rs`
- `sandbox/src/actors/event_store.rs`

## Immediate Next Actions

1. Define a typed worker event schema for multi-agent observability (chat/terminal/supervisor/watcher) so UI can render by event kind instead of heuristic payload parsing.
2. Add terminal-agent event emission for raw tool lifecycle records (`tool_call`, `tool_result`, command duration) as first-class worker events.
3. Add watcher prototype actor that subscribes to worker event topics and emits escalation signals for timeout/failure/retry patterns.
4. Add explicit websocket integration tests that assert actor-call chunks stream in-order with reasoning/tool events under scoped session/thread.

## Direction Reset (2026-02-08)

Reason for reset:
- Abstract architecture work was feeling diffuse; next steps need to be concrete, high-leverage, and visibly multi-agent.

Updated near-term priority:
1. `ResearcherActor` as first networked capability actor exposed as `web_search` tool.
2. `PromptBarActor` as universal entrypoint above app actors.
3. `GitActor` as first local capability actor through the same contract.
4. `McpActor` after PromptBar + Git/Researcher prove the capability pattern.
5. `PolicyActor` is deferred as deterministic high-risk escalation only (not first-line routing).
6. PDF app implementation is deferred; land a PDF implementation guide first.

Design rule now in force:
- Treat every tool call as an actor call (`tool -> agent -> actor`), with one lifecycle/event contract and one observability stream (`actor_call`).

Concrete next milestone (Phase B continuation):
- Ship `ResearcherActor v1` with:
  - delegated execution from ChatAgent via tool abstraction (`web_search`)
  - streaming phases (`planning`, `search_results`, `fetch_started/completed`, `synthesis`)
  - citation-rich output + openable page links
  - websocket integration tests for ordered actor-call flow

Deferred planning item added:
- `docs/architecture/pdf-app-implementation-guide.md` (guide-only phase before PDF app implementation).

## Phase B Implementation Checklist

### Step 1: Control-Plane Contract
- [x] Add delegated task envelope with:
  - [x] `task_id`
  - [x] `correlation_id`
  - [x] `actor_id`
  - [x] `session_id`
  - [x] `thread_id`
  - [x] `kind`
  - [x] `payload`
- [x] Add delegated task result envelope with:
  - [x] `status` (`accepted|running|completed|failed`)
  - [x] `output`
  - [x] `error`
  - [x] timestamps
- [x] Define event topic conventions:
  - [x] `worker.task.started`
  - [x] `worker.task.progress`
  - [x] `worker.task.completed`
  - [x] `worker.task.failed`

### Step 2: Supervisor Delegation API (`run_async` style)
- [x] Add non-blocking supervisor API for terminal delegation.
- [x] Supervisor generates `task_id` + `correlation_id`.
- [x] API returns immediate acceptance response (does not block on execution completion).
- [x] Background execution path publishes lifecycle events through EventBus.

### Step 3: Terminal Worker Path (Next)
- [x] Route delegated terminal command through TerminalActor session.
- [x] Emit streamed progress/output events.
- [x] Add timeout and cancellation behavior.

### Step 4: ChatAgent Routing Integration (Next)
- [x] Replace direct terminal tool execution path with delegation API.
- [x] Keep fallback for non-terminal tools during transition.

### Step 5: UI Actor-Call Timeline (Next)
- [x] Subscribe to worker lifecycle topics in websocket/UI.
- [x] Render live actor-call state and output.

### Step 6: Phase B Gate Test (Next)
- [x] Add integration test: supervisor delegation -> terminal execution -> persisted trace.

## 2026-02-09 Update (Single-Loop Simplification)

- [x] Removed chat `plan -> separate synthesis` split; chat now runs a single autonomous loop and emits final response from loop state.
- [x] Added deterministic final-response fallback from gathered tool results when model omits a final response.
- [x] Preserved non-blocking async delegated research flow under websocket/API tests.
- [x] Re-ran live Superbowl matrix (no object-level tool hints in prompt):
  - `executed=15`
  - `strict_passes=8`
  - `non_blocking=true`
  - `signal_to_answer=true`
  - `polluted_count=0`
  - `search_then_bash=false`

Next checklist focus:
- [ ] Unify `chat`, `terminal`, `researcher` on one shared autonomous loop harness abstraction.
- [ ] Add continuation policy for capability escalation (`search -> terminal`) when evidence is incomplete.
- [ ] Improve provider quality controls (ranking/filtering) so noisy search hits do not leak into final user-facing answer.

## 2026-02-09 Capability Policy Milestone

- [x] Documented capability escalation contract in
  `docs/architecture/capability-escalation-policy.md`.
- [x] Researcher now emits objective-completion metadata:
  - `objective_status` (`complete|incomplete|blocked`)
  - `completion_reason`
  - `recommended_next_capability`
  - `recommended_next_objective`
- [x] Supervisor now enforces policy-driven escalation from research -> terminal when allowed.
- [x] Added policy controls:
  - `CHOIR_RESEARCH_ENABLE_TERMINAL_ESCALATION`
  - `CHOIR_RESEARCH_TERMINAL_ESCALATION_TIMEOUT_MS`
  - `CHOIR_RESEARCH_TERMINAL_ESCALATION_MAX_STEPS`

## Phase B Progress Update (Steps 3-5)

- Step 3 completed:
  - delegated terminal tasks now route via TerminalActor sessions
  - supervisor publishes `worker.task.progress` updates with output chunks
  - timeout path sends terminal interrupt and marks task failure on timeout
  - TerminalActor now exposes an agentic execution harness (`RunAgenticTask`) that can plan multi-step command sequences and synthesize a summary
- Step 4 completed:
  - ChatAgent now routes `bash` tool calls to supervisor terminal delegation
  - ChatAgent now awaits delegated task completion and returns terminal output/error as tool result
  - direct `bash` execution is blocked; `bash` runs only via TerminalActor delegation
- Step 5 completed partially:
  - websocket stream emits `actor_call` chunks from `worker_*` events
  - terminal delegation completion payload carries transparency fields:
    - `reasoning`
    - `executed_commands`
    - `steps` (command, exit_code, output_excerpt)
  - frontend now renders actor-call timeline entries during execution with live phase/reasoning/command/output metadata
- Step 6 completed:
  - added supervision integration gate test for delegated terminal trace persistence with correlation ID continuity

## Code Review Outcome (2026-02-07)

Findings addressed in this pass:
- Fixed terminal-agent failure semantics:
  - non-zero terminal command exits now emit `worker_failed` (not `worker_complete`)
  - completion payload still includes transparency fields (`reasoning`, `executed_commands`, `steps`)
- Added regression test coverage:
  - `test_terminal_delegation_nonzero_exit_marks_failed`

Residual risks:
- Terminal agent client registry is currently Bedrock-only in `TerminalActor`; model-selection parity with `ChatAgent` is pending.
- UI actor timeline currently renders JSON-backed sections; richer typed UX and grouped phases are still pending.

Validation rerun:
- `cargo check -p sandbox`
- `cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture`
- `cargo test -p sandbox --lib test_run_agentic_task_executes_curl_against_local_server -- --nocapture`
- `cargo check` (in `dioxus-desktop/`)
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture`

Additional API test coverage:
- Added websocket integration test to prove live actor-call observability path:
  - connect `/ws/chat/{actor_id}?user_id={user_id}`
  - delegate terminal task via `AppState::delegate_terminal_task(...)`
  - trigger websocket chat stream with message frames
  - assert receipt of `actor_call` chunks carrying worker task metadata

## Day-End Report (Workday Window)

Time window: **2026-02-06 04:00 EST** through **2026-02-07 01:39 EST**

### Outcome
- Phase B landed with live observability in place for delegated terminal execution.
- Chat and terminal separation remains intact, with actor-call streaming now visible through websocket and UI.
- Day ended in a stable state with all targeted tests passing.

### Commits In Window (16)
1. `b50879c` (Fri Feb 6 13:18:44 2026 -0500) `fix: resolve 5 critical bugs from porting review`
2. `5ce6b92` (Fri Feb 6 13:19:00 2026 -0500) `docs: move review reports to docs directory`
3. `48d7627` (Fri Feb 6 13:52:53 2026 -0500) `bug fixes`
4. `6ded167` (Fri Feb 6 14:32:41 2026 -0500) `need to fix connecting to desktop bug and too many open files`
5. `25e6427` (Fri Feb 6 17:07:29 2026 -0500) `Fix Dioxus runtime panic`
6. `bf90464` (Fri Feb 6 18:29:20 2026 -0500) `Stabilize terminal websocket lifecycle across reloads and multi-browser sessions`
7. `0e25530` (Fri Feb 6 18:54:17 2026 -0500) `Add window drag and mobile layout`
8. `973ea53` (Fri Feb 6 20:53:19 2026 -0500) `refactor: complete supervision cutover, remove ActorManager runtime`
9. `d9790c3` (Fri Feb 6 21:06:36 2026 -0500) `docs: archive React migration docs and remove sandbox-ui directory`
10. `0b7fc0b` (Fri Feb 6 21:35:05 2026 -0500) `Document critical roadmap gaps`
11. `e53c1f9` (Fri Feb 6 21:54:33 2026 -0500) `Act on roadmap progress update`
12. `f67ee36` (Fri Feb 6 22:04:52 2026 -0500) `Document scoped roadmap progress`
13. `0e77f99` (Fri Feb 6 22:12:57 2026 -0500) `Document ChatAgent scope fixes`
14. `00e7769` (Sat Feb 7 00:52:52 2026 -0500) `Investigate agent communication time`
15. `eaabac7` (Sat Feb 7 01:08:07 2026 -0500) `Plan multiagent terminal API`
16. `6b095dd` (Sat Feb 7 01:33:06 2026 -0500) `Fetch Boston weather via API`

### Commit Scope Snapshot (Latest 3 in Window)
- `00e7769`: 15 files changed, 1759 insertions, 16 deletions
- `eaabac7`: 2 files changed, 66 insertions, 8 deletions
- `6b095dd`: 7 files changed, 662 insertions, 22 deletions

### Metrics (Full 16-Commit Window)
- Commit count: `16`
- Files changed (sum across commits): `249`
- Unique files touched: `165`
- LOC added: `27,165`
- LOC deleted: `9,374`
- Net LOC: `+17,791`
- Largest addition commit: `b50879c` (`+9,954 / -121`, `16 files`)
- Largest deletion commit: `d9790c3` (`+2,724 / -7,906`, `56 files`)

### Key Delivered Items (Across Today’s Commits)
- Delegated terminal execution path through supervisor/app-state contract.
- Terminal agent progress model (`phase`, `reasoning`, `command`, output excerpts).
- Worker lifecycle publishing and websocket `actor_call` streaming.
- UI rendering for actor updates in chat tool activity stream.
- Websocket integration tests validating actor-call delivery for delegated terminal tasks.

### Narrative
1. Stabilized runtime + websocket behavior and closed high-priority UI/runtime defects.
2. Completed supervision cutover and removed ActorManager-era runtime coupling.
3. Consolidated roadmap artifacts and converted critical-path analysis into tracked execution.
4. Landed Phase B observability: terminal delegation telemetry, actor-call streaming, and test-backed websocket visibility.
