# Conductor Simplification Plan

> Historical note: parts of this plan are superseded.
> Current authority: Conductor is orchestration-only and does not execute tools directly.
> Tool schemas are defined once and granted per agent/worker; Writer is canonical for
> living-document/revision mutation authority.
> See `docs/architecture/2026-02-14-capability-ownership-matrix.md`.

## Problem Statement

The current Conductor uses excessive BAML structured output, creating:
1. **Rigid report format** - Structured sections (Objective, Run, Agenda, Artifacts) bury the lede
2. **No live streaming** - Report generated at end, not updated as findings arrive
3. **Complex state reconstruction** - `build_decision_input()` rebuilds huge input from structured state
4. **Token waste** - Massive `ConductorDecisionInput` with agenda, calls, artifacts, events

Example from dev logs showing the bloat:
```
ConductorDecisionInput {
  agenda: [ConductorAgendaItem { id, capability, objective, dependencies, status, priority }, ...],
  active_calls: [ConductorCapabilityCall { call_id, agenda_item_id, capability, objective, status }, ...],
  artifacts: [ConductorArtifact { artifact_id, name, content_type, summary }, ...],
  recent_events: [EventSummary { event_id, event_type, timestamp, payload }, ...],
  worker_outputs: [WorkerOutput { call_id, agenda_item_id, status, result_summary, ... }, ...]
}
```

## Target State

### Living Document Model

Conductor maintains `conductor/runs/{run_id}/draft.md`:

```markdown
# Run: What's Going On (Song)

## Current Understanding

Marvin Gaye's "What's Going On" (1971) is a landmark protest song. 
Researcher found 47 sources. Key findings so far:

- Originally conceived by Four Tops member Obie Benson after witnessing 
  police brutality in Berkeley (1969)
- Berry Gordy initially rejected it as "the worst piece of crap I ever heard"
- Ranked #4 on Rolling Stone's 500 Greatest Songs

## In Progress

- [x] Research song background
- [ ] Check chart performance details
- [ ] Find notable covers

## Next Steps

Dispatching terminal to verify chart data from Billboard archives...
```

### Simplified Decision

```baml
class ConductorDecision {
  action ConductorAction
  args map<string, string>?
  reason string
}

enum ConductorAction {
  SpawnWorker    // Spawn a capability worker
  UpdateDraft    // Update the living document
  Complete       // Run is done
  Block          // Cannot proceed
}

function Decide(input: ConductorDecisionInput) -> ConductorDecision {
  // Minimal input - just objective and document_path
}
```

### ConductorDecisionInput (Minimal)

```baml
class ConductorDecisionInput {
  run_id string
  objective string
  document_path string  // Path to draft.md
  last_error string?    // If previous action failed
}
```

The **document contains all context**. Conductor reads its draft, decides, updates draft.

## Implementation Steps

### Phase 1: Add File Tools to Conductor (Task 15)

**Files to modify:**
- `sandbox/src/actors/conductor/output.rs` - Add document emission
- `sandbox/src/actors/conductor/policy.rs` - Add file tool execution

**Changes:**
1. Add `file_read`, `file_write`, `file_edit` capabilities to Conductor
2. Create `conductor/runs/{run_id}/draft.md` on run start
3. Add `emit_document_update` like ResearcherAdapter has

**BAML additions:**
```baml
// In conductor.baml
class ConductorFileToolArgs {
  operation string  // read, write, edit
  path string
  content string?   // for write
  old_text string?  // for edit
  new_text string?  // for edit
}
```

### Phase 2: Live Document Streaming (Task 16)

**Files to modify:**
- `sandbox/src/actors/conductor/output.rs` - Document update events
- `sandbox/src/api/websocket.rs` - Event routing
- `dioxus-desktop/src/views/run_view.rs` - Live document display

**Events:**
- `conductor.run.document_update` - When conductor writes to draft
- `conductor.run.progress` - Phase changes (SpawnWorker, Waiting, etc.)

**UI changes:**
- New view: Live document preview (markdown rendered)
- Updates stream in real-time as document_update events arrive
- Collapsible raw events as drill-down

### Phase 3: Cull Excessive BAML (Task 17)

**Files to modify:**
- `baml_src/conductor.baml` - Remove complex types

**Types to remove:**
- `ConductorAgendaItem` - Agenda goes in document
- `ConductorCapabilityCall` - Call tracking goes in document
- `ConductorArtifact` - Artifacts section in document
- `WorkerOutput` - Worker results in document
- `EventSummary` - Recent events narrative in document
- `FollowupRecommendation` - Next steps in document
- `DecisionType` enum - Use simple `ConductorAction`
- `TerminalityStatus` - Not needed (Complete/Block actions)
- `ConductorAssessTerminality` function - Decision covers this

**Functions to simplify:**
- `ConductorDecideNextAction` → `Decide` (minimal input/output)
- Keep `ConductorBootstrapAgenda` (needed for initial dispatch)
- Keep `ConductorRefineObjective` (or merge into Decide)

### Phase 4: Update Report Generation

**Files to modify:**
- `sandbox/src/actors/conductor/output.rs` - `generate_run_report()`

**Change:**
```rust
// Before: Build report from structured state
let report = format!(
    "# Conductor Report\n\n## Objective\n{}\n\n## Agenda\n{:?}\n...",
    run.objective, run.agenda
);

// After: Just return the living document
let report = fs::read_to_string(&run.document_path).await?;
```

## New Architecture

### Conductor Loop

```rust
loop {
    // 1. Read living document
    let draft = fs::read_to_string(&document_path).await?;
    
    // 2. Decide next action
    let decision = decide(&objective, &document_path).await?;
    
    match decision.action {
        ConductorAction::SpawnWorker { role, objective } => {
            // Spawn worker with its own draft
            let worker_draft = format!("conductor/runs/{run_id}/worker_{idx}.md");
            spawn_worker(role, objective, worker_draft).await?;
            
            // Update conductor draft
            update_draft(&document_path, "\n\nSpawned {role} to: {objective}").await?;
        }
        ConductorAction::UpdateDraft { content } => {
            // Model wants to update its understanding
            fs::write(&document_path, content).await?;
            emit_document_update(&run_id, &document_path, &content).await?;
        }
        ConductorAction::Complete => {
            return Ok(());
        }
        ConductorAction::Block { reason } => {
            return Err(ConductorError::Blocked(reason));
        }
    }
}
```

### Document Structure (Freeform)

No forced sections. Model writes what makes sense:

```markdown
# {Objective}

## Summary
(Top-level finding - don't bury the lede!)

## What We Know
- Bullet points from completed workers
- Inline citations [source](url)

## In Progress
- Current worker tasks

## Uncertainties
- What we still need to find

## Next Steps
- What to do next
```

Model can restructure arbitrarily - delete sections, add new ones, reorder.

## Integration with Harness Workers

### Worker Spawning

```rust
// Conductor decides to spawn researcher
Action::SpawnWorker {
    role: "researcher",
    objective: "Find chart performance data",
    working_draft: "conductor/runs/01KH5.../worker_001.md"
}

// Researcher runs harness loop with its own draft
// Emits document_update events as it writes
```

### Worker Results

```rust
// When worker completes, Conductor reads its draft
let worker_draft = fs::read_to_string("worker_001.md").await?;

// Appends summary to conductor draft
let update = format!("\n\n## Researcher Findings\n\n{}", extract_summary(&worker_draft));
file_edit(&conductor_draft, "## In Progress", &update).await?;
```

## Benefits

1. **Freeform presentation** - Model presents findings optimally, not in rigid sections
2. **Live streaming** - Document updates visible immediately
3. **Simpler state** - File is state, no complex reconstruction
4. **Resumable** - Crash recovery: read draft, continue
5. **Human readable** - Draft is natural language narrative
6. **Less tokens** - No massive structured input to BAML

## Migration Path

1. Keep current Conductor working
2. Add file tools alongside existing functionality
3. Start writing draft.md in parallel
4. Switch UI to show draft.md
5. Remove structured report generation
6. Simplify BAML types

## Files Changed

| File | Change |
|------|--------|
| `baml_src/conductor.baml` | Remove ~200 lines of complex types |
| `sandbox/src/actors/conductor/policy.rs` | Simplify decision input building |
| `sandbox/src/actors/conductor/output.rs` | Add document emission |
| `sandbox/src/actors/conductor/runtime/decision.rs` | Add file tool execution |
| `dioxus-desktop/src/views/run_view.rs` | Live document view |

## Acceptance Criteria

1. Conductor maintains `conductor/runs/{run_id}/draft.md`
2. Draft updates live on screen as model writes
3. Report shows freeform narrative, not structured sections
4. BAML `ConductorDecisionInput` has < 5 fields (vs current 9+)
5. Document can be read on Conductor wake to reconstruct context
