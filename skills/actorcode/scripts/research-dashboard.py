#!/usr/bin/env python3
"""
Research Dashboard - Tmux-based monitoring for actorcode research tasks.

Creates a tmux session with multiple panes showing:
- Active research tasks status
- Live findings feed
- Statistics and summary
"""

import subprocess
import sys
import time
from pathlib import Path

# Add multi-terminal to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "multi-terminal" / "scripts"))
from terminal_session import TerminalSession


def create_research_dashboard():
    """Create a tmux dashboard for monitoring research tasks."""
    
    session = TerminalSession("research-dashboard")
    
    # Window 1: Research status grid
    session.add_window("status", "just research-status --watch 2>/dev/null || watch -n 5 'just research-status'", split=False)
    
    # Window 2: Live findings feed
    session.add_window("findings", "just findings list --limit 50 2>/dev/null || echo 'Run: just findings list'", split=False)
    
    # Window 3: Statistics
    session.add_window("stats", "just findings stats 2>/dev/null || echo 'Run: just findings stats'", split=False)
    
    # Window 4: Control/commands
    session.add_window("control", "", split=False)
    session.send_keys("control", "# Research Dashboard Controls")
    session.send_keys("control", "Return")
    session.send_keys("control", "# Check status: just research-status")
    session.send_keys("control", "Return")
    session.send_keys("control", "# View findings: just findings list")
    session.send_keys("control", "Return")
    session.send_keys("control", "# Launch research: just research security-audit code-quality")
    session.send_keys("control", "Return")
    session.send_keys("control", "# Monitor sessions: just actorcode supervisor")
    session.send_keys("control", "Return")
    
    # Create a split view in window 1 (status + live feed)
    session._run_tmux("select-window", "-t", "research-dashboard:status")
    session._run_tmux("split-window", "-v", "-t", "research-dashboard:status")
    session._run_tmux("send-keys", "-t", "research-dashboard:status.1", 
                     "while true; do clear; just findings list --limit 10 2>/dev/null || echo 'No findings yet'; sleep 5; done", "C-m")
    
    # Resize splits
    session._run_tmux("resize-pane", "-t", "research-dashboard:status.0", "-y", "60%")
    
    print("✓ Research dashboard created: tmux attach -t research-dashboard")
    print("\nWindows:")
    print("  1. status    - Active research tasks + live findings")
    print("  2. findings  - Recent findings list")
    print("  3. stats     - Statistics and summary")
    print("  4. control   - Command reference")
    print("\nAttach with: tmux attach -t research-dashboard")
    
    return session


def create_compact_dashboard():
    """Create a compact single-window dashboard with splits."""
    
    session = TerminalSession("research-dash")
    
    # Main window with multiple panes
    # Top-left: Status
    session.add_window("main", "", split=False)
    
    # Create grid layout
    # Split horizontally first
    session._run_tmux("split-window", "-h", "-t", "research-dash:main")
    # Split vertically on left
    session._run_tmux("split-window", "-v", "-t", "research-dash:main.0")
    # Split vertically on right  
    session._run_tmux("split-window", "-v", "-t", "research-dash:main.2")
    
    # Pane 0 (top-left): Status
    session._run_tmux("send-keys", "-t", "research-dash:main.0",
                     "while true; do clear; echo '=== Research Status ==='; just research-status 2>/dev/null || echo 'No active sessions'; sleep 10; done", "C-m")
    
    # Pane 1 (bottom-left): Recent findings
    session._run_tmux("send-keys", "-t", "research-dash:main.1",
                     "while true; do clear; echo '=== Recent Findings ==='; just findings list --limit 15 2>/dev/null || echo 'No findings yet'; sleep 5; done", "C-m")
    
    # Pane 2 (top-right): Stats
    session._run_tmux("send-keys", "-t", "research-dash:main.2",
                     "while true; do clear; echo '=== Statistics ==='; just findings stats 2>/dev/null || echo 'No data yet'; sleep 15; done", "C-m")
    
    # Pane 3 (bottom-right): Commands
    session._run_tmux("send-keys", "-t", "research-dash:main.3",
                     "echo 'Research Dashboard Ready' && echo '' && echo 'Commands:' && echo '  just research-status' && echo '  just findings list' && echo '  just findings stats' && echo '  just research <template>' && echo '' && echo 'Templates: security-audit, code-quality, docs-gap, performance, bug-hunt' && bash", "C-m")
    
    print("✓ Compact research dashboard created: tmux attach -t research-dash")
    print("\nLayout: 2x2 grid")
    print("  Top-Left:    Research status (10s refresh)")
    print("  Bottom-Left: Recent findings (5s refresh)")
    print("  Top-Right:   Statistics (15s refresh)")
    print("  Bottom-Right: Interactive shell")
    
    return session


def kill_dashboard():
    """Kill the research dashboard session."""
    subprocess.run(["tmux", "kill-session", "-t", "research-dashboard"], capture_output=True)
    subprocess.run(["tmux", "kill-session", "-t", "research-dash"], capture_output=True)
    print("✓ Research dashboard killed")


def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="Research Dashboard for actorcode")
    parser.add_argument("command", choices=["create", "compact", "kill", "attach"], 
                       default="compact", nargs="?",
                       help="Command to run (default: compact)")
    parser.add_argument("--compact", action="store_true",
                       help="Use compact 2x2 grid layout")
    
    args = parser.parse_args()
    
    if args.command == "kill":
        kill_dashboard()
    elif args.command == "attach":
        session_name = "research-dash" if args.compact else "research-dashboard"
        subprocess.run(["tmux", "attach", "-t", session_name])
    elif args.command == "compact" or args.compact:
        create_compact_dashboard()
    else:
        create_research_dashboard()


if __name__ == "__main__":
    main()
