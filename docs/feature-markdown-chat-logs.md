# Feature: Markdown Chat Logs for ActorCode Sessions

## Problem Statement

Currently, session logs are stored in a custom format in `logs/actorcode/ses_<id>.log` that is not human-readable. We need:

1. **Full chat message logs** in readable markdown format for each session
2. **Accessible via web dashboard** for easy review
3. **Proper formatting** of messages, tool calls, reasoning, and responses

## Current State

**Existing Log Format:**
```
2026-02-01T17:58:36.365Z [session] spawned title=kimi-paid-working agent=explore model=kimi-for-coding/k2p5 tier=micro
2026-02-01T17:58:36.371Z [session] prompt_async dispatched
2026-02-01T17:58:37 +123ms service=llm providerID=opencode modelID=gpt-5-nano...
```

**Issues:**
- Not human-readable
- Doesn't show actual conversation content
- Hard to debug what happened in a session
- Web dashboard can't display meaningful session history

## Proposed Solution

### 1. Markdown Chat Log Format

Create a markdown file for each session at `logs/actorcode/chat/ses_<id>.md`:

```markdown
# Session: ses_3e5958891ffefImhWWQmfKF9e1
**Title:** OpenCode TUI vs Headless API Codepath Investigation  
**Model:** openai/gpt-5.2-codex (milli)  
**Agent:** explore  
**Started:** 2026-02-01T18:15:10.968Z  
**Status:** BUSY

---

## Message 1: User
**Time:** 2026-02-01T18:15:10.968Z  
**ID:** msg_c1a6a7778001V1c3s0t77mmf7V

Investigate why Kimi For Coding API works via OpenCode TUI but fails via headless API...

## Message 2: Assistant
**Time:** 2026-02-01T18:15:11.245Z  
**ID:** msg_c1a6a7778001V1c3s0t77mmf7V

[STEP-START]

I'll help you investigate the OpenCode TUI vs headless API codepath issue. Let me start by examining the OpenCode SDK and provider implementations.

**Tool: glob** - Searching for provider-related files...

[STEP-FINISH]

## Message 3: Assistant
**Time:** 2026-02-01T18:15:15.102Z  
**ID:** msg_c1a6a7781001aBc3s0t77mmf7W

[REASONING]

Looking at the @ai-sdk/openai-compatible package structure...

---

## Summary

**Total Messages:** 3  
**Last Activity:** 2026-02-01T18:15:15.102Z  
**Findings:** 0 (in progress)
```

### 2. Implementation Plan

#### Phase 1: Message Capture
- [ ] Create `logs/actorcode/chat/` directory
- [ ] Modify `actorcode.js` to write markdown logs alongside regular logs
- [ ] Capture all message parts (text, tool calls, reasoning, errors)
- [ ] Format tool calls with syntax highlighting
- [ ] Handle streaming responses properly

#### Phase 2: Real-time Updates
- [ ] Update markdown file as messages arrive
- [ ] Add file locking to prevent corruption
- [ ] Include timestamps and message IDs
- [ ] Format code blocks and JSON properly

#### Phase 3: Web Dashboard Integration
- [ ] Serve markdown files via findings-server.js
- [ ] Add endpoint: `/api/chat/<session_id>`
- [ ] Render markdown in dashboard.html
- [ ] Add search/filter capabilities
- [ ] Show chat preview in session list

#### Phase 4: Enhanced Features
- [ ] Export chat as PDF or HTML
- [ ] Diff view between sessions
- [ ] Chat replay/simulation
- [ ] Learning tag highlighting
- [ ] Tool call statistics

### 3. Technical Details

**File Structure:**
```
logs/actorcode/
├── ses_<id>.log              # Current machine-readable logs
├── chat/
│   ├── ses_<id>.md          # New human-readable chat logs
│   └── index.json           # Index of all chat logs
└── supervisor.log
```

**Message Types to Capture:**
- `text` - Regular text content
- `tool` - Tool calls with parameters and results
- `thinking` / `reasoning` - Model reasoning/thinking blocks
- `error` - Error messages and stack traces
- `step-start` / `step-finish` - Step boundaries
- `patch` - Code patches

**Markdown Formatting:**
- Use code blocks with language tags for code
- Use blockquotes for reasoning/thinking
- Use tables for tool call parameters
- Use collapsible sections for long content
- Use emojis for message types (optional)

### 4. API Endpoints

```javascript
// findings-server.js additions

// Get chat log as markdown
GET /api/chat/:sessionId

// Get chat log as JSON
GET /api/chat/:sessionId/json

// Get chat preview (first N messages)
GET /api/chat/:sessionId/preview?limit=10

// Search across all chats
GET /api/chat/search?q=error

// Export chat
GET /api/chat/:sessionId/export?format=html|pdf
```

### 5. Dashboard UI

**Session List Enhancement:**
- Show last message preview
- Show message count
- Show model/tier used
- Quick link to full chat

**Chat View:**
- Rendered markdown
- Syntax highlighting
- Collapsible tool calls
- Search within chat
- Export button

### 6. Example Output

See example at: `logs/actorcode/chat/ses_3e5958891ffefImhWWQmfKF9e1.md`

## Benefits

1. **Debugging:** Easy to see what happened in a session
2. **Transparency:** Full conversation history accessible
3. **Analysis:** Can analyze patterns across sessions
4. **Sharing:** Easy to share session logs
5. **Documentation:** Chat logs can be used for docs

## Next Steps

1. Create `lib/chat-logger.js` module
2. Update `actorcode.js` to use chat logger
3. Update `findings-server.js` with chat endpoints
4. Update `dashboard.html` with chat view
5. Test with active sessions

## Related Files

- `skills/actorcode/scripts/lib/logs.js` - Current logging
- `skills/actorcode/scripts/actorcode.js` - Main script
- `skills/actorcode/scripts/findings-server.js` - Web server
- `skills/actorcode/dashboard.html` - Web UI
