# CLAUDE.md - ChoirOS + Claude Flow Integration

## Narrative Summary (1-minute read)

**ChoirOS** is a supervision-tree-first runtime built in Rust (ractor + tokio). **Claude Flow** is a multi-agent swarm orchestration layer with self-learning memory (AgentDB + HNSW). Together: ChoirOS provides durable execution with actor isolation; Claude Flow provides intelligent coordination with pattern memory.

**When building ChoirOS:**
1. Use **ChoirOS patterns** for actor lifecycle, message passing, and failure domains
2. Use **Claude Flow patterns** for complex multi-file changes, research tasks, and accumulating institutional knowledge
3. The **sandbox** (ChoirOS) owns runtime state; **Claude Flow** owns coordination memory

---

## What Changed (2026-02-16)

- Integrated Claude Flow V3 for swarm coordination and AgentDB memory
- ChoirOS remains the authoritative runtime (Rust actors, supervision trees)
- Claude Flow provides learned routing and pattern retrieval atop ChoirOS
- New: `npx @claude-flow/cli@latest` commands for memory, swarms, skills
- New: 60+ specialized agents available via MCP

---

## Architecture: Two-Layer Stack

```
┌─────────────────────────────────────────────────────────────┐
│  CLAUDE FLOW (Orchestration Layer)                          │
│  - Swarm coordination (queen-led hierarchical)              │
│  - AgentDB memory with HNSW (150x faster retrieval)         │
│  - SONA router (learned task-to-agent mapping)              │
│  - 60+ agent types (coder, reviewer, researcher, etc.)      │
├─────────────────────────────────────────────────────────────┤
│  CHOIROS (Runtime Layer)                                    │
│  - Supervision trees: Application → Session → {Conductor,   │
│    Terminal, Desktop, Writer}                               │
│  - Actor model: ractor + tokio, message-passing concurrency │
│  - Scope isolation: session_id, thread_id for event routing │
│  - EventBus/EventStore: observability backbone              │
│  - WASM boundary: Dioxus frontend ←→ Rust backend           │
└─────────────────────────────────────────────────────────────┘
```

### Responsibilities by Layer

| Concern | ChoirOS (Runtime) | Claude Flow (Orchestration) |
|---------|------------------|----------------------------|
| **Process lifecycle** | Supervisors restart failed actors | Swarms spawn agents for tasks |
| **State management** | Actor-local state + EventStore | AgentDB patterns + HNSW index |
| **Execution** | TerminalActor runs bash commands | Agents suggest/plan changes |
| **Failure handling** | Let-it-crash, supervisor restart | Drift detection, consensus |
| **Learning** | Event sourcing for replay | SONA router, LoRA adapters |
| **Scope** | session_id/thread_id isolation | Namespace-scoped memory |

---

## ChoirOS: Core Runtime Patterns

### Supervision Tree (Authoritative)

```
ApplicationSupervisor
└── SessionSupervisor
    ├── ConductorSupervisor
    │   └── ConductorActor (orchestrates workers)
    ├── TerminalSupervisor
    │   └── TerminalActor (bash execution, telemetry)
    ├── DesktopSupervisor
    │   └── DesktopActor (Dioxus WebSocket)
    └── WriterSupervisor
        └── WriterActor (living-document authority)
```

### Model-Led Control Flow (Hard Rule)

- **Default to model-managed control flow** for multi-step orchestration
- **Do not encode brittle workflows** where model planning is expected
- **Conductor turns are non-blocking**: never poll child agents, never wait in loops
- **Deterministic logic only for safety rails**: identity/routing, capabilities, budgets/timeouts, cancellation, audit/trace
- **Natural-language messages carry objectives**; control authority via typed actor metadata

### Execution Boundaries

- **Conductor** = orchestration-only, no direct tool execution
- **TerminalActor** = exclusive bash execution path
- **Writer** = canonical authority for living-document mutations
- **EventBus/EventStore** = observability backbone for worker/task tracing

### Scope Isolation (Critical)

```rust
// Always include for event retrieval
session_id: String,  // Prevents cross-session bleed
thread_id: String,   // Prevents cross-instance bleed
```

---

## Claude Flow: Swarm & Memory Patterns

### When to Use Swarms vs ChoirOS Actors

| Scenario | Use | Why |
|----------|-----|-----|
| Multi-file refactor across modules | **Swarm** (5-8 agents) | Parallel exploration, consensus on approach |
| Research spike (new dependency, API) | **Swarm** (researcher agents) | Breadth-first information gathering |
| Implement single actor | **ChoirOS** (1 agent via Task) | Direct execution, deterministic result |
| Debug failing test | **ChoirOS** (terminal + conductor) | Tight feedback loop, precise control |
| Accumulate patterns over time | **AgentDB** | HNSW retrieval, EWC++ isolation |

### 3-Tier Model Routing (Claude Flow)

| Tier | Handler | Latency | Cost | Use Cases |
|------|---------|---------|------|-----------|
| **1** | Agent Booster (WASM) | <1ms | $0 | Simple transforms (var→const, add types) |
| **2** | Haiku | ~500ms | $0.0002 | Simple tasks, low complexity (<30%) |
| **3** | Sonnet/Opus | 2-5s | $0.003-0.015 | Complex reasoning, architecture (>30%) |

### Memory Commands (AgentDB)

```bash
# Store a pattern (success/failure tracked)
npx @claude-flow/cli@latest memory store \
  --key "actor-impl-pattern" \
  --value "Use Actor::pre_start for init, handle in main loop" \
  --namespace choir-patterns \
  --tags "rust,ractor,boilerplate"

# Retrieve similar patterns
npx @claude-flow/cli@latest memory search \
  --query "how to implement new actor" \
  --namespace choir-patterns \
  --limit 5

# Check what's learned
npx @claude-flow/cli@latest memory list --namespace choir-patterns
```

### Swarm Initialization

```bash
# Initialize for complex multi-file work
npx @claude-flow/cli@latest swarm init \
  --topology hierarchical \
  --max-agents 8 \
  --strategy specialized \
  --namespace choir-session-$(date +%s)
```

---

## Integration: ChoirOS + Claude Flow

### Pattern 1: Swarm Plans, ChoirOS Executes

```rust
// 1. Claude Flow swarm analyzes codebase, proposes refactor plan
// 2. Plan handed to ChoirOS Conductor as structured task
// 3. Conductor spawns workers (TerminalActor, FileEdit tools)
// 4. Outcomes logged to EventStore AND AgentDB

ConductorMsg::WakeWithRequest { request } => {
    // Query AgentDB for similar past refactors
    let patterns = claude_flow_memory_search(
        &request.description,
        "choir-refactors"
    ).await?;

    if patterns.iter().any(|p| p.success_rate > 0.9) {
        // High confidence: delegate to workers
        spawn_terminal_workers(patterns[0].steps.clone()).await?;
    } else {
        // Low confidence: conductor-led with checkpoints
        plan_and_execute_with_checkpoints(request).await?;
    }
}
```

### Pattern 2: Terminal Outcomes → AgentDB

```rust
// TerminalActor logs command outcomes
TerminalMsg::CommandComplete { command, exit_code, output } => {
    let outcome = Outcome {
        success: exit_code == 0,
        duration_ms: elapsed.as_millis() as u32,
        output_hash: hash(&output),
    };

    // ChoirOS: EventStore for replay/debugging
    event_store.send(Event::TerminalOutcome { ... }).await?;

    // Claude Flow: AgentDB for pattern learning
    claude_flow_memory_record(
        &command.to_string(),
        Action::TerminalCommand(command),
        outcome,
    ).await?; // Fire-and-forget
}
```

### Pattern 3: Session Isolation

```rust
// ChoirOS: session_id for actor/event isolation
// Claude Flow: namespace for memory isolation

let session_id = generate_session_id();
let namespace = format!("choir-session-{}", session_id);

// All AgentDB ops for this session use namespace
// EWC++ ensures learning here doesn't leak to other sessions
```

---

## Development Workflow

### Quick Commands

```bash
# ChoirOS (Rust backend)
just dev-sandbox          # Run backend API server
just test-integration     # Run integration tests
cargo test -p sandbox --test desktop_api_test

# Claude Flow (Node.js tooling)
npx @claude-flow/cli@latest daemon start
npx @claude-flow/cli@latest memory init
npx @claude-flow/cli@latest doctor --fix

# Combined: Start dev environment with memory
just dev-sandbox &
npx @claude-flow/cli@latest daemon start
```

### Code Style (ChoirOS - Rust)

**Formatting:** `cargo fmt` (enforced in CI), max 100 chars, 4 spaces
**Naming:** `PascalCase` types, `snake_case` functions, `SCREAMING_SNAKE_CASE` consts
**Error Handling:** `thiserror` for custom errors, `anyhow` for app-level propagation
**Async:** `tokio` + `ractor::Actor` pattern

See original ChoirOS guidelines (imports order, documentation patterns, etc.) preserved below.

---

## Current High-Priority Targets

1. Typed worker event schema for actor-call rendering (`spawned/progress/complete/failed`)
2. Terminal loop event enrichment (`tool_call`, `tool_result`, durations, retry/error metadata)
3. Direct worker/app-to-conductor request-message contract
4. **NEW:** Claude Flow memory integration (pattern retrieval in Conductor)
5. **NEW:** Accumulate ChoirOS patterns in AgentDB (actor boilerplate, test patterns)
6. Ordered websocket integration tests for scoped multi-instance streams
7. Writer app-agent harness completion
8. Conductor wake-context hardening with bounded system agent-tree snapshots

---

## Naming Reconciliation (Authoritative)

| Term | Meaning |
|------|---------|
| `Logging` | Event capture/persistence/transport only |
| `Watcher` | Optional recurring-event detection/alerting actor; not core run-step authority |
| `Summarizer` | Human-readable compression over event batches/windows |
| `Agent` (ChoirOS) | ractor Actor - isolated process with message inbox |
| `Agent` (Claude Flow) | LLM-powered worker in a swarm |
| `Session` | ChoirOS session (supervision tree scope) = Claude Flow namespace |
| `Pattern` | AgentDB stored reasoning trace with success rate |
| `Expert` | LoRA adapter specialized for task type |

---

## Documentation Readability Rule

Unrendered markdown must still be readable.

For major architecture/roadmap docs, include these top sections:
- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`

Primary human-first index: `docs/architecture/NARRATIVE_INDEX.md`

---

## Original ChoirOS Guidelines (Preserved)

### Code Style Guidelines (Rust)

**Formatting:**
- Use `cargo fmt` (enforced in CI)
- Max line length: 100 characters (default)
- 4 spaces for indentation
- Trailing commas in multi-line structs/enums

**Imports:**
```rust
// Standard library first
use std::collections::HashMap;
use std::sync::Arc;

// External crates (alphabetical within groups)
use chrono::{DateTime, Utc};
use ractor::{concurrency::Duration, Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, error};

// Internal modules
use crate::actors::{ChatActor, EventStoreActor, TerminalActor};
use shared_types::{ActorId, Event};
```

**Error Handling:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActorError {
    #[error("Actor not found: {0}")]
    NotFound(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

### Project Structure

```
choiros-rs/
├── Cargo.toml           # Workspace definition
├── Justfile             # Task runner commands
├── sandbox/             # Backend API + actors (ChoirOS runtime)
│   ├── src/
│   │   ├── actors/      # Ractor actors (conductor, terminal, events)
│   │   ├── api/         # HTTP/WebSocket endpoints
│   │   └── tools/       # Agent tool system
│   └── tests/           # Integration tests
├── dioxus-desktop/      # Dioxus 0.7 frontend (WASM)
├── shared-types/        # Common types (ActorId, Event, etc.)
├── hypervisor/          # VM management component
├── .claude/             # Claude Flow V3 (generated)
│   ├── skills/          # 29 installed skills
│   ├── agents/          # 99 agent definitions
│   └── settings.json    # Hook configurations
└── .claude-flow/        # Claude Flow runtime
    ├── config.yaml
    ├── data/            # AgentDB + HNSW
    └── sessions/
```

---

## Claude Flow V3 CLI Reference

| Command | Description |
|---------|-------------|
| `npx @claude-flow/cli@latest init --wizard` | Initialize project |
| `npx @claude-flow/cli@latest daemon start` | Start background workers |
| `npx @claude-flow/cli@latest memory search --query "..."` | Retrieve patterns |
| `npx @claude-flow/cli@latest memory store --key "..." --value "..."` | Store pattern |
| `npx @claude-flow/cli@latest swarm init --topology hierarchical` | Start swarm |
| `npx @claude-flow/cli@latest doctor --fix` | Diagnose and repair |

---

## Support

- **ChoirOS Runtime**: Rust docs, `cargo doc --open`, architecture in `docs/architecture/`
- **Claude Flow**: https://github.com/ruvnet/claude-flow
- **Integration Issues**: Tag with `choir-integration` in GitHub issues
