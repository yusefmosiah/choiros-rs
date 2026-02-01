#!/usr/bin/env python3
"""
Quick test script for multi-terminal management.

This creates a tmux session with your two services:
- dev-sandbox
- dev-ui

Then you can attach to monitor them.
"""

import sys
sys.path.insert(0, '/Users/wiz/choiros-rs/skills/multi-terminal/scripts')

from terminal_session import TerminalSession
import time

def main():
    print("Creating multi-terminal session for choiros-rs...")
    
    # Create session
    session = TerminalSession("choiros-dev", "/Users/wiz/choiros-rs")
    
    # Add your two services
    print("Starting dev-sandbox...")
    session.add_window("sandbox", "just dev-sandbox")
    time.sleep(2)  # Brief delay between starts
    
    print("Starting dev-ui...")
    session.add_window("ui", "just dev-ui")
    time.sleep(2)
    
    # Check initial output
    print("\n--- Sandbox output (first 20 lines) ---")
    print(session.capture_output("sandbox", lines=20))
    
    print("\n--- UI output (first 20 lines) ---")
    print(session.capture_output("ui", lines=20))
    
    print("\n✅ Both services started!")
    print("\nCommands you can run:")
    print(f"  Attach to see all windows: tmux attach -t choiros-dev")
    print(f"  List windows: tmux list-windows -t choiros-dev")
    print(f"  Switch windows: Ctrl+b + window number")
    print(f"  Detach: Ctrl+b d")
    print()
    print("To stop:")
    print(f"  python3 -c \"import sys; sys.path.insert(0, '/Users/wiz/choiros-rs/skills/multi-terminal/scripts'); from terminal_session import TerminalSession; TerminalSession('choiros-dev').kill()\"")
    
    # Optionally wait and monitor
    print("\nMonitoring for 10 seconds (Ctrl+C to stop)...")
    try:
        for i in range(10):
            time.sleep(1)
            # Quick check for errors
            sandbox_out = session.capture_output("sandbox", lines=5)
            ui_out = session.capture_output("ui", lines=5)
            
            if "error" in sandbox_out.lower() or "error" in ui_out.lower():
                print(f"⚠️  Potential error detected at second {i}!")
                
    except KeyboardInterrupt:
        print("\n\nStopping...")
    
    print("\nSession 'choiros-dev' is still running.")
    print("Attach anytime with: tmux attach -t choiros-dev")

if __name__ == "__main__":
    main()
