#!/bin/bash
#
# Files API Smoke Tests - Happy Path Testing
# Tests the Files API endpoints with valid operations
#

set -e

# Configuration
BASE_URL="${BASE_URL:-http://localhost:8080}"
API_PREFIX="/files"
SANDBOX_ROOT="/Users/wiz/choiros-rs/sandbox"

# Test tracking
PASSED=0
FAILED=0

# Colors for output (if terminal supports it)
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Generate unique test folder name to avoid conflicts
TEST_FOLDER="test_$(date +%s)_$$"

# Helper functions
print_result() {
    local test_name="$1"
    local status="$2"
    local reason="${3:-}"

    if [ "$status" = "OK" ]; then
        echo -e "[${test_name}] ... ${GREEN}OK${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "[${test_name}] ... ${RED}FAILED${NC}: ${reason}"
        FAILED=$((FAILED + 1))
    fi
}

make_request() {
    local method="$1"
    local endpoint="$2"
    local data="${3:-}"
    local query_params="${4:-}"

    local url="${BASE_URL}${API_PREFIX}${endpoint}"
    if [ -n "$query_params" ]; then
        url="${url}?${query_params}"
    fi

    if [ -n "$data" ]; then
        curl -s -w "\n%{http_code}" -X "$method" \
            -H "Content-Type: application/json" \
            -d "$data" \
            "$url"
    else
        curl -s -w "\n%{http_code}" -X "$method" "$url"
    fi
}

extract_body() {
    local response="$1"
    echo "$response" | sed '$d'
}

extract_status() {
    local response="$1"
    echo "$response" | tail -n1
}

cleanup() {
    echo ""
    echo "Cleaning up test artifacts..."
    rm -rf "${SANDBOX_ROOT}/${TEST_FOLDER}"
}

# Set trap for cleanup on exit
trap cleanup EXIT

# Check prerequisites
echo "=== Files API Smoke Tests ==="
echo "Base URL: ${BASE_URL}"
echo "Test Folder: ${TEST_FOLDER}"
echo ""

# Check if server is reachable
if ! curl -s "${BASE_URL}/api/health" > /dev/null 2>&1; then
    echo "Warning: Server at ${BASE_URL} may not be reachable"
    echo "Continuing anyway..."
    echo ""
fi

# Test 1: Create Directory
echo "Running tests..."
{
    response=$(make_request "POST" "/mkdir" "{\"path\": \"${TEST_FOLDER}\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.created == true' > /dev/null 2>&1; then
        print_result "Create Directory" "OK"
    else
        print_result "Create Directory" "FAILED" "Expected 200 with created=true, got ${status}: ${body}"
    fi
}

# Test 2: Create File
{
    response=$(make_request "POST" "/create" "{\"path\": \"${TEST_FOLDER}/test.txt\", \"content\": \"Hello, World!\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.created == true' > /dev/null 2>&1; then
        print_result "Create File" "OK"
    else
        print_result "Create File" "FAILED" "Expected 200 with created=true, got ${status}: ${body}"
    fi
}

# Test 3: Write to File
{
    response=$(make_request "POST" "/write" "{\"path\": \"${TEST_FOLDER}/test.txt\", \"content\": \"Updated content here\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.bytes_written > 0' > /dev/null 2>&1; then
        print_result "Write to File" "OK"
    else
        print_result "Write to File" "FAILED" "Expected 200 with bytes_written>0, got ${status}: ${body}"
    fi
}

# Test 4: Read File Content
{
    response=$(make_request "GET" "/content" "" "path=${TEST_FOLDER}/test.txt")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.content == "Updated content here"' > /dev/null 2>&1; then
        print_result "Read File Content" "OK"
    else
        print_result "Read File Content" "FAILED" "Expected 200 with correct content, got ${status}: ${body}"
    fi
}

# Test 5: Get File Metadata
{
    response=$(make_request "GET" "/metadata" "" "path=${TEST_FOLDER}/test.txt")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.is_file == true and .name == "test.txt"' > /dev/null 2>&1; then
        print_result "Get File Metadata" "OK"
    else
        print_result "Get File Metadata" "FAILED" "Expected 200 with is_file=true, got ${status}: ${body}"
    fi
}

# Test 6: List Directory
{
    response=$(make_request "GET" "/list" "" "path=${TEST_FOLDER}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.entries | length >= 1' > /dev/null 2>&1; then
        print_result "List Directory" "OK"
    else
        print_result "List Directory" "FAILED" "Expected 200 with entries, got ${status}: ${body}"
    fi
}

# Test 7: Copy File
{
    response=$(make_request "POST" "/copy" "{\"source\": \"${TEST_FOLDER}/test.txt\", \"target\": \"${TEST_FOLDER}/test_copy.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.copied == true' > /dev/null 2>&1; then
        print_result "Copy File" "OK"
    else
        print_result "Copy File" "FAILED" "Expected 200 with copied=true, got ${status}: ${body}"
    fi
}

# Test 8: Rename File
{
    response=$(make_request "POST" "/rename" "{\"source\": \"${TEST_FOLDER}/test_copy.txt\", \"target\": \"${TEST_FOLDER}/test_renamed.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.renamed == true' > /dev/null 2>&1; then
        print_result "Rename File" "OK"
    else
        print_result "Rename File" "FAILED" "Expected 200 with renamed=true, got ${status}: ${body}"
    fi
}

# Test 9: Delete File
{
    response=$(make_request "POST" "/delete" "{\"path\": \"${TEST_FOLDER}/test_renamed.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.deleted == true' > /dev/null 2>&1; then
        print_result "Delete File" "OK"
    else
        print_result "Delete File" "FAILED" "Expected 200 with deleted=true, got ${status}: ${body}"
    fi
}

# Test 10: Delete Directory (recursive)
{
    response=$(make_request "POST" "/delete" "{\"path\": \"${TEST_FOLDER}\", \"recursive\": true}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "200" ] && echo "$body" | jq -e '.deleted == true' > /dev/null 2>&1; then
        print_result "Delete Directory" "OK"
    else
        print_result "Delete Directory" "FAILED" "Expected 200 with deleted=true, got ${status}: ${body}"
    fi
}

# Test 11: Verify directory is deleted (should get NOT_FOUND)
{
    response=$(make_request "GET" "/metadata" "" "path=${TEST_FOLDER}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ]; then
        print_result "Verify Directory Deleted" "OK"
    else
        print_result "Verify Directory Deleted" "FAILED" "Expected 404, got ${status}"
    fi
}

# Summary
echo ""
echo "=== Summary ==="
echo "Passed: ${PASSED}"
echo "Failed: ${FAILED}"

if [ ${FAILED} -gt 0 ]; then
    exit 1
else
    exit 0
fi
