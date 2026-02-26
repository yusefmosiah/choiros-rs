# Writer Living Document System Review

Date: 2026-02-26
Status: Active
Owner: runtime + desktop

## Narrative Summary (1-minute read)

Current Writer behavior is failing the product contract for delegated runs:

1. Writer window opens, but content can remain blank early in runs.
2. Delegated runs can consume too many LLM turns for simple tasks.
3. Metadata and marginalia have been mixed into canonical content paths, causing document churn.

The required end-state is simpler:

1. One canonical living markdown document per run.
2. Marginalia remains first-class, but in a separate lane from canonical content.
3. Versions are snapshots of canonical content; overlays and progress are contextual annotations.

This review maps the current system, identifies failure modes, and defines a simplification sequence.

## What Changed

Immediate corrective changes implemented in this pass:

1. Writer inbox routing now splits non-user messages into two lanes:
   - `researcher`/`terminal` -> canonical section content updates
   - `writer`/`conductor` -> `writer.run.progress` marginalia updates
2. User prompt handling now skips immediate writer synthesis when delegation is already dispatched asynchronously, reducing duplicate orchestration loops before worker dispatch.
3. Strict message-passing remains in force (`message_writer` / actor envelopes), with no adapter-side auto-forwarding.

## What To Do Next

1. Preserve marginalia as a separate representation layer and render it alongside canonical content.
2. Add typed user-edit marginalia entries (diff summary + base/result version linkage).
3. Require deterministic run lifecycle:
   - visible first patch <= 1 tool step
   - terminal status + final patch on completion
4. Add regression tests for:
   - non-empty early Writer state
   - bounded delegated weather run budget
   - monotonic revision progression for one run document.
5. Standardize strict message-passing contract: workers are responsible for explicit Writer updates via `message_writer` and completion envelopes.

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
  -> canonical rendering + marginalia rendering
```

## Failure Modes Observed

1. Blank early window:
   - No guaranteed early user-visible patch before worker completion.
2. Loop inflation:
   - Duplicate orchestration/synthesis loops can accumulate before first worker dispatch.
3. Haphazard updates:
   - Multiple update paths (status/progress/overlay/version reload) compete for visible state.
4. Lane confusion:
   - Metadata summaries and status text can replace canonical content if routed through content mutation paths.

## Correctness Contract (Authoritative)

For run document `conductor/runs/<run_id>/draft.md`:

1. First visible update must appear quickly:
   - either bootstrap content or worker progress patch before substantial tool fanout.
2. Revisions are monotonic and tied to visible doc changes.
3. Final completion must include:
   - terminal run status
   - final document patch/version
4. UI must not depend on side channels to infer content readiness.
5. Marginalia (researcher/terminal/user diff entries, links, status) must not overwrite canonical sections.

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
2. Worker content updates are direct canonical section updates.
3. Marginalia/progress/status updates are separate and never mutate canonical section text.
4. Prompt edits are user diff ops against canonical base version.

## Cutover Sequence

1. Stabilize live updates (now in progress):
   - progress -> writer marginalia visibility
   - ensure first worker dispatch occurs before optional synthesis work
   - empty-content placeholder
2. Add user-edit marginalia:
   - emit user diff summary entries on prompt submit
   - include base/result version linkage
3. Enforce run lifecycle invariants in tests:
   - early visible update
   - bounded loop count
   - final completion coherence.
