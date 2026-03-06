# Checkpoint: Writer, Tracing, and Bootstrap Readiness

Date: 2026-03-06
Status: Active
Authors: wiz + Claude

## Narrative Summary (1-minute read)

Writer reprompting works — the deadlock that blocked revisions is fixed (commit 90e5e66).
But three entangled problems remain that reveal a deeper architectural need: **message-level
tracing**. The writer revision loop creates multiple versions when it should create one,
citations in documents don't link to anything, and reprompt runs are invisible in the trace.
These aren't independent bugs — they're symptoms of incomplete contracts between agents.

Separately, CI/CD deploys are working (push to main auto-deploys to Node A), but deploys
are hard-restart, not rolling. The deployment flow is fully encapsulated in GitHub Actions,
which is a prerequisite for moving development from local Claude Code to cloud-based agents.

## What Changed (since 2026-03-05 checkpoint)

1. **Writer deadlock fixed** — `OrchestrateUserPrompt` now spawns as `tokio::spawn` background
   task instead of blocking the actor. The adapter's `ractor::call!` back to the same actor
   no longer deadlocks. (90e5e66)
2. **Model defaults upgraded** — Writer and conductor callsite defaults changed from
   `ClaudeBedrockHaiku45` to `ClaudeBedrockSonnet46` for better revision quality.
3. **Clippy clean** — All pre-existing warnings fixed (61fe658).
4. **E2E test passing** — `writer-bugfix-e2e.spec.ts` passes in 4.1s solo. Video at
   `tests/artifacts/playwright/videos/writer-bugfix-PASSED.webm`.
5. **Sandbox registry deadlock fixed** — Mutex wasn't held across await (d4673d0).
6. **Non-blocking sandbox boot** — Hypervisor doesn't block on sandbox health (de942cd).

## Three Entangled Writer Problems

These are related through a common root cause: the system traces tool calls and LLM calls,
but doesn't trace **message passing between agents**. This makes it impossible to see how
agents compose their work, which is the level that actually matters for debugging multi-agent
orchestration.

### Problem 1: Circular Revisions

**Symptom:** Reprompting creates multiple versions (v5, v6, v7...) where each revision
undoes or rewrites what the previous one produced.

**Root cause:** The writer harness has `max_steps: 5`. After a successful `write_revision`
tool call, the LLM doesn't always call `finished` in the same step. The harness loops,
calls the LLM again, and the LLM sees its own previous output as tool result context —
so it decides to revise again. Each revision only sees the original base document and diff,
not the version it just created, leading to circular edits.

**Why not just make write_revision terminal?** For short edits, yes. But for longer documents,
the writer may need to delegate to researcher, get results back, then revise — a multi-step
flow where terminating after the first write_revision would be wrong.

**Real fix needed:** Version-aware context. After `write_revision` succeeds, the next step's
context should include the new version content, not just the original diff. And the system
prompt should be explicit about when to stop: "If you've produced a satisfactory revision
and have no pending delegations, call `finished`."

**File:** `sandbox/src/actors/writer/adapter.rs:564-583` (system context) and
`sandbox/src/actors/writer/mod.rs:1568-1579` (objective construction).

### Problem 2: Reprompt Runs Invisible in Trace

**Symptom:** When you reprompt a document, the trace view shows nothing new. The LLM calls
happen (visible in the `llm:writer` timeline dots) but there's no "run" entry to group them.

**Root cause:** The trace UI builds its run list from `conductor.run.started` /
`conductor.task.*` events (`dioxus-desktop/src/components/trace/parsers.rs:554-563`). Writer
reprompting emits `writer.actor.user_prompt_orchestration.*` events, which the trace parser
doesn't recognize as run events. The LLM trace events carry the correct `run_id` but have
no parent "run" to group under.

**Quick fix:** Emit a `conductor.run.started`-compatible event from the writer reprompt flow,
or teach the trace parser to recognize `writer.actor.user_prompt_orchestration.dispatched`
as a run start.

**Real fix needed:** This foreshadows the deeper issue — **we should be tracing message
passing, not just tool calls and LLM calls.** The interesting question isn't "what tools did
the writer call?" but "what messages did the conductor send to the writer, what did the writer
send to the researcher, and what came back?" The trace should show the actor message graph,
not just individual agent timelines.

**Files:** `sandbox/src/actors/writer/mod.rs:1759-1832` (event emission),
`dioxus-desktop/src/components/trace/parsers.rs:554-563` (run event filter).

### Problem 3: Source Citation Markers Broken

**Symptom:** Document body contains `[^s1]`, `[^s2]` markers that don't link to anything.
The sidebar shows "Sources (59)" with raw URLs, but there's no mapping between the footnote
markers and the URL list.

**Root cause:** Contract gap between three layers:
1. **Researcher** — collects `ResearchCitation` objects with `{id, url, title, snippet}`
2. **Writer** — receives source URLs via `WriterInboundEnvelope.sources`, stores them as
   `source_refs` (flat URL strings) on the `RunDocument`
3. **LLM** — generates markdown footnote references (`[^s1]`, `[^s2]`) because the system
   prompt doesn't specify a citation format
4. **Frontend** — renders `source_refs` as a sidebar list, renders document body as raw
   markdown with no footnote resolution

Nobody owns the citation index. The researcher knows the URLs, the LLM invents footnote IDs,
and the frontend can't connect them.

**Fix options:**
- **(a) Remove footnotes, use sidebar only.** Tell the writer LLM not to generate `[^sN]`
  markers. Sources appear in the sidebar. Simpler, works now.
- **(b) Build a citation index.** Researcher emits numbered citations. Writer references them
  by index. Frontend resolves `[^sN]` to the Nth source. Proper but requires new contract.

Option (a) is the pragmatic near-term fix. Option (b) is the right long-term design.

**Files:** `sandbox/src/actors/writer/adapter.rs:564-583` (system prompt — no citation
format specified), `sandbox/src/actors/researcher/mod.rs:131-136` (`ResearchCitation` struct),
`dioxus-desktop/src/components/writer/view.rs:1135-1143` (sidebar source rendering).

## The Deeper Issue: Message-Level Tracing

All three problems point to the same gap: **we trace tool execution and LLM calls, but not
the messages that flow between actors.** The trace shows "the researcher made 4 web searches"
and "the writer called the LLM 3 times," but not "the conductor asked the writer to produce
a report, the writer asked the researcher for sources, the researcher returned 12 citations,
the writer composed a document using 5 of them."

What we need:
- Every `ractor::cast!` / `ractor::call!` between supervision-tree actors should emit a trace
  event with `{from_actor, to_actor, message_type, correlation_id, payload_summary}`
- The trace UI should render these as a message sequence diagram, not just per-agent timelines
- Tool calls and LLM calls become implementation details *within* an agent's processing of
  a message — the message graph is the primary view

This aligns with the CLAUDE.md principle: "trust that workers and tool calls work, focus
attention on how they compose."

## Deployment and CI/CD Status

### What's Working
- Push to main triggers: fmt check → cargo test → deploy to Node A
- Deploy: SSH → pull → nix build on server → copy binaries → restart hypervisor
- Health check: curl with retry after restart
- Fully encapsulated in `.github/workflows/ci.yml`

### What's Missing for Rolling Deploys
The current deploy does `systemctl restart hypervisor` — hard restart with brief downtime.
No rolling/zero-downtime deploy exists yet.

What a rolling deploy would need:
1. **Health-gated switchover** — start new binary, wait for health, then stop old
2. **Sandbox VM lifecycle** — `ovh-runtime-ctl.sh restart` for the sandbox VM after
   binary update (currently not in CI, done manually)
3. **Two-node coordination** — deploy to Node B first, verify, then Node A (not started)
4. **Connection draining** — Caddy upstream health checks to route away from restarting node

Current assessment: **not done, but the prerequisites are in place.** The deploy flow is
encapsulated, the two-node infra exists, and Caddy can do health-based routing. The actual
rolling logic hasn't been implemented yet.

### CI for Cloud-Based Development

To move from local Claude Code to cloud-based agents developing ChoirOS:
- [x] Deploy flow encapsulated in GitHub Actions
- [x] SSH deploy key (not personal key) in CI secrets
- [x] Tests run in CI before deploy
- [ ] Rolling deploy (so cloud agents don't cause downtime)
- [ ] Sandbox VM restart in deploy pipeline
- [ ] Branch-based deploys (dev vs prod sandbox)
- [ ] Claude Code cloud agent with repo access + deploy permissions

## What To Do Next

### Immediate (unblock daily use)
1. Fix circular revisions: add version-aware context to writer system prompt, add
   "call finished after write_revision unless you have pending delegations" instruction
2. Fix citation markers: add system prompt instruction to not use footnote syntax,
   reference sources by URL inline or let the sidebar handle it
3. Fix trace visibility: teach trace parser to recognize writer reprompt events as runs

### Near-term (enable cloud development)
4. Add sandbox VM restart to CI deploy pipeline
5. Implement rolling deploy: start new → health check → stop old
6. Set up branch-based deploy (dev sandbox for testing, live for production)

### Strategic (message-level tracing)
7. Design actor message trace event schema
8. Instrument key actor message paths (conductor→writer, writer→researcher, etc.)
9. Build message sequence diagram view in trace UI
10. Deprecate per-agent timeline as the primary trace view
