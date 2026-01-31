# ChoirOS Progress - 2026-01-31

## Summary

**Desktop Foundation Complete** - Built new Dioxus desktop with floating windows, app dock, and prompt bar. All 38 tests passing. Backend API needs fix for empty responses before themes can be applied.

## What's Working

### Backend (sandbox) ✅
- **Server:** Running on localhost:8080
- **Database:** libsql/SQLite with event sourcing
- **Actors:** EventStoreActor, ChatActor, DesktopActor, ActorManager
- **API Endpoints:** All endpoints implemented (health, chat, desktop, websocket)
- **WebSocket:** Infrastructure for real-time updates at `/ws`
- **CORS:** Allow-list enforced for known UI origins
- **Tests:** All 38 tests passing (18 unit + 20 integration)

### Frontend (sandbox-ui) ✅
- **Framework:** Dioxus 0.7 (WASM) - compiles successfully
- **New Components:**
  - **Desktop** - Main container with CSS token system for themes
  - **AppDock** - Left sidebar with app icons and labels
  - **FloatingWindow** - Draggable, resizable windows with z-index
  - **PromptBar** - Bottom command input with WebSocket status
  - **Interop** - WASM bindings for drag/resize/WebSocket
- **Features:**
  - Responsive layout (desktop >1024px vs mobile)
  - Theme-ready architecture with CSS variables
  - Window management (open, close, focus, z-index)
  - WebSocket client for real-time sync
  - Dark default theme

## Current Status

### ✅ Completed
- Desktop foundation with app dock, prompt bar, floating windows
- CSS token system for theme abstraction
- WebSocket API infrastructure
- WASM interop for drag/resize
- All 38 tests passing
- Old React/Vite prototype tests removed
- Deployment runbook archived

### ⚠️ In Progress
- **Backend API returning empty responses** - `/desktop/{id}` returns empty
- Need to investigate why DesktopActor state isn't being serialized properly
- Frontend shows "Error loading desktop: Failed to parse JSON"

### 📋 Next Steps
1. **Fix backend API** - Debug why desktop state returns empty
2. **Theme Subagents** - Once API works, create tasks for each theme:
   - Neo-Aero / Frutiger-style gloss
   - Glassmorphism / Translucent layers
   - Neo-Brutalism / Soft Brutalism
   - Retrofuturism
   - And 10+ more themes from design doc
3. **Deployment** - Deploy working version to EC2

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

cargo run -p sandbox
# Server starts on http://localhost:8080
```

### Run the Frontend (Development)
```bash
# Install Dioxus CLI (one time)
cargo install dioxus-cli

# Start dev server
cd sandbox-ui
dx serve --port 3000
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

1. `e649f2b` - feat: migrate from sqlx to libsql
2. `361fd86` - docs: cleanup and solidify documentation
3. `77bfc81` - feat: implement Dioxus chat UI with full end-to-end testing
4. `8e4efc5` - feat: implement DesktopActor with window management and app registry
5. `9230716` - feat: implement mobile-first Desktop UI with window system
6. `5dde681` - feat: desktop foundation with floating windows, dock, prompt bar
7. `7937a4b` - fix: resolve compilation errors and test desktop foundation

## Documentation

- `README.md` - Quick start guide
- `docs/ARCHITECTURE_SPECIFICATION.md` - Full architecture spec
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop-specific design (Phase 1 complete)
- `docs/DEPLOYMENT_STRATEGIES.md` - Current and future deployment options
- `docs/archive/` - Old deployment runbook archived

---

*Last updated: 2026-01-31*
*Status: Desktop foundation complete, API fix needed before themes*
