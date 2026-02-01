# Actorcode Usage

## Install

```bash
cd skills/actorcode
npm install
```

## Start the OpenCode server

```bash
opencode serve
```

## Environment

```bash
export OPENCODE_SERVER_URL=http://localhost:4096
export OPENCODE_SERVER_USERNAME=opencode
export OPENCODE_SERVER_PASSWORD=your_password
```

## Allowed models (fast to pricey)

Tier labels (fast to pricey):

1. pico: `zai-coding-plan/glm-4.7-flash`
2. nano: `zai-coding-plan/glm-4.7`
3. micro: `kimi-for-coding/k2p5`
4. milli: `openai/gpt-5.2-codex`

Capabilities:

- pico: Text-only. Run scripts/tools and quick research; not for writing new code.
- nano: Text-only. Coding-capable worker for straightforward changes.
- micro: Multimodal (text+image). General-purpose, resource-efficient default.
- milli: Multimodal (text+image). Long-context + debugging heavy lifting.

If `--model` is omitted, actorcode defaults to `pico`.

## Spawn a session

```bash
node skills/actorcode/scripts/actorcode.js spawn \
  --title "auth-research" \
  --agent explore \
  --tier pico \
  --prompt "Audit the auth flow and summarize risks."
```

By default, spawn wraps your prompt in a contract that includes situational
context (repo path, date, model/tier) plus expectations about reporting changes
and tests. Disable it with `--no-contract` if you need raw prompts.

## Check status

```bash
node skills/actorcode/scripts/actorcode.js status
```

## List models

```bash
node skills/actorcode/scripts/actorcode.js models
```

## Send a message

```bash
node skills/actorcode/scripts/actorcode.js message \
  --to <session_id> \
  --text "Focus on the session cookie path and expiry."
```

## Read messages (poll or wait)

```bash
# Latest assistant message
node skills/actorcode/scripts/actorcode.js messages \
  --id <session_id> \
  --role assistant \
  --latest

# Wait until a new assistant reply exists
node skills/actorcode/scripts/actorcode.js messages \
  --id <session_id> \
  --role assistant \
  --latest \
  --wait \
  --interval 1000

# Only return messages with text parts
node skills/actorcode/scripts/actorcode.js messages \
  --id <session_id> \
  --role assistant \
  --latest \
  --require-text
```

## Tail events

```bash
node skills/actorcode/scripts/actorcode.js events --session <session_id>
```

Notes:

- The events stream drives log updates and registry activity. Keep `events` running
  in a background window for long-running sessions.
- If `status` shows `unknown`, validate the server is running and keep `events` up.

## Tail logs

```bash
node skills/actorcode/scripts/actorcode.js logs --id <session_id>
```

## Supervisor loop

```bash
# Stream events and keep status registry fresh
node skills/actorcode/scripts/actorcode.js supervisor --interval 5000

# Print status snapshots alongside events
node skills/actorcode/scripts/actorcode.js supervisor --print-status

# Scope to one session
node skills/actorcode/scripts/actorcode.js supervisor --session <session_id>
```

## Registry recovery

If commands fail due to a corrupted registry, actorcode will move the file to
`.actorcode/registry.json.corrupt-<timestamp>` and rebuild a fresh registry.

## Debug dashboard (tmux)

Example using the multi-terminal skill to create a 4-pane log grid plus control
windows for events and commands:

```bash
python skills/multi-terminal/scripts/terminal_session.py <<'PY'
from skills.multi_terminal.scripts.terminal_session import TerminalSession

session = TerminalSession("actorcode-dashboard", "/Users/wiz/choiros-rs")

# Keep events streaming (drives log updates)
session.add_window("events", "just actorcode events")

# Control window for commands
session.add_window("control", "just actorcode status")

# Log grid (4 panes in one window)
session.add_window("pico", "just actorcode logs --id <pico_session_id>")
session.add_window("nano", "just actorcode logs --id <nano_session_id>", split=True)
session.add_window("micro", "just actorcode logs --id <micro_session_id>", split=True, split_direction="horizontal")
session.add_window("milli", "just actorcode logs --id <milli_session_id>", split=True)

print("Attach: tmux attach -t actorcode-dashboard")
PY
```

## Attach to server

```bash
node skills/actorcode/scripts/actorcode.js attach --
```

## Automated Research Tasks

Launch non-blocking research tasks that report findings incrementally:

```bash
# Launch security audit and code quality review
just research security-audit code-quality

# Available templates:
# - security-audit    : Security vulnerability scan
# - code-quality      : Code smells and refactoring opportunities  
# - docs-gap          : Missing documentation analysis
# - performance       : Performance bottleneck detection
# - bug-hunt          : Bug hunting across codebase

# Launch with background monitor
just research security-audit --monitor

# Check findings
just actorcode messages --id <session_id> --role assistant --latest --wait

# Monitor specific sessions
just research-monitor <session_id1> <session_id2>
```

Research agents report findings using `[LEARNING]` tags:
- `[LEARNING] SECURITY: Hardcoded API key in config.rs`
- `[LEARNING] BUG: Race condition in actor init`
- `[LEARNING] REFACTOR: Unused import in main.rs`
- `[LEARNING] DOCS: Missing README for sandbox module`
- `[LEARNING] PERFORMANCE: Inefficient query in events.rs`

The monitor collects these and prints a summary on exit.

## Research Status

Check the status of all research tasks:

```bash
# Show active research sessions
just research-status

# Show all sessions including completed
just research-status --all

# Show recent learnings for each session
just research-status --learnings
```

## Findings Database

Query the persisted findings database:

```bash
# List recent findings
just findings list

# Filter by session
just findings list --session <session_id>

# Filter by category
just findings list --category SECURITY

# Show statistics
just findings stats

# Export findings
just findings export --format json
just findings export --format csv
```

## Dashboards

### Tmux Dashboard

Create a live tmux dashboard with multiple panes:

```bash
# Compact 2x2 grid layout
just research-dashboard

# Full multi-window dashboard
just research-dashboard create

# Kill dashboard
just research-dashboard kill

# Attach to existing
just research-dashboard attach
```

### Web Dashboard

Open the web dashboard in your browser:

```bash
just research-web
```

The web dashboard shows:
- Active research sessions with real-time status
- Category distribution chart
- Recent findings with filtering
- Statistics and summary
