#!/bin/bash
set -e

echo "=== ConductorActor End-to-End Test ==="

# Configuration
BASE_URL="${BASE_URL:-http://localhost:8080}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper function to print colored output
print_status() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

print_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

print_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    echo "Warning: jq is not installed. JSON output will not be formatted."
    JQ_CMD="cat"
else
    JQ_CMD="jq ."
fi

# Check if curl is installed
if ! command -v curl &> /dev/null; then
    print_error "curl is not installed. Please install curl to run this test."
    exit 1
fi

print_info "Testing against: $BASE_URL"
echo ""

# ============================================================================
# Test 1: Health Check
# ============================================================================
echo "1. Testing health endpoint..."
HEALTH_RESPONSE=$(curl -s -w "\n%{http_code}" "$BASE_URL/health" 2>/dev/null || echo -e "\n000")
HEALTH_STATUS=$(echo "$HEALTH_RESPONSE" | tail -n1)
HEALTH_BODY=$(echo "$HEALTH_RESPONSE" | sed '$d')

if [ "$HEALTH_STATUS" = "200" ]; then
    print_status "Health endpoint returned 200 OK"
    echo "$HEALTH_BODY" | $JQ_CMD
else
    print_error "Health endpoint failed with status $HEALTH_STATUS"
    echo "$HEALTH_BODY" | $JQ_CMD
    exit 1
fi

echo ""

# ============================================================================
# Test 2: Submit Conductor Task
# ============================================================================
echo "2. Submitting Conductor task..."
CORRELATION_ID="e2e-test-$(date +%s)"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/conductor/execute" \
  -H "Content-Type: application/json" \
  -d "{
    \"objective\": \"Generate a short terminal-backed report\",
    \"desktop_id\": \"test-desktop-001\",
    \"output_mode\": \"markdown_report_to_writer\",
    \"worker_plan\": [{
      \"worker_type\": \"terminal\",
      \"objective\": \"Print a greeting in terminal\",
      \"terminal_command\": \"echo conductor-test\",
      \"timeout_ms\": 5000,
      \"max_steps\": 1
    }],
    \"correlation_id\": \"$CORRELATION_ID\"
  }" 2>/dev/null || echo -e "\n000")

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_STATUS" = "202" ]; then
    print_status "Task submitted successfully (HTTP 202 Accepted)"
else
    print_error "Task submission failed with HTTP $HTTP_STATUS"
    echo "$BODY" | $JQ_CMD
    exit 1
fi

echo "$BODY" | $JQ_CMD

# Extract task_id from response
if command -v jq &> /dev/null; then
    TASK_ID=$(echo "$BODY" | jq -r '.task_id // empty')
    if [ -z "$TASK_ID" ] || [ "$TASK_ID" = "null" ]; then
        print_error "No task_id in response"
        exit 1
    fi
    print_info "Task ID: $TASK_ID"
else
    # Fallback: try to extract task_id with grep/sed
    TASK_ID=$(echo "$BODY" | grep -o '"task_id":"[^"]*"' | sed 's/"task_id":"//;s/"$//' || echo "")
    if [ -n "$TASK_ID" ]; then
        print_info "Task ID: $TASK_ID"
    fi
fi

echo ""

# ============================================================================
# Test 3: Check Task Status
# ============================================================================
echo "3. Checking task status..."

if [ -n "$TASK_ID" ]; then
    STATUS_RESPONSE=$(curl -s -w "\n%{http_code}" "$BASE_URL/conductor/tasks/$TASK_ID" 2>/dev/null || echo -e "\n000")
    STATUS_CODE=$(echo "$STATUS_RESPONSE" | tail -n1)
    STATUS_BODY=$(echo "$STATUS_RESPONSE" | sed '$d')

    if [ "$STATUS_CODE" = "200" ]; then
        print_status "Task status retrieved successfully"
        echo "$STATUS_BODY" | $JQ_CMD
    else
        print_error "Unexpected status code: $STATUS_CODE"
        echo "$STATUS_BODY" | $JQ_CMD
    fi
else
    print_info "Skipping task status check (no task_id available)"
fi

echo ""

# ============================================================================
# Test 4: Validation - Empty Objective
# ============================================================================
echo "4. Testing validation - empty objective..."

VALIDATION_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/conductor/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "objective": "",
    "desktop_id": "test-desktop-002",
    "output_mode": "markdown_report_to_writer"
  }' 2>/dev/null || echo -e "\n000")

VALIDATION_STATUS=$(echo "$VALIDATION_RESPONSE" | tail -n1)
VALIDATION_BODY=$(echo "$VALIDATION_RESPONSE" | sed '$d')

if [ "$VALIDATION_STATUS" = "400" ]; then
    print_status "Validation working - empty objective rejected (HTTP 400)"
    echo "$VALIDATION_BODY" | $JQ_CMD
else
    print_error "Validation failed - expected HTTP 400, got $VALIDATION_STATUS"
    echo "$VALIDATION_BODY" | $JQ_CMD
fi

echo ""

# ============================================================================
# Test 5: Validation - Empty Desktop ID
# ============================================================================
echo "5. Testing validation - empty desktop_id..."

VALIDATION_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/conductor/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "objective": "Research Rust async patterns",
    "desktop_id": "",
    "output_mode": "markdown_report_to_writer"
  }' 2>/dev/null || echo -e "\n000")

VALIDATION_STATUS=$(echo "$VALIDATION_RESPONSE" | tail -n1)
VALIDATION_BODY=$(echo "$VALIDATION_RESPONSE" | sed '$d')

if [ "$VALIDATION_STATUS" = "400" ]; then
    print_status "Validation working - empty desktop_id rejected (HTTP 400)"
    echo "$VALIDATION_BODY" | $JQ_CMD
else
    print_error "Validation failed - expected HTTP 400, got $VALIDATION_STATUS"
    echo "$VALIDATION_BODY" | $JQ_CMD
fi

echo ""

# ============================================================================
# Test 6: 404 for Non-existent Task
# ============================================================================
echo "6. Testing 404 for non-existent task..."

NOTFOUND_RESPONSE=$(curl -s -w "\n%{http_code}" "$BASE_URL/conductor/tasks/non-existent-task-12345" 2>/dev/null || echo -e "\n000")
NOTFOUND_STATUS=$(echo "$NOTFOUND_RESPONSE" | tail -n1)
NOTFOUND_BODY=$(echo "$NOTFOUND_RESPONSE" | sed '$d')

if [ "$NOTFOUND_STATUS" = "404" ]; then
    print_status "Non-existent task returns 404 as expected"
    echo "$NOTFOUND_BODY" | $JQ_CMD
else
    print_error "Expected 404 for non-existent task, got $NOTFOUND_STATUS"
    echo "$NOTFOUND_BODY" | $JQ_CMD
fi

echo ""

# ============================================================================
# Test 7: Auto-generated Correlation ID
# ============================================================================
echo "7. Testing auto-generated correlation_id..."

AUTO_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/conductor/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "objective": "Generate a short terminal-backed report",
    "desktop_id": "test-desktop-003",
    "output_mode": "markdown_report_to_writer",
    "worker_plan": [{
      "worker_type": "terminal",
      "objective": "Print a greeting in terminal",
      "terminal_command": "echo conductor-auto-correlation",
      "timeout_ms": 5000,
      "max_steps": 1
    }]
  }' 2>/dev/null || echo -e "\n000")

AUTO_STATUS=$(echo "$AUTO_RESPONSE" | tail -n1)
AUTO_BODY=$(echo "$AUTO_RESPONSE" | sed '$d')

if [ "$AUTO_STATUS" = "202" ]; then
    if command -v jq &> /dev/null; then
        AUTO_CORR_ID=$(echo "$AUTO_BODY" | jq -r '.correlation_id // empty')
        if [ -n "$AUTO_CORR_ID" ] && [ "$AUTO_CORR_ID" != "null" ]; then
            print_status "Auto-generated correlation_id: $AUTO_CORR_ID"
        else
            print_error "No correlation_id in response"
        fi
    else
        print_status "Task submitted without correlation_id (HTTP 202)"
    fi
    echo "$AUTO_BODY" | $JQ_CMD
else
    print_error "Auto-correlation test failed with HTTP $AUTO_STATUS"
    echo "$AUTO_BODY" | $JQ_CMD
fi

echo ""

# ============================================================================
# Summary
# ============================================================================
echo "=== Test Complete ==="
print_info "Correlation ID used: $CORRELATION_ID"
if [ -n "$TASK_ID" ]; then
    print_info "Task ID created: $TASK_ID"
fi
echo ""
echo "All basic tests completed. Check output above for any failures."
