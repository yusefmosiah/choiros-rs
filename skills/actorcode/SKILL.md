# Actorcode (OpenCode Orchestration)

Actorcode is a skill suite that orchestrates OpenCode sessions over HTTP using the OpenCode SDK. It provides a lightweight supervisor CLI for spawning agents, sending messages, watching events, and logging per-session activity.

## Highlights

- HTTP-first orchestration via `@opencode-ai/sdk`
- Per-session model selection
- Lightweight registry for supervisor context
- File-based observability logs

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

## Commands

- `actorcode spawn --title "research" --agent explore --tier pico --prompt "..."`
- `actorcode status`
- `actorcode models`
- `actorcode message --to <session_id> --text "..."`
- `actorcode messages --id <session_id> --require-text`
- `actorcode abort --id <session_id>`
- `actorcode events`
- `actorcode logs --id <session_id>`
- `actorcode supervisor --interval 5000 --print-status`
- `actorcode attach -- <opencode-attach-args>`
- `actorcode research-status [--all] [--learnings]`
- `actorcode findings <list|stats|export>`

## Research System

Launch non-blocking research tasks with incremental reporting:

```bash
# Launch research tasks
just research security-audit code-quality

# Check status
just research-status

# View findings
just findings list
just findings stats
```

## Dashboards

```bash
# Tmux dashboard (2x2 grid)
just research-dashboard

# Web dashboard (requires findings-server)
just findings-server  # Terminal 1: Start API server
just research-web     # Terminal 2: Open browser
```

## Environment

- `OPENCODE_SERVER_URL` (default `http://localhost:4096`)
- `OPENCODE_SERVER_USERNAME` (default `opencode`)
- `OPENCODE_SERVER_PASSWORD` (optional)

## Files

- Registry: `.actorcode/registry.json`
- Logs:
  - `logs/actorcode/supervisor.log`
  - `logs/actorcode/<session_id>.log`

For examples, see `skills/actorcode/docs/usage.md`.
