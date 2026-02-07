# ChoirOS - The Automatic Computer

**ChoirOS** is the operating system for the **Agent Choir** - a multi-agent system where autonomous agents collaborate in harmony. Each user gets an isolated sandbox where agents (actors) manage state, execute tools, and compose solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language. The Agent Choir sings in the automatic computer.*

## Current Status (2026-02-01)

**✅ Working:**
- **Agent Choir** - Multi-agent system with ractor actors
- EventStoreActor with libsql/SQLite backend
- ChatActor with message persistence
- HTTP API for agent communication
- All tests passing
- Server running on localhost:8080

**🚧 In Progress:**
- Agent tool calling system (bash, file ops, code execution)
- LLM integration with BAML for agent reasoning
- WebSocket support for real-time agent updates
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
   ┌─────────────────┐    ┌──────────────────┐    ┌─────────────┐
   │  Chat Actor     │    │  Tool Actor      │    │  Code Actor │
   │  (Conversation) │    │  (Bash, Files)   │    │  (Execute)  │
   └────────┬────────┘    └────────┬─────────┘    └──────┬──────┘
            │                      │                      │
            └──────────────────────┼──────────────────────┘
                                   │
                          ┌────────┴────────┐
                          │  EventStore     │
                          │  (Source of     │
                          │   Truth)        │
                          └────────┬────────┘
                                   │
                          ┌────────┴────────┐
                          │     SQLite      │
                          │   (libsql)      │
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
│   │   ├── actors/         # EventStore, Chat actors
│   │   ├── api/            # HTTP handlers
│   │   └── actor_manager.rs
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
