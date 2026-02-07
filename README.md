# ChoirOS - The Automatic Computer

**ChoirOS** is the operating system for the **Agent Choir** - a multi-agent system where autonomous agents collaborate in harmony. Each user gets an isolated sandbox where agents (actors) manage state, execute tools, and compose solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language. The Agent Choir sings in the automatic computer.*

## Current Status (2026-02-07)

**✅ Working:**
- Supervision-tree runtime (`ApplicationSupervisor -> SessionSupervisor -> chat/desktop/terminal`)
- EventStoreActor + EventBus-backed worker lifecycle tracing
- ChatAgent tool routing with delegated `bash` execution through TerminalActor
- WebSocket chat streaming for `tool_call`, `tool_result`, and `actor_call` updates
- Scope-aware chat isolation (`session_id` + `thread_id`) across shared actor IDs
- Headless integration tests for `/chat/*` and `/ws/chat/*` paths
- Server running on localhost:8080

**🚧 In Progress:**
- Typed worker-event schema hardening for multi-agent observability
- Watcher/supervisor escalation loops (timeouts, retries, failure signals)
- Richer UI grouping for actor-call timelines (clean-by-default, deep-inspect on demand)
- Hypervisor routing for multi-user sandboxes

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
curl -X POST http://localhost:8080/chat/send \
  -H "Content-Type: application/json" \
  -d '{"actor_id":"test","user_id":"me","text":"hello"}'
curl http://localhost:8080/chat/test/messages
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
   ┌─────▼───┐ ┌──▼──────────┐
   │ChatAgent│ │TerminalActor │
   └────┬────┘ └──────┬───────┘
        │              │
        └──────┬───────┘
               │
       ┌───────▼────────┐
       │EventBus + Store │
       │(worker/tool/chat│
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
│   │   ├── actors/         # ChatAgent, TerminalActor, EventStore/EventBus, desktop/chat
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
- `POST /chat/send` - Send chat message
- `GET /chat/{actor_id}/messages` - Get chat history
- `GET /ws/chat/{actor_id}` - Chat websocket stream (thinking/tool/actor updates)
- `GET /ws/chat/{actor_id}/{user_id}` - Chat websocket stream with path user
- `GET /ws/terminal/{terminal_id}` - Terminal websocket stream

## Testing Notes

- Core integration:
  - `cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture`
  - `cargo test -p sandbox --test websocket_chat_test -- --nocapture`
- Use provider-agnostic prompts/commands in tests; avoid coupling to one external API.

## The Vision

**ChoirOS** is the operating system for the **Agent Choir** - where autonomous agents collaborate in harmony to build, execute, and evolve software. Each sandbox is a stage where agents perform:

- **Chat Agents** handle conversation and reasoning
- **Tool Agents** execute bash commands and file operations
- **Code Agents** write, test, and deploy code
- **Meta Agents** orchestrate the choir

The Agent Choir sings in the automatic computer. Agency lives in computation.

## Next Steps

1. **Agent Tools** - Implement bash, file, and code execution tools
2. **LLM Integration** - Connect BAML for agent reasoning and planning
3. **Agent Registry** - Dynamic agent discovery and composition
4. **WebSocket Events** - Real-time agent communication
5. **Hypervisor** - Multi-tenant sandbox orchestration

See `docs/ARCHITECTURE_SPECIFICATION.md` for full specification.

## License

MIT
