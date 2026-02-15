# pi_agent_rust Integration Analysis

## Narrative Summary (1-minute read)

This document analyzes refactoring ChoirOS to use [pi_agent_rust](https://github.com/Dicklesworthstone/pi_agent_rust) as the unified agent harness, replacing BAML-constrained agents with full coding agents that communicate via ractor messaging exposed as "tools."

**Decision**: Proceed with pi_agent_rust as the unified harness. Provider flexibility (Z.ai, Kimi, etc.) is achieved through Anthropic/OpenAI-compatible endpoints, not through custom provider implementations.

**What To Do Next**: Implement the hybrid migration path (Strategy 2) — keep ractor for actor supervision, use pi_agent_rust for worker execution via RPC mode.

---

## Current ChoirOS Architecture

### Overview

ChoirOS uses a **supervision-tree-first** architecture with ractor actors:

```
ApplicationSupervisor
└── SessionSupervisor
    ├── ConductorSupervisor
    │   └── ConductorActor (orchestration)
    ├── TerminalSupervisor
    │   └── TerminalActor (bash execution)
    ├── DesktopSupervisor
    └── WriterSupervisor
        └── WriterApp (document evolution)
```

### Current Agent Harness (BAML-Based)

**Location**: `sandbox/src/actors/agent_harness/`

The current harness follows a **DECIDE → EXECUTE → LOOP** pattern:

1. **BAML Decision**: Call `B.Decide()` with tool descriptions
2. **Tool Execution**: Execute returned `AgentToolCall`s
3. **Loop or Complete**: Continue until `Action::Complete` or `Action::Block`

```rust
// Simplified harness loop
loop {
    let decision = baml_decide(&context, &tools).await?;
    match decision.action {
        Action::ToolCall => execute_tools(&decision.tools).await?,
        Action::Complete => break,
        Action::Block => escalate(),
    }
}
```

**Constraints**:
- Static tool schema defined in BAML
- Model policy via TOML config (not dynamic)
- No native streaming in BAML (progress via separate mpsc channels)
- Complex union types for tool arguments (20+ optional fields)

### BAML Integration Points

| Component | BAML Usage |
|-----------|-----------|
| `agent.baml` | `Decide` function for agent tool selection |
| `conductor.baml` | `ConductorDecide`, `ConductorBootstrapAgenda` |
| `types.baml` | `AgentDecision`, `AgentToolCall`, `Action` enum |

---

## pi_agent_rust Architecture

### Overview

pi_agent_rust is a **full coding agent CLI** with:
- 7 built-in tools: `read`, `write`, `edit`, `bash`, `grep`, `find`, `ls`
- Session management with JSONL + branching
- QuickJS extension system (187/223 extensions pass conformance)
- RPC mode for headless IDE integration

### Execution Model

| Mode | Use Case |
|------|----------|
| `pi` (interactive) | Full TUI with streaming |
| `pi -p "..."` | Single response, scriptable |
| `pi --mode rpc` | Headless JSON protocol |

### Tool System

Tools are **native Rust functions** with automatic truncation and cleanup:

```rust
// Conceptual tool definition
fn bash(command: &str, timeout_ms: u32) -> Result<Output> {
    // Process tree cleanup guarantee
    // Auto-truncate at 2000 lines / 50KB
}
```

### Provider Flexibility

pi_agent_rust officially supports Anthropic, but **provider diversity is achieved through compatibility endpoints**, not custom implementations:

| Provider | How to Use | Base URL Override |
|----------|-----------|-------------------|
| **Anthropic** | Native | `https://api.anthropic.com` |
| **Z.ai** | Via Anthropic compatibility | `https://api.z.ai/anthropic/v1` |
| **Kimi (Moonshot)** | Via Anthropic compatibility | `https://api.moonshot.cn/anthropic/v1` |
| **OpenAI** | Native OpenAI support | `https://api.openai.com` |
| **Groq** | Via OpenAI compatibility | `https://api.groq.com/openai/v1` |
| **DeepSeek** | Via OpenAI compatibility | `https://api.deepseek.com` |

This is actually **superior to multiple provider implementations**:
- One Anthropic-compatible client covers most modern providers
- One OpenAI-compatible client covers the rest
- Configuration is just a base URL + API key, not code changes

---

## Integration Strategies

### Strategy 1: Full Migration (pi_agent_rust replaces BAML harness)

**Approach**: Replace the agent_harness with pi_agent_rust as the execution engine.

**Changes Required**:

1. **Replace BAML `Decide` loop with pi_agent orchestration**
   - Remove `baml_src/agent.baml`, `conductor.baml`
   - Replace with pi_agent's native tool-coding loop

2. **Map ractor messages to pi_agent "tools"**
   ```rust
   // New: Ractor message as a tool
   #[pi_tool]
   async fn actor_call(
       actor_id: ActorId,
       message: JsonValue,
   ) -> Result<JsonValue> {
       let actor = registry.get(&actor_id)?;
       ractor::call!(actor, |reply| Message::from_json(message, reply)).await
   }
   ```

3. **Conductor becomes a pi_agent with actor-call tools**
   - Instead of BAML `ConductorBootstrapAgenda`, conductor uses natural language + tool calls
   - Workers spawned via `actor_call` tool to supervisor

**Pros**:
- Simpler mental model: every agent is a coding agent
- Richer tool ecosystem (7 built-in + extensions)
- Native streaming support

**Cons**:
- Major rewrite of conductor, agent_harness
- Loses BAML's structured output guarantees
- Testing complexity (non-deterministic LLM decisions)

**Effort**: High (4-6 hours)

---

### Strategy 2: Hybrid (pi_agent_rust for workers, keep ractor/BAML for conductor)

**Approach**: Keep the conductor's BAML-orchestrated agenda management, but workers become pi_agent_rust instances.

**Architecture**:

```
Conductor (BAML-orchestrated)
├── WorkerPort for Terminal → spawns pi_agent --mode rpc
├── WorkerPort for Researcher → spawns pi_agent with research skills
└── WorkerPort for Writer → spawns pi_agent with writer skills
```

**Changes Required**:

1. **New `PiAgentWorkerPort`** implementing `WorkerPort`
   ```rust
   pub struct PiAgentWorkerPort {
       rpc_handle: Child,
       stdin: ChildStdin,
       stdout: ChildStdout,
   }

   #[async_trait]
   impl WorkerPort for PiAgentWorkerPort {
       async fn execute_tool_call(&self, ctx: &ExecutionContext, call: &AgentToolCall) -> Result<...> {
           // Serialize to pi_agent RPC protocol
           let rpc_cmd = json!({"cmd": "prompt", "content": ...});
           self.send_rpc(rpc_cmd).await
       }
   }
   ```

2. **pi_agent skills for ChoirOS-specific tools**
   - Create `~/.pi/agent/skills/choiros/` with actor-messaging tools
   - Skill defines: `actor_call`, `event_publish`, `capability_register`

**Pros**:
- Incremental adoption possible
- Keep conductor's deterministic orchestration
- Workers get full coding capabilities

**Cons**:
- Two agent paradigms to maintain
- RPC overhead per worker spawn
- Skill system dependency

**Effort**: Medium (2-3 hours)

---

### Strategy 3: Incremental (adopt pi_agent patterns in ChoirOS)

**Approach**: Don't use pi_agent_rust directly, but refactor ChoirOS to match its architecture.

**Changes**:

1. **Native tool definitions** (like pi_agent's 7 tools)
   - Replace BAML-generated tool schema with Rust trait-based tools
   - `trait Tool: Send + Sync { fn execute(&self, args: Value) -> Result<Value>; }`

2. **Streaming-first design**
   - Integrate streaming into the decision loop
   - Each tool execution streams partial results

3. **Session branching** (from pi_agent's JSONL format)
   - Tree-structured conversation history
   - Allow exploration branches

4. **Skill system** (inspired by pi_agent's SKILL.md)
   - Drop `SKILL.md` files in `skills/`
   - Skills define prompts + tools for specific domains

**Pros**:
- No external dependency
- Tailored to ChoirOS needs
- Keeps ractor integration native

**Cons**:
- Reinventing parts of pi_agent_rust
- No QuickJS extension ecosystem

**Effort**: Medium-High (3-4 hours)

---

## Key Design Questions

### 1. How would agent-to-agent messaging work?

**Current**: Direct ractor `ActorRef::send_message()` calls

**With pi_agent**: Expose as a tool:

```rust
// Tool available to all pi_agent instances
#[tool]
async fn send_to_actor(
    target_actor_id: String,
    message_type: String,
    payload: JsonValue,
) -> Result<JsonValue> {
    let target = ACTOR_REGISTRY.get(&target_actor_id)?;
    let response = ractor::call!(target, |reply| {
        Message::from_parts(&message_type, payload, reply)
    }).await?;
    Ok(json!(response))
}
```

**Consideration**: pi_agent_rust expects tools to be self-contained. Ractor's actor registry is global state—how do we inject it?

### 2. What about the supervision tree?

**Current**: ractor supervisors monitor actor lifecycle

**With pi_agent**: pi_agent processes are OS processes. We'd need:
- A supervisor that monitors child pi_agent processes
- Health checks via RPC `get-state` command
- Automatic restart on failure

### 3. How do we maintain structured output?

**Current**: BAML ensures type-safe outputs via generated types

**With pi_agent**: pi_agent returns unstructured text. Options:
1. **Prompt engineering**: Request JSON output, parse defensively
2. **Modeled output**: Add a `structured_output` tool that validates against schema
3. **Hybrid**: Keep BAML for structured decisions, pi_agent for execution

### 4. What about event streaming?

**Current**: mpsc channels from workers → RunWriter → WebSocket

**With pi_agent**: pi_agent has native streaming. Integration options:

```rust
// pi_agent streams events as line-delimited JSON
while let Some(line) = pi_agent_stdout.next_line().await? {
    let event: PiAgentEvent = serde_json::from_str(&line)?;
    match event {
        PiAgentEvent::ToolStart { tool, args } => {
            event_bus.publish(Event::ToolStarted { ... }).await?;
        }
        PiAgentEvent::ToolOutput { content } => {
            event_bus.publish(Event::ToolProgress { ... }).await?;
        }
        PiAgentEvent::Complete { result } => break,
    }
}
```

---

## Recommended Approach: Strategy 2 (Hybrid)

**Decision**: Use pi_agent_rust for workers, keep BAML-orchestrated Conductor (for now).

**Rationale**:
- Conductor's agenda-based orchestration is tested and working
- Workers benefit most from full coding capabilities (file editing, bash, search)
- Provider flexibility achieved through Anthropic/OpenAI compatibility endpoints
- Incremental migration reduces risk

### Phase 1: pi_agent Worker Prototype (1 hour)

1. Create `PiAgentWorkerPort` wrapping `pi_agent --mode rpc`
2. Create ChoirOS skill with actor-messaging tools
3. Port TerminalActor to pi_agent, test task execution

### Phase 2: Worker Migration (1-2 hours)

1. Port ResearcherWorker to pi_agent
2. Port WriterWorker to pi_agent
3. Define skill boundaries: terminal skill, research skill, writer skill

### Phase 3: Conductor Hardening (1 hour)

1. Enhance Conductor's worker dispatch to use pi_agent
2. Maintain BAML orchestration for agenda management
3. Add provider configuration (base URL override as needed)

### Phase 4: Optional Conductor Migration (future)

If BAML orchestration becomes a bottleneck:
- Migrate Conductor to pi_agent with `actor_call` tools
- Remove BAML dependency entirely
- This is optional—only if model-led orchestration proves superior

---

## Implementation Sketch: PiAgentWorkerPort

```rust
// sandbox/src/actors/agent_harness/pi_agent_port.rs

use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde_json::{json, Value};

pub struct PiAgentWorkerPort {
    skill_path: PathBuf,
    child: Child,
    stdin: tokio::io::ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<tokio::io::ChildStdout>>,
    request_counter: AtomicU64,
}

impl PiAgentWorkerPort {
    pub async fn spawn(skill_name: &str, context: &ExecutionContext) -> Result<Self> {
        let mut child = Command::new("pi")
            .arg("--mode")
            .arg("rpc")
            .arg("--skill")
            .arg(skill_name)
            .env("CHOIROS_SESSION_ID", &context.session_id)
            .env("CHOIROS_THREAD_ID", &context.thread_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stdout_lines = BufReader::new(stdout).lines();

        Ok(Self {
            skill_path: skills_dir().join(skill_name),
            child,
            stdin,
            stdout_lines,
            request_counter: AtomicU64::new(0),
        })
    }

    pub async fn execute(&mut self, objective: &str) -> Result<WorkerTurnReport> {
        let req_id = self.request_counter.fetch_add(1, Ordering::SeqCst);

        // Send prompt command via RPC
        let cmd = json!({
            "cmd": "prompt",
            "id": req_id,
            "content": objective
        });

        self.send(&cmd).await?;

        // Stream events until completion
        let mut findings = Vec::new();
        let mut artifacts = Vec::new();

        while let Some(line) = self.stdout_lines.next_line().await? {
            let event: PiAgentEvent = serde_json::from_str(&line)?;

            match event {
                PiAgentEvent::ToolCall { tool, args } => {
                    // Emit progress event
                    self.emit_progress(ToolStarted { tool: tool.clone() }).await?;

                    // If it's an actor_call, translate to ractor message
                    if tool == "actor_call" {
                        let result = self.handle_actor_call(args).await?;
                        self.send_tool_result(&tool, result).await?;
                    }
                }
                PiAgentEvent::ToolOutput { tool, content } => {
                    self.emit_progress(ToolProgress { tool, content }).await?;
                }
                PiAgentEvent::Complete { result } => {
                    findings.push(result);
                    break;
                }
                PiAgentEvent::Error { message } => {
                    return Err(HarnessError::AgentFailed(message));
                }
            }
        }

        Ok(WorkerTurnReport {
            findings,
            artifacts,
            ..Default::default()
        })
    }

    async fn handle_actor_call(&self, args: Value) -> Result<Value> {
        let target_id = args["actor_id"].as_str().unwrap();
        let message = args["message"].clone();

        let target = ACTOR_REGISTRY.get(target_id)?;
        let response = ractor::call!(target, |reply| {
            Message::from_json(message, reply)
        }).await?;

        Ok(json!(response))
    }

    async fn send(&mut self, value: &Value) -> Result<()> {
        let line = value.to_string() + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl WorkerPort for PiAgentWorkerPort {
    fn get_model_role(&self) -> &str {
        "pi_agent"
    }

    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError> {
        // Delegate to pi_agent's tool execution
        self.forward_to_pi_agent(call).await
    }
}
```

---

## ChoirOS Skill Example

```markdown
<!-- skills/pi/choiros-base/SKILL.md -->
# ChoirOS Base Skill

## Description
Base capabilities for ChoirOS agent workers.

## Tools

### actor_call
Send a message to a ractor actor.

**Args**:
- `actor_id` (string): Target actor identifier
- `message_type` (string): Message variant name
- `payload` (object): Message payload

**Returns**: Actor response as JSON

### event_publish
Publish an event to the ChoirOS EventBus.

**Args**:
- `channel` (string): "control" or "telemetry"
- `event_type` (string): Event variant
- `payload` (object): Event data

### capability_register
Register a capability with the Conductor.

**Args**:
- `capability` (string): Capability name
- `handler_actor` (string): Actor that handles this capability

## System Prompt

You are a ChoirOS agent worker. Your role is to execute tasks using the
available tools. When you need to interact with other actors, use the
`actor_call` tool. Stream progress via `event_publish`.

Always prefer tool use over asking the user. If stuck, escalate via
`capability_register` with the "escalate" capability.
```

---

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| pi_agent_rust maintenance | Medium | Fork or vendor if needed |
| Performance (process per worker) | Medium | Process pool, not spawn per task |
| Structured output loss | High | Add schema validation layer |
| Debuggability (LLM decisions) | Medium | Comprehensive tracing |
| Migration time | Low | Agentic coding (4 hours) |

---

## Decision Matrix

| Criteria | Current BAML | Strategy 1 (Full) | **Strategy 2 (Hybrid)** | Strategy 3 (Incremental) |
|----------|--------------|-------------------|---------------------|--------------------------|
| Development speed | ✓✓ | ✓ | **✓✓** | ✓✓ |
| Type safety | ✓✓ | ✓ | **✓✓** | ✓✓ |
| Flexibility | ✓ | ✓✓ | **✓✓** | ✓✓ |
| Maintainability | ✓✓ | ✓ | **✓✓** | ✓✓ |
| Migration risk | — | ✓ | **✓✓** | ✓✓ |
| Ecosystem (extensions) | ✗ | ✓✓ | **✓✓** | ✗ |

**Selected**: Strategy 2 (Hybrid) — pi_agent_rust for workers, BAML Conductor for orchestration.

Rationale: Lowest risk, highest immediate value. Workers get full coding capabilities (file editing, bash, search) while preserving tested orchestration logic.

---

## Appendix: pi_agent_rust RPC Protocol

```typescript
// Commands (to pi_agent)
interface PromptCommand {
  cmd: "prompt";
  id: number;
  content: string;
}

interface SteerCommand {
  cmd: "steer";
  content: string;  // Interrupt current generation
}

interface AbortCommand {
  cmd: "abort";
}

// Events (from pi_agent)
interface ToolCallEvent {
  type: "tool_call";
  tool: string;
  args: Record<string, any>;
}

interface ToolOutputEvent {
  type: "tool_output";
  tool: string;
  content: string;
}

interface CompleteEvent {
  type: "complete";
  result: string;
}

interface ErrorEvent {
  type: "error";
  message: string;
}
```
