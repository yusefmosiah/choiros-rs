# Coding Session Prompt: Remove Dual Path, Simplify to Conductor-First

**Session Goal:** Eliminate dual-path confusion by removing Chat entirely. Conductor is the only orchestration path.

## Root Problem: Dual Path Confusion (The Big Mess)

**Chat Path (Obsolete Bootstrap):**
- Chat has "bash" and "web_search" tools
- These aren't real tools - they route to models with specific commands/terms
- Example: Chat calls "curl weather.com" or searches for "superbowl weather"
- This is wrong because it bypasses agent-level planning

**Conductor Path (Correct):**
- Conductor gives natural language objectives to agents
- TerminalAgent receives: "Find out today's weather" (figures out commands itself)
- ResearcherActor receives: "Get weather information" (figures out search terms itself)
- Agents use their own agentic loops to accomplish objectives

**The Fix:**
- Remove Chat entirely (it was just bootstrap to get started)
- Conductor is the ONLY orchestration path
- Prompt Bar → Conductor → Agents (Terminal, Researcher, ...)
- No more dual path logic

## Strategic Context

**Current Architecture (Wrong - Has Dual Path):**
```
Prompt Bar → Chat → bash tool → direct command (BROKEN)
            ↘ web_search tool → specific terms (BROKEN)

Prompt Bar → Conductor → TerminalAgent → natural language objective (CORRECT)
                    ↘ ResearcherActor → natural language objective (CORRECT)
```

**Target Architecture (Correct - Single Path):**
```
Prompt Bar → Conductor → TerminalAgent (figures out commands)
                    ↘ ResearcherActor (figures out searches)
                    ↘ [Future Agents]
```

**What Conductor Actually Does:**
- Receives natural language objective from Prompt Bar
- Decides which agents to dispatch (via BAML policy)
- Sends natural language directives to agents
- Agents plan and execute using their own loops
- Conductor aggregates results and emits final response

## Workstreams (Execute in Parallel Subagents)

---

### Workstream 1: E2E Intelligence for Conductor Flows

**Subagent Task: Research → Create → Run → Analyze**

**Step 1: Research Phase (1-2 hours)**
Using `explore` subagent:
- Find existing Conductor integration tests
- Find existing tests that call Conductor agents (TerminalActor, ResearcherActor)
- Document: What Conductor flows actually work today?

**Context to Aggregate:**
- Which tests demonstrate Conductor → TerminalActor delegation?
- Which tests demonstrate Conductor → ResearcherActor delegation?
- What event sequences are emitted in working flows?
- What's the current Prompt Bar → Conductor path?

**Step 2: Create Test Suite (2-3 hours)**
Create `sandbox/tests/e2e_conductor_scenarios.rs` focusing ONLY on Conductor paths:

```rust
// These are FOR OBSERVATION, not gate-keeping
#[tokio::test]
async fn test_conductor_to_terminal_delegation() {
    // Objective: "list files in sandbox"
    // Observe: Does Conductor dispatch to TerminalActor?
    // Document: What natural language objective is passed?
}

#[tokio::test]
async fn test_conductor_to_researcher_delegation() {
    // Objective: "get weather for superbowl"
    // Observe: Does Conductor dispatch to ResearcherActor?
    // Document: What natural language directive is passed?
}

#[tokio::test]
async fn test_conductor_multi_agent_delegation() {
    // Objective: "research superbowl weather then save results to file"
    // Observe: Does Conductor dispatch to both Researcher and Terminal?
    // Document: How are results aggregated?
}

#[tokio::test]
async fn test_conductor_synthesis() {
    // Multi-agent run
    // Observe: Does Conductor synthesize final response?
    // Document: How are agent results combined?
}

// Add more scenarios as needed based on research findings
```

**Key Instructions:**
- Tests should ASSERT NOTHING initially - just OBSERVE and DOCUMENT
- Use `log::info!()` to document what you see
- Focus ONLY on Conductor paths (ignore Chat paths, they're going away)
- Document natural language objectives/directives passed to agents

**Step 3: Run and Analyze (1 hour)**
Run all tests and create findings document:
```
docs/reports/conductor-intelligence-2026-02-10.md
```

Include:
- Which tests passed/failed and why
- What natural language objectives are passed to agents
- What event sequences you observe
- How agents currently behave with natural language directives
- Recommendations for Workstream 2 (agent harness) and Workstream 3 (Chat removal)

---

### Workstream 2: Unified Agent Harness (Terminal + Researcher Only)

**Subagent Task: Research → Extract → Migrate → Remove Duplication**

**Clarification:**
- Chat is being REMOVED, don't include it
- Focus on TerminalActor and ResearcherAgent
- These are the agents Conductor delegates to

**Step 1: Research Phase (1-2 hours)**
Using `explore` subagent:
- Read TerminalActor's loop implementation (`sandbox/src/actors/terminal.rs`)
- Read ResearcherActor's loop implementation (`sandbox/src/actors/researcher/mod.rs`)
- Document: What's duplicated? What's unique?

**Context to Aggregate:**
- How does TerminalAgent receive natural language objectives from Conductor?
- How does ResearcherAgent receive natural language directives from Conductor?
- What loop logic is identical (step caps, timeout, BAML planning)?
- Which agent already has typed worker events?
- What are the tool surfaces for each agent?

**Step 2: Extract Shared Harness (3-4 hours)**
Create `sandbox/src/actors/agent_harness/mod.rs`:

```rust
pub struct AgentHarness<A: AgentAdapter> {
    adapter: A,
    model_registry: ModelRegistry,
    timeout_budget: Duration,
    max_steps: usize,
}

pub trait AgentAdapter: Send + Sync {
    fn get_model_role(&self) -> &str;  // "terminal" | "researcher"
    fn get_tool_description(&self) -> &str;
    fn execute_tool_call(&self, tool_name: &str, args: &Value) -> Result<ToolResult>;
    fn should_defer(&self, tool_name: &str) -> bool;
}

impl<A: AgentAdapter> AgentHarness<A> {
    pub async fn run(&self, objective: &str) -> AgentResult {
        // Shared state machine:
        // RECEIVE_OBJECTIVE → PLAN_STEP → EXECUTE_TOOLS → OBSERVE_RESULTS → PLAN_STEP or SYNTHESIZE
        // Uses BAML PlanAction and SynthesizeResponse
    }
}
```

**Use Researcher as Reference:**
- Researcher already receives natural language objectives
- Researcher already emits `worker.task.started/completed/failed` events
- Researcher builds `WorkerTurnReport` with findings/learnings

**Step 3: Migrate Agents (3-4 hours)**

1. **Researcher** (easiest - reference implementation):
   - Move loop logic into harness
   - Keep only ResearcherAdapter for provider selection
   - Verify it still receives and plans from natural language objectives
   - Verify tests still pass

2. **Terminal** (medium):
   - Move `run_agentic_task` into harness
   - Create TerminalAdapter for bash tool execution
   - Verify it receives natural language objectives (not specific commands)
   - Verify it plans commands using BAML (not hardcoded)
   - Verify step caps/timeout now use shared logic
   - Verify `emit_progress` replaced with typed worker events

**Step 4: Remove Duplication (1-2 hours)**
Delete old loop code from each agent after migration verified.

**Key Instructions:**
- Focus ONLY on Terminal and Researcher (Chat is being removed)
- Keep backward compatibility during migration
- Run existing tests after each agent migration
- Don't break current working Conductor → Agent flows

---

### Workstream 3: Remove Chat Entirely

**Subagent Task: Research → Identify Dependencies → Remove → Test**

**Step 1: Research Phase (1 hour)**
Using `explore` subagent:
- Find all Chat-related code:
  - `sandbox/src/actors/chat.rs`
  - `sandbox/src/actors/chat_agent.rs`
  - `sandbox/src/api/chat.rs`
  - `sandbox/src/api/websocket_chat.rs`
  - `dioxus-desktop/src/components/chat.rs`
- Identify dependencies:
  - What depends on ChatActor?
  - What depends on Chat API endpoints?
  - What depends on Chat WebSocket?
  - What uses Chat's bash/web_search tools?

**Context to Aggregate:**
- Is Chat used anywhere outside of manual testing?
- Does Prompt Bar currently route to Chat or Conductor?
- Are there any critical workflows that depend on Chat?
- What frontend components reference Chat?

**Step 2: Remove Chat Backend (2-3 hours)**
In order:

1. **Stop routing to Chat**:
   - Check Prompt Bar routing (likely in supervisor or app state)
   - Change routing from Chat → Conductor
   - Test: Does Prompt Bar now go to Conductor?

2. **Remove ChatActor**:
   - Delete `sandbox/src/actors/chat.rs`
   - Delete `sandbox/src/actors/chat_agent.rs`
   - Remove from supervision tree
   - Update imports

3. **Remove Chat API**:
   - Delete `sandbox/src/api/chat.rs`
   - Remove routes from `sandbox/src/api/mod.rs`
   - Delete `sandbox/src/api/websocket_chat.rs`

4. **Update Conductor**:
   - Ensure Conductor handles all objectives that Chat was handling
   - Test: Can Conductor handle simple queries that Chat used to handle?

**Step 3: Remove Chat Frontend (1-2 hours)**
1. **Remove Chat component**:
   - Delete `dioxus-desktop/src/components/chat.rs`
   - Remove from `components.rs` exports
   - Remove from desktop icon grid

2. **Update Prompt Bar**:
   - Ensure it sends directly to Conductor API
   - Remove any Chat-specific code paths

**Step 4: Verification (1 hour)**
Run manual tests:
```
# Start server
just dev-sandbox

# Test Conductor directly
curl -X POST http://localhost:8080/api/conductor/execute \
  -H "Content-Type: application/json" \
  -d '{"objective": "list files in sandbox"}'

# Test another objective
curl -X POST http://localhost:8080/api/conductor/execute \
  -H "Content-Type: application/json" \
  -d '{"objective": "get weather information"}'

# Test multi-agent
curl -X POST http://localhost:8080/api/conductor/execute \
  -H "Content-Type: application/json" \
  -d '{"objective": "research superbowl weather and save to file"}'
```

Verify:
- All objectives handled by Conductor
- TerminalAgent and ResearcherAgent receive natural language directives
- Responses are synthesized by Conductor
- No Chat errors in logs

**Step 5: Update Tests (30 minutes)**
- Remove or update tests that depend on Chat
- Ensure Conductor tests still pass
- Update test documentation to reflect new architecture

---

### Workstream 4: Observability API (Based on Conductor Intelligence)

**Subagent Task: Research (wait for Workstream 1) → Design → Implement**

**Step 1: Wait for Conductor Intelligence**
Don't start until `docs/reports/conductor-intelligence-2026-02-10.md` exists.

**Step 2: Design Based on Conductor Flows (1 hour)**
Read Conductor intelligence report:
- What natural language objectives are actually passed?
- What event sequences occur in Conductor → Agent flows?
- Which events are useful for debugging Conductor behavior?

Design `GET /api/runs/{run_id}/timeline` response focused on:
- Conductor decisions (which agents to dispatch)
- Agent objectives (natural language directives)
- Agent planning steps (how they accomplish objectives)
- Agent results (findings, learnings)
- Final synthesis

**Key Design Principle:**
- Focus on Conductor orchestration, not Chat paths (Chat is gone)
- Base design on actual Conductor behavior from E2E tests
- Keep it simple

**Step 3: Implement (2-3 hours)**
Create `sandbox/src/api/run_observability.rs`:
- Query EventStore for run events
- Build ordered timeline
- Categorize events: conductor decisions, agent objectives, agent planning, agent results
- Filter by category if requested

Add route to `sandbox/src/api/mod.rs`.

**Step 4: Manual Verification (30 minutes)**
Run Conductor executions and query timeline:
```
# Submit objective
curl -X POST http://localhost:8080/api/conductor/execute \
  -H "Content-Type: application/json" \
  -d '{"objective": "list files"}'

# Query timeline
curl http://localhost:8080/api/runs/{run_id}/timeline
```

Verify timeline shows:
- Conductor dispatch decisions
- Agent natural language objectives
- Agent planning steps
- Agent results

---

## Execution Order

**Recommended:**
1. Start Workstream 1 (creates Conductor intelligence for all others)
2. Workstream 1 Step 1 (research) runs in parallel with Workstream 2 Step 1 (research) and Workstream 3 Step 1 (research)
3. Workstream 4 waits for Workstream 1 intelligence
4. Workstreams 2 and 3 run in parallel after research phases

**Parallel Launch:**
```
Session Start
├─ Subagent: Workstream 1 Research (Conductor flows)
├─ Subagent: Workstream 2 Research (Terminal/Researcher loops)
└─ Subagent: Workstream 3 Research (Chat dependencies)

[After research phases complete]
├─ Subagent: Workstream 1 Create Tests
├─ Subagent: Workstream 1 Run Tests & Analyze
├─ Subagent: Workstream 2 Extract Harness
├─ Subagent: Workstream 2 Migrate Agents
└─ Subagent: Workstream 3 Remove Chat Backend

[After Workstream 1 intelligence ready]
├─ Subagent: Workstream 4 Design Observability
├─ Subagent: Workstream 4 Implement
└─ Subagent: Verify Final System
```

---

## Success Criteria (Per Workstream)

**Workstream 1 (Conductor E2E Intelligence):**
- [ ] Conductor → Terminal delegation test created
- [ ] Conductor → Researcher delegation test created
- [ ] Multi-agent Conductor test created
- [ ] Conductor synthesis test created
- [ ] Tests run and document actual behavior
- [ ] `docs/reports/conductor-intelligence-2026-02-10.md` created
- [ ] Findings include: natural language objectives passed, event sequences, what works, what needs simplification

**Workstream 2 (Agent Harness):**
- [ ] `sandbox/src/actors/agent_harness/mod.rs` created (NOT including Chat)
- [ ] Researcher migrated to harness (tests pass)
- [ ] Terminal migrated to harness (tests pass)
- [ ] Both agents receive natural language objectives from Conductor
- [ ] Both agents plan using BAML (not hardcoded)
- [ ] Duplicate loop logic removed from both agents
- [ ] Step caps and timeout logic now shared

**Workstream 3 (Remove Chat):**
- [ ] All Chat dependencies identified and documented
- [ ] Prompt Bar routes to Conductor (not Chat)
- [ ] ChatActor deleted
- [ ] Chat API endpoints deleted
- [ ] Chat WebSocket deleted
- [ ] Chat frontend component deleted
- [ ] Manual verification: all objectives handled by Conductor
- [ ] Chat references removed from tests

**Workstream 4 (Conductor Observability):**
- [ ] `GET /api/runs/{run_id}/timeline` implemented
- [ ] Response includes: conductor decisions, agent objectives, agent planning, agent results
- [ ] Events categorized by Conductor orchestration stage
- [ ] Manual verification passes with Conductor runs

---

## Risk Mitigation

**Don't Break Conductor During Chat Removal:**
- Verify Conductor can handle ALL objectives before removing Chat
- Keep Chat code around until Conductor path verified
- Run manual tests for different objective types (files, research, multi-agent)

**Don't Break Agent Natural Language Processing:**
- Agents MUST continue receiving natural language objectives
- Agents MUST plan using BAML (not hardcoded commands/terms)
- Run agent-specific tests after harness migration

**Don't Block on Tests:**
- Tests are for learning, not gatekeeping
- If a test reveals a bug, decide: fix now or document and move on
- Goal: Observe Conductor behavior, not achieve 100% test pass rate

---

## Deliverables

After session, expect:
1. `sandbox/tests/e2e_conductor_scenarios.rs` - Conductor flow tests
2. `docs/reports/conductor-intelligence-2026-02-10.md` - behavior observations
3. `sandbox/src/actors/agent_harness/mod.rs` - shared agent harness (Terminal + Researcher)
4. Both TerminalActor and ResearcherActor using shared harness
5. Chat entirely removed (backend + frontend)
6. Prompt Bar → Conductor routing only
7. `sandbox/src/api/run_observability.rs` - Conductor timeline endpoint
8. Updated `progress.md` with session outcomes

---

## Getting Started

1. Read this entire prompt carefully
2. Launch explore subagent for Workstream 1 research
3. Launch explore subagent for Workstream 2 research (parallel)
4. Launch explore subagent for Workstream 3 research (parallel)
5. Aggregate findings
6. Proceed to implementation phases

**Key Insight:** Chat is bootstrap code that's now obsolete. Remove it entirely, let Conductor be the single orchestration path. Agents receive natural language objectives, not specific commands/terms.

Good luck! Focus on eliminating dual path confusion and simplifying to Conductor-first architecture.
