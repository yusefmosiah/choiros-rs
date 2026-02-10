#!/bin/bash
#
# Writer API Smoke Test Script
#
# Tests basic Writer API functionality:
# - Open existing document
# - Save with correct revision
# - Preview markdown
#

set -e

BASE_URL="${BASE_URL:-http://localhost:8080}"
TEST_DIR="test_writer_$$"

echo "=== Writer API Smoke Test ==="
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
    rm -f "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}/test_doc.md"
    rmdir "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}" 2>/dev/null || true
}

trap cleanup EXIT

# Create test directory
mkdir -p "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}"

# Create a test file
echo "# Test Document

This is a test file for the Writer API." > "/Users/wiz/choiros-rs/sandbox/${TEST_DIR}/test_doc.md"

info "Created test file: ${TEST_DIR}/test_doc.md"

# Test 1: Open existing document
echo ""
echo "Test 1: Open existing document"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/test_doc.md\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Open document returned 200"
else
    fail "Open document returned HTTP $HTTP_CODE: $BODY"
fi

# Extract values from response
REVISION=$(echo "$BODY" | jq -r '.revision')
CONTENT=$(echo "$BODY" | jq -r '.content')
MIME=$(echo "$BODY" | jq -r '.mime')
PATH=$(echo "$BODY" | jq -r '.path')

if [ "$REVISION" = "1" ]; then
    pass "Initial revision is 1"
else
    fail "Expected revision 1, got $REVISION"
fi

if [ "$MIME" = "text/markdown" ]; then
    pass "MIME type is text/markdown"
else
    fail "Expected MIME type text/markdown, got $MIME"
fi

if echo "$CONTENT" | grep -q "Test Document"; then
    pass "Content contains expected text"
else
    fail "Content doesn't contain expected text: $CONTENT"
fi

info "Document opened successfully (revision: $REVISION)"

# Test 2: Save document with correct revision
echo ""
echo "Test 2: Save document with correct revision"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/save" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/test_doc.md\", \"base_rev\": $REVISION, \"content\": \"# Updated Test Document\\n\\nThis content has been updated.\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Save document returned 200"
else
    fail "Save document returned HTTP $HTTP_CODE: $BODY"
fi

NEW_REVISION=$(echo "$BODY" | jq -r '.revision')
SAVED=$(echo "$BODY" | jq -r '.saved')

if [ "$NEW_REVISION" = "2" ]; then
    pass "Revision incremented to 2"
else
    fail "Expected revision 2, got $NEW_REVISION"
fi

if [ "$SAVED" = "true" ]; then
    pass "Saved flag is true"
else
    fail "Saved flag is not true: $SAVED"
fi

info "Document saved successfully (new revision: $NEW_REVISION)"

# Test 3: Verify saved content
echo ""
echo "Test 3: Verify saved content"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/test_doc.md\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Re-open document returned 200"
else
    fail "Re-open document returned HTTP $HTTP_CODE: $BODY"
fi

NEW_CONTENT=$(echo "$BODY" | jq -r '.content')
CURRENT_REVISION=$(echo "$BODY" | jq -r '.revision')

if echo "$NEW_CONTENT" | grep -q "updated"; then
    pass "Saved content is correct"
else
    fail "Saved content doesn't match: $NEW_CONTENT"
fi

if [ "$CURRENT_REVISION" = "2" ]; then
    pass "Current revision is 2 after re-open"
else
    fail "Expected current revision 2, got $CURRENT_REVISION"
fi

# Test 4: Preview markdown content
echo ""
echo "Test 4: Preview markdown content"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/preview" \
    -H "Content-Type: application/json" \
    -d '{"content": "# Preview Test\n\nThis is **bold** and *italic* text.\n\n- Item 1\n- Item 2"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Preview returned 200"
else
    fail "Preview returned HTTP $HTTP_CODE: $BODY"
fi

HTML=$(echo "$BODY" | jq -r '.html')

if echo "$HTML" | grep -q "<h1>Preview Test</h1>"; then
    pass "Preview contains expected h1 tag"
else
    fail "Preview doesn't contain expected h1: $HTML"
fi

if echo "$HTML" | grep -q "<strong>bold</strong>"; then
    pass "Preview contains bold text"
else
    fail "Preview doesn't contain bold text: $HTML"
fi

if echo "$HTML" | grep -q "<ul>"; then
    pass "Preview contains unordered list"
else
    fail "Preview doesn't contain list: $HTML"
fi

info "Markdown preview works correctly"

# Test 5: Preview by path
echo ""
echo "Test 5: Preview by path"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/preview" \
    -H "Content-Type: application/json" \
    -d "{\"path\": \"${TEST_DIR}/test_doc.md\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    pass "Preview by path returned 200"
else
    fail "Preview by path returned HTTP $HTTP_CODE: $BODY"
fi

HTML=$(echo "$BODY" | jq -r '.html')

if echo "$HTML" | grep -q "<h1>Updated Test Document</h1>"; then
    pass "Preview by path renders correct content"
else
    fail "Preview by path doesn't render correctly: $HTML"
fi

# Test 6: Open non-existent file
echo ""
echo "Test 6: Open non-existent file"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "${BASE_URL}/writer/open" \
    -H "Content-Type: application/json" \
    -d '{"path": "nonexistent_file_xyz.md"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "404" ]; then
    pass "Open non-existent file returns 404"
else
    fail "Expected 404, got HTTP $HTTP_CODE: $BODY"
fi

ERROR_CODE=$(echo "$BODY" | jq -r '.error.code')

if [ "$ERROR_CODE" = "NOT_FOUND" ]; then
    pass "Error code is NOT_FOUND"
else
    fail "Expected error code NOT_FOUND, got $ERROR_CODE"
fi

echo ""
echo "=== All Smoke Tests Passed ==="
echo ""
