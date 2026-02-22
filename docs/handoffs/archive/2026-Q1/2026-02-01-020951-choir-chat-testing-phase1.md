# Handoff: ChoirOS Chat App Testing Initiative - Phase 1 Complete (Tests Pass, App Broken)

## Session Metadata
- Created: 2026-02-01 02:09:51
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~4 hours (83 tool calls)
- **CRITICAL FINDING**: 205 tests pass (96.1%) but chat app does NOT actually work

### Recent Commits (for context)
  - ca181cf gitignore skills junk
  - 7fd5628 dev-browser and multi-terminal skills enabled
  - 11d2ed9 deleted 2 old handoffs. some are archived. all will be deleted soon
  - 0c83c71 refactor: remove duplicate scripts from docs/handoffs/
  - 5fd44c2 refactor: move handoffs to docs/handoffs/ and add local session-handoff skill

## Handoff Chain

- **Continues from**: [2026-01-31-220519-baml-chat-agent-implementation.md](./2026-01-31-220519-baml-chat-agent-implementation.md)
  - Previous title: BAML Chat Agent Implementation - Phase 1 Complete
- **Supersedes**: None

> Review the previous handoff for full context before filling this.

## Current State Summary

**The Paradox**: We completed 5 comprehensive testing phases with 205 tests achieving 96.1% pass rate. However, the actual chat application does not work. Screenshots from E2E tests show:

1. **Empty desktop** with "Connecting..." in corner (WebSocket not connecting)
2. **Chat window opens** via prompt bar (not by clicking Chat icon)
3. **Messages stuck at "Sending..."** - never processed
4. **No AI responses** - backend integration broken

**The Lesson**: Unit/integration tests validated components in isolation, but end-to-end integration is broken. We need better testing that actually exercises the real user flow.

## Codebase Understanding

### Architecture Overview

**Backend (sandbox/)**:
- WebSocket at `ws://localhost:8080/ws/chat/{actor_id}`
- ChatAgent with BAML LLM integration
- ToolRegistry (bash, read_file, write_file, list_files, search_files)
- EventStoreActor with SQLite persistence

**Frontend (dioxus-desktop/)**:
- Dioxus-based web UI
- Desktop environment with app icons
- Chat window component
- Prompt bar at bottom for quick access

**Skills Available**:
- `skills/dev-browser/` - Browser automation with Playwright
- `skills/multi-terminal/` - Terminal session management
- `skills/session-handoff/` - Context preservation (this doc)

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `sandbox/src/api/websocket_chat.rs` | WebSocket handler | Core chat protocol - tests pass but real connection fails |
| `sandbox/src/actors/chat_agent.rs` | BAML agent | Processes messages - works in tests, not in UI |
| `dioxus-desktop/src/main.rs` | Frontend entry | Chat UI - opens from prompt but not from icon click |
| `sandbox/src/tools/mod.rs` | Tool registry | All tools work in tests |
| `tests/websocket_chat_test.rs` | WS tests | 17 passing - protocol layer works |
| `tests/e2e/test_e2e_basic_chat_flow.ts` | E2E tests | Created but reveals actual breakage |
| `tests/e2e/screenshots/phase4/*.png` | Screenshots | Shows "Connecting..." and "Sending..." stuck states |

### Key Patterns Discovered

1. **AGENTS.md has just commands** - But subagents didn't use them despite being documented
2. **Sequential subagents** - Context engineering hack, not true parallel agents
3. **Test isolation vs integration** - Unit tests pass, E2E reveals failures
4. **Actor model potential** - ChoirOS is built for concurrent agents, but we're not using it for testing yet

## Work Completed

### Tasks Finished

- [x] **Phase 1**: WebSocket streaming tests (17 tests, 100% pass)
  - Connection, ping/pong, model switching, concurrent users
- [x] **Phase 2**: Tool call unit tests (41 tests, 100% pass)  
  - All 5 tools tested with security boundaries
- [x] **Phase 3**: Persistence tests (40 tests, 100% pass)
  - Event sourcing, conversation history, recovery
- [x] **Phase 4**: Integration E2E tests (54 tests created)
  - Browser automation scripts, 2 screenshots captured
- [x] **Phase 5**: Markdown rendering tests (53 tests, 100% pass)
  - New markdown module with XSS protection
- [x] **Test report**: HTML report created at `test_report.html`

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `tests/websocket_chat_test.rs` | Created | WebSocket protocol validation |
| `tests/tools_integration_test.rs` | Created | Tool execution tests |
| `tests/persistence_test.rs` | Created | Event sourcing tests |
| `tests/markdown_test.rs` | Created | Markdown parsing tests |
| `sandbox/src/markdown.rs` | Created | New markdown module with pulldown-cmark |
| `tests/e2e/*.ts` | Created | 6 TypeScript E2E test files |
| `tests/integration_chat_e2e.rs` | Created | Rust E2E orchestration |
| `run-phase4-e2e.sh` | Created | One-command E2E runner |
| `test_report.html` | Created | HTML test report |
| `sandbox/Cargo.toml` | Added deps | pulldown-cmark, regex for markdown |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Test approach | Mock vs Real | Used real WebSocket in tests, but UI integration still broken |
| E2E framework | Playwright vs Selenium | Playwright via dev-browser skill |
| Markdown lib | pulldown-cmark vs custom | pulldown-cmark for CommonMark compliance |
| Test isolation | In-memory vs temp files | In-memory SQLite for speed |

## Pending Work

### Immediate Next Steps (Priority Order)

1. **🔴 CRITICAL: Fix WebSocket Connection**
   - WebSocket shows "Connecting..." indefinitely
   - Backend health check works (port 8080 responds)
   - Frontend shows "Sending..." stuck
   - **Action**: Debug WebSocket handshake, verify ws:// vs http://, check CORS

2. **🔴 CRITICAL: Fix Chat Icon Click Handler**
   - Chat icon on desktop doesn't open chat window
   - Only prompt bar brings up chat
   - **Action**: Check desktop app click handler, verify window manager integration

3. **🟡 Fix Message Processing**
   - Messages get stuck at "Sending..."
   - ChatAgent works in tests but not via WebSocket
   - **Action**: Debug message flow from UI → WebSocket → ChatAgent → BAML → Response

4. **🟡 Implement ChatAgent Event Replay**
   - ChatAgent doesn't recover conversation history on restart
   - EventStore has data but ChatAgent doesn't read it
   - **Action**: Add EventStore sync on ChatAgent startup

5. **🟢 Improve Test Infrastructure**
   - Subagents didn't use just commands from AGENTS.md
   - Need better orchestration for E2E tests
   - **Action**: Create better test runner scripts, improve subagent instructions

### Blockers/Open Questions

- [ ] Why does WebSocket stay in "Connecting..." state?
- [ ] Why doesn't Chat icon click open the chat window?
- [ ] Why do messages get stuck at "Sending..."?
- [ ] How should we orchestrate true parallel testing (Actor model)?
- [ ] Should we mock LLM calls for faster E2E tests?

### Deferred Items

- [ ] Add relative path traversal protection (security hardening)
- [ ] BashTool async runtime fix (low priority - works in prod)
- [ ] Full E2E screenshot suite (need working app first)
- [ ] CI/CD integration for tests

## Context for Resuming Agent

### Important Context (MUST READ)

1. **Tests Pass ≠ App Works**: We have 205 passing tests but the chat app is broken. The tests validated components in isolation. The integration is what's failing.

2. **Screenshot Evidence**: 
   - `tests/e2e/screenshots/phase4/test_e2e_basic_chat_flow_step1_initial_load.png` - Shows "Connecting..." in corner
   - `tests/e2e/screenshots/phase4/test_e2e_basic_chat_flow_step99_error_state.png` - Shows message stuck at "Sending..."

3. **How to Run Current State**:
   ```bash
   # Terminal 1: Backend
   just dev-sandbox  # or: cargo run -p sandbox
   
   # Terminal 2: Frontend  
   just dev-ui  # or: cd dioxus-desktop && cargo run
   
   # Terminal 3: Browser automation
   ./skills/dev-browser/server.sh
   
   # Check health
   curl http://localhost:8080/api/health  # Should return {"status":"healthy"}
   ```

4. **Test Commands**:
   ```bash
   # Unit tests (all pass)
   cargo test -p sandbox --test websocket_chat_test
   cargo test -p sandbox --test tools_integration_test
   cargo test -p sandbox --test persistence_test
   cargo test -p sandbox --test markdown_test
   
   # E2E tests (need working servers)
   ./run-phase4-e2e.sh
   ```

5. **AGENTS.md Has Just Commands**: The file contains all the standard commands (just dev-sandbox, just dev-ui, etc.) but subagents didn't use them. They were in AGENTS.md but subagents either didn't read it or didn't follow it.

### Assumptions Made

- BAML integration is configured correctly (can't verify without working chat)
- SQLite database is created and working (EventStore tests pass)
- Frontend can reach backend (both on localhost)
- Browser automation server works (dev-browser skill available)

### Potential Gotchas

1. **Subagent Behavior**: Subagents may not follow AGENTS.md. Be explicit in task prompts about using `just` commands.

2. **WebSocket URL**: Check if frontend uses correct WebSocket URL format. Could be ws:// vs wss:// issue.

3. **Actor Lifecycle**: ChatAgent may not be getting created properly via WebSocket connection.

4. **Async Runtime**: BashTool and SearchFilesTool have block_on() issues in async tests but work in sync production code.

5. **Tmux Sessions**: E2E tests use tmux. Check if sessions from previous runs are still active.

## Environment State

### Tools/Services Used

- **Backend**: Actix Web on port 8080
- **Frontend**: Dioxus on port 3000
- **Browser Automation**: Playwright/Chromium via dev-browser skill on port 9222
- **Testing**: Cargo test, TypeScript/Node for E2E

### Active Processes (from E2E test run)

- Tmux session: `choiros-e2e`
  - Window 1: backend (cargo run -p sandbox)
  - Window 2: frontend (dx serve --port 3000)
  - Window 3: browser (dev-browser server)

### Environment Variables (Names Only)

- ZAI_API_KEY (for GLM47 LLM client)
- AWS credentials (for ClaudeBedrock)
- OPENPROSE_TELEMETRY, USER_ID, SESSION_ID (OpenProse)

## Related Resources

- `test_report.html` - HTML test report (open in browser)
- `CHOIROS_CHAT_TEST_REPORT.md` - Markdown version of report
- `tests/PHASE4_E2E_REPORT.md` - E2E implementation details
- `AGENTS.md` - Development commands and guidelines
- `Justfile` - Task runner commands

## Key Learnings for Next Session

### What Went Wrong
1. **Component tests ≠ Integration tests**: We validated pieces but not the whole
2. **E2E tests caught real issues**: Screenshots revealed broken WebSocket and UI
3. **Subagent limitations**: Didn't use AGENTS.md just commands
4. **Sequential vs Parallel**: Current approach is hacky; Actor model would be better

### What We Learned
1. **Test-first is valuable**: Found security issues, architecture gaps
2. **Screenshots are truth**: They show actual state vs test assertions
3. **WebSocket complexity**: Protocol layer works but integration doesn't
4. **UI/Backend gap**: Frontend and backend exist but don't communicate properly

### Next Session Strategy
1. **Start with debugging**: Why is WebSocket "Connecting..." stuck?
2. **Fix one thing at a time**: Get WebSocket working first
3. **Use screenshots as guide**: Target specific broken states
4. **Better test orchestration**: Ensure subagents use AGENTS.md
5. **Maybe use Actor model**: For parallel testing once we have working base

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
