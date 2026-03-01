# Handoff: Documentation Updating and Simplification

**Date:** 2026-02-10
**From:** Refactoring Session (Dual-Path Elimination)
**To:** Documentation Simplification Task
**Status:** ✅ Refactoring Complete - Ready for Documentation Update

---

## What Just Happened (Refactoring Summary)

### Major Architecture Change: Chat Eliminated

**Before (Dual-Path - Messy):**
```
Prompt Bar → Chat → bash tool → direct command (BROKEN)
            ↘ web_search tool → specific terms (BROKEN)

Prompt Bar → Conductor → TerminalAgent (CORRECT)
                    ↘ ResearcherActor (CORRECT)
```

**After (Single-Path - Clean):**
```
Prompt Bar → Conductor → TerminalAgent (natural language objectives)
                    ↘ ResearcherActor (natural language objectives)
```

### Files Deleted (~4,300 lines removed)

**Backend:**
- `sandbox/src/actors/chat.rs` (705 lines)
- `sandbox/src/actors/chat_agent.rs` (2,014 lines)
- `sandbox/src/api/chat.rs` (316 lines)
- `sandbox/src/api/websocket_chat.rs` (633 lines)
- `sandbox/src/supervisor/chat.rs` (313 lines)

**Frontend:**
- `dioxus-desktop/src/components/chat.rs` (1,456 lines)

**Tests:**
- `sandbox/tests/chat_agent_tests.rs`
- `sandbox/tests/chat_api_test.rs`
- `sandbox/tests/chat_superbowl_live_matrix_test.rs`
- `sandbox/tests/websocket_chat_test.rs`

### New Unified Agent Harness

**Created:** `sandbox/src/actors/agent_harness/mod.rs` (37 KB)

- Shared loop framework for TerminalAgent and ResearcherAgent
- `AgentAdapter` trait for worker-specific implementations
- Unified event emission (WorkerTurnReport with findings/learnings)
- Both agents now use the same state machine: RECEIVE → PLAN → EXECUTE → OBSERVE → SYNTHESIZE

### New Observability API

**Created:** `sandbox/src/api/run_observability.rs`

- `GET /api/runs/{run_id}/timeline` endpoint
- Events categorized: conductor_decisions, agent_objectives, agent_planning, agent_results

### E2E Test Suite

**Created:** `sandbox/tests/e2e_conductor_scenarios.rs`

- 6 observation-focused tests for Conductor flows
- All tests passing (verified multiple times)

### Intelligence Report

**Created:** `docs/reports/conductor-intelligence-2026-02-10.md`

- Documents actual Conductor behavior
- Event sequences, decision types, natural language objective flow

---

## Documentation Tasks (What Needs Doing)

### 1. Update Architecture Documentation

**Files to check/update:**
- `docs/architecture/NARRATIVE_INDEX.md` - Main entry point
- `docs/architecture/2026-02-10-conductor-watcher-baml-cutover.md` - May overlap
- `docs/architecture/2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md` - May overlap
- Any docs mentioning ChatActor, ChatAgent, or dual-path

**Required changes:**
- Remove all references to ChatActor/ChatAgent
- Update supervision tree diagrams (no more ChatSupervisor)
- Clarify single-path architecture: Prompt Bar → Conductor → Agents
- Update "NO ADHOC WORKFLOW" rule documentation

### 2. Simplify CLAUDE.md

**Current file:** `/Users/wiz/choiros-rs/CLAUDE.md`

**Issues to fix:**
- Remove Chat-related sections
- Update supervision tree description
- Simplify model policy section (now unified)
- Update "Current High-Priority Development Targets" - many are done
- Clarify agent harness as the standard pattern

**Add new sections:**
- Unified Agent Harness usage pattern
- Conductor-first orchestration examples
- Observability API usage

### 3. Clean Up Roadmap/Planning Docs

**Look for and update:**
- Any docs referencing Chat removal (mark as done)
- Dual-path elimination (mark as done)
- Agent harness creation (mark as done)
- Update "What To Do Next" sections

### 4. API Documentation

**Update:**
- Remove Chat API endpoints from API docs
- Document Conductor API endpoints
- Document Observability API (`/api/runs/{run_id}/timeline`)
- Document Agent Harness interface (for future agents)

### 5. Code Documentation

**Update inline docs:**
- `sandbox/src/actors/agent_harness/mod.rs` - Ensure complete rustdoc
- `sandbox/src/api/run_observability.rs` - Add endpoint documentation
- `sandbox/src/actors/conductor/` - Update module docs

---

## Key Concepts to Document Clearly

### 1. Conductor-First Architecture

**The Rule:** All user objectives go through Conductor. No exceptions.

**Why:** Conductor provides:
- Policy-based decision making
- Multi-agent orchestration
- Failure recovery (retry, fallback, block)
- Observability and tracing
- Natural language objective refinement

### 2. Natural Language Objectives (Not Commands)

**Correct:**
- Conductor receives: "Find out today's weather"
- TerminalAgent receives: "Find out today's weather" (figures out commands)
- ResearcherActor receives: "Get weather information" (figures out search terms)

**Incorrect (Old Chat Pattern - REMOVED):**
- Chat receives: "get weather"
- Chat calls bash with: "curl weather.com" (hardcoded command)
- Chat calls web_search with: "superbowl weather" (hardcoded terms)

### 3. Unified Agent Harness

**Purpose:** Eliminate duplicated loop logic between agents

**Pattern:**
```rust
// Each agent implements AgentAdapter
trait AgentAdapter {
    fn get_model_role(&self) -> &str;  // "terminal" | "researcher"
    fn execute_tool_call(&self, ...) -> Result<ToolExecution>;
    fn emit_worker_report(&self, report: WorkerTurnReport);
}

// Harness runs the loop
let harness = AgentHarness::new(adapter, config);
let result = harness.run(objective).await?;
```

### 4. Observability Through EventStore

**Pattern:** All significant events emitted to EventStore

**Categories:**
- `conductor_decisions` - Policy decisions (Dispatch, Retry, Block, etc.)
- `agent_objectives` - Natural language objectives passed to agents
- `agent_planning` - Planning steps and agenda creation
- `agent_results` - Findings, learnings, artifacts

**API:** `GET /api/runs/{run_id}/timeline?category=agent_results`

---

## Files That Reference Chat (Update These)

### Critical (Likely Outdated)

1. `CLAUDE.md` - Mentions Chat in supervision tree
2. `docs/architecture/NARRATIVE_INDEX.md` - May reference Chat
3. Any docs with "Chat" in the filename

### Check These

```bash
# Find docs mentioning Chat
grep -r "ChatActor\|ChatAgent\|chat_actor\|chat_agent" docs/ --include="*.md"

# Find docs mentioning dual path
grep -r "dual.path\|dual path\|dual-path" docs/ --include="*.md"

# Find docs mentioning old tool pattern
grep -r "bash tool\|web_search tool" docs/ --include="*.md"
```

---

## Success Criteria for Documentation Task

- [ ] All Chat references removed from docs
- [ ] Supervision tree diagrams updated (no ChatSupervisor)
- [ ] Single-path architecture clearly documented
- [ ] Conductor-first rule documented with examples
- [ ] Agent harness pattern documented for future agents
- [ ] Observability API documented
- [ ] CLAUDE.md simplified and accurate
- [ ] NARRATIVE_INDEX.md updated as primary entry point

---

## Key Files for Context

### New Implementation Files
- `sandbox/src/actors/agent_harness/mod.rs` - Read for harness pattern
- `sandbox/src/api/run_observability.rs` - Read for observability API
- `sandbox/tests/e2e_conductor_scenarios.rs` - Read for Conductor behavior examples

### Intelligence Report
- `docs/reports/conductor-intelligence-2026-02-10.md` - Read for actual behavior documentation

### Current Architecture (Post-Refactor)
```
sandbox/src/supervisor/mod.rs - Supervision tree
sandbox/src/actors/conductor/ - Conductor implementation
sandbox/src/actors/terminal.rs - TerminalAgent (uses harness)
sandbox/src/actors/researcher/ - ResearcherAgent (uses harness)
```

---

## Questions?

If unclear about any aspect of the refactoring:
1. Check the intelligence report: `docs/reports/conductor-intelligence-2026-02-10.md`
2. Check the E2E tests: `sandbox/tests/e2e_conductor_scenarios.rs`
3. Check the harness implementation: `sandbox/src/actors/agent_harness/mod.rs`

---

**Bottom Line:** Chat is gone. Conductor is the only path. Agents receive natural language objectives, not commands. The harness unifies agent loop logic. Document this clearly and remove all outdated dual-path references.
