# pi_agent_rust Unified Agent Harness Refactor

Date: 2026-02-15
Status: Research Proposal
Author: Architecture Review

## Narrative Summary (1-minute read)

**Proposal**: Replace ChoirOS's BAML-based `AgentHarness` with [pi_agent_rust](https://github.com/Dicklesworthstone/pi_agent_rust) as the unified agent runtime for all agent types (Conductor, App Agents, Workers).

**Core idea**: Every agent becomes a full coding agent. Role differentiation happens through system prompts and tool grants, not constrained capability models. Actor-to-actor messaging becomes tool calls.

**What this enables**:
- Native streaming support (currently missing)
- Multi-provider support without BAML
- Battle-tested agent loop from a mature project
- Simpler mental model (all agents are the same, just configured differently)

**What this removes**:
- BAML dependency for LLM calls
- Constrained capability model (ResearcherAdapter, TerminalAdapter)
- WorkerPort trait abstraction

**Tradeoff**: Lose structured output guarantees from BAML. Agents are "full" and may require more tokens per turn.

---

## What Changed

| Before | After |
|--------|-------|
| BAML `Decide` function returns structured `Action` | Native tool calling with streaming |
| `WorkerPort` trait defines role-specific behavior | `Tool` trait + tool grants define capabilities |
| Constrained capabilities per role | Full coding agents constrained by system prompt |
| Manual streaming implementation | Native SSE with partial message updates |
| Single provider at a time | Multi-provider support built-in |

---

## What To Do Next

1. **Prototype**: Create `PiAgentAdapter` wrapping pi_agent_rust for one worker
2. **Tool Bridge**: Implement ractor messaging as pi_agent_rust tools
3. **Model Mapping**: Map ChoirOS model-policy to pi_agent_rust ModelRegistry
4. **Incremental Cutover**: Terminal → Researcher → Conductor

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Architecture Analysis](#current-architecture-analysis)
3. [pi_agent_rust Architecture Analysis](#pi_agent_rust-architecture-analysis)
4. [Comparative Analysis](#comparative-analysis)
5. [Proposed Unified Architecture](#proposed-unified-architecture)
6. [Integration Design](#integration-design)
7. [Migration Strategy](#migration-strategy)
8. [Tradeoffs and Risks](#tradeoffs-and-risks)
9. [Implementation Phases](#implementation-phases)
10. [Code Examples](#code-examples)
11. [Decision Log](#decision-log)

---

## Executive Summary

### Problem Statement

ChoirOS currently uses a custom `AgentHarness` with BAML for LLM integration. This has limitations:

1. **No native streaming** - Responses appear in bulk, not token-by-token
2. **Single provider model** - BAML functions are tied to specific providers
3. **Constrained capability model** - Workers have hard-coded tool sets via `WorkerPort`
4. **Duplication** - We're maintaining agent loop logic that exists in mature projects

### Proposed Solution

Adopt [pi_agent_rust](https://github.com/Dicklesworthstone/pi_agent_rust) as the unified agent harness:

1. **Every agent is a full coding agent** - Same runtime, different configuration
2. **Role differentiation via prompts and tool grants** - Not capability models
3. **Actor messaging as tool calls** - `send_actor_message`, `spawn_worker`, etc.
4. **Native multi-provider support** - Anthropic, OpenAI, Gemini, Bedrock, etc.

### Scope

| In Scope | Out of Scope |
|----------|--------------|
| Agent loop replacement | UI changes |
| Tool system redesign | EventStore changes |
| Model registry mapping | ractor supervision tree changes |
| Actor messaging tools | Session persistence format (keep JSONL) |

---

## Current Architecture Analysis

### Component Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    ApplicationSupervisor                         │
└─────────────────────────┬───────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────────┐
│                     SessionSupervisor                            │
└──────┬──────────────────┬──────────────────┬───────────────────┘
       │                  │                  │
┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
│ Conductor   │    │  Terminal   │    │ Researcher  │
│   Actor     │    │   Actor     │    │   Actor     │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
│AgentHarness │    │AgentHarness │    │AgentHarness │
│     +       │    │     +       │    │     +       │
│   BAML      │    │   BAML      │    │   BAML      │
│  Gateway    │    │  Adapter    │    │  Adapter    │
└─────────────┘    └─────────────┘    └─────────────┘
```

### AgentHarness Design

Location: `sandbox/src/actors/agent_harness/mod.rs`

```rust
pub struct AgentHarness<W: WorkerPort> {
    worker_port: W,
    model_registry: ModelRegistry,
    config: HarnessConfig,
    trace_emitter: LlmTraceEmitter,
}

pub trait WorkerPort: Send + Sync {
    fn get_model_role(&self) -> &str;
    fn get_tool_description(&self) -> String;
    fn get_system_context(&self, ctx: &ExecutionContext) -> String;
    async fn execute_tool_call(
        &self,
        ctx: &ExecutionContext,
        tool_call: &AgentToolCall,
    ) -> Result<ToolExecution, HarnessError>;
    fn should_defer(&self, tool_name: &str) -> bool;
    async fn emit_worker_report(&self, ctx: &ExecutionContext, report: WorkerTurnReport) 
        -> Result<(), HarnessError>;
    async fn emit_progress(&self, ctx: &ExecutionContext, progress: AgentProgress) 
        -> Result<(), HarnessError>;
    fn validate_terminal_decision(&self, ctx: &ExecutionContext, decision: &AgentDecision, 
        tool_executions: &[ToolExecution]) -> Result<(), String>;
}
```

**Loop Model**:
```
DECIDE (BAML) → EXECUTE (WorkerPort) → (loop or return)
```

**Action Types** (from BAML):
```rust
enum Action {
    ToolCall,   // Execute tools, then loop
    Complete,   // Task done, return summary
    Block,      // Cannot proceed, return reason
}
```

### BAML Integration

BAML functions used:

| Function | Purpose | Caller |
|----------|---------|--------|
| `Decide` | Core decision loop | AgentHarness |
| `ConductorBootstrapAgenda` | Bootstrap run | Conductor |
| `ConductorDecide` | Orchestration decision | Conductor |
| `ConductorRefineObjective` | Refine objectives | Conductor |
| `QuickResponse` | Fast responses | Various |

BAML types:
```baml
class AgentDecision {
  action Action        // ToolCall | Complete | Block
  tool_calls AgentToolCall[]
  summary string?
  reason string?
}

class AgentToolCall {
  tool_name string
  tool_args ToolArgs
  reasoning string?
}
```

### Current Capability Model

**ResearcherAdapter** (`sandbox/src/actors/researcher/adapter.rs`):
- Tools: `web_search`, `fetch_url`, `file_read`, `file_write`, `file_edit`, `message_writer`
- Role: External information gathering
- Model: Resolved via model-policy.toml (default: GLM)

**TerminalAdapter** (implied from pattern):
- Tools: `bash`, `file_read`, `file_write`, `file_edit`
- Role: Local execution
- Model: Resolved via model-policy.toml

**Conductor** (`sandbox/src/actors/conductor/`):
- Not a harness worker - uses BAML directly for orchestration
- Delegates to Researcher/Terminal via ractor messages
- Maintains run state, agenda, artifacts

### Actor Messaging Pattern

```rust
// Conductor dispatches to Terminal
ractor::call!(terminal_actor, |reply| TerminalMsg::RunBashTool {
    request: TerminalBashToolRequest {
        cmd,
        timeout_ms,
        model_override: None,
        reasoning: Some("conductor capability dispatch".to_string()),
        run_id,
        call_id,
    },
    progress_tx,
    reply,
})
```

---

## pi_agent_rust Architecture Analysis

### Overview

pi_agent_rust is a high-performance AI coding agent CLI written in Rust with zero unsafe code. It's a port of the TypeScript Pi Agent with ~40k lines of Rust.

**Key Features**:
- Single binary, fast startup (<100ms), low memory (<50MB)
- Native streaming with SSE parser
- 7 built-in tools (read, write, edit, bash, grep, find, ls)
- Multi-provider support (Anthropic, OpenAI, Gemini, Bedrock, Vertex, Cohere, Azure, etc.)
- Session persistence with JSONL + SQLite index
- Extension system with embedded QuickJS runtime
- Rich terminal UI with rich_rust library
- Three execution modes: Interactive, Print, RPC

### Core Agent Loop

Location: `src/agent.rs`

```rust
pub struct Agent {
    provider: Arc<dyn Provider>,
    tools: ToolRegistry,
    config: AgentConfig,
    extensions: Option<ExtensionManager>,
    messages: Vec<Message>,
    steering_fetcher: Option<MessageFetcher>,
    follow_up_fetcher: Option<MessageFetcher>,
    message_queue: MessageQueue,
}
```

**Loop Model**:
```
User Input → Stream Completion → If Tool Calls → Execute Tools → Loop
                                    Else → Return
```

**Events emitted**:
```rust
pub enum AgentEvent {
    AgentStart { session_id: String },
    AgentEnd { session_id: String, messages: Vec<Message>, error: Option<String> },
    TurnStart { session_id: String, turn_index: usize, timestamp: i64 },
    TurnEnd { session_id: String, turn_index: usize, message: Message, tool_results: Vec<Message> },
    MessageStart { message: Message },
    MessageUpdate { message: Message, assistant_message_event: Box<AssistantMessageEvent> },
    MessageEnd { message: Message },
    ToolExecutionStart { tool_call_id: String, tool_name: String, args: Value },
    ToolExecutionUpdate { tool_call_id: String, tool_name: String, args: Value, partial_result: ToolOutput },
    ToolExecutionEnd { tool_call_id: String, tool_name: String, result: ToolOutput, is_error: bool },
    AutoCompactionStart { reason: String },
    AutoCompactionEnd { result: Option<Value>, aborted: bool, will_retry: bool, error_message: Option<String> },
    AutoRetryStart { attempt: u32, max_attempts: u32, delay_ms: u64, error_message: String },
    AutoRetryEnd { success: bool, attempt: u32, final_error: Option<String> },
    ExtensionError { extension_id: Option<String>, event: String, error: String },
}
```

### Tool System

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: Value, update: ToolUpdate) -> Result<ToolOutput, ToolError>;
}
```

Built-in tools:
| Tool | Description |
|------|-------------|
| `read` | Read file contents, supports images |
| `write` | Create or overwrite files |
| `edit` | Surgical string replacement |
| `bash` | Execute shell commands with timeout |
| `grep` | Search file contents with context |
| `find` | Discover files by pattern |
| `ls` | List directory contents |

### Provider System

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn model_id(&self) -> &str;
    fn api(&self) -> &str;
    async fn stream(&self, context: &Context, options: &StreamOptions) 
        -> Result<Box<dyn Stream<Item = Result<StreamEvent>>>, Error>;
}
```

Supported providers:
- Anthropic (Claude)
- OpenAI (Chat Completions + Responses API)
- Google (Gemini)
- Cohere
- Azure OpenAI
- AWS Bedrock
- Google Vertex AI
- GitHub Copilot
- GitLab
- OpenRouter
- Groq, Cerebras, DeepInfra, Fireworks, Together, Perplexity, xAI, etc.

### Session Persistence

Format: JSONL v3 with tree structure

```
~/.pi/agent/sessions/
├── --home-user-project--/
│   ├── 2024-01-15T10-30-00.jsonl
│   └── 2024-01-15T14-22-00.jsonl
└── session-index.sqlite
```

Features:
- Append-only with branching
- SQLite index for fast lookups
- Automatic compaction
- Session recovery

### Extension System

pi_agent_rust runs JS/TS extensions via embedded QuickJS:
- 187/223 extensions pass conformance tests
- Capability-based security (tool/exec/http/session/ui connectors)
- No Node.js/Bun dependency

---

## Comparative Analysis

### Architecture Comparison

| Aspect | ChoirOS (Current) | pi_agent_rust |
|--------|------------------|---------------|
| **Agent Loop** | Custom `AgentHarness` | Mature `Agent` struct |
| **LLM Layer** | BAML (structured prompts) | Direct provider API |
| **Decision Model** | `Action::{ToolCall,Complete,Block}` | Native tool calling |
| **Streaming** | None (bulk responses) | Native SSE with partial updates |
| **Providers** | Via BAML functions | Built-in multi-provider |
| **Tool System** | `WorkerPort` trait | `Tool` trait + `ToolRegistry` |
| **Session** | EventStore (SQLite) | JSONL + SQLite index |
| **Extensions** | None | QuickJS runtime |
| **Binary Size** | Part of sandbox | ~15MB standalone |
| **Startup** | Via cargo/tokio | <100ms |

### Capability Model Comparison

| Aspect | ChoirOS | pi_agent_rust |
|--------|---------|---------------|
| **Constraint Mechanism** | Role-based `WorkerPort` impl | Tool grants + system prompt |
| **Tool Discovery** | `get_tool_description()` string | JSON Schema parameters |
| **Validation** | `validate_terminal_decision()` | Model decides with context |
| **Delegation** | ractor messages | Would need tool bridge |

### Message Flow Comparison

**Current (ChoirOS)**:
```
User → Conductor → BAML Decide → WorkerPort.execute_tool_call()
                                   ↓
                           EventStore.emit()
```

**Proposed (pi_agent_rust)**:
```
User → Agent → Provider.stream() → Tool.execute()
                                      ↓
                              AgentEvent emitted
```

---

## Proposed Unified Architecture

### Design Principles

1. **Every agent is a full coding agent** - Same `Agent` struct, different configuration
2. **Role via prompt, not code** - System prompts define behavior, not trait implementations
3. **Tool grants constrain capability** - Filter `ToolRegistry` per agent role
4. **Actor messaging as tools** - `send_message`, `spawn_worker`, `call_researcher`
5. **Preserve supervision tree** - ractor actors wrap pi_agent_rust `Agent`

### Component Map

```
┌─────────────────────────────────────────────────────────────────┐
│                    ApplicationSupervisor                         │
│                    (unchanged - ractor)                          │
└─────────────────────────┬───────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────────┐
│                     SessionSupervisor                            │
│                    (unchanged - ractor)                          │
└──────┬──────────────────┬──────────────────┬───────────────────┘
       │                  │                  │
┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
│ Conductor   │    │  Terminal   │    │ Researcher  │
│   Actor     │    │   Actor     │    │   Actor     │
│  (ractor)   │    │  (ractor)   │    │  (ractor)   │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
│ PiAgentCore │    │ PiAgentCore │    │ PiAgentCore │
│             │    │             │    │             │
│ - Agent     │    │ - Agent     │    │ - Agent     │
│ - Provider  │    │ - Provider  │    │ - Provider  │
│ - Tools     │    │ - Tools     │    │ - Tools     │
│ - Session   │    │ - Session   │    │ - Session   │
└─────────────┘    └─────────────┘    └─────────────┘
       │                  │                  │
       └──────────────────┴──────────────────┘
                          │
                   ┌──────▼──────┐
                   │  Shared     │
                   │ Actor Tools │
                   │             │
                   │ - send_msg  │
                   │ - spawn     │
                   │ - emit_event│
                   └─────────────┘
```

### Agent Role Configuration

**Conductor**:
```rust
let conductor_tools = ToolRegistry::new()
    .with(send_message_tool())
    .with(call_researcher_tool())
    .with(call_terminal_tool())
    .with(emit_event_tool())
    .with(read_file_tool())  // For reading run state
    .without(bash_tool());   // No direct execution

let conductor_prompt = r#"
You are the Conductor agent. Your role is orchestration.

You DO NOT execute tools directly. You delegate:
- External research → call_researcher
- Local execution → call_terminal
- State changes → send_message to Writer

You maintain run state and make delegation decisions.
"#;
```

**Terminal Worker**:
```rust
let terminal_tools = ToolRegistry::new()
    .with(bash_tool())
    .with(read_file_tool())
    .with(write_file_tool())
    .with(edit_file_tool())
    .with(send_message_tool())  // Report back to Conductor
    .without(web_search_tool()) // No external access
    .without(fetch_url_tool());

let terminal_prompt = r#"
You are the Terminal worker. Your role is local execution.

You execute shell commands and file operations.
Report results via send_message to Conductor.
Do not attempt web searches or external API calls.
"#;
```

**Researcher Worker**:
```rust
let researcher_tools = ToolRegistry::new()
    .with(web_search_tool())
    .with(fetch_url_tool())
    .with(read_file_tool())
    .with(write_file_tool())
    .with(send_message_tool())  // Report findings
    .without(bash_tool());      // No shell access

let researcher_prompt = r#"
You are the Researcher worker. Your role is information gathering.

You search the web and fetch URLs for information.
Report findings via send_message to Conductor.
Do not execute shell commands.
"#;
```

---

## Integration Design

### Actor Messaging Tools

New tools that bridge pi_agent_rust to ractor:

```rust
/// Tool for sending messages to other actors
pub struct SendMessageTool {
    actor_ref: ActorRef<ConductorMsg>,
    run_id: String,
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str { "send_message" }
    
    fn description(&self) -> &str {
        "Send a message to another actor in the system"
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["conductor", "writer", "terminal", "researcher"]
                },
                "message_type": { "type": "string" },
                "payload": { "type": "object" }
            },
            "required": ["target", "message_type", "payload"]
        })
    }
    
    async fn execute(&self, args: Value, _update: ToolUpdate) -> Result<ToolOutput, ToolError> {
        let target: String = args["target"].as_str().unwrap().to_string();
        let message_type: String = args["message_type"].as_str().unwrap().to_string();
        let payload: Value = args["payload"].clone();
        
        match target.as_str() {
            "conductor" => {
                let msg = ConductorMsg::ProcessEvent {
                    run_id: self.run_id.clone(),
                    event_type: message_type,
                    payload,
                    metadata: EventMetadata::default(),
                };
                self.actor_ref.send_message(msg).map_err(|e| ToolError::Execution(e.to_string()))?;
            }
            // ... other targets
        }
        
        Ok(ToolOutput::success(json!({ "sent": true })))
    }
}

/// Tool for delegating work to Researcher
pub struct CallResearcherTool {
    researcher_actor: ActorRef<ResearcherMsg>,
}

#[async_trait]
impl Tool for CallResearcherTool {
    fn name(&self) -> &str { "call_researcher" }
    
    fn description(&self) -> &str {
        "Delegate research work to the Researcher worker"
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "objective": { "type": "string" },
                "timeout_ms": { "type": "integer" }
            },
            "required": ["objective"]
        })
    }
    
    async fn execute(&self, args: Value, _update: ToolUpdate) -> Result<ToolOutput, ToolError> {
        let objective = args["objective"].as_str().unwrap();
        
        let result = ractor::call!(self.researcher_actor, |reply| {
            ResearcherMsg::ExecuteTask {
                request: ResearcherRequest {
                    objective: objective.to_string(),
                    ..Default::default()
                },
                reply,
            }
        }).await.map_err(|e| ToolError::Execution(e.to_string()))?;
        
        Ok(ToolOutput::success(json!({
            "summary": result.summary,
            "findings": result.findings,
        })))
    }
}

/// Tool for emitting events to EventStore
pub struct EmitEventTool {
    event_store: ActorRef<EventStoreMsg>,
    actor_id: String,
    user_id: String,
}

#[async_trait]
impl Tool for EmitEventTool {
    fn name(&self) -> &str { "emit_event" }
    
    fn description(&self) -> &str {
        "Emit an event to the system event store"
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "event_type": { "type": "string" },
                "payload": { "type": "object" }
            },
            "required": ["event_type", "payload"]
        })
    }
    
    async fn execute(&self, args: Value, _update: ToolUpdate) -> Result<ToolOutput, ToolError> {
        let event_type = args["event_type"].as_str().unwrap();
        let payload = args["payload"].clone();
        
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.actor_id.clone(),
            user_id: self.user_id.clone(),
        };
        
        self.event_store
            .send_message(EventStoreMsg::AppendAsync { event })
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        
        Ok(ToolOutput::success(json!({ "emitted": true })))
    }
}
```

### Model Registry Bridge

Map ChoirOS model-policy.toml to pi_agent_rust ModelRegistry:

```rust
pub fn choiros_to_pi_model_registry(policy: &ModelPolicy) -> ModelRegistry {
    let mut registry = ModelRegistry::new();
    
    // Map conductor role
    registry.register(ModelEntry {
        id: policy.conductor_default_model.clone(),
        provider: provider_from_model(&policy.conductor_default_model),
        context_window: 200_000,
        ..Default::default()
    });
    
    // Map researcher role
    registry.register(ModelEntry {
        id: policy.researcher_default_model.clone(),
        provider: provider_from_model(&policy.researcher_default_model),
        context_window: 128_000,
        ..Default::default()
    });
    
    // Map terminal role
    registry.register(ModelEntry {
        id: policy.terminal_default_model.clone(),
        provider: provider_from_model(&policy.terminal_default_model),
        context_window: 128_000,
        ..Default::default()
    });
    
    registry
}

fn provider_from_model(model_id: &str) -> &'static str {
    match model_id {
        m if m.starts_with("claude") => "anthropic",
        m if m.starts_with("gpt") => "openai",
        m if m.starts_with("gemini") => "gemini",
        m if m.contains("bedrock") => "bedrock",
        _ => "anthropic", // default
    }
}
```

### Event Bridge

Bridge pi_agent_rust events to ChoirOS EventStore:

```rust
pub struct EventBridge {
    event_store: ActorRef<EventStoreMsg>,
    actor_id: String,
    user_id: String,
    run_id: String,
}

impl EventBridge {
    pub fn on_agent_event(&self, event: &AgentEvent) {
        match event {
            AgentEvent::TurnStart { turn_index, .. } => {
                self.emit("agent.turn.start", json!({ "turn_index": turn_index }));
            }
            AgentEvent::TurnEnd { turn_index, message, tool_results, .. } => {
                self.emit("agent.turn.end", json!({
                    "turn_index": turn_index,
                    "message": message,
                    "tool_results": tool_results,
                }));
            }
            AgentEvent::ToolExecutionStart { tool_call_id, tool_name, args } => {
                self.emit("agent.tool.start", json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "args": args,
                }));
            }
            AgentEvent::ToolExecutionEnd { tool_call_id, tool_name, result, is_error } => {
                self.emit("agent.tool.end", json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "result": result,
                    "is_error": is_error,
                }));
            }
            _ => {}
        }
    }
    
    fn emit(&self, event_type: &str, payload: Value) {
        let event = AppendEvent {
            event_type: event_type.to_string(),
            payload,
            actor_id: self.actor_id.clone(),
            user_id: self.user_id.clone(),
        };
        let _ = self.event_store.send_message(EventStoreMsg::AppendAsync { event });
    }
}
```

### Session Bridge

Keep ChoirOS EventStore for system events, use pi_agent_rust JSONL for conversation:

```rust
pub struct SessionBridge {
    pi_session: Session,
    event_store: ActorRef<EventStoreMsg>,
    run_id: String,
}

impl SessionBridge {
    /// Save pi_agent_rust session and emit ChoirOS events
    pub async fn save(&mut self, messages: &[Message]) -> Result<()> {
        // Save to pi_agent_rust JSONL
        self.pi_session.save(messages)?;
        
        // Emit ChoirOS event
        let event = AppendEvent {
            event_type: "session.saved".to_string(),
            payload: json!({
                "run_id": self.run_id,
                "message_count": messages.len(),
            }),
            actor_id: "session_bridge".to_string(),
            user_id: "system".to_string(),
        };
        self.event_store.send_message(EventStoreMsg::AppendAsync { event });
        
        Ok(())
    }
}
```

---

## Migration Strategy

### Approach: Incremental Cutover

Do not replace everything at once. Migrate one agent type at a time:

1. **Phase 1**: Terminal worker (simplest, no external dependencies)
2. **Phase 2**: Researcher worker (web search, fetch URL)
3. **Phase 3**: Conductor (orchestration, most complex)

### Phase 1: Terminal Worker

**Goal**: Replace TerminalAdapter with pi_agent_rust Agent

**Steps**:
1. Add pi_agent_rust as workspace dependency
2. Create `PiTerminalActor` wrapping pi_agent_rust `Agent`
3. Implement `BashTool` adapter for ChoirOS command policy
4. Add `EmitEventTool` for EventStore integration
5. Wire up model resolution from ChoirOS model-policy
6. Run parallel tests (old vs new)
7. Switch traffic to new implementation

**Success Criteria**:
- All existing terminal tests pass
- Streaming works (tokens appear incrementally)
- Events are emitted correctly

### Phase 2: Researcher Worker

**Goal**: Replace ResearcherAdapter with pi_agent_rust Agent

**Steps**:
1. Create `PiResearcherActor` wrapping pi_agent_rust `Agent`
2. Implement `WebSearchTool` using existing providers
3. Implement `FetchUrlTool` using existing providers
4. Add `SendMessageTool` for Writer integration
5. Wire up model resolution
6. Run parallel tests
7. Switch traffic

**Success Criteria**:
- All existing researcher tests pass
- Web search and fetch work
- Writer integration works via send_message

### Phase 3: Conductor

**Goal**: Replace Conductor BAML calls with pi_agent_rust Agent

**Steps**:
1. Create `PiConductorCore` using pi_agent_rust `Agent`
2. Implement `CallResearcherTool`
3. Implement `CallTerminalTool`
4. Implement `SendMessageTool` for Writer
5. Implement `ReadRunStateTool` for agenda management
6. Remove BAML dependency from Conductor
7. Run parallel tests
8. Switch traffic

**Success Criteria**:
- All existing conductor tests pass
- Delegation to workers works
- Run state management works
- Writer integration works

### Phase 4: Cleanup

**Steps**:
1. Remove `AgentHarness` and `WorkerPort` trait
2. Remove BAML dependency from Cargo.toml
3. Remove `ResearcherAdapter`, `TerminalAdapter`
4. Update documentation
5. Remove dead code

---

## Tradeoffs and Risks

### Advantages

| Pro | Description |
|-----|-------------|
| **Native Streaming** | Token-by-token output instead of bulk responses |
| **Multi-Provider** | Built-in support for 15+ providers |
| **Battle-Tested** | Mature agent loop from active project |
| **Simpler Model** | One agent type, configured differently |
| **Extension System** | QuickJS runtime for extensions |
| **Session Features** | Branching, compaction, recovery |
| **Rich Output** | Terminal UI with markup |

### Disadvantages

| Con | Description |
|-----|-------------|
| **Lose BAML Guarantees** | No structured output enforcement |
| **Larger Binary** | ~15MB vs current |
| **Different Session Format** | JSONL vs EventStore |
| **QuickJS Overhead** | Extension runtime we may not use |
| **Less Type Safety** | Dynamic tool args vs typed BAML |
| **Migration Cost** | Significant refactor effort |

### Risks

| Risk | Mitigation |
|------|------------|
| **Structured Output Drift** | Model may not follow expected action format | Validate responses, add retries |
| **Token Usage Increase** | Full agents may use more tokens | Monitor usage, optimize prompts |
| **Session Incompatibility** | Existing sessions may not load | Build migration tool, keep both formats |
| **Extension Complexity** | QuickJS runtime adds attack surface | Disable extensions by default |
| **Provider Differences** | Same model may behave differently via direct API vs BAML | Test thoroughly, adjust prompts |

### Open Questions

1. **Should we keep EventStore for all events or use pi_agent_rust sessions?**
   - Recommendation: Keep EventStore for system events, pi_agent_rust for conversation
   
2. **Do we need the extension system?**
   - Recommendation: Disable by default, enable if needed later

3. **How to handle BAML-specific features like `ConductorBootstrapAgenda`?**
   - Recommendation: Convert to system prompts + tool grants

4. **What about the Writer app agent?**
   - Recommendation: Also migrate to pi_agent_rust with `message_writer` tool

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)

- [ ] Add pi_agent_rust as workspace dependency
- [ ] Create `PiAgentCore` wrapper struct
- [ ] Implement model registry bridge
- [ ] Implement event bridge
- [ ] Write integration tests

### Phase 2: Terminal Worker (Week 3-4)

- [ ] Create `PiTerminalActor`
- [ ] Implement `BashTool` with ChoirOS policy
- [ ] Implement `FileTool` adapters
- [ ] Add `EmitEventTool`
- [ ] Parallel testing
- [ ] Cutover

### Phase 3: Researcher Worker (Week 5-6)

- [ ] Create `PiResearcherActor`
- [ ] Implement `WebSearchTool`
- [ ] Implement `FetchUrlTool`
- [ ] Add `SendMessageTool`
- [ ] Parallel testing
- [ ] Cutover

### Phase 4: Conductor (Week 7-8)

- [ ] Create `PiConductorCore`
- [ ] Implement `CallResearcherTool`
- [ ] Implement `CallTerminalTool`
- [ ] Implement delegation tools
- [ ] Parallel testing
- [ ] Cutover

### Phase 5: Cleanup (Week 9)

- [ ] Remove AgentHarness
- [ ] Remove BAML dependency
- [ ] Remove WorkerPort trait
- [ ] Update documentation
- [ ] Performance validation

---

## Code Examples

### Example: PiTerminalActor

```rust
use pi_agent_rust::{Agent, AgentConfig, Provider, ToolRegistry, Tool, ToolOutput, ToolError};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use async_trait::async_trait;
use serde_json::json;

/// Terminal actor using pi_agent_rust
pub struct PiTerminalActor;

pub struct PiTerminalState {
    agent: Agent,
    event_bridge: EventBridge,
}

pub struct PiTerminalArgs {
    pub event_store: ActorRef<EventStoreMsg>,
    pub conductor_actor: ActorRef<ConductorMsg>,
    pub model_registry: ModelRegistry,
    pub run_id: String,
    pub user_id: String,
}

#[async_trait]
impl Actor for PiTerminalActor {
    type Msg = TerminalMsg;
    type State = PiTerminalState;
    type Arguments = PiTerminalArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Build tool registry
        let tools = ToolRegistry::new()
            .with(BashTool::new())
            .with(ReadFileTool::new())
            .with(WriteFileTool::new())
            .with(EditFileTool::new())
            .with(EmitEventTool::new(
                args.event_store.clone(),
                format!("terminal:{}", args.run_id),
                args.user_id.clone(),
            ))
            .with(SendMessageTool::new(
                args.conductor_actor,
                args.run_id.clone(),
            ));

        // Create provider
        let model_id = args.model_registry.resolve_for_role("terminal", Default::default())?;
        let provider = create_provider(&model_id)?;

        // Create agent
        let config = AgentConfig {
            system_prompt: Some(TERMINAL_SYSTEM_PROMPT.to_string()),
            max_tool_iterations: 50,
            ..Default::default()
        };
        
        let agent = Agent::new(Arc::new(provider), tools, config);

        // Create event bridge
        let event_bridge = EventBridge::new(
            args.event_store,
            format!("terminal:{}", args.run_id),
            args.user_id,
            args.run_id,
        );

        Ok(PiTerminalState { agent, event_bridge })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TerminalMsg::RunBashTool { request, reply } => {
                let result = self.execute_bash(state, request).await;
                let _ = reply.send(result);
            }
            TerminalMsg::ExecuteObjective { objective, reply } => {
                let result = self.execute_objective(state, objective).await;
                let _ = reply.send(result);
            }
            _ => {}
        }
        Ok(())
    }
}

impl PiTerminalActor {
    async fn execute_objective(
        &self,
        state: &mut PiTerminalState,
        objective: String,
    ) -> Result<TerminalResult, TerminalError> {
        // Create event callback
        let bridge = state.event_bridge.clone();
        let on_event = move |event: AgentEvent| {
            bridge.on_agent_event(&event);
        };

        // Run agent
        let result = state.agent
            .run(objective, on_event)
            .await
            .map_err(|e| TerminalError::Execution(e.to_string()))?;

        Ok(TerminalResult {
            summary: result.content.iter()
                .filter_map(|c| match c {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            success: !matches!(result.stop_reason, StopReason::Error | StopReason::Aborted),
        })
    }
}

const TERMINAL_SYSTEM_PROMPT: &str = r#"
You are a Terminal worker agent. Your role is local execution.

Capabilities:
- Execute shell commands (bash tool)
- Read files (read tool)
- Write files (write tool)
- Edit files (edit tool)
- Report progress (emit_event tool)
- Communicate with Conductor (send_message tool)

You do NOT have:
- Web search capability
- URL fetch capability
- Direct access to other workers

When given an objective:
1. Plan the execution steps
2. Execute tools as needed
3. Report results via send_message or in your final response

Be concise and efficient. Prefer editing over rewriting entire files.
"#;
```

### Example: CallResearcherTool

```rust
/// Tool for Conductor to delegate to Researcher
pub struct CallResearcherTool {
    researcher_actor: ActorRef<ResearcherMsg>,
    run_id: String,
}

impl CallResearcherTool {
    pub fn new(researcher_actor: ActorRef<ResearcherMsg>, run_id: String) -> Self {
        Self { researcher_actor, run_id }
    }
}

#[async_trait]
impl Tool for CallResearcherTool {
    fn name(&self) -> &str { "call_researcher" }
    
    fn description(&self) -> &str {
        "Delegate research work to the Researcher worker. Use this for web searches, URL fetching, and information gathering."
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "objective": {
                    "type": "string",
                    "description": "The research objective to delegate"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Optional timeout in milliseconds",
                    "default": 60000
                },
                "model_override": {
                    "type": "string",
                    "description": "Optional model to use for this research task"
                }
            },
            "required": ["objective"]
        })
    }
    
    async fn execute(&self, args: Value, update: ToolUpdate) -> Result<ToolOutput, ToolError> {
        let objective = args["objective"].as_str()
            .ok_or_else(|| ToolError::InvalidArgs("objective required".to_string()))?
            .to_string();
        
        let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(60000);
        let model_override = args["model_override"].as_str().map(String::from);
        
        // Send progress update
        update.send(ToolUpdate::partial(json!({
            "status": "delegating",
            "objective": objective,
        })));
        
        // Create call_id for tracking
        let call_id = ulid::Ulid::new().to_string();
        
        // Make actor call
        let result = ractor::call!(self.researcher_actor, |reply| {
            ResearcherMsg::ExecuteTask {
                request: ResearcherRequest {
                    objective,
                    timeout_ms: Some(timeout_ms),
                    model_override,
                    run_id: Some(self.run_id.clone()),
                    call_id: Some(call_id.clone()),
                    ..Default::default()
                },
                reply,
            }
        }).await.map_err(|e| ToolError::Execution(format!("Actor call failed: {}", e)))?;
        
        // Send completion update
        update.send(ToolUpdate::partial(json!({
            "status": "completed",
            "call_id": call_id,
        })));
        
        // Format result
        match result {
            Ok(report) => Ok(ToolOutput::success(json!({
                "status": "completed",
                "call_id": call_id,
                "summary": report.summary,
                "findings": report.findings,
                "learnings": report.learnings,
                "artifacts": report.artifacts,
            }))),
            Err(e) => Ok(ToolOutput::error(json!({
                "status": "failed",
                "call_id": call_id,
                "error": e.to_string(),
            }))),
        }
    }
}
```

### Example: Conductor with pi_agent_rust

```rust
/// Conductor using pi_agent_rust Agent
pub struct PiConductorCore {
    agent: Agent,
    run_id: String,
    event_bridge: EventBridge,
}

impl PiConductorCore {
    pub fn new(
        args: ConductorArgs,
    ) -> Result<Self, ConductorError> {
        // Build tools for conductor
        let tools = ToolRegistry::new()
            // Delegation tools
            .with(CallResearcherTool::new(
                args.researcher_actor.clone(),
                args.run_id.clone(),
            ))
            .with(CallTerminalTool::new(
                args.terminal_actor.clone(),
                args.run_id.clone(),
            ))
            // Communication tools
            .with(SendMessageTool::new(
                args.writer_actor.clone(),
                args.run_id.clone(),
            ))
            .with(EmitEventTool::new(
                args.event_store.clone(),
                format!("conductor:{}", args.run_id),
                args.user_id.clone(),
            ))
            // State tools
            .with(ReadRunStateTool::new(args.run_id.clone()))
            // NO execution tools - conductor does not execute directly
            ;
        
        // Create provider
        let model_id = args.model_registry.resolve_for_role("conductor", Default::default())?;
        let provider = create_provider(&model_id)?;
        
        // Create agent
        let config = AgentConfig {
            system_prompt: Some(CONDUCTOR_SYSTEM_PROMPT.to_string()),
            max_tool_iterations: 50,
            ..Default::default()
        };
        
        let agent = Agent::new(Arc::new(provider), tools, config);
        
        let event_bridge = EventBridge::new(
            args.event_store,
            format!("conductor:{}", args.run_id),
            args.user_id,
            args.run_id.clone(),
        );
        
        Ok(Self {
            agent,
            run_id: args.run_id,
            event_bridge,
        })
    }
    
    pub async fn orchestrate(&mut self, objective: String) -> Result<ConductorResult, ConductorError> {
        let bridge = self.event_bridge.clone();
        let on_event = move |event: AgentEvent| {
            bridge.on_agent_event(&event);
        };
        
        let result = self.agent
            .run(objective, on_event)
            .await
            .map_err(|e| ConductorError::Execution(e.to_string()))?;
        
        Ok(ConductorResult {
            run_id: self.run_id.clone(),
            final_message: result.content.iter()
                .filter_map(|c| match c {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            stop_reason: result.stop_reason,
        })
    }
}

const CONDUCTOR_SYSTEM_PROMPT: &str = r#"
You are the Conductor agent. Your role is orchestration and delegation.

You DO NOT execute tools directly. You delegate work to specialized workers:
- call_researcher: For web search, URL fetching, information gathering
- call_terminal: For shell commands, file operations, local execution

You maintain run state and make delegation decisions.

Workflow:
1. Analyze the objective
2. Break down into subtasks
3. Delegate each subtask to appropriate worker
4. Collect results
5. Synthesize and report

Always use call_researcher for:
- Web searches
- Fetching URLs
- Current events/news
- External documentation

Always use call_terminal for:
- Shell commands
- File operations
- Build/test/run tasks
- Local system operations

Use send_message to communicate with the Writer for living-document updates.

Be decisive. Prefer delegation over deliberation.
"#;
```

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-15 | Research pi_agent_rust integration | Explore alternatives to BAML-based harness |
| 2026-02-15 | Propose incremental cutover | Reduce risk vs big-bang replacement |
| 2026-02-15 | Keep EventStore, add pi sessions | Preserve existing event infrastructure |
| 2026-02-15 | Actor messaging as tools | Preserve ractor supervision tree |

---

## Appendix: pi_agent_rust File Structure

```
pi_agent_rust/
├── src/
│   ├── main.rs           # CLI entry
│   ├── cli.rs            # Clap definitions
│   ├── config.rs         # Settings
│   ├── auth.rs           # API key resolution
│   ├── models.rs         # Model registry
│   ├── provider.rs       # Provider trait
│   ├── providers/        # Provider implementations
│   │   ├── anthropic.rs
│   │   ├── openai.rs
│   │   ├── gemini.rs
│   │   ├── bedrock.rs
│   │   └── ...
│   ├── tools.rs          # Tool registry
│   ├── session.rs        # JSONL persistence
│   ├── agent.rs          # Core agent loop
│   ├── modes.rs          # Print/RPC/Interactive
│   ├── tui.rs            # Terminal UI
│   ├── extensions.rs     # Extension manager
│   └── extensions_js.rs  # QuickJS runtime
├── tests/
├── benches/
└── Cargo.toml
```

---

## References

- [pi_agent_rust GitHub](https://github.com/Dicklesworthstone/pi_agent_rust)
- [pi_agent_rust README](https://raw.githubusercontent.com/Dicklesworthstone/pi_agent_rust/main/README.md)
- [ChoirOS AgentHarness](/Users/wiz/choiros-rs/sandbox/src/actors/agent_harness/mod.rs)
- [ChoirOS ResearcherAdapter](/Users/wiz/choiros-rs/sandbox/src/actors/researcher/adapter.rs)
- [ChoirOS Conductor](/Users/wiz/choiros-rs/sandbox/src/actors/conductor/actor.rs)
- [NARRATIVE_INDEX](/Users/wiz/choiros-rs/docs/architecture/NARRATIVE_INDEX.md)
