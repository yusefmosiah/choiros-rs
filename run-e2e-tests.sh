#!/bin/bash
# Run E2E tests for ChoirOS

set -e

echo "=== ChoirOS E2E Test Runner ==="

# Check if servers are running
echo "Checking if backend is running..."
if ! curl -s http://localhost:8080/health > /dev/null; then
    echo "❌ Backend not running at http://localhost:8080"
    echo "   Start with: cargo run -p sandbox"
    exit 1
fi
echo "✅ Backend is running"

echo "Checking if frontend is running..."
if ! curl -s http://localhost:5173 > /dev/null; then
    echo "❌ Frontend not running at http://localhost:5173"
    echo "   Start with: cd sandbox-ui && dx serve"
    exit 1
fi
echo "✅ Frontend is running"

# Setup Python environment
echo ""
echo "Setting up Python environment..."
cd tests/e2e

if [ ! -d "venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv venv
fi

source venv/bin/activate

# Install dependencies
echo "Installing dependencies..."
pip install -q -r requirements.txt

# Install playwright browsers if needed
if [ ! -d "$HOME/.cache/ms-playwright" ]; then
    echo "Installing Playwright browsers..."
    playwright install chromium
fi

# Create screenshots directory
mkdir -p screenshots

# Run tests
echo ""
echo "Running E2E tests..."
echo "===================="

# Check for headed mode
if [ "$1" == "--headed" ]; then
    HEADED="--headed"
    echo "Running in headed mode (browser visible)"
else
    HEADED=""
    echo "Running in headless mode (use --headed for visible browser)"
fi

pytest -v $HEADED "$@"

echo ""
echo "✅ E2E tests complete!"
echo "Screenshots saved to: tests/e2e/screenshots/"
