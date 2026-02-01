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
 - Actorcode AX contract + verification lattice (coherence/repo-truth/world-truth)
 - Dashboard UX plan: whole-log + summary views for runs

### 📋 Next Steps
1. **Fix actorcode observability** - add whole-log + summary views in web dashboard
2. **Background run contract** - background runs must emit a Markdown doc (no inline summary)
3. **Actorcode demo** - spawn pico/nano/micro/milli under one supervisor
4. **Add small helper** - optional `just opencode-serve`
5. **Theme work** - resume desktop theming now that chat works

---

## Update: Actorcode Research System - 2026-02-01

### ✅ Research System Complete

**What was built:**
- Non-blocking research task launcher (`just research <template> --monitor`)
- [LEARNING] protocol for incremental findings reporting
- Background monitor collecting findings to JSON database
- `research-status` command showing active/completed tasks
- `findings` CLI for querying statistics and exporting data
- Tmux dashboard (`just research-dashboard`) with live updates
- Web dashboard (`just research-web`) for visual monitoring
- Session cleanup utility (`just research-cleanup`)
- Diagnostic tool (`just research-diagnose`)

**Key Fix:**
- Research-launch.js wasn't passing model to promptAsync - subagents weren't running
- Fixed by adding model specification: `{ providerID, modelID }`

**Verification Results:**
- Cleaned 82 orphaned sessions from registry
- Successfully launched docs-gap research task
- Subagent explored codebase using bash/read tools
- Reported 20 [LEARNING] DOCS findings
- Monitor collected all findings to database
- 58 total findings now in database (57 DOCS + 1 TEST)

**Next Step:**
Use the 20 documentation findings to create missing READMEs and improve docs.

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

## Critical Fix: OpenCode Kimi Provider

**Problem:** Headless API with `kimi-for-coding/k2p5` failed with "Kimi For Coding is currently only available for Coding Agents..."

**Root Cause:** TUI uses `@ai-sdk/anthropic` provider internally, but headless API was configured with `@ai-sdk/openai-compatible`

**Fix:** Changed `opencode.json` provider npm package:
```diff
- "npm": "@ai-sdk/openai-compatible"
+ "npm": "@ai-sdk/anthropic"
```

**Result:** Micro tier (`kimi-for-coding/k2p5`) now works via headless API. Actorcode can spawn agents with all tiers.

See: `docs/research-opencode-codepaths.md` for full investigation details.

## Documentation

- `README.md` - Quick start guide
- `docs/ARCHITECTURE_SPECIFICATION.md` - Full architecture spec
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop-specific design (Phase 1 complete)
- `docs/DEPLOYMENT_STRATEGIES.md` - Current and future deployment options
- `docs/archive/` - Old deployment runbook archived

## Actorcode AX + Observability Notes

- **Producer role is already possible**: a supervisor can prompt another supervisor run to spawn more runs.
- **Observability gap**: latest-only message view is too brittle; need whole-log and summary views in the web dashboard.
- **Background runs**: must output a single Markdown doc (no in-task summary) for archival and review.
- **Doc accuracy as verifier**: internal coherence + repo-truth + external validation.

---

*Last updated: 2026-02-01*
*Status: Chat app functional, actorcode orchestration added*
