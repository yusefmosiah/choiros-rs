# E2E Tests for ChoirOS

End-to-end tests using Playwright.

## Setup

```bash
# Install dependencies
pip install playwright pytest pytest-asyncio
playwright install chromium

# Run tests
pytest tests/e2e/ -v

# Run with UI (headed mode)
pytest tests/e2e/ -v --headed

# Run specific test
pytest tests/e2e/test_first_time_user.py -v
```

## Test Structure

- `conftest.py` - Shared fixtures and configuration
- `test_*.py` - Test files organized by feature
- `cuj/` - Critical User Journey tests
- `screenshots/` - Baseline and test screenshots

## Running Tests

Tests require both backend and frontend servers to be running:

```bash
# Terminal 1: Start backend
cargo run -p sandbox

# Terminal 2: Start frontend
cd sandbox-ui && dx serve

# Terminal 3: Run E2E tests
pytest tests/e2e/ -v
```
