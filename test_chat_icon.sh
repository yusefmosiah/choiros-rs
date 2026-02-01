#!/bin/bash
# Test chat icon click functionality

set -e

echo "=== Testing Chat Icon Click ==="

# Start backend in background
echo "1. Starting backend..."
cargo run -p sandbox &
BACKEND_PID=$!
sleep 5

# Check backend health
echo "2. Checking backend health..."
curl -s http://localhost:8080/api/health || {
    echo "ERROR: Backend not healthy"
    kill $BACKEND_PID 2>/dev/null
    exit 1
}

# Test opening a window via API (simulating what the frontend does)
echo "3. Testing window open API..."
RESPONSE=$(curl -s -X POST http://localhost:8080/desktop/test-desktop/windows \
  -H "Content-Type: application/json" \
  -d '{"app_id":"chat","title":"Chat"}')

echo "API Response: $RESPONSE"

# Check if success
if echo "$RESPONSE" | grep -q '"success":true'; then
    echo "✅ Window opened successfully via API"
    WINDOW_ID=$(echo "$RESPONSE" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
    echo "Window ID: $WINDOW_ID"
else
    echo "❌ Failed to open window"
    echo "Response: $RESPONSE"
    kill $BACKEND_PID 2>/dev/null
    exit 1
fi

# Get desktop state to verify window exists
echo "4. Checking desktop state..."
STATE=$(curl -s http://localhost:8080/desktop/test-desktop)
echo "Desktop state: $STATE"

if echo "$STATE" | grep -q "$WINDOW_ID"; then
    echo "✅ Window found in desktop state"
else
    echo "❌ Window not found in desktop state"
fi

# Cleanup
echo "5. Cleaning up..."
kill $BACKEND_PID 2>/dev/null
wait $BACKEND_PID 2>/dev/null || true

echo "=== Test Complete ==="
