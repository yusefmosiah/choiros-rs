# E2E Tests for ChoirOS

End-to-end tests using Playwright and browser automation.

## Phase 4: Integration Screenshot Tests

New comprehensive E2E test suite for the ChoirOS chat system with full browser automation and screenshot documentation.

### Quick Start

```bash
# Run all Phase 4 E2E tests with full orchestration
./run-phase4-e2e.sh

# Run specific test
./run-phase4-e2e.sh basic      # Basic chat flow
./run-phase4-e2e.sh multiturn  # Multiturn conversation
./run-phase4-e2e.sh tool       # Tool execution
./run-phase4-e2e.sh error      # Error handling
./run-phase4-e2e.sh recovery   # Connection recovery
./run-phase4-e2e.sh concurrent # Concurrent users
```

### Architecture

**Backend (`sandbox/`)**
- WebSocket at `ws://localhost:8080/ws/chat/{actor_id}`
- REST API at `http://localhost:8080`
- Event-sourced with SQLite persistence
- ChatAgent with BAML LLM integration

**Frontend (`dioxus-desktop/`)**
- Dioxus-based web UI
- Connects via WebSocket for real-time chat
- Runs on port 3000 (dev server)

**Browser Automation (`skills/dev-browser/`)**
- Playwright-based browser automation
- TypeScript test scripts
- Screenshots for visual verification

### Test Files

#### Main Test Orchestrator
- `tests/integration_chat_e2e.rs` - Rust test that orchestrates servers and runs all E2E tests

#### TypeScript E2E Test Scripts
- `tests/e2e/test_e2e_basic_chat_flow.ts` - Basic chat interaction
- `tests/e2e/test_e2e_multiturn_conversation.ts` - Context preservation across exchanges
- `tests/e2e/test_e2e_tool_execution.ts` - Tool calling and execution
- `tests/e2e/test_e2e_error_handling.ts` - Graceful error handling
- `tests/e2e/test_e2e_connection_recovery.ts` - Reconnection and history persistence
- `tests/e2e/test_e2e_concurrent_users.ts` - Conversation isolation between users
- `tests/e2e/standalone-test.ts` - Quick smoke test (no orchestration)

### Running with Cargo

```bash
# Full suite (with server orchestration)
cargo test -p sandbox --test integration_chat_e2e -- --ignored --nocapture

# Individual tests
cargo test -p sandbox --test integration_chat_e2e test_e2e_basic_chat_flow -- --ignored --nocapture
cargo test -p sandbox --test integration_chat_e2e test_e2e_multiturn_conversation -- --ignored --nocapture
```

### Standalone Test (Servers Already Running)

```bash
# Start browser automation server
cd skills/dev-browser && ./server.sh &

# Run quick smoke test
cd skills/dev-browser && npx tsx ../../tests/e2e/standalone-test.ts
```

### Screenshot Output

All screenshots saved to `tests/screenshots/phase4/`:
```
{test_name}_step{step_number}_{description}.png
```

Examples:
- `test_e2e_basic_chat_flow_step1_initial_load.png`
- `test_e2e_concurrent_users_A_step3_message_sent.png`

---

## Legacy E2E Tests (Python/Playwright)

Original E2E test suite using Python and pytest.

### Setup

```bash
# Install dependencies
pip install playwright pytest pytest-asyncio
playwright install chromium

# Run tests
pytest tests/e2e/ -v

# Run with UI (headed mode)
pytest tests/e2e/ -v --headed

# Run specific test
pytest tests/e2e/test_first_time_user.py -v
```

### Test Structure

- `conftest.py` - Shared fixtures and configuration
- `test_*.py` - Test files organized by feature
- `cuj/` - Critical User Journey tests
- `screenshots/` - Baseline and test screenshots

### Running Tests

Tests require both backend and frontend servers to be running:

```bash
# Terminal 1: Start backend
cargo run -p sandbox

# Terminal 2: Start frontend
cd dioxus-desktop && dx serve

# Terminal 3: Run E2E tests
pytest tests/e2e/ -v
```

## Deliverables

### Phase 4 Implementation ✅

- [x] `tests/integration_chat_e2e.rs` - Test orchestrator
- [x] `tests/e2e/test_e2e_basic_chat_flow.ts` - Basic chat test
- [x] `tests/e2e/test_e2e_multiturn_conversation.ts` - Context test
- [x] `tests/e2e/test_e2e_tool_execution.ts` - Tool execution test
- [x] `tests/e2e/test_e2e_error_handling.ts` - Error handling test
- [x] `tests/e2e/test_e2e_connection_recovery.ts` - Recovery test
- [x] `tests/e2e/test_e2e_concurrent_users.ts` - Concurrent users test
- [x] `tests/e2e/standalone-test.ts` - Quick smoke test
- [x] `run-phase4-e2e.sh` - Test runner script

### Test Coverage

| Test | Description | Status |
|------|-------------|--------|
| Basic Chat Flow | Send/receive messages | ✅ |
| Multiturn | Context preservation | ✅ |
| Tool Execution | File listing, tool UI | ✅ |
| Error Handling | Graceful error display | ✅ |
| Connection Recovery | Reconnect, history restore | ✅ |
| Concurrent Users | Conversation isolation | ✅ |

## CI/CD Integration

```yaml
- name: Run Phase 4 E2E Tests
  run: |
    ./run-phase4-e2e.sh
    
- name: Archive Screenshots
  uses: actions/upload-artifact@v4
  with:
    name: e2e-screenshots
    path: tests/screenshots/phase4/
  if: always()
```

## Troubleshooting

### Servers won't start
```bash
# Free up ports
lsof -ti:8080,3000 | xargs kill -9
```

### Browser automation fails
```bash
# Install dev-browser dependencies
cd skills/dev-browser && npm install

# Check browser server
curl http://localhost:3001/health
```

### Tests timeout
- First run may take 30-60s for compilation
- Increase timeouts in test scripts if needed
