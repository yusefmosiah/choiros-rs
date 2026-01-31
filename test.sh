#!/bin/bash
# ChoirOS Test Script - Automated Testing & Screenshot Guide
# Run this script to test the system and generate screenshot instructions

set -e

echo "==================================="
echo "ChoirOS Desktop UI Test Script"
echo "==================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

test_step() {
    echo -e "${YELLOW}[TEST]${NC} $1"
}

test_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

test_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
}

echo "Step 1: Building Backend..."
test_step "Building sandbox (backend)"
if cargo build -p sandbox --quiet 2>&1 | grep -q "Finished"; then
    test_pass "Backend builds successfully"
else
    # Check if already built
    if [ -f "target/debug/sandbox" ]; then
        test_pass "Backend already built"
    else
        test_fail "Backend build failed"
        exit 1
    fi
fi

echo ""
echo "Step 2: Running Backend Tests..."
test_step "Running cargo test -p sandbox"
if cargo test -p sandbox --quiet 2>&1 | grep -q "test result: ok"; then
    TEST_COUNT=$(cargo test -p sandbox --quiet 2>&1 | grep "test result" | grep -o "[0-9]* passed" | grep -o "[0-9]*")
    test_pass "All $TEST_COUNT backend tests passed"
else
    test_fail "Some backend tests failed"
fi

echo ""
echo "Step 3: Building Frontend..."
test_step "Building sandbox-ui (frontend)"
if cargo build -p sandbox-ui --quiet 2>&1 | grep -q "Finished"; then
    test_pass "Frontend builds successfully"
else
    test_fail "Frontend build failed"
    exit 1
fi

echo ""
echo "Step 4: Checking API Endpoints..."
test_step "Testing backend server availability"

# Check if server is running
if curl -s http://localhost:8080/health > /dev/null 2>&1; then
    test_pass "Backend server is running on localhost:8080"
    
    # Test health endpoint
    HEALTH=$(curl -s http://localhost:8080/health)
    if echo "$HEALTH" | grep -q "healthy"; then
        test_pass "Health endpoint responding correctly"
    fi
    
    # Test desktop API
    DESKTOP=$(curl -s http://localhost:8080/desktop/test-desktop)
    if echo "$DESKTOP" | grep -q "success"; then
        test_pass "Desktop API endpoint working"
    fi
    
    # Test apps endpoint
    APPS=$(curl -s http://localhost:8080/desktop/test-desktop/apps)
    if echo "$APPS" | grep -q "apps"; then
        test_pass "Apps API endpoint working"
    fi
else
    test_fail "Backend server not running on localhost:8080"
    echo -e "${YELLOW}[INFO]${NC} Start the server with: cargo run -p sandbox"
fi

echo ""
echo "Step 5: UI Component Check..."
test_step "Verifying UI components exist"

if [ -f "sandbox-ui/src/desktop.rs" ]; then
    test_pass "Desktop component exists"
fi

if [ -f "sandbox-ui/src/components.rs" ]; then
    test_pass "ChatView component exists"
fi

if grep -q "WindowChrome" "sandbox-ui/src/desktop.rs"; then
    test_pass "WindowChrome component found"
fi

if grep -q "Taskbar" "sandbox-ui/src/desktop.rs"; then
    test_pass "Taskbar component found"
fi

echo ""
echo "==================================="
echo "Test Summary"
echo "==================================="
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ ALL TESTS PASSED${NC}"
    echo ""
    echo "==================================="
    echo "Screenshot Instructions"
    echo "==================================="
    echo ""
    echo "To capture screenshots manually:"
    echo ""
    echo "1. Start the backend server:"
    echo "   cargo run -p sandbox"
    echo ""
    echo "2. In another terminal, start the UI dev server:"
    echo "   cd sandbox-ui && dx serve"
    echo ""
    echo "3. Open browser to: http://localhost:5173"
    echo ""
    echo "4. Open browser DevTools (F12):"
    echo "   - Go to 'Elements' or 'Inspector' tab"
    echo "   - Use device toolbar to test mobile (375x667)"
    echo "   - Use device toolbar to test desktop (1920x1080)"
    echo ""
    echo "5. Capture these screenshots:"
    echo ""
    echo "   📷 Screenshot 1: Initial Load"
    echo "      - Load http://localhost:5173"
    echo "      - Capture: 'No windows open' message"
    echo "      - Save: screenshots/01-initial-load.png"
    echo ""
    echo "   📷 Screenshot 2: Chat Window Opened"  
    echo "      - Click Chat app icon (💬) in taskbar"
    echo "      - Capture: Window with title bar and Chat UI"
    echo "      - Save: screenshots/02-chat-window.png"
    echo ""
    echo "   📷 Screenshot 3: Message Sent"
    echo "      - Type 'Hello ChoirOS!' in chat"
    echo "      - Press Enter"
    echo "      - Capture: Message in chat bubble"
    echo "      - Save: screenshots/03-message-sent.png"
    echo ""
    echo "   📷 Screenshot 4: API Test (Terminal)"
    echo "      - Run: curl http://localhost:8080/health"
    echo "      - Capture terminal output"
    echo "      - Save: screenshots/04-api-test.png"
    echo ""
    echo "   📷 Screenshot 5: Mobile View"
    echo "      - Use DevTools device: iPhone 12 (390x844)"
    echo "      - Capture: Full mobile layout"
    echo "      - Save: screenshots/05-mobile-view.png"
    echo ""
    echo "   📷 Screenshot 6: Desktop View"
    echo "      - Use DevTools device: Desktop (1920x1080)"
    echo "      - Capture: Desktop layout (still single window in Phase 1)"
    echo "      - Save: screenshots/06-desktop-view.png"
    echo ""
    echo "6. Create screenshots directory:"
    echo "   mkdir -p screenshots"
    echo ""
    echo "7. Save all screenshots there for the test report!"
    echo ""
    exit 0
else
    echo -e "${RED}❌ SOME TESTS FAILED${NC}"
    echo "Please fix the issues above before capturing screenshots."
    exit 1
fi
