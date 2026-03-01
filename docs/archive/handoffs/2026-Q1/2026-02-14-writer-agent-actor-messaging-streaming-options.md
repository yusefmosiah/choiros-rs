# Writer Agent, Actor Messaging, and Long-Run Streaming Options

Date: 2026-02-14  
Status: Direction record from regression-debug session  
Scope: Writer authority, conductor capability boundaries, live long-run worker streaming

## Narrative Summary (1-minute read)

The observed failure is not "research failed"; research completed, but Writer stayed on a stub document and never reflected run output.
This exposes a control-path mismatch: progress/status can stream, while document mutation authority was not consistently routed through a single writer authority path.

Direction is to simplify and harden:
1. canonical run identity is `run_id` only (no `task_id`/`correlation_id` fallback for runtime control),
2. Conductor is orchestration-only and must not have file tools,
3. Writer app agent owns document mutation and has file tools,
4. workers stream progress/findings continuously for long-running tasks,
5. control flow uses typed actor messages, not events,
6. events remain for tracing/observability only.

## What Changed

1. Captured the concrete failure sequence from the Feb 14 regression session.
2. Recorded hard constraints decided in-session (run_id-only, no deterministic fallback, no event-driven control flow).
3. Defined and compared implementation options for worker-to-writer live updates.
4. Chose a recommended option aligned with model-led control flow and actor messaging.
5. Specified implementation packets and acceptance criteria.

## What To Do Next

1. Land actor-messaging tools in the agent harness so workers can send typed messages to Writer.
2. Remove file tools from Conductor grants and grant them to Writer app agent.
3. Rename/refactor `adapter` to `worker_port` with a smaller execution-only boundary.
4. Implement Writer proposal/canon revision UX (gray proposals/comments + version seek controls).
5. Keep worker progress streaming independent from Writer document patching so hour-long research remains visible.

---

## Observed Regression Sequence (Feb 14, 2026)

Reference run example: `01KHEYMKVMMD67SV6P4HWKSEFF`

1. Human submits prompt.
2. Writer opens and initially shows useful run context.
3. Writer then reloads to a stub document template.
4. Backend run continues and completes (research/conductor logs show completion).
5. Writer remains on stub and does not reflect completed findings.

Important implication: backend completion alone is insufficient; document mutation route to Writer was not effective for this run path.

## Hard Constraints and Decisions

1. No deterministic fallback generation to mask routing failures.
2. Runtime identity is `run_id` as canonical control key.
3. Conductor has no file tools (orchestration-only).
4. Writer app agent is canonical mutation authority and must hold file tools.
5. Worker progress/findings must stream live even without immediate canon merge.
6. Events are tracing transport, not workflow control authority.
7. Coordination/control uses typed actor messages.

## Problem Framing

The system currently mixes two channels:
1. worker/conductor progress status channel,
2. document mutation channel.

If channel (1) works and channel (2) fails or diverges, UI can show run status changes while text remains stubbed.
The fix is not fallback text generation. The fix is explicit message authority and a single writer mutation path.

## UI Direction (Option 1 from session)

1. Proposed edits appear inline as gray text.
2. Human comments also appear as gray proposal context.
3. Human can press prompt action to request revision.
4. Writer agent can also auto-produce a new version using proposal context.
5. Title bar includes `<` and `>` controls to seek revisions.

This preserves living-document interactivity while keeping canon/proposal distinction visible.

## Architecture Options

### Option A (Recommended): Direct Worker -> Writer Actor Messaging Tool Calls

Flow:
1. Worker receives run context with `run_id` and writer actor handle.
2. Worker emits progress/status messages continuously.
3. Worker uses harness tool call `message_actor` to send typed patch/proposal commands directly to Writer.
4. Writer applies mutation, increments revision, emits writer update events for UI/tracing.
5. Conductor remains orchestration-only and does not proxy file writes.

Pros:
1. Lowest control-path ambiguity.
2. Clear ownership: workers propose, Writer mutates.
3. Supports long-running research with incremental proposals.
4. Aligns with model-led control and non-blocking conductor turns.

Tradeoffs:
1. Requires new harness messaging tools and capability grants.
2. Requires strict message schema/versioning.

### Option B: Worker -> Conductor Request -> Writer Relay

Flow:
1. Worker sends typed request to Conductor.
2. Conductor validates/routes to Writer for apply.

Pros:
1. Central policy checkpoint.
2. Easier global throttling/accounting in one place.

Tradeoffs:
1. Higher latency and extra coupling to conductor turn scheduling.
2. Reintroduces risk of conductor becoming document-mutation bottleneck.
3. Harder to keep conductor strictly orchestration-only in practice.

### Option C: Worker -> EventBus Proposal, Writer Watches and Applies

Flow:
1. Worker writes proposal events.
2. Writer consumes event stream and mutates.

Pros:
1. Decoupled transport semantics.
2. Reuses existing event infrastructure.

Tradeoffs:
1. Violates decision that events are not control authority.
2. Harder deterministic delivery/ack semantics for mutation-critical control.
3. Encourages hidden workflow coupling via event patterns.

Decision: choose Option A.

## Control and Trace Separation

1. Control-plane messages:
   - typed actor envelopes,
   - explicit sender/recipient/run scope,
   - required ack/error semantics.
2. Trace-plane events:
   - append-only observability records,
   - may be delayed/replayed,
   - must never be required for forward run progression.

## Proposed Minimal Message Contracts (v0)

### Worker -> Writer

1. `WriterApplyPatch { run_id, section, ops, proposal, source_worker_id }`
2. `WriterAppendNote { run_id, section, markdown, proposal, source_worker_id }`
3. `WriterMarkSectionState { run_id, section, state }`

### Writer -> Worker (Ack/Error)

1. `WriterAck { run_id, message_id, revision }`
2. `WriterReject { run_id, message_id, reason }`

### Worker/Writer -> Conductor (Request path)

1. `ConductorRequest { run_id, kind, payload, correlation }`

Note: `correlation` can exist for diagnostics, but orchestration identity remains `run_id`.

## Capability Ownership Corrections

1. Conductor capability grant:
   - orchestration messaging only,
   - no `file_read`, `file_write`, `file_edit`.
2. Writer app agent capability grant:
   - `file_read`, `file_write`, `file_edit`,
   - revision/index persistence tools as needed.
3. Researcher/Terminal worker grants:
   - retain file tools for worker-local artifacts,
   - shared run document mutation must go through Writer actor messages.

## Harness Refactor Direction

1. Rename `adapter` to `worker_port`.
2. Keep `worker_port` minimal:
   - worker spec identity,
   - tool execution,
   - actor messaging tool surface.
3. Keep loop control/progress budgeting in shared harness runtime.

## Implementation Packets

1. Packet 1: Identity and grants hardening
   - enforce `run_id` canonical parsing in control paths,
   - remove conductor file tool grants,
   - add writer file tool grants.
2. Packet 2: Harness actor-messaging tools
   - add typed `message_actor`/`request_conductor` tool schemas,
   - add ack/error handling utilities.
3. Packet 3: Writer app agent integration
   - spawn/register writer actor per run,
   - apply patches, revision monotonicity, broadcast updates.
4. Packet 4: UI proposal/revision UX
   - gray proposals/comments,
   - version seek (`<`/`>`),
   - prompt-triggered and auto-revision modes.
5. Packet 5: Streaming and endurance validation
   - long-duration worker simulation (hours-scale),
   - ensure progress stream continuity independent of final completion.

## Acceptance Criteria

1. Prompt submit opens Writer once (no stub reload loop).
2. Writer transitions from initial context to live evolving document, not static stub.
3. Worker progress remains visible continuously throughout long runs.
4. Researcher findings incrementally appear as proposals before final synthesis.
5. Conductor performs no file tool calls in run execution.
6. Writer is the only authority applying shared run-document mutations.
7. Event loss/delay does not block control flow; actor message path still progresses run.

## Open Questions

1. Should Writer auto-revision trigger be timer-based, threshold-based, or explicit-only?
2. Do we require per-section patch conflict policy before multi-worker writes to same section?
3. How should manual human edits be represented relative to worker proposals in revision history?

## Non-Goals

1. Reintroducing deterministic fallback content generation.
2. Making EventBus/EventStore a mutation-control mechanism.
3. Returning to conductor direct tool execution for document updates.
