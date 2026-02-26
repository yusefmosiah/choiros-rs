# Writer Living Document System Review

Date: 2026-02-26
Status: Active
Owner: runtime + desktop

## Narrative Summary (1-minute read)

Current Writer behavior is failing the product contract for delegated runs:

1. Writer window opens, but content can remain blank early in runs.
2. Delegated runs can consume too many LLM turns for simple tasks.
3. Marginalia/overlay mechanics increase state complexity without delivering a stable UX.

The required end-state is simpler:

1. One canonical living markdown document per run.
2. Every worker/writer update is a visible patch to that canonical document.
3. Versions are snapshots of canonical content; overlays are optional and not on the critical path.

This review maps the current system, identifies failure modes, and defines a simplification sequence.

## What Changed

Immediate corrective changes implemented in this pass:

1. Researcher adapter now emits `WriterMsg::ReportProgress` during harness progress updates.
2. Terminal adapter now emits `WriterMsg::ReportProgress` during harness progress updates.
3. Researcher default max loop steps reduced from `100` to `20` when no explicit override is provided.
4. Writer UI now renders a run-progress placeholder when document content is empty but run state is active.

## What To Do Next

1. Remove non-essential marginalia path from default Writer runtime mode.
2. Make canonical `writer.run.patch` events the sole live-update source in UI.
3. Require deterministic run lifecycle:
   - visible first patch <= 1 tool step
   - terminal status + final patch on completion
4. Add regression tests for:
   - non-empty early Writer state
   - bounded delegated weather run budget
   - monotonic revision progression for one run document.
5. Standardize this as a worker contract: every delegated worker adapter must mirror `emit_progress` into Writer progress/state events when run context exists.

## Current System Map

```text
PromptBar
  -> Conductor run start
    -> Writer EnsureRunDocument + bootstrap note
    -> Writer orchestration delegates Researcher/Terminal

Researcher/Terminal (Harness)
  -> tool loops (web_search, etc.)
  -> message_writer calls (proposal_append/completion)
  -> Writer EnqueueInbound

WriterActor
  -> inbox queue + optional synthesis path
  -> WriterDocumentRuntime (versions + overlays + patches)
  -> emits writer.run.* WS events

Dioxus WriterView
  -> open document via /writer/open
  -> tracks ACTIVE_WRITER_RUNS from WS
  -> applies pending patches to content
  -> optional marginalia rendering
```

## Failure Modes Observed

1. Blank early window:
   - No guaranteed early user-visible patch before worker completion.
2. Loop inflation:
   - High default worker step budget (`100`) enables long tail loops for simple tasks.
3. Haphazard updates:
   - Multiple update paths (status/progress/overlay/version reload) compete for visible state.
4. Marginalia complexity:
   - Overlay-heavy path introduces extra mental model and state handling for MVP flows.

## Correctness Contract (Authoritative)

For run document `conductor/runs/<run_id>/draft.md`:

1. First visible update must appear quickly:
   - either bootstrap content or worker progress patch before substantial tool fanout.
2. Revisions are monotonic and tied to visible doc changes.
3. Final completion must include:
   - terminal run status
   - final document patch/version
4. UI must not depend on side channels to infer content readiness.

## Simplified Target Design

```text
User Prompt
  -> Conductor
    -> WriterActor (single authority)
      -> Delegation workers
      -> Direct canonical section updates
      -> Patch stream
    -> UI consumes patch stream
```

Rules:

1. Canonical document is source of truth.
2. Worker updates are direct canonical section updates (no mandatory writer re-synthesis hop).
3. Marginalia/overlays are optional adjuncts, not required for core run progression.
4. Prompt edits are user diff ops against canonical base version.

## Cutover Sequence

1. Stabilize live updates (now in progress):
   - progress -> writer visibility
   - lower loop budget defaults
   - empty-content placeholder
2. Reduce state surfaces:
   - keep overlays behind feature flag in default run UI
3. Enforce run lifecycle invariants in tests:
   - early visible update
   - bounded loop count
   - final completion coherence.
