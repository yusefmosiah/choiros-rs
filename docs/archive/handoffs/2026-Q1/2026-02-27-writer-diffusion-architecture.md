# Writer Diffusion Architecture

Date: 2026-02-27
Status: Active — canonical authority for Writer design decisions.

## Narrative Summary (1-minute read)

The Writer is a living-document system. It solves AI UX problems that chat
cannot: instead of autoregressive append-only output, the Writer operates by
**diffusion** — updating a document with diffs rather than appending to the end.

This produces a natural separation: a **document control plane** (the living
document itself) and **concurrent workers** (researcher, terminal) that produce
proposed diffs. The document is the shared state. Workers read it, propose
changes, and those changes appear as pending proposals until accepted.

Everything is a diff. There are no "prompts" separate from the document.

## What Changed

Previous implementations conflated three things:
1. A synchronous "enqueue" path that bypassed the inbox for worker messages
2. A separate "user prompt" path with forced-delegation token matching
3. A heavyweight synthesis LLM loop that rewrote the whole document

All three are eliminated. The new model:
- One inbox, one processing path, all sources
- Workers submit proposed diffs, never touch canon
- User input is a diff (on empty doc if from prompt bar, on current doc if editing)
- No synthesis rewriting; the document grows by accepted diffs

## What To Do Next

Implement the rewrite of `sandbox/src/actors/writer/mod.rs` to match this spec.
Remove `WriterSynthesisAdapter`. Update callers.

---

## Core Principle: Everything Is a Diff

A user typing "research quantum computing and write a summary" in the prompt bar
is submitting a diff on an empty document. That diff creates version 1. A user
editing paragraph 3 of an existing document is submitting a diff on the current
head version. A researcher worker returning findings is submitting a proposed
diff. A terminal worker returning command output is submitting a proposed diff.

There is no separate "objective" concept. The document IS the objective. Its
content is what the system is working toward. The title/heading may be derived
from the first version, but it is document content, not external metadata.

### Diff Envelope

Every mutation to the document arrives as the same structure:

```
source:           who (user | researcher | terminal | conductor)
content:          the diff payload (text to add/replace/remove)
base_version_id:  which version this diff is against
proposal:         false for canonical (user), true for pending (workers)
```

That's it. No 14-field envelopes. No citation metadata in the mutation path.
Citation/source metadata belongs in events (observability), not in the diff
contract.

### Canonical vs Proposed

- **Canonical diffs** come from the user. They are applied immediately and
  create a new document version. The user is the authority.
- **Proposed diffs** come from workers. They create overlays (pending proposals)
  that appear as gray/pending text in the document. The user (or an explicit
  accept action) promotes them to canon.

Workers never mutate canon. This is a hard rule.

## Architecture

### One Inbox

All diffs enter the same inbox queue. `ProcessInbox` handles them uniformly:

1. Pop next diff from queue
2. If `proposal: false` (user) → create new canonical version
3. If `proposal: true` (worker) → create overlay (pending proposal)
4. Emit patch event over websocket for live UI update
5. Continue to next item

No source-specific branching in the enqueue path. No synchronous shortcuts.
The inbox is async — `EnqueueInbound` queues and returns an ack immediately.
`ProcessInbox` is a self-scheduled wake that drains the queue.

### Worker Flow

```
Conductor
  → delegates to Researcher/Terminal workers
  → workers do their work (search, execute, read files)
  → workers send proposed diffs to Writer via EnqueueInbound
  → Writer creates overlays
  → UI shows proposed text as pending/gray
  → User accepts or dismisses
```

### No Synthesis Loop

The old architecture had a `WriterSynthesisAdapter` that ran an LLM to rewrite
the entire document after workers completed. This is eliminated. The document
grows by accumulation of accepted diffs, not by meta-rewriting.

If the user wants the document revised/polished, they can explicitly request it
(a new diff cycle). But the default path is: workers propose, user curates.

### No Forced Delegation Token Matching

The old architecture parsed user prompts for literal strings like
`"delegate_researcher"` or `"delegate_terminal"` to decide routing. This is the
antipattern AGENTS.md explicitly forbids: phrase matching as orchestration
authority.

Delegation decisions are made by the conductor (model-led), not by string
matching in the Writer.

## Message Protocol (Minimal)

Removed:
- `EnqueueInboundAsync` (duplicate of EnqueueInbound, fire-and-forget is just cast)
- `ApplyHumanComment` (alias for ApplyText with hardcoded args)
- `DelegateTask` (redundant with OrchestrateObjective)
- Forced delegation in `dispatch_user_prompt_delegation`

Kept (all used by external callers):
- `EnsureRunDocument` — initialize document for a run
- `EnqueueInbound` — submit a diff (the unified entry point)
- `ProcessInbox` — internal wake to drain queue
- `ApplyText` — low-level text mutation (used by workers via message_writer tool)
- `ReportProgress` — non-mutating progress metadata
- `SetSectionState` — section lifecycle state
- `ListWriterDocumentVersions` — query versions
- `GetWriterDocumentVersion` — query single version
- `ListWriterDocumentOverlays` — query pending proposals
- `DismissWriterDocumentOverlay` — reject a proposal
- `CreateWriterDocumentVersion` — accept/create canonical version
- `SubmitUserPrompt` — user diff from prompt bar (wraps EnqueueInbound)
- `OrchestrateObjective` — conductor entry point for delegation planning
- `DelegationWorkerCompleted` — worker lifecycle signal

## Frontend Contract

Proposed diffs appear as **inline gray text** in the document body, not as
margin cards. The user sees what will change, in place, and can accept or
dismiss each proposal.

Patch events over websocket carry:
- `overlay_id` (present = proposal, absent = canonical)
- `target_version_id` (present = canonical version created)
- `ops` (the diff operations)

The frontend distinguishes proposal patches from canonical patches using these
fields and renders accordingly.

## What This Enables

1. **Concurrent workers.** Multiple workers can propose diffs simultaneously.
   They don't block each other or the user. The document is the merge point.

2. **User agency.** The user sees proposals arrive in real-time and decides
   what to keep. Not a black box that emits a final result.

3. **Incremental progress.** The document grows visibly as workers complete,
   not after a synthesis pass. Latency to first visible change is bounded by
   worker latency, not synthesis latency.

4. **Composability.** The conductor can dispatch multiple worker types
   (researcher + terminal) in parallel. Their outputs arrive as independent
   proposed diffs. No coordination bottleneck.

5. **Simplicity.** One inbox, one processing path, one diff type. The Writer
   actor becomes a queue processor + document store, not an orchestration engine.
