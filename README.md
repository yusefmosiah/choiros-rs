# ChoirOS - The Automatic Computer

A self-modifying, multi-tenant system where users prompt the computer to build new programs. Each user gets an isolated sandbox with actors managing state in SQLite, and a Dioxus frontend.

## Current Status (2026-01-31)

**✅ Working:**
- Actor system with Actix
- EventStoreActor with libsql/SQLite backend  
- ChatActor with message persistence
- HTTP API with multiturn chat
- All 11 tests passing
- Server running on localhost:8080

**🚧 Not Yet Implemented:**
- Dioxus frontend UI (placeholder only)
- LLM integration (BAML in deps but unused)
- Tool calling system
- WebSocket support
- Hypervisor routing

## Quick Start

```bash
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

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────┐
│   Dioxus UI     │────▶│   Actix Server   │────▶│   SQLite    │
│  (WASM - WIP)   │     │  (Port 8080)     │     │   (libsql)  │
└─────────────────┘     └──────────────────┘     └─────────────┘
                              │
                    ┌─────────┴─────────┐
                    │    Actor System   │
                    │  • EventStore     │
                    │  • ChatActor      │
                    └───────────────────┘
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Frontend | Dioxus 0.7 (WASM) |
| Backend | Actix Web + Actix Actors |
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
├── sandbox-ui/             # Dioxus frontend (WIP)
├── hypervisor/             # Edge router (WIP)
├── shared-types/           # Shared types between FE/BE
└── docs/
    ├── ARCHITECTURE_SPECIFICATION.md  # Full architecture spec
    └── archive/            # Old docs
```

## Key Design Principles

1. **Actor-owned state** - All state lives in SQLite, actors query their own state
2. **Event sourcing** - All changes logged to events table (seq, event_type, payload)
3. **UI is a projection** - UI components read from actors, never own state
4. **Optimistic updates** - UI updates immediately, confirms async with actor

## API Endpoints

- `GET /health` - Health check
- `POST /chat/send` - Send chat message
- `GET /chat/{actor_id}/messages` - Get chat history

## Next Steps

1. Build Dioxus chat UI
2. Add LLM integration with BAML
3. Implement tool calling (bash, file ops)
4. Add WebSocket support
5. Build hypervisor for multi-user routing

See `docs/ARCHITECTURE_SPECIFICATION.md` for full specification.

## License

MIT
