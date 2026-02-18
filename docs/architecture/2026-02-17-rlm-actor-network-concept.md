# RLM Actor Network: Conceptual Architecture

**Date:** 2026-02-17
**Status:** Research / Conceptual
**Authors:** ChoirOS Core Team

---

## Narrative Summary (1-minute read)

**Recursive Language Models (RLMs)** give the model control over its own execution harness. Rather than linear tool loops, the model composes context and controls topology—sequential, parallel, recursive—for each turn. This document explores RLM as the *default* execution mode in ChoirOS, with linear loops as a special case.

**Key insight:** RLM in an actor network is different from RLM in a single process. Each "recursive call" can become an actor message to another microVM. The model doesn't just delegate compute—it delegates *to a fresh security domain*.

**Self-prompting:** Models query MemoryAgent to compose their own context and working memory, replacing static "You are a..." role prompts with dynamic capability contracts.

**Deployment model:** Each ChoirOS sandbox runs in a microVM. Users run at least two (for live upgrades with headless verification and safe revert). RLM isn't sandboxed *inside* the sandbox—RLM *is* how sandboxes orchestrate each other.

**Security stance:** RLM doesn't introduce new attack vectors beyond existing LLM risks (prompt injection, data exfil). The recursive capability is exercised through typed actor messages, not arbitrary code execution. The security boundary remains the microVM.

**Model contracts:** ChoirOS defines capability contracts at three levels—System (RLM harness), Harness (Conductor/Terminal/Researcher), and Task (specific objective). These are API documentation, not role assignments.

---

## The Core Idea

### Traditional vs RLM Execution

| Aspect | Traditional Agent | RLM Actor Network |
|--------|------------------|-------------------|
| **Context** | Append-only message history | Model-composed per turn from documents |
| **Topology** | Linear: decide → execute → loop | Model-controlled: parallel, speculative, recursive |
| **Delegation** | Function calls in same process | Actor messages across microVMs |
| **State** | Accumulating context window | Document store + working memory |
| **Default** | Simple loop is the easy path | RLM is the default; simple loops are `NextAction::ToolCalls` |

### The RLM as Default

Linear execution becomes a degenerate case:

```rust
// Linear loop (what we have now)
loop {
    let decision = decide(&messages).await?;
    match decision.action {
        Action::ToolCall => execute_tools(),  // Continue
        Action::Complete => return Ok(()),    // Done
        Action::Block => return Err(()),      // Stuck
    }
}

// RLM default (what we're exploring)
loop {
    let composition = decide_context(&docs).await?;  // Model composes context
    match composition.next_action {
        NextAction::ToolCalls(_) => { /* Linear case */ }
        NextAction::FanOut(_) => { /* Parallel exploration */ }
        NextAction::Recurse(_) => { /* Delegate to sub-harness */ }
        NextAction::Complete(_) => { /* Done */ }
        NextAction::Block(_) => { /* Stuck */ }
    }
}
```

The model *always* decides what context to load and what topology to use. Most turns may choose simple tool calls. But the capability for parallel recursion is always available.

---

## Actor Network Semantics

### Authority Boundary (Conductor vs Writer)

- Conductor routes app-agents, not raw workers.
- Writer owns worker lifecycle and delegation planning for `researcher`/`terminal`.
- This keeps conductor as orchestration-only and makes worker policy local to the
  app-agent that mutates the living document.

### Single-Process RLM vs Actor RLM

| RLM Variant | Sub-call Implementation | Use Case |
|-------------|------------------------|----------|
| **In-process** | `tokio::spawn` internal harness | Fast, same-security-domain |
| **Same-sandbox** | Actor message to sibling agent | Same microVM, different capability |
| **Cross-sandbox** | Actor message to external sandbox | Different microVM, full isolation |
| **Cross-user** | Verified capability delegation | Collaborative computation |

### The Actor Message as Recursive Call

In ChoirOS, an RLM "recursive call" becomes:

```rust
// Model outputs: "I need to delegate this sub-task"
let sub_objective = "Analyze the security implications of this code";

// Harness translates to actor message
let sub_harness = spawn_sub_harness(SubHarnessSpec {
    objective: sub_objective,
    context_seed: selected_documents,  // Not full history
    diversity_config: DiversitySpec {
        model: "haiku",              // Cheaper for sub-task
        temperature: 0.2,
    },
}).await?;

// Parent continues; will receive result via actor message
```

The sub-harness is an actor. It may run in:
- Same process (fast, shared memory)
- Same microVM (isolated by actor boundary)
- Different microVM (full isolation, different security domain)

---

## Deployment Architecture

### MicroVM Topology

```
┌─────────────────────────────────────────────────────────────┐
│ User Deployment (minimum 2 microVMs)                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐        ┌─────────────────┐             │
│  │  MicroVM A      │        │  MicroVM B      │             │
│  │  (Active)       │◄──────►│  (Staging)      │             │
│  │                 │  live  │                 │             │
│  │  ┌───────────┐  │ upgrade│  ┌───────────┐  │             │
│  │  │Conductor  │  │        │  │Conductor  │  │             │
│  │  │Agent      │  │        │  │Agent      │  │             │
│  │  └─────┬─────┘  │        │  └─────┬─────┘  │             │
│  │        │         │        │        │         │             │
│  │  ┌─────┴─────┐  │        │  ┌─────┴─────┐  │             │
│  │  │Terminal   │  │        │  │Terminal   │  │             │
│  │  │Researcher │  │        │  │Researcher │  │             │
│  │  │Writer     │  │        │  │Writer     │  │             │
│  │  └───────────┘  │        │  └───────────┘  │             │
│  │                 │        │                 │             │
│  └─────────────────┘        └─────────────────┘             │
│           ▲                          │                      │
│           └────────── switchover ────┘                      │
│                    (verify, then migrate)                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Why Minimum 2 MicroVMs

1. **Headless verification:** Deploy new code to staging, run E2E tests without affecting active
2. **Safe revert:** If staging tests fail, never activate; if active fails, switch back
3. **Live upgrade:** Zero-downtime code updates for long-running agents
4. **RLM delegation:** Cross-sandbox recursion for security isolation

### RLM Delegation Across MicroVMs

```rust
// Conductor in MicroVM A decides to delegate
let remote_result = conductor
    .delegate_to("microvm-b", SubRequest {
        objective: "Verify this plan is safe",
        constraints: Constraints {
            max_cost: CostBudget::from_usd(0.10),
            max_latency: Duration::from_secs(30),
            required_capabilities: vec!["verification"],
        },
    })
    .await?;

// This is an RLM recursive call across security domains
```

---

## Security Analysis

### What's Different About RLM?

**Claim:** RLM is not inherently more dangerous than existing LLM patterns.

| Risk | Traditional LLM | RLM | Mitigation |
|------|-----------------|-----|------------|
| **Prompt injection** | Model executes injected instructions | Same risk | Input validation, constrained action space |
| **Data exfiltration** | Model leaks data via tool calls | Same risk | Network policy on microVM, audit logging |
| **Resource exhaustion** | Infinite loops, expensive calls | Explicit with recursion depth limits | Budgets, timeouts, depth limits |
| **Capability escalation** | Model requests higher-privilege tools | Same risk | Capability grants explicit, not inherited |

### What RLM Adds (Controlled)

| New Capability | Risk | Mitigation |
|----------------|------|------------|
| **Parallel fan-out** | Cost amplification | Explicit budget constraints per branch |
| **Recursive delegation** | Unbounded tree expansion | Max depth, max total calls, circuit breakers |
| **Cross-sandbox calls** | Lateral movement | Mutual authentication, capability attenuation |

### The MicroVM as Security Boundary

```rust
// RLM "code" is not arbitrary execution—it's constrained choice
pub enum NextAction {
    // These are the only options; model cannot invent new ones
    ToolCalls(Vec<ToolCall>),      // Pre-defined tools only
    FanOut(FanOutSpec),             // Bounded parallelism
    Recurse(RecurseSpec),           // Delegation to typed harness
    Complete(String),               // Terminal
    Block(String),                  // Terminal
}

// The "REPL" is not a real Python interpreter—it's a typed actor protocol
```

---

## Context Composition (The Key Shift)

### From Append-Only to Composed

**Traditional:**
```
Turn 1: [system] + [user] + [assistant] + [tool result]
Turn 2: [system] + [user] + [assistant] + [tool result] + [assistant] + [tool result]
Turn 3: ... keeps growing
```

**RLM Composer:**
```
Turn 1: [system] + [user objective] + [working memory] → decide → execute
Turn 2: [system] + [selected docs from Turn 1] + [new working memory] → decide → execute
Turn 3: [system] + [fresh composition] + [compressed history] → decide → execute
```

The model *selects* what to include, rather than inheriting everything.

### Document-Driven Integration

RLM composition maps to ChoirOS's existing document system:

```rust
pub struct ContextComposerCode {
    /// What documents to load
    pub sources: Vec<SourceRef>,

    /// How to query/transform them
    pub transformations: Vec<Transform>,

    /// Working memory (ephemeral, model-written)
    pub working_memory: String,

    /// Next action decision
    pub next_action: NextAction,
}

pub enum SourceRef {
    Document { id: DocId, version: Version },
    Query { query: String, index: IndexRef },
    PreviousTurn { turn_id: TurnId, selector: Selector },
    ToolOutput { call_id: CallId, summary: bool },
}
```

Documents are the durable layer. Working memory is ephemeral. The model composes the context window from these sources each turn.

---

## Diversity Strategies (Research Multipliers)

### Why RLM Helps Weaker Models

1. **Bounded context:** Don't overwhelm with full history
2. **Explicit selection:** Forces model to articulate what it needs
3. **Parallel exploration:** Multiple cheap models can explore; one strong model synthesizes
4. **Retry at composition:** Regenerate selection code (cheap) vs full tool retry (expensive)

### Diversity Dimensions

```rust
pub struct DiversitySpec {
    /// Different prompt framings
    pub framing_variants: Vec<String>,

    /// Different models per branch
    pub model_cascade: Vec<String>,

    /// Temperature variance
    pub temperature_range: (f32, f32),

    /// Capability specialization
    pub specialist_configs: Vec<CapabilityConfig>,
}

// Example: Security audit with diversity
let spec = DiversitySpec {
    framing_variants: vec![
        "Analyze as security audit",
        "Analyze as performance review",
        "Analyze as maintainability check",
    ],
    model_cascade: vec!["haiku", "sonnet", "opus"],
    ..Default::default()
};
```

---

## Implementation Path (Conceptual)

### Phase 1: Internal Harness RLM
- Add `NextAction` variants to existing `AgentDecision`
- Implement in-process fan-out and recursion
- Keep external interface identical

### Phase 2: Cross-Actor RLM
- Sub-harness spawning via actor messages
- Same-sandbox delegation
- Progress reporting for parallel branches

### Phase 3: Cross-Sandbox RLM
- Inter-microVM actor protocol
- Verified capability delegation
- Live upgrade with delegation drain

### Phase 4: Research Optimization
- Diversity strategies for weak models
- Model cascading (cheap compose → expensive synthesize)
- Automatic context compaction policies

---

## Open Questions

1. **Cache vs. Composition:** How much do we lose without prompt caching? Can we pre-compute document embeddings?

2. **Debugging:** How do we trace a recursive execution tree? How do we replay?

3. **Cost Attribution:** In a fan-out with 10 branches, who pays? How do we budget?

4. **Consensus:** When parallel branches disagree, how does the parent resolve?

5. **Live Upgrade:** How do we drain in-flight delegations during microVM switchover?

---

## What This Enables

| Capability | Before | After |
|------------|--------|-------|
| **Research tasks** | Linear search → synthesize | Parallel search strategies, recursive drill-down |
| **Code review** | Single pass | Multi-angle analysis (security, perf, maintainability) |
| **Verification** | Self-critique (unreliable) | Delegate to isolated harness with different model |
| **Long-running tasks** | Accumulating context | Fresh composition each turn, bounded context |
| **Multi-model workflows** | Manual orchestration | Automatic cascading based on task complexity |

---

## The RLM Capability Contract

Even self-prompting models need to understand their capabilities. This is not role-based prompting—it's an **interface contract** documenting the power available and how to exercise it.

### System Contract Template

```
You are operating within a Recursive Language Model (RLM) harness.

CAPABILITIES

1. CONTEXT COMPOSITION
   Each turn, you output code that selects what context to load. You are not
   given context automatically—you choose it.

   Available sources:
   - MemoryQuery: Retrieve relevant episodes from long-term memory
   - Document: Load specific files or living documents
   - PreviousTurn: Selectively include past outputs (not automatic)
   - ToolOutput: Include results from tool executions

2. EXECUTION TOPOLOGY
   You control how computation proceeds:

   - ToolCalls: Execute tools sequentially (linear mode)
   - FanOut: Spawn parallel branches with different approaches
   - Recurse: Delegate to a sub-harness with fresh context
   - Complete: Terminate with final answer
   - Block: Signal that you cannot proceed

3. TOPOLOGY SELECTION GUIDANCE

   Use ToolCalls (linear) when:
   - You have high confidence in the next step
   - The task is straightforward and sequential
   - You need to gather information before deciding

   Use FanOut (parallel) when:
   - Multiple approaches seem viable
   - You need to explore alternatives simultaneously
   - Verification from different angles would help

   Use Recurse (delegation) when:
   - A sub-task is complex enough to benefit from fresh context
   - The sub-task has different constraints than the parent
   - You want isolation for speculative exploration

4. WORKING MEMORY
   You maintain a working_memory string that carries your focus and reasoning
   across turns. This is ephemeral—recomposed each turn, not accumulated.
   Articulate your current focus clearly.

5. LONG-TERM MEMORY
   MemoryAgent stores episodic history. Query it explicitly:
   - "How did I solve similar problems?"
   - "What patterns led to success in this domain?"
   - "What failed before in similar situations?"

   Memory retrieval is not automatic. You must request it.

CONSTRAINTS

- Max recursion depth: {max_depth}
- Parallel branch budget: {max_parallel_branches}
- Cost budget per fan-out: {budget}
- Context window: {context_limit} tokens (you manage this via composition)

OUTPUT FORMAT

You output a ContextSnapshot containing:
1. sources: Vec<ContextSource> — what to load
2. working_memory: String — your current focus and reasoning
3. next_action: NextAction — topology decision

Do not explain your reasoning outside the working_memory field.
```

### Contract vs Role

| Aspect | Role-Based | RLM Contract |
|--------|-----------|--------------|
| **Content** | "You are a coding assistant" | "You have these capabilities" |
| **Nature** | Identity assignment | Interface documentation |
| **Mutability** | Fixed for session | Referenced each turn, not repeated |
| **Source of truth** | System prompt | Composed from memory + documents |
| **Model behavior** | Acts within role | Chooses capabilities based on situation |

### Why This Matters

Without the contract, the model doesn't know it *can* fan out, recurse, or query memory. It falls back to default linear behavior. The contract is **enabling constraints**—it defines the possibility space so the model can choose appropriately.

### Model-Specific Contracts

Different models may need different contract elaborations:

**For strong models (Sonnet, Opus, o1):**
```
You are responsible for topology decisions. Choose FanOut when exploration
would help, Recurse when delegation is appropriate. You have the full capability
set available.
```

**For weaker models (Haiku, GPT-4o-mini):**
```
Default to ToolCalls (linear) unless you are explicitly uncertain.
FanOut is available but costs more—use when the benefit is clear.
Query MemoryAgent liberally—it compensates for limited context window.
```

**For specialized sub-harnesses:**
```
You are a verification specialist. Your job is to check work, not do it.
Default to Complete with critique, or Recurse if the task needs more work.
```

## ChoirOS Model Contract Hierarchy

ChoirOS contracts with models at multiple levels:

```
┌─────────────────────────────────────────────────────────────┐
│  LEVEL 1: SYSTEM CONTRACT                                    │
│  (Invariant across all ChoirOS interactions)                │
│                                                              │
│  - You operate in an RLM harness                            │
│  - You may compose context and control topology             │
│  - Episodic memory is available via query                   │
│  - Filesystem is truth; memory is resonance                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────────┐
│  LEVEL 2: HARNESS CONTRACT                                   │
│  (Specific to harness type: Conductor, Terminal, etc.)      │
│                                                              │
│  Conductor harness:                                          │
│  - You orchestrate via actor messages                       │
│  - You do not execute tools directly                        │
│  - You spawn workers and receive their reports              │
│                                                              │
│  Terminal harness:                                           │
│  - You execute bash commands                                │
│  - You report results to Conductor                          │
│                                                              │
│  Researcher harness:                                         │
│  - You search and synthesize information                    │
│  - You write findings to documents                          │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────────┐
│  LEVEL 3: TASK CONTRACT                                      │
│  (Specific to current objective)                            │
│                                                              │
│  - User provides objective                                  │
│  - Model retrieves relevant patterns from memory            │
│  - Model composes context appropriate to task               │
│  - Model selects topology based on uncertainty/confidence   │
└─────────────────────────────────────────────────────────────┘
```

### Contract Versioning and Validation

Contracts are versioned and validated:

```rust
pub struct ModelContract {
    pub version: String,           // "2026.02.17"
    pub level: ContractLevel,      // System | Harness | Task
    pub content: String,           // The contract text
    pub min_model_tier: ModelTier, // Haiku | Sonnet | Opus
    pub validation_hash: String,   // Ensure contract integrity
}

pub enum ContractLevel {
    System,    // All ChoirOS models
    Harness,   // Specific harness type
    Task,      // Specific task instance
}
```

**Validation:**
- Contracts are hashed and stored
- Models reference contract version in responses
- Mismatches trigger warnings or fallback behavior

### The Contract as API Documentation

Think of the RLM contract as **API documentation for the harness**. Just as a developer needs docs to use a library, the model needs the contract to use its capabilities effectively.

The difference from role-based prompting:
- **Role**: "You are a doctor" → constrains identity
- **Contract**: "You have these tools" → enables capability

The model still decides *how* to use the capabilities based on the situation.

## Self-Prompting: Models Compose Their Own Context

The RLM architecture enables a fundamental shift from **role-based prompting** to **self-prompting**. The model doesn't rely on engineered system prompts—it queries memory and composes its own context.

### The Problem with Role-Based Prompting

Current patterns:
```
"You are a helpful coding assistant..."
"You are an expert security researcher..."
"You are a technical writer..."
```

These are:
- **Static** — same prompt regardless of task
- **Brittle** — model must remember its role across turns
- **Wasteful** — consumes context window with boilerplate
- **Limiting** — constrains the model to a single modality

### Self-Prompting Through Context Composition

The RLM model writes its own prompt by selecting what to include:

```rust
// Model outputs ContextComposerCode that returns:
ContextSnapshot {
    sources: vec![
        // Retrieve relevant expertise patterns from memory
        SourceRef::MemoryQuery {
            query: "successful security analysis patterns",
            filter: high_quality_strategies(),
            top_k: 3,
        },
        // Include specific technical context
        SourceRef::Document { id: "current_code_module" },
        // Pull in relevant prior work
        SourceRef::MemoryQuery {
            query: objective,
            filter: similar_past_tasks(),
            top_k: 2,
        },
    ],
    working_memory: "Analyzing auth module. Focus: race conditions in session management. "
                  "Pattern from memory: check shared state first, then synchronization.",
    next_action: NextAction::ToolCalls([...]),
}
```

The "system prompt" emerges from:
1. **Retrieved patterns** — what worked before
2. **Current context** — what's relevant now
3. **Working memory** — the model's own articulation of focus

### Example: Self-Prompting Evolution

**Turn 1 — Initial approach:**
```rust
working_memory: "User wants to fix auth bug. Need to understand the codebase."
sources: [codebase_structure, auth_related_files]
```

**Turn 2 — After discovering it's a race condition:**
```rust
working_memory: "Focus: race condition in session_manager.rs. "
                  "Need to identify shared state and synchronization points."
sources: [
    session_manager.rs,
    MemoryQuery { "race condition fixes that worked", top_k: 3 },
    concurrency_best_practices_doc,
]
```

**Turn 3 — Pivoting to parallel exploration:**
```rust
working_memory: "Uncertain about fix approach. Exploring 3 angles: "
                  "(1) mutex placement, (2) atomic operations, (3) state elimination"
next_action: NextAction::FanOut({
    branches: [
        "Analyze mutex placement in session_manager",
        "Evaluate atomic operation alternatives",
        "Assess whether shared state can be eliminated"
    ]
})
```

The model isn't following a role—it's **prompting itself** based on evolving understanding.

### Memory as Prompt Library

As episodic memory grows, it becomes a **queryable prompt library**:

| Query Type | Returns | Becomes |
|------------|---------|---------|
| "How did I solve similar problems?" | Past successful strategies | Few-shot examples |
| "What do I know about this domain?" | Accumulated domain knowledge | Expertise context |
| "What failed before?" | Past mistakes with critiques | Anti-patterns to avoid |
| "What's the standard approach?" | Common successful patterns | Default methodology |

The model retrieves these *into* its working memory, effectively constructing a custom prompt for the current situation.

### The Prompting Stack

```
┌─────────────────────────────────────────┐
│  MODEL-GENERATED "SYSTEM PROMPT"        │  ← Composed each turn from:
│                                         │
│  [Retrieved patterns from memory]       │     - Past successes
│  [Current technical context]            │     - Domain knowledge
│  [Working memory articulation]          │     - Model's focus
│  [Tool descriptions]                    │     - Available actions
│                                         │
│  "Working on: {objective}              │
│   Context: {selected_documents}        │
│   Patterns: {retrieved_memories}       │
│   Focus: {working_memory}"             │
├─────────────────────────────────────────┤
│  USER MESSAGE / OBJECTIVE               │
└─────────────────────────────────────────┘
```

No static "You are a..." — the model's identity emerges from what it retrieves and how it articulates its focus.

### Implications for MemoryAgent Design

MemoryAgent must store **prompt-worthy content**:

```rust
pub struct MemoryRecord {
    // Not just what happened, but how to prompt with it
    pub text: String,           // The episode content
    pub prompt_template: String, // "When you see X, consider Y"
    pub success_pattern: String, // Why this worked
    pub failure_pattern: Option<String>, // What to avoid

    // For retrieval
    pub embedding: Vec<f32>,
    pub quality_score: f32,
    // ...
}
```

When the RLM queries memory, it gets back **usable prompt fragments**, not just raw history.

### Research Task Example

**Traditional prompting:**
```
You are a research assistant. Please search for information about
distributed systems consensus protocols and summarize your findings.
```

**Self-prompting with RLM:**
```rust
// Model composes:
working_memory: "Research goal: understand consensus protocols for system design. "
                  "Approach: start with established sources, then recent developments."

sources: [
    // Query memory for prior research patterns
    MemoryQuery { "effective research strategies for distributed systems", top_k: 2 },
    // Retrieved: "Start with academic surveys, then check recent conference proceedings"

    // Query for existing knowledge
    MemoryQuery { "what I know about consensus protocols", top_k: 3 },
    // Retrieved: Prior notes on Raft, Paxos, and recent papers
]

// Composed effective prompt without explicit instruction:
// "Research distributed systems consensus. Pattern: academic surveys first,
//  then recent proceedings. Existing knowledge: Raft, Paxos. Gap: recent developments."
```

### The Meta-Pattern

The RLM doesn't just execute tasks—it **discovers and stores prompting patterns**:

```
Run succeeds → Store: "For X type of problem, Y approach works"
              ↓
Future similar problem → Retrieve pattern → Include in context
              ↓
Model self-prompts with proven strategy
```

SONA learns which retrieval patterns lead to success. The system gets better at prompting itself over time.

## Connection to MemoryAgent

The RLM's `ContextSource::MemoryQuery` is the bridge to MemoryAgent. The RLM queries episodic memory to populate its working memory:

```rust
pub enum ContextSource {
    // ... other variants

    /// Query episodic memory (long-term)
    MemoryQuery {
        query: String,
        filter: MemoryFilter,
        top_k: usize,
    },
}

// In composition code, model can:
fn compose_context(docs, objective) -> ContextSnapshot {
    // Retrieve relevant episodes from long-term memory
    let relevant_history = memory_query(
        query: objective,
        filter: MemoryFilter {
            since: Some(Duration::days(30)),
            event_type_prefix: Some("conductor.run.completed"),
        },
        top_k: 3
    );

    ContextSnapshot {
        sources: [
            // Include retrieved episodes
            ContextSource::MemoryResults(relevant_history),
            // Plus current working docs
            ContextSource::Document { id: "current_task" },
        ],
        working_memory: format!("Focus: {}. Relevant past: {:?}",
            objective,
            summarize_episodes(relevant_history)),
        next_action: // ...
    }
}
```

### The Retrieval Flow

```
User Input
    │
    ├── RLM composes ContextSnapshot
    │       │
    │       └── MemoryQuery { query: "how to approach X" }
    │           │
    │           └── MemoryAgent.recall()
    │               │
    │               ├── HNSW similarity search (Layer 1)
    │               ├── Hyperbolic reranking (Layer 2) — optional
    │               └── Episode expansion (Layer 3) — hypergraph k-hop
    │
    └── Retrieved episodes → included in composed context
```

The RLM decides *what* to query; MemoryAgent handles *how* to retrieve. This separation lets each layer optimize—RLM for strategy, MemoryAgent for retrieval quality.

### Memory-Driven Topology Decisions

Memory retrieval can trigger topology shifts:

```rust
// Model retrieves past similar tasks
let similar_tasks = memory_query(objective, top_k: 5);

// Based on retrieved patterns, decides topology
if similar_tasks.iter().any(|t| t.quality_score > 0.9) {
    // High-confidence pattern exists: execute directly
    NextAction::ToolCalls(proven_strategy_from_memory())
} else if similar_tasks.len() >= 3 {
    // Multiple partial matches: fan out to explore variations
    NextAction::FanOut(similar_tasks.into_iter().map(|t| {
        BranchSpec { prompt_variant: t.strategy }
    }).collect())
} else {
    // No relevant history: recurse into decomposition
    NextAction::Recurse({
        objective: format!("Break down: {}", objective),
        context_seed: minimal_context(),
    })
}
```

Memory doesn't just inform—it **drives control flow**.

## See Also

- `simplified-agent-harness.md` - Current harness architecture
- `2026-02-14-three-level-hierarchy-runtime.md` - Actor hierarchy
- `2026-02-14-capability-ownership-matrix.md` - Capability boundaries
- `2026-02-16-memory-agent-architecture.md` - Episodic memory as prompt library
- External: [RLM Paper](https://arxiv.org/abs/2512.24601), [Jido Framework](https://github.com/agentjido/jido)

---

## What To Do Next

1. **Validate concept:** Does this align with ChoirOS's model-led control flow philosophy?
2. **Prototype Phase 1:** Add `NextAction` variants to existing harness, implement in-process fan-out
3. **Security review:** Formal analysis of cross-sandbox delegation risks
4. **Cost modeling:** Simulate diversity strategies on research tasks
5. **Community:** Share RLM actor network concept with RLM research community
