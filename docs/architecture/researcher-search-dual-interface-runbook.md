# Researcher Search Dual-Interface Runbook

Date: 2026-02-08
Status: Authoritative implementation + hardening runbook (current architecture aligned)

## Narrative Summary (1-minute read)

Researcher baseline is now running in the runtime path after Logging, Watcher, and Model Policy gates.
This runbook keeps the **dual contract** by design:

1. `uactor -> actor`: universal orchestration delegates natural-language objectives to Researcher.
2. `appactor -> toolactor`: app-facing actors (for example Chat) invoke typed Researcher tools, not raw terminal/web APIs.

Researcher owns web search capabilities (Tavily, Brave, Exa). Chat and Terminal do not call those providers directly.
All researcher execution and signals are EventStore-first, then relayed to EventBus.

## What Changed

- Status moved from pre-implementation to active hardening:
  - `web_search` delegations now execute through `ResearcherActor`,
  - provider lifecycle events and citations are visible in run logs.

- Replaced stale EventBus-first guidance with ADR-0001 alignment:
  - EventStore is canonical, EventBus is delivery plane.
- Replaced outdated file paths and module assumptions with current tree:
  - `sandbox/src/supervisor/mod.rs`, `sandbox/src/actors/*`, `baml_src/*`, existing logs/ws APIs.
- Aligned signaling to `worker-signal-contract.md`:
  - typed worker turn reports, anti-spam gates, escalation semantics.
- Aligned to model policy gate:
  - researcher role must be policy-routed and model-attributed in events.
- Removed oversized speculative checklist items and kept only near-term, implementable steps.

## What To Do Next

1. Harden live provider path coverage for Brave + Exa (Tavily already validated in interactive runs).
2. Tighten signal quality controls (`finding/learning` anti-spam and confidence gating) in runtime validators.
3. Expand websocket/run-markdown ordered assertions for multi-provider (`provider=all`) runs.
4. Align researcher prompt/model policy defaults with temporal-awareness contract (UTC timestamp in prompt context).
5. Keep provider-specific quirks out of tests; retain provider-agnostic acceptance criteria.

---

## 1) Scope and Goals

### In Scope (v1)

- Researcher capability actor with dual interface contracts.
- Provider-isolated search adapters:
  - Tavily
  - Brave Search
  - Exa
- Deterministic normalized result/citation shape.
- Worker signal report emission for findings/learnings/escalations.
- Full observability in run logs (`/logs/events`, `/ws/logs/events`, `/logs/run.md`).

### Out of Scope (v1)

- Browser automation/scraping outside provider APIs.
- Autonomous code execution inside Researcher.
- Multi-hop research planning with self-spawned workers.
- UI redesign beyond existing run/log surfaces.

---

## 2) Canonical Contracts

## 2.1 `uactor -> actor` (delegation)

Use for conductor/universal routing into Researcher.

Conceptual envelope:

```toml
kind = "research.objective"
objective = "Find current consensus on X and cite sources"
scope.session_id = "session:..."
scope.thread_id = "thread:..."
constraints.max_results = 8
constraints.timeout_ms = 45000
constraints.provider_preference = "auto|tavily|brave|exa"
constraints.allowed_domains = ["..."]
constraints.blocked_domains = ["..."]
```

Return payload includes:

- summary
- citations
- findings/learnings/escalations (typed worker report)
- execution metadata (provider calls, durations, errors)

## 2.2 `appactor -> toolactor` (typed tool calls)

Use for app-level invocation (for example Chat tool surface).

Initial app-facing tool surface:

- `web_search`

Tool args are schema-per-tool (not one shared generic bag).

```toml
query = "weather API outage patterns"
provider = "auto|tavily|brave|exa"
max_results = 6
time_range = "day|week|month|year"
include_domains = ["example.com"]
exclude_domains = ["ads.example"]
```

The app actor never calls provider APIs directly; it delegates to Researcher.

---

## 3) Event and Signal Model (Authoritative)

Researcher events are persisted to EventStore first. EventRelay may publish to EventBus.

### 3.1 Mandatory lifecycle events

- `research.task.started`
- `research.task.progress`
- `research.task.completed`
- `research.task.failed`

### 3.2 Provider call events

- `research.provider.call`
- `research.provider.result`
- `research.provider.error`

### 3.3 Worker signal events (from typed report contract)

- `research.finding.created`
- `research.learning.created`
- `worker.signal.escalation_requested`
- `worker.signal.rejected`

### 3.4 Required metadata in all researcher events

- `scope.session_id`
- `scope.thread_id`
- `correlation_id`
- `trace_id`
- `span_id`
- `interface_kind` (`uactor_actor` or `appactor_toolactor`)
- `model_requested`
- `model_used`
- `actor_id` (emitter)

---

## 4) Provider Integration Contract

## 4.1 Environment keys

- `TAVILY_API_KEY`
- `BRAVE_API_KEY`
- `EXA_API_KEY`

If missing, provider is skipped with explicit diagnostic events (not silent fallback).

## 4.2 Default routing (v1)

For `provider=auto`:

1. Tavily
2. Brave
3. Exa

Override heuristics:

- news/freshness-heavy queries: prefer Tavily or Brave
- deep semantic/research-heavy queries: allow Exa priority

## 4.3 Normalized result shape

Each provider maps into a single deterministic record:

```toml
id = "stable-url-or-provider-id"
provider = "tavily|brave|exa"
title = "..."
url = "https://..."
snippet = "..."
published_at = "ISO8601 or null"
score = 0.0
author = "..."
```

Citations are derived directly from normalized results and preserved in run logs.

---

## 5) Model Policy Requirements Before Researcher Rollout

Researcher must participate in policy-driven model routing.

Required additions:

- `researcher_default_model`
- `researcher_allowed_models`

Code touchpoints:

- `sandbox/src/actors/model_config.rs`
- `config/model-policy.toml`

Acceptance:

- model selection for researcher emits auditable `model.selection` (or equivalent role-specific event)
- researcher lifecycle events include both `model_requested` and `model_used`

---

## 6) Implementation Checklist (Decision Complete)

## Phase A: Schema and Policy Prep

1. Add researcher role fields in model policy structs and resolution logic.
2. Add defaults/allowlists in `config/model-policy.toml`.
3. Add/extend BAML types for:
   - `WebSearchToolArgs` (app-facing)
   - provider-specific request/response structs
   - normalized search result/citation structs
4. Keep schema-per-tool design; do not collapse into one flat generic args object.

## Phase B: Researcher Actor Core

5. Create `sandbox/src/actors/researcher.rs`.
6. Add actor to exports in `sandbox/src/actors/mod.rs`.
7. Register/supervise actor from current supervisor tree in `sandbox/src/supervisor/mod.rs`.
8. Implement both ingress paths:
   - delegation message (`uactor -> actor`)
   - typed tool invocation (`appactor -> toolactor`)

## Phase C: Provider Adapters

9. Implement adapters for Tavily/Brave/Exa with explicit request/response mapping.
10. Add deterministic fallback/routing logic for `provider=auto`.
11. Emit per-provider call/result/error events with latency and count metadata.

## Phase D: Worker Signal Integration

12. Emit typed worker turn reports from Researcher turns.
13. Apply runtime anti-spam gates from worker signal contract:
    - per-turn caps
    - confidence thresholds
    - dedup hashes
    - escalation cooldown
14. Map accepted/rejected signals into canonical events.

## Phase E: API + Observability Wiring

15. Ensure chat/websocket surfaces receive researcher events in order.
16. Ensure run markdown export (`/logs/run.md`) includes researcher lifecycle and citations.
17. Ensure logs websocket (`/ws/logs/events`) can filter by `research.` prefixes.

## Phase F: Tests

18. Add unit tests for provider mapping normalization.
19. Add integration tests for dual interface routing and event ordering.
20. Add run-markdown assertion tests for researcher runs.
21. Add live smoke tests (env-gated) for Tavily/Brave/Exa.

---

## 7) Test Matrix (Minimum)

## 7.1 Unit

- Tavily -> normalized mapping
- Brave -> normalized mapping
- Exa -> normalized mapping
- missing/partial fields do not panic

## 7.2 Integration

- `uactor -> actor` objective reaches Researcher and returns summary + citations
- `appactor -> toolactor` `web_search` tool delegates to Researcher
- `provider=auto` fallback works when first provider fails
- event stream order is monotonic per run:
  - started -> progress -> completed/failed
- worker signal anti-spam gates reject duplicates and log rejection reason

## 7.3 Run-log/WS

- `/ws/logs/events` includes researcher events
- `/logs/run.md` groups researcher events into same run correlation
- markdown export includes citations and provider metadata

## 7.4 Live (gated)

- Tavily smoke with `TAVILY_API_KEY`
- Brave smoke with `BRAVE_API_KEY`
- Exa smoke with `EXA_API_KEY`

No provider-specific weather assumptions in tests.

---

## 8) Failure Modes and Guardrails

- Provider auth failure:
  - emit `research.provider.error` with provider + status category
  - continue fallback chain when configured
- Total provider failure:
  - emit `research.task.failed` with attempted providers and error summary
- Signal spam:
  - reject and emit `worker.signal.rejected`
- Missing model policy mapping:
  - fallback to allowed model only, emit diagnostic event

---

## 9) Acceptance Criteria (Gate to Merge)

1. Researcher can be invoked through both contracts.
2. Provider APIs are isolated behind Researcher (no direct chat/provider calls).
3. EventStore contains complete researcher lifecycle and provider call trail.
4. Worker findings/learnings/escalations are typed, validated, and replayable.
5. Run markdown export shows a coherent researcher timeline with citations.
6. Tests pass:
   - mapping units
   - dual-interface integration
   - logs ws/run markdown assertions
   - env-gated live smoke tests.

---

## 10) References

- `/Users/wiz/choiros-rs/docs/architecture/adr-0001-eventstore-eventbus-reconciliation.md`
- `/Users/wiz/choiros-rs/docs/architecture/worker-signal-contract.md`
- `/Users/wiz/choiros-rs/docs/architecture/actor-network-orientation.md`
- `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
- `/Users/wiz/choiros-rs/roadmap_progress.md`
