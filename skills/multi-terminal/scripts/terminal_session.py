#!/usr/bin/env python3
"""
Multi-terminal session management for AI agents using tmux.

This module provides a high-level interface for managing multiple tmux windows/panes
from a single AI agent, with full support for programmatic control and human visibility.
"""

import subprocess
import time
import re
from typing import Optional, List, Dict, Generator
from dataclasses import dataclass
from pathlib import Path


@dataclass
class WindowConfig:
    """Configuration for a tmux window."""
    name: str
    command: str
    split: bool = False
    split_direction: str = "vertical"  # "vertical" or "horizontal"
    working_dir: Optional[str] = None


class TerminalSession:
    """
    Manages a tmux session with multiple windows for concurrent process orchestration.
    
    Example:
        session = TerminalSession("myproject", "~/myproject")
        session.add_window("server", "npm run dev")
        session.add_window("test", "npm test", split=True)
        session.wait_for_pattern("server", "ready")
        output = session.capture_output("test")
        session.kill()
    """
    
    def __init__(self, name: str, working_dir: str = "."):
        self.name = name
        self.working_dir = Path(working_dir).expanduser().resolve()
        self.windows: Dict[str, str] = {}  # name -> window_id
        self._ensure_session()
    
    def _run_tmux(self, *args, capture=True) -> subprocess.CompletedProcess:
        """Execute a tmux command."""
        cmd = ["tmux"] + list(args)
        if capture:
            return subprocess.run(cmd, capture_output=True, text=True)
        else:
            return subprocess.run(cmd)
    
    def _ensure_session(self):
        """Create tmux session if it doesn't exist."""
        # Check if session exists
        result = self._run_tmux("has-session", "-t", self.name)
        if result.returncode != 0:
            # Create new detached session
            self._run_tmux(
                "new-session", "-d", "-s", self.name,
                "-c", str(self.working_dir)
            )
    
    def add_window(self, name: str, command: str, 
                   split: bool = False, 
                   split_direction: str = "vertical",
                   working_dir: Optional[str] = None) -> str:
        """
        Add a new window to the session.
        
        Args:
            name: Window name (must be unique)
            command: Command to run in the window
            split: If True, split the current window instead of creating new
            split_direction: "vertical" or "horizontal" split
            working_dir: Working directory for the command
            
        Returns:
            Window identifier string
        """
        if name in self.windows:
            raise ValueError(f"Window '{name}' already exists")
        
        target_dir = Path(working_dir).expanduser() if working_dir else self.working_dir
        
        if split and self.windows:
            # Split the most recently added window
            last_window = list(self.windows.values())[-1]
            split_flag = "-v" if split_direction == "vertical" else "-h"
            self._run_tmux("split-window", split_flag, "-t", last_window, "-c", str(target_dir))
            # Get the new pane ID
            result = self._run_tmux("list-panes", "-t", self.name, "-F", "#{pane_id}")
            pane_id = result.stdout.strip().split("\n")[-1]
            window_id = f"{self.name}:{pane_id}"
        else:
            # Create new window
            self._run_tmux("new-window", "-t", self.name, "-n", name, "-c", str(target_dir))
            window_id = f"{self.name}:{name}"
        
        self.windows[name] = window_id
        
        # Send the command
        if command:
            self.send_keys(name, command)
            self.send_keys(name, "Enter")
        
        return window_id
    
    def send_keys(self, window_name: str, keys: str, literal: bool = False):
        """
        Send keystrokes to a window.
        
        Args:
            window_name: Name of the target window
            keys: Keys to send. Special keys: C-c (Ctrl+C), C-d (Ctrl+D), Enter, etc.
            literal: If True, send literally without interpreting special keys
        """
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        window_id = self.windows[window_name]
        
        # Convert special key names to tmux format
        key_map = {
            "Enter": "C-m",
            "Return": "C-m",
            "Tab": "Tab",
            "Space": "Space",
            "C-c": "C-c",
            "C-d": "C-d",
            "C-z": "C-z",
            "C-l": "C-l",
            "Escape": "Escape",
        }
        
        if keys in key_map:
            keys = key_map[keys]
        
        if literal:
            # Send literal text character by character
            for char in keys:
                if char == " ":
                    self._run_tmux("send-keys", "-t", window_id, "Space")
                elif char == "\n":
                    self._run_tmux("send-keys", "-t", window_id, "Enter")
                else:
                    self._run_tmux("send-keys", "-t", window_id, char)
        else:
            self._run_tmux("send-keys", "-t", window_id, keys)
    
    def capture_output(self, window_name: str, lines: int = 100) -> str:
        """
        Capture output from a window.
        
        Args:
            window_name: Name of the window to capture
            lines: Number of lines to capture (from end)
            
        Returns:
            Captured text output
        """
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        window_id = self.windows[window_name]
        
        # Capture pane with scrollback
        result = self._run_tmux("capture-pane", "-t", window_id, "-S", f"-{lines}", "-p")
        return result.stdout
    
    def wait_for_pattern(self, window_name: str, pattern: str, 
                        timeout: int = 30, interval: float = 0.5) -> bool:
        """
        Wait for a pattern to appear in window output.
        
        Args:
            window_name: Name of the window to monitor
            pattern: Regex pattern to search for
            timeout: Maximum time to wait in seconds
            interval: Polling interval in seconds
            
        Returns:
            True if pattern found, False if timeout
        """
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        start_time = time.time()
        regex = re.compile(pattern, re.IGNORECASE)
        
        while time.time() - start_time < timeout:
            output = self.capture_output(window_name)
            if regex.search(output):
                return True
            time.sleep(interval)
        
        return False
    
    def wait_for_change(self, window_name: str, 
                       timeout: int = 30, interval: float = 0.5) -> bool:
        """
        Wait for window output to change.
        
        Args:
            window_name: Name of the window to monitor
            timeout: Maximum time to wait in seconds
            interval: Polling interval in seconds
            
        Returns:
            True if output changed, False if timeout
        """
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        initial = self.capture_output(window_name)
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            time.sleep(interval)
            current = self.capture_output(window_name)
            if current != initial:
                return True
        
        return False
    
    def stream_output(self, window_name: str, 
                     interval: float = 0.5) -> Generator[str, None, None]:
        """
        Stream output from a window in real-time.
        
        Args:
            window_name: Name of the window to stream
            interval: Polling interval in seconds
            
        Yields:
            New lines of output
        """
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        last_output = ""
        
        while True:
            current = self.capture_output(window_name, lines=1000)
            
            # Find new content
            if current != last_output:
                # Simple diff - yield new lines
                last_lines = last_output.split("\n") if last_output else []
                current_lines = current.split("\n")
                
                # Yield lines that are new
                for line in current_lines[len(last_lines):]:
                    if line:
                        yield line
                
                last_output = current
            
            time.sleep(interval)
    
    def kill_window(self, window_name: str):
        """Kill a specific window."""
        if window_name not in self.windows:
            raise ValueError(f"Window '{window_name}' not found")
        
        window_id = self.windows[window_name]
        self._run_tmux("kill-window", "-t", window_id)
        del self.windows[window_name]
    
    def kill(self):
        """Kill the entire session and all windows."""
        self._run_tmux("kill-session", "-t", self.name)
        self.windows.clear()
    
    def list_windows(self) -> List[Dict[str, str]]:
        """List all windows in the session."""
        result = self._run_tmux(
            "list-windows", "-t", self.name, 
            "-F", "#{window_name}:#{window_id}:#{window_active}"
        )
        
        windows = []
        for line in result.stdout.strip().split("\n"):
            if line:
                parts = line.split(":")
                if len(parts) >= 2:
                    windows.append({
                        "name": parts[0],
                        "id": parts[1],
                        "active": len(parts) > 2 and parts[2] == "1"
                    })
        
        return windows
    
    def attach(self):
        """Attach human to the session (blocks until detached)."""
        self._run_tmux("attach", "-t", self.name, capture=False)
    
    def detach(self):
        """Detach from the session."""
        self._run_tmux("detach", "-s", self.name)
    
    def snapshot(self) -> Dict[str, str]:
        """
        Get a snapshot of all windows and their current output.
        
        Returns:
            Dictionary mapping window names to their output
        """
        snapshot = {}
        for name in self.windows:
            snapshot[name] = self.capture_output(name)
        return snapshot


class MultiServiceOrchestrator:
    """
    Orchestrate multiple long-running services in a single tmux session.
    
    Perfect for microservices, full-stack apps, or any multi-process workflow.
    """
    
    def __init__(self, session_name: str, working_dir: str = "."):
        self.session = TerminalSession(session_name, working_dir)
        self.services: Dict[str, Dict] = {}
    
    def add_service(self, name: str, command: str, 
                   working_dir: Optional[str] = None,
                   ready_pattern: Optional[str] = None):
        """
        Add a service to orchestrate.
        
        Args:
            name: Service name
            command: Command to run
            working_dir: Optional override for working directory
            ready_pattern: Pattern to wait for indicating service is ready
        """
        window_id = self.session.add_window(name, command, working_dir=working_dir)
        self.services[name] = {
            "command": command,
            "window_id": window_id,
            "ready_pattern": ready_pattern,
            "ready": False
        }
    
    def start_all(self, stagger: float = 1.0):
        """
        Start all services with optional stagger delay.
        
        Args:
            stagger: Seconds to wait between starting services
        """
        for name in self.services:
            # Service already started by add_service, just wait
            time.sleep(stagger)
    
    def wait_for_ready(self, timeout_per_service: int = 30) -> List[str]:
        """
        Wait for all services with ready patterns to signal ready.
        
        Returns:
            List of service names that became ready
        """
        ready = []
        
        for name, config in self.services.items():
            if config["ready_pattern"]:
                if self.session.wait_for_pattern(name, config["ready_pattern"], 
                                                timeout=timeout_per_service):
                    config["ready"] = True
                    ready.append(name)
        
        return ready
    
    def monitor_logs(self, patterns: Optional[List[str]] = None) -> Dict[str, List[str]]:
        """
        Monitor all services for error patterns.
        
        Args:
            patterns: List of regex patterns to search for (default: ["ERROR", "FATAL"])
            
        Returns:
            Dictionary mapping service names to list of matched lines
        """
        if patterns is None:
            patterns = ["ERROR", "FATAL", "CRASH", "Exception"]
        
        matches = {}
        
        for name in self.services:
            output = self.session.capture_output(name, lines=100)
            matches[name] = []
            
            for pattern in patterns:
                for line in output.split("\n"):
                    if re.search(pattern, line, re.IGNORECASE):
                        matches[name].append(line)
        
        return matches
    
    def shutdown_all(self, graceful: bool = True):
        """
        Shutdown all services.
        
        Args:
            graceful: If True, send Ctrl+C first, then kill after delay
        """
        if graceful:
            for name in self.services:
                self.session.send_keys(name, "C-c")
            
            time.sleep(2)  # Give time for graceful shutdown
        
        self.session.kill()


# Convenience functions for quick usage

def quick_session(name: str, commands: Dict[str, str], working_dir: str = ".") -> TerminalSession:
    """
    Quickly create a session with multiple windows.
    
    Args:
        name: Session name
        commands: Dictionary of window_name -> command
        working_dir: Base working directory
        
    Returns:
        Configured TerminalSession
    """
    session = TerminalSession(name, working_dir)
    
    for window_name, command in commands.items():
        session.add_window(window_name, command)
    
    return session


def dev_environment(project_dir: str, 
                   editor_cmd: str = "vim .",
                   dev_server_cmd: str = "npm run dev",
                   test_cmd: str = "npm test -- --watch") -> TerminalSession:
    """
    Create a standard development environment.
    
    Args:
        project_dir: Project root directory
        editor_cmd: Command for editor window
        dev_server_cmd: Command for dev server window
        test_cmd: Command for test window
        
    Returns:
        TerminalSession with editor, server, and test windows
    """
    session = TerminalSession("dev", project_dir)
    
    session.add_window("editor", editor_cmd)
    session.add_window("server", dev_server_cmd)
    session.add_window("test", test_cmd)
    
    return session


if __name__ == "__main__":
    # Example usage demonstration
    print("Multi-Terminal Session Manager")
    print("==============================")
    print()
    print("Example: Create development environment")
    print()
    print("```python")
    print("from skills.multi_terminal.terminal_session import dev_environment")
    print()
    print("session = dev_environment(")
    print("    project_dir='~/myproject',")
    print("    dev_server_cmd='npm run dev',")
    print("    test_cmd='npm test -- --watch'")
    print(")")
    print()
    print("# Wait for server to be ready")
    print("if session.wait_for_pattern('server', 'Server ready|Listening'):")
    print("    print('Server is up!')")
    print()
    print("# Check test output")
    print("output = session.capture_output('test', lines=50)")
    print("if 'FAIL' in output:")
    print("    print('Tests failing!')")
    print()
    print("# Attach to see everything")
    print("session.attach()")
    print("```")
