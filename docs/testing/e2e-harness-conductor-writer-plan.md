# E2E Testing Plan: Harness + Conductor + Writer

## Test Suite 1: Simplified Harness (Ready Now)

### Test 1.1: Researcher File Tools
```bash
# Submit research task
curl -X POST http://localhost:8080/api/researcher/search \
  -H "Content-Type: application/json" \
  -d '{"query": "Rust async runtime comparison", "max_rounds": 3}'

# Verify:
# - Report file created in sandbox/reports/
# - Document updates streamed via websocket
# - Final report is freeform markdown (not structured sections)
```

### Test 1.2: Terminal Bash Execution
```bash
# Submit terminal task
curl -X POST http://localhost:8080/api/terminal/run \
  -H "Content-Type: application/json" \
  -d '{"objective": "List files in current directory"}'

# Verify:
# - Command executed
# - Output returned
# - Tool execution events emitted
```

### Test 1.3: Harness Decision Loop
```bash
# Run unit tests
cargo test -p sandbox --lib agent_harness

# Verify:
# - Decide -> ToolCall -> Decide -> Complete flow works
# - No synthesis phase needed
# - Action enum variants work correctly
```

## Test Suite 2: Concurrent Run Narrative (03.5.2)

### Test 2.1: Concurrent Worker Dispatch
```bash
# Create run with multiple workers
POST /api/conductor/runs
{
  "objective": "Research Rust web frameworks and benchmark them",
  "capabilities": ["researcher", "terminal"]
}

# Verify:
# - Both workers spawn concurrently
# - Conductor doesn't block on first worker
# - Workers write to separate draft files
```

### Test 2.2: Run Description Accumulation
```bash
# Poll run state
GET /api/conductor/runs/{run_id}

# Verify:
# - run.draft_path points to living document
# - Document updated as workers complete
# - Can read full narrative from draft.md
```

### Test 2.3: Conductor Wake Context
```bash
# Simulate Conductor restart mid-run
# Stop Conductor, start new instance
# GET /api/conductor/runs/{run_id}

# Verify:
# - New Conductor reads draft.md
# - Can reconstruct context from document
# - Continues orchestration (doesn't fail)
```

## Test Suite 3: Writer PROMPT Button (04)

### Test 3.1: PROMPT Button Visibility
```rust
// Frontend test: dioxus-desktop
#[test]
fn prompt_button_shows_when_dirty() {
    // Load writer with clean document
    // Verify: PROMPT button hidden
    
    // Type in editor
    // Verify: PROMPT button visible
}
```

### Test 3.2: Diff Computation
```bash
# Submit PROMPT request
POST /api/writer/prompt
{
  "file_path": "/docs/report.md",
  "base_revision": "abc123",
  "saved_content": "# Original\n\nContent",
  "draft_content": "# Original\n\nContent\n\nAdded text",
  "user_instruction": "Improve clarity"
}

# Verify:
# - Diff computed correctly
# - Conductor task created with diff payload
```

### Test 3.3: Live Patch Events
```bash
# WebSocket listener
ws://localhost:8080/ws

# Expect events:
# - writer.prompt.started
# - writer.prompt.progress (multiple)
# - writer.prompt.patch (edit operations)
# - writer.prompt.completed

# Verify:
# - Patches can be applied to editor
# - Editor content updates live
```

## Integration Tests

### Test I.1: Full Research Flow
```
User: "What's going on (song)"
  ↓
Chat → Conductor
  ↓
Conductor spawns Researcher
  ↓
Researcher writes draft.md (live updates)
  ↓
Conductor reads draft, spawns Terminal (verify charts)
  ↓
Terminal completes, Conductor updates draft
  ↓
Conductor completes, returns summary
  ↓
UI shows final document (freeform, not structured)
```

### Test I.2: Writer Edit Flow
```
User: Types in Writer (document dirty)
  ↓
PROMPT button appears
  ↓
User clicks PROMPT, enters instruction
  ↓
Diff sent to Conductor
  ↓
Conductor spawns Researcher (context gathering)
  ↓
Researcher writes findings
  ↓
Conductor generates patches
  ↓
Patches stream to Writer UI
  ↓
Editor applies patches live
  ↓
Completion signal, PROMPT button returns
```

## Validation Commands

```bash
# Build check
cargo check -p sandbox
cargo check -p dioxus-desktop

# Unit tests
cargo test -p sandbox --lib agent_harness
cargo test -p sandbox --lib conductor
cargo test -p sandbox --lib watcher

# Integration tests
cargo test -p sandbox --test websocket_chat_test
cargo test -p sandbox --test conductor_api_test

# E2E (manual)
just dev-sandbox  # Terminal 1
just dev-ui       # Terminal 2
# Run test scenarios via UI
```

## Success Criteria

| Test | Criteria |
|------|----------|
| Harness | All 138 tests pass |
| Concurrent | 2+ workers run in parallel |
| Living Doc | Document streams live updates |
| PROMPT Button | Shows/hides based on dirty state |
| Patches | Applied live without page refresh |
| Freeform | No structured report sections |

## Known Gaps

1. **Writer harness file authority** - Writer app-agent harness ownership cutover still pending (Task 15)
2. **Live document UI** - Frontend needs document_update handler (Task 16)
3. **BAML culling** - Complex types still in use (Task 17)
4. **Writer diff** - Diff computation not implemented
5. **Patch events** - writer.prompt.* events not defined
