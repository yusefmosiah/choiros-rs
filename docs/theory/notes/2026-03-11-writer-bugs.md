# Writer Bugs

Date: 2026-03-11
Kind: Note
Status: Active
Priority: 1
Requires: []

Known writer bugs observed during worker workload testing. The conductor →
writer → terminal agent pipeline works correctly (Trace confirms: 4 LLM calls,
3 tools, 1 worker, completed in 14.8s). The bugs are all in the writer's
content lifecycle.

## Bug 1: Final rewrite produces blank content

**Observed:** Writer opens with perfunctory revision 1 (echoes prompt as
heading), status goes to `waiting_for_worker`, terminal agent runs commands,
run completes — but the final rewrite wipes the prose body to blank/empty.
Status shows "Done" with revision 1, but the contenteditable div is empty.

**Expected:** Writer should rewrite with initial results from terminal agent,
then keep rewriting as more results come in, then yield with a stable version
containing the full output.

**Evidence:** `tests/artifacts/playwright/interactive-03-env-01-10s.png` shows
170 chars of content at t+10s. By t+20s, content is 1 char (blank). Stays
blank through completion. Trace confirms the run completed successfully.

## Bug 2: Workers send diffs instead of findings

**Observed:** Workers (terminal agent) send diffs back to the writer when they
should be emitting findings, results, questions, etc. The writer receives a
diff format that it doesn't know how to incorporate into the living document.

**Expected:** Workers should send structured results — command output, file
contents, error messages, questions — not diffs. The writer decides how to
present results in the document.

## Bug 3: Second prompt in same session produces no worker

**Observed:** After the first prompt completes (even with blank content), the
second prompt in the same session completes instantly without spawning a
worker. The Trace shows only 1 run for both prompts. The writer may be
reusing the completed run context and skipping delegation.

**Evidence:** Go compile prompt (step 7 in interactive test) never got its own
Trace card. Writer went straight to "Done" with blank content.

**Expected:** Each prompt should create a new run with fresh worker delegation.

## Bug 4: No rewrite triggered after worker completion (ROOT CAUSE of Bug 1) — FIXED

**Observed:** `handle_delegation_worker_completed` (mod.rs:2028-2161) does
three things when a worker finishes: sets section state to Complete/Failed,
emits a progress event with the summary, and cleans up the worker actor.

It never triggers a new Writer LLM call to incorporate the results. The
worker summary goes into a progress event (marginalia lane), not back through
the Writer's LLM for a revision. The perfunctory revision stays as the final
content because no one ever asks Writer to rewrite with the findings.

**This is the root cause of Bug 1.** The full chain:

```
User prompt
  → Writer LLM → perfunctory revision 1 ✓
  → delegates to terminal
  → Writer LLM calls finished (too early, see Bug 5)
  → terminal runs, completes
  → handle_delegation_worker_completed
  → progress event only, no rewrite triggered
  → document stays at perfunctory revision 1
  → UI shows "Done" with minimal/blank content
```

**Fix (implemented):** After worker completion, `handle_delegation_worker_completed`
now sends an `OrchestrateUserPrompt` message that re-invokes the Writer LLM
with the worker's summary and current document content. The LLM produces a
revised version incorporating the findings. This implements option (a) from
Bug 5 — event-driven re-invocation on worker completion.

## Bug 5: Writer LLM delegates then finishes without waiting for results — FIXED (via Bug 4 fix)

**Observed:** The Writer LLM runs in a `tokio::spawn` background task
(`orchestrate_user_prompt_bg`). The system prompt tells the LLM to "call
finished immediately unless unresolved worker delegation still needs to
change the document." But the LLM has no mechanism to block and wait for
worker results — the harness loop runs to completion in one shot.

The LLM delegates to terminal, then in the same or next turn calls `finished`
because there's nothing else it can do. The harness ends. The worker
completes later, but the Writer LLM session is already gone.

**Fix (implemented):** Option (a) — event-driven re-invocation. The original
harness is allowed to finish after delegation. When worker results arrive
via `DelegationWorkerCompleted`, a new `OrchestrateUserPrompt` is dispatched
with the worker summary + current document as objective. This starts a fresh
Writer LLM call that can produce the final revision. The actor model is
preserved — Writer wakes on worker completion message, starts a new LLM
call with updated context.

## Bug 6: Inbox processing flag can stall permanently — RESOLVED (already guarded)

**Status:** Not a bug. Code already wraps the processing loop in an async
block and unconditionally clears `inbox_processing` after the `.await`
(mod.rs:1281-1285). Comment explains panic recovery via ractor actor
recreation. No fix needed.

## Bug 7: Overlay superseding on version creation drops pending worker content

**Observed:** When a new document version is created (document_runtime
lines 159-177), all pending overlays on the parent version are superseded.
If worker completions arrive as overlays and a new version is created before
they're processed (e.g., the perfunctory revision creates version 1,
superseding any overlays from fast-returning workers), the worker content
is silently discarded.

**Expected:** Worker results should survive version creation. Either:
- Don't supersede worker overlays on perfunctory revisions
- Process worker overlays before creating new versions
- Use the inbox queue (not overlays) for worker results

## Reproduction

```bash
cd tests/playwright
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
  npx playwright test worker-interactive.spec.ts --project=hypervisor
```

Screenshots land in `tests/artifacts/playwright/interactive-*.png`.
Trace app shows run telemetry (LLM calls, tools, workers, timing).

## Correct Writer Flow (reference)

1. User submits prompt via prompt bar
2. Conductor dispatches to writer
3. Writer creates revision 1: perfunctory rewrite (reiterates topic, notifies
   user of plan)
4. Writer delegates to terminal agent (status: waiting_for_worker)
5. Terminal agent executes commands, sends results back
6. Writer rewrites with initial results (revision 2)
7. If more results arrive, writer rewrites again (revision 3, 4, ...)
8. Writer yields with stable final version (status: completed)

Currently step 6 never happens (bug 4 — no rewrite triggered after worker
completion). Step 5 sends diffs instead of findings (bug 2). The Writer LLM
exits before workers finish (bug 5). The blank content (bug 1) is a symptom
of bugs 4 and 5 combined.

## Root Cause Summary

The fundamental issue was architectural: the Writer LLM harness ran as a
one-shot background task. It delegated, then finished. There was no mechanism
to re-invoke the Writer LLM when worker results arrived. The actor received
worker completion messages but only logged them as progress events — it never
fed them back into an LLM call for document revision.

**Fixed (2026-03-11):** `handle_delegation_worker_completed` now dispatches
`OrchestrateUserPrompt` on successful worker completion, re-invoking the
Writer LLM with the worker's findings. The harness is still one-shot per
invocation, but worker completion triggers a new invocation — event-driven
re-invocation across delegation boundaries. This implements the
spatial/temporal pattern from ADR-0021: worker results (temporal events)
flow through Writer's re-invocation to produce document state (spatial)
updates.

Remaining bugs (2, 3, 7) are lower priority and may resolve partially as
side effects of the Bug 4+5 fix. Bug 2 (worker contract) is still relevant
but now the re-invocation path provides a structured channel for worker
results even if they arrive as diffs. Bug 3 (second prompt) needs separate
investigation. Bug 7 (overlay superseding) is mitigated because worker
results now flow through re-invocation rather than overlays.
