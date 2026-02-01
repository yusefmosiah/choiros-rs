# Multi-Terminal Session Management

Enable AI agents to control multiple concurrent terminal sessions using tmux, with full human visibility.

## Overview

This skill provides utilities for managing multiple terminal sessions from a single AI agent. Unlike multi-agent setups (running multiple AI instances), this focuses on **one agent orchestrating multiple processes** - dev servers, test runners, databases, log tailers, etc.

**Key Benefits:**
- Agent can automate multiple concurrent tasks
- Human can attach anytime to see state: `tmux attach -t mysession`
- Best of both worlds: automation + visibility
- No special protocols - uses standard tmux

## Installation

```bash
# macOS
brew install tmux

# Ubuntu/Debian
apt-get install tmux

# Python helper (optional but recommended)
pip install libtmux
```

## Quick Start

### Basic CLI Pattern

```bash
# Create session with multiple windows
tmux new-session -d -s myproject -n editor -c ~/myproject "vim"
tmux new-window -t myproject -n server "npm run dev"
tmux new-window -t myproject -n test "npm test -- --watch"
tmux new-window -t myproject -n logs "tail -f logs/app.log"

# Attach to see everything
tmux attach -t myproject

# From another terminal, agent can send commands
tmux send-keys -t myproject:server C-c  # Stop server
tmux send-keys -t myproject:server "npm run build" Enter

# Capture output programmatically
tmux capture-pane -t myproject:test -p | tail -20
```

### Python Helper (Recommended)

```python
from skills.multi_terminal import TerminalSession

# Create managed session
session = TerminalSession("myproject", "~/myproject")

# Add multiple windows
session.add_window("server", "npm run dev")
session.add_window("test", "npm test -- --watch", split=True)
session.add_window("db", "psql mydb")

# Send commands to specific windows
session.send_keys("server", "rs")  # Type 'rs' in server window
session.wait_for_output("test", "PASS", timeout=30)

# Get output from any window
logs = session.capture_output("test", lines=50)

# Clean shutdown
session.kill()
```

## Architecture Patterns

### Pattern 1: IDE Layout
```
┌─────────────────┬──────────────┐
│                 │   Server     │
│    Editor       │   (logs)     │
│    (vim)        ├──────────────┤
│                 │   Tests      │
│                 │   (watch)    │
└─────────────────┴──────────────┘
```

### Pattern 2: Horizontal Strip
```
┌──────────────────────────────┐
│  Editor                      │
├──────────────────────────────┤
│  Server logs                 │
├──────────────────────────────┤
│  Test output                 │
└──────────────────────────────┘
```

### Pattern 3: Dashboard Grid
```
┌──────────────┬──────────────┐
│  Frontend    │  Backend     │
│  dev server  │  API server  │
├──────────────┼──────────────┤
│  Database    │  Worker      │
│  console     │  queue       │
└──────────────┴──────────────┘
```

## Common Workflows

### Development Server + Hot Reload

```python
session = TerminalSession("dev", "~/myapp")

# Window 0: Editor
session.add_window("editor", "vim .")

# Window 1: Dev server with logs
session.add_window("server", "npm run dev")

# Window 2: Test in watch mode  
session.add_window("test", "npm test -- --watch")

# Monitor for errors
if session.wait_for_pattern("server", "ERROR|CRASH", timeout=60):
    error_context = session.capture_output("server", lines=30)
    # Analyze and fix...
```

### Database Migration + Verification

```python
session = TerminalSession("migration", "~/myapp")

# Terminal 1: Run migration
session.add_window("migrate", "alembic upgrade head")
session.wait_for_pattern("migrate", "done|complete|error")

# Terminal 2: Verify in database console
session.add_window("db", "psql mydb")
session.send_keys("db", "\\d users")
session.send_keys("db", "Enter")

# Capture verification output
schema = session.capture_output("db")
# Verify migration worked...
```

### Multi-Service Orchestration

```python
session = TerminalSession("microservices", "~/project")

# Start all services in parallel
services = [
    ("api", "cd api && npm run dev"),
    ("worker", "cd worker && python worker.py"),
    ("frontend", "cd frontend && npm start"),
    ("db", "docker-compose up postgres"),
]

for name, cmd in services:
    session.add_window(name, cmd)
    time.sleep(2)  # Stagger starts

# Monitor all for errors
errors = session.monitor_all(patterns=["ERROR", "FATAL", "CRASH"])
```

## Advanced Features

### Real-time Output Monitoring

```python
# Stream output from a window
for line in session.stream_output("server"):
    if "error" in line.lower():
        notify(f"Error in server: {line}")
```

### Synchronized Commands

```bash
# Send to all windows in session
tmux send-keys -t myproject: "git status" Enter

# Send to all panes in a window
tmux send-keys -t myproject:server "echo 'synced'" Enter
```

### Session Persistence

```bash
# Detach without killing
tmux detach -t myproject

# Reattach later (human or agent)
tmux attach -t myproject

# List all sessions
tmux list-sessions
```

### Window Layout Management

```python
# Create complex layouts
session.create_layout("ide", """
    main-vertical:
        - editor (60%)
        - server (20%)
        - test (20%)
""")
```

## Best Practices

### 1. Always Name Sessions
```bash
# Good: Named session
tmux new-session -d -s myproject

# Bad: Anonymous session
tmux new-session -d  # Hard to reference later
```

### 2. Use Window Names
```bash
tmux new-window -t myproject -n "server"
# Reference as: myproject:server instead of myproject:1
```

### 3. Capture Output Before Sending Keys
```python
# Capture current state first
before = session.capture_output("window")
session.send_keys("window", "command")
session.wait_for_change("window")
after = session.capture_output("window")
```

### 4. Handle Long-Running Processes
```python
# Use --watch, --follow, or persistent modes
session.add_window("logs", "tail -f app.log")  # Good
# Not: session.add_window("logs", "cat app.log")  # Ends immediately
```

### 5. Clean Shutdown
```python
# Graceful shutdown sequence
session.send_keys("server", "C-c")  # SIGINT
session.wait_for_pattern("server", "shutting down|exited")
session.kill_window("server")
```

## Troubleshooting

### Session Already Exists
```bash
# Kill existing and recreate
tmux kill-session -t myproject 2>/dev/null; tmux new-session -d -s myproject
```

### Pane Not Found
```bash
# List all panes with IDs
tmux list-panes -t myproject:window -F "#{pane_id} #{pane_current_command}"
```

### Output Capture Empty
```bash
# Pane might have scrolled, increase scrollback
tmux capture-pane -t myproject:window -S -1000 -E -
```

## Integration with Handoff System

When creating long-running sessions, include in handoff:

```markdown
## Active Terminal Sessions

- **myproject** (tmux): Dev environment with 4 windows
  - editor: vim . (attached)
  - server: npm run dev (PID 12345)
  - test: npm test --watch (waiting)
  - logs: tail -f app.log
  
Attach: `tmux attach -t myproject`
```

## Further Reading

- [tmux documentation](https://github.com/tmux/tmux/wiki)
- [libtmux Python API](https://libtmux.readthedocs.io/)
- [tmuxinator](https://github.com/tmuxinator/tmuxinator) - Session templates
- [tmuxp](https://github.com/tmux-python/tmuxp) - YAML session configs
