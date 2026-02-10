#!/bin/bash
#
# Writer API Conflict Test Script
#
# Tests optimistic concurrency control:
# - Detect conflicts when base_rev is stale
# - Return current content on conflict
# - Allow retry with updated revision
#

set -e

BASE_URL="${BASE_URL:-http://localhost:8080}"
TEST_DIR="test_writer_conflict_$$"

echo "=== Writer API Conflict Test ==="
echo "Base URL: $BASE_URL"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    exit 1
}

info() {
    echo -e "${YELLOW}ℹ INFO${NC}: $1"
}

# Cleanup function
cleanup() {
    info "Cleaning up test files..."
    rm -f "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}/conflict_test.md"
    rmdir "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}" 2>/dev/null || true
}

trap cleanup EXIT

# Create test directory
mkdir -p "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}"

# Create a test file
echo "# Conflict Test Document

Initial version." > "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}/conflict_test.md"

info "Created test file: ${TEST_DIR}/conflict_test.md"

# Step 1: Both clients open the document
echo ""
echo "Step 1: Both clients open the document"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/conflict_test.md\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" != "200" ]; then
    fail "Failed to open document: $BODY"
fi

CLIENT_A_REVISION=$(echo "$BODY" | jq -r '.revision')
info "Both clients see revision: $CLIENT_A_REVISION"

# Step 2: Client A saves first
echo ""
echo "Step 2: Client A saves first with revision $CLIENT_A_REVISION"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/conflict_test.md\", \"base_rev\": $CLIENT_A_REVISION, \"content\": \"# Conflict Test Document\\n\\nChanges from Client A.\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" != "200" ]; then
    fail "Client A save failed: $BODY"
fi

CLIENT_A_NEW_REVISION=$(echo "$BODY" | jq -r '.revision')
pass "Client A saved successfully (new revision: $CLIENT_A_NEW_REVISION)"

# Step 3: Client B tries to save with stale revision
echo ""
echo "Step 3: Client B tries to save with stale revision $CLIENT_A_REVISION"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/conflict_test.md\", \"base_rev\": $CLIENT_A_REVISION, \"content\": \"# Conflict Test Document\\n\\nChanges from Client B.\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "409" ]; then
    pass "Conflict detected - Server returns 409"
else
    fail "Expected 409 conflict, got HTTP $HTTP_CODE: $BODY"
fi

ERROR_CODE=$(echo "$BODY" | jq -r '.error.code')
if [ "$ERROR_CODE" = "CONFLICT" ]; then
    pass "Error code is CONFLICT"
else
    fail "Expected error code CONFLICT, got $ERROR_CODE"
fi

SERVER_REVISION=$(echo "$BODY" | jq -r '.current_revision')
if [ "$SERVER_REVISION" = "$CLIENT_A_NEW_REVISION" ]; then
    pass "Server reports current revision: $SERVER_REVISION"
else
    fail "Expected server revision $CLIENT_A_NEW_REVISION, got $SERVER_REVISION"
fi

SERVER_CONTENT=$(echo "$BODY" | jq -r '.current_content')
if echo "$SERVER_CONTENT" | grep -q "Client A"; then
    pass "Server returns current content from Client A"
else
    fail "Server content doesn't match Client A's changes: $SERVER_CONTENT"
fi

info "Client B receives conflict response with current server state"

# Step 4: Client B resolves conflict and retries
echo ""
echo "Step 4: Client B resolves conflict and retries with revision $SERVER_REVISION"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/conflict_test.md\", \"base_rev\": $SERVER_REVISION, \"content\": \"# Conflict Test Document\\n\\nChanges from Client A and Client B.\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Client B save succeeds with updated revision"
else
    fail "Client B retry failed: $BODY"
fi

CLIENT_B_REVISION=$(echo "$BODY" | jq -r '.revision')
if [ "$CLIENT_B_REVISION" = "3" ]; then
    pass "Final revision is 3 (initial + A + B)"
else
    fail "Expected final revision 3, got $CLIENT_B_REVISION"
fi

# Step 5: Verify final content
echo ""
echo "Step 5: Verify final content"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/conflict_test.md\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" != "200" ]; then
    fail "Failed to verify final content: $BODY"
fi

FINAL_CONTENT=$(echo "$BODY" | jq -r '.content')
FINAL_REVISION=$(echo "$BODY" | jq -r '.revision')

if echo "$FINAL_CONTENT" | grep -q "Client A and Client B"; then
    pass "Final content contains merged changes"
else
    fail "Final content doesn't match: $FINAL_CONTENT"
fi

if [ "$FINAL_REVISION" = "3" ]; then
    pass "Final revision is 3"
else
    fail "Expected final revision 3, got $FINAL_REVISION"
fi

# Test 6: Multiple rapid saves
echo ""
echo "Test 6: Multiple sequential saves increment revision correctly"
for i in 4 5 6 7 8; do
    PREV=$((i - 1))
    RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
        -H "Content-Type: application/json" \
        -d "{\"path\": \"${TEST_DIR}/conflict_test.md\", \"base_rev\": $PREV, \"content\": \"Version $i\"}")

    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | sed '$d')

    if [ "$HTTP_CODE" != "200" ]; then
        fail "Save at revision $PREV failed: $BODY"
    fi

    REV=$(echo "$BODY" | jq -r '.revision')
    if [ "$REV" != "$i" ]; then
        fail "Expected revision $i, got $REV"
    fi
done
pass "Sequential saves work correctly (revisions 3→8)"

# Test 7: Path traversal should be rejected
echo ""
echo "Test 7: Path traversal protection"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d '{"path": "../escape_attempt.txt", "base_rev": 1, "content": "Escaped!"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

if [ "$HTTP_CODE" = "403" ]; then
    pass "Path traversal in save is rejected with 403"
else
    fail "Expected 403 for path traversal, got $HTTP_CODE"
fi

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d '{"path": "../Cargo.toml"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

if [ "$HTTP_CODE" = "403" ]; then
    pass "Path traversal in open is rejected with 403"
else
    fail "Expected 403 for path traversal, got $HTTP_CODE"
fi

# Test 8: Save to directory should fail
echo ""
echo "Test 8: Save to directory fails appropriately"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}\", \"base_rev\": 1, \"content\": \"Can't save to dir\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

if [ "$HTTP_CODE" = "400" ]; then
    pass "Save to directory returns 400"
else
    fail "Expected 400 for directory save, got $HTTP_CODE"
fi

echo ""
echo "=== All Conflict Tests Passed ==="
echo ""
