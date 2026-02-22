# Handoff: BAML Chat Agent Implementation - Phase 1 Complete

## Session Metadata
- Created: 2026-01-31 22:05:19
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~3 hours

### Recent Commits (for context)
  - 1c56b66 feat: BAML chat agent with tool execution and WebSocket streaming
  - df3845f ui improving. removed odd message
  - 9934d12 ui improving. desktop looking more real
  - 7daf751 ui loading. chat app needs fixing
  - b4e7bf5 update docs

## Handoff Chain

- **Continues from**: [2026-01-31-desktop-foundation-api-fix.md](./2026-01-31-desktop-foundation-api-fix.md)
  - Previous title: Desktop Foundation Complete - API Fix Needed
- **Supersedes**: None

## Current State Summary

Phase 1 of the BAML chat agent implementation is **complete and committed**. The backend infrastructure is ready:

1. **Desktop UI** (committed earlier by user): 
   - Removed left sidebar
   - Added desktop icons grid (Chat, Writer, Terminal, Files)
   - Enhanced prompt bar with running app indicators
   - Responsive layout

2. **BAML Chat Agent Backend** (just committed):
   - BAML configuration for ClaudeBedrock (AWS) and GLM47 (Z.ai)
   - ChatAgent actor with BAML-powered agent harness
   - Tool registry (bash, read_file, write_file, list_files, search_files)
   - WebSocket endpoints for streaming
   - Model switching support

**What's NOT done yet:**
- Chat UI is not connected to the new WebSocket endpoints
- Frontend doesn't display chat messages, thinking states, or tool calls
- No model selector UI dropdown
- Testing end-to-end flow

**Next session should focus on:** Building the chat UI components and connecting them to the backend WebSocket.

## Codebase Understanding

### Architecture Overview

**Actor Model (Backend):**
- `EventStoreActor` - SQLite-backed event sourcing
- `DesktopActor` - Window management, app registry
- `ChatAgent` - NEW: BAML-powered agent with tool execution
- `ChatActor` - OLD: Basic chat (being replaced by ChatAgent)

**BAML Integration:**
- Source files in `baml_src/` (clients.baml, agent.baml, types.baml, etc.)
- Generated client in `sandbox/src/baml_client/`
- Regenerate with: `baml-cli generate`

**Tool System:**
- `sandbox/src/tools/mod.rs` - Tool trait + implementations
- Security: Restricted to project directory (`/Users/wiz/choiros-rs`)

**WebSocket:**
- `sandbox/src/api/websocket_chat.rs` - Chat streaming endpoint
- Endpoint: `/ws/chat/{actor_id}`
- Streams: thinking, tool calls, tool results, responses

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `baml_src/clients.baml` | BAML client configs | Change models, auth |
| `baml_src/agent.baml` | BAML agent functions | Modify agent behavior |
| `sandbox/src/actors/chat_agent.rs` | ChatAgent actor | Core agent logic |
| `sandbox/src/tools/mod.rs` | Tool implementations | Add/modify tools |
| `sandbox/src/api/websocket_chat.rs` | WebSocket handlers | Frontend integration |
| `dioxus-desktop/src/desktop.rs` | Desktop UI | Chat UI needs building |
| `dioxus-desktop/src/components.rs` | UI components | Where ChatView lives |

### Key Patterns Discovered

1. **BAML Auto-Auth**: `aws-bedrock` provider auto-reads `AWS_BEARER_TOKEN_BEDROCK` - no config needed
2. **ClientRegistry Pattern**: Model switching via runtime client override (see choirOS prototype)
3. **Event Sourcing**: All actions log to EventStoreActor for persistence
4. **Tool Security**: Tools validate paths are within project directory
5. **WebSocket Streaming**: Agent streams thinking → tool_use → tool_result → text

## Work Completed

### Tasks Finished

- [x] Install and configure BAML CLI
- [x] Create BAML source files (clients, agent, types)
- [x] Generate Rust BAML client
- [x] Create ChatAgent actor with BAML integration
- [x] Implement tool registry (5 tools)
- [x] Set up WebSocket streaming endpoints
- [x] Integrate with EventStore
- [x] Configure AWS Bedrock bearer token auth
- [x] Configure Z.ai GLM47 provider
- [x] All 22 tests passing
- [x] UI desktop layout (icons, prompt bar) - committed earlier

### Files Modified/Created

| File | Changes | Rationale |
|------|---------|-----------|
| `baml_src/*.baml` | NEW | BAML configuration |
| `sandbox/src/baml_client/` | NEW | Generated Rust client |
| `sandbox/src/actors/chat_agent.rs` | NEW | Agent actor |
| `sandbox/src/tools/mod.rs` | NEW | Tool system |
| `sandbox/src/api/websocket_chat.rs` | NEW | WebSocket handlers |
| `sandbox/Cargo.toml` | Modified | Add baml + websocket deps |
| `sandbox/src/actor_manager.rs` | Modified | Add ChatAgent management |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use BAML CLI generation | Runtime API | Type safety, compile-time validation |
| aws-bedrock provider | openai-generic | Native AWS support, auto bearer token auth |
| WebSocket streaming | SSE/Polling | Real-time tool call feedback essential |
| SQLite EventStore | NATS (from prototype) | Simpler architecture, no external deps |
| 5 initial tools | All tools from prototype | MVP scope - can add more later |

## Pending Work

### Immediate Next Steps

1. **Build Chat UI Components** (CRITICAL)
   - Chat message display component
   - Streaming text/thinking display
   - Tool call visualization (show which tools are being called)
   - Message input area
   - Model selector dropdown (Claude vs GLM)

2. **Connect UI to WebSocket** (CRITICAL)
   - Open WebSocket connection on chat window open
   - Send messages via WebSocket
   - Handle streaming response types (thinking, tool_use, tool_result, text)
   - Display tool execution status

3. **Test End-to-End Flow**
   - Open chat via prompt bar
   - Send message
   - Verify agent planning and tool execution
   - Check response streaming
   - Test model switching

### Blockers/Open Questions

- [ ] Database path issue: `/opt/choiros/data/events.db` requires directory creation (use `DATABASE_URL` env var to override)
- [ ] Chat UI location: Need to build it in `dioxus-desktop/src/` - where exactly?
- [ ] How to display tool calls in UI? Progress indicators? Collapsible sections?

### Deferred Items

- File uploads (multimodal) - text only for now
- Controlling other apps (Writer, Terminal) - apps not built yet
- Advanced tool: web_search (needs Tavily API key in .env)
- File edit tool (complexity)
- Git operations

## Context for Resuming Agent

### Important Context

**1. AWS Authentication:**
- Uses `AWS_BEARER_TOKEN_BEDROCK` environment variable
- BAML's `aws-bedrock` provider auto-detects this - NO explicit config in BAML files
- Model ID: `us.anthropic.claude-opus-4-5-20251101-v1:0` (note the `us.` prefix)

**2. Frontend/Backend Not Connected Yet:**
- Backend has WebSocket ready at `/ws/chat/{actor_id}`
- Frontend has Chat window that opens but uses OLD ChatActor API
- Need to migrate frontend to use NEW WebSocket streaming

**3. Tool Execution Flow:**
```
User message → BAML PlanAction → Returns AgentPlan
    → Stream "thinking" to UI
    → If tool_calls: execute each tool
        → Stream "tool_use" (name, args) to UI
        → Execute tool
        → Stream "tool_result" (success/failure, output) to UI
    → BAML SynthesizeResponse → Stream "text" (final response) to UI
```

**4. Environment Variables Required:**
- `AWS_BEARER_TOKEN_BEDROCK` - For Claude via Bedrock
- `ZAI_API_KEY` - For GLM47
- `DATABASE_URL` - Optional: override default `/opt/choiros/data/events.db`

### Assumptions Made

- User will provide AWS_BEARER_TOKEN_BEDROCK in .env file
- Chat UI should show tool calls in real-time (not just final result)
- Model switching happens via UI dropdown (not automatic routing yet)
- File uploads deferred to later phase

### Potential Gotchas

- **BAML regeneration**: If you modify `baml_src/*.baml`, run `baml-cli generate`
- **Database directory**: Backend needs `/opt/choiros/data/` or use `DATABASE_URL` env var
- **WebSocket URL**: Frontend needs to connect to `ws://localhost:8080/ws/chat/{actor_id}`
- **Model IDs**: ClaudeBedrock uses `us.anthropic.claude-opus-4-5-20251101-v1:0` with `us.` prefix
- **Tool security**: Tools only work within `/Users/wiz/choiros-rs` directory

## Environment State

### Tools/Services Used

- BAML CLI (cargo install baml-cli)
- AWS Bedrock API via bearer token
- Z.ai API for GLM47
- SQLite (libsql) for EventStore
- Actix-web for backend
- Dioxus for frontend

### Active Processes

- None killed before handoff

### Environment Variables (Names Only)

- `AWS_BEARER_TOKEN_BEDROCK`
- `ZAI_API_KEY`
- `DATABASE_URL` (optional)
- `TAVILY_API_KEY` (for web search - deferred)

## Related Resources

- ChoirOS prototype reference: `~/choirOS/supervisor/agent/harness.py`
- BAML docs: https://docs.boundaryml.com
- AWS Bedrock API keys: https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys.html
- WebSocket protocol defined in: `sandbox/src/api/websocket_chat.rs` (ServerMessage enum)

---

**Security Reminder**: No secrets in this handoff. All API keys are in `.env` file (not committed).

**DevX Note**: User wants to improve multi-terminal control for agents. Current limitation: subagents can't share terminal state. Possible solutions: tmux control, named pipe communication, or file-based coordination.
