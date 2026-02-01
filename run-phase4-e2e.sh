#!/bin/bash
# Run ChoirOS E2E Integration Tests for Phase 4
# 
# Usage:
#   ./run-phase4-e2e.sh              # Run full E2E suite
#   ./run-phase4-e2e.sh <test_name>  # Run specific test

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     ChoirOS Phase 4: E2E Integration Test Runner              ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
echo

# Create screenshot directory
mkdir -p tests/screenshots/phase4
mkdir -p tests/data

# Function to cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    # Kill any remaining processes
    pkill -f "cargo run -p sandbox" 2>/dev/null || true
    pkill -f "cargo run -p sandbox-ui" 2>/dev/null || true
    pkill -f "server.sh" 2>/dev/null || true
    sleep 2
}
trap cleanup EXIT

# Check if dev-browser skill is set up
echo -e "${BLUE}Checking dev-browser skill...${NC}"
if [ ! -d "skills/dev-browser/node_modules" ]; then
    echo -e "${YELLOW}Installing dev-browser dependencies...${NC}"
    cd skills/dev-browser && npm install && cd ../..
fi

# Check arguments
TEST_NAME="$1"

if [ -z "$TEST_NAME" ]; then
    # Run full E2E suite
    echo -e "${GREEN}Running full E2E test suite...${NC}"
    echo
    cargo test -p sandbox --test integration_chat_e2e -- --ignored --nocapture
else
    # Run specific test
    echo -e "${GREEN}Running specific test: $TEST_NAME${NC}"
    echo
    
    case "$TEST_NAME" in
        "basic"|"chat")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_basic_chat_flow -- --ignored --nocapture
            ;;
        "multiturn"|"context")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_multiturn_conversation -- --ignored --nocapture
            ;;
        "tool"|"tools")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_tool_execution -- --ignored --nocapture
            ;;
        "error"|"errors")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_error_handling -- --ignored --nocapture
            ;;
        "recovery"|"reconnect")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_connection_recovery -- --ignored --nocapture
            ;;
        "concurrent"|"users")
            cargo test -p sandbox --test integration_chat_e2e test_e2e_concurrent_users -- --ignored --nocapture
            ;;
        "all"|"full")
            cargo test -p sandbox --test integration_chat_e2e integration_chat_e2e_full_suite -- --ignored --nocapture
            ;;
        *)
            echo -e "${RED}Unknown test: $TEST_NAME${NC}"
            echo "Available tests:"
            echo "  basic|chat      - Basic chat flow"
            echo "  multiturn       - Multiturn conversation"
            echo "  tool|tools      - Tool execution"
            echo "  error|errors    - Error handling"
            echo "  recovery        - Connection recovery"
            echo "  concurrent      - Concurrent users"
            echo "  all|full        - Full suite"
            exit 1
            ;;
    esac
fi

echo -e "\n${GREEN}✅ Test execution complete!${NC}"
echo -e "${BLUE}Screenshots saved to: tests/screenshots/phase4/${NC}"

# List generated screenshots
echo -e "\n${YELLOW}Generated screenshots:${NC}"
ls -lh tests/screenshots/phase4/*.png 2>/dev/null || echo "No screenshots found"
