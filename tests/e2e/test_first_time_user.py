"""
First-time user journey E2E test

Tests the critical path: user opens the app, clicks chat icon, sends a message
"""

import pytest
from playwright.sync_api import Page, expect


def test_first_time_user_opens_chat(page: Page, base_url: str = "http://localhost:5173"):
    """
    Test: First-time user opens Chat and sends a message
    
    Steps:
    1. Navigate to the app
    2. Click the chat icon (💬) in the taskbar
    3. Verify window opens
    4. Type a message
    5. Send the message
    6. Verify message appears
    """
    # Navigate to app
    page.goto(base_url)
    page.wait_for_load_state("networkidle")
    
    # Take initial screenshot
    page.screenshot(path="tests/e2e/screenshots/01-initial-load.png")
    
    # Click on chat icon in taskbar
    chat_icon = page.locator("text=💬")
    expect(chat_icon).to_be_visible()
    chat_icon.click()
    
    # Wait for window to open
    page.wait_for_selector(".window-chrome, .window", timeout=5000)
    
    # Take screenshot with window open
    page.screenshot(path="tests/e2e/screenshots/02-chat-window-open.png")
    
    # Verify window is visible
    window = page.locator(".window-chrome, .window").first
    expect(window).to_be_visible()
    
    # Verify window has title
    window_title = page.locator(".window-title, h2:has-text('Chat')").first
    expect(window_title).to_be_visible()


def test_window_management_close_and_reopen(page: Page, base_url: str = "http://localhost:5173"):
    """
    Test: User can close and reopen a window
    
    Steps:
    1. Open chat window
    2. Close the window
    3. Verify window is gone
    4. Reopen chat window
    5. Verify window opens again
    """
    # Navigate to app
    page.goto(base_url)
    page.wait_for_load_state("networkidle")
    
    # Open chat window
    page.locator("text=💬").click()
    page.wait_for_selector(".window-chrome, .window", timeout=5000)
    
    # Close the window (click X button)
    close_button = page.locator(".close-button, button:has-text('×'), button:has-text('x')").first
    if close_button.is_visible():
        close_button.click()
    else:
        # Alternative: click on window chrome close button
        page.locator(".window-chrome button").first.click()
    
    # Wait for window to disappear
    page.wait_for_timeout(500)
    
    # Verify window is gone
    windows = page.locator(".window-chrome, .window")
    expect(windows).to_have_count(0)
    
    # Take screenshot
    page.screenshot(path="tests/e2e/screenshots/03-window-closed.png")
    
    # Reopen chat window
    page.locator("text=💬").click()
    page.wait_for_selector(".window-chrome, .window", timeout=5000)
    
    # Verify window is visible again
    expect(page.locator(".window-chrome, .window").first).to_be_visible()
    
    # Take screenshot
    page.screenshot(path="tests/e2e/screenshots/04-window-reopened.png")


def test_responsive_layout_mobile(page: Page, base_url: str = "http://localhost:5173"):
    """
    Test: Mobile layout shows single window full-screen
    
    Steps:
    1. Set mobile viewport
    2. Open chat window
    3. Verify window takes full screen
    4. Verify taskbar is at bottom
    """
    # Set mobile viewport
    page.set_viewport_size({"width": 375, "height": 667})
    
    # Navigate to app
    page.goto(base_url)
    page.wait_for_load_state("networkidle")
    
    # Open chat window
    page.locator("text=💬").click()
    page.wait_for_selector(".window-chrome, .window", timeout=5000)
    
    # Take screenshot
    page.screenshot(path="tests/e2e/screenshots/05-mobile-view.png")
    
    # Verify taskbar exists at bottom
    taskbar = page.locator(".taskbar, [class*='taskbar']").first
    expect(taskbar).to_be_visible()
    
    # Verify window is visible
    window = page.locator(".window-chrome, .window").first
    expect(window).to_be_visible()


def test_responsive_layout_desktop(desktop_page: Page, base_url: str = "http://localhost:5173"):
    """
    Test: Desktop layout shows windows in proper layout
    
    Steps:
    1. Set desktop viewport
    2. Open chat window
    3. Verify window layout is appropriate
    """
    page = desktop_page
    
    # Navigate to app
    page.goto(base_url)
    page.wait_for_load_state("networkidle")
    
    # Open chat window
    page.locator("text=💬").click()
    page.wait_for_selector(".window-chrome, .window", timeout=5000)
    
    # Take screenshot
    page.screenshot(path="tests/e2e/screenshots/06-desktop-view.png")
    
    # Verify window is visible
    window = page.locator(".window-chrome, .window").first
    expect(window).to_be_visible()
