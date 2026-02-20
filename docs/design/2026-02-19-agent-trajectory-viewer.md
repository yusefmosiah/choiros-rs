# Runbook: Agent Trajectory Viewer

**Status:** Active  
**Last Updated:** February 2026  
**Audience:** Agentic engineer executing implementation autonomously

---

## Narrative Summary (1-minute read)

The tracing app in `dioxus-desktop/src/components/trace.rs` currently renders four event types out of ~twenty in the EventStore. This runbook specifies all implementation and testing work to add: inter-agent messaging visualization, worker lifecycle rendering, a trajectory grid (tool × step with duration/token overlays), and run-list enhancements.

Each phase has backend integration tests (Rust, `tower::oneshot` pattern), a Playwright spec against the live UI, and an eval harness that fires a small suite of real prompts and asserts observable event patterns.

Nothing here replaces existing structure. All work is additive.

---

## What Changed

- Converted from PRD to executable runbook with exact file targets, struct signatures, test commands, and eval prompt sets
- Added backend integration tests for each new event parsing path
- Added Playwright specs per phase that assert the UI surfaces the right elements
- Added eval harness spec with concrete prompt suite and assertion criteria

---

## What To Do Next

Execute phases in order. Each phase ends with its tests passing before moving to the next.

1. Phase 1 — Inter-agent messaging (backend parsing + delegation timeline UI)
2. Phase 2 — Worker lifecycle (struct + graph node + lifecycle strip)  
3. Phase 3 — Trajectory grid (tool × step visualization)
4. Phase 4 — Time and token overlays (duration bars, token bars, sparkline)
5. Phase 5 — Eval harness (cross-phase, real prompts, observable assertions)

---

## Architecture Reference

### Correlation model

```
run_id     groups all events for a conductor run
task_id    groups all events for a single worker execution loop
call_id    correlates conductor.worker.call → conductor.capability.*
trace_id   pairs llm.call.started / llm.call.completed / llm.call.failed
tool_trace_id  pairs worker.tool.call / worker.tool.result
```

### Event types currently parsed by trace.rs

```
llm.call.started / llm.call.completed / llm.call.failed
worker.tool.call / worker.tool.result
trace.prompt.received / conductor.task.started   (run grouping only)
conductor.writer.enqueue / conductor.writer.enqueue.failed
```

### Event types to add (full payload schemas in PRD)

```
Phase 1 — Inter-agent:
  conductor.worker.call         { run_id, worker_type, worker_objective }
  conductor.worker.result       { run_id, worker_type, success, result_summary }
  conductor.capability.completed { run_id, capability, data.call_id, data.summary, _meta.lane }
  conductor.capability.failed    { run_id, capability, data.call_id, data.error, data.failure_kind?, _meta.lane }
  conductor.capability.blocked   { run_id, capability, data.call_id, data.reason, _meta.lane }
  conductor.task.completed       { run_id, output_mode, report_path, status }
  conductor.task.failed          { run_id, error_code, error_message, status, failure_kind? }

Phase 2 — Worker lifecycle:
  worker.task.started    { task_id, worker_id, phase, objective, model_used }
  worker.task.progress   { task_id, worker_id, phase, message, model_used? }
  worker.task.completed  { task_id, worker_id, phase, summary }
  worker.task.failed     { task_id, worker_id, phase, error }
  worker.task.finding    { task_id, worker_id, finding_id, claim, confidence, evidence_refs }
  worker.task.learning   { task_id, worker_id, learning_id, insight, confidence }
  harness.progress.received   { correlation_id, run_id, kind, content, metadata }
  worker.report.received      { task_id, worker_id, report }
```

### Key files

```
dioxus-desktop/src/components/trace.rs     — all UI work goes here (~2492 lines)
sandbox/tests/logs_api_test.rs             — model for new backend integration tests
tests/playwright/                          — all new Playwright specs go here
tests/playwright/playwright.config.ts      — baseURL http://127.0.0.1:3000, project "sandbox"
sandbox/src/api/logs.rs                    — GET /logs/events (query by event_type_prefix, run_id)
sandbox/src/api/run_observability.rs       — GET /conductor/runs/:run_id/timeline
```

### Test command reference

```bash
# Backend unit + integration
./scripts/sandbox-test.sh --lib                          # fast unit tests
./scripts/sandbox-test.sh --test logs_api_test           # specific integration binary
cargo test -p sandbox --test trace_viewer_test           # new test binary (Phase 1)

# Playwright (requires both servers running)
just dev-sandbox          # terminal 1: port 8080
just dev-ui               # terminal 2: port 3000
cd tests/playwright && npx playwright test trace-viewer-phase1.spec.ts
cd tests/playwright && npx playwright test trace-viewer-phase2.spec.ts
cd tests/playwright && npx playwright test trace-viewer-phase3.spec.ts
cd tests/playwright && npx playwright test trace-viewer-phase4.spec.ts
cd tests/playwright && npx playwright test trace-viewer-eval.spec.ts

# Run all trace viewer specs
cd tests/playwright && npx playwright test --grep "trace-viewer"
```

---

## Phase 1: Inter-Agent Messaging

### 1.1 New structs in trace.rs

Add after the existing `WriterEnqueueEvent` struct (around line 329):

```rust
#[derive(Clone, Debug)]
struct ConductorDelegationEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    run_id: String,
    // conductor.worker.call / conductor.worker.result
    worker_type: Option<String>,
    worker_objective: Option<String>,
    success: Option<bool>,
    result_summary: Option<String>,
    // conductor.capability.* (control-lane events)
    call_id: Option<String>,        // from data.call_id
    capability: Option<String>,
    error: Option<String>,          // from data.error
    failure_kind: Option<String>,   // from data.failure_kind
    reason: Option<String>,         // from data.reason (blocked)
    lane: Option<String>,           // from _meta.lane: "control" | "telemetry"
}

#[derive(Clone, Debug)]
struct ConductorRunEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    run_id: String,
    phase: Option<String>,
    status: Option<String>,
    message: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}
```

### 1.2 Parse functions in trace.rs

Add after `parse_writer_enqueue_event`:

```rust
fn parse_conductor_delegation_event(event: &LogsEvent) -> Option<ConductorDelegationEvent> {
    let is_delegation = matches!(
        event.event_type.as_str(),
        "conductor.worker.call"
            | "conductor.worker.result"
            | "conductor.capability.completed"
            | "conductor.capability.failed"
            | "conductor.capability.blocked"
    );
    if !is_delegation {
        return None;
    }
    let p = &event.payload;
    let data = p.get("data").unwrap_or(p);
    let meta = p.get("_meta");
    let run_id = payload_run_id(p)?;
    Some(ConductorDelegationEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        worker_type: p.get("worker_type").and_then(|v| v.as_str()).map(|s| s.to_string())
            .or_else(|| p.get("capability").and_then(|v| v.as_str()).map(|s| s.to_string())),
        worker_objective: p.get("worker_objective").and_then(|v| v.as_str()).map(|s| s.to_string()),
        success: p.get("success").and_then(|v| v.as_bool()),
        result_summary: p.get("result_summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
        call_id: data.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        capability: p.get("capability").and_then(|v| v.as_str()).map(|s| s.to_string()),
        error: data.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()),
        failure_kind: data.get("failure_kind").and_then(|v| v.as_str()).map(|s| s.to_string()),
        reason: data.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string()),
        lane: meta.and_then(|m| m.get("lane")).and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

fn parse_conductor_run_event(event: &LogsEvent) -> Option<ConductorRunEvent> {
    let is_run = matches!(
        event.event_type.as_str(),
        "conductor.run.started"
            | "conductor.task.completed"
            | "conductor.task.failed"
            | "conductor.task.progress"
    );
    if !is_run {
        return None;
    }
    let p = &event.payload;
    let run_id = payload_run_id(p)?;
    Some(ConductorRunEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        run_id,
        phase: p.get("phase").and_then(|v| v.as_str()).map(|s| s.to_string()),
        status: p.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        message: p.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()),
        error_code: p.get("error_code").and_then(|v| v.as_str()).map(|s| s.to_string()),
        error_message: p.get("error_message").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}
```

### 1.3 Wire into the main state loop in trace.rs

The main component processes `LogsEvent` items from the WebSocket stream and backfill. Add the new parsers alongside the existing ones. Add state signals:

```rust
let delegation_events: Signal<Vec<ConductorDelegationEvent>> = use_signal(Vec::new);
let run_events: Signal<Vec<ConductorRunEvent>> = use_signal(Vec::new);
```

In the event ingestion loop, call both new parsers and push to the signals, similar to the existing `writer_enqueues` accumulation.

### 1.4 RunGraphSummary additions

Add to `RunGraphSummary`:

```rust
worker_calls: usize,
capability_failures: usize,
run_status: String,   // "completed" | "failed" | "in-progress"
```

Derive `run_status` from `ConductorRunEvent`: `conductor.task.completed` → "completed", `conductor.task.failed` → "failed", else "in-progress".

Derive `worker_calls` and `capability_failures` from `ConductorDelegationEvent` counts per `run_id`.

In the run list card render, add a status badge pill (green / red / yellow) and the worker calls count.

### 1.5 Delegation timeline in Run Detail

In the run detail view (rendered when a run is selected), add a **Delegation Timeline** section above the existing node chip row.

Render as a horizontal sequence of bands. Each band corresponds to one `conductor.worker.call` event:

- Label: `worker_type` value
- Right side: outcome badge (completed / failed / blocked)
- Duration: timestamp delta between the `.call` and the matching `.capability.*` event (matched by `call_id`)
- Color: green = completed, red = failed, amber = blocked
- On click: scroll to the loop group in the accordion that matches (see Open Question 1 — for v1, match by `call_id` present on `worker.task.*` events once Phase 2 lands; until then, show the band without scroll-link)
- Expand on hover: show `worker_objective` text (truncated to 200 chars)

CSS class: `trace-delegation-timeline`, `trace-delegation-band`, `trace-delegation-band--failed`, `trace-delegation-band--blocked`.

### 1.6 Delegation edges on SVG graph

In the SVG graph render function, after drawing existing nodes, add edges for `conductor.worker.call` events attributed to the current run:

- Source: Conductor node (existing)
- Target: worker type label — create a node if not already present (same positioning logic as `GraphNodeKind::Actor`)
- Edge label: `worker_type` value
- Edge color: green if a matching `capability.completed` exists, red if `capability.failed`, amber if `capability.blocked`, grey if no terminal yet
- Dashed stroke for blocked edges

### 1.7 Backend integration test

Create `sandbox/tests/trace_viewer_test.rs`. Pattern from `logs_api_test.rs`.

```rust
// Tests to write:

// test_delegation_events_are_queryable
// Seeds EventStore with conductor.worker.call, conductor.capability.completed (with call_id match)
// and conductor.capability.failed events. Calls GET /logs/events?event_type_prefix=conductor.worker
// and GET /logs/events?event_type_prefix=conductor.capability. Asserts correct counts and
// that payload fields (run_id, worker_type, call_id, lane, success) round-trip correctly.

// test_run_status_derivable_from_terminal_events
// Seeds: conductor.task.started, then conductor.task.completed for run_id_a;
//        conductor.task.started, then conductor.task.failed for run_id_b.
// Calls GET /logs/events?run_id=run_id_a and GET /logs/events?run_id=run_id_b.
// Asserts that the terminal event for each run carries the correct status field.

// test_capability_call_id_correlation
// Seeds: conductor.worker.call (no call_id — call_id is absent from this event type
// currently; test documents the current state and asserts the fields that ARE present).
// Seeds: conductor.capability.completed with data.call_id set.
// Verifies that GET /logs/events?event_type_prefix=conductor.capability returns the
// event with data.call_id intact in the payload.
```

Run: `./scripts/sandbox-test.sh --test trace_viewer_test`

### 1.8 Playwright spec

Create `tests/playwright/trace-viewer-phase1.spec.ts`:

```typescript
// Requires: just dev-sandbox + just dev-ui running

import { test, expect } from "@playwright/test";

const BACKEND = "http://127.0.0.1:8080";

// Helper reused across all trace-viewer specs
async function fetchEvents(request: any, prefix: string, limit = 200) {
  const resp = await request.get(
    `${BACKEND}/logs/events?event_type_prefix=${encodeURIComponent(prefix)}&limit=${limit}`
  );
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()).events ?? [];
}

async function triggerRun(request: any, objective: string, desktopId: string) {
  const resp = await request.post(`${BACKEND}/conductor/execute`, {
    data: { objective, desktop_id: desktopId, output_mode: "markdown_report_to_writer" },
  });
  expect(resp.status()).toBeLessThan(500);
  const body = await resp.json();
  return body.run_id ?? body.data?.run_id ?? null;
}

// Test 1: delegation events are emitted for a real conductor run
test("conductor.worker.call is emitted for a delegating prompt", async ({ request }) => {
  const desktopId = `trace-p1-delegation-${Date.now()}`;
  await triggerRun(request, "List the files in the src directory.", desktopId);

  // Wait up to 60s for at least one conductor.worker.call
  const deadline = Date.now() + 60_000;
  let events: any[] = [];
  while (Date.now() < deadline) {
    events = await fetchEvents(request, "conductor.worker.call");
    if (events.length > 0) break;
    await new Promise((r) => setTimeout(r, 1_500));
  }
  expect(events.length).toBeGreaterThan(0);
  const ev = events[0];
  expect(typeof ev.payload.run_id).toBe("string");
  expect(typeof ev.payload.worker_type).toBe("string");
  expect(ev.payload.worker_type.length).toBeGreaterThan(0);
});

// Test 2: delegation timeline renders in the trace UI
test("delegation timeline band appears in trace window after run", async ({ page, request }) => {
  const desktopId = `trace-p1-ui-${Date.now()}`;
  await triggerRun(request, "Summarize the Justfile.", desktopId);

  await page.goto("/");

  // Open trace window
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  const traceWindowTitle = page.locator(".floating-window .window-titlebar span")
    .filter({ hasText: "Trace" }).first();
  await expect(traceWindowTitle).toBeVisible({ timeout: 15_000 });

  // Wait for delegation timeline to appear (any band)
  await expect(page.locator(".trace-delegation-band").first())
    .toBeVisible({ timeout: 90_000 });
});

// Test 3: run status badge reflects terminal state
test("run status badge shows completed or failed (not in-progress) after run finishes", async ({ page, request }) => {
  const desktopId = `trace-p1-status-${Date.now()}`;
  await triggerRun(request, "Echo hello world.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  // Poll for a terminal status badge
  await expect.poll(async () => {
    const completed = await page.locator(".trace-run-status--completed").count();
    const failed = await page.locator(".trace-run-status--failed").count();
    return completed + failed;
  }, { timeout: 120_000, intervals: [2000] }).toBeGreaterThan(0);
});
```

---

## Phase 2: Worker Lifecycle

### 2.1 New struct in trace.rs

Add after `ConductorRunEvent`:

```rust
#[derive(Clone, Debug)]
struct WorkerLifecycleEvent {
    seq: i64,
    event_id: String,
    event_type: String,
    timestamp: String,
    worker_id: String,
    task_id: String,
    phase: String,
    run_id: Option<String>,
    // started
    objective: Option<String>,
    model_used: Option<String>,
    // progress
    message: Option<String>,
    // completed
    summary: Option<String>,
    // failed
    status: Option<String>,
    error: Option<String>,
    // finding
    finding_id: Option<String>,
    claim: Option<String>,
    confidence: Option<f64>,
    // learning
    learning_id: Option<String>,
    insight: Option<String>,
}
```

### 2.2 Parse function

```rust
fn parse_worker_lifecycle_event(event: &LogsEvent) -> Option<WorkerLifecycleEvent> {
    let is_lifecycle = matches!(
        event.event_type.as_str(),
        "worker.task.started"
            | "worker.task.progress"
            | "worker.task.completed"
            | "worker.task.failed"
            | "worker.task.finding"
            | "worker.task.learning"
    );
    if !is_lifecycle {
        return None;
    }
    let p = &event.payload;
    let task_id = p.get("task_id").and_then(|v| v.as_str())?.to_string();
    let worker_id = p.get("worker_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    Some(WorkerLifecycleEvent {
        seq: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        worker_id,
        task_id,
        phase: p.get("phase").and_then(|v| v.as_str()).unwrap_or("agent_loop").to_string(),
        run_id: payload_run_id(p),
        objective: p.get("objective").and_then(|v| v.as_str()).map(|s| s.to_string()),
        model_used: p.get("model_used").and_then(|v| v.as_str()).map(|s| s.to_string()),
        message: p.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()),
        summary: p.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
        status: p.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        error: p.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()),
        finding_id: p.get("finding_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        claim: p.get("claim").and_then(|v| v.as_str()).map(|s| s.to_string()),
        confidence: p.get("confidence").and_then(|v| v.as_f64()),
        learning_id: p.get("learning_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        insight: p.get("insight").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}
```

### 2.3 Wire state signal + accumulate in event loop

```rust
let worker_lifecycle: Signal<Vec<WorkerLifecycleEvent>> = use_signal(Vec::new);
```

Accumulate in the same ingestion loop as delegation events.

### 2.4 Worker summary per task_id

Add a helper that computes the current status of a worker from its lifecycle events:

```rust
fn worker_summary(task_id: &str, events: &[WorkerLifecycleEvent]) -> (&'static str, Option<&str>) {
    // Returns (status, latest_message)
    // status: "running" | "completed" | "failed"
    let task_events: Vec<_> = events.iter().filter(|e| e.task_id == task_id).collect();
    let terminal = task_events.iter().rev().find(|e| {
        matches!(e.event_type.as_str(), "worker.task.completed" | "worker.task.failed")
    });
    // ...
}
```

### 2.5 Worker node in GraphNodeKind

Add `Worker` variant to the existing `GraphNodeKind` enum. Add `worker_id` and `task_id` fields to `GraphNode`. Extend `build_run_graph` to emit worker nodes for each distinct `worker_id` seen in `worker_lifecycle` for the run. Position worker nodes as a third column to the right of the Actor nodes.

Add `worker_count` and `worker_failures` to `RunGraphSummary`.

### 2.6 Lifecycle strip in loop accordion

Each `TraceLoopGroup` is keyed by `loop_id` (= `task_id` or `call_id`). When `task_id` is present, look up matching `WorkerLifecycleEvent` entries and render a lifecycle strip at the top of the loop group:

```
[started: "analyze repo structure"] → [progress: "reading files"] → [completed: "3 files analyzed"]
```

Each chip:
- Phase color: grey = started, blue = progress, green = completed, red = failed, amber = finding, teal = learning
- Expanded on click: shows full `objective` / `message` / `summary` / `error` / `claim` / `insight`
- CSS: `trace-lifecycle-chip`, `trace-lifecycle-chip--completed`, `trace-lifecycle-chip--failed`, etc.

### 2.7 Backend integration test additions to trace_viewer_test.rs

```rust
// test_worker_lifecycle_events_round_trip
// Seeds: worker.task.started, worker.task.progress x2, worker.task.completed,
//        all with same task_id and worker_id. A separate worker.task.failed with different task_id.
// Asserts: GET /logs/events?event_type_prefix=worker.task returns all 5 events.
// Asserts field integrity: task_id, worker_id, phase, objective, message, summary, error
//   all round-trip correctly from payload.

// test_worker_finding_and_learning_events
// Seeds: worker.task.finding with finding_id, claim, confidence, evidence_refs.
//        worker.task.learning with learning_id, insight, confidence.
// Asserts payload fields are intact on query.

// test_run_graph_includes_worker_counts
// Seeds delegation + lifecycle events for two workers under same run_id.
// Calls GET /conductor/runs/:run_id/timeline.
// Asserts agent_objectives category contains events for both workers.
```

### 2.8 Playwright spec

Create `tests/playwright/trace-viewer-phase2.spec.ts`:

```typescript
// Test 1: worker lifecycle strip appears in loop accordion
test("worker lifecycle strip appears in trace for a delegating run", async ({ page, request }) => {
  const desktopId = `trace-p2-lifecycle-${Date.now()}`;
  // Use a prompt that reliably triggers a worker
  await triggerRun(request, "Read the Justfile and list all available commands.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  // Wait for run to appear, click it
  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();

  // Worker lifecycle chip should appear
  await expect(page.locator(".trace-lifecycle-chip").first())
    .toBeVisible({ timeout: 60_000 });
});

// Test 2: worker node appears in SVG graph
test("worker node appears in SVG agent graph", async ({ page, request }) => {
  const desktopId = `trace-p2-graph-${Date.now()}`;
  await triggerRun(request, "Inspect the src directory structure.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator("svg .trace-worker-node").first())
    .toBeVisible({ timeout: 90_000 });
});

// Test 3: worker count pill on run card
test("run card shows worker count pill after workers are spawned", async ({ page, request }) => {
  const desktopId = `trace-p2-pills-${Date.now()}`;
  await triggerRun(request, "Summarize the top-level files.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-pill").filter({ hasText: /\d+ worker/ }).first())
    .toBeVisible({ timeout: 90_000 });
});
```

---

## Phase 3: Trajectory Grid

### 3.1 New data structure

Add to `trace.rs`:

```rust
#[derive(Clone, Debug)]
struct TrajectoryCell {
    seq: i64,
    step_index: usize,   // derived: position in seq-sorted event list for this run
    row_key: String,     // tool name, or "llm:{actor_key}", or "worker:{worker_id}"
    event_type: String,
    tool_name: Option<String>,
    actor_key: Option<String>,
    status: TrajectoryStatus,
    duration_ms: Option<i64>,
    total_tokens: Option<i64>,
    // navigation target
    loop_id: String,     // task_id or call_id — for scrolling to loop accordion
    item_id: String,     // trace_id or tool_trace_id — for highlighting specific card
}

#[derive(Clone, Debug, PartialEq)]
enum TrajectoryStatus {
    Completed,
    Failed,
    Inflight,
    Blocked,
}
```

Add a builder function:

```rust
fn build_trajectory_cells(
    traces: &[TraceGroup],
    tools: &[ToolTraceEvent],
    lifecycle: &[WorkerLifecycleEvent],
    delegations: &[ConductorDelegationEvent],
    run_id: &str,
) -> Vec<TrajectoryCell>
```

This collects all events for `run_id`, sorts by `seq`, assigns `step_index` (0-based counter), and maps each event to a `TrajectoryCell` with the appropriate `row_key`:
- LLM calls → `"llm:{actor_key}"`
- Tool calls → tool name (from `tool_name` field)
- Worker lifecycle (started/completed/failed) → `"worker:{worker_id}"`
- Delegation calls → `"delegation:{worker_type}"`

### 3.2 Grid render component

Add a `#[component] fn TrajectoryGrid(cells: Vec<TrajectoryCell>, display_mode: TrajectoryMode)` component.

```rust
#[derive(Clone, Copy, PartialEq)]
enum TrajectoryMode {
    Status,
    Duration,
    Tokens,
}
```

Render as an HTML `<table>` or SVG grid (SVG preferred for fine-grained dot sizing):
- One `<row>` per unique `row_key`, sorted: LLM rows first (alphabetical by actor_key), then tool rows (alphabetical), then worker rows
- One column per `step_index` (or bucketed at N steps per column for long runs)
- Each cell: a `<circle>` if a `TrajectoryCell` exists at `(row_key, step_index)`
  - Status mode: `fill` = green / red / yellow / amber per `TrajectoryStatus`
  - Duration mode: `r` = log-scaled from `duration_ms`; add red ring if > threshold (5000ms default)
  - Tokens mode: `r` = log-scaled from `total_tokens`; only meaningful for LLM rows
- Mode toggle: three pills above the grid (`Status | Duration | Tokens`)
- Click handler on each `<circle>`: sets a selected span state that scrolls the loop accordion to the matching `loop_id` + highlights `item_id`

CSS classes: `trace-traj-grid`, `trace-traj-row-label`, `trace-traj-cell--completed`, `trace-traj-cell--failed`, `trace-traj-cell--inflight`, `trace-traj-cell--blocked`.

Threshold ring: `<circle class="trace-traj-slow-ring">` rendered as a concentric circle with red stroke when `duration_ms > threshold`.

### 3.3 Bucketing for long runs

If `step_count > 80`, bucket into columns of `ceil(step_count / 80)` steps each. For each bucket, show the worst status across all cells in that bucket (red > amber > yellow > green). Show step range in tooltip.

### 3.4 Wire into Run Detail

In the run detail render block, after the delegation timeline (Phase 1) and before the node chip row, insert:

```rust
rsx! {
    TrajectoryGrid {
        cells: trajectory_cells_for_run(selected_run_id, &all_cells),
        display_mode: *trajectory_mode.read(),
    }
}
```

Add a `Signal<TrajectoryMode>` to the component state.

### 3.5 Backend integration test additions

```rust
// test_trajectory_cells_build_correctly
// Seeds: 3 llm.call.completed events + 2 worker.tool.result events all with same run_id,
//        ordered by seq. One tool.result has success=false.
// In-process: calls build_trajectory_cells() directly (this is a pure function — no HTTP needed).
// Asserts: correct step_index assignments, correct row_keys, correct statuses,
//          failed cell has TrajectoryStatus::Failed, others Completed.

// test_trajectory_cells_long_run_bucketing
// Seeds 120 tool calls with sequential seqs.
// Asserts that build_trajectory_cells returns 120 cells but bucket_cells(cells, 80)
// reduces to 80 columns with correct worst-status aggregation.
```

Note: `build_trajectory_cells` and `bucket_cells` are pure functions on `trace.rs` data types. Test them as lib tests in `dioxus-desktop/src/components/trace.rs` using `#[cfg(test)]` inline blocks, not as a separate integration test binary. Run via `cargo test -p sandbox-ui --lib` (or the crate name for dioxus-desktop).

### 3.6 Playwright spec

Create `tests/playwright/trace-viewer-phase3.spec.ts`:

```typescript
test("trajectory grid appears in run detail", async ({ page, request }) => {
  const desktopId = `trace-p3-grid-${Date.now()}`;
  await triggerRun(request, "List all Rust source files in the sandbox/src directory.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();

  // Grid should appear
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });
  // At least one cell should be present
  await expect(page.locator(".trace-traj-grid circle, .trace-traj-grid .trace-traj-cell").first())
    .toBeVisible({ timeout: 15_000 });
});

test("trajectory grid mode toggle switches between Status and Duration", async ({ page, request }) => {
  const desktopId = `trace-p3-mode-${Date.now()}`;
  await triggerRun(request, "Read Cargo.toml and list workspace members.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });

  // Click Duration mode
  await page.locator("text=Duration").first().click();
  // Grid still present, no crash
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 5_000 });
  await page.screenshot({ path: "../artifacts/playwright/traj-grid-duration.png", fullPage: true });
});

test("clicking a trajectory cell scrolls to corresponding span card", async ({ page, request }) => {
  const desktopId = `trace-p3-click-${Date.now()}`;
  await triggerRun(request, "Read the README if one exists.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });

  const firstCell = page.locator(".trace-traj-grid circle, .trace-traj-cell").first();
  await firstCell.click();
  // A span card should be highlighted or scrolled into view
  await expect(page.locator(".trace-call-card.highlighted, .trace-call-card--selected").first())
    .toBeVisible({ timeout: 5_000 });
});
```

---

## Phase 4: Time and Token Overlays

### 4.1 Duration bar in call cards

In the existing `trace-call-card` render for both `TraceGroup` (LLM) and `ToolTracePair` (tool), add a thin `<div class="trace-duration-bar">` element below the card header. Width is `duration_ms / max_duration_in_loop * 100%`. Color: green for fast, red if `duration_ms > threshold`.

Max duration is computed per loop group (not globally) to preserve meaningful relative width within context.

CSS:
```css
.trace-duration-bar {
    height: 3px;
    border-radius: 2px;
    background: #22c55e;
    margin-top: 4px;
    transition: width 0.2s;
}
.trace-duration-bar--slow {
    background: #ef4444;
}
```

### 4.2 Token bar in LLM call cards

In `TraceGroup` cards where token data is present, add a stacked token bar. Three segments:
- Cached input tokens: `#6366f1` (indigo)  
- Input tokens (non-cached): `#3b82f6` (blue)
- Output tokens: `#22c55e` (green)

Width of each segment: `token_count / total_tokens_in_loop_llm_calls * 100%`.

Show numerical totals as `128K in / 2.1K out / 64K cached` in small text beside the bar.

CSS: `trace-token-bar`, `trace-token-segment--cached`, `trace-token-segment--input`, `trace-token-segment--output`.

### 4.3 Run list sparkline

In the `RunGraphSummary` row render, add a `<svg class="trace-run-sparkline">` to the right of the pills. Fixed width 120px, height 16px. Plot all `ToolTracePair` and `TraceGroup` events for the run as dots left-to-right by `seq`, using the same status colors. Dot radius 3px. No Y-axis.

To avoid performance issues when the runs list is long, limit sparkline data to the first 60 events per run.

### 4.4 Aggregate pills

Add to `RunGraphSummary` and render as pills:
- `total_duration_ms`: sum of `duration_ms` across all LLM + tool events for the run. Format: `42.3s`
- `total_tokens`: sum of `total_tokens` across all `llm.call.completed` events. Format: `128K tok`

### 4.5 Backend integration test additions

```rust
// test_duration_ms_present_on_completed_events
// Seeds llm.call.completed with duration_ms = 1234, worker.tool.result with duration_ms = 567.
// Queries and asserts duration_ms survives the round-trip.

// test_token_counts_present_on_llm_completed
// Seeds llm.call.completed with usage.input_tokens=100, usage.output_tokens=50,
//   usage.cached_input_tokens=25, usage.total_tokens=150.
// Queries and asserts all four token fields round-trip.
```

### 4.6 Playwright spec

Create `tests/playwright/trace-viewer-phase4.spec.ts`:

```typescript
test("duration bar appears in span cards", async ({ page, request }) => {
  const desktopId = `trace-p4-dur-${Date.now()}`;
  await triggerRun(request, "Read the top-level Cargo.toml.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();
  await expect(page.locator(".trace-duration-bar").first()).toBeVisible({ timeout: 30_000 });
});

test("sparkline appears on run list rows", async ({ page, request }) => {
  const desktopId = `trace-p4-spark-${Date.now()}`;
  await triggerRun(request, "List the contents of the sandbox directory.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-run-sparkline").first())
    .toBeVisible({ timeout: 90_000 });
});

test("total duration and token pills appear on run cards", async ({ page, request }) => {
  const desktopId = `trace-p4-pills-${Date.now()}`;
  await triggerRun(request, "Describe the actor system architecture.", desktopId);

  await page.goto("/");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" }).first();
  await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
  await traceLauncher.click();

  await expect(page.locator(".trace-pill").filter({ hasText: /tok/ }).first())
    .toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".trace-pill").filter({ hasText: /\d+\.\d+s/ }).first())
    .toBeVisible({ timeout: 90_000 });
});
```

---

## Phase 5: Eval Harness

This phase validates the complete trajectory viewer across a small prompt suite using real model calls. Assertions are behavioral: check that the right events were emitted and that the UI surfaces the right elements, not that model output has specific content.

### 5.1 Eval prompt suite

The prompts are chosen to exercise specific event paths with high reliability. They do not require internet access (no weather/external API calls).

```typescript
// tests/playwright/trace-viewer-eval.spec.ts

const EVAL_PROMPTS: Array<{
  id: string;
  prompt: string;
  expects_delegation: boolean;   // expect conductor.worker.call
  expects_worker_lifecycle: boolean;  // expect worker.task.started/completed
  expects_tool_calls: boolean;   // expect worker.tool.call
  min_llm_calls: number;
}> = [
  {
    id: "file-listing",
    prompt: "List all Rust source files in the sandbox/src/actors directory.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
  {
    id: "cargo-inspect",
    prompt: "Read the workspace Cargo.toml and summarize the member crates and key dependencies.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
  {
    id: "short-answer",
    prompt: "What is 2 + 2? Answer briefly.",
    expects_delegation: false,   // may or may not delegate; conductor may answer directly
    expects_worker_lifecycle: false,
    expects_tool_calls: false,
    min_llm_calls: 1,
  },
  {
    id: "multi-file",
    prompt: "Read both the Justfile and the top-level README if it exists. Summarize what you find.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 2,
  },
  {
    id: "structure-summary",
    prompt: "Describe the high-level structure of the sandbox/src directory tree.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
];
```

### 5.2 Per-prompt eval assertions

```typescript
for (const scenario of EVAL_PROMPTS) {
  test(`eval: ${scenario.id} — event emission and UI visibility`, async ({ page, request }) => {
    const desktopId = `eval-${scenario.id}-${Date.now()}`;

    // Trigger run
    const runResp = await request.post(`${BACKEND}/conductor/execute`, {
      data: { objective: scenario.prompt, desktop_id: desktopId, output_mode: "markdown_report_to_writer" },
    });
    expect(runResp.status()).toBeLessThan(500);

    // Wait for run to reach terminal state: conductor.task.completed or conductor.task.failed
    const terminalEvent = await waitForEvent(
      request,
      "conductor.task",
      (ev) =>
        (ev.event_type === "conductor.task.completed" || ev.event_type === "conductor.task.failed") &&
        ev.payload?.desktop_id === desktopId,
      180_000
    );
    expect(terminalEvent).toBeTruthy();
    const runId = terminalEvent.payload.run_id;
    expect(typeof runId).toBe("string");

    // Assert delegation events if expected
    if (scenario.expects_delegation) {
      const delegationEvents = await fetchEvents(request, "conductor.worker.call", 500);
      const forThisRun = delegationEvents.filter((e: any) => e.payload?.run_id === runId);
      expect(forThisRun.length).toBeGreaterThan(0);
    }

    // Assert worker lifecycle events if expected
    if (scenario.expects_worker_lifecycle) {
      const lifecycleEvents = await fetchEvents(request, "worker.task", 500);
      const started = lifecycleEvents.filter(
        (e: any) => e.event_type === "worker.task.started" && e.payload?.run_id === runId
      );
      expect(started.length).toBeGreaterThan(0);
    }

    // Assert tool calls if expected
    if (scenario.expects_tool_calls) {
      const toolEvents = await fetchEvents(request, "worker.tool.call", 500);
      const forThisRun = toolEvents.filter((e: any) => e.payload?.run_id === runId);
      expect(forThisRun.length).toBeGreaterThan(0);
    }

    // Assert minimum LLM calls
    const llmEvents = await fetchEvents(request, "llm.call.completed", 500);
    const forThisRun = llmEvents.filter((e: any) => e.payload?.run_id === runId);
    expect(forThisRun.length).toBeGreaterThanOrEqual(scenario.min_llm_calls);

    // Assert timeline endpoint returns structured data
    const timelineResp = await request.get(`${BACKEND}/conductor/runs/${runId}/timeline`);
    expect(timelineResp.ok()).toBeTruthy();
    const timeline = await timelineResp.json();
    expect(timeline.run_id).toBe(runId);
    expect(Array.isArray(timeline.events)).toBeTruthy();
    expect(timeline.events.length).toBeGreaterThan(0);

    // UI: open trace window and verify run appears with trajectory grid
    await page.goto("/");
    const traceLauncher = page.locator("button, [role='button'], .desktop-icon")
      .filter({ hasText: "Trace" }).first();
    await expect(traceLauncher).toBeVisible({ timeout: 30_000 });
    await traceLauncher.click();

    // Run card should appear in the list
    await expect.poll(async () => {
      const bodyText = await page.locator("body").innerText();
      return bodyText.includes(runId.slice(0, 8)) || bodyText.includes(scenario.prompt.slice(0, 30));
    }, { timeout: 30_000, intervals: [1000] }).toBeTruthy();

    await page.locator(".trace-run-toggle").first().click();

    // Trajectory grid should appear
    await expect(page.locator(".trace-traj-grid").first())
      .toBeVisible({ timeout: 15_000 });

    // Delegation timeline should appear if delegation was expected
    if (scenario.expects_delegation) {
      await expect(page.locator(".trace-delegation-band").first())
        .toBeVisible({ timeout: 15_000 });
    }

    // Worker lifecycle chip if expected
    if (scenario.expects_worker_lifecycle) {
      await expect(page.locator(".trace-lifecycle-chip").first())
        .toBeVisible({ timeout: 15_000 });
    }

    await page.screenshot({
      path: `../artifacts/playwright/eval-${scenario.id}.png`,
      fullPage: true,
    });
  });
}
```

### 5.3 Aggregate pass rate assertion

```typescript
// Run at the end of the eval suite to enforce overall pass rate.
// Individual test failures are captured above; this test enforces the aggregate.

test("eval: aggregate pass rate >= 4/5 prompts", async ({ request }) => {
  // This test is structural — it passes if at least 4 of the 5 scenario tests passed.
  // In practice, Playwright's --reporter=list output captures this.
  // This test exists to make the gate explicit in CI output.
  const resp = await request.get(`${BACKEND}/health`);
  expect(resp.ok()).toBeTruthy();
  // Actual pass-rate enforcement is via the Playwright reporter exit code.
  // Set maxFailures: 1 in playwright.config.ts for the eval project.
});
```

### 5.4 Eval project in playwright.config.ts

Add a third project to the config:

```typescript
{
  name: "trace-eval",
  testMatch: ["trace-viewer-eval.spec.ts"],
  use: {
    baseURL: "http://127.0.0.1:3000",
    trace: "on",
    video: "on",
    screenshot: "on",
    viewport: { width: 1720, height: 980 },
  },
  // Allow 1 failure out of 5 scenarios
  // (set via --max-failures=1 flag when running)
},
```

Run the eval suite:

```bash
cd tests/playwright
npx playwright test trace-viewer-eval.spec.ts --max-failures=1 --reporter=list
```

Artifacts: per-scenario screenshots at `tests/artifacts/playwright/eval-{id}.png`, trace zips at `tests/artifacts/playwright/test-results/`.

---

## Open Questions (resolution required before implementation)

### OQ-1: `conductor.worker.call` → `task_id` linkage

`conductor.worker.call` carries `worker_type` and `run_id` but no `task_id`. The `task_id` is only on `worker.task.started`. To link a delegation band to the correct loop accordion entry, one of:

**Option A (backend change):** Add `call_id` to the `worker.task.started` payload in `agent_harness/mod.rs`. This is a one-line addition and makes the link explicit. **Recommended.**

**Option B (frontend inference):** Match by temporal proximity: find the `worker.task.started` event whose `timestamp` is closest to the `conductor.worker.call` with the same `worker_type` and `run_id`. Fragile if multiple workers of the same type run concurrently.

Decision needed before Phase 1 delegation-band click navigation is implemented.

### OQ-2: Trajectory grid X-axis

**Option A (global):** All events for the run laid out by `seq`. Step N for row "read" and step N for row "llm:conductor" refer to the same moment. Temporal alignment across workers is preserved. Long runs have sparse rows.

**Option B (per-loop):** X resets at each `task_id` boundary. Each worker's loop is normalized independently. Workers of the same type at different times are not directly comparable.

Recommendation: global for v1, with loop boundary dividers. Simpler to implement, better for cross-worker timing inspection.

### OQ-3: Duration threshold configurability

Hardcode 5000ms for v1, stored as a `const` in `trace.rs`. Add user-configurable override via a local `Signal<i64>` in Phase 4 if 5000ms proves wrong in practice.

### OQ-4: Conductor tool calls in the trajectory grid

`conductor.tool.call` / `conductor.tool.result` exist alongside `worker.tool.call` / `worker.tool.result`. They should appear in the grid as separate rows: `"conductor-tool:{tool_name}"` vs `"tool:{tool_name}"`. This avoids mixing conductor-direct tool calls with delegated worker tool calls in the same row.

---

## Verification Checklist

Before marking each phase complete:

- [ ] `./scripts/sandbox-test.sh --test trace_viewer_test` passes
- [ ] `cargo clippy -p sandbox -- -D warnings` clean
- [ ] `cargo fmt --check -p sandbox` clean  
- [ ] Phase Playwright spec passes with both servers running
- [ ] Screenshot artifact committed to `tests/artifacts/playwright/` (or reviewed and discarded)
- [ ] No new panics in `just dev-sandbox` logs during Playwright run

After Phase 5:

- [ ] All 5 eval scenarios pass (max 1 failure tolerated)
- [ ] Per-scenario screenshots reviewed for visual correctness
- [ ] Trace zip artifacts inspected for at least one scenario to verify no flakiness in element selectors
