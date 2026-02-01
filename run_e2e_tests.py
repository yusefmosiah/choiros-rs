#!/usr/bin/env python3
"""
E2E Test Orchestrator for ChoirOS Chat App

This script orchestrates the full E2E test suite using the multi-terminal skill:
1. Backend server (cargo run -p sandbox) on port 8080
2. Frontend dev server (cd sandbox-ui && cargo run) on port 3000
3. Browser automation server (./skills/dev-browser/server.sh) on port 8000
4. E2E tests (cd tests/e2e && npx tsx test_e2e_basic_chat_flow.ts)
"""

import sys
import time
import subprocess
from pathlib import Path

# Add the multi-terminal scripts to path
sys.path.insert(0, '/Users/wiz/choiros-rs/skills/multi-terminal/scripts')

from terminal_session import TerminalSession

# Configuration
BASE_DIR = Path("/Users/wiz/choiros-rs")
SCREENSHOT_DIR = BASE_DIR / "tests/e2e/screenshots/phase4"
HEALTH_CHECK_TIMEOUT = 120  # seconds
TEST_TIMEOUT = 300  # seconds

def check_health_endpoint(url: str, timeout: int = 30) -> bool:
    """Check if a health endpoint is responding."""
    import urllib.request
    import socket
    
    start_time = time.time()
    while time.time() - start_time < timeout:
        try:
            req = urllib.request.Request(url, method='GET')
            with urllib.request.urlopen(req, timeout=2) as response:
                if response.status == 200:
                    return True
        except (urllib.error.URLError, socket.timeout, ConnectionRefusedError):
            pass
        time.sleep(1)
    return False

def main():
    print("=" * 80)
    print("🎭 ChoirOS E2E Test Suite Orchestrator")
    print("=" * 80)
    print()
    
    # Kill any existing session
    print("🧹 Cleaning up any existing sessions...")
    cleanup = subprocess.run(
        ["tmux", "kill-session", "-t", "choiros-e2e"],
        capture_output=True
    )
    time.sleep(1)
    
    # Create session
    print("📦 Creating tmux session 'choiros-e2e'...")
    session = TerminalSession("choiros-e2e", str(BASE_DIR))
    print("✅ Session created")
    print()
    
    # Create data directory for database
    data_dir = BASE_DIR / "data"
    data_dir.mkdir(exist_ok=True)
    db_path = data_dir / "events.db"
    
    # Window 1: Backend Server
    print("🔧 [Window 1] Starting Backend Server...")
    print("   Command: DATABASE_URL=./data/events.db cargo run -p sandbox")
    print("   Port: 8080")
    print(f"   Database: {db_path}")
    session.add_window("backend", "export DATABASE_URL=\"./data/events.db\" && cargo run -p sandbox")
    time.sleep(3)
    
    # Wait for backend health check
    print("   ⏳ Waiting for backend health check (http://localhost:8080/health)...")
    backend_ready = check_health_endpoint("http://localhost:8080/health", HEALTH_CHECK_TIMEOUT)
    if not backend_ready:
        print("   ❌ Backend failed to start within timeout!")
        print("   Backend output:")
        print(session.capture_output("backend", lines=50))
        session.kill()
        return 1
    print("   ✅ Backend is healthy!")
    print()
    
    # Window 2: Frontend Server
    print("🎨 [Window 2] Starting Frontend Dev Server...")
    print("   Command: cd sandbox-ui && cargo run")
    print("   Port: 3000")
    session.add_window("frontend", "cargo run", working_dir=str(BASE_DIR / "sandbox-ui"))
    time.sleep(3)
    
    # Wait for frontend
    print("   ⏳ Waiting for frontend (http://localhost:3000)...")
    frontend_ready = check_health_endpoint("http://localhost:3000", HEALTH_CHECK_TIMEOUT)
    if not frontend_ready:
        print("   ❌ Frontend failed to start within timeout!")
        print("   Frontend output:")
        print(session.capture_output("frontend", lines=50))
        session.kill()
        return 1
    print("   ✅ Frontend is ready!")
    print()
    
    # Window 3: Browser Automation Server
    print("🌐 [Window 3] Starting Browser Automation Server...")
    print("   Command: ./skills/dev-browser/server.sh")
    print("   Port: 8000")
    session.add_window("browser", "./skills/dev-browser/server.sh")
    time.sleep(3)
    
    # Wait for browser server
    print("   ⏳ Waiting for browser server (http://localhost:8000/health)...")
    browser_ready = check_health_endpoint("http://localhost:8000/health", HEALTH_CHECK_TIMEOUT)
    if not browser_ready:
        print("   ⚠️  Browser server health check not responding, but continuing...")
        print("   (Server may still be starting up)")
    else:
        print("   ✅ Browser server is ready!")
    print()
    
    # Give browser server extra time to fully initialize
    print("   ⏳ Allowing browser server to initialize (5s)...")
    time.sleep(5)
    
    # Window 4: E2E Test
    print("🧪 [Window 4] Running E2E Test...")
    print("   Command: cd tests/e2e && npx tsx test_e2e_basic_chat_flow.ts")
    print("   Screenshots will be saved to: tests/e2e/screenshots/phase4/")
    session.add_window("e2e-test", "npx tsx test_e2e_basic_chat_flow.ts", working_dir=str(BASE_DIR / "tests/e2e"))
    print()
    
    # Monitor test execution
    print("=" * 80)
    print("📊 Monitoring E2E Test Execution...")
    print("=" * 80)
    
    test_complete = False
    start_time = time.time()
    
    while time.time() - start_time < TEST_TIMEOUT:
        time.sleep(2)
        
        # Capture test output
        test_output = session.capture_output("e2e-test", lines=30)
        
        # Check for test completion indicators
        if "SCREENSHOT:" in test_output or "Test completed" in test_output or "✓" in test_output:
            print("   📝 Test is running...")
            
        # Check for errors
        if "error" in test_output.lower() and "error:" in test_output.lower():
            print("   ⚠️  Error detected in test output!")
            
        # Check if test process has completed (no new output for a while)
        # We'll check if the window is still active and running
        try:
            windows = session.list_windows()
            e2e_window = next((w for w in windows if w['name'] == 'e2e-test'), None)
            if e2e_window:
                # Capture latest output
                latest = session.capture_output("e2e-test", lines=100)
                if "Test completed successfully" in latest or "All tests passed" in latest:
                    test_complete = True
                    break
        except Exception as e:
            print(f"   Warning: Could not check window status: {e}")
    
    print()
    print("=" * 80)
    print("📋 Test Execution Summary")
    print("=" * 80)
    print()
    
    # Capture all window outputs
    print("📝 Backend Output (last 30 lines):")
    print("-" * 80)
    print(session.capture_output("backend", lines=30))
    print()
    
    print("📝 Frontend Output (last 30 lines):")
    print("-" * 80)
    print(session.capture_output("frontend", lines=30))
    print()
    
    print("📝 Browser Server Output (last 30 lines):")
    print("-" * 80)
    print(session.capture_output("browser", lines=30))
    print()
    
    print("📝 E2E Test Output (last 50 lines):")
    print("-" * 80)
    print(session.capture_output("e2e-test", lines=50))
    print()
    
    # Check for screenshots
    print("=" * 80)
    print("📸 Screenshots Generated")
    print("=" * 80)
    
    if SCREENSHOT_DIR.exists():
        screenshots = list(SCREENSHOT_DIR.glob("*.png"))
        if screenshots:
            print(f"✅ Found {len(screenshots)} screenshot(s):")
            for screenshot in sorted(screenshots):
                size = screenshot.stat().st_size
                print(f"   📷 {screenshot.name} ({size:,} bytes)")
        else:
            print("⚠️  No screenshots found in the screenshot directory")
            print(f"   Directory: {SCREENSHOT_DIR}")
    else:
        print("❌ Screenshot directory does not exist:")
        print(f"   {SCREENSHOT_DIR}")
    print()
    
    # Session info
    print("=" * 80)
    print("🔍 Session Information")
    print("=" * 80)
    print(f"Session name: choiros-e2e")
    print(f"Windows:")
    for window in session.list_windows():
        print(f"   - {window['name']} (ID: {window['id']})")
    print()
    print("Commands:")
    print(f"   Attach to session: tmux attach -t choiros-e2e")
    print(f"   List windows: tmux list-windows -t choiros-e2e")
    print(f"   Kill session: tmux kill-session -t choiros-e2e")
    print()
    
    print("=" * 80)
    print("✅ E2E Test Orchestration Complete!")
    print("=" * 80)
    
    # Optionally kill the session (uncomment to auto-cleanup)
    # print("\n🧹 Cleaning up session...")
    # session.kill()
    # print("✅ Session killed")
    
    return 0

if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("\n\n⚠️  Interrupted by user")
        print("Session 'choiros-e2e' is still running.")
        print("Attach with: tmux attach -t choiros-e2e")
        print("Kill with: tmux kill-session -t choiros-e2e")
        sys.exit(1)
