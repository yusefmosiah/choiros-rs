#!/bin/bash

# E2E Test Runner Script
# This script runs the E2E test in a controlled way

cd /Users/wiz/choiros-rs/tests/e2e

echo "🧪 Running E2E Test: test_e2e_basic_chat_flow.ts"
echo "================================================"
echo ""

# Run the test with tsx
npx tsx test_e2e_basic_chat_flow.ts 2>&1

exit_code=$?

echo ""
echo "================================================"
echo "Test completed with exit code: $exit_code"
echo "================================================"

exit $exit_code
