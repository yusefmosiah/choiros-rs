# ChoirOS Progress - 2026-02-01

## Summary

**Chat App Functional + Actorcode Orchestration Added** - Chat UI and backend flow now work end-to-end (WebSocket, icon open, message processing). Added actorcode skill suite to orchestrate OpenCode sessions via HTTP SDK with logs and model tiers.

## What's Working

### Backend (sandbox) ✅
- **Server:** Running on localhost:8080
- **Database:** libsql/SQLite with event sourcing
- **Actors:** EventStoreActor, ChatActor, DesktopActor, ActorManager
- **API Endpoints:** Health, chat, desktop, websocket
- **WebSocket:** Connection works and stays alive
- **Chat processing:** Messages reach ChatAgent and AI responses return

### Frontend (sandbox-ui) ✅
- **Framework:** Dioxus 0.7 (WASM)
- **Desktop UI:** dock, floating windows, prompt bar
- **Chat UI:** modern bubbles, typing indicator, input affordances
- **Icon open:** chat opens from desktop icon (double-click)
- **WebSocket status:** shows connected

## Current Status

### ✅ Completed
- Chat app end-to-end functionality (WebSocket, icon open, message flow)
- Chat UI polish with modern bubbles
- Actorcode orchestration skill suite
- Consolidated actorcode architecture doc

### ⚠️ In Progress
- Actorcode demo run (spawn one agent per model tier)
- Observability checks for actorcode logs and events

### 📋 Next Steps
1. **Actorcode demo** - spawn pico/nano/micro/milli under one supervisor
2. **Add small helper** - optional `just opencode-serve`
3. **Theme work** - resume desktop theming now that chat works

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────┐
│  Dioxus Desktop │────▶│   Actix Server   │────▶│   SQLite    │
│  (WASM:3000)    │◄────│   (localhost:8080)│     │   (libsql)  │
│                 │ WS  │                   │     │             │
│  ┌───────────┐  │     │  ┌─────────────┐  │     │             │
│  │ App Dock  │  │     │  │DesktopActor │  │     │             │
│  │ (left)    │  │     │  │  (state)    │  │     │             │
│  └───────────┘  │     │  └─────────────┘  │     │             │
│  ┌───────────┐  │     │        │          │     │             │
│  │ Windows   │  │     │  ┌─────┴─────┐    │     │             │
│  │ (floating)│  │     │  │EventStore │    │     │             │
│  └───────────┘  │     │  └───────────┘    │     │             │
│  ┌───────────┐  │     │                   │     │             │
│  │Prompt Bar │  │     └───────────────────┘     └─────────────┘
│  │ (bottom)  │  │
│  └───────────┘  │
└─────────────────┘
```

## Quick Start

### Run the Backend
```bash
# Set local database path (required for local development)
export DATABASE_URL="./data/events.db"

just dev-sandbox
# Server starts on http://localhost:8080
```

### Run the Frontend (Development)
```bash
# Install Dioxus CLI (one time)
cargo install dioxus-cli

# Start dev server
just dev-ui
# UI available at http://localhost:3000
```

### Production vs Local
- **Local Development**: Requires `export DATABASE_URL="./data/events.db"`
- **Production Server**: Uses hardcoded `/opt/choiros/data/events.db` (no export needed)

### Test Everything
```bash
# Backend health
curl http://localhost:8080/health

# Run all tests
cargo test -p sandbox

# Build UI
cargo build -p sandbox-ui --target wasm32-unknown-unknown
```

## Commits

1. `2084209` - feat: Chat App Core Functionality - WebSocket, Icon Click, Message Flow
2. `bd9330f` - feat: add actorcode orchestration suite

## Documentation

- `README.md` - Quick start guide
- `docs/ARCHITECTURE_SPECIFICATION.md` - Full architecture spec
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop-specific design (Phase 1 complete)
- `docs/DEPLOYMENT_STRATEGIES.md` - Current and future deployment options
- `docs/archive/` - Old deployment runbook archived

---

*Last updated: 2026-02-01*
*Status: Chat app functional, actorcode orchestration added*
