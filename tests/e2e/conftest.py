"""
Pytest configuration for E2E tests
"""

import pytest
import subprocess
import time
import os
from playwright.sync_api import sync_playwright, Page, Browser

# Configuration
BASE_URL = os.getenv("CHOIROS_BASE_URL", "http://localhost:5173")
BACKEND_URL = os.getenv("CHOIROS_BACKEND_URL", "http://localhost:8080")


@pytest.fixture(scope="session")
def browser():
    """Start browser once for all tests"""
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=os.getenv("HEADLESS", "1") == "1")
        yield browser
        browser.close()


@pytest.fixture
def page(browser: Browser):
    """Create a new page for each test"""
    page = browser.new_page(viewport={"width": 375, "height": 667})  # Mobile viewport
    yield page
    page.close()


@pytest.fixture
def desktop_page(browser: Browser):
    """Create a desktop viewport page"""
    page = browser.new_page(viewport={"width": 1920, "height": 1080})
    yield page
    page.close()


@pytest.fixture(autouse=True)
def check_servers():
    """Verify backend and frontend are running before tests"""
    import requests
    
    # Check backend
    try:
        response = requests.get(f"{BACKEND_URL}/health", timeout=2)
        assert response.status_code == 200, "Backend not responding"
    except requests.exceptions.ConnectionError:
        pytest.skip(f"Backend not running at {BACKEND_URL}. Start with: cargo run -p sandbox")
    
    # Check frontend (optional - some tests might work without it)
    # We don't fail here since tests will naturally fail if frontend isn't available


def wait_for_load(page: Page, url: str):
    """Navigate to URL and wait for network idle"""
    page.goto(url)
    page.wait_for_load_state("networkidle")
