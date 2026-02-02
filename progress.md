# ChoirOS Progress - 2026-02-01

## Summary

**Major Day: Docs Cleanup, Coherence Fixes, Automatic Computer Architecture Defined** - Archived 9 outdated docs, fixed 18 coherence issues across core documents, created lean architecture doc, and handed off to event bus implementation. 27 commits today.

## Today's Commits (27 total)

**Recent (Last 3 hours):**
- `2472392` - handoff to event bus implementation
- `2472392` - docs: major cleanup, coherence fixes, and automatic computer architecture
- `9fe306c` - docs: add multi-agent vision and upgrade notes
- `471732b` - actorcode dashboard progress
- `473ca07` - actorcode progress

**Earlier Today:**
- `2084209` - feat: Chat App Core Functionality
- `bd9330f` - feat: add actorcode orchestration suite
- Plus 21 more: research system, clippy fixes, OpenCode Kimi provider fix, handoff docs, etc.

## What Was Accomplished Today

### ✅ Docs Cleanup (9 docs archived/deleted)
**Archived:**
- DEPLOYMENT_REVIEW_2026-01-31.md
- DEPLOYMENT_STRATEGIES.md  
- actorcode_architecture.md

**Deleted:**
- AUTOMATED_WORKFLOW.md
- DESKTOP_API_BUG.md
- PHASE5_MARKMARKDOWN_TESTS.md
- choirOS_AUTH_ANALYSIS_PROMPT.md
- feature-markdown-chat-logs.md
- research-opencode-codepaths.md

### ✅ Coherence Fixes (18 issues resolved)
**Critical fixes:**
- Removed Sprites.dev references (never implemented)
- Fixed actor list (removed WriterActor/BamlActor/ToolExecutor, added ChatAgent)
- Marked hypervisor as stub implementation
- Fixed test counts (18 → 171+)
- Updated dev-browser → agent-browser
- Marked Docker as pending NixOS research
- Marked CI/CD as planned (not implemented)
- Fixed port numbers (:5173 → :3000)
- Fixed database tech (SQLite → libSQL)
- Fixed API contracts
- Fixed BAML paths (sandbox/baml/ → baml_src/)
- Added handoffs to doc taxonomy
- Clarified actorcode dashboard separation
- Marked vision actors as planned
- Added OpenProse disclaimer
- Documented missing dependencies
- Fixed E2E test paths
- Rewrote AGENTS.md with task concurrency rules

### ✅ New Documentation
- `AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Lean architecture doc (contrast with OpenAI's blocking UX)
- `dev-blog/2026-02-01-why-agents-need-actors.md` - Actor model argument
- `handoffs/2026-02-01-docs-upgrade-runbook.md` - 19 actionable fixes
- `handoffs/2026-02-01-event-bus-implementation.md` - Ready for next session

### ✅ New Skills & Tools
- **system-monitor** - ASCII actor network visualization
- **actorcode dashboard** - Multi-view web dashboard (list/network/timeline/hierarchy)
- **Streaming LLM summaries** - Real-time summary generation
- **NixOS research supervisor** - 5 workers + merge/critique/report pipeline
- **Docs upgrade supervisor** - 18 parallel workers for coherence fixes

### ✅ NixOS Research Complete
- 3/5 initial workers succeeded
- Merge → Web Critique → Final Report all completed
- Comprehensive research docs in `docs/research/nixos-research-2026-02-01/`

## What's Working

### Backend (sandbox) ✅
- **Server:** Running on localhost:8080
- **Database:** libsql/SQLite with event sourcing
- **Actors:** EventStoreActor, ChatActor, DesktopActor, ActorManager, ChatAgent
- **API Endpoints:** Health, chat, desktop, websocket
- **WebSocket:** Connection works and stays alive
- **Chat processing:** Messages reach ChatAgent and AI responses return

### Frontend (sandbox-ui) ✅
- **Framework:** Dioxus 0.7 (WASM)
- **Desktop UI:** dock, floating windows, prompt bar
- **Chat UI:** modern bubbles, typing indicator, input affordances
- **Icon open:** chat opens from desktop icon (double-click)
- **WebSocket status:** shows connected

### Actorcode Orchestration ✅
- **Research system:** Non-blocking task launcher with findings database
- **Dashboard:** Multi-view with streaming summaries
- **Supervisors:** Can spawn parallel workers (docs upgrade: 18 workers)
- **Artifact persistence:** Workers write to JSONL logs

## Current Status

### ✅ Completed Today
- Major docs cleanup (9 docs archived/deleted)
- 18 coherence fixes across core documents
- Automatic computer architecture defined
- NixOS research completed
- System monitor skill
- Multi-view dashboard with streaming
- Task concurrency rules documented

### 📋 Next Steps (from handoff)
1. **Event Bus Implementation** - Build pub/sub system for async workers
2. **Worker Integration** - Make workers emit events
3. **Dashboard WebSocket** - Real-time event streaming
4. **Prompt Bar** - Shell-like interface for spawning workers

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     USER INTERFACE                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Prompt Bar  │  │  App Windows│  │   Dashboard         │  │
│  │ (shell)     │  │  (tmux)     │  │   (observability)   │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
└─────────┼────────────────┼────────────────────┼─────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    EVENT BUS (Pub/Sub) ←── Next Session      │
└─────────────────────────────────────────────────────────────┘
```

## Key Insight: Anti-Chatbot

**OpenAI Data Agent:** "Worked for 6m 1s" → user blocked, staring at spinner
**ChoirOS Automatic Computer:** User spawns worker, continues working, observes via dashboard

The difference: Infrastructure vs Participant. We build the former.

## Documentation

**Authoritative:**
- `README.md` - Quick start
- `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Core architecture
- `docs/ARCHITECTURE_SPECIFICATION.md` - Detailed spec (now coherent)
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop design
- `AGENTS.md` - Development guide with concurrency rules

**Handoffs:**
- `docs/handoffs/2026-02-01-event-bus-implementation.md` - Ready to implement

**Research:**
- `docs/research/nixos-research-2026-02-01/` - NixOS deployment research

---

*Last updated: 2026-02-01 19:35*  
*Status: Major docs cleanup complete, architecture defined, ready for event bus*  
*Commits today: 27*
