# ChoirOS Architecture Analysis: Agents, Workers, and MicroVMs

**Date:** 2026-04-12  
**Scope:** Deep exploration of choiros-rs architecture at ~/choiros-rs  
**Purpose:** Clarify the relationship between Agents, Workers, Sandboxes, and MicroVMs

---

## Executive Summary

ChoirOS implements a **3-tier architecture** that separates concerns across Control Plane (Hypervisor), Runtime Plane (per-user MicroVMs), and Client Plane (frontends). The key insight is:

- **MicroVM** = Per-user isolation boundary (vfkit/cloud-hypervisor VM)
- **Sandbox** = Per-branch runtime instance running inside a MicroVM container
- **Agent** = Actor-based AI worker with decision loop (DECIDE → EXECUTE)
- **Worker** = Primitive execution capability (Terminal, Researcher) - a subset of agents

---

## 1. What is an Agent?

### Definition
An **Agent** is a Rust actor that implements the `WorkerPort` trait and uses the `AgentHarness` for the agentic decision loop.

### Key Components

#### Agent Harness (`sandbox/src/actors/agent_harness/mod.rs`)
The unified harness provides a generic framework for building agentic workers:

```rust
pub struct AgentHarness<W: WorkerPort> {
    worker_port: W,
    model_registry: ModelRegistry,
    config: HarnessConfig,
    trace_emitter: LlmTraceEmitter,
}
```

The harness implements the simplified loop:
```
DECIDE → EXECUTE TOOLS → (loop or return final message)
```

#### WorkerPort Trait
All agents implement `WorkerPort`:

```rust
#[async_trait]
pub trait WorkerPort: Send + Sync {
    fn get_model_role(&self) -> &str;  // e.g., "terminal", "researcher"
    fn get_tool_description(&self) -> String;
    fn get_system_context(&self, ctx: &ExecutionContext) -> String;
    async fn execute_tool_call(&self, ctx: &ExecutionContext, tool_call: &AgentToolCall) 
        -> Result<ToolExecution, HarnessError>;
    // ... other methods
}
```

### Agent Types (Defined in BAML)

| Agent | BAML Definition | Actor Implementation | Role |
|-------|----------------|---------------------|------|
| **Conductor** | `conductor.baml` | `ConductorActor` | Global orchestrator |
| **Writer** | `writer.baml` | `WriterActor` | App agent (living documents) |
| **Terminal** | `agent.baml` | `TerminalActor` | Worker primitive |
| **Researcher** | `researcher.baml` | `ResearcherActor` | Worker primitive |

### Agent Spawning
Agents are spawned via the **ApplicationSupervisor** (`sandbox/src/supervisor/mod.rs`):

```rust
// Supervision tree hierarchy:
ApplicationSupervisor (one_for_one)
└── SessionSupervisor (one_for_one)
    ├── ConductorSupervisor
    ├── DesktopSupervisor
    ├── TerminalSupervisor
    ├── ResearcherSupervisor
    └── WriterSupervisor
```

Agents are spawned lazily via `GetOrCreate*` messages:
- `GetOrCreateConductor { conductor_id, user_id }`
- `GetOrCreateTerminal { terminal_id, user_id, shell, working_dir }`
- `GetOrCreateWriter { writer_id, user_id }`
- `GetOrCreateResearcher { researcher_id, user_id }`

---

## 2. What is a Worker?

### Definition
A **Worker** is a primitive execution capability that executes bounded jobs. Workers are a subset of agents that are reusable across different app agents.

### Current Worker Primitives

| Worker | Purpose | Spawns Agents? |
|--------|---------|----------------|
| **Terminal** | Bash/command execution, file operations | No |
| **Researcher** | Web search, URL fetching | No |
| **Memory** | Context retrieval | No |

### Worker vs App Agent Distinction (ADR-0021)

```
Worker Primitive          App Agent
─────────────────────────────────────────────
Reusable execution        Domain-specific product surface
Bounded jobs              Durable authored state machine
Serves multiple apps      Owns specific domain (documents, browser)
Emits signals/results     Orchestrates within domain
```

**Key Rule:** Workers should NOT be top-level product surfaces. They serve app agents.

### The "Choir in Choir" Concept
The "choir in choir" refers to harness-level orchestration - sequencing entire agent runs as atomic units. This is the missing abstraction between:
- Turn-level orchestration (agent harness loop)
- Capability-level routing (conductor)

Example: Code → Verify → Fix → Re-verify is a cyclic workflow across multiple runs.

---

## 3. What are MicroVMs For?

### Definition
A **MicroVM** is a lightweight virtual machine (vfkit on macOS, cloud-hypervisor on Linux) that provides per-user isolation.

### VM Spawning Trigger

**When:** A request arrives for a user/branch that doesn't have a running VM.

**Trigger chain:**
```
HTTP Request → Hypervisor Middleware → ensure_running() → spawn_instance()
```

**VM Lifecycle** (`hypervisor/src/sandbox/mod.rs`):

```rust
pub enum SandboxStatus {
    Running,           // VM is active
    Stopped,           // VM was stopped
    Hibernated,        // VM snapshot saved to disk
    Starting(watch::Receiver<Result<u16, String>>), // Boot in progress
    Failed,            // VM crashed
}
```

### What Runs Inside a MicroVM?

```
User MicroVM (vfkit/cloud-hypervisor)
├── NixOS container@main (port 12000)
│   └── Sandbox binary (REST API + actors)
│       ├── ConductorActor
│       ├── WriterActor
│       ├── TerminalActor
│       └── ResearcherActor
├── NixOS container@dev (port 12001)
│   └── Sandbox binary
└── NixOS container@feature-X (port 12002)
    └── Sandbox binary
```

**Important:** The ENTIRE sandbox (with all its actors/agents) runs inside the MicroVM container.

### Per-User VM vs Per-Task VM

| Aspect | Per-User VM (Current) | Per-Task VM (Future) |
|--------|----------------------|----------------------|
| Scope | One VM per user | One VM per task/run |
| Lifetime | Long-running, hibernates | Ephemeral, task-scoped |
| Use case | Shared state across runs | Complete isolation |
| Resource cost | Lower (shared) | Higher (many VMs) |

Current implementation uses **per-user VMs** with branch containers inside.

---

## 4. The 3-Tier Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CONTROL PLANE (Hypervisor)                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │   Identity   │  │    Route     │  │   Secrets    │  │     Provider    │ │
│  │   Service    │  │   Registry   │  │    Broker    │  │     Gateway     │ │
│  │  (WebAuthn)  │  │  (pointers)  │  │  (API keys)  │  │ (rate-limited)  │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └─────────────────┘ │
└─────────┼─────────────────┼─────────────────┼──────────────────────────────┘
          │                 │                 │
          │    ┌────────────┴─────────────────┴────────────┐
          │    │         RUNTIME PLANE (Per-User)          │
          │    │  ┌─────────────────────────────────────┐  │
          │    │  │         User MicroVM (vfkit)        │  │
          │    │  │   ┌─────────┐ ┌─────────┐ ┌──────┐  │  │
          │    │  │   │  main   │ │   dev   │ │feat-*│  │  │
          │    │  │   │container│ │container│ │  ... │  │  │
          │    │  │   └────┬────┘ └────┬────┘ └───┬──┘  │  │
          │    │  └────────┼───────────┼──────────┼──────┘  │
          │    └───────────┼───────────┼──────────┼─────────┘
          │                │           │          │
┌─────────┴────────────────┴───────────┴──────────┴──────────────────────────┐
│                              CLIENT PLANE                                     │
│                     (Web/Desktop/Mobile Frontends)                            │
└───────────────────────────────────────────────────────────────────────────────┘
```

### Layer Interactions

1. **Control Plane → Runtime Plane**
   - Hypervisor spawns VMs via `runtime_ctl` script
   - Hypervisor routes requests to VM ports via middleware
   - Hypervisor provides secrets (provider gateway tokens) to VMs

2. **Runtime Plane → Control Plane**
   - Sandboxes call provider gateway for LLM API access
   - Sandboxes emit events that flow back through hypervisor

3. **Client Plane → Control Plane**
   - WebAuthn authentication via hypervisor
   - API calls proxied through hypervisor to appropriate VM

---

## 5. Data Flow: User Input → Result

### Complete Flow

```
1. USER INPUT
   ↓
2. Client (Web/Desktop) sends HTTP/WebSocket request
   ↓
3. HYPERVISOR MIDDLEWARE
   - Authenticates user
   - Resolves route pointer (main/dev/branch)
   - Calls ensure_running(user_id, role) → spawns VM if needed
   - Proxies request to VM port
   ↓
4. SANDBOX API (inside MicroVM container)
   - Receives request at axum HTTP server
   - Routes to appropriate actor
   ↓
5. CONDUCTOR ACTOR (if orchestration needed)
   - Receives task via ConductorMsg::ExecuteTask
   - Determines required capabilities
   - Dispatches to worker actors
   ↓
6. WORKER ACTOR (Terminal/Researcher)
   - Receives capability call
   - Runs AgentHarness loop:
     a. DECIDE: Call BAML Decide function
     b. EXECUTE: Run tool calls (bash, web_search, etc.)
     c. LOOP until finished
   - Returns WorkerTurnReport
   ↓
7. WRITER ACTOR (if document context)
   - Synthesizes worker outputs into living document
   - Creates versions, overlays, proposals
   ↓
8. RESPONSE
   - Flows back through Conductor → API → Hypervisor → Client
   ↓
9. EVENTS
   - Events persisted to EventStore (SQLite)
   - WebSocket push to clients
```

### Actor Message Flow Example

```rust
// 1. Conductor receives task
ConductorMsg::ExecuteTask { request, reply }

// 2. Conductor calls worker via registry workers.rs
call_researcher(researcher_actor, objective, ...)
call_terminal(terminal_actor, objective, ...)

// 3. Worker runs harness loop (agent_harness/mod.rs)
AgentHarness::run(objective, ...)
  → decide() → BAML Decide
  → execute_tool_call() → bash/web_search/etc
  → loop until finished

// 4. Worker returns result
ResearcherResult / TerminalAgentResult

// 5. Conductor processes completion
handle_capability_call_finished(result)
```

---

## 6. VM Spawning Details

### Spawn Trigger

VM spawning is **lazy** - triggered by first request for a user/branch:

```rust
// hypervisor/src/sandbox/mod.rs
pub async fn ensure_running(
    self: &Arc<Self>,
    user_id: &str,
    role: SandboxRole,
) -> anyhow::Result<u16> {
    // 1. Check if already running
    // 2. Check capacity (MAX_CONCURRENT_VMs, memory)
    // 3. Allocate port atomically via PortAllocator
    // 4. Insert Starting status with watch channel (boot coalescing)
    // 5. Spawn boot task
    // 6. Wait for result via watch channel
}
```

### Boot Coalescing (ADR-0022)

Multiple concurrent requests for the same VM join a single boot:

```rust
pub enum SandboxStatus {
    Starting(watch::Receiver<Result<u16, String>>), // All waiters join this
    ...
}
```

### Port Allocation

```rust
pub struct PortAllocator {
    reserved: DashSet<u16>,  // Atomic test-and-set
    range_start: u16,
    range_end: u16,
}

impl PortAllocator {
    pub fn reserve(&self) -> Option<u16> {
        // DashSet::insert returns false if already present (atomic)
        (self.range_start..=self.range_end)
            .find(|&port| self.reserved.insert(port))
    }
}
```

### VM Lifecycle Management

**Start:**
```
ensure_running() → spawn_role_boot_task() → spawn_instance() 
→ runtime_ctl ensure → systemd ensure → VM ready
```

**Stop/Hibernate:**
```
idle watchdog → hibernate() → snapshot VM state → release port
```

---

## 7. Key Architectural Decisions

### ALM vs Linear Harness (alm.baml)

**ALM (Actor Language Model)** is the GENERAL execution mode where the model outputs a program each turn:
- Context sources (memory, documents, previous turns)
- Working memory (reasoning state)
- Next action (ToolCalls, Program/DAG, FanOut, Recurse, Complete, Block)

**Linear Harness** is a degenerate case of ALM with simple ToolCalls.

Current implementation uses linear harness; ALM is implemented but not fully activated.

### BAML vs Native Tool-Use

**Current:** BAML provides structured output parsing for `Decide` function.

**Target (per ADR notes):** Replace with native tool-use protocol:
- Anthropic Messages API tool_use blocks
- OpenAI function calling
- Removes extra latency from BAML parsing layer

### Machine Classes (ADR-0014 Phase 6)

Different VM configurations per user:

```rust
pub struct MachineClass {
    pub hypervisor: String,      // "vfkit" or "cloud-hypervisor"
    pub transport: String,       // "virtiofs" or "virtio-blk"
    pub vcpu: u32,
    pub memory_mb: u32,
}
```

---

## 8. File Reference Guide

### Critical Files by Topic

| Topic | File | Purpose |
|-------|------|---------|
| **Agent Harness** | `sandbox/src/actors/agent_harness/mod.rs` | Unified agent loop framework |
| **Agent Harness** | `sandbox/src/actors/agent_harness/alm.rs` | ALM (Actor Language Model) implementation |
| **BAML Agents** | `baml_src/agent.baml` | Base agent function definitions |
| **BAML Agents** | `baml_src/alm.baml` | ALM-specific BAML contracts |
| **BAML Agents** | `baml_src/conductor.baml` | Conductor agent definition |
| **Supervision** | `sandbox/src/supervisor/mod.rs` | ApplicationSupervisor - root of actor tree |
| **Conductor** | `sandbox/src/actors/conductor/actor.rs` | Conductor actor implementation |
| **Conductor** | `sandbox/src/actors/conductor/workers.rs` | Worker call adapters |
| **VM Lifecycle** | `hypervisor/src/sandbox/mod.rs` | SandboxRegistry, VM spawning, idle watchdog |
| **VM Systemd** | `hypervisor/src/sandbox/systemd.rs` | Systemd lifecycle management |
| **Architecture** | `docs/adr-0007-3-tier-control-runtime-client-architecture.md` | 3-tier architecture ADR |
| **Architecture** | `docs/adr-0021-writer-app-agent-and-collaborative-living-documents.md` | App agent vs worker distinction |
| **Architecture** | `docs/adr-0022-hypervisor-concurrency-and-capacity.md` | VM concurrency, boot coalescing |
| **Architecture** | `docs/note-2026-03-11-agent-architecture-session-notes.md` | Agent design session notes |

---

## 9. Clarifications on Common Confusions

### Agent vs Worker

**Agent** is the general term for any AI-driven actor using the harness.  
**Worker** is a specific type of agent - primitive execution capabilities.

All workers are agents, not all agents are workers. Writer is an App Agent (not a worker). Terminal is a Worker.

### Sandbox vs MicroVM

**MicroVM** = The virtual machine (vfkit/cloud-hypervisor)  
**Sandbox** = The runtime process (axum + actors) running inside a container inside the MicroVM

Multiple sandboxes (one per branch) can run inside a single MicroVM.

### Conductor vs App Agent

**Conductor** = Global orchestrator, routes tasks, manages run lifecycle  
**App Agent** = Domain-specific orchestrator (Writer for documents, future Browser for web)

Hierarchy: `Conductor → App Agent → Worker`

---

## 10. Summary

| Concept | Definition | Runs In | Spawned By |
|---------|-----------|---------|------------|
| **Agent** | Actor with `WorkerPort` + `AgentHarness` | Sandbox | SessionSupervisor |
| **Worker** | Primitive agent (Terminal, Researcher) | Sandbox | Conductor delegation |
| **App Agent** | Domain orchestrator (Writer) | Sandbox | User request or Conductor |
| **Sandbox** | axum + actors runtime | VM container | Hypervisor on first request |
| **MicroVM** | Per-user virtual machine | Host (Hypervisor) | `ensure_running()` lazy spawn |
| **Container** | Per-branch NixOS container | MicroVM | `runtime_ctl ensure` |

**The golden rule:** Users get MicroVMs. MicroVMs contain branch containers. Containers run sandboxes. Sandboxes host agents. Agents use the harness to DECIDE and EXECUTE.
