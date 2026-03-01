# Simplified Multiagent Comms: Implementation Runbook

**Date:** 2026-02-13  
**Source Contract:** `docs/handoffs/2026-02-13-simplified-multiagent-comms-architecture.md`  
**Tracing Runbook:** `docs/handoffs/2026-02-13-llm-tracing-runbook.md`  
**Status:** Slice A/B complete; Slice C/D/E pending

## Narrative Summary (1-minute read)

This runbook translates the communication contract into code-ready slices that can ship safely.

Execution order is strict:
1. Remove document log-spam path (`AppendLogLine`) from normal worker streaming.
2. Enforce terminal/researcher role boundaries via capability-correct prompting/contracts.
3. De-scope watcher wake from normal healthy run progression.
4. Cut legacy flat tool args and enforce nested typed args only.
5. Then build revision back/forward navigation.

The first three slices are control and reliability work. Revision UX and WriterAgent come after the runtime contract is stable.

## What Changed

- Added concrete PR sequence with file-level ownership and acceptance gates.
- Added baseline test snapshot before cutover.
- Added explicit "done" criteria for each slice to prevent partial migration.

## What To Do Next

1. Keep Slice A/B as complete and stable (no regression).
2. Implement a minimal LLM tracing surface before Slice C/D/E so model-call behavior is visible and debuggable.
   - Execute: `docs/handoffs/2026-02-13-llm-tracing-runbook.md`
3. Execute Slice C with tracing active to validate watcher escalation-only behavior.
4. Execute Slice D with tracing active to verify strict nested tool args and rejection paths.
5. Execute Slice E after C/D contracts are stable.

---

## Completion Snapshot (A/B)

Commands run:

```bash
cargo test -p sandbox --test run_writer_contract_test -- --nocapture
cargo test -p sandbox --test capability_boundary_test -- --nocapture
```

Results:
- `run_writer_contract_test`: 10 passed, 0 failed.
- `capability_boundary_test`: 2 passed, 0 failed.

A/B completion observations:
- Worker progress streaming no longer mutates run document via `AppendLogLine` in normal path; conductor now emits non-mutating section progress ticks.
- Terminal/researcher capability boundaries are now explicit in conductor objective shaping and worker system contexts.
- Deterministic loop guards were intentionally not introduced in Slice B.

## Slice A: Remove Document Log Spam Path

### Goal

Worker progress should update status telemetry, not mutate run document proposal content by default.

### Files

- `sandbox/src/actors/conductor/runtime/decision.rs`
- `sandbox/src/actors/run_writer/messages.rs`
- `sandbox/src/actors/run_writer/mod.rs`
- `sandbox/tests/run_writer_contract_test.rs`
- `dioxus-desktop/src/desktop/state.rs`
- `dioxus-desktop/src/components/writer.rs`

### Changes

1. Stop using `RunWriterMsg::AppendLogLine` in worker progress fanout for both researcher and terminal.
2. Keep `RunWriterMsg::ApplyPatch` as the only worker data-plane mutation path.
3. Emit concise worker status ticks (`worker`, `phase`, `message`) through writer progress/status events.
4. Keep full details in telemetry only (`worker.task.*`, `conductor.*`, `actor_call`).
5. Mark `AppendLogLine` as legacy and remove from normal run path tests.

### Acceptance Gates

- Active run document no longer accumulates timestamped progress log lines by default.
- Writer still shows live per-worker status within seconds.
- `cargo test -p sandbox --test run_writer_contract_test -- --nocapture` passes.

## Slice B: Enforce Role Boundaries

### Goal

Terminal stays local execution focused. Researcher owns external web research.

### Files

- `sandbox/src/actors/terminal.rs`
- `sandbox/src/actors/researcher/adapter.rs`
- `sandbox/src/actors/agent_harness/mod.rs`
- `sandbox/tests/capability_boundary_test.rs`
- `sandbox/tests/e2e_conductor_scenarios.rs`

### Changes

1. Tighten terminal system context to reject generic web-research behavior.
2. Tighten conductor policy prompt context so capability routing is explicit (`researcher` for research, `terminal` for local execution).
3. Keep researcher prompts/tooling as the external source collection lane.
4. Do not add deterministic loop guards in this slice; fix behavior via capability-correct prompting and contracts.

### Acceptance Gates

- Terminal agent no longer enters generic `curl` research loops for objectives that belong to researcher.
- Conductor routing chooses `researcher` for web research objectives when available.
- Capability boundary tests remain green.

## Slice C: Watcher De-scope from Normal Runs

### Goal

Watcher is escalation-only, not routine run step authority.

### Files

- `sandbox/src/main.rs`
- `sandbox/src/actors/watcher.rs`
- `sandbox/src/actors/conductor/actor.rs`
- `sandbox/src/actors/conductor/protocol.rs`
- `sandbox/tests/logs_api_test.rs`

### Changes

1. Default watcher wake-to-conductor path to disabled for healthy runs.
2. Gate wake actions to explicit failure/anomaly classes with high urgency.
3. Preserve watcher review/escalation telemetry events in EventStore.

### Acceptance Gates

- Healthy runs progress through conductor control plane without watcher wake dependency.
- Watcher still emits review/escalation events for observability.
- Failure/anomaly escalation can still wake conductor when enabled.

## Slice D: Strict Tool Schema (Nested Args Only)

### Goal

Remove fallback flat fields and require typed nested tool args.

### Files

- `baml_src/types.baml`
- `sandbox/src/actors/terminal.rs`
- `sandbox/src/actors/researcher/adapter.rs`
- `sandbox/src/baml_client/types/classes.rs` (generated)
- `sandbox/src/baml_client/stream_types/classes.rs` (generated)
- `sandbox/tests/model_provider_live_test.rs`

### Changes

1. Remove flat compatibility fields from `AgentToolArgs`.
2. Remove adapter fallbacks like `.or(tool_call.tool_args.command)` and `.or(args.max_results)`.
3. Return explicit typed validation errors for malformed calls.

### Acceptance Gates

- Malformed/non-nested tool args are rejected deterministically.
- Valid nested calls still execute.
- No legacy fallback parse path remains.

## Slice E: Revision Cursor + Back/Forward UI

### Goal

Linear revision history with deterministic cursor navigation.

### Files

- `sandbox/src/actors/run_writer/state.rs`
- `sandbox/src/actors/run_writer/messages.rs`
- `sandbox/src/actors/run_writer/mod.rs`
- `dioxus-desktop/src/components/writer.rs`
- `dioxus-desktop/src/desktop/state.rs`
- `sandbox/tests/run_writer_contract_test.rs`

### Changes

1. Persist revision snapshots (or snapshot + compaction) with monotonic ids.
2. Add cursor APIs/messages for `revision N` vs live head.
3. Add Writer back/forward arrows and `Viewing revision N of M` banner.
4. Block direct edits while cursor is behind head unless explicit action.

### Acceptance Gates

- Back and forward navigation is deterministic.
- Live head is preserved; no silent divergence from historical cursor edits.
- Revision behavior is test-covered.

## Suggested Execution Rhythm

1. Slice A (smallest high-value cut; low blast radius).
2. Slice B (capability-correct prompting/contracts; no deterministic loop guards).
3. LLM Tracing MVP (new gate before further cutover):
   - Emit first-class `llm.call.started|completed|failed` events for BAML calls.
   - Include model, role, function, system context, input context/messages, output/error, latency.
   - Add a dedicated Trace app/view (or Trace mode in Logs) focused on model-call inspection.
4. Slice C (control-plane hardening with watcher de-scope).
5. Slice D (breaking schema cutover, nested args only).
6. Slice E (revision cursor/navigation UX).

Each slice should land with tests before the next starts.
