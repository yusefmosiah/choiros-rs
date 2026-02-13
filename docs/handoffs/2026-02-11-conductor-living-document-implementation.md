# Handoff: Conductor Living Document Implementation

**Date:** 2026-02-11
**Session Type:** Implementation (Conductor Living Document)
**Status:** Complete - All Acceptance Criteria Met

---

## Narrative Summary

This session implemented the final phase of the agent harness simplification: making the Conductor use the same living document pattern as the Researcher. The Conductor previously produced rigid structured reports that buried the lede and didn't stream live. This work adds file tools to Conductor, implements live document streaming, and culls excessive BAML types.

The key insight: **The document is the state**. Instead of reconstructing complex structured state for BAML inputs, the Conductor now maintains a freeform markdown document at `conductor/runs/{run_id}/draft.md` that serves as both working memory and final report.

---

## What Changed

### 1. BAML Simplification (Major Reduction)

**Before:** ~333 lines with complex structured types
**After:** ~144 lines with minimal types

**Removed Types:**
- `ConductorAgendaItem` - Agenda now lives in document
- `ConductorCapabilityCall` - Call tracking in document
- `ConductorArtifact` - Artifacts section in document
- `WorkerOutput` - Worker results in document
- `EventSummary` - Events as narrative in document
- `FollowupRecommendation` - Next steps in document
- `DecisionType` enum (6 variants) - Replaced with `ConductorAction` (4 variants)
- `TerminalityStatus` - Not needed
- `RetryPolicy` - Simplified
- `ConductorAssessTerminality` function - Covered by `ConductorDecide`
- `ConductorDecisionOutput` - Replaced with `ConductorDecision`

**New Simplified Types:**

```baml
enum ConductorAction {
  SpawnWorker    // Dispatch a capability worker
  UpdateDraft    // Update the living document
  Complete       // Run is done
  Block          // Cannot proceed
}

class ConductorDecision {
  action ConductorAction
  args map<string, string>?
  reason string
}

class ConductorDecisionInput {
  run_id string
  objective string
  document_path string  // Read document for context
  last_error string?
}
```

**Functions Simplified:**
- `ConductorDecideNextAction` → `ConductorDecide` (renamed, minimal input/output)
- `ConductorBootstrapAgenda` - Kept (needed for initial dispatch)
- `ConductorRefineObjective` - Kept (needed for objective tailoring)

### 2. File Tools for Conductor (New Module)

**New File:** `sandbox/src/actors/conductor/file_tools.rs`

Provides sandboxed file operations:
- `file_read(path)` - Read file within sandbox
- `file_write(path, content)` - Write/create file
- `file_edit(path, old_text, new_text)` - Find/replace edit
- `create_initial_draft(run_id, objective)` - Create run document
- `get_run_document_path(run_id)` - Get path helper

Path validation prevents:
- Absolute paths (`/etc/passwd`)
- Path traversal (`../Cargo.toml`)
- Sandbox escape

### 3. Document Emission (Live Streaming)

**File:** `sandbox/src/actors/conductor/events.rs`

Added `emit_document_update()` function:

```rust
pub async fn emit_document_update(
    event_store: &ActorRef<EventStoreMsg>,
    run_id: &str,
    task_id: &str,
    document_path: &str,
    content_excerpt: &str,
)
```

Emits `conductor.run.document_update` events for WebSocket streaming to UI.

### 4. Run State Update

**File:** `shared-types/src/lib.rs`

Added `document_path: String` field to `ConductorRunState`.

### 5. Bootstrap Creates Draft

**File:** `sandbox/src/actors/conductor/runtime/bootstrap.rs`

On run start:
1. Creates `conductor/runs/{run_id}/draft.md`
2. Writes initial content with objective and agenda
3. Stores path in run state

Initial document format:
```markdown
# {Objective}

## Current Understanding

Run started with objective: {objective}

Run ID: `{run_id}`

## In Progress

- [ ] Bootstrap agenda

## Next Steps

Initializing...
```

### 6. Policy Simplification

**File:** `sandbox/src/actors/conductor/policy.rs`

`build_decision_input()` simplified from ~150 lines to ~10 lines:

**Before:**
```rust
ConductorDecisionInput {
    run_id: run.run_id.clone(),
    task_id: run.task_id.clone(),
    objective: run.objective.clone(),
    run_status: format!("{:?}", run.status),
    agenda,              // Complex reconstruction
    active_calls,        // Complex reconstruction
    artifacts,           // Complex reconstruction
    recent_events,       // Complex reconstruction
    worker_outputs,      // Complex reconstruction
}
```

**After:**
```rust
ConductorDecisionInput {
    run_id: run.run_id.clone(),
    objective: run.objective.clone(),
    document_path: run.document_path.clone(),
    last_error: None,
}
```

### 7. Decision Application Updated

**File:** `sandbox/src/actors/conductor/runtime/decision.rs`

Updated `apply_decision()` to handle new `ConductorAction` variants:
- `SpawnWorker` - Creates agenda item and spawns capability
- `UpdateDraft` - Model wants to update document (signals continuation)
- `Complete` - Marks run completed
- `Block` - Marks run blocked

### 8. Report Generation Simplified

**File:** `sandbox/src/actors/conductor/output.rs`

`build_worker_output_from_run()` now simply reads draft.md:

```rust
pub fn build_worker_output_from_run(run: &ConductorRunState) -> WorkerOutput {
    let report_content = match std::fs::read_to_string(&run.document_path) {
        Ok(content) => content,
        Err(e) => format!("# Error\n\nFailed to read report: {}", e),
    };

    WorkerOutput {
        report_content,
        citations: /* extracted from artifacts */,
    }
}
```

### 9. WebSocket Event Routing

**File:** `sandbox/src/api/websocket.rs`

Added `DocumentUpdate` variant to `WsMessage`:

```rust
DocumentUpdate {
    run_id: String,
    document_path: String,
    content_excerpt: String,
    timestamp: String,
},
```

### 10. Frontend Document View (New Component)

**New File:** `dioxus-desktop/src/components/run.rs`

`RunView` component features:
- Live markdown rendering of conductor document
- WebSocket subscription for real-time updates
- Collapsible raw events section (Show Events/Hide Events)
- Connection status indicator (Live/Reconnecting)
- Error handling for connection failures

**Updated Files:**
- `dioxus-desktop/src/desktop/ws.rs` - Added `DocumentUpdate` event parsing
- `dioxus-desktop/src/desktop/state.rs` - Event handling
- `dioxus-desktop/src/desktop/apps.rs` - Added "run" app
- `dioxus-desktop/src/desktop_window.rs` - Window rendering

### 11. Test Updates

**Files:**
- `sandbox/src/actors/conductor/tests/support.rs` - Updated for new types
- `sandbox/src/actors/conductor/state.rs` - Added document_path to test fixtures

---

## Acceptance Criteria Verification

| Criteria | Status | Evidence |
|----------|--------|----------|
| 1. Conductor creates `conductor/runs/{run_id}/draft.md` on run start | ✅ | `file_tools::create_initial_draft()` in bootstrap.rs |
| 2. Document updates live on screen as model writes | ✅ | `RunView` component with WebSocket streaming |
| 3. Report endpoint returns freeform markdown, not structured JSON | ✅ | `build_worker_output_from_run()` reads draft.md directly |
| 4. BAML `ConductorDecisionInput` has < 5 fields (vs current 9+) | ✅ | Now has 4 fields: run_id, objective, document_path, last_error |
| 5. Document can be read on Conductor wake to reconstruct context | ✅ | Model reads document_path to understand state |
| 6. No loss of existing functionality (workers still spawn, complete) | ✅ | All 52 conductor tests pass |

---

## Lines Changed Summary

| Metric | Value |
|--------|-------|
| BAML lines removed | ~190 |
| New Rust files | 2 (`file_tools.rs`, `run.rs`) |
| Rust files modified | ~15 |
| Test files updated | 2 |
| Total tests passing | 140 |

---

## Expected Current ChoirOS State

### Backend (sandbox)

**Compiles:** ✅ Yes
**Tests Pass:** ✅ 140/140

**Key Capabilities:**
1. Conductor creates living document on run start
2. File tools available for Conductor to maintain document
3. Document updates emit WebSocket events
4. Simplified BAML decision loop (minimal input/output)
5. Report generation returns draft.md content

**API Endpoints:**
- `POST /api/conductor/runs` - Creates run with draft.md
- `GET /api/conductor/runs/{id}` - Returns run state with document_path
- `GET /ws` - WebSocket with document_update events

### Frontend (dioxus-desktop)

**Compiles:** ✅ Yes (via `dx build`)

**Key Capabilities:**
1. Run app (🚀 icon) opens run view window
2. Live document rendering via MarkdownViewer
3. Real-time updates via WebSocket
4. Collapsible raw events panel

**Window Props for Run App:**
```json
{
  "app_id": "run",
  "run_id": "01KH5...",
  "document_path": "conductor/runs/01KH5.../draft.md"
}
```

---

## Example: Before vs After

### Decision Input (What BAML Sees)

**Before (~200 tokens of structured data):**
```json
{
  "run_id": "01KH5...",
  "objective": "Research Rust web frameworks",
  "run_status": "Running",
  "agenda": [
    {"id": "item_1", "capability": "researcher", "objective": "...", "status": "completed"}
  ],
  "active_calls": [],
  "artifacts": [...],
  "recent_events": [...],
  "worker_outputs": [...]
}
```

**After (~50 tokens + document):**
```json
{
  "run_id": "01KH5...",
  "objective": "Research Rust web frameworks",
  "document_path": "conductor/runs/01KH5.../draft.md",
  "last_error": null
}
```

The model reads `draft.md` for context:
```markdown
# Research Rust web frameworks

## Current Understanding

Found 3 major frameworks worth comparing:
- **Axum**: Most popular, maintained by Tokio team
...
```

### Report Output

**Before (rigid sections):**
```markdown
# Conductor Report

## Objective
Research Rust web frameworks

## Run
- Run ID: `01KH5...`
- Status: `Completed`

## Agenda
- `item_1` `researcher` `Completed`

## Run Narrative
- Dispatch: Spawning initial capabilities

## Artifacts
- No artifacts produced.
```

**After (freeform):**
```markdown
# Research Rust web frameworks

## Current Understanding

Found 3 major frameworks worth comparing:

- **Axum**: Most popular, maintained by Tokio team, 20k+ GitHub stars
- **Actix-web**: High performance, actor-based, 18k+ stars
- **Rocket**: Ergonomic API, 24k+ stars, v0.5 now stable

Researcher analyzed 15 sources. Key findings:
- Axum has the most active development (daily commits)
- Actix-web has the highest throughput in benchmarks
- Rocket has the best developer experience ratings

## In Progress

- [x] Search for top Rust web frameworks
- [x] Check popularity metrics
- [ ] Verify recent benchmarks

## Next Steps

Spawned terminal to run latest benchmarks...
```

---

## Known Limitations / Next Steps

1. **Document Update Triggering**: Currently model must explicitly write file to trigger update. Could optimize with file watcher.

2. **Worker Drafts**: Worker outputs (researcher/terminal) could also write to their own draft files that Conductor reads.

3. **Document Persistence**: Documents stored in filesystem. For distributed deployment, consider S3 or database storage.

4. **Collab Editing**: Living document could support concurrent editing with operational transforms.

5. **Search/Indexing**: Documents not yet indexed for search across runs.

---

## Testing Commands

```bash
# Backend tests
cargo test -p sandbox --lib conductor

# Full test suite
cargo test -p sandbox

# Build check
cargo check -p sandbox

# Frontend build (in dioxus-desktop/)
cd dioxus-desktop && dx build
```

---

## Documentation References

- `/Users/wiz/choiros-rs/docs/architecture/simplified-agent-harness.md` - Harness architecture
- `/Users/wiz/choiros-rs/docs/architecture/conductor-simplification-plan.md` - Detailed plan
- `/Users/wiz/choiros-rs/docs/prompts/05-conductor-living-document-implementation.md` - Original prompt

---

## Files Changed (Complete List)

### BAML
- `baml_src/conductor.baml` - Simplified types

### Sandbox (Backend)
- `sandbox/src/actors/conductor/file_tools.rs` [NEW]
- `sandbox/src/actors/conductor/mod.rs`
- `sandbox/src/actors/conductor/policy.rs`
- `sandbox/src/actors/conductor/output.rs`
- `sandbox/src/actors/conductor/events.rs`
- `sandbox/src/actors/conductor/protocol.rs`
- `sandbox/src/actors/conductor/runtime/bootstrap.rs`
- `sandbox/src/actors/conductor/runtime/decision.rs`
- `sandbox/src/actors/conductor/runtime/observability.rs`
- `sandbox/src/actors/conductor/tests/support.rs`
- `sandbox/src/actors/conductor/state.rs`
- `sandbox/src/api/websocket.rs`
- `sandbox/src/api/conductor.rs`
- `sandbox/src/baml_client/` - Regenerated

### Dioxus Desktop (Frontend)
- `dioxus-desktop/src/components/run.rs` [NEW]
- `dioxus-desktop/src/components.rs`
- `dioxus-desktop/src/desktop/ws.rs`
- `dioxus-desktop/src/desktop/state.rs`
- `dioxus-desktop/src/desktop/apps.rs`
- `dioxus-desktop/src/desktop_window.rs`

### Shared Types
- `shared-types/src/lib.rs`

---

**End of Handoff Document**
