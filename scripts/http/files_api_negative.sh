#!/bin/bash
#
# Files API Negative Tests - Error Case Testing
# Tests the Files API endpoints with invalid operations to ensure proper error handling
#

# Don't use set -e because we expect some commands to fail

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
TEST_FOLDER="test_neg_$(date +%s)_$$"

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
            "$url" 2>/dev/null
    else
        curl -s -w "\n%{http_code}" -X "$method" "$url" 2>/dev/null
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

# Setup: Create test directory and file for negative tests
setup_test_env() {
    mkdir -p "${SANDBOX_ROOT}/${TEST_FOLDER}"
    echo "test content" > "${SANDBOX_ROOT}/${TEST_FOLDER}/existing.txt"
    mkdir -p "${SANDBOX_ROOT}/${TEST_FOLDER}/subdir"
}

# Check prerequisites
echo "=== Files API Negative Tests ==="
echo "Base URL: ${BASE_URL}"
echo "Test Folder: ${TEST_FOLDER}"
echo ""

# Setup test environment
setup_test_env

echo "Running tests..."

# Test 1: Path Traversal (../ escaping sandbox)
{
    response=$(make_request "GET" "/content" "" "path=../etc/passwd")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "403" ] && echo "$body" | jq -e '.error.code == "PATH_TRAVERSAL"' > /dev/null 2>&1; then
        print_result "Path Traversal (../)" "OK"
    else
        print_result "Path Traversal (../)" "FAILED" "Expected 403 with PATH_TRAVERSAL, got ${status}: ${body}"
    fi
}

# Test 2: Absolute Path Rejection
{
    response=$(make_request "GET" "/content" "" "path=/etc/passwd")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "403" ] && echo "$body" | jq -e '.error.code == "PATH_TRAVERSAL"' > /dev/null 2>&1; then
        print_result "Absolute Path Rejection" "OK"
    else
        print_result "Absolute Path Rejection" "FAILED" "Expected 403 with PATH_TRAVERSAL, got ${status}: ${body}"
    fi
}

# Test 3: Reading Non-Existent File
{
    response=$(make_request "GET" "/content" "" "path=${TEST_FOLDER}/nonexistent_file.txt")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "Read Non-Existent File" "OK"
    else
        print_result "Read Non-Existent File" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 4: Writing to Non-Existent Directory
{
    response=$(make_request "POST" "/create" "{\"path\": \"${TEST_FOLDER}/nonexistent_dir/file.txt\", \"content\": \"test\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "400" ] && echo "$body" | jq -e '.error.code == "NOT_A_DIRECTORY"' > /dev/null 2>&1; then
        print_result "Write to Non-Existent Directory" "OK"
    else
        print_result "Write to Non-Existent Directory" "FAILED" "Expected 400 with NOT_A_DIRECTORY, got ${status}: ${body}"
    fi
}

# Test 5: Creating File That Already Exists
{
    response=$(make_request "POST" "/create" "{\"path\": \"${TEST_FOLDER}/existing.txt\", \"content\": \"new content\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "409" ] && echo "$body" | jq -e '.error.code == "ALREADY_EXISTS"' > /dev/null 2>&1; then
        print_result "Create Existing File" "OK"
    else
        print_result "Create Existing File" "FAILED" "Expected 409 with ALREADY_EXISTS, got ${status}: ${body}"
    fi
}

# Test 6: Creating Directory That Already Exists
{
    response=$(make_request "POST" "/mkdir" "{\"path\": \"${TEST_FOLDER}\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "409" ] && echo "$body" | jq -e '.error.code == "ALREADY_EXISTS"' > /dev/null 2>&1; then
        print_result "Create Existing Directory" "OK"
    else
        print_result "Create Existing Directory" "FAILED" "Expected 409 with ALREADY_EXISTS, got ${status}: ${body}"
    fi
}

# Test 7: Reading Directory as File
{
    response=$(make_request "GET" "/content" "" "path=${TEST_FOLDER}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "400" ] && echo "$body" | jq -e '.error.code == "NOT_A_FILE"' > /dev/null 2>&1; then
        print_result "Read Directory as File" "OK"
    else
        print_result "Read Directory as File" "FAILED" "Expected 400 with NOT_A_FILE, got ${status}: ${body}"
    fi
}

# Test 8: Listing File as Directory
{
    response=$(make_request "GET" "/list" "" "path=${TEST_FOLDER}/existing.txt")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "400" ] && echo "$body" | jq -e '.error.code == "NOT_A_DIRECTORY"' > /dev/null 2>&1; then
        print_result "List File as Directory" "OK"
    else
        print_result "List File as Directory" "FAILED" "Expected 400 with NOT_A_DIRECTORY, got ${status}: ${body}"
    fi
}

# Test 9: Invalid JSON Body
{
    response=$(curl -s -w "\n%{http_code}" -X POST \
        -H "Content-Type: application/json" \
        -d "{invalid json" \
        "${BASE_URL}${API_PREFIX}/create" 2>/dev/null)
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    # Should get 400 Bad Request for invalid JSON
    if [ "$status" = "400" ]; then
        print_result "Invalid JSON Body" "OK"
    else
        print_result "Invalid JSON Body" "FAILED" "Expected 400, got ${status}: ${body}"
    fi
}

# Test 10: Copy Non-Existent Source
{
    response=$(make_request "POST" "/copy" "{\"source\": \"${TEST_FOLDER}/nonexistent.txt\", \"target\": \"${TEST_FOLDER}/dest.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "Copy Non-Existent Source" "OK"
    else
        print_result "Copy Non-Existent Source" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 11: Copy Directory (not a file)
{
    response=$(make_request "POST" "/copy" "{\"source\": \"${TEST_FOLDER}/subdir\", \"target\": \"${TEST_FOLDER}/subdir_copy\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "400" ] && echo "$body" | jq -e '.error.code == "NOT_A_FILE"' > /dev/null 2>&1; then
        print_result "Copy Directory" "OK"
    else
        print_result "Copy Directory" "FAILED" "Expected 400 with NOT_A_FILE, got ${status}: ${body}"
    fi
}

# Test 12: Copy to Existing Target (without overwrite)
{
    echo "overwrite test" > "${SANDBOX_ROOT}/${TEST_FOLDER}/target_exists.txt"
    response=$(make_request "POST" "/copy" "{\"source\": \"${TEST_FOLDER}/existing.txt\", \"target\": \"${TEST_FOLDER}/target_exists.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "409" ] && echo "$body" | jq -e '.error.code == "ALREADY_EXISTS"' > /dev/null 2>&1; then
        print_result "Copy to Existing Target" "OK"
    else
        print_result "Copy to Existing Target" "FAILED" "Expected 409 with ALREADY_EXISTS, got ${status}: ${body}"
    fi
}

# Test 13: Rename Non-Existent Source
{
    response=$(make_request "POST" "/rename" "{\"source\": \"${TEST_FOLDER}/nonexistent.txt\", \"target\": \"${TEST_FOLDER}/renamed.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "Rename Non-Existent Source" "OK"
    else
        print_result "Rename Non-Existent Source" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 14: Rename to Existing Target (without overwrite)
{
    response=$(make_request "POST" "/rename" "{\"source\": \"${TEST_FOLDER}/existing.txt\", \"target\": \"${TEST_FOLDER}/target_exists.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "409" ] && echo "$body" | jq -e '.error.code == "ALREADY_EXISTS"' > /dev/null 2>&1; then
        print_result "Rename to Existing Target" "OK"
    else
        print_result "Rename to Existing Target" "FAILED" "Expected 409 with ALREADY_EXISTS, got ${status}: ${body}"
    fi
}

# Test 15: Delete Non-Existent Path
{
    response=$(make_request "POST" "/delete" "{\"path\": \"${TEST_FOLDER}/nonexistent_delete.txt\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "Delete Non-Existent Path" "OK"
    else
        print_result "Delete Non-Existent Path" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 16: Get Metadata for Non-Existent Path
{
    response=$(make_request "GET" "/metadata" "" "path=${TEST_FOLDER}/nonexistent_meta.txt")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "Metadata for Non-Existent Path" "OK"
    else
        print_result "Metadata for Non-Existent Path" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 17: List Non-Existent Directory
{
    response=$(make_request "GET" "/list" "" "path=${TEST_FOLDER}/nonexistent_dir")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "404" ] && echo "$body" | jq -e '.error.code == "NOT_FOUND"' > /dev/null 2>&1; then
        print_result "List Non-Existent Directory" "OK"
    else
        print_result "List Non-Existent Directory" "FAILED" "Expected 404 with NOT_FOUND, got ${status}: ${body}"
    fi
}

# Test 18: Path with Null Bytes (should be rejected)
{
    response=$(make_request "GET" "/content" "" "path=${TEST_FOLDER}/file%00.txt")
    # URL encoded null byte - server should reject
    status=$(extract_status "$response")

    # This might be handled at the HTTP layer or API layer
    if [ "$status" = "403" ] || [ "$status" = "400" ]; then
        print_result "Path with Null Bytes" "OK"
    else
        print_result "Path with Null Bytes" "FAILED" "Expected 403 or 400, got ${status}"
    fi
}

# Test 19: Deep Path Traversal (multiple ../)
{
    response=$(make_request "GET" "/content" "" "path=${TEST_FOLDER}/subdir/../../../etc/passwd")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "403" ] && echo "$body" | jq -e '.error.code == "PATH_TRAVERSAL"' > /dev/null 2>&1; then
        print_result "Deep Path Traversal" "OK"
    else
        print_result "Deep Path Traversal" "FAILED" "Expected 403 with PATH_TRAVERSAL, got ${status}: ${body}"
    fi
}

# Test 20: Write to Directory (not a file)
{
    response=$(make_request "POST" "/write" "{\"path\": \"${TEST_FOLDER}\", \"content\": \"test\"}")
    body=$(extract_body "$response")
    status=$(extract_status "$response")

    if [ "$status" = "400" ] && echo "$body" | jq -e '.error.code == "NOT_A_FILE"' > /dev/null 2>&1; then
        print_result "Write to Directory" "OK"
    else
        print_result "Write to Directory" "FAILED" "Expected 400 with NOT_A_FILE, got ${status}: ${body}"
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
