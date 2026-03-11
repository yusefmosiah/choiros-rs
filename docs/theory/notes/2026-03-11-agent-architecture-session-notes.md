# Agent Architecture Session Notes

Date: 2026-03-11
Kind: Note
Status: Active
Priority: 4
Requires: [ADR-0021, ADR-0022]
Owner: platform/runtime

## Narrative Summary (1-minute read)

Session exploring how to integrate coding agents into ChoirOS, rethink the
agent loop, and plan the path toward choir.go. Key conclusions: BAML should be
replaced with native tool-use protocol calls (cost + latency win), cagent
should grow into a multi-harness orchestrator, and the missing abstraction in
ChoirOS is harness-level orchestration (sequencing entire agent runs, not
individual model turns).

## What Changed

Nothing in code. This is a requirements and design exploration session.

## What To Do Next

See prioritized work items below.

---

## 1. BAML Removal / Native Tool-Use Migration

**Problem**: BAML interposes a structured-output parsing layer between the model
and tool execution. This forces a DECIDE -> EXECUTE two-phase loop instead of
the standard tool-use protocol loop. Extra latency per turn, extra tokens for
schema encoding, extra build dependency.

**Observation**: BAML coupling is concentrated in:
- `baml_client/` (generated code)
- Agent harness DECIDE step
- Conductor/Writer/Researcher/Terminal adapters (tool schema provision)

The actor system, supervision tree, events, websockets — none touch BAML.

**Target**: Replace with a multi-provider client that speaks native tool-use
(Anthropic Messages API tool_use blocks, OpenAI function calling). The harness
loop simplifies to:

```
loop {
    response = client.call(messages, tools).await?;
    if response.stop_reason == EndTurn { break; }
    for tool_call in response.tool_calls {
        result = execute(tool_call);
        messages.push(tool_result(result));
    }
}
```

**Effort**: ~2-3 weeks. Rest of codebase untouched.

**Saves**: Token overhead from BAML schema encoding, latency from extra parsing
step, build complexity from BAML codegen.

## 2. Writer Contract Fix (ADR-0021)

**Problem**: Workers currently treated as diff-makers. `AUTO_ACCEPT_WORKER_DIFFS
= true` is hardcoded. Workers should send structured signals (findings, results,
progress, requests), not diffs. Writer synthesizes from signals into document
revisions using its own judgment.

**This is a bug, not a feature gap.** The inbound envelope contract already
carries structured fields (summary, citations, source_refs, confidence). But
the abstraction leaks and raw diffs sneak through.

**Blocks**: Writer UX work, proper verification at the app-agent level.

## 3. Provider Auth (BYOK)

**Problem**: Provider gateway uses a single global credential set. No per-user
token storage, no OAuth flow.

**Target**:
- OpenAI: OAuth2 + PKCE flow (ChatGPT subscription tokens). Officially blessed
  for third-party use.
- Anthropic: Paste-token flow (no subscription OAuth yet). Fine for individual
  dev use, TOS-grey for distribution. BYOK via API keys for users.
- Per-user credential store in hypervisor, gateway routes per-user.

**Reference**: PicoClaw (github.com/sipeed/picoclaw) implemented this in
~200 lines of Go (Issue #18, merged 2026-02-12). OAuth2 PKCE + paste-token +
auto-refresh + token storage at `~/.picoclaw/auth.json`.

**Enables**: Free tier on Bedrock credits with rate limits, BYOK for power users.

## 4. Inception Provider

New fast LLM provider using diffusion instead of autoregression. Needs a
provider gateway adapter. Details TBD.

## 5. Harness-Level Orchestration (The Missing Abstraction)

**Problem**: ChoirOS has turn-level orchestration (agent harness) and
capability-level routing (conductor). Missing: orchestration of entire agent
runs as atomic units.

**Example**: Code a feature (run 1, Codex, 50 steps) -> verify (run 2, Claude,
10 steps) -> fix (run 3, Codex, 30 steps) -> re-verify (run 2 again). Each run
is a different model, different provider, different token budget. The outer loop
sequences runs and decides what to do with results.

**Key insight**: This is inherently cyclic (code -> verify -> fix -> verify),
so DAGs don't fit. It's not turn-level (each run is many turns). It's not what
conductor does (conductor routes intent, doesn't sequence runs).

**ALM harness collision**: ALM has FanOut and Recurse, but these are within-run
actions. The missing piece is across-run sequencing with cycle support. ALM's
explicit working memory and context composition are relevant though — the
orchestrator needs minimal context (just harness results, not full turn
histories).

**Resolution path**: This is what cagent becomes. Not a CLI that spawns vendor
agents, but a multi-harness orchestrator that sequences, monitors, and verifies
entire agent runs.

## 6. cagent -> choir.go Path

**Progression**:
1. cagent today: CLI that spawns vendor coding agents (Go, ~6 adapters)
2. cagent next: native agent loop (no subprocess, just API + tools, <10MB)
3. cagent after: multi-harness orchestrator (spawn/sequence/verify runs)
4. cagent eventually: choir.go (full runtime with memory, app agents, gateway)

**Rationale**: Go build times matter for rapid iteration on agent logic. Rust
stays for performance-critical outer shell (hypervisor, VM management). Agent
logic (fast-changing, experimental) moves to Go.

**Reference**: PicoClaw proves a full agent loop with tool use fits in <10MB RAM
in Go. Codex CLI (Rust, codex-core library) proves the same in Rust.

## 7. ALM Harness Activation (Deferred)

**Status**: Complete, tested, unused. ADR-0005 recommends activation.

**Useful pieces**: FanOut (parallel dispatch), Recurse (sub-harness delegation),
explicit working memory, context composition via resolve_source.

**Not useful for iteration**: DAGs are acyclic. Code -> verify -> fix -> verify
is a loop. The simple harness loop already handles within-run iteration.

**Decision**: Defer until after BAML removal and writer fix. May be useful for
app-agent-level orchestration (writer dispatching parallel research tasks).
May also be partially superseded by choir.go.

---

## 8. The Core Invariant: Code + Tests + Docs Are Atomic

The fundamental design principle for the harness-level orchestrator (whether
cagent, choir.go, or integrated into ChoirOS): a coding task is not complete
until code, tests, and documentation all cohere. This is an enforced invariant,
not a best practice.

A task graph for "implement feature X" includes:
- Write the code
- Write/update tests
- Verify tests pass
- Update affected docs
- Verify docs reflect the actual implementation

None of these are optional follow-ups. They're all completion criteria for the
same task. The harness won't mark a task green until all are satisfied.

This is what makes autonomous coding tractable. The docs stay current not
because a separate system watches for staleness, and not because a human
remembers to update them, but because the execution framework won't let work
finish without them. Up-to-date docs then provide accurate context for the next
coding task, creating a virtuous cycle.

Writer (the app agent) manages individual living documents. The documentarian
work (updating project docs after code changes) is a researcher-class worker
dispatched as part of a coding task graph — scoped, directed, not autonomous.
It runs because the task definition says "update docs" is a completion step,
not because it independently decided docs were stale.

The second half of this invariant: the docs must be sufficient for a coding
agent to always have work to do. Docs describe what the system should be
(design, contracts, ADRs) and what it currently is (state, reports, test
results). The delta between those two is the task backlog. A coding agent reads
the docs, sees the gap, does the work, updates the docs. The next agent reads
the updated docs, sees the next gap. No human writes tickets. No one triages a
backlog. The docs are the backlog.

This is the self-sustaining loop that makes Choir-on-Choir viable: the system
maintains its own work queue by maintaining its own documentation.

---

## Priority Order by Category

### Bootstrap work (enables choir-on-choir)
1. Writer contract fix (ADR-0021) — workers send signals not diffs, writer
   synthesizes properly. Unblocks writer UX and verification.
2. Provider auth (BYOK) — use own tokens for development. OpenAI OAuth,
   Anthropic paste-token.

### Infra work (invisible to users, enables everything)
3. BAML removal / native tool-use — cost/latency savings, simpler agent loop.
4. microvm.nix fork integration — Firecracker support, better VM transport.
   (In progress, separate session.)

### Product work (what users see)
5. Writer UX (ADR-0021 TLC) — the living document experience.
6. Inception provider adapter — new diffusion-based LLM.

### Endgame work (choir builds choir)
7. cagent native agent loop — no subprocess, just API + tools.
8. Harness-level orchestration — sequence/verify entire agent runs.
9. Documentation agent — live docs index, staleness gating, batch updates.
10. Continuous background research — always-on web research alongside coding.
11. choir.go — full runtime, grows from cagent.

Bootstrap and infra can be parallelized across sessions.
Product work depends on bootstrap (writer contract fix).
Endgame depends on all of the above.

## 9. Documentation-Driven Development and the Work Queue Problem

The current landscape of agent context files (.cursorrules, AGENTS.md, CLAUDE.md,
memory.md) is fragmented and manually maintained. Each tool has its own format.
None enforce consistency. The result is stale context that drifts from reality.

The core insight: if docs accurately describe what the system should be and what
it currently is, the delta is the task backlog. No separate issue tracker needed.
The docs are the work queue.

**Beads (Steve Yegge)** is the closest existing system to this idea:
- Tasks as JSONL in git, hash-based IDs for multi-agent merge safety
- Task graph with dependencies, `bd ready` surfaces unblocked work
- Dolt (version-controlled SQL) for cell-level merge and branching
- Semantic memory decay (compact closed tasks to save context)
- Go (93.2%), actively developed (v0.59.0, high churn)

**What to learn from Beads:**
- `bd ready` pattern: derive work queue from graph state, don't maintain backlog
- Hash-based IDs for multi-agent safety
- Semantic memory decay for context management
- Tasks in the repo so agents always have access

**What not to adopt:**
- Dolt (too heavy for microVMs; SQLite + manual versioning is lighter)
- External task tracker paradigm (our invariant is stronger: code+tests+docs
  atomic, not a TODO list)
- The bloat and churn (rapidly evolving, hard to depend on)

**On Dolt for writer versioning:** Version-controlled SQL would replace writer's
in-process version management (canon snapshots, overlays, proposals) with
branch/merge/diff at the database level. Appealing for merge semantics, but
Dolt is a full MySQL-compatible server — too heavy for a microVM. SQLite with
manual versioning (current approach) is the right tradeoff for now.

**The ChoirOS approach should be:** the docs system (whatever form it takes) is
both the project knowledge base and the implicit work queue. The harness
invariant (code+tests+docs atomic) keeps it current. No separate task tracker.
The format should be simple enough that any coding agent can read and update it
without specialized tooling.

**Tasks are the wrong primitive.** Tasks are lossy compression of context. A
coding agent given a task still has to reconstruct full understanding from
scratch. The richer primitive is the doc itself — ADRs carry intent and
constraints, guides carry the how, state reports carry current reality. The
delta between desired state and current state is the work. No task needed.

**Continuous externalization, not post-hoc summaries.** The core requirement is
that the harness externalizes understanding into docs as it works, not as a
cleanup step. Every decision, failed attempt, discovered constraint gets written
to the relevant doc immediately. "We tried X and it didn't work because Y" is
documentation, not a memory entry or session artifact — it belongs in the ADR
alongside the decision it informed.

This solves the compaction problem. When context is lost (compaction, new
session, different model), the docs already contain everything. Nothing to
compress. Nothing to remember. Just read. The docs are the brain. The model is
temporary compute.

---

## External Research Summary

### Codex CLI (OpenAI)
- Rust (94.5%), Apache 2.0, open source
- `codex exec` headless mode with JSONL streaming
- `codex-core` reusable Rust library ("for building native applications")
- Landlock/Seatbelt sandboxing per platform
- ChatGPT subscription BYOK via OAuth
- 240+ tok/s throughput
- JSON-RPC app-server mode for sidecar use

### PicoClaw (Sipeed)
- Go, MIT, single binary, <10MB RAM, 1s startup on $10 RISC-V hardware
- OAuth2 PKCE for OpenAI, paste-token for Anthropic (merged 2026-02-12)
- Multi-provider (OpenRouter, Anthropic, OpenAI, Gemini)
- Reference for minimal agent architecture and auth patterns

### Goose (Block)
- Rust (57.9%) + TypeScript, Apache 2.0
- CLI + desktop + Docker, MCP-native, model-agnostic
- 32.8k stars, very active, headless CI mode planned

### Legal Notes
- OpenAI: ChatGPT subscription use in third-party apps officially blessed
- Anthropic: Subscription use in third-party apps prohibited (TOS). Individual
  dev/testing fine. BYOK via API keys for distributed use.
