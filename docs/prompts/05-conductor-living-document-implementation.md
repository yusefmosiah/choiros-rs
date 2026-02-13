# Prompt 05: Conductor Living Document Implementation

## Narrative Summary

This session implements the final phase of the agent harness simplification: making the Conductor use the same living document pattern as the Researcher. The current Conductor produces rigid structured reports that bury the lede and don't stream live. This work adds file tools to Conductor, implements live document streaming, and culls excessive BAML types.

## What Changed (Baseline)

The agent harness has been simplified:
- BAML `Decide` function replaces `PlanAction` + `SynthesizeResponse`
- `AgentDecision` with `Action` enum (ToolCall, Complete, Block)
- 3-state loop instead of 7-state state machine
- Researcher already uses living document (`reports/{task_id}.md`)
- Terminal uses harness for bash execution

See `/Users/wiz/choiros-rs/docs/architecture/simplified-agent-harness.md`

## What To Do Next

Implement Conductor living document model per `/Users/wiz/choiros-rs/docs/architecture/conductor-simplification-plan.md`:

1. Add file tools to Conductor (file_read, file_write, file_edit)
2. Create `conductor/runs/{run_id}/draft.md` on run start
3. Emit `conductor.run.document_update` events
4. Simplify BAML types (remove AgendaItem, CapabilityCall, etc.)
5. Update UI to stream document updates live

## Critical Rules

1. **Use subtasks for all implementation work** - Keep main context free
2. **No string matching** - Use typed events and structured decisions
3. **Minimal structured output** - Document is freeform, BAML is minimal
4. **Living document is source of truth** - State in file, not data structures
5. **Stream everything** - UI shows document updates live

## Implementation Tasks (Use Subtasks)

### Task A: Add File Tools to Conductor

**Subtask A1**: Add file tool execution to Conductor runtime
- Files: `sandbox/src/actors/conductor/runtime/decision.rs`, `output.rs`
- Add `file_read`, `file_write`, `file_edit` tool handlers
- Sandbox path validation (like ResearcherAdapter)

**Subtask A2**: Add document emission to Conductor
- File: `sandbox/src/actors/conductor/output.rs`
- Add `emit_document_update()` function
- Emit `conductor.run.document_update` events

**Subtask A3**: Create draft.md on run start
- File: `sandbox/src/actors/conductor/runtime/bootstrap.rs`
- Create `conductor/runs/{run_id}/draft.md` with initial plan
- Write objective and initial agenda as freeform markdown

### Task B: Simplify Conductor BAML

**Subtask B1**: Cull excessive BAML types
- File: `baml_src/conductor.baml`
- Remove: `ConductorAgendaItem`, `ConductorCapabilityCall`, `ConductorArtifact`, `WorkerOutput`, `EventSummary`, `FollowupRecommendation`, `DecisionType`, `TerminalityStatus`
- Simplify `ConductorDecisionInput` to: run_id, objective, document_path, last_error

**Subtask B2**: Simplify decision output
- Replace `ConductorDecisionOutput` with minimal `ConductorDecision`
- Action enum: SpawnWorker, UpdateDraft, Complete, Block
- Regenerate BAML client

**Subtask B3**: Update policy.rs
- File: `sandbox/src/actors/conductor/policy.rs`
- Simplify `build_decision_input()` to read document instead of reconstructing state
- Remove complex type conversions

### Task C: Live Streaming

**Subtask C1**: Add websocket event routing
- File: `sandbox/src/api/websocket.rs`
- Route `conductor.run.document_update` to UI

**Subtask C2**: Frontend document view
- File: `dioxus-desktop/src/views/run_view.rs` (or new component)
- Render markdown document from events
- Update live as document_update events arrive

**Subtask C3**: Collapsible raw events
- Raw events as drill-down below document
- Semantic run view is default

### Task D: Report Generation

**Subtask D1**: Update report endpoint
- File: `sandbox/src/actors/conductor/output.rs`
- `generate_run_report()` returns draft.md content, not structured report
- Remove `ConductorReport` struct with rigid sections

## Acceptance Criteria

1. Conductor creates `conductor/runs/{run_id}/draft.md` on run start
2. Document updates live on screen as model writes
3. Report endpoint returns freeform markdown, not structured JSON
4. BAML `ConductorDecisionInput` has < 5 fields (vs current 9+)
5. Document can be read on Conductor wake to reconstruct context
6. No loss of existing functionality (workers still spawn, complete)

## Validation

```bash
# Build
cargo check -p sandbox
cargo check -p dioxus-desktop

# Tests
cargo test -p sandbox --lib conductor
cargo test -p sandbox --test conductor_api_test

# E2E manual test
just dev-sandbox
just dev-ui
# Create run, verify document streams live
```

## Documentation References

- `/Users/wiz/choiros-rs/docs/architecture/simplified-agent-harness.md` - Harness architecture
- `/Users/wiz/choiros-rs/docs/architecture/conductor-simplification-plan.md` - Detailed plan
- `/Users/wiz/choiros-rs/docs/testing/e2e-harness-conductor-writer-plan.md` - Test plan

## Final Summary Requirements

Include in handoff:
1. Files changed
2. BAML lines removed
3. Example of freeform document vs old structured report
4. Screenshot or log of live streaming working
5. Tests passing
