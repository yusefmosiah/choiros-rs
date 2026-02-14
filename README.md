# ChoirOS - The Automatic Computer

**ChoirOS** is the operating system for the **Agent Choir** - a multi-agent system where autonomous agents collaborate in harmony. Each user gets an isolated sandbox where agents (actors) manage state, execute tools, and compose solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language. The Agent Choir sings in the automatic computer.*

## Current Status (2026-02-07)

Human-first docs entrypoint:
- `/Users/wiz/choiros-rs/docs/architecture/NARRATIVE_INDEX.md`

**✅ Working:**
- Supervision-tree runtime (`ApplicationSupervisor -> SessionSupervisor -> conductor/desktop/terminal`)
- EventStoreActor + EventBus-backed worker lifecycle tracing
- Conductor-centered actor messaging with delegated terminal execution
- WebSocket streaming for `actor_call` and worker lifecycle updates
- Scope-aware isolation (`session_id` + `thread_id`) across shared actor IDs
- Server running on localhost:8080

**🚧 In Progress:**
- Typed worker-event schema hardening for multi-agent observability
- Direct app/worker-to-conductor request message contract (minimal typed request kinds)
- Richer UI grouping for actor-call timelines (clean-by-default, deep-inspect on demand)
- Hypervisor routing for multi-user sandboxes

## Execution Policy (2026-02-09)

- Primary orchestration surface is `Prompt Bar -> Conductor`.
- Human interaction is living-document-first.
- Domain direction is `choir-ip.com`: durable outputs over ephemeral chat modality.
- Prefer skills and scripts for repeatable high-accuracy tasks over app-specific heuristics.
- `Model-Led Control Flow`: default to model-managed orchestration; keep deterministic logic for safety/operability rails only.

## Quick Start

### Local Development Setup

```bash
# Set local database path (required for local development)
export DATABASE_URL="./data/events.db"

# Build
cargo build -p sandbox

# Test
cargo test -p sandbox

# Run server
cargo run -p sandbox

# Test API (in another terminal)
curl http://localhost:8080/health
```

### Production Server

On the production server, the database path is hardcoded to `/opt/choiros/data/events.db` and no DATABASE_URL export is needed.

## Architecture - The Agent Choir

```
                    ┌─────────────────────────────────────┐
                    │         The Agent Choir             │
                    │    (Multi-Agent Collaboration)      │
                    └─────────────────────────────────────┘
                                      │
            ┌─────────────────────────┼─────────────────────────┐
            │                         │                         │
            ▼                         ▼                         ▼
   ┌───────────────────┐
   │ApplicationSupervisor
   └─────────┬─────────┘
             │
     ┌───────▼────────┐
     │SessionSupervisor│
     └───┬────────┬───┘
         │        │
    ┌─────▼──────┐ ┌──▼──────────┐
    │ConductorActor│ │TerminalActor │
    └────┬────────┘ └──────┬───────┘
         │              │
         └──────┬───────┘
                │
        ┌───────▼────────┐
        │EventBus + Store │
        │(worker/tool/human│
        │ stream + query) │
        └─────────────────┘
```

**Agent Choir Pattern:**
- **Actors as Agents** - Each actor is an autonomous agent with specific capabilities
- **Event Sourcing** - All agent actions recorded as events
- **Collective Intelligence** - Agents collaborate through shared event stream
- **Tool Augmentation** - Agents invoke tools (bash, file, code) to extend capabilities

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Frontend | Dioxus 0.7 (WASM) |
| Backend | Axum + Ractor |
| Database | SQLite via libsql 0.9 |
| Serialization | serde + serde_json |
| IDs | ULID |

## Project Structure

```
choiros-rs/
├── Cargo.toml              # Workspace definition
├── sandbox/                # Per-user ChoirOS instance
│   ├── src/
│   │   ├── main.rs         # Server entry point
│   │   ├── actors/         # Conductor, Terminal, EventStore/EventBus, desktop/workers
│   │   ├── api/            # HTTP handlers
│   │   ├── supervisor/     # supervision tree orchestration
│   │   └── tools/          # tool schemas and execution contracts
│   └── Cargo.toml
├── dioxus-desktop/         # Dioxus 0.7 frontend (WASM)
├── hypervisor/             # Edge router (WIP)
├── shared-types/           # Shared types between FE/BE
└── docs/
    ├── ARCHITECTURE_SPECIFICATION.md  # Full architecture spec
    └── archive/            # Old docs
```

## Key Design Principles

1. **Agent Choir** - Multiple autonomous agents collaborate through shared event stream
2. **Actor-owned state** - Each agent (actor) manages its own state in SQLite
3. **Event sourcing** - All agent actions logged as events (seq, event_type, payload)
4. **Tool augmentation** - Agents invoke tools to extend capabilities beyond conversation
5. **Collective intelligence** - Emergent behavior from agent collaboration

## API Endpoints

- `GET /health` - Health check
- For current human-interface and orchestration APIs, see `docs/architecture/NARRATIVE_INDEX.md` and active backend routes.

## Testing Notes

- Core integration:
  - `cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture`
  - `cargo test -p sandbox --test <exact_integration_binary> -- --nocapture`
- Use provider-agnostic prompts/commands in tests; avoid coupling to one external API.

## The Vision

**ChoirOS** is the operating system for the **Agent Choir** - where autonomous agents collaborate in harmony to build, execute, and evolve software. Each sandbox is a stage where agents perform:

- **Conductor + App Agents** orchestrate and execute capability work
- **Tool Agents** execute bash commands and file operations
- **Code Agents** write, test, and deploy code
- **Meta Agents** orchestrate the choir

The Agent Choir sings in the automatic computer. Agency lives in computation.

## Next Steps

1. **Prompt Bar + Conductor Flow** - Conductor is the primary orchestration surface; living-document UX is the primary human interface
2. **Skill Library Buildout** - Route common tasks to durable skills instead of app-specific logic
3. **Living-Document UX Hardening** - Keep human interaction durable, composable, and artifact-first
4. **Typed Protocol Adoption** - Remove remaining deterministic workflow gates where model-managed planning should lead
5. **Hypervisor** - Multi-tenant sandbox orchestration

### Architecture Policy Reminders

- **Model-Led Control Flow**: Multi-step orchestration is model-managed by default; deterministic logic is reserved for safety and operability rails
- **Authoritative Terminology**: `Logging` = event capture/persistence/transport; `Watcher` = optional recurring-event detection actor (not run-step authority); `Summarizer` = human-readable compression over event batches

See `docs/ARCHITECTURE_SPECIFICATION.md` for full specification.

## License

MIT
