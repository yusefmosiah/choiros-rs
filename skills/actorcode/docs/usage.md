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
3. micro: `opencode/kimi-k2.5-free`
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
