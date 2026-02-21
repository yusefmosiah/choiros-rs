# ChoirOS Testing Architecture

## Overview

This document defines the testing strategy for ChoirOS, structured around **transaction steps** that can be composed into complete test workflows. Each layer builds on the previous, creating a reliable testing pyramid from unit tests to full E2E validation.

## Testing Layers

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 5: E2E Validation                                         │
│ - Browser automation with agent-browser                         │
│ - Full user journey validation                                  │
│ - Screenshot-based regression testing                          │
├─────────────────────────────────────────────────────────────────┤
│ Layer 4: Integration Orchestration                              │
│ - Multi-process coordination (backend + frontend + browser)     │
│ - Health checks and service readiness                           │
│ - Transaction-based setup/teardown                              │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: API/Protocol Tests                                     │
│ - WebSocket protocol validation                                 │
│ - HTTP endpoint testing                                         │
│ - Actor message passing                                         │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Component Tests                                        │
│ - Tool registry and individual tools                            │
│ - ChatAgent logic (mocked LLM)                                  │
│ - Event sourcing and persistence                                │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Unit Tests                                             │
│ - Function-level logic                                          │
│ - Data structure validation                                     │
│ - Utility function testing                                      │
└─────────────────────────────────────────────────────────────────┘
```

## Transaction Step Architecture

Each testing phase is a **transaction** - either fully succeeds or fully fails with rollback:

### Step 1: Environment Setup Transaction

**Purpose**: Prepare isolated testing environment

```bash
# Transaction: Setup
function setup_test_env() {
    # Create temp directory
    export TEST_DIR=$(mktemp -d)
    export TEST_DB="$TEST_DIR/test.db"
    
    # Set environment variables
    export CHOIR_TEST_MODE=1
    export CHOIR_DB_PATH="$TEST_DB"
    
    # Rollback on failure
    trap 'rm -rf "$TEST_DIR"' EXIT
}
```

**Success Criteria**:
- [ ] Temp directory created
- [ ] Environment variables set
- [ ] Rollback trap registered

**Rollback**: Remove temp directory, unset env vars

---

### Step 2: Service Startup Transaction

**Purpose**: Start all required services in dependency order

```bash
# Transaction: Start Services
function start_services() {
    # 1. Backend (port 8080)
    just dev-sandbox &
    BACKEND_PID=$!
    
    # 2. Wait for backend health
    wait_for_health "http://localhost:8080/api/health" 30
    
    # 3. Frontend (port 3000)
    just dev-ui &
    FRONTEND_PID=$!
    
    # 4. Wait for frontend
    wait_for_health "http://localhost:3000" 30
    
    # Rollback on failure
    trap 'kill $BACKEND_PID $FRONTEND_PID 2>/dev/null; rm -rf "$TEST_DIR"' EXIT
}
```

**Success Criteria**:
- [ ] Backend process started
- [ ] Backend health check passes
- [ ] Frontend process started
- [ ] Frontend health check passes

**Rollback**: Kill all started processes

---

### Step 3: Test Execution Transaction

**Purpose**: Run actual tests with full environment

```bash
# Transaction: Run Tests
function run_e2e_tests() {
    # Test parameters
    local test_name=$1
    local screenshot_dir="tests/screenshots/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$screenshot_dir"
    
    # Run test with agent-browser
    agent-browser open http://localhost:3000
    agent-browser screenshot "$screenshot_dir/01-initial.png"
    
    # Get snapshot and interact
    agent-browser snapshot -i --json > "$screenshot_dir/snapshot.json"
    
    # ... test steps ...
    
    # Validate results
    if [ -f "$screenshot_dir/final.png" ]; then
        return 0  # Success
    else
        return 1  # Failure - triggers rollback
    fi
}
```

**Success Criteria**:
- [ ] All test assertions pass
- [ ] Screenshots captured
- [ ] No errors in logs

**Rollback**: Archive screenshots, generate failure report

---

### Step 4: Cleanup Transaction

**Purpose**: Clean shutdown and artifact preservation

```bash
# Transaction: Cleanup
function cleanup() {
    # Save artifacts
    if [ -d "$screenshot_dir" ]; then
        mv "$screenshot_dir" "tests/screenshots/archive/"
    fi
    
    # Kill processes
    kill $BACKEND_PID $FRONTEND_PID 2>/dev/null
    wait $BACKEND_PID $FRONTEND_PID 2>/dev/null
    
    # Remove temp files
    rm -rf "$TEST_DIR"
    
    # Unset env vars
    unset CHOIR_TEST_MODE CHOIR_DB_PATH
}
```

**Success Criteria**:
- [ ] Artifacts saved to archive
- [ ] Processes terminated
- [ ] Temp files removed

---

## Browser Automation Action Planning

When designing a feature, define the **actions** an agent would take to test it:

### Template: Feature Test Plan

```markdown
## Feature: [Feature Name]

### User Story
As a [user], I want to [action], so that [benefit]

### Browser Automation Actions

#### Action 1: [Action Name]
**Trigger**: [What starts this action]
**Precondition**: [Required state]
**Steps**:
1. agent-browser [command]
2. agent-browser [command]
3. agent-browser [command]

**Expected Result**: [What success looks like]
**Validation**:
- [ ] Screenshot shows [state]
- [ ] Element @e[ref] contains [text]
- [ ] No errors in console

#### Action 2: [Action Name]
...
```

### Example: Chat Feature

```markdown
## Feature: Chat Application

### Browser Automation Actions

#### Action 1: Open Chat Window
**Trigger**: User clicks Chat icon or types in prompt bar
**Precondition**: Desktop is loaded
**Steps**:
1. agent-browser open http://localhost:3000
2. agent-browser wait --text "Desktop"
3. agent-browser snapshot -i
4. agent-browser click @e1  # Chat icon

**Expected Result**: Chat window appears with title "ChoirOS Chat"
**Validation**:
- [ ] Screenshot shows chat window
- [ ] Element with text "ChoirOS Chat" visible
- [ ] Input field present

#### Action 2: Send Message
**Trigger**: User types and sends message
**Precondition**: Chat window is open
**Steps**:
1. agent-browser fill @e2 "Hello, AI!"
2. agent-browser click @e3  # Send button
3. agent-browser wait --text "Sending..." --timeout 2000
4. agent-browser wait --text "AI response" --timeout 30000

**Expected Result**: AI responds to message
**Validation**:
- [ ] User message appears in chat
- [ ] "Sending..." indicator shown
- [ ] AI response received within 30s
- [ ] Response contains meaningful text

#### Action 3: Verify Persistence
**Trigger**: Page refresh
**Precondition**: Conversation exists
**Steps**:
1. agent-browser reload
2. agent-browser wait --text "Desktop"
3. agent-browser click @e1  # Chat icon
4. agent-browser wait --text "Hello, AI!"

**Expected Result**: Previous conversation restored
**Validation**:
- [ ] Previous messages visible
- [ ] No "Loading..." indicators stuck
```

---

## Test Orchestration Script

Create a unified test runner that executes transactions:

```bash
#!/bin/bash
# tests/run-e2e-suite.sh

set -euo pipefail  # Exit on error, undefined vars, pipe failures

# Configuration
SCREENSHOT_DIR="tests/screenshots/$(date +%Y%m%d-%H%M%S)"
TEST_RESULTS="$SCREENSHOT_DIR/results.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Logging
log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Transaction: Setup
setup() {
    log_info "Setting up test environment..."
    mkdir -p "$SCREENSHOT_DIR"
    
    # Create result tracking
    echo '{"tests": [], "start_time": "'$(date -Iseconds)'"}' > "$TEST_RESULTS"
    
    # Check prerequisites
    command -v agent-browser >/dev/null 2>&1 || {
        log_error "agent-browser not installed. Run: npx skills add vercel-labs/agent-browser@agent-browser -g"
        exit 1
    }
    
    command -v just >/dev/null 2>&1 || {
        log_error "just not installed. Install from https://github.com/casey/just"
        exit 1
    }
}

# Transaction: Start Services
start_services() {
    log_info "Starting services..."
    
    # Start backend
    log_info "Starting backend on port 8080..."
    just dev-sandbox &
    BACKEND_PID=$!
    
    # Wait for backend
    log_info "Waiting for backend health check..."
    for i in {1..30}; do
        if curl -s http://localhost:8080/api/health | grep -q "healthy"; then
            log_info "Backend ready!"
            break
        fi
        sleep 1
    done
    
    # Start frontend
    log_info "Starting frontend on port 3000..."
    just dev-ui &
    FRONTEND_PID=$!
    
    # Wait for frontend
    log_info "Waiting for frontend..."
    for i in {1..30}; do
        if curl -s http://localhost:3000 | grep -q "Desktop\|ChoirOS"; then
            log_info "Frontend ready!"
            break
        fi
        sleep 1
    done
    
    # Export PIDs for cleanup
    export BACKEND_PID FRONTEND_PID
}

# Transaction: Run Test
run_test() {
    local test_name=$1
    local test_steps=$2
    
    log_info "Running test: $test_name"
    
    local test_dir="$SCREENSHOT_DIR/$test_name"
    mkdir -p "$test_dir"
    
    local start_time=$(date +%s)
    local success=0
    
    # Execute test steps
    if eval "$test_steps"; then
        success=1
        log_info "✓ Test passed: $test_name"
    else
        log_error "✗ Test failed: $test_name"
    fi
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    # Record results
    cat "$TEST_RESULTS" | jq \
        --arg name "$test_name" \
        --argjson success "$success" \
        --argjson duration "$duration" \
        '.tests += [{"name": $name, "success": $success, "duration": $duration}]' \
        > "$TEST_RESULTS.tmp" && mv "$TEST_RESULTS.tmp" "$TEST_RESULTS"
    
    return $((1 - success))
}

# Transaction: Cleanup
cleanup() {
    log_info "Cleaning up..."
    
    # Kill services
    if [ -n "${BACKEND_PID:-}" ]; then
        kill $BACKEND_PID 2>/dev/null || true
        wait $BACKEND_PID 2>/dev/null || true
    fi
    
    if [ -n "${FRONTEND_PID:-}" ]; then
        kill $FRONTEND_PID 2>/dev/null || true
        wait $FRONTEND_PID 2>/dev/null || true
    fi
    
    # Generate report
    log_info "Generating report..."
    local passed=$(jq '[.tests[] | select(.success == 1)] | length' "$TEST_RESULTS")
    local total=$(jq '.tests | length' "$TEST_RESULTS")
    
    jq --arg passed "$passed" --arg total "$total" \
       '. + {"end_time": "'$(date -Iseconds)'", "passed": $passed, "total": $total}' \
       "$TEST_RESULTS" > "$TEST_RESULTS.tmp" && mv "$TEST_RESULTS.tmp" "$TEST_RESULTS"
    
    log_info "Results: $passed/$total tests passed"
    log_info "Screenshots: $SCREENSHOT_DIR"
    log_info "Report: $TEST_RESULTS"
}

# Main execution
main() {
    # Set trap for cleanup
    trap cleanup EXIT
    
    # Execute transactions
    setup
    start_services
    
    # Run tests
    run_test "chat-open" '
        agent-browser open http://localhost:3000 &&
        agent-browser wait 2000 &&
        agent-browser screenshot "$test_dir/01-open.png" &&
        agent-browser snapshot -i > "$test_dir/snapshot.json"
    '
    
    run_test "chat-interaction" '
        agent-browser open http://localhost:3000 &&
        agent-browser wait --text "Desktop" &&
        agent-browser snapshot -i > "$test_dir/snapshot.json" &&
        # Find and click chat element
        REF=$(cat "$test_dir/snapshot.json" | grep -o "ref=e[0-9]*" | head -1 | cut -d= -f2) &&
        agent-browser click @$REF &&
        agent-browser wait 1000 &&
        agent-browser screenshot "$test_dir/02-chat-open.png"
    '
    
    # Success
    exit 0
}

main "$@"
```

---

## Directory Structure

```
tests/
├── architecture.md           # This document
├── unit/                     # Layer 1: Unit tests
│   └── (inline in src/)
├── component/                # Layer 2: Component tests
│   ├── tools_test.rs
│   ├── chat_agent_test.rs
│   └── persistence_test.rs
├── integration/              # Layer 3: API/Protocol tests
│   ├── websocket_test.rs
│   └── api_test.rs
├── e2e/                      # Layer 4 & 5: E2E tests
│   ├── README.md
│   ├── run-suite.sh          # Orchestration script
│   ├── actions/              # Browser automation action definitions
│   │   ├── chat-open.action
│   │   ├── chat-message.action
│   │   └── chat-persistence.action
│   └── scripts/              # Executable test scripts
│       ├── test-chat-flow.sh
│       └── test-tool-execution.sh
└── screenshots/              # Generated screenshots
    ├── archive/              # Historical test runs
    └── latest/               # Symlink to latest run
```

---

## Best Practices

### 1. Test Independence
Each test should be independent - no shared state between tests:
```bash
# Good: Fresh actor_id for each test
ACTOR_ID="test-$(uuidgen)"
agent-browser open "http://localhost:3000?actor_id=$ACTOR_ID"
```

### 2. Deterministic Selectors
Use stable selectors, not positional:
```bash
# Bad: May break if UI changes
agent-browser click @e5

# Good: Semantic locator
agent-browser find text "Send" click
```

### 3. Explicit Waits
Always wait for conditions, not arbitrary delays:
```bash
# Bad
sleep 5

# Good
agent-browser wait --text "Success"
agent-browser wait --load networkidle
```

### 4. Screenshot Everything
Screenshots are the ultimate debug tool:
```bash
# Capture state at each step
agent-browser screenshot "$STEP_DIR/01-open.png"
agent-browser screenshot "$STEP_DIR/02-click.png"
agent-browser screenshot "$STEP_DIR/03-result.png"
```

### 5. JSON Output for Assertions
Use `--json` flag for programmatic validation:
```bash
RESULT=$(agent-browser get text @e1 --json | jq -r '.text')
[ "$RESULT" = "Expected" ] || exit 1
```

---

## CI/CD Integration

```yaml
# .github/workflows/e2e.yml
name: E2E Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install just
        uses: extractions/setup-just@v1
      
      - name: Install agent-browser
        run: npx skills add vercel-labs/agent-browser@agent-browser -g
      
      - name: Run E2E Suite
        run: ./tests/e2e/run-suite.sh
      
      - name: Upload Screenshots
        uses: actions/upload-artifact@v3
        with:
          name: e2e-screenshots
          path: tests/screenshots/
```

---

## Summary

**Key Principles:**
1. **Transaction Steps**: Each phase is atomic with rollback
2. **Action Planning**: Define browser automation when designing features
3. **One Script**: Single orchestration script runs everything
4. **Screenshots as Truth**: Visual validation is primary
5. **CLI-Based**: agent-browser commands over code for simplicity

**Next Steps:**
1. [ ] Refactor existing E2E tests to use this architecture
2. [ ] Create action definitions for current features
3. [ ] Implement run-suite.sh with transaction support
4. [ ] Add to CI/CD pipeline
5. [ ] Document feature test plans as they're developed
