# Packet E - Tracing Foundation Assessment

**Date:** 2026-02-14
**Status:** Research & Documentation Complete
**Sequence Position:** Human UX (assessed) -> Headless API (assessed) -> Harness (readiness checklist)

## Narrative Summary (1-minute read)

Tracing infrastructure is well-established with a solid foundation. The human UX Trace app (`trace.rs`) is **feature-complete and usable**, displaying LLM calls, tool calls, durations, model names, and run graphs. The headless API (`/logs/events`, `/ws/logs/events`, `/runs/{run_id}/timeline`) is **stable and documented**. The harness can begin consuming traces immediately via existing endpoints.

**Key Gap:** No dedicated integration tests assert `llm.call.*` event emission in actor flows, though unit tests for payload shape exist. The trace emitter wiring is mandatory in production paths but not verified via actor-level tests.

## What Changed

- Assessed human-first tracing UX quality (Trace app)
- Assessed headless API stability and documentation
- Created harness readiness checklist for app-agent consumption
- Identified gaps in test coverage for tracing wiring

## What To Do Next

1. Add integration tests asserting `llm.call.*` emission in conductor/harness flows
2. Consider adding `/runs/{run_id}/traces` endpoint optimized for harness consumption
3. Document trace event schema in OpenAPI/TypeScript types for external consumers

---

## 1. Human Tracing UX Assessment

### 1.1 Component: `dioxus-desktop/src/components/trace.rs`

**Status: Feature-complete and usable**

### What Works

| Feature | Status | Notes |
|---------|--------|-------|
| Run graph visualization | Working | SVG-based graph showing User -> Conductor -> Researcher/Terminal -> Tools |
| LLM call list | Working | Shows role/function_name, status badge, model name, duration |
| Trace detail view | Working | Displays system_context, input/output payloads, error details |
| Scope ID display | Working | Shows run_id, task_id, call_id, session_id, thread_id as badges |
| Live websocket updates | Working | Real-time streaming via `/ws/logs/events` |
| Status indicators | Working | Color-coded: completed (green), failed (red), started (yellow) |
| Provider display | Working | Shows model provider when available |
| Duration tracking | Working | Displays duration_ms on completed calls |
| Error details | Working | Shows error_code, error_message, failure_kind on failures |
| Payload inspection | Working | Collapsible details for system_context, input, output with pretty-print |
| Run selection | Working | Tab-style run_id buttons to switch between runs |
| Preload on mount | Working | Fetches last 1000 events on component mount |

### UX Quality

- **Auditable:** Yes. Every LLM call can be traced from start to completion/failure with full context.
- **Filterable:** Yes (by run_id). Additional filtering would require client-side implementation.
- **Real-time:** Yes. WebSocket streaming with 200ms polling.

### Gaps Identified

1. **No filtering by event_type_prefix** - Client-side parses all events but doesn't expose filter UI
2. **No search functionality** - Cannot search within payloads
3. **No export capability** - Cannot download trace data (but `/logs/export` exists separately)
4. **Tool trace detail limited** - `ToolTraceEvent` parsed but not rendered in detail view

### Verdict: Human-first tracing workflows are **usable and auditable**.

---

## 2. Headless API Assessment

### 2.1 Endpoints

| Endpoint | Method | Purpose | Stability |
|----------|--------|---------|-----------|
| `/logs/events` | GET | Paginated event query | Stable |
| `/logs/latest-seq` | GET | Get latest sequence number | Stable |
| `/logs/export` | GET | Export run as markdown | Stable |
| `/logs/export/jsonl` | GET | Export events as JSONL | Stable |
| `/ws/logs/events` | WS | Real-time event streaming | Stable |
| `/runs/{run_id}/timeline` | GET | Categorized run timeline | Stable |

### 2.2 Event Types for Tracing

Defined in `shared-types/src/lib.rs`:

```rust
pub const EVENT_TOPIC_TRACE_PROMPT_RECEIVED: &str = "trace.prompt.received";
pub const EVENT_TOPIC_LLM_CALL_STARTED: &str = "llm.call.started";
pub const EVENT_TOPIC_LLM_CALL_COMPLETED: &str = "llm.call.completed";
pub const EVENT_TOPIC_LLM_CALL_FAILED: &str = "llm.call.failed";
pub const EVENT_TOPIC_WORKER_TOOL_CALL: &str = "worker.tool.call";
pub const EVENT_TOPIC_WORKER_TOOL_RESULT: &str = "worker.tool.result";
```

### 2.3 Query Parameters (Logs API)

```
GET /logs/events
  ?since_seq=<i64>      # Start from sequence (default: 0)
  &limit=<i64>          # Max events (1-1000, default: 200)
  &event_type_prefix=<str>  # Filter by prefix
  &actor_id=<str>       # Filter by actor
  &user_id=<str>        # Filter by user
  &run_id=<str>         # Filter by run_id
```

### 2.4 WebSocket Protocol

```json
// Connection confirmation
{"type": "connected", "since_seq": 0, "limit": 200, ...}

// Event message
{"type": "event", "seq": 42, "event_id": "...", "event_type": "llm.call.completed", "payload": {...}}

// Ping/pong
-> {"type": "ping"}
<- {"type": "pong"}
```

### 2.5 Run Timeline API

```
GET /runs/{run_id}/timeline
  ?category=<str>           # conductor_decisions, agent_objectives, agent_conduct, agent_results, system
  &required_milestones=<str>  # Comma-separated event types

Response:
{
  "run_id": "...",
  "events": [...],
  "summary": {
    "objective": "...",
    "status": "running|completed|failed|blocked",
    "total_events": 42,
    "event_counts_by_category": {...},
    "decisions": [...],
    "artifacts": [...]
  }
}
```

### API Stability Assessment

- **Contract stability:** High. Event schema is versioned via `shared_types::Event`.
- **Backward compatibility:** Breaking changes require migration.
- **Documentation:** Event types documented in runbook; API endpoints have inline docs.
- **Type safety:** TypeScript types derivable via `ts_rs` from `shared_types::Event`.

### Gaps Identified

1. **No trace-specific endpoint** - `/runs/{run_id}/traces` would be useful for harness
2. **No aggregation endpoints** - Duration/cost summaries not exposed
3. **No filtering by trace_id** - Requires client-side filtering

### Verdict: API contract is **stable enough for harness consumption**.

---

## 3. Harness Readiness Checklist

### 3.1 What an App-Agent Harness Needs to Consume Traces

| Requirement | Status | Notes |
|-------------|--------|-------|
| HTTP endpoint for batch queries | Ready | `/logs/events` with pagination |
| WebSocket endpoint for streaming | Ready | `/ws/logs/events` |
| Event type constants | Ready | Defined in `shared_types` |
| Trace event schema | Ready | `llm.call.*` payloads well-defined |
| Scope identifiers | Ready | run_id, task_id, call_id, session_id, thread_id |
| Duration tracking | Ready | `duration_ms` in completed events |
| Error classification | Ready | `failure_kind`, `error_code`, `error_message` |
| Payload bounds | Ready | Truncation metadata included |

### 3.2 Integration Points

```
Harness Actor
    |
    v
EventStore (via ActorRef)
    |
    v
LlmTraceEmitter (via constructor injection)
    |
    v
EventStoreMsg::AppendAsync
```

### 3.3 Readiness Checklist

- [x] Trace emitter available as importable module
- [x] Event types defined in shared_types
- [x] HTTP API for querying events
- [x] WebSocket API for streaming events
- [x] Run timeline API for categorized view
- [x] Scope identifiers supported (run_id, task_id, call_id, session_id, thread_id)
- [x] Bounded payloads with truncation markers
- [x] Sensitive key redaction
- [ ] Integration tests asserting llm.call.* emission in actor flows
- [ ] Trace-specific endpoint for harness queries
- [ ] TypeScript types exported for external consumers

### 3.4 Recommended Harness API Contract

For app-agent harness consumption, consider adding:

```
GET /runs/{run_id}/traces
  ?event_types=<str>  # Comma-separated: llm.call.*, worker.tool.*

Response:
{
  "run_id": "...",
  "traces": [
    {
      "trace_id": "...",
      "role": "conductor",
      "function_name": "ConductorDecide",
      "model_used": "claude-sonnet-4",
      "status": "completed",
      "duration_ms": 1234,
      "started_at": "...",
      "ended_at": "...",
      "has_error": false
    }
  ],
  "tool_traces": [...],
  "summary": {
    "total_llm_calls": 5,
    "total_tool_calls": 12,
    "total_duration_ms": 5678,
    "error_count": 0
  }
}
```

---

## 4. Changes Made

None. This was a research and documentation task.

---

## 5. Test Coverage Assessment

### 5.1 Existing Tests

| Module | Test Type | Coverage |
|--------|-----------|----------|
| `llm_trace.rs` | Unit | Payload shape, truncation, redaction |
| `run_observability.rs` | Unit | Event categorization, timeline building |
| `logs.rs` | Unit | Sequence finding, markdown export |

### 5.2 Missing Tests

1. **Actor integration tests** - Assert `llm.call.started/completed/failed` emitted in:
   - Conductor policy flows (`bootstrap_agenda`, `decide_next_action`)
   - Harness decide flows (`Decide`)
   - Watcher flows (`llm_review_window`, `recommend_mitigation`)

2. **WebSocket integration tests** - Assert event ordering in streaming

3. **Trace emitter wiring tests** - Assert emitter is always attached in production constructors

---

## 6. Summary

| Layer | Status | Action Needed |
|-------|--------|---------------|
| Human UX (Trace app) | Complete | Minor UX enhancements optional |
| Headless API | Stable | Consider trace-specific endpoint |
| Harness Readiness | Ready | Integration tests recommended |

The tracing foundation is solid. Human-first tracing workflows are usable and auditable. The API contract is stable enough for harness consumption. No implementation changes were required for this assessment.
