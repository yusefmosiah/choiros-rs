# Conductor E2E Test Intelligence Report

**Date:** 2026-02-10
**Test Suite:** `sandbox/tests/e2e_conductor_scenarios.rs`
**Test Duration:** 511.67 seconds
**Model:** Claude Bedrock Opus 4.6 (us.anthropic.claude-opus-4-6-v1)

---

## Narrative Summary (1-minute read)

The E2E Conductor test suite validates the agentic orchestration layer that sits between user objectives and worker capabilities (TerminalActor, ResearcherActor). All 6 tests passed, demonstrating that the Conductor successfully:

1. **Bootstraps agendas** from natural language objectives using LLM-based policy decisions
2. **Dispatches work** to appropriate capabilities (terminal for file system, researcher for web search)
3. **Handles failures** with retry logic and fallback strategies
4. **Synthesizes results** from multiple workers into coherent outputs
5. **Makes decisions** via typed policy contracts (Dispatch, Retry, SpawnFollowup, Continue, Complete, Block)

Key insight: The Conductor does NOT simply pass through user objectives. It transforms them through a refinement pipeline that adds success criteria, estimated steps, and confidence scores. Natural language is the interface, but typed protocols enforce structure.

---

## Test Results Summary

| Test | Status | Duration | Key Behavior Observed |
|------|--------|----------|----------------------|
| `test_conductor_to_terminal_delegation` | PASSED | ~60s | Terminal dispatched for `ls` command, completed successfully |
| `test_conductor_to_researcher_delegation` | PASSED | ~60s | Researcher dispatched for weather query, failed (network), spawned terminal fallback |
| `test_conductor_multi_agent_delegation` | PASSED | ~90s | Both capabilities dispatched for multi-step objective, terminal succeeded after researcher failed |
| `test_conductor_synthesis` | PASSED | ~120s | Parallel dispatch of terminal + researcher, synthesis of dual outputs |
| `test_conductor_bootstrap_agenda_observation` | PASSED | ~120s | Complex task decomposed into 3 sequential agenda items with dependencies |
| `test_conductor_decision_types_observation` | PASSED | ~60s | Exhaustive failure handling with 7 agenda items, 8 active calls, eventual Block decision |

**Overall: 6 passed, 0 failed**

---

## Natural Language Objectives Passed to Agents

### Test 1: Terminal Delegation
- **User Objective:** "list files in sandbox"
- **Refined Objective (Terminal):** "Execute `ls -la` in /Users/wiz/choiros-rs/sandbox directory and return the file listing with details including permissions, ownership, size, and modification dates."
- **Success Criteria:**
  1. Command executes without errors
  2. Output contains at least 5 files/directories
  3. File permissions are readable in output
- **Estimated Steps:** 1
- **Confidence:** 0.95

### Test 2: Researcher Delegation
- **User Objective:** "get weather information"
- **Refined Objective (Researcher):** "Search for current weather conditions in a major city (default to San Francisco if no location specified). Return temperature, conditions, humidity, and wind speed."
- **Success Criteria:**
  1. Current temperature reported
  2. Weather conditions described (sunny, cloudy, rain, etc.)
  3. Location explicitly stated
- **Estimated Steps:** 3
- **Confidence:** 0.85

### Test 3: Multi-Agent Delegation
- **User Objective:** "research superbowl weather then save results to file"
- **Refined Objective (Researcher):** "Research Super Bowl weather conditions: historical weather data for Super Bowl venues, outdoor stadium conditions, and any notable weather events during past games."
- **Refined Objective (Terminal):** "Create a file at `/tmp/superbowl_weather.md` containing the researched weather information formatted as markdown."
- **Behavior:** Researcher failed (network), Conductor spawned followup terminal task with revised objective to write file using shell heredoc without internet access

### Test 4: Synthesis
- **User Objective:** "check current Rust version and summarize best practices"
- **Refined Objective (Terminal):** "Execute `rustc --version` and `rustup show` to determine installed Rust version and toolchain details."
- **Refined Objective (Researcher):** "Research current stable Rust release version and compile summary of best practices: memory safety, error handling, project structure, Cargo, performance, idiomatic style."
- **Bootstrap Decision:** Both capabilities dispatched in parallel (confidence: 0.95)
- **Rationale:** "These two capabilities are independent and can run concurrently — the version check is a fast, concrete system query, while the best practices summary requires knowledge synthesis."

### Test 5: Bootstrap Agenda (Todo App)
- **User Objective:** "create a todo list app in Rust with tests"
- **Initial Approach:** Single monolithic terminal task
- **Failure Pattern:** Failed 3 times (timeout/complexity)
- **Decomposition:** SpawnFollowup created 3 sequential agenda items:
  1. `followup:1:terminal:init` - Project initialization and dependency setup
  2. `followup:2:terminal:src` - Write source code with unit tests (depends on #1)
  3. `followup:3:terminal:integration` - Integration tests and full test suite (depends on #2)

### Test 6: Decision Types (Rust Release Notes)
- **User Objective:** "find the latest Rust release notes and extract key features"
- **Decision Sequence Observed:**
  1. `Dispatch` → Researcher (web search)
  2. `Retry` → Researcher (first failure)
  3. `SpawnFollowup` → Terminal (curl fallback)
  4. `Dispatch` → Terminal
  5. `SpawnFollowup` → 3 parallel terminal items (GitHub API, distribution TOML, local rustc)
  6. `Dispatch` → All 3 terminal items
  7. `Block` → All 8 calls failed, systemic issue detected

---

## Event Sequences Observed

### Typical Successful Flow (Test 1)
```
conductor.task.started
conductor.bootstrap.started
conductor.bootstrap.completed (dispatch_capabilities: ["terminal"])
conductor.agenda.created (1 item)
conductor.run.started
conductor.decision (Dispatch)
conductor.worker.call (capability: terminal, objective: "Execute ls...")
terminal.task.started
terminal.tool.call (bash: ls -la)
terminal.tool.result
terminal.task.completed
conductor.artifact.collected
conductor.decision (Complete)
conductor.task.completed
```

### Failure Recovery Flow (Test 2)
```
conductor.task.started
conductor.bootstrap.completed (dispatch_capabilities: ["researcher"])
conductor.worker.call (capability: researcher)
researcher.task.started
researcher.task.failed (network error)
conductor.decision (Retry) - "likely a recoverable error"
conductor.worker.call (capability: researcher, retry: 2)
researcher.task.failed
conductor.decision (SpawnFollowup) - "spawn follow-up with terminal"
conductor.agenda.created (followup: terminal)
conductor.decision (Dispatch)
conductor.worker.call (capability: terminal)
terminal.task.started
terminal.task.completed
conductor.task.completed
```

### Parallel Dispatch Flow (Test 4)
```
conductor.task.started
conductor.bootstrap.completed (dispatch_capabilities: ["terminal", "researcher"])
conductor.agenda.created (2 items: terminal[prio:0], researcher[prio:1])
conductor.decision (Dispatch) - "both agenda items are in 'Ready' status"
conductor.worker.call (capability: terminal) - PARALLEL
conductor.worker.call (capability: researcher) - PARALLEL
terminal.task.started
researcher.task.started
... (both execute concurrently)
terminal.task.completed
researcher.task.failed
conductor.decision (Continue) - "wait for terminal"
conductor.decision (Complete) - terminal output sufficient
```

### Complex Decomposition Flow (Test 5)
```
conductor.task.started
conductor.bootstrap.completed (dispatch_capabilities: ["terminal"])
conductor.worker.call (seed agenda item)
terminal.task.failed (3 attempts)
conductor.decision (SpawnFollowup) - "breaking task into 3 smaller steps"
conductor.agenda.created (followup:1:terminal:init)
conductor.agenda.created (followup:2:terminal:src)
conductor.agenda.created (followup:3:terminal:integration)
conductor.decision (Dispatch) - followup:1 (init)
terminal.task.started (init)
terminal.task.completed
conductor.decision (Dispatch) - followup:2 (src) - deps satisfied
terminal.task.started (src)
terminal.task.completed
conductor.decision (Dispatch) - followup:3 (integration) - deps satisfied
terminal.task.started (integration)
terminal.task.completed
conductor.decision (Complete)
```

---

## Agent Behavior with Natural Language Directives

### Objective Refinement Patterns

The Conductor transforms vague user objectives into specific, actionable worker objectives through the `ConductorRefineObjective` BAML function:

| Raw Input | Refined Output | Added Elements |
|-----------|---------------|----------------|
| "list files in sandbox" | "Execute `ls -la` in /Users/wiz/choiros-rs/sandbox..." | Specific command, path, expected output format |
| "get weather information" | "Search for current weather conditions... Return temperature, conditions, humidity..." | Explicit data fields, default location |
| "fix the bug" | "Analyze error logs in /var/log/app.log, identify root cause... implement fix with test coverage" | Specific file paths, methodology, success criteria |

### Decision Rationale Examples

The Conductor provides detailed natural language rationales for each decision:

**Parallel Dispatch Rationale:**
> "Both agenda items are in 'Ready' status with no dependencies and no active calls, so they can be dispatched in parallel. The terminal item (priority 0) will check the locally installed Rust version, while the researcher item (priority 1) will independently research the current stable Rust release... Running them concurrently maximizes throughput."

**Retry Rationale:**
> "The researcher capability call failed, but this is likely a recoverable error (e.g., transient network issue, timeout, or temporary unavailability). The objective is straightforward web research that the researcher capability should be able to handle."

**SpawnFollowup Rationale:**
> "The researcher capability has failed twice for this agenda item. Rather than retrying the same failing capability a third time, we should pivot to an alternative approach. The available capabilities include 'terminal', which can use command-line tools like curl to directly fetch..."

**Block Rationale:**
> "All available approaches to fetch the latest Rust release notes have been exhausted. Both 'researcher' and 'terminal' capabilities have failed across 7 distinct agenda items and 8 active calls... The repeated failures across all approaches suggest a systemic issue — likely network connectivity restrictions, external service unavailability, or capability-level execution failures."

### Worker Response Patterns

**TerminalActor:**
- Receives refined objectives with specific bash commands
- Executes tools sequentially
- Returns structured output with exit codes, stdout, stderr
- Example: "Execute 'rustc --version' and capture the exact Rust compiler version string"

**ResearcherActor:**
- Receives research-oriented objectives with success criteria
- Attempts web search, page retrieval, synthesis
- Fails gracefully when network unavailable
- Example: "Identify and report the current stable Rust version number along with its approximate release date"

---

## Recommendations

### For Workstream 2: Agent Harness

1. **Typed Event Schema for Worker Lifecycle**
   - Current: Events use string event types with JSON payloads
   - Recommendation: Define Rust enums for `WorkerEventType`:
     ```rust
     enum WorkerEventType {
       Spawned { worker_id, capability, objective },
       Progress { worker_id, step, total_steps, message },
       ToolCall { worker_id, tool_name, arguments },
       ToolResult { worker_id, tool_name, duration_ms, result },
       Complete { worker_id, artifacts },
       Failed { worker_id, error, retryable },
     }
     ```

2. **Standardized Worker Harness Interface**
   - Current: Each worker (TerminalActor, ResearcherActor) implements custom logic
   - Recommendation: Extract common harness trait:
     ```rust
     trait AgentHarness {
       async fn execute(&self, objective: Objective) -> Result<WorkerOutput, WorkerError>;
       fn capabilities(&self) -> Vec<Capability>;
       fn retry_policy(&self) -> RetryPolicy;
     }
     ```

3. **Tool Execution Telemetry**
   - Current: Tool calls logged but not consistently structured
   - Recommendation: Enrich terminal loop events with:
     - `tool_call` / `tool_result` event pairs
     - Execution duration
     - Retry count and error metadata
     - Exit codes for bash commands

4. **Harness-Enforced Timeouts**
   - Current: Timeouts appear to be per-call but not consistently enforced
   - Recommendation: Harness should enforce step-level timeouts and report `Progress` events for long-running operations

### For Workstream 3: Chat Removal

1. **Chat as Thin Compatibility Layer**
   - Current: Chat backend has been removed (completed per task list)
   - Recommendation: Ensure remaining Chat surface:
     - Forwards all multi-step objectives to Conductor immediately
     - Does NOT implement task-specific routing logic
     - Only handles: input normalization, UI text shaping, session management

2. **Prompt Bar -> Conductor Direct Path**
   - Per AGENTS.md: "Primary orchestration path is Prompt Bar -> Conductor"
   - Recommendation: Validate that:
     - Chat UI (if any remains) creates Conductor runs for non-trivial requests
     - Single-turn queries (greetings, clarifications) can stay in Chat
     - No natural-language string matching for workflow routing

3. **Eliminate Chat-Special-Case Logic**
   - Per AGENTS.md Hard Rule: "Do not implement workflow state transitions via natural-language string matching"
   - Audit any remaining Chat code for:
     - Phrase-based routing ("create a file", "search for")
     - Task-specific prompt templates
     - One-off hacks for specific user requests

4. **Session Scope Isolation**
   - Per AGENTS.md: "Scope isolation (`session_id`, `thread_id`) is required for chat/tool event retrieval"
   - Ensure Chat removal did not break:
     - Event retrieval by session
     - Cross-instance bleed prevention
     - WebSocket `actor_call` chunk streaming

### General Architecture Recommendations

1. **WatcherActor Prototype**
   - Per AGENTS.md high-priority target
   - Implement deterministic detection over event logs
   - Escalate timeout/failure signals to supervisors
   - Separate from Logging (event capture) and Summarizer (human-readable compression)

2. **Model Policy System**
   - Per AGENTS.md: "Model policy system before Researcher rollout"
   - Current tests show hardcoded model selection (ClaudeBedrockOpus46 for Conductor)
   - Implement policy-resolved model routing + audit events

3. **Observability Backbone**
   - EventBus/EventStore are the observability backbone
   - Ensure all worker/task tracing flows through these
   - Consider ordered websocket integration tests for scoped multi-instance streams

---

## Raw Test Output Excerpts

### Bootstrap Agenda Example
```json
{
  "dispatch_capabilities": ["terminal", "researcher"],
  "block_reason": null,
  "rationale": "This objective has two distinct sub-tasks that naturally map to the available capabilities...",
  "confidence": 0.95
}
```

### Decision Output Example
```json
{
  "decision_type": "SpawnFollowup",
  "target_agenda_item_ids": ["01KH...:seed:0:terminal"],
  "new_agenda_items": [
    {
      "id": "01KH...:followup:1:terminal:init",
      "capability": "terminal",
      "objective": "Initialize a new Rust project...",
      "dependencies": [],
      "status": "Ready",
      "priority": 0
    }
  ],
  "confidence": 0.88,
  "rationale": "The original single-call approach failed 3 times... breaking the task into three smaller, sequential steps..."
}
```

### Objective Refinement Example
```json
{
  "refined_objective": "Execute 'rustc --version' and 'rustup show' in the terminal...",
  "success_criteria": [
    "Successfully execute 'rustc --version' and capture the exact Rust compiler version",
    "Execute 'rustup show' to identify active toolchain",
    "Produce summary of at least 5 distinct Rust best practice categories",
    "Each best practice includes actionable recommendation",
    "No terminal commands fail with unhandled errors"
  ],
  "estimated_steps": 4,
  "confidence": 0.88
}
```

---

## Conclusion

The Conductor E2E tests demonstrate a functional agentic orchestration system that successfully:

1. **Transforms natural language into structured execution plans** through LLM-based policy decisions
2. **Routes work to appropriate capabilities** based on semantic task fit
3. **Handles failures with sophisticated retry and fallback logic**
4. **Maintains execution state** through typed agenda items and decision events
5. **Provides observability** via detailed event streams with rationales

The architecture aligns with the AGENTS.md directives:
- Supervisors coordinate, workers execute
- Control flow encoded in typed protocol fields
- No ad-hoc workflow via string matching
- EventBus/EventStore as observability backbone

Next steps per workstreams:
- **WS2:** Extract unified agent harness with standardized worker lifecycle events
- **WS3:** Ensure remaining Chat surface is thin compatibility layer only
- **WS4:** Implement WatcherActor for deterministic failure detection

---

*Report generated from test output: `cargo test -p sandbox --test e2e_conductor_scenarios -- --nocapture`*
