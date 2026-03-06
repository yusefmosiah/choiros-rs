# Simplified Agent Harness

## Narrative Summary (1-minute read)

The agent harness has been simplified from a complex multi-phase loop to a clean, unified DECIDE -> EXECUTE -> (loop/return) pattern. This document describes the new architecture, worker integration patterns, and how Conductor spawns harness-based workers.

## What Changed

### Before: Complex Multi-Phase Loop

The previous harness had separate phases for planning, execution, and synthesis with complex state transitions:
- `Planning` -> `Executing` -> `Synthesizing` -> `Complete/Blocked`
- Multiple BAML functions for different phases
- Complex message passing between phases
- Hard to reason about state transitions

### After: Simplified Unified Loop

The new harness uses a single `Decide` function that returns one of three actions:
- `ToolCall` - Execute tools and continue the loop
- `Complete` - Done, return summary
- `Block` - Stuck, return reason

Key changes:
1. **Single BAML function** (`Decide`) instead of multiple phase-specific functions
2. **Simplified `AgentDecision` type** with `action`, `tool_calls`, `summary`, `reason`
3. **Unified loop** in `AgentHarness::run()` - no phase transitions
4. **WorkerTurnReport** emitted at completion with findings, learnings, escalations
5. **Progress events** streamed throughout execution

## New Architecture

### Core Loop Pattern

```rust
// Simplified loop: DECIDE -> EXECUTE -> (loop or return)
while step_count < max_steps && loop_state == Running {
    step_count += 1;

    // 1. DECIDE: Call BAML Decide to get action
    let decision = self.decide(&client_registry, &messages, &ctx).await?;

    match decision.action {
        Action::ToolCall => {
            // 2. EXECUTE: Execute tools from decision.tool_calls
            for tool_call in &decision.tool_calls {
                let tool_result = self.adapter.execute_tool_call(&ctx, tool_call).await;
                // Add result to messages for next decision round
                messages.push(BamlMessage { ... });
            }
            // Continue loop for next decision
        }
        Action::Complete => {
            // Return summary
            loop_state = Complete;
            break;
        }
        Action::Block => {
            // Return reason
            loop_state = Blocked;
            break;
        }
    }
}

// Emit WorkerTurnReport at completion
let report = self.adapter.build_worker_report(&ctx, &final_summary, success);
self.adapter.emit_worker_report(&ctx, report).await?;
```

### Key Types

```rust
// Simplified decision from BAML
pub struct AgentDecision {
    pub action: Action,              // ToolCall | Complete | Block
    pub tool_calls: Vec<AgentToolCall>,  // Empty if Complete/Block
    pub summary: Option<String>,     // Present if Complete
    pub reason: Option<String>,      // Present if Block
}

pub enum Action {
    ToolCall,
    Complete,
    Block,
}

// Tool call specification
pub struct AgentToolCall {
    pub tool_name: String,
    pub tool_args: AgentToolArgs,
    pub reasoning: Option<String>,
}

// Unified tool args (all tools in one struct with optional fields)
pub struct AgentToolArgs {
    // Nested per-tool args
    pub bash: Option<BashToolArgs>,
    pub read_file: Option<ReadFileToolArgs>,
    pub write_file: Option<WriteFileToolArgs>,
    // ... etc

    // Legacy flat fields for compatibility
    pub command: Option<String>,
    pub path: Option<String>,
    pub content: Option<String>,
    // ... etc
}
```

### AgentAdapter Trait

Workers implement this trait to customize harness behavior:

```rust
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Model role for model resolution (e.g., "terminal", "researcher")
    fn get_model_role(&self) -> &str;

    /// Tool description for BAML planning
    fn get_tool_description(&self) -> String;

    /// System context for the planning LLM
    fn get_system_context(&self, ctx: &ExecutionContext) -> String;

    /// Execute a tool call
    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError>;

    /// Check if tool should be deferred to external handling
    fn should_defer(&self, tool_name: &str) -> bool;

    /// Emit WorkerTurnReport at completion
    async fn emit_worker_report(
        &self,
        ctx: &ExecutionContext,
        report: WorkerTurnReport,
    ) -> Result<(), HarnessError>;

    /// Emit progress during execution
    async fn emit_progress(
        &self,
        ctx: &ExecutionContext,
        progress: AgentProgress,
    ) -> Result<(), HarnessError>;
}
```

## Worker Types

### ResearcherAdapter

Location: `/Users/wiz/choiros-rs/sandbox/src/actors/researcher/adapter.rs`

Tools available:
- `web_search` - Search the web via multiple providers (tavily, brave, exa)
- `fetch_url` - Fetch and extract content from URLs
- `file_read` - Read local files within sandbox
- `file_write` - Write/create files
- `file_edit` - Edit existing files (find/replace)

Key features:
- Sandboxed file operations (no path traversal, no absolute paths)
- Document update events emitted on file_write/file_edit for live streaming
- Provider selection for web search (auto, tavily, brave, exa)

```rust
impl AgentAdapter for ResearcherAdapter {
    fn get_model_role(&self) -> &str { "researcher" }

    fn get_tool_description(&self) -> String {
        // Describes web_search, fetch_url, file_read, file_write, file_edit
    }

    async fn execute_tool_call(&self, ctx: &ExecutionContext, tool_call: &AgentToolCall)
        -> Result<ToolExecution, HarnessError> {
        match tool_call.tool_name.as_str() {
            "web_search" => { /* ... */ }
            "fetch_url" => { /* ... */ }
            "file_read" => { /* ... */ }
            "file_write" => { /* ... */ }
            "file_edit" => { /* ... */ }
            _ => Err(HarnessError::ToolExecution(format!("Unknown tool: {}", tool_call.tool_name)))
        }
    }
}
```

### TerminalAdapter

Location: `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`

Tools available:
- `bash` - Execute shell commands

Key features:
- Command policy validation (allowed prefixes via env var)
- Curl normalization (adds timeouts, follow redirects)
- Progress emission with command, output excerpt, exit code
- Escalation generation on failure

```rust
impl AgentAdapter for TerminalAdapter {
    fn get_model_role(&self) -> &str { "terminal" }

    fn get_tool_description(&self) -> String {
        // Describes bash tool
    }

    async fn execute_tool_call(&self, ctx: &ExecutionContext, tool_call: &AgentToolCall)
        -> Result<ToolExecution, HarnessError> {
        if tool_call.tool_name != "bash" {
            return Ok(ToolExecution { success: false, error: Some("Unknown tool".to_string()), ... });
        }
        // Extract command and execute via execute_bash()
    }
}
```

## Integration Points

### How Conductor Spawns Harness-Based Workers

Location: `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/runtime/decision.rs`

The Conductor spawns workers concurrently using `tokio::spawn`:

```rust
// In spawn_capability_call()
let conductor_ref = myself.clone();
let run_id_owned = run_id.to_string();
let call_id_owned = call_id.clone();
let objective = item.objective.clone();
let researcher = state.researcher_actor.clone();
let terminal = state.terminal_actor.clone();

tokio::spawn(async move {
    let result = match capability.as_str() {
        "researcher" => match researcher {
            Some(researcher_ref) => call_researcher(
                &researcher_ref, objective, Some(60_000), Some(8), Some(3)
            ).await.map(CapabilityWorkerOutput::Researcher),
            None => Err(ConductorError::WorkerFailed("...".to_string())),
        },
        "terminal" => match terminal {
            Some(terminal_ref) => call_terminal(
                &terminal_ref, objective, None, Some(60_000), Some(6)
            ).await.map(CapabilityWorkerOutput::Terminal),
            None => Err(ConductorError::WorkerFailed("...".to_string())),
        },
        unknown => Err(ConductorError::WorkerFailed(format!("Unsupported capability '{}'", unknown))),
    };

    // Send result back to conductor
    let _ = conductor_ref.send_message(ConductorMsg::CapabilityCallFinished {
        run_id: run_id_owned,
        call_id: call_id_owned,
        agenda_item_id,
        capability,
        result,
    });
});
```

### Worker Call Adapters

Location: `/Users/wiz/choiros-rs/sandbox/src/actors/conductor/workers.rs`

```rust
/// Call the ResearcherActor for an agentic task
pub async fn call_researcher(
    researcher: &ActorRef<ResearcherMsg>,
    objective: String,
    timeout_ms: Option<u64>,
    max_results: Option<u32>,
    max_rounds: Option<u8>,
) -> Result<ResearcherResult, ConductorError> {
    call!(researcher, |reply| ResearcherMsg::RunAgenticTask {
        objective,
        timeout_ms,
        max_results,
        max_rounds,
        model_override: None,
        progress_tx: None,
        reply,
    })
    .map_err(|e| ConductorError::WorkerFailed(format!("Failed to call researcher actor: {e}")))?
    .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
}

/// Call the TerminalActor for either a command or an agentic objective
pub async fn call_terminal(
    terminal: &ActorRef<TerminalMsg>,
    objective: String,
    terminal_command: Option<String>,
    timeout_ms: Option<u64>,
    max_steps: Option<u8>,
) -> Result<TerminalAgentResult, ConductorError> {
    // Start terminal if not running
    match call!(terminal, |reply| TerminalMsg::Start { reply }) { ... }

    if let Some(cmd) = terminal_command {
        // Run single bash command
        call!(terminal, |reply| TerminalMsg::RunBashTool { request, progress_tx: None, reply })
    } else {
        // Run agentic task through harness
        call!(terminal, |reply| TerminalMsg::RunAgenticTask {
            objective, timeout_ms, max_steps, model_override: None, progress_tx: None, reply
        })
    }
}
```

### Event Emission

The harness emits structured events via `EventStoreEmitter`:

```rust
// Progress events
pub fn emit_worker_progress(&self, task_id: &str, phase: &str, message: &str, model_used: Option<&str>);
pub fn emit_worker_started(&self, task_id: &str, objective: &str, model: &str);
pub fn emit_worker_completed(&self, task_id: &str, summary: &str);
pub fn emit_worker_failed(&self, task_id: &str, error: &str);

// Semantic events
pub fn emit_worker_finding(&self, task_id: &str, finding_id: &str, claim: &str, confidence: f64, evidence_refs: &[String]);
pub fn emit_worker_learning(&self, task_id: &str, learning_id: &str, insight: &str, confidence: f64);
```

## Files Changed

1. `/Users/wiz/choiros-rs/baml_src/types.baml` - Simplified `AgentDecision` with `Action` enum
2. `/Users/wiz/choiros-rs/baml_src/agent.baml` - Unified `Decide` function
3. `/Users/wiz/choiros-rs/sandbox/src/actors/agent_harness/mod.rs` - Simplified loop implementation
4. `/Users/wiz/choiros-rs/sandbox/src/actors/researcher/adapter.rs` - ResearcherAdapter implementation
5. `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs` - TerminalAdapter implementation

## Usage Example

```rust
// Create adapter for your worker type
let adapter = TerminalAdapter::new(
    terminal_id,
    working_dir,
    shell,
    Some(event_store),
    Some(progress_tx),
);

// Create harness with adapter
let harness = AgentHarness::with_config(adapter, ModelRegistry::new(), HarnessConfig::default());

// Run the agentic loop
let result = harness.run(
    worker_id,
    user_id,
    objective,
    model_override,
    progress_tx,
).await?;

// Result contains summary, success status, tool executions, worker report
println!("Summary: {}", result.summary);
println!("Success: {}", result.success);
println!("Steps taken: {}", result.steps_taken);
```

## Benefits

1. **Simpler reasoning** - Single loop instead of phase transitions
2. **Easier testing** - Deterministic inputs/outputs per step
3. **Better observability** - Consistent progress events
4. **Flexible workers** - Adapter pattern allows custom tool sets
5. **Type safety** - BAML-generated types for LLM interactions
6. **Concurrent execution** - Workers run in parallel via tokio::spawn
