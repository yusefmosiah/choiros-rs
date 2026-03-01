# Simplified Multiagent Communications Architecture (Writer-First)

**Date:** 2026-02-13
**Status:** Proposed for immediate implementation
**Supersedes/Refines:**
- `docs/handoffs/2026-02-12-document-driven-multiagent.md`
- `docs/handoffs/2026-02-13-aggressive-writer-cutover.md`
- `docs/handoffs/2026-02-13-writer-first-cutover-implementation-report.md`

## Narrative Summary (1-minute read)

We currently have too many communication patterns in one runtime path:
- direct actor messages,
- EventStore/EventBus wake signaling,
- run document updates,
- and UI log/proposal duplication.

This creates ambiguity, repeated loops, and poor legibility.

This handoff defines a strict three-plane architecture:
1. **Control Plane:** `ractor` messages only (authority for orchestration).
2. **Data Plane:** `RunWriter` patch stream only (authority for shared run document state).
3. **Telemetry Plane:** EventStore events only (observability, never orchestration authority in normal flow).

The result is a Writer-first system where:
- workers stream useful document deltas,
- users see concise live status,
- conductor remains orchestration-only,
- and watcher is de-scoped from normal run progression.

## What Changed

- Clarified and reduced communication patterns to one per plane (control/data/telemetry).
- Reframed Watcher as escalation-only, not routine scheduler for run steps.
- Defined hard role boundaries between `researcher` and `terminal`.
- Defined removal target for fallback/legacy dual-schema tool args.
- Added Writer revision-history requirements (back/forward navigation).
- Added Writer-agent direction: harness-backed actor that curates noisy worker output into readable document revisions.

## What To Do Next

1. Adopt the communication contract in Section 2 as the source of truth.
2. Implement the Phase 0-3 cutover in Section 8 before adding net-new features.
3. Add Writer revision history and arrows (Section 6) as the next UX milestone.
4. Introduce WriterAgent harness (Section 7) only after the contract and tests are green.

---

## 1) Problem Statement (Current Pain)

### 1.1 Symptoms observed

- Long-running loops with repeated external fetch attempts.
- Terminal performing web research behavior that should belong to Researcher.
- Writer showing both a proposal banner and full log-like document content, increasing noise.
- High-volume progress lines dominate the document and reduce answer legibility.

### 1.2 Root causes

- Overlapping communication patterns with unclear authority boundaries.
- Generic harness prompt/tool schema encourages permissive behavior unless role constraints are explicit.
- Progress/log events are being promoted to document content in the same channel as collaborative edits.
- Fallback-compatible schemas (nested + flat args) increase malformed/ambiguous tool calls.

---

## 2) Communication Contract (Authoritative)

This section is the canonical contract for run execution.

### 2.1 Plane A: Control Plane (orchestration authority)

**Primitive:** direct `ractor` messages.

**Allowed operations:**
- Conductor spawning/dispatching workers.
- Conductor receiving worker completion/failure.
- Conductor requesting writer state transitions (section state, commit/discard proposal).

**Not allowed:**
- EventStore/EventBus deciding normal next-step orchestration.
- Worker-to-worker direct orchestration.

### 2.2 Plane B: Data Plane (shared run state authority)

**Primitive:** typed `RunWriter` patch commands and `writer.run.patch` stream.

**Allowed operations:**
- Worker emits `ApplyPatch` to its section.
- Writer/User emits `ApplyPatch` to user/editorial section.
- Conductor emits commit/discard/state commands only, not freeform text mutation.

**Not allowed:**
- Workers writing shared run docs directly via filesystem bypass.
- Progress spam as canonical document content.

### 2.3 Plane C: Telemetry Plane (observability authority)

**Primitive:** EventStore append + query + websocket fanout.

**Allowed operations:**
- Lifecycle and audit events.
- Worker tool-call/result traces.
- Watcher review/escalation records.

**Not allowed:**
- Telemetry events as primary state authority for run document.
- Telemetry wake events driving normal run progression.

### 2.4 Rule: one decision authority per concern

- Run flow decisions: Conductor policy via control plane.
- Document state: RunWriter via data plane.
- Monitoring/forensics: EventStore/Watcher via telemetry plane.

---

## 3) Role Boundaries (Hard)

### 3.1 Conductor

- Owns planning, dispatch, completion, block.
- Does not perform dense text merge in MVP end state once WriterAgent exists.
- Does not stream verbose worker logs to user-facing document.

### 3.2 Researcher

- Allowed: `web_search`, `fetch_url`, citations, source synthesis, doc patch proposals.
- Not allowed: shell orchestration for general web browsing via `curl` when researcher capability is available.

### 3.3 Terminal

- Allowed: local shell/file/system inspection and execution.
- Not allowed: general web research loops by scraping news/search pages unless explicitly assigned a terminal-specific objective that requires it.

### 3.4 Writer (current actor + future agent)

- Current `RunWriterActor`: single mutation authority and revision monotonicity.
- Future `WriterAgent`: curates intermediate worker updates into readable revisions and keeps the shared doc legible.

### 3.5 Watcher

- Escalation-only (timeouts, failures, anomaly spikes).
- Must not trigger routine step-by-step orchestration for healthy runs.

---

## 4) Current Code Reconciliation (Where We Are)

### 4.1 Confirmed current behavior

- Conductor currently forwards worker progress into `RunWriterMsg::AppendLogLine` with `proposal: true` for both researcher and terminal.
- `RunWriter` appends timestamped log text into section proposal content and emits full-document patch events.
- Writer UI shows proposal banner plus full document text, so users see effectively duplicated progress channels.
- Watcher is now **disabled by default** (`WATCHER_ENABLED=false` unless explicitly set true).
- Conductor `DispatchReady` now dispatches existing ready agenda items directly before asking policy for another action.
- Terminal harness is generic enough to repeatedly call `bash` with external `curl` commands.
- Tool arg schema still includes nested and flat compatibility fields.

### 4.2 Keep vs De-scope vs Remove

| Area | Current | Decision |
|---|---|---|
| `ConductorMsg` control flow | Active | **Keep** |
| `RunWriterActor` single writer | Active | **Keep** |
| `writer.run.*` websocket stream | Active | **Keep** |
| EventStore persistence | Active | **Keep** |
| EventBus in run flow | Limited/legacy | **De-scope from run path** |
| Watcher wake for routine progression | Available | **De-scope** |
| `AppendLogLine` as primary live output | Active | **Remove from normal worker streaming** |
| Dual-schema tool args (nested + flat) | Active | **Remove after strict cutover** |
| Worker direct file sync for run docs | Mostly blocked already | **Keep blocked** |

### 4.3 Why this reconciliation

- It minimizes moving parts without deleting observability.
- It preserves the strongest pieces already working (`RunWriter`, typed events).
- It removes ambiguous and expensive control loops.

---

## 5) UX Contract for Writer Live Runs

### 5.1 Live surface should be sparse

During active run, default visible status should be:
- worker type,
- model/provider,
- one-line latest status.

Not full command logs by default.

### 5.2 Document surface should prioritize readability

- Intermediate worker contributions should appear as scoped proposal edits, not raw trace logs.
- Conductor/WriterAgent periodically rewrites/condenses noisy intermediate text into readable narrative.

### 5.3 Optional deep observability

- Full tool call logs remain available in Logs app / expandable panel.
- Observability is opt-in detail, not default writing surface.

---

## 6) Revision Model + Back/Forward Arrows

### 6.1 Revision requirements

- Every accepted mutation increments monotonic `revision`.
- Writer supports cursoring through historical revisions independent from live head.

### 6.2 MVP history model

- Linear revision chain per run:
  - `revision N` snapshot addressable,
  - `current_cursor_revision`,
  - `live_head_revision`.

### 6.3 Writer UI requirements

- Add back arrow: move `cursor` to prior revision.
- Add forward arrow: move `cursor` toward live head.
- Show banner when not at live head: `Viewing revision N of M`.
- Editing while not at head should require explicit action (`fork from N` or `jump to head`) to avoid silent divergence.

### 6.4 Non-goal for MVP

- No DAG branch UI yet.
- Linear history first; DAG later.

---

## 7) WriterAgent Harness Direction (Post-simplification)

### 7.1 Why WriterAgent

A deterministic writer actor is necessary but insufficient for readability under concurrent workers.

WriterAgent should:
- ingest user inline input + worker proposals,
- decide when enough signal accumulated,
- produce coherent revised document patch,
- keep context isolated per run/document.

### 7.2 WriterAgent responsibilities

- Summarize/merge repeated worker updates.
- Preserve citations/provenance pointers.
- Keep user-facing narrative concise.
- Avoid reintroducing conductor-heavy merge logic.

### 7.3 WriterAgent boundaries

- WriterAgent proposes patches through RunWriter (still single writer authority).
- Conductor remains orchestration authority across many runs/apps.

---

## 8) Migration Plan (Aggressive, Low-ambiguity)

### Phase 0: Freeze complexity growth

- No new communication mechanisms in run path.
- No additional fallback branches.

### Phase 1: Enforce role boundaries

- Tighten Terminal prompt/policy to local execution only.
- Tighten Researcher prompt/policy to external research only.
- Add guardrails for repeated-domain/query loops.

### Phase 2: Remove document log spam path

- Stop default worker streaming through `AppendLogLine` to proposal text.
- Stream concise status ticks separately.
- Keep patch-based content updates only.

### Phase 3: Watcher de-scope in normal runs

- Restrict watcher-conductor wake actions to explicit failure/anomaly classes.
- Disable watcher normal-run wake path by default.

### Phase 4: Strict tool schema

- Remove legacy flat tool args; nested typed args only.
- Reject malformed tool calls with explicit typed errors.

### Phase 5: Revision navigation

- Persist revision snapshots/diffs needed for back/forward arrows.
- Add writer UI controls and cursor semantics.

### Phase 6: WriterAgent integration

- Introduce harness-backed writer actor/agent over stable contract.
- Move merge/condense responsibilities from conductor to writer agent.

---

## 9) Test Gates (Required)

### 9.1 Behavioral gates

- `conductor/execute` opens Writer immediately for accepted run.
- Live run updates appear within seconds (not completion-gated).
- Terminal does not perform generic web research when researcher available.
- Repeated-command loop guard triggers block/complete rather than infinite retries.

### 9.2 Contract gates

- No fallback parse path for legacy tool args.
- No run accepted without valid `run_id` + `document_path` + writer target.
- Run document mutations only accepted through RunWriter messages.

### 9.3 UX gates

- Default run view shows concise per-worker status line.
- Full logs hidden behind explicit expansion.
- Back/forward revision navigation works deterministically.

---

## 10) Open Questions (Explicit)

1. Should WriterAgent auto-commit worker proposals continuously or only at policy checkpoints?
2. Should user inline gray text be represented as `user.proposal` section or separate editorial directive layer?
3. For revision navigation, do we store full snapshots each revision or patch+periodic snapshot compaction?

---

## 11) Immediate Execution Checklist

1. Ship this contract doc and link it from active implementation handoff.
2. Remove worker `AppendLogLine` default path from run document streaming.
3. Introduce concise status-tick event shape and UI rendering.
4. Enforce researcher/terminal role boundaries at adapter/harness layer.
5. Gate watcher wake behavior to failures/anomalies only.
6. Add revision cursor model and Writer arrow controls.
7. Then begin WriterAgent harness integration.

### 11.1 Implementation Runbook Link

- Execution-ready runbook: `docs/handoffs/2026-02-13-simplified-multiagent-comms-implementation-runbook.md`

### 11.2 LLM Tracing Runbook Link

- Execution-ready runbook: `docs/handoffs/2026-02-13-llm-tracing-runbook.md`
