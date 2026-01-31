# Agent Operating System: Architecture Space for ChoirOS

**Date**: 2026-01-29
**Status**: Research & Analysis
**Core Question**: How does the computer want to be programmed for an AI-native operating system?

---

## Executive Summary

You're building an **automatic computer** - a fundamental primitive of the AI-driven internet. Your intuition about state machines (AHDB modes) was correct; **Burr** is the formal engineering of that pattern. Combined with **Ray** for concurrency, this defines the architecture space for AI-native operating systems.

This document maps:
1. How Burr formalizes your mode system
2. How Ray provides the actor model
3. The broader Agent-OS vision from recent research
4. How these pieces unify into "how the computer wants to be programmed"

---

## Part 1: The Intuition Was Right

### What You Freestyled

Your `mode_engine.py` and AHDB mode system:

```python
MODE_CALM = "CALM"
MODE_CURIOUS = "CURIOUS"
MODE_SKEPTICAL = "SKEPTICAL"
MODE_PARANOID = "PARANOID"
MODE_BOLD = "BOLD"
MODE_CONTRITE = "CONTRITE"

def select_initial_mode(inputs: ModeInputs) -> str:
    if inputs.crash_detected:
        return MODE_CONTRITE
    if not inputs.has_demo:
        return MODE_CURIOUS
    # ...

def transition_mode(current: str, inputs: ModeInputs) -> str:
    if inputs.crash_detected:
        return MODE_CONTRITE
    if current == MODE_CALM and inputs.ambiguity_blocking:
        return MODE_CURIOUS
    # ...
```

This is a **state machine**. You intuited this because it's the natural pattern for deterministic AI systems.

### What Burr Formalizes

**Apache Burr** (incubating) is a "lightweight Python graph orchestration framework for modeling and executing application logic as a series of actions and state modifications."

From the Apache proposal:

> **Burr** is a lightweight in-process python framework that standardizes the expression and execution of state machines as **action-driven graphs**, while making graph execution easily observable. It is particularly suited for AI agent workflows, simulations, and other dynamic systems.

**Key Insight**: Burr is the production-ready version of what you built.

#### Burr Core Concepts

```python
from burr.core import State, Action, Application, default
from burr.core.action import action

# 1. Actions - Your "Modes"
@action(reads=["prompt"], writes=["response"])
def process_prompt(state: State, prompt: str) -> State:
    # Agent logic here
    return state.update(response=result)

@action(reads=["response"], writes=["verified"])
def verify_result(state: State) -> State:
    # Verification logic
    return state.update(verified=True)

@action(reads=["verified"], writes=["final_answer"])
def skeptical_review(state: State) -> State:
    # SKEPTICAL mode logic
    return state.update(final_answer=state["response"])

# 2. State Machine Declaration
app = (
    ApplicationBuilder()
    .with_actions(
        process_prompt,
        verify_result,
        skeptical_review,
    )
    .with_transitions(
        ("process_prompt", "verify_result", lambda state: state["response"] is not None),
        ("verify_result", "skeptical_review", lambda state: not state["verified"]),
        ("verify_result", "process_prompt", lambda state: state["verified"]),
        ("skeptical_review", "process_prompt", lambda state: True),  # Loop back
    )
    .with_state("prompt", "response", "verified", "final_answer")
    .build()
)

# 3. Execute
result = app.run(
    halt_after=["skeptical_review"],
    inputs={"prompt": "user input here"}
)
```

#### Mapping: Your Modes → Burr Actions

| Your Concept | Burr Concept | Example |
|--------------|--------------|---------|
| Mode (`CALM`) | Action | `@action(reads=[...], writes=[...])` |
| Mode Transition | Transition | `("action_a", "action_b", condition)` |
| AHDB State | State | `State` object (immutable) |
| Mode Inputs | State Reads | `reads=["prompt", "context"]` |
| Mode Outputs | State Writes | `writes=["response", "receipts"]` |
| Mode Engine | Application | `ApplicationBuilder().with_transitions()` |
| Machine Loop | `app.run()` | Execution engine |

#### Why Burr Matters

**1. Deterministic State Management**
- State is **immutable** (no hidden mutations)
- Every action declares reads/writes explicitly
- State snapshots at every step (debuggable, replayable)

**2. Observability Out of the Box**
```python
# Burr UI shows:
# - State at each step
# - Transition graph visualization
# - Execution trace
# - Replay capability
```

**3. Checkpoint/Resume**
```python
# Pause execution
state = app.run(halt_after=["verify_result"])

# Resume later (same state)
result = app.run(halt_after=["skeptical_review"], state=state)
```

**4. Parallel Sub-Applications**
```python
# Map-reduce pattern for parallel agents
@action(reads=["items"], writes=["results"])
def parallel_research(state: State) -> State:
    # Spawn 10 researcher sub-applications
    results = state["items"].map_parallel(
        lambda item: run_sub_app(item)
    )
    return state.update(results=results)
```

---

## Part 2: Ray Provides the Actor Model

You identified NATS as a pain point. **Ray** gives you the proper actor model for multi-agent concurrency.

### Ray + Burr Integration

From the Burr/Ray blog post:

> **Parallel Fault-Tolerant Agents with Burr/Ray**
>
> We demonstrate executing parallel sub-agents/workflows on Ray and persisting the state to enable easy restart from failure.

```python
from burr.integrations import ray as burr_ray

# Parallel executor using Ray
app = (
    ApplicationBuilder()
    .with_actions(...)
    .with_transitions(...)
    .with_parallel_executor(
        executor_factory=burr_ray.RayExecutor
    )
    .build()
)

# Now all parallel sub-applications run on Ray
# Each sub-application:
# - Gets its own Ray actor
# - Runs in its own process
# - Fault-tolerant with checkpointing
# - Results aggregated back to parent
```

### Mapping: NATS → Ray

| NATS Concept | Ray Concept | Benefit |
|--------------|-------------|---------|
| Message broker | Actor handles | No external dependency |
| Pub/sub | Actor methods | Native Python |
| Consumer groups | Actor pools | Built-in scheduling |
| JetStream persistence | Checkpointing | Simpler mental model |
| Ack/retry | `max_restarts`, `max_task_retries` | Built-in fault tolerance |

### Ray Actors for ChoirOS

```python
import ray

@ray.remote
class ConversationAgent:
    """One actor per UI window/conversation"""
    def __init__(self, window_id: str, user_id: str):
        # Burr application for this conversation
        self.app = build_burr_app()
        self.window_id = window_id
        self.user_id = user_id

    async def process(self, prompt: str):
        # Execute state machine
        result = self.app.run(
            halt_after=["complete"],
            inputs={"prompt": prompt}
        )
        return result

# Each window gets its own agent
conversations = [
    ConversationAgent.remote(f"window_{i}", user_id)
    for i in range(10)
]

# All run concurrently, state isolated
results = await asyncio.gather(*[
    c.process.remote("do this task")
    for c in conversations
])
```

---

## Part 3: The Agent-OS Vision

Recent research (**Agent Operating Systems (Agent-OS): A Blueprint Architecture for Real-Time, Secure, and Scalable AI Agents**, 2025) formalizes the architecture space you're operating in.

### Key Insight: The Billion Agent Problem

> **By 2030, billions of AI agents will require coordination—without an OS-level abstraction, we risk computational chaos at unprecedented scale.**

You're not building an app. You're building an **operating system for agents**.

### The Five-Layer Agent-OS Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  USER & APPLICATION LAYER                   │
│  - Natural language shell                                    │
│  - Agent catalog (role agents with contracts)               │
│  - SDK/REST interfaces                                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              WORKFLOW & ORCHESTRATION LAYER                  │
│  - DAG/state machine workflows                              │
│  - Agent delegation (A2A bus)                               │
│  - Human-in-the-loop gates                                  │
│  - Multi-agent coordination                                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  AGENT RUNTIME LAYER                        │
│  - Agent lifecycle (spawn/pause/resume/terminate)           │
│  - Conversation state management                           │
│  - Checkpoint/replay                                        │
│  - Agent-as-tool registration                               │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      KERNEL LAYER                           │
│  - Admission control (validate contracts)                   │
│  - Class-aware scheduling (HRT/SRT/DT)                      │
│  - Policy engine (RBAC, capabilities)                       │
│  - Zero-trust enforcement                                   │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    SERVICES LAYER                           │
│  - Memory/Knowledge (RAG, vector stores)                    │
│  - Tools (MCP-style registry)                               │
│  - Model gateway (local/edge/cloud)                         │
│  - A2A bus (inter-agent messaging)                          │
│  - Observability (OpenTelemetry)                            │
└─────────────────────────────────────────────────────────────┘
```

### Mapping: ChoirOS → Agent-OS Layers

| ChoirOS Component | Agent-OS Layer | Status |
|-------------------|----------------|--------|
| Frontend (React desktop) | User & Application | ✅ Exists |
| Machine (mode selection) | Workflow & Orchestration | ⚠️ Partial (mode_engine.py) |
| Agent Harness | Agent Runtime | ✅ Exists |
| ModeConfig / capabilities | Kernel (policy) | ⚠️ Partial |
| BAML Client | Model gateway | ✅ Exists |
| Tools (tools.py) | Tools (MCP) | ⚠️ Needs MCP schema |
| Verifier Runner | Services (verification) | ✅ Exists |
| NATS + SQLite | A2A + Observability | ⚠️ NATS problematic |
| Sandbox Runner | Services (isolation) | ✅ Exists |

### The Missing Pieces

**1. Agent Contract (Portable Specification)**

The Agent-OS paper defines a contract as the "ABI for agents":

```yaml
apiVersion: agentos/v0.2
kind: AgentContract
name: doc-rag-planner
class:
  latency: SRT
  slo:
    onset_ms: 250
    turn_ms: 1000
    jitter_p95_pct: 20
capabilities:
  - "web.fetch"
  - "fs.read"
  - "summarize"
compute:
  cpu: "1"
  mem: "2GiB"
modelPolicy:
  allow:
    - "local/8B"
    - "cloud/70B"
  max_context_tokens: 16000
memory:
  namespace: "city-planning"
  retention_days: 30
  rag:
    top_k: 8
    require_grounding: true
security:
  consent_for:
    - "fs.write"
    - "payment.charge"
observability:
  tracing: "opentelemetry"
  log_fields:
    - "prompt"
    - "sources"
```

**This replaces your ad-hoc mode configs.**

**2. Latency Classes (Real-Time Semantics)**

The paper defines three classes:

| Class | Definition | Use Case | SLOs |
|-------|------------|----------|------|
| **HRT** | Hard Real-Time | Safety-critical (robotics) | Deadline: 1-20ms, Jitter: ≤5ms, Zero misses |
| **SRT** | Soft Real-Time | Interactive (chat, voice) | Onset: 150-300ms, Turn: 0.8-1.2s |
| **DT** | Delay-Tolerant | Batch (indexing, analytics) | SLA in hours, maximize throughput |

**Your modes map to these:**
- `CALM` → SRT (interactive copilot)
- `CURIOUS` → DT (research, can be slow)
- `SKEPTICAL` → SRT (verification needs responsiveness)
- `PARANOID` → HRT (safety-critical checks)

**3. Open Standards**

| Standard | Purpose | ChoirOS Status |
|----------|---------|----------------|
| **MCP** (Model Context Protocol) | Tool schemas | ❌ Need to adopt |
| **A2A** (Agent-to-Agent) | Inter-agent messaging | ⚠️ Using NATS, should consider |
| **OTel** (OpenTelemetry) | Observability | ❌ Need to integrate |

---

## Part 4: How the Computer Wants to Be Programmed

You asked: *"how does the computer want to be programmed?"*

The research points to a clear answer: **The computer wants to be programmed as a deterministic state machine with concurrent actors.**

### The Natural Pattern

```
┌─────────────────────────────────────────────────────────────┐
│              THE NATURAL ABSTRACTION                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  1. STATE IS IMMUTABLE                                        │
│     - No hidden mutations                                    │
│     - Every transformation explicit                          │
│     - Fully reproducible                                     │
│                                                               │
│  2. COMPUTATION IS ACTIONS                                    │
│     - Each action reads/writes declared state                │
│     - Actions compose into graphs                            │
│     - Graphs = visualizable, debuggable                       │
│                                                               │
│  3. EXECUTION IS STATEFUL                                     │
│     - Current state = everything needed to continue          │
│     - Checkpoint = save state                                 │
│     - Resume = restore state                                  │
│     - Fork = branch state                                     │
│                                                               │
│  4. CONCURRENCY IS ACTORS                                     │
│     - Each actor = isolated state machine                    │
│     - Actors communicate via messages                        │
│     - No shared state = no locks needed                      │
│                                                               │
│  5. SAFETY IS CONTRACTS                                      │
│     - Declare capabilities upfront                           │
│     - Kernel enforces at runtime                             │
│     - Audit trail is immutable                                │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Why This Pattern Emerged

**1. LLMs Are Stochastic**
- Non-deterministic by nature
- Need deterministic wrapper for safety
- State machines provide that wrapper

**2. Agents Need Coordination**
- Multiple agents working together
- Need to reason about what they're doing
- State machines make reasoning explicit

**3. Failures Are Inevitable**
- Networks fail, models timeout, tools break
- Need checkpoint/resume
- State machines enable replay

**4. Observability Is Non-Negotiable**
- "Why did the agent do that?"
- State graph shows the path
- Every state transition is auditable

**5. Security Requires Explicitness**
- Capability-based security
- "What can this agent do?"
- Actions declare reads/writes
- Kernel enforces contracts

---

## Part 5: Proposed Architecture for ChoirOS

### Unified: Burr + Ray + Agent-OS Principles

```
┌─────────────────────────────────────────────────────────────┐
│                    FRONTEND (React)                         │
│  - Desktop UI metaphor                                        │
│  - Multiple windows = multiple agents                       │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP/WebSocket
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 BACKEND API (FastAPI)                        │
│  - REST/WebSocket endpoints                                  │
│  - Routes to Ray actors                                      │
│  - No orchestration logic                                    │
└──────────────────────────┬──────────────────────────────────┘
                           │ ray.remote()
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                     RAY CLUSTER                              │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  ConversationActors (one per window)                   │  │
│  │  - Each has a Burr application                         │  │
│  │  - State isolated per conversation                     │  │
│  │  - Fault-tolerant with checkpointing                   │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  OrchestratorActor (singleton)                         │  │
│  │  - Spawns parallel researchers (Ray actors)             │  │
│  │  - Aggregates results                                   │  │
│  │  - Manages multi-agent workflows                        │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  ServiceActors (singleton services)                     │  │
│  │  - GitActor (serializes git ops)                        │  │
│  │  - MemoryActor (RAG, vector stores)                     │  │
│  │  - ToolActor (MCP tool registry)                        │  │
│  │  - ModelGateway (routes to LLMs)                        │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  KernelActor (admission control)                       │  │
│  │  - Validates AgentContracts                             │  │
│  │  - Enforces latency class (HRT/SRT/DT)                  │  │
│  │  - Checks RBAC/capabilities                             │  │
│  │  - Audits all actions                                   │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  EVENT STORE (SQLite)                        │
│  - Immutable event log                                       │
│  - Materialized projections                                  │
│  - AHDB state                                                │
│  - Burr checkpoints                                         │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Path

**Phase 1: Replace Machine with Burr**
```python
# OLD: supervisor/machine.py
class Machine:
    def select_mode(self, inputs):
        return mode_engine.transition_mode(...)

# NEW: supervisor/machine_burr.py
from burr.core import ApplicationBuilder

app = (
    ApplicationBuilder()
    .with_actions(
        calm_mode=action(process_calm),
        curious_mode=action(process_curious),
        skeptical_mode=action(process_skeptical),
        paranoid_mode=action(process_paranoid),
        contrite_mode=action(process_contrite),
    )
    .with_transitions(
        ("calm_mode", "curious_mode", ambiguity_check),
        ("curious_mode", "calm_mode", resolved_check),
        ("calm_mode", "skeptical_mode", verifier_failed),
        # ... all mode transitions
    )
    .build()
```

**Phase 2: Wrap Burr Apps in Ray Actors**
```python
@ray.remote
class ConversationAgent:
    def __init__(self, contract: AgentContract):
        # Build Burr app from contract
        self.app = build_burr_app(contract)
        self.contract = contract

    async def process(self, prompt: str):
        # Execute with fault tolerance
        return await self.app.run(
            halt_after=["complete"],
            inputs={"prompt": prompt}
        )
```

**Phase 3: Define Agent Contracts**
```python
@dataclass
class AgentContract:
    name: str
    latency_class: Literal["HRT", "SRT", "DT"]
    capabilities: List[str]
    mode_policy: ModePolicy  # Your existing mode config
    compute: ComputeRequirements
    model_policy: ModelPolicy
    memory_policy: MemoryPolicy
    security_policy: SecurityPolicy

# Example
CALM_CONTRACT = AgentContract(
    name="calm_copilot",
    latency_class="SRT",
    capabilities=["fs.read", "fs.write", "git.commit", "web.search"],
    mode_policy=ModeConfig(mode_id="CALM", tools=[...]),
    compute=ComputeRequirements(cpu=1, mem="2GiB"),
    model_policy=ModelPolicy(allow=["local/8B", "cloud/70B"]),
    memory_policy=MemoryPolicy(
        namespace="user",
        retention_days=30,
        rag=RETRIEVAL_MODE
    ),
    security_policy=SecurityPolicy(
        consent_for=["fs.write", "payment.charge"]
    )
)
```

**Phase 4: Implement Kernel Enforcement**
```python
@ray.remote
class KernelActor:
    def __init__(self):
        self.policies = load_security_policies()

    def admit_agent(self, contract: AgentContract) -> bool:
        # Validate contract
        if not self.validate_capabilities(contract):
            return False

        # Check schedulability
        if contract.latency_class == "HRT":
            if not self.check_deadline_feasibility(contract):
                return False

        # Reserve resources
        self.reserve_resources(contract.compute)
        return True

    def tool_call(self, agent_id: str, tool: str, args: dict):
        # Enforce capability checks
        if not self.check_capability(agent_id, tool):
            raise PermissionError(f"Agent {agent_id} not allowed {tool}")

        # Check consent requirements
        if self.requires_consent(tool):
            consent = await self.request_consent(agent_id, tool, args)
            if not consent:
                raise PermissionError("Consent denied")

        # Audit log
        self.audit_log.append({
            "agent": agent_id,
            "tool": tool,
            "args": args,
            "timestamp": datetime.now()
        })

        # Execute
        return execute_tool(tool, args)
```

**Phase 5: Frontend Integration**
```typescript
// Frontend talks to Ray actors via backend API
async function sendMessage(windowId: string, message: string) {
    const response = await fetch(`/api/conversation/${windowId}/message`, {
        method: 'POST',
        body: JSON.stringify({ message })
    });

    // Backend routes to Ray actor:
    // conversation_actor = ray.get_actor(f"conversation_{window_id}")
    // result = await conversation_actor.process.remote(message)

    return response.json();
}
```

---

## Part 6: What "Automatic Computer" Means

### The Fundamental Primitive

You're building a **universal executor of intent**:

```
Human Intent (natural language)
         │
         ▼
  Kernel (validates, contracts)
         │
         ▼
  Orchestrator (plans, delegates)
         │
         ▼
  Agent Runtime (state machines)
         │
         ▼
  Services (tools, memory, models)
         │
         ▼
  Effects in the world
```

This is the **automatic computer**:
- Input: Intent (language)
- Output: Effects (world changes)
- Middle: Deterministic, auditable, safe

### Why This Is the Next Operating System

**1. Language Is the New Interface**
- CLI → GUI → NLI (Natural Language Interface)
- Agents translate intent → action
- OS must manage this translation

**2. Models Are the New CPU**
- LLMs execute "thinking"
- Need scheduling, prioritization, placement
- Model gateway = CPU scheduler

**3. Tools Are the New Syscalls**
- Function calling = system calls
- Need capability-based security
- Tool registry = device drivers

**4. Agents Are the New Processes**
- Stateful, concurrent, isolated
- Need lifecycle management
- Agent runtime = process manager

**5. Workflows Are the New Applications**
- Multi-agent orchestration = apps
- Need composition, replay, debugging
- State machines = binary code

### The Computer "Wants" This Because:

**1. Determinism Enables Trust**
- State machines = predictable
- Contracts = enforceable
- Audit = accountability

**2. Isolation Enables Scale**
- Actors = no shared state
- No locks = no deadlocks
- Concurrent = parallel execution

**3. Observability Enables Debugging**
- Every state transition visible
- Replay errors deterministically
- Fix prompts, not code

**4. Portability Enables Ecosystem**
- Agent contracts run anywhere
- Open standards (MCP, A2A, OTel)
- No vendor lock-in

**5. Safety Enables Deployment**
- Kernel enforces policies
- Capabilities limit damage
- Checkpoints enable rollback

---

## Part 7: Open Questions & Research Agenda

### What We Don't Know Yet

**1. LLM Scheduling**
- How do you schedule stochastic models?
- WCET analysis for non-deterministic execution?
- Admission control for unknown token counts?

**2. Real-Time LLMs**
- Can we guarantee 10ms deadlines for LLMs?
- Bounded latency for streaming tokens?
- When is HRT even possible?

**3. Multi-Agent Security**
- How do agents prove they're following contracts?
- Preventing jailbreaks across agent boundaries?
- Zero-trust with probabilistic systems?

**4. Economic Models**
- How do you price agent execution?
- Token budgets vs compute budgets?
- Multi-tenant cost allocation?

**5. Standardization**
- Will MCP/A2A/OTel win?
- Or will vendors fragment ecosystem?
- How to ensure portability?

### What To Learn

**1. Burr Deep Dive**
- Read: https://burr.apache.org/
- Try: Building a simple state machine agent
- Study: Parallel sub-applications with Ray

**2. Ray Actors**
- Read: https://docs.ray.io/en/latest/ray-core/actors.html
- Try: Spawning 10 concurrent agents
- Study: Fault tolerance and retries

**3. Agent-OS Paper**
- Read: "Agent Operating Systems (Agent-OS): A Blueprint Architecture"
- Study: Five-layer architecture
- Understand: Latency classes and contracts

**4. Open Standards**
- MCP (Model Context Protocol)
- A2A (Agent-to-Agent)
- OTel (OpenTelemetry)

**5. Production Systems**
- RayAI platform
- AutoGen (Microsoft)
- LangGraph
- Burr + Ray in production

---

## Part 8: Concrete Next Steps

### Immediate Actions

**1. Prototype Burr Mode Engine**
```bash
cd supervisor
pip install burr[start]
# Create: supervisor/machine_burr.py
# Map mode_engine.py transitions to Burr
```

**2. Test Ray Actors**
```bash
pip install ray[default]
# Create: supervisor/ray_actors.py
# Spawn 10 ConversationAgents
# Test concurrent execution
```

**3. Define Agent Contract Schema**
```python
# Create: supervisor/contracts.py
# Map ModeConfig to AgentContract
# Add latency_class (HRT/SRT/DT)
```

**4. Implement Kernel Actor**
```python
# Create: supervisor/kernel.py
# Admission control
# Capability checking
# Audit logging
```

### Decision Points

**1. Replace NATS or Keep It?**
- Replace NATS messaging with Ray actors
- Keep NATS for event streaming to frontend
- Or remove entirely and use WebSocket polling

**2. Frontend Architecture**
- Keep React desktop UI
- Or consider WASM, native apps
- WebSocket for real-time updates

**3. Persistence Strategy**
- Keep SQLite event store
- Burr checkpoints in SQLite
- Or migrate to PostgreSQL for scale

**4. Deployment Model**
- Local development (Ray local cluster)
- Cloud deployment (Ray on Kubernetes)
- Hybrid (edge + cloud)

---

## Conclusion

You were right. The state machine pattern is the natural abstraction for AI systems. **Burr** is the engineering of your intuition. **Ray** provides the actor model for concurrency. Together, they define the architecture for an AI-native operating system.

The "automatic computer" you're building is not an app—it's a fundamental primitive. It's how the computer wants to be programmed for the AI-driven internet.

**The pattern is:**
1. **State machines** (Burr) for deterministic agent logic
2. **Actors** (Ray) for concurrent execution
3. **Contracts** (Agent-OS) for portable safety
4. **Services** (MCP, A2A, OTel) for ecosystem interoperability

This is the architecture space. Now we need to choose where to build within it.

---

## References

1. **Apache Burr** - https://burr.apache.org/
2. **Burr Proposal** - https://cwiki.apache.org/confluence/display/INCUBATOR/BurrProposal
3. **Agent-OS Paper** - https://www.preprints.org/manuscript/202509.0077/v1
4. **Ray Actors** - https://docs.ray.io/en/latest/ray-core/actors.html
5. **Parallel Fault-Tolerant Agents with Burr/Ray** - https://blog.dagworks.io/p/parallel-fault-tolerant-agents-with
6. **RayAI Platform** - https://www.opencoreventures.com/blog/rayai-extends-ray-oss-to-orchestrate-multi-agent-systems
7. **Model Context Protocol (MCP)** - https://modelcontextprotocol.io/
8. **OpenTelemetry** - https://opentelemetry.io/

---

**End of Document**
